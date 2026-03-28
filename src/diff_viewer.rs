use crate::shadow::ShadowRepo;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers, MouseEventKind,
};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, HighlightSpacing, List, ListItem, ListState, Paragraph};
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

const TEXT: Color = Color::Rgb(226, 232, 240);
const MUTED: Color = Color::Rgb(148, 163, 184);
const DIM: Color = Color::Rgb(100, 116, 139);
const ACCENT: Color = Color::Rgb(125, 211, 252);
const ACCENT_BG: Color = Color::Rgb(8, 47, 73);
const BORDER: Color = Color::Rgb(51, 65, 85);
const ADDED: Color = Color::Rgb(74, 222, 128);
const REMOVED: Color = Color::Rgb(248, 113, 113);

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
        let selected = self
            .file_state
            .selected()
            .and_then(|idx| self.files.get(idx))
            .cloned();
        let (base, target) = self.snapshot_range();
        self.files = get_changed_files(shadow, &base, &target);
        let next_selection = selected
            .as_ref()
            .and_then(|file| self.files.iter().position(|path| path == file))
            .or_else(|| (!self.files.is_empty()).then_some(0));
        self.file_state.select(next_selection);
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

    fn snapshot_meta(&self) -> String {
        if self.current_snapshot == 0 {
            "Unsaved changes".to_string()
        } else if let Some(snapshot) = self.snapshots.get(self.current_snapshot - 1) {
            format!(
                "snapshot {}  •  {}  •  {}",
                snapshot.hash, snapshot.message, snapshot.time
            )
        } else {
            format!("Snapshot #{}", self.current_snapshot)
        }
    }

    fn selected_file(&self) -> Option<&str> {
        self.file_state
            .selected()
            .and_then(|idx| self.files.get(idx))
            .map(|file| file.as_str())
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
        let next = prev_wrapped_index(self.files.len(), self.file_state.selected());
        self.file_state.select(next);
        self.refresh_diff(shadow);
    }

    fn select_next_file(&mut self, shadow: &ShadowRepo) {
        let next = next_wrapped_index(self.files.len(), self.file_state.selected());
        self.file_state.select(next);
        self.refresh_diff(shadow);
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
                .constraints([
                    Constraint::Length(26),
                    Constraint::Length(1),
                    Constraint::Min(40),
                ])
                .split(rows[1]);

            render_file_list(frame, cols[0], &mut viewer);
            render_divider(frame, cols[1]);
            diff_area_height = render_diff(frame, cols[2], &viewer);
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
        " History ",
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
    )];

    let max_visible = ((area.width as usize).saturating_sub(24) / 10)
        .max(1)
        .min(total);
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
            format!("…{} ", hidden_older),
            Style::default().fg(DIM),
        ));
    }

    for i in (start..end).rev() {
        let idx = i + 1;
        spans.push(Span::raw(" "));
        if idx == current {
            let label = if let Some(s) = viewer.snapshots.get(i) {
                format!("[{} {}]", s.hash, compact_age(&s.time))
            } else {
                format!("[#{}]", idx)
            };
            spans.push(Span::styled(
                label,
                Style::default()
                    .fg(TEXT)
                    .bg(ACCENT_BG)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            let label = if let Some(s) = viewer.snapshots.get(i) {
                format!(" {} ", s.hash)
            } else {
                format!(" #{} ", idx)
            };
            spans.push(Span::styled(label, Style::default().fg(MUTED)));
        }
    }

    if hidden_newer > 0 {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!("…{}", hidden_newer),
            Style::default().fg(DIM),
        ));
    }

    spans.push(Span::raw(" "));
    if current == 0 {
        spans.push(Span::styled(
            "[unsaved]",
            Style::default()
                .fg(TEXT)
                .bg(ACCENT_BG)
                .add_modifier(Modifier::BOLD),
        ));
    } else {
        spans.push(Span::styled(" unsaved ", Style::default().fg(DIM)));
    }

    let bar = Paragraph::new(Line::from(spans)).block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(BORDER)),
    );

    frame.render_widget(bar, area);
}

fn render_file_list(frame: &mut ratatui::Frame, area: Rect, viewer: &mut DiffViewer) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(area);

    let title = Line::from(vec![
        Span::styled(
            " Files ",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("({})", viewer.files.len()),
            Style::default().fg(DIM),
        ),
    ]);
    let title_bar = Paragraph::new(title).block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(BORDER)),
    );
    frame.render_widget(title_bar, rows[0]);

    if viewer.files.is_empty() {
        let empty = Paragraph::new("No changed files")
            .style(Style::default().fg(DIM))
            .alignment(Alignment::Left);
        frame.render_widget(empty, rows[1]);
        return;
    }

    let items: Vec<ListItem> = viewer
        .files
        .iter()
        .map(|file| ListItem::new(file.as_str()))
        .collect();

    let list = List::new(items)
        .highlight_style(
            Style::default()
                .fg(TEXT)
                .bg(ACCENT_BG)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▌ ")
        .highlight_spacing(HighlightSpacing::Always);

    frame.render_stateful_widget(list, rows[1], &mut viewer.file_state);
}

