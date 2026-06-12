//! ratatui 交互界面: 上方搜索 + 会话列表, 下方对话预览
//!
//! 配色遵循 ProArt 风格: 近黑背景, 金色唯一强调色

use crate::open::{self, Mode};
use crate::sessions;
use crate::sessions::{Msg, Role, Session};
use anyhow::{bail, Result};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Cell, Paragraph, Row, Table, TableState, Wrap};
use ratatui::{DefaultTerminal, Frame};
use std::collections::HashMap;

const GOLD: Color = Color::Rgb(0xc9, 0xa9, 0x62);
const GOLD_DIM: Color = Color::Rgb(0x8b, 0x73, 0x55);
const BG_HL: Color = Color::Rgb(0x1a, 0x1a, 0x1a);
const TEXT: Color = Color::Rgb(0xe5, 0xe5, 0xe5);
const TEXT_SEC: Color = Color::Rgb(0xa3, 0xa3, 0xa3);
const MUTED: Color = Color::Rgb(0x52, 0x52, 0x52);
const ERR: Color = Color::Rgb(0xb0, 0x5c, 0x5c);

enum Action {
    Quit,
    /// enter: 退出 TUI 后由 main 以 Here 模式 exec
    Resume(usize),
}

struct App {
    sessions: Vec<Session>,
    query: String,
    /// 当前过滤结果 (sessions 的下标)
    filtered: Vec<usize>,
    table: TableState,
    matcher: SkimMatcherV2,
    preview_cache: HashMap<usize, Vec<Msg>>,
    /// None = 自动锚定到对话末尾
    preview_scroll: Option<u16>,
    /// 最近一次 tab/window/overlay 操作的结果 (消息, 是否错误)
    status: Option<(String, bool)>,
}

/// 返回 enter 选中的 (项目目录, 会话id); Quit 时返回 None
pub fn run(initial_query: &str) -> Result<Option<(String, String)>> {
    use std::io::IsTerminal;
    if !std::io::stdout().is_terminal() || !std::io::stdin().is_terminal() {
        bail!("interactive UI requires a TTY (use `cc-list list` in non-interactive contexts)");
    }
    let sessions = sessions::scan()?;
    if sessions.is_empty() {
        bail!("no sessions found");
    }
    let mut app = App::new(sessions, initial_query);
    let mut terminal = ratatui::init();
    let action = app.event_loop(&mut terminal);
    ratatui::restore();
    match action? {
        Action::Quit => Ok(None),
        Action::Resume(idx) => {
            let s = &app.sessions[idx];
            Ok(Some((s.cwd.clone(), s.sid.clone())))
        }
    }
}

impl App {
    fn new(sessions: Vec<Session>, initial_query: &str) -> Self {
        let mut app = Self {
            sessions,
            query: initial_query.to_string(),
            filtered: Vec::new(),
            table: TableState::default(),
            matcher: SkimMatcherV2::default(),
            preview_cache: HashMap::new(),
            preview_scroll: None,
            status: None,
        };
        app.refilter();
        app
    }

    fn current(&self) -> Option<usize> {
        self.table
            .selected()
            .and_then(|i| self.filtered.get(i))
            .copied()
    }

    fn refilter(&mut self) {
        let q = self.query.trim();
        if q.is_empty() {
            // 已按时间倒序
            self.filtered = (0..self.sessions.len()).collect();
        } else {
            let mut scored: Vec<(i64, usize)> = self
                .sessions
                .iter()
                .enumerate()
                .filter_map(|(i, s)| {
                    let hay = format!("{} {} {}", s.cwd_display, s.summary, s.date);
                    self.matcher.fuzzy_match(&hay, q).map(|sc| (sc, i))
                })
                .collect();
            // 稳定排序: 同分时保持时间倒序
            scored.sort_by(|a, b| b.0.cmp(&a.0));
            self.filtered = scored.into_iter().map(|(_, i)| i).collect();
        }
        // 自下而上: 最新/最匹配的在底部 (贴近搜索栏), 默认选中底部
        self.filtered.reverse();
        self.table.select(self.filtered.len().checked_sub(1));
        self.preview_scroll = None;
    }

