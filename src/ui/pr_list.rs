use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, Paragraph};

use crate::app::App;

use super::{dim, relative_age};

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::bordered().title(format!(
        " open PRs involving @{} ({}) ",
        app.viewer,
        app.prs.len()
    ));

    if app.prs.is_empty() {
        let msg = if app.loading {
            "fetching PRs…"
        } else if app.list_loaded {
            "no open PRs involve you — press r to refresh"
        } else {
            ""
        };
        f.render_widget(Paragraph::new(msg).style(dim()).block(block), area);
        return;
    }

    let items: Vec<ListItem> = app.prs.iter().map(item).collect();
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▶ ");
    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn item(pr: &crate::github::types::PrSummary) -> ListItem<'static> {
    let mut spans = vec![
        Span::styled(
            format!("{:<40}", format!("{}#{}", pr.repo, pr.number)),
            Style::new().fg(Color::Cyan),
        ),
        Span::raw(" "),
    ];
    if pr.is_draft {
        spans.push(Span::styled("[draft] ", dim()));
    }
    spans.push(Span::raw(pr.title.clone()));
    spans.push(Span::styled("  — ", dim()));
    match pr.review_decision.as_deref() {
        Some("APPROVED") => {
            spans.push(Span::styled("✓ approved ", Style::new().fg(Color::Green)))
        }
        Some("CHANGES_REQUESTED") => {
            spans.push(Span::styled("± changes ", Style::new().fg(Color::Red)))
        }
        Some("REVIEW_REQUIRED") => {
            spans.push(Span::styled("• review ", Style::new().fg(Color::Yellow)))
        }
        _ => {}
    }
    spans.push(Span::styled(
        format!(
            "{} · {} · 💬{}",
            pr.author,
            relative_age(pr.updated_at),
            pr.comment_count
        ),
        dim(),
    ));
    ListItem::new(Line::from(spans))
}