fn render_diff(frame: &mut ratatui::Frame, area: Rect, viewer: &DiffViewer) -> usize {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Min(1),
        ])
        .split(area);

    let title = viewer.selected_file().unwrap_or("No changed files");
    frame.render_widget(
        Paragraph::new(title).style(Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
        rows[0],
    );

    let meta = viewer.snapshot_meta();
    let meta_bar = Paragraph::new(meta)
        .style(Style::default().fg(MUTED))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(BORDER)),
        );
    frame.render_widget(meta_bar, rows[1]);

    let inner_height = rows[2].height as usize;
    if viewer.diff_lines.is_empty() {
        let empty = Paragraph::new("No changes in this view")
            .style(Style::default().fg(DIM))
            .alignment(Alignment::Left);
        frame.render_widget(empty, rows[2]);
        return inner_height;
    }

    let visible: Vec<Line> = viewer
        .diff_lines
        .iter()
        .skip(viewer.diff_scroll)
        .take(inner_height)
        .map(render_diff_line)
        .collect();

    frame.render_widget(Paragraph::new(visible), rows[2]);
    inner_height
}

fn render_footer(frame: &mut ratatui::Frame, area: Rect) {
    let help = Paragraph::new(Line::from(vec![
        Span::styled(" q ", Style::default().fg(ACCENT)),
        Span::styled("quit  ", Style::default().fg(DIM)),
        Span::styled(" tab ", Style::default().fg(ACCENT)),
        Span::styled("files  ", Style::default().fg(DIM)),
        Span::styled(" ←→ ", Style::default().fg(ACCENT)),
        Span::styled("snapshots  ", Style::default().fg(DIM)),
        Span::styled(" ↑↓ ", Style::default().fg(ACCENT)),
        Span::styled("scroll  ", Style::default().fg(DIM)),
        Span::styled(" ^u/^d ", Style::default().fg(ACCENT)),
        Span::styled("page", Style::default().fg(DIM)),
    ]));

    frame.render_widget(help, area);
}

fn render_divider(frame: &mut ratatui::Frame, area: Rect) {
    let lines: Vec<Line> = (0..area.height)
        .map(|_| Line::from(Span::styled("│", Style::default().fg(BORDER))))
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

fn render_diff_line(line: &DiffLine) -> Line<'static> {
    match line {
        DiffLine::Added(text) => styled_diff_line(text, ADDED),
        DiffLine::Removed(text) => styled_diff_line(text, REMOVED),
        DiffLine::Context(text) => Line::from(Span::styled(text.clone(), Style::default().fg(DIM))),
    }
}

fn styled_diff_line(text: &str, color: Color) -> Line<'static> {
    if let Some((prefix, rest)) = text.chars().next().map(|prefix| (prefix, &text[1..])) {
        Line::from(vec![
            Span::styled(
                prefix.to_string(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(rest.to_string(), Style::default().fg(color)),
        ])
    } else {
        Line::default()
    }
}

fn compact_age(age: &str) -> String {
    let age = age.trim();
    if age.is_empty() {
        return String::new();
    }

    match age {
        "just now" => return "now".to_string(),
        "a minute ago" => return "1m".to_string(),
        "an hour ago" => return "1h".to_string(),
        "yesterday" => return "1d".to_string(),
        _ => {}
    }

    let mut parts = age.split_whitespace();
    let value = match parts.next() {
        Some(value) => value,
        None => return age.to_string(),
    };
    let unit = match parts.next() {
        Some(unit) => unit,
        None => return age.to_string(),
    };

    let suffix = match unit {
        "second" | "seconds" => "s",
        "minute" | "minutes" => "m",
        "hour" | "hours" => "h",
        "day" | "days" => "d",
        "week" | "weeks" => "w",
        "month" | "months" => "mo",
        "year" | "years" => "y",
        _ => return age.to_string(),
    };

    format!("{value}{suffix}")
}

fn next_wrapped_index(len: usize, selected: Option<usize>) -> Option<usize> {
    match len {
        0 => None,
        _ => Some(match selected {
            Some(idx) if idx + 1 < len => idx + 1,
            _ => 0,
        }),
    }
}

fn prev_wrapped_index(len: usize, selected: Option<usize>) -> Option<usize> {
    match len {
        0 => None,
        _ => Some(match selected {
            Some(idx) if idx > 0 => idx - 1,
            _ => len - 1,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{next_wrapped_index, prev_wrapped_index};

    #[test]
    fn next_wrapped_index_loops_to_start() {
        assert_eq!(next_wrapped_index(3, Some(0)), Some(1));
        assert_eq!(next_wrapped_index(3, Some(1)), Some(2));
        assert_eq!(next_wrapped_index(3, Some(2)), Some(0));
    }

    #[test]
    fn prev_wrapped_index_loops_to_end() {
        assert_eq!(prev_wrapped_index(3, Some(2)), Some(1));
        assert_eq!(prev_wrapped_index(3, Some(1)), Some(0));
        assert_eq!(prev_wrapped_index(3, Some(0)), Some(2));
    }

    #[test]
    fn wrapped_index_handles_empty_or_unselected_state() {
        assert_eq!(next_wrapped_index(0, None), None);
        assert_eq!(prev_wrapped_index(0, None), None);
        assert_eq!(next_wrapped_index(3, None), Some(0));
        assert_eq!(prev_wrapped_index(3, None), Some(2));
    }
}

fn load_snapshots(shadow: &ShadowRepo) -> Vec<SnapshotInfo> {
    let output = shadow
        .shadow_git(&["log", "--format=%h|%s|%ar", "--skip=1"])
        .unwrap_or_default();

    output
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(3, '|').collect();
            if parts.len() >= 3 {
                Some(SnapshotInfo {
                    hash: parts[0].to_string(),
                    message: parts[1].to_string(),
                    time: parts[2].to_string(),
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