    fn move_sel(&mut self, delta: i64) {
        if self.filtered.is_empty() {
            return;
        }
        let cur = self.table.selected().unwrap_or(0) as i64;
        let next = (cur + delta).clamp(0, self.filtered.len() as i64 - 1);
        self.table.select(Some(next as usize));
        self.preview_scroll = None;
    }

    fn event_loop(&mut self, terminal: &mut DefaultTerminal) -> Result<Action> {
        loop {
            terminal.draw(|f| self.draw(f))?;
            let Event::Key(k) = event::read()? else {
                continue;
            };
            if k.kind != KeyEventKind::Press {
                continue;
            }
            let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
            match k.code {
                KeyCode::Esc => return Ok(Action::Quit),
                KeyCode::Char('c') if ctrl => return Ok(Action::Quit),
                KeyCode::Enter => {
                    if let Some(i) = self.current() {
                        return Ok(Action::Resume(i));
                    }
                }
                // 派生动作: 在循环内执行, 不退出 TUI, 结果显示在状态行
                KeyCode::Char('t') if ctrl => self.open_aux(Mode::Tab, terminal)?,
                KeyCode::Char('n') if ctrl => self.open_aux(Mode::Window, terminal)?,
                KeyCode::Char('o') if ctrl => self.open_aux(Mode::Overlay, terminal)?,
                KeyCode::Up => self.move_sel(-1),
                KeyCode::Down => self.move_sel(1),
                KeyCode::Char('p') if ctrl => self.move_sel(-1),
                KeyCode::Char('j') if ctrl => self.move_sel(1),
                KeyCode::Char('k') if ctrl => self.move_sel(-1),
                KeyCode::PageUp => {
                    let cur = self.preview_scroll.unwrap_or(u16::MAX);
                    self.preview_scroll = Some(cur.saturating_sub(10));
                }
                KeyCode::PageDown => {
                    self.preview_scroll =
                        Some(self.preview_scroll.unwrap_or(u16::MAX).saturating_add(10));
                }
                KeyCode::Backspace => {
                    self.query.pop();
                    self.refilter();
                }
                KeyCode::Char(ch) if !ctrl => {
                    self.query.push(ch);
                    self.refilter();
                }
                _ => {}
            }
        }
    }

    /// 不退出 TUI 的打开动作; 外部命令可能弄脏屏幕 (tmux popup / 焦点切换), 执行后强制全量重绘
    fn open_aux(&mut self, mode: Mode, terminal: &mut DefaultTerminal) -> Result<()> {
        let Some(i) = self.current() else {
            return Ok(());
        };
        let s = &self.sessions[i];
        let result = open::open(mode, &s.cwd, &s.sid);
        terminal.clear()?;
        self.status = Some(match result {
            Ok(msg) => (msg, false),
            Err(e) => (format!("{e:#}"), true),
        });
        Ok(())
    }

    fn draw(&mut self, f: &mut Frame) {
        // fzf 风格布局: 列表在上, 搜索栏居中 (上下留白), 预览在下
        let [list_area, _, search, status, preview_area, footer] = Layout::vertical([
            Constraint::Min(5),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Percentage(55),
            Constraint::Length(1),
        ])
        .areas(f.area());

        self.draw_list(f, list_area);
        self.draw_search(f, search);
        self.draw_status(f, status);
        self.draw_preview(f, preview_area);
        draw_footer(f, footer);
    }

    fn draw_status(&self, f: &mut Frame, area: Rect) {
        let Some((msg, is_err)) = &self.status else {
            return;
        };
        let (mark, color) = if *is_err {
            ("[!] ", ERR)
        } else {
            ("[i] ", GOLD_DIM)
        };
        let line = Line::from(vec![
            Span::styled(format!(" {mark}"), Style::new().fg(color)),
            Span::styled(msg.clone(), Style::new().fg(color)),
        ]);
        f.render_widget(Paragraph::new(line), area);
    }

    fn draw_search(&self, f: &mut Frame, area: Rect) {
        let count = format!(" {}/{} ", self.filtered.len(), self.sessions.len());
        let line = Line::from(vec![
            Span::styled(" CC-LIST ", Style::new().fg(GOLD)),
            Span::styled("│ ", Style::new().fg(GOLD_DIM)),
            Span::styled("search: ", Style::new().fg(TEXT_SEC)),
            Span::styled(self.query.clone(), Style::new().fg(TEXT)),
            Span::styled("▌", Style::new().fg(GOLD)),
            Span::styled(count, Style::new().fg(MUTED)),
        ]);
        f.render_widget(Paragraph::new(line), area);
    }

