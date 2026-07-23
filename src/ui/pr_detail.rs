use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, List, ListItem, Paragraph, Wrap};

use crate::app::{App, Pane};
use crate::github::types::{ReviewThread, ReviewVerdict, ThreadSort, TimelineItem, TimelineKind};

use super::{dim, hunk_lines, markdown_body, relative_age, sanitize};

const HUNK_TAIL_LINES: usize = 8;

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let Some(detail) = app.detail.as_ref() else {
        let title = app
            .current_pr
            .as_ref()
            .map(|p| p.to_string())
            .unwrap_or_default();
        let msg = if app.loading { "loading…" } else { "" };
        f.render_widget(
            Paragraph::new(msg)
                .style(dim())
                .block(Block::bordered().title(format!(" {title} "))),
            area,
        );
        return;
    };

    // Build everything owned first so the immutable borrow of app.detail ends
    // before we render with &mut list states.
    let title_line = Line::from(vec![
        Span::styled(
            format!("#{} {}", detail.number, detail.title),
            Style::new().add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        state_span(&detail.state, detail.is_draft),
    ]);
    let meta_line = Line::styled(
        format!(
            "by {} · {} ← {}{}",
            detail.author,
            detail.base_ref,
            detail.head_ref,
            if detail.timeline_truncated {
                " · (older timeline items omitted)"
            } else {
                ""
            }
        ),
        dim(),
    );
    let tabs_line = tabs_line(
        app.pane,
        app.thread_sort,
        detail.timeline.len(),
        detail.unresolved_count(),
        detail.threads.len(),
    );

    let (items, body, comment_line): (Vec<ListItem<'static>>, Text<'static>, Option<usize>) =
        match app.pane {
            Pane::Conversation => {
                let items = detail.timeline.iter().map(timeline_item).collect();
                let body = app
                    .timeline_state
                    .selected()
                    .and_then(|i| detail.timeline.get(i))
                    .map(timeline_body)
                    .unwrap_or_else(|| {
                        Text::styled("no conversation comments yet — press c to add one", dim())
                    });
                (items, body, None)
            }
            Pane::Threads => {
                let items = detail.threads.iter().map(thread_item).collect();
                let (body, comment_line) = app
                    .thread_state
                    .selected()
                    .and_then(|i| detail.threads.get(i))
                    .map(|t| {
                        let (body, line) = thread_body(t, app.comment_sel);
                        (body, Some(line))
                    })
                    .unwrap_or_else(|| (Text::styled("no review threads", dim()), None));
                (items, body, comment_line)
            }
        };

    let [head_area, tabs_area, list_area, body_area] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Percentage(35),
        Constraint::Min(3),
    ])
    .areas(area);

    f.render_widget(Paragraph::new(vec![title_line, meta_line]), head_area);
    f.render_widget(Paragraph::new(tabs_line), tabs_area);

    let list = List::new(items)
        .block(Block::bordered())
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");
    match app.pane {
        Pane::Conversation => f.render_stateful_widget(list, list_area, &mut app.timeline_state),
        Pane::Threads => f.render_stateful_widget(list, list_area, &mut app.thread_state),
    }

    if app.scroll_to_comment {
        app.scroll_to_comment = false;
        if let Some(idx) = comment_line {
            let inner_w = body_area.width.saturating_sub(2).max(1); // borders
            app.body_scroll = visual_row(&body, idx, inner_w);
        }
    }

    f.render_widget(
        Paragraph::new(body)
            .block(Block::bordered())
            .wrap(Wrap { trim: false })
            .scroll((app.body_scroll, 0)),
        body_area,
    );
}

/// Estimated visual row of logical line `idx` when wrapped to `width`.
/// Word-wrapping can occasionally use one more row than this estimate for
/// long lines broken at word boundaries; close enough for scroll targeting.
fn visual_row(text: &Text, idx: usize, width: u16) -> u16 {
    text.lines
        .iter()
        .take(idx)
        .map(|l| l.width().max(1).div_ceil(width as usize))
        .sum::<usize>()
        .min(u16::MAX as usize) as u16
}

