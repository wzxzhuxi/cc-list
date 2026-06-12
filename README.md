# cc-list

[English](README.en.md)

跨项目浏览所有 Claude Code 历史会话的 TUI,选中后:

- **覆盖当前终端**恢复 (enter)
- 在**新 tab** 中恢复 (ctrl-t)
- 在**新窗口**中恢复 (ctrl-n)
- 在 **overlay/弹窗**中恢复 (ctrl-o)

开窗动作在运行时探测环境,自动适配 tmux / zellij / kitty / wezterm / Windows Terminal / 任意 GUI 终端 / macOS Terminal.app。恢复前自动 cd 到会话记录的 cwd,跨项目恢复不需要手动进目录。

单二进制 (ratatui),运行时仅依赖 claude CLI,Linux/macOS/Windows 通用。

## 安装

### 一键安装 (Linux/macOS)

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/wzxzhuxi/cc-list/releases/latest/download/cc-list-installer.sh | sh
```

Windows (PowerShell):

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/wzxzhuxi/cc-list/releases/latest/download/cc-list-installer.ps1 | iex"
```

安装到 `~/.cargo/bin`(不需要 Rust 环境)。

### 从源码构建

```bash
git clone https://github.com/wzxzhuxi/cc-list.git
cd cc-list && cargo build --release
ln -s "$PWD/target/release/cc-list" ~/.local/bin/cc-list
```

或下载 [Releases](https://github.com/wzxzhuxi/cc-list/releases) 中对应平台的预编译二进制放入 PATH。

### Claude Code 插件 (可选)

提供会话内的 `/cc-list:resume` 斜杠命令 (需要 cc-list 二进制已在 PATH):

```
/plugin marketplace add wzxzhuxi/cc-list
/plugin install cc-list@cc-list-marketplace
```

### kitty 用户 (可选)

`~/.config/kitty/kitty.conf` 开启远程控制 (tab/overlay 模式需要),可绑全局快捷键随处呼出:

```
allow_remote_control yes
map ctrl+shift+r launch --type=overlay cc-list
```

修改后完全重启 kitty。tmux/wezterm/zellij 用户无需任何配置。

## 使用

```bash
cc-list              # TUI 浏览全部会话
cc-list breeze       # 带初始搜索词
cc-list list 30      # TSV 输出最近 30 条 (脚本/插件消费)
cc-list open tab ~/proj <session-id>   # 直接开窗, 模式: tab|window|overlay|here
```

界面:上方为会话列表 (日期/项目目录/首条消息),中间模糊搜索,下方预览该会话末尾的对话。直接输入即过滤;`↑↓` 选择,`pgup/pgdn` 滚动预览;`enter`/`^t`/`^n`/`^o` 按上述四种方式打开。

会话内:`/cc-list:resume [搜索词]`。

## 开窗后端映射

| 环境 | tab | window | overlay |
|------|-----|--------|---------|
| tmux | `new-window` | 新 GUI 窗口 (无图形环境时同 tab) | `display-popup` (tmux ≥ 3.2) |
| zellij | `run` (新 pane) | 新 GUI 窗口 | `run --floating` |
| kitty + `allow_remote_control` | `@ launch --type=tab` | `--type=os-window` | `--type=overlay` |
| wezterm | `cli spawn` | `cli spawn --new-window` | `cli split-pane` |
| Windows Terminal | `wt nt` | `wt` | 不支持 |
| 其他 GUI 终端 | 回退为新窗口 | `$TERMINAL` 或探测 kitty/alacritty/foot/ghostty/konsole/gnome-terminal 等 | 不支持,报错提示 |
| macOS 无探测命中 | — | Terminal.app (osascript) | — |

## 注意

- "覆盖当前终端"在 Unix 上通过 `exec` 系统调用替换进程实现;Windows 上退化为 spawn + 等待。只在 shell 入口可用 —— Claude Code 进程无法替换承载它自己的终端,会话内最接近的形态是 overlay。
- 会话数据读取自 `~/.claude/projects/*/*.jsonl`,尊重 `CLAUDE_CONFIG_DIR`。
- 项目目录已删除时报错退出。

## License

MIT
