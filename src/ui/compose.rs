use ratatui::Frame;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Clear};

use crate::app::{App, ComposeTarget};

use super::centered_rect;

pub fn draw(f: &mut Frame, app: &mut App) {
    let Some(compose) = app.compose.as_mut() else {
        return;
    };
    let area = centered_rect(f.area(), 70, 50);

    let title = match &compose.target {
        ComposeTarget::Conversation => " new comment ".to_string(),
        ComposeTarget::ThreadReply { path, .. } => format!(" reply · {path} "),
    };
    let hint = if compose.sending {
        " sending… "
    } else if compose.confirm_discard {
        " unsaved text — Esc again to discard "
    } else {
        " Ctrl-S send · Esc cancel "
    };
    let border = if compose.confirm_discard {
        Style::new().fg(Color::Yellow)
    } else {
        Style::new().fg(Color::Blue)
    };
    compose.textarea.set_block(
        Block::bordered()
            .border_style(border)
            .title(title)
            .title_bottom(hint),
    );

    f.render_widget(Clear, area);
    f.render_widget(&compose.textarea, area);
}
