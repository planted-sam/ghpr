mod compose;
mod pr_detail;
mod pr_list;

use jiff::Timestamp;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{App, Screen};

const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn draw(f: &mut Frame, app: &mut App) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(f.area());

    draw_header(f, app, header);
    match app.screen {
        Screen::List => pr_list::draw(f, app, body),
        Screen::Detail => pr_detail::draw(f, app, body),
    }
    draw_footer(f, app, footer);
    compose::draw(f, app);
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let mut spans = vec![
        Span::styled(" ghpr ", Style::new().add_modifier(Modifier::BOLD)),
        Span::styled(format!("@{} ", app.viewer), dim()),
    ];
    if app.loading {
        spans.push(Span::styled(
            format!(
                "{} loading… ",
                SPINNER_FRAMES[app.spinner % SPINNER_FRAMES.len()]
            ),
            Style::new().fg(Color::Yellow),
        ));
    }
    if let Some(version) = &app.update_available {
        spans.push(Span::styled(
            format!("· update available: v{version} — press U to install "),
            Style::new().fg(Color::Yellow),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let line = if let Some(err) = &app.error {
        Line::styled(format!(" ✗ {err}"), Style::new().fg(Color::Red))
    } else if let Some(status) = &app.status {
        Line::styled(format!(" {status}"), Style::new().fg(Color::Green))
    } else {
        let hints = if app.compose.is_some() {
            " Ctrl-S send · Esc cancel"
        } else {
            match app.screen {
                Screen::List => " j/k move · Enter open · r refresh · o browser · q quit",
                Screen::Detail => {
                    " Tab pane · j/k select · J/K comment · y copy · ]/[ unresolved · s sort · c comment · a reply · x resolve · C-d/u scroll · r refresh · o browser · Esc back"
                }
            }
        };
        Line::styled(hints, dim())
    };
    f.render_widget(Paragraph::new(line), area);
}

pub fn dim() -> Style {
    Style::new().add_modifier(Modifier::DIM)
}

/// Make a single line safe for terminal cells: expand tabs (ratatui treats
/// them as zero-width, but the terminal jumps to a tab stop — misaligning the
/// buffer diff and leaving stale cells) and drop other control characters.
pub fn sanitize(line: &str) -> String {
    line.replace('\t', "    ")
        .chars()
        .filter(|c| !c.is_control())
        .collect()
}

/// Render a comment body as styled markdown lines, owned (so callers can drop
/// the borrow of App before rendering) and cell-sanitized.
pub fn markdown_body(body: &str) -> Vec<Line<'static>> {
    tui_markdown::from_str(body)
        .lines
        .into_iter()
        .map(|line| {
            let spans: Vec<Span<'static>> = line
                .spans
                .into_iter()
                .map(|s| Span::styled(sanitize(&s.content), s.style))
                .collect();
            let mut out = Line::from(spans);
            out.style = line.style;
            out.alignment = line.alignment;
            out
        })
        .collect()
}

pub fn relative_age(ts: Timestamp) -> String {
    let secs = (Timestamp::now().as_second() - ts.as_second()).max(0);
    const MIN: i64 = 60;
    const HOUR: i64 = 60 * MIN;
    const DAY: i64 = 24 * HOUR;
    if secs < MIN {
        "now".to_string()
    } else if secs < HOUR {
        format!("{}m", secs / MIN)
    } else if secs < DAY {
        format!("{}h", secs / HOUR)
    } else if secs < 30 * DAY {
        format!("{}d", secs / DAY)
    } else if secs < 365 * DAY {
        format!("{}mo", secs / (30 * DAY))
    } else {
        format!("{}y", secs / (365 * DAY))
    }
}

/// Style the tail of a diff hunk: + green, - red, @@ cyan, context dim.
pub fn hunk_lines(hunk: &str, max_lines: usize) -> Vec<Line<'static>> {
    let lines: Vec<&str> = hunk.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    let mut out = Vec::with_capacity(lines.len() - start + 1);
    if start > 0 {
        out.push(Line::styled(
            format!("  … {start} earlier hunk lines"),
            dim(),
        ));
    }
    for l in &lines[start..] {
        let style = if l.starts_with("@@") {
            Style::new().fg(Color::Cyan)
        } else if l.starts_with('+') {
            Style::new().fg(Color::Green)
        } else if l.starts_with('-') {
            Style::new().fg(Color::Red)
        } else {
            dim()
        };
        out.push(Line::styled(sanitize(l), style));
    }
    out
}

pub fn centered_rect(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let [_, mid_v, _] = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .areas(area);
    let [_, mid, _] = Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .areas(mid_v);
    mid
}
