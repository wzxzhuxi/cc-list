//! 终端适配层: 探测当前终端环境, 用对应方式打开会话
//!
//! 探测优先级 (环境变量优先于命令存在性):
//! tmux → zellij → kitty(远程控制) → wezterm → Windows Terminal → 通用 GUI 终端 → macOS Terminal.app

use anyhow::{anyhow, bail, Result};
use clap::ValueEnum;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Copy, ValueEnum)]
pub enum Mode {
    /// New tab
    Tab,
    /// New window
    Window,
    /// On top of the current window (popup/floating/overlay)
    Overlay,
    /// Replace the current terminal (exec)
    Here,
}

/// 打开会话, 成功时返回描述实际发生了什么的消息 (Here 模式永不返回)
pub fn open(mode: Mode, dir: &str, sid: &str) -> Result<String> {
    let dir = expand_tilde(dir);
    if !dir.is_dir() {
        bail!("project directory no longer exists: {}", dir.display());
    }
    match mode {
        Mode::Here => here(&dir, sid),
        Mode::Tab => tab(&dir, sid),
        Mode::Window => window(&dir, sid),
        Mode::Overlay => overlay(&dir, sid),
    }
}

fn expand_tilde(d: &str) -> PathBuf {
    if d == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(d));
    }
    if let Some(rest) = d.strip_prefix("~/") {
        if let Some(h) = dirs::home_dir() {
            return h.join(rest);
        }
    }
    PathBuf::from(d)
}

/// 覆盖当前终端: Unix 上 exec 替换进程映像; Windows 无 exec, 退化为等待子进程
fn here(dir: &Path, sid: &str) -> Result<String> {
    let mut cmd = Command::new("claude");
    cmd.args(["--resume", sid]).current_dir(dir);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // exec 成功则永不返回
        Err(anyhow!(cmd.exec()).context("failed to launch claude (is it in PATH?)"))
    }
    #[cfg(not(unix))]
    {
        let status = cmd.status()?;
        std::process::exit(status.code().unwrap_or(1));
    }
}

fn envset(k: &str) -> bool {
    std::env::var_os(k).is_some_and(|v| !v.is_empty())
}

fn has(cmd: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|p| {
        #[cfg(windows)]
        {
            p.join(format!("{cmd}.exe")).is_file() || p.join(cmd).is_file()
        }
        #[cfg(not(windows))]
        {
            p.join(cmd).is_file()
        }
    })
}

