# cc-list

[中文](README.md)

A TUI to browse Claude Code sessions across all projects. Pick one, then:

- Resume **in the current terminal** (enter)
- Resume in a **new tab** (ctrl-t)
- Resume in a **new window** (ctrl-n)
- Resume in an **overlay/popup** (ctrl-o)

The open actions probe the environment at runtime and adapt to tmux / zellij / kitty / wezterm / Windows Terminal / any GUI terminal / macOS Terminal.app. Before resuming, cc-list automatically cd's into the cwd recorded in the session — no manual directory hopping across projects.

Single binary (ratatui). The only runtime dependency is the claude CLI. Works on Linux/macOS/Windows.

## Install

### One-liner (Linux/macOS)

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/wzxzhuxi/cc-list/releases/latest/download/cc-list-installer.sh | sh
```

Windows (PowerShell):

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/wzxzhuxi/cc-list/releases/latest/download/cc-list-installer.ps1 | iex"
```

Installs into `~/.cargo/bin` (no Rust toolchain required).

### From source

```bash
git clone https://github.com/wzxzhuxi/cc-list.git
cd cc-list && cargo build --release
ln -s "$PWD/target/release/cc-list" ~/.local/bin/cc-list
```

Or grab a prebuilt binary for your platform from [Releases](https://github.com/wzxzhuxi/cc-list/releases) and put it in PATH.

### Claude Code plugin (optional)

Provides the in-session `/cc-list:resume` slash command (requires the cc-list binary in PATH):

```
/plugin marketplace add wzxzhuxi/cc-list
/plugin install cc-list@cc-list-marketplace
```

### kitty users (optional)

Enable remote control in `~/.config/kitty/kitty.conf` (required for tab/overlay modes), and optionally bind a global hotkey to summon cc-list anywhere:

```
allow_remote_control yes
map ctrl+shift+r launch --type=overlay cc-list
```

Fully restart kitty afterwards. tmux/wezterm/zellij users need no configuration.

## Usage

```bash
cc-list              # browse all sessions in the TUI
cc-list breeze       # with an initial search query
cc-list list 30      # TSV output of the 30 most recent sessions (for scripts/plugins)
cc-list open tab ~/proj <session-id>   # open directly; modes: tab|window|overlay|here
```

Layout: session list on top (date / project dir / first message), fuzzy search in the middle, a preview of the tail of the conversation below. Type to filter; `↑↓` to select, `pgup/pgdn` to scroll the preview; `enter`/`^t`/`^n`/`^o` to open in the four ways above.

Inside a Claude Code session: `/cc-list:resume [query]`.

## Backend mapping

| Environment | tab | window | overlay |
|------|-----|--------|---------|
| tmux | `new-window` | new GUI window (falls back to tab without a display) | `display-popup` (tmux ≥ 3.2) |
| zellij | `run` (new pane) | new GUI window | `run --floating` |
| kitty + `allow_remote_control` | `@ launch --type=tab` | `--type=os-window` | `--type=overlay` |
| wezterm | `cli spawn` | `cli spawn --new-window` | `cli split-pane` |
| Windows Terminal | `wt nt` | `wt` | unsupported |
| other GUI terminals | falls back to new window | `$TERMINAL` or probes kitty/alacritty/foot/ghostty/konsole/gnome-terminal etc. | unsupported, errors with a hint |
| macOS, nothing matched | — | Terminal.app (osascript) | — |

## Notes

- "Resume in the current terminal" replaces the process via the `exec` syscall on Unix; on Windows it degrades to spawn + wait. Only available from the shell entry point — the Claude Code process cannot replace the terminal hosting itself; the closest in-session equivalent is overlay.
- Session data is read from `~/.claude/projects/*/*.jsonl`, honoring `CLAUDE_CONFIG_DIR`.
- Errors out if the project directory no longer exists.

## License

MIT