fn state_span(state: &str, is_draft: bool) -> Span<'static> {
    if is_draft {
        return Span::styled("[DRAFT]", dim());
    }
    match state {
        "OPEN" => Span::styled("[OPEN]", Style::new().fg(Color::Green)),
        "MERGED" => Span::styled("[MERGED]", Style::new().fg(Color::Magenta)),
        "CLOSED" => Span::styled("[CLOSED]", Style::new().fg(Color::Red)),
        other => Span::raw(format!("[{other}]")),
    }
}

fn tabs_line(
    pane: Pane,
    sort: ThreadSort,
    timeline_len: usize,
    unresolved: usize,
    threads: usize,
) -> Line<'static> {
    let conv = format!(" [1] Conversation ({timeline_len}) ");
    let sort_label = match sort {
        ThreadSort::Position => "file",
        ThreadSort::Activity => "recent",
    };
    let thr = format!(" [2] Threads ({unresolved} open / {threads} · by {sort_label}) ");
    let active = Style::new()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
    match pane {
        Pane::Conversation => {
            Line::from(vec![Span::styled(conv, active), Span::styled(thr, dim())])
        }
        Pane::Threads => Line::from(vec![Span::styled(conv, dim()), Span::styled(thr, active)]),
    }
}

fn first_line(s: &str) -> String {
    let line = sanitize(s.lines().next().unwrap_or_default());
    let mut out: String = line.chars().take(80).collect();
    if out.len() < line.len() || s.lines().count() > 1 {
        out.push('…');
    }
    out
}

fn verdict_span(kind: TimelineKind) -> Span<'static> {
    match kind {
        TimelineKind::Comment => Span::styled("💬", Style::new()),
        TimelineKind::Review(ReviewVerdict::Approved) => {
            Span::styled("✔ approved", Style::new().fg(Color::Green))
        }
        TimelineKind::Review(ReviewVerdict::ChangesRequested) => {
            Span::styled("✖ changes requested", Style::new().fg(Color::Red))
        }
        TimelineKind::Review(ReviewVerdict::Commented) => {
            Span::styled("👁 reviewed", Style::new().fg(Color::Yellow))
        }
        TimelineKind::Review(ReviewVerdict::Dismissed) => Span::styled("∅ dismissed", dim()),
        TimelineKind::Review(ReviewVerdict::Other) => Span::styled("• review", dim()),
    }
}

fn timeline_item(item: &TimelineItem) -> ListItem<'static> {
    let spans = vec![
        verdict_span(item.kind),
        Span::raw(" "),
        Span::styled(item.author.clone(), Style::new().fg(Color::Cyan)),
        Span::styled(format!(" · {} · ", relative_age(item.created_at)), dim()),
        Span::raw(first_line(&item.body)),
    ];
    ListItem::new(Line::from(spans))
}

fn timeline_body(item: &TimelineItem) -> Text<'static> {
    let mut lines = vec![
        Line::from(vec![
            verdict_span(item.kind),
            Span::raw(" "),
            Span::styled(
                item.author.clone(),
                Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" · {}", relative_age(item.created_at)), dim()),
        ]),
        Line::default(),
    ];
    lines.extend(markdown_body(&item.body));
    Text::from(lines)
}

fn thread_item(thread: &ReviewThread) -> ListItem<'static> {
    let status = if thread.is_resolved {
        Span::styled("✓", Style::new().fg(Color::Green))
    } else {
        Span::styled("●", Style::new().fg(Color::Red))
    };
    let loc = match thread.line {
        Some(line) => format!("{}:{}", thread.path, line),
        None => thread.path.clone(),
    };
    let mut spans = vec![
        status,
        Span::raw(" "),
        Span::raw(loc),
        Span::styled(
            format!(
                " · {} · 💬{} · {}",
                thread
                    .comments
                    .first()
                    .map(|c| c.author.clone())
                    .unwrap_or_default(),
                thread.comments.len() as u64 + thread.hidden_count,
                relative_age(thread.last_activity),
            ),
            dim(),
        ),
    ];
    if thread.is_outdated {
        spans.push(Span::styled(" (outdated)", dim()));
    }
    ListItem::new(Line::from(spans))
}

