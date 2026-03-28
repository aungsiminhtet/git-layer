use crate::shadow::ShadowRepo;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers, MouseEventKind,
};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Terminal;
use std::io;

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = stdout.execute(DisableMouseCapture);
        let _ = stdout.execute(LeaveAlternateScreen);
    }
}

struct SnapshotInfo {
    hash: String,
    message: String,
    author: String,
    time: String,
}

#[derive(Clone)]
enum DiffLine {
    Context(String),
    Added(String),
    Removed(String),
}

struct DiffViewer {
    snapshots: Vec<SnapshotInfo>,
    current_snapshot: usize,
    files: Vec<String>,
    file_state: ListState,
    diff_lines: Vec<DiffLine>,
    diff_scroll: usize,
}

impl DiffViewer {
    fn new(shadow: &ShadowRepo) -> Self {
        let snapshots = load_snapshots(shadow);
        let mut viewer = Self {
            snapshots,
            current_snapshot: 0,
            files: Vec::new(),
            file_state: ListState::default(),
            diff_lines: Vec::new(),
            diff_scroll: 0,
        };
        viewer.refresh_files(shadow);
        viewer
    }

    fn refresh_files(&mut self, shadow: &ShadowRepo) {
        let (base, target) = self.snapshot_range();
        self.files = get_changed_files(shadow, &base, &target);
        if !self.files.is_empty() {
            self.file_state.select(Some(0));
        } else {
            self.file_state.select(None);
        }
        self.refresh_diff(shadow);
    }

    fn refresh_diff(&mut self, shadow: &ShadowRepo) {
        self.diff_scroll = 0;
        let (base, target) = self.snapshot_range();
        if let Some(idx) = self.file_state.selected() {
            if let Some(file) = self.files.get(idx) {
                self.diff_lines = get_file_diff(shadow, &base, &target, file);
                return;
            }
        }
        self.diff_lines = Vec::new();
    }

    fn snapshot_range(&self) -> (String, String) {
        if self.current_snapshot == 0 {
            ("HEAD".to_string(), String::new())
        } else {
            let target = if self.current_snapshot == 1 {
                "HEAD".to_string()
            } else {
                format!("HEAD~{}", self.current_snapshot - 1)
            };
            let base = format!("HEAD~{}", self.current_snapshot);
            (base, target)
        }
    }

    fn snapshot_label(&self) -> String {
        if self.current_snapshot == 0 {
            "Unsaved changes".to_string()
        } else if let Some(s) = self.snapshots.get(self.current_snapshot - 1) {
            format!("{} — {} ({}, {})", s.hash, s.message, s.time, s.author)
        } else {
            format!("Snapshot #{}", self.current_snapshot)
        }
    }

    fn prev_snapshot(&mut self, shadow: &ShadowRepo) {
        if self.current_snapshot < self.snapshots.len() {
            self.current_snapshot += 1;
            self.refresh_files(shadow);
        }
    }

    fn next_snapshot(&mut self, shadow: &ShadowRepo) {
        if self.current_snapshot > 0 {
            self.current_snapshot -= 1;
            self.refresh_files(shadow);
        }
    }

    fn select_prev_file(&mut self, shadow: &ShadowRepo) {
        if let Some(idx) = self.file_state.selected() {
            if idx > 0 {
                self.file_state.select(Some(idx - 1));
                self.refresh_diff(shadow);
            }
        }
    }

    fn select_next_file(&mut self, shadow: &ShadowRepo) {
        if let Some(idx) = self.file_state.selected() {
            if idx + 1 < self.files.len() {
                self.file_state.select(Some(idx + 1));
                self.refresh_diff(shadow);
            }
        }
    }

    fn scroll_up(&mut self) {
        self.diff_scroll = self.diff_scroll.saturating_sub(1);
    }

    fn scroll_down(&mut self, visible_height: usize) {
        let max = self.diff_lines.len().saturating_sub(visible_height);
        if self.diff_scroll < max {
            self.diff_scroll += 1;
        }
    }

    fn half_page_up(&mut self, visible_height: usize) {
        let half = visible_height / 2;
        self.diff_scroll = self.diff_scroll.saturating_sub(half);
    }

    fn half_page_down(&mut self, visible_height: usize) {
        let half = visible_height / 2;
        let max = self.diff_lines.len().saturating_sub(visible_height);
        self.diff_scroll = (self.diff_scroll + half).min(max);
    }
}

