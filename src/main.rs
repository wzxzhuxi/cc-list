//! cc-list: 跨项目浏览/恢复 Claude Code 历史会话
//!
//! 无子命令     → ratatui 交互界面 (可带初始搜索词)
//! list [N]     → TSV 会话列表 (供插件斜杠命令消费)
//! open <mode>  → 终端适配层, 在 tab/window/overlay/here 中打开会话

mod open;
mod sessions;
mod ui;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "cc-list",
    version,
    about = "Browse/resume Claude Code sessions across projects"
)]
struct Cli {
    /// Initial fuzzy query for the interactive UI
    query: Option<String>,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Print sessions as TSV: epoch/date/session_id/file/cwd/summary
    List {
        /// Max rows, 0 = unlimited
        #[arg(default_value_t = 0)]
        limit: usize,
    },
    /// Open a session: tab|window|overlay|here
    Open {
        #[arg(value_enum)]
        mode: open::Mode,
        /// Project directory (~ prefix supported)
        dir: String,
        /// Session id
        sid: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Some(Cmd::List { limit }) => cmd_list(limit),
        Some(Cmd::Open { mode, dir, sid }) => {
            let msg = open::open(mode, &dir, &sid)?;
            println!("{msg}");
            Ok(())
        }
        None => {
            let query = cli.query.unwrap_or_default();
            match ui::run(&query)? {
                None => Ok(()),
                // enter: 终端已恢复, exec 覆盖当前进程
                Some((cwd, sid)) => open::open(open::Mode::Here, &cwd, &sid).map(|_| ()),
            }
        }
    }
}

fn cmd_list(limit: usize) -> Result<()> {
    let sessions = sessions::scan()?;
    let n = if limit == 0 { usize::MAX } else { limit };
    let mut out = String::new();
    for s in sessions.iter().take(n) {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\n",
            s.epoch,
            s.date,
            s.sid,
            s.file.display(),
            s.cwd_display,
            s.summary
        ));
    }
    print!("{out}");
    Ok(())
}
