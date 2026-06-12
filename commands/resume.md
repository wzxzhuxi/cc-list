---
description: 浏览所有项目的历史会话,选择后在新 tab/新窗口/overlay 中恢复 (自动适配 tmux/zellij/kitty/wezterm/通用终端)
argument-hint: "[搜索词]"
allowed-tools: Bash(cc-list:*)
---

## 历史会话列表 (最近 30 条)

!`cc-list list 30`

## 任务

上面是所有项目的 Claude Code 历史会话,TSV 列含义: epoch、日期、session_id、文件路径、项目目录、首条用户消息摘要。

如果上面报错 "command not found",说明 cc-list 二进制未安装,提示用户: 从 https://github.com/wzxzhuxi/cc-list/releases 下载或 `cargo build --release` 构建,并放入 PATH。

1. 如果用户提供了搜索词 "$ARGUMENTS",先按项目路径和摘要过滤;无匹配时展示全部并说明。
2. 用紧凑表格展示候选会话: 序号、日期、项目目录、摘要(截断到 60 字符)。不要展示 epoch、session_id 和文件路径。
3. 用 AskUserQuestion 让用户选择要恢复的会话(最多列 4 个最相关的作为选项,其余的用户可通过 Other 输入序号),然后再问打开方式: **新 tab** / **新窗口** / **overlay (覆盖当前窗口)**。
4. 执行: `cc-list open <tab|window|overlay> '<项目目录>' <session_id>`
   - cc-list 会自动探测当前终端 (tmux/zellij/kitty/wezterm/通用 GUI 终端) 并选择对应的开窗方式,项目目录中的 `~` 也由它展开。
5. 如果 cc-list 报错,把它的 stderr 原样转告用户 (里面包含针对当前终端的修复提示,例如 kitty 需要 `allow_remote_control yes`)。
6. 提醒: 想"覆盖当前终端"(退出当前 claude 并原地恢复)只能在 shell 中直接运行 `cc-list` 完成,会话内做不到。