pub fn run_interactive(shadow: &ShadowRepo) -> anyhow::Result<()> {
    let mut stdout = io::stdout();
    terminal::enable_raw_mode()?;
    let _guard = TerminalGuard;
    stdout.execute(EnterAlternateScreen)?;
    stdout.execute(EnableMouseCapture)?;

    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut viewer = DiffViewer::new(shadow);
    let mut diff_area_height: usize = 20;

    loop {
        terminal.draw(|frame| {
            let size = frame.area();

            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(5),
                    Constraint::Length(1),
                ])
                .split(size);

            render_snapshot_bar(frame, rows[0], &viewer);

            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(28), Constraint::Min(40)])
                .split(rows[1]);

            render_file_list(frame, cols[0], &mut viewer);
            diff_area_height = render_diff(frame, cols[1], &viewer);
            render_footer(frame, rows[2]);
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => match (key.code, key.modifiers) {
                    (KeyCode::Char('q'), _) | (KeyCode::Esc, _) => break,
                    (KeyCode::Up, _) | (KeyCode::Char('k'), KeyModifiers::NONE) => {
                        viewer.scroll_up();
                    }
                    (KeyCode::Down, _) | (KeyCode::Char('j'), KeyModifiers::NONE) => {
                        viewer.scroll_down(diff_area_height);
                    }
                    (KeyCode::Tab, KeyModifiers::NONE) => {
                        viewer.select_next_file(shadow);
                    }
                    (KeyCode::BackTab, _) => {
                        viewer.select_prev_file(shadow);
                    }
                    (KeyCode::Left, _) | (KeyCode::Char('h'), KeyModifiers::NONE) => {
                        viewer.prev_snapshot(shadow);
                    }
                    (KeyCode::Right, _) | (KeyCode::Char('l'), KeyModifiers::NONE) => {
                        viewer.next_snapshot(shadow);
                    }
                    (KeyCode::Char('w'), KeyModifiers::NONE) => viewer.scroll_up(),
                    (KeyCode::Char('s'), KeyModifiers::NONE) => {
                        viewer.scroll_down(diff_area_height);
                    }
                    (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                        viewer.half_page_up(diff_area_height);
                    }
                    (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                        viewer.half_page_down(diff_area_height);
                    }
                    _ => {}
                },
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::ScrollUp => viewer.scroll_up(),
                    MouseEventKind::ScrollDown => viewer.scroll_down(diff_area_height),
                    _ => {}
                },
                _ => {}
            }
        }
    }

    Ok(())
}