fn thread_body(thread: &ReviewThread, sel: usize) -> (Text<'static>, usize) {
    let sel = sel.min(thread.comments.len().saturating_sub(1));
    let mut header_idx = 0;
    let mut lines: Vec<Line<'static>> = Vec::new();
    let loc = match thread.line {
        Some(line) => format!("{}:{}", thread.path, line),
        None => thread.path.clone(),
    };
    let status = if thread.is_resolved {
        Span::styled(" · resolved ✓", Style::new().fg(Color::Green))
    } else {
        Span::styled(" · unresolved ●", Style::new().fg(Color::Red))
    };
    lines.push(Line::from(vec![
        Span::styled(loc, Style::new().add_modifier(Modifier::BOLD)),
        status,
    ]));
    if !thread.diff_hunk.is_empty() {
        lines.extend(hunk_lines(&thread.diff_hunk, HUNK_TAIL_LINES));
    }
    if thread.hidden_count > 0 {
        lines.push(Line::styled(
            format!("(+{} earlier comments not loaded)", thread.hidden_count),
            dim(),
        ));
    }
    for (i, comment) in thread.comments.iter().enumerate() {
        lines.push(Line::default());
        let mut header = vec![
            Span::styled(
                comment.author.clone(),
                Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" · {}", relative_age(comment.created_at)), dim()),
        ];
        if i == sel {
            header_idx = lines.len();
            header.insert(0, Span::raw("▶ "));
            lines.push(Line::from(header).style(Style::new().add_modifier(Modifier::REVERSED)));
        } else {
            lines.push(Line::from(header));
        }
        lines.extend(markdown_body(&comment.body));
    }
    (Text::from(lines), header_idx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::types::ThreadComment;

    #[test]
    fn visual_row_counts_wrapped_lines() {
        let text = Text::from(vec![
            Line::raw("1234567890"),      // 10 wide → 1 row at width 10
            Line::raw(""),                // empty → still 1 row
            Line::raw("123456789012345"), // 15 wide → 2 rows at width 10
            Line::raw("target"),
        ]);
        assert_eq!(visual_row(&text, 0, 10), 0);
        assert_eq!(visual_row(&text, 1, 10), 1);
        assert_eq!(visual_row(&text, 2, 10), 2);
        assert_eq!(visual_row(&text, 3, 10), 4);
    }

    fn thread(n: usize) -> ReviewThread {
        ReviewThread {
            id: "t".to_string(),
            is_resolved: false,
            is_outdated: false,
            path: "src/lib.rs".to_string(),
            line: Some(1),
            reply_to_db_id: None,
            diff_hunk: String::new(),
            comments: (0..n)
                .map(|i| ThreadComment {
                    author: format!("user{i}"),
                    body: format!("comment {i}"),
                    created_at: jiff::Timestamp::UNIX_EPOCH,
                })
                .collect(),
            hidden_count: 0,
            last_activity: jiff::Timestamp::UNIX_EPOCH,
        }
    }

    #[test]
    fn thread_body_marks_selected_comment() {
        let (text, idx) = thread_body(&thread(3), 1);
        let line: String = text.lines[idx]
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(line.starts_with("▶ user1"), "got: {line}");
    }

    #[test]
    fn thread_body_clamps_out_of_range_selection() {
        let (text, idx) = thread_body(&thread(2), 99);
        let line: String = text.lines[idx]
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(line.starts_with("▶ user1"), "got: {line}");
    }

    #[test]
    fn thread_body_empty_thread_returns_zero_offset() {
        let (_, idx) = thread_body(&thread(0), 0);
        assert_eq!(idx, 0);
    }
}