fn in_tmux() -> bool {
    envset("TMUX") && has("tmux")
}
fn in_zellij() -> bool {
    envset("ZELLIJ") && has("zellij")
}
fn in_wezterm() -> bool {
    envset("WEZTERM_PANE") && has("wezterm")
}
fn kitty_rc() -> bool {
    envset("KITTY_WINDOW_ID")
        && has("kitty")
        && Command::new("kitty")
            .args(["@", "ls"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
}

/// 同步执行并捕获输出 (TUI 仍在运行, 子进程的 stdout/stderr 不能直写屏幕)
fn run(prog: &str, args: &[&str]) -> Result<()> {
    let out = Command::new(prog)
        .args(args)
        .stdin(Stdio::null())
        .output()?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("{prog} failed: {}", err.trim());
    }
    Ok(())
}

fn spawn_detached(prog: &str, args: &[&str]) -> Result<()> {
    Command::new(prog)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

fn tab(dir: &Path, sid: &str) -> Result<String> {
    let d = dir.to_string_lossy();
    if in_tmux() {
        run("tmux", &["new-window", "-c", &d, "claude", "--resume", sid])?;
        Ok("opened tmux window (tab)".into())
    } else if in_zellij() {
        run(
            "zellij",
            &[
                "run", "--name", "claude", "--cwd", &d, "--", "claude", "--resume", sid,
            ],
        )?;
        Ok("opened zellij pane".into())
    } else if kitty_rc() {
        let cwd = format!("--cwd={d}");
        run(
            "kitty",
            &[
                "@",
                "launch",
                "--type=tab",
                &cwd,
                "--",
                "claude",
                "--resume",
                sid,
            ],
        )?;
        Ok("opened kitty tab".into())
    } else if in_wezterm() {
        run(
            "wezterm",
            &["cli", "spawn", "--cwd", &d, "--", "claude", "--resume", sid],
        )?;
        Ok("opened wezterm tab".into())
    } else if cfg!(windows) && has("wt") {
        // -w 0: 复用当前窗口; nt: 新 tab
        run(
            "wt",
            &["-w", "0", "nt", "-d", &d, "claude", "--resume", sid],
        )?;
        Ok("opened new tab".into())
    } else {
        let via = spawn_terminal_window(dir, sid)?;
        Ok(format!(
            "this terminal can't open tabs programmatically (kitty needs allow_remote_control yes + restart); opened a new window instead ({via})"
        ))
    }
}

fn window(dir: &Path, sid: &str) -> Result<String> {
    let d = dir.to_string_lossy();
    if kitty_rc() {
        let cwd = format!("--cwd={d}");
        run(
            "kitty",
            &[
                "@",
                "launch",
                "--type=os-window",
                &cwd,
                "--",
                "claude",
                "--resume",
                sid,
            ],
        )?;
        Ok("opened kitty OS window".into())
    } else if in_wezterm() {
        run(
            "wezterm",
            &[
                "cli",
                "spawn",
                "--new-window",
                "--cwd",
                &d,
                "--",
                "claude",
                "--resume",
                sid,
            ],
        )?;
        Ok("opened wezterm window".into())
    } else if in_tmux() && !envset("DISPLAY") && !envset("WAYLAND_DISPLAY") {
        // 无图形环境 (SSH/tty): 退化为 tmux 新 window
        run("tmux", &["new-window", "-c", &d, "claude", "--resume", sid])?;
        Ok("no GUI environment; opened tmux window".into())
    } else if cfg!(windows) && has("wt") {
        run("wt", &["-d", &d, "claude", "--resume", sid])?;
        Ok("opened new window".into())
    } else {
        let via = spawn_terminal_window(dir, sid)?;
        Ok(format!("opened new terminal window ({via})"))
    }
}

fn overlay(dir: &Path, sid: &str) -> Result<String> {
    let d = dir.to_string_lossy();
    if in_tmux() {
        run(
            "tmux",
            &[
                "display-popup",
                "-d",
                &d,
                "-w",
                "85%",
                "-h",
                "85%",
                "-E",
                "claude",
                "--resume",
                sid,
            ],
        )?;
        Ok("tmux popup closed".into())
    } else if in_zellij() {
        run(
            "zellij",
            &[
                "run",
                "--floating",
                "--name",
                "claude",
                "--cwd",
                &d,
                "--",
                "claude",
                "--resume",
                sid,
            ],
        )?;
        Ok("opened zellij floating pane".into())
    } else if kitty_rc() {
        let cwd = format!("--cwd={d}");
        run(
            "kitty",
            &[
                "@",
                "launch",
                "--type=overlay",
                &cwd,
                "--",
                "claude",
                "--resume",
                sid,
            ],
        )?;
        Ok("opened kitty overlay (returns here when closed)".into())
    } else if in_wezterm() {
        run(
            "wezterm",
            &[
                "cli",
                "split-pane",
                "--cwd",
                &d,
                "--",
                "claude",
                "--resume",
                sid,
            ],
        )?;
        Ok("opened wezterm split pane".into())
    } else {
        bail!("this terminal doesn't support overlay (kitty needs allow_remote_control yes + restart); use tab/window")
    }
}

/// 通用回退: 拉起一个新的 GUI 终端窗口, 返回所用终端名
fn spawn_terminal_window(dir: &Path, sid: &str) -> Result<String> {
    let d = dir.to_string_lossy().into_owned();
    let term_env = std::env::var("TERMINAL").unwrap_or_default();
    let mut cands: Vec<&str> = Vec::new();
    if !term_env.is_empty() {
        cands.push(&term_env);
    }
    cands.extend([
        "kitty",
        "wezterm",
        "alacritty",
        "foot",
        "ghostty",
        "konsole",
        "gnome-terminal",
        "xterm",
    ]);
    for t in cands {
        if !has(t) {
            continue;
        }
        let base = Path::new(t)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(t);
        let wd = format!("--working-directory={d}");
        match base {
            "kitty" => spawn_detached(
                "kitty",
                &["--detach", "--directory", &d, "claude", "--resume", sid],
            )?,
            "wezterm" => spawn_detached(
                "wezterm",
                &["start", "--cwd", &d, "--", "claude", "--resume", sid],
            )?,
            "alacritty" => spawn_detached(
                "alacritty",
                &["--working-directory", &d, "-e", "claude", "--resume", sid],
            )?,
            "foot" => spawn_detached("foot", &[wd.as_str(), "claude", "--resume", sid])?,
            "ghostty" => {
                spawn_detached("ghostty", &[wd.as_str(), "-e", "claude", "--resume", sid])?
            }
            "konsole" => spawn_detached(
                "konsole",
                &["--workdir", &d, "-e", "claude", "--resume", sid],
            )?,
            "gnome-terminal" => spawn_detached(
                "gnome-terminal",
                &[wd.as_str(), "--", "claude", "--resume", sid],
            )?,
            _ => {
                let sh = format!("cd '{}' && claude --resume {sid}", d.replace('\'', r"'\''"));
                spawn_detached(t, &["-e", "bash", "-c", &sh])?;
            }
        }
        return Ok(base.to_string());
    }
    #[cfg(target_os = "macos")]
    {
        let esc = d.replace('"', "\\\"");
        let script = format!(
            "tell application \"Terminal\" to do script \"cd '{esc}' && claude --resume {sid}\""
        );
        run("osascript", &["-e", &script])?;
        return Ok("Terminal.app".to_string());
    }
    #[allow(unreachable_code)]
    {
        bail!("no usable terminal emulator found (set $TERMINAL to specify one)")
    }
}