fn render_snapshot_bar(frame: &mut ratatui::Frame, area: Rect, viewer: &DiffViewer) {
    let total = viewer.snapshots.len();
    let current = viewer.current_snapshot;

    let mut spans = vec![Span::styled(
        " Snapshots: ",
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )];

    // Oldest snapshots first (left = oldest, right = newest/unsaved)
    let max_visible = (area.width as usize).saturating_sub(30).min(total);
    let selected = current.saturating_sub(1);
    let start = if max_visible == 0 || selected < max_visible {
        0
    } else {
        (selected + 1).saturating_sub(max_visible)
    };
    let end = (start + max_visible).min(total);
    let hidden_older = total.saturating_sub(end);
    let hidden_newer = start;

    if hidden_older > 0 {
        spans.push(Span::styled(
            format!("+{} more ", hidden_older),
            Style::default().fg(Color::DarkGray),
        ));
    }

    for i in (start..end).rev() {
        let idx = i + 1;
        spans.push(Span::raw(" "));
        if idx == current {
            let label = if let Some(s) = viewer.snapshots.get(i) {
                format!("[{} {}]", s.hash, s.time)
            } else {
                format!("[#{}]", idx)
            };
            spans.push(Span::styled(
                label,
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            let label = if let Some(s) = viewer.snapshots.get(i) {
                format!(" {} ", s.hash)
            } else {
                format!(" #{} ", idx)
            };
            spans.push(Span::styled(label, Style::default().fg(Color::DarkGray)));
        }
    }

    if hidden_newer > 0 {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!("+{} newer", hidden_newer),
            Style::default().fg(Color::DarkGray),
        ));
    }

    spans.push(Span::raw(" "));
    if current == 0 {
        spans.push(Span::styled(
            "[unsaved]",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
    } else {
        spans.push(Span::styled(
            " unsaved ",
            Style::default().fg(Color::DarkGray),
        ));
    }

    let bar = Paragraph::new(Line::from(spans)).block(Block::default().borders(Borders::BOTTOM));

    frame.render_widget(bar, area);
}

fn render_file_list(frame: &mut ratatui::Frame, area: Rect, viewer: &mut DiffViewer) {
    let items: Vec<ListItem> = viewer
        .files
        .iter()
        .map(|f| ListItem::new(f.as_str()))
        .collect();

    let title = format!(" Files ({}) ", viewer.files.len());
    let list = List::new(items)
        .block(Block::default().title(title).borders(Borders::ALL))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    frame.render_stateful_widget(list, area, &mut viewer.file_state);
}

fn render_diff(frame: &mut ratatui::Frame, area: Rect, viewer: &DiffViewer) -> usize {
    let inner_height = area.height.saturating_sub(2) as usize;

    let visible: Vec<Line> = viewer
        .diff_lines
        .iter()
        .skip(viewer.diff_scroll)
        .take(inner_height)
        .map(|line| match line {
            DiffLine::Added(text) => Line::from(Span::styled(
                text.clone(),
                Style::default().fg(Color::Green),
            )),
            DiffLine::Removed(text) => {
                Line::from(Span::styled(text.clone(), Style::default().fg(Color::Red)))
            }
            DiffLine::Context(text) => Line::from(Span::styled(
                text.clone(),
                Style::default().fg(Color::DarkGray),
            )),
        })
        .collect();

    let label = viewer.snapshot_label();
    let title = format!(" {} ", label);

    let paragraph =
        Paragraph::new(visible).block(Block::default().title(title).borders(Borders::ALL));

    frame.render_widget(paragraph, area);
    inner_height
}

fn render_footer(frame: &mut ratatui::Frame, area: Rect) {
    let footer = Paragraph::new(Line::from(vec![
        Span::styled(" ↑↓ ", Style::default().fg(Color::Cyan)),
        Span::styled("scroll  ", Style::default().fg(Color::DarkGray)),
        Span::styled("tab/shift+tab ", Style::default().fg(Color::Cyan)),
        Span::styled("file  ", Style::default().fg(Color::DarkGray)),
        Span::styled("←→ ", Style::default().fg(Color::Cyan)),
        Span::styled("snapshot  ", Style::default().fg(Color::DarkGray)),
        Span::styled("^u/^d ", Style::default().fg(Color::Cyan)),
        Span::styled("half-page  ", Style::default().fg(Color::DarkGray)),
        Span::styled("q ", Style::default().fg(Color::Cyan)),
        Span::styled("quit", Style::default().fg(Color::DarkGray)),
    ]));

    frame.render_widget(footer, area);
}

fn load_snapshots(shadow: &ShadowRepo) -> Vec<SnapshotInfo> {
    let output = shadow
        .shadow_git(&["log", "--format=%h|%s|%an|%ar", "--skip=1"])
        .unwrap_or_default();

    output
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(4, '|').collect();
            if parts.len() >= 4 {
                Some(SnapshotInfo {
                    hash: parts[0].to_string(),
                    message: parts[1].to_string(),
                    author: parts[2].to_string(),
                    time: parts[3].to_string(),
                })
            } else {
                None
            }
        })
        .collect()
}

fn get_changed_files(shadow: &ShadowRepo, base: &str, target: &str) -> Vec<String> {
    let output = if target.is_empty() {
        shadow.shadow_git(&["diff", "--name-only", base])
    } else {
        shadow.shadow_git(&["diff", "--name-only", base, target])
    };

    output
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_string())
        .collect()
}

fn get_file_diff(shadow: &ShadowRepo, base: &str, target: &str, file: &str) -> Vec<DiffLine> {
    let output = if target.is_empty() {
        shadow.shadow_git(&["diff", base, "--", file])
    } else {
        shadow.shadow_git(&["diff", base, target, "--", file])
    };

    parse_diff(&output.unwrap_or_default())
}

fn parse_diff(raw: &str) -> Vec<DiffLine> {
    raw.lines()
        .filter_map(|line| {
            if line.starts_with("diff ")
                || line.starts_with("index ")
                || line.starts_with("+++")
                || line.starts_with("---")
                || line.starts_with("@@")
            {
                None
            } else if line.starts_with('+') {
                Some(DiffLine::Added(line.to_string()))
            } else if line.starts_with('-') {
                Some(DiffLine::Removed(line.to_string()))
            } else {
                Some(DiffLine::Context(line.to_string()))
            }
        })
        .collect()
}