    fn draw_list(&mut self, f: &mut Frame, area: Rect) {
        // 列表不满时锚定到底部, 贴着搜索栏
        let needed = (self.filtered.len() as u16).saturating_add(2); // +2 边框
        let h = needed.min(area.height);
        let area = Rect {
            y: area.y + area.height - h,
            height: h,
            ..area
        };
        let rows: Vec<Row> = self
            .filtered
            .iter()
            .map(|&i| {
                let s = &self.sessions[i];
                Row::new(vec![
                    Cell::from(s.date.clone()).style(Style::new().fg(MUTED)),
                    Cell::from(s.cwd_display.clone()).style(Style::new().fg(GOLD_DIM)),
                    Cell::from(s.summary.clone()).style(Style::new().fg(TEXT)),
                ])
            })
            .collect();
        let table = Table::new(
            rows,
            [
                Constraint::Length(11),
                Constraint::Max(40),
                Constraint::Fill(1),
            ],
        )
        .row_highlight_style(Style::new().bg(BG_HL))
        .highlight_symbol(Span::styled("▌ ", Style::new().fg(GOLD)))
        .block(
            Block::bordered()
                .border_style(Style::new().fg(GOLD_DIM))
                .title(Span::styled(" Sessions ", Style::new().fg(GOLD))),
        );
        f.render_stateful_widget(table, area, &mut self.table);
    }

    fn draw_preview(&mut self, f: &mut Frame, area: Rect) {
        let Some(idx) = self.current() else {
            let empty = Paragraph::new(Line::styled(
                "(no matching sessions)",
                Style::new().fg(MUTED),
            ))
            .block(Block::bordered().border_style(Style::new().fg(GOLD_DIM)));
            f.render_widget(empty, area);
            return;
        };
        let file = self.sessions[idx].file.clone();
        let msgs = self
            .preview_cache
            .entry(idx)
            .or_insert_with(|| sessions::load_messages(&file, 40));

        let mut lines: Vec<Line> = Vec::new();
        for m in msgs.iter() {
            let (label, color) = match m.role {
                Role::User => ("── user ─────────────────────", GOLD),
                Role::Assistant => ("── claude ───────────────────", MUTED),
            };
            lines.push(Line::styled(label, Style::new().fg(color)));
            let body = if m.role == Role::User { TEXT } else { TEXT_SEC };
            for l in m.text.lines() {
                lines.push(Line::styled(l.to_string(), Style::new().fg(body)));
            }
            lines.push(Line::raw(""));
        }

        // 估算换行后的总行数, 默认锚定到对话末尾
        let inner_w = area.width.saturating_sub(2).max(1) as usize;
        let inner_h = area.height.saturating_sub(2);
        let total: u16 = lines
            .iter()
            .map(|l| (l.width().max(1)).div_ceil(inner_w) as u16)
            .sum();
        let max_scroll = total.saturating_sub(inner_h);
        let scroll = self.preview_scroll.unwrap_or(max_scroll).min(max_scroll);
        self.preview_scroll = self.preview_scroll.map(|_| scroll);

        let s = &self.sessions[idx];
        let title = format!(" {} · {} ", s.cwd_display, s.date);
        let para = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0))
            .block(
                Block::bordered()
                    .border_style(Style::new().fg(GOLD_DIM))
                    .title(Span::styled(title, Style::new().fg(GOLD))),
            );
        f.render_widget(para, area);
    }
}

fn draw_footer(f: &mut Frame, area: Rect) {
    let key = |k: &'static str| Span::styled(k, Style::new().fg(GOLD_DIM));
    let txt = |t: &'static str| Span::styled(t, Style::new().fg(MUTED));
    let line = Line::from(vec![
        key(" enter"),
        txt(" resume here  "),
        key("^t"),
        txt(" tab  "),
        key("^n"),
        txt(" window  "),
        key("^o"),
        txt(" overlay  "),
        key("↑↓"),
        txt(" select  "),
        key("pgup/pgdn"),
        txt(" scroll  "),
        key("esc"),
        txt(" quit"),
    ]);
    f.render_widget(Paragraph::new(line), area);
}
