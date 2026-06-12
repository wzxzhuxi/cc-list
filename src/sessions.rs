//! 数据层: 扫描 ~/.claude/projects/*/*.jsonl, 提取会话元数据与对话内容

use anyhow::{Context, Result};
use rayon::prelude::*;
use serde_json::Value;
use std::collections::VecDeque;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub struct Session {
    pub epoch: u64,
    pub date: String,
    pub sid: String,
    pub file: PathBuf,
    /// 会话记录的项目目录 (绝对路径)
    pub cwd: String,
    /// 显示用 (~ 缩写)
    pub cwd_display: String,
    /// 首条真实用户消息
    pub summary: String,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Role {
    User,
    Assistant,
}

pub struct Msg {
    pub role: Role,
    pub text: String,
}

fn projects_dir() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".claude"))
        .join("projects")
}

/// 扫描全部会话, 按修改时间倒序
pub fn scan() -> Result<Vec<Session>> {
    let root = projects_dir();
    let mut files = Vec::new();
    let entries =
        fs::read_dir(&root).with_context(|| format!("directory not found: {}", root.display()))?;
    for proj in entries.flatten() {
        let proj = proj.path();
        if !proj.is_dir() {
            continue;
        }
        for f in fs::read_dir(&proj).into_iter().flatten().flatten() {
            let f = f.path();
            let name = f.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if f.extension().is_some_and(|e| e == "jsonl") && !name.starts_with("agent-") {
                files.push(f);
            }
        }
    }
    let home = dirs::home_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let mut sessions: Vec<Session> = files
        .par_iter()
        .filter_map(|f| parse_session(f, &home))
        .collect();
    sessions.sort_by_key(|s| std::cmp::Reverse(s.epoch));
    Ok(sessions)
}

fn parse_session(file: &Path, home: &str) -> Option<Session> {
    let sid = file.file_stem()?.to_str()?.to_string();
    let epoch = fs::metadata(file)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs();
    let date = chrono::DateTime::from_timestamp(epoch as i64, 0)?
        .with_timezone(&chrono::Local)
        .format("%m-%d %H:%M")
        .to_string();
    let (cwd, summary) = parse_head(file);
    let cwd_display = if !home.is_empty() && cwd.starts_with(home) {
        cwd.replacen(home, "~", 1)
    } else {
        cwd.clone()
    };
    Some(Session {
        epoch,
        date,
        sid,
        file: file.to_path_buf(),
        cwd,
        cwd_display,
        summary,
    })
}

/// 只读文件头部: 找到 cwd 和首条真实用户消息即停
fn parse_head(file: &Path) -> (String, String) {
    let mut cwd: Option<String> = None;
    let mut summary: Option<String> = None;
    if let Ok(f) = fs::File::open(file) {
        for line in BufReader::new(f).lines().take(300).map_while(Result::ok) {
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if cwd.is_none() {
                if let Some(c) = v.get("cwd").and_then(Value::as_str) {
                    cwd = Some(c.to_string());
                }
            }
            if summary.is_none() {
                if let Some(t) = user_text(&v) {
                    summary = Some(oneline(&t, 120));
                }
            }
            if cwd.is_some() && summary.is_some() {
                break;
            }
        }
    }
    (
        cwd.unwrap_or_else(|| "?".into()),
        summary.unwrap_or_else(|| "(no user message)".into()),
    )
}

/// 读取整个会话, 保留末尾 `keep` 条可展示消息 (用于预览)
pub fn load_messages(file: &Path, keep: usize) -> Vec<Msg> {
    let Ok(f) = fs::File::open(file) else {
        return Vec::new();
    };
    let mut msgs: VecDeque<Msg> = VecDeque::with_capacity(keep + 1);
    for line in BufReader::new(f).lines().map_while(Result::ok) {
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let role = match v.get("type").and_then(Value::as_str) {
            Some("user") => Role::User,
            Some("assistant") => Role::Assistant,
            _ => continue,
        };
        if v.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let Some(text) = v
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(text_content)
        else {
            continue;
        };
        if !displayable(&text) {
            continue;
        }
        msgs.push_back(Msg {
            role,
            text: truncate_chars(&text, 600),
        });
        if msgs.len() > keep {
            msgs.pop_front();
        }
    }
    msgs.into()
}

/// 提取一行记录中的真实用户消息 (跳过 sidechain / 命令注入 / Caveat)
fn user_text(v: &Value) -> Option<String> {
    if v.get("type").and_then(Value::as_str) != Some("user") {
        return None;
    }
    if v.get("isSidechain").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    v.get("message")
        .and_then(|m| m.get("content"))
        .and_then(text_content)
        .filter(|t| displayable(t))
}

/// message.content 可能是字符串或 content block 数组
fn text_content(c: &Value) -> Option<String> {
    match c {
        Value::String(s) => Some(s.clone()),
        Value::Array(arr) => {
            let parts: Vec<&str> = arr
                .iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .collect();
            (!parts.is_empty()).then(|| parts.join("\n"))
        }
        _ => None,
    }
}

fn displayable(t: &str) -> bool {
    let t = t.trim_start();
    !t.is_empty() && !t.starts_with('<') && !t.starts_with("Caveat:")
}

fn oneline(t: &str, max: usize) -> String {
    let s: String = t
        .chars()
        .map(|c| {
            if matches!(c, '\t' | '\n' | '\r') {
                ' '
            } else {
                c
            }
        })
        .collect();
    truncate_chars(&s, max)
}

pub fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}…")
    }
}
