use crossterm::event::{Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::widgets::ListState;
use tokio::sync::mpsc::UnboundedSender;
use tui_textarea::TextArea;

use crate::cli::PrRef;
use crate::event::{ApiEvent, ApiResult, AppEvent};
use crate::github::GhClient;
use crate::github::types::{PrDetail, PrSummary, ReviewThread, ThreadSort};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    List,
    Detail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Conversation,
    Threads,
}

pub enum ComposeTarget {
    Conversation,
    ThreadReply { comment_db_id: u64, path: String },
}

pub struct Compose {
    pub target: ComposeTarget,
    pub textarea: TextArea<'static>,
    pub sending: bool,
    pub confirm_discard: bool,
}

pub struct App {
    client: GhClient,
    tx: UnboundedSender<ApiEvent>,
    pub viewer: String,
    pub screen: Screen,

    pub prs: Vec<PrSummary>,
    pub list_loaded: bool,
    pub list_state: ListState,

    pub current_pr: Option<PrRef>,
    pub detail: Option<PrDetail>,
    pub pane: Pane,
    pub timeline_state: ListState,
    pub thread_state: ListState,
    pub thread_sort: ThreadSort,
    pub body_scroll: u16,
    pub comment_sel: usize,
    pub scroll_to_comment: bool,
    clipboard: Option<arboard::Clipboard>,

    pub compose: Option<Compose>,

    pub loading: bool,
    pub spinner: usize,
    pub error: Option<String>,
    pub status: Option<String>,

    pub update_available: Option<String>,
    pub update_requested: bool,

    req_seq: u64,
    direct: bool,
    pub should_quit: bool,
}

impl App {
    pub fn new(
        client: GhClient,
        tx: UnboundedSender<ApiEvent>,
        viewer: String,
        direct: Option<PrRef>,
    ) -> Self {
        let mut app = App {
            client,
            tx,
            viewer,
            screen: Screen::List,
            prs: Vec::new(),
            list_loaded: false,
            list_state: ListState::default(),
            current_pr: None,
            detail: None,
            pane: Pane::Conversation,
            timeline_state: ListState::default(),
            thread_state: ListState::default(),
            thread_sort: ThreadSort::default(),
            body_scroll: 0,
            comment_sel: 0,
            scroll_to_comment: false,
            clipboard: None,
            compose: None,
            loading: false,
            spinner: 0,
            error: None,
            status: None,
            update_available: None,
            update_requested: false,
            req_seq: 0,
            direct: direct.is_some(),
            should_quit: false,
        };
        match direct {
            Some(pr) => app.open_pr(pr),
            None => app.load_list(),
        }
        app.check_for_update();
        app
    }

    fn check_for_update(&self) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            if let Some(version) = crate::update::check_for_update(&client).await {
                let _ = tx.send(ApiEvent {
                    req: None,
                    payload: ApiResult::UpdateAvailable(version),
                });
            }
        });
    }

    fn next_req(&mut self) -> u64 {
        self.req_seq += 1;
        self.req_seq
    }

    pub fn load_list(&mut self) {
        let req = self.next_req();
        self.loading = true;
        self.error = None;
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let res = client.search_involved_prs().await;
            let _ = tx.send(ApiEvent {
                req: Some(req),
                payload: ApiResult::PrList(res),
            });
        });
    }

    pub fn open_pr(&mut self, pr: PrRef) {
        self.screen = Screen::Detail;
        self.detail = None;
        self.pane = Pane::Conversation;
        self.timeline_state = ListState::default();
        self.thread_state = ListState::default();
        self.reset_body_view();
        self.current_pr = Some(pr);
        self.refresh_detail();
    }

    pub fn refresh_detail(&mut self) {
        let Some(pr) = self.current_pr.clone() else {
            return;
        };
        let req = self.next_req();
        self.loading = true;
        self.error = None;
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let res = client.fetch_pr(&pr).await;
            let _ = tx.send(ApiEvent {
                req: Some(req),
                payload: ApiResult::PrDetail(res),
            });
        });
    }

    pub fn handle_event(&mut self, ev: AppEvent) {
        match ev {
            AppEvent::Tick => self.spinner = self.spinner.wrapping_add(1),
            AppEvent::Api(msg) => self.handle_api(msg),
            AppEvent::Term(TermEvent::Key(key)) if key.kind == KeyEventKind::Press => {
                self.handle_key(key)
            }
            _ => {}
        }
    }

    fn handle_api(&mut self, msg: ApiEvent) {
        if let Some(req) = msg.req
            && req != self.req_seq
        {
            return; // stale fetch, superseded by a newer request
        }
        match msg.payload {
            ApiResult::PrList(Ok(prs)) => {
                self.loading = false;
                self.list_loaded = true;
                let sel = self.list_state.selected().unwrap_or(0);
                self.list_state.select(if prs.is_empty() {
                    None
                } else {
                    Some(sel.min(prs.len() - 1))
                });
                self.prs = prs;
            }
            ApiResult::PrList(Err(e)) => {
                self.loading = false;
                self.error = Some(e.to_string());
            }
            ApiResult::PrDetail(Ok(mut detail)) => {
                self.loading = false;
                detail.sort_threads(self.thread_sort);
                Self::clamp(&mut self.timeline_state, detail.timeline.len());
                Self::clamp(&mut self.thread_state, detail.threads.len());
                self.detail = Some(detail);
            }
            ApiResult::PrDetail(Err(e)) => {
                self.loading = false;
                self.error = Some(e.to_string());
            }
            ApiResult::Posted(Ok(())) => {
                self.compose = None;
                self.status = Some("posted ✓".to_string());
                self.refresh_detail();
            }
            ApiResult::Posted(Err(e)) => {
                if let Some(c) = &mut self.compose {
                    c.sending = false;
                }
                self.error = Some(e.to_string());
            }
            ApiResult::ThreadResolved(Ok((id, resolved))) => {
                if let Some(detail) = &mut self.detail
                    && let Some(t) = detail.threads.iter_mut().find(|t| t.id == id)
                {
                    t.is_resolved = resolved;
                }
                self.status = Some(if resolved {
                    "thread resolved ✓".to_string()
                } else {
                    "thread unresolved".to_string()
                });
            }
            ApiResult::ThreadResolved(Err(e)) => {
                self.error = Some(e.to_string());
            }
            ApiResult::UpdateAvailable(version) => {
                self.update_available = Some(version);
            }
        }
    }

    fn clamp(state: &mut ListState, len: usize) {
        if len == 0 {
            state.select(None);
        } else {
            state.select(Some(state.selected().unwrap_or(0).min(len - 1)));
        }
    }

    fn move_sel(state: &mut ListState, len: usize, delta: i64) {
        if len == 0 {
            state.select(None);
            return;
        }
        let cur = state.selected().unwrap_or(0) as i64;
        let next = (cur + delta).clamp(0, len as i64 - 1) as usize;
        state.select(Some(next));
    }

    pub fn selected_thread(&self) -> Option<&ReviewThread> {
        let detail = self.detail.as_ref()?;
        detail.threads.get(self.thread_state.selected()?)
    }

    fn reset_body_view(&mut self) {
        self.body_scroll = 0;
        self.comment_sel = 0;
        self.scroll_to_comment = false;
    }

    fn handle_key(&mut self, key: KeyEvent) {
        // Any keypress dismisses transient messages.
        self.error = None;
        self.status = None;

        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
            return;
        }
        if self.compose.is_some() {
            self.handle_key_compose(key);
            return;
        }
        if key.code == KeyCode::Char('U') && self.update_available.is_some() {
            self.update_requested = true;
            self.should_quit = true;
            return;
        }
        match self.screen {
            Screen::List => self.handle_key_list(key),
            Screen::Detail => self.handle_key_detail(key),
        }
    }

    fn handle_key_list(&mut self, key: KeyEvent) {
        let len = self.prs.len();
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('j') | KeyCode::Down => Self::move_sel(&mut self.list_state, len, 1),
            KeyCode::Char('k') | KeyCode::Up => Self::move_sel(&mut self.list_state, len, -1),
            KeyCode::Char('g') | KeyCode::Home => {
                Self::move_sel(&mut self.list_state, len, i64::MIN / 2)
            }
            KeyCode::Char('G') | KeyCode::End => {
                Self::move_sel(&mut self.list_state, len, i64::MAX / 2)
            }
            KeyCode::Enter => {
                let pr = self
                    .list_state
                    .selected()
                    .and_then(|i| self.prs.get(i))
                    .and_then(|p| p.pr_ref());
                if let Some(pr) = pr {
                    self.open_pr(pr);
                }
            }
            KeyCode::Char('r') => self.load_list(),
            KeyCode::Char('o') => {
                let url = self
                    .list_state
                    .selected()
                    .and_then(|i| self.prs.get(i))
                    .and_then(|p| p.pr_ref())
                    .map(|p| p.url());
                if let Some(url) = url {
                    let _ = open::that(url);
                }
            }
            _ => {}
        }
    }

    fn handle_key_detail(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                if self.direct {
                    self.should_quit = true;
                } else {
                    self.screen = Screen::List;
                    if !self.list_loaded {
                        self.load_list();
                    }
                }
            }
            KeyCode::Tab => {
                self.pane = match self.pane {
                    Pane::Conversation => Pane::Threads,
                    Pane::Threads => Pane::Conversation,
                };
                self.reset_body_view();
            }
            KeyCode::Char('1') => {
                self.pane = Pane::Conversation;
                self.reset_body_view();
            }
            KeyCode::Char('2') => {
                self.pane = Pane::Threads;
                self.reset_body_view();
            }
            KeyCode::Char('j') | KeyCode::Down => self.move_detail_sel(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_detail_sel(-1),
            KeyCode::Char('g') | KeyCode::Home => self.move_detail_sel(i64::MIN / 2),
            KeyCode::Char('G') | KeyCode::End => self.move_detail_sel(i64::MAX / 2),
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.body_scroll = self.body_scroll.saturating_add(5);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.body_scroll = self.body_scroll.saturating_sub(5);
            }
            KeyCode::PageDown => self.body_scroll = self.body_scroll.saturating_add(5),
            KeyCode::PageUp => self.body_scroll = self.body_scroll.saturating_sub(5),
            KeyCode::Char('J') => self.move_comment_sel(1),
            KeyCode::Char('K') => self.move_comment_sel(-1),
            KeyCode::Char('y') => self.copy_selected_comment(),
            KeyCode::Char('s') => self.toggle_thread_sort(),
            KeyCode::Char(']') => self.jump_unresolved(1),
            KeyCode::Char('[') => self.jump_unresolved(-1),
            KeyCode::Char('c') => self.start_compose_conversation(),
            KeyCode::Char('a') => self.start_compose_reply(),
            KeyCode::Char('x') => self.toggle_resolve(),
            KeyCode::Char('r') => self.refresh_detail(),
            KeyCode::Char('o') => {
                if let Some(pr) = &self.current_pr {
                    let _ = open::that(pr.url());
                }
            }
            _ => {}
        }
    }

    fn move_detail_sel(&mut self, delta: i64) {
        let Some(detail) = &self.detail else { return };
        match self.pane {
            Pane::Conversation => {
                Self::move_sel(&mut self.timeline_state, detail.timeline.len(), delta)
            }
            Pane::Threads => Self::move_sel(&mut self.thread_state, detail.threads.len(), delta),
        }
        self.reset_body_view();
    }

    fn move_comment_sel(&mut self, delta: i64) {
        self.pane = Pane::Threads;
        let Some(thread) = self.selected_thread() else {
            return;
        };
        let len = thread.comments.len();
        if len == 0 {
            return;
        }
        let cur = self.comment_sel.min(len - 1) as i64;
        self.comment_sel = (cur + delta).clamp(0, len as i64 - 1) as usize;
        self.scroll_to_comment = true;
    }

    fn copy_selected_comment(&mut self) {
        self.pane = Pane::Threads;
        let Some(thread) = self.selected_thread() else {
            self.status = Some("no thread selected".to_string());
            return;
        };
        let Some(comment) = thread.comments.get(
            self.comment_sel
                .min(thread.comments.len().saturating_sub(1)),
        ) else {
            return;
        };
        let (body, author) = (comment.body.clone(), comment.author.clone());
        if self.clipboard.is_none() {
            match arboard::Clipboard::new() {
                Ok(cb) => self.clipboard = Some(cb),
                Err(e) => {
                    self.error = Some(format!("clipboard unavailable: {e}"));
                    return;
                }
            }
        }
        match self.clipboard.as_mut().unwrap().set_text(body) {
            Ok(()) => self.status = Some(format!("copied comment by {author} ✓")),
            Err(e) => self.error = Some(format!("copy failed: {e}")),
        }
    }

    fn toggle_thread_sort(&mut self) {
        self.thread_sort = match self.thread_sort {
            ThreadSort::Position => ThreadSort::Activity,
            ThreadSort::Activity => ThreadSort::Position,
        };
        self.pane = Pane::Threads;
        self.reset_body_view();
        if let Some(detail) = &mut self.detail {
            detail.sort_threads(self.thread_sort);
            Self::clamp(&mut self.thread_state, detail.threads.len());
            self.thread_state.select(if detail.threads.is_empty() {
                None
            } else {
                Some(0)
            });
        }
        self.status = Some(match self.thread_sort {
            ThreadSort::Position => "threads sorted by file (unresolved first)".to_string(),
            ThreadSort::Activity => "threads sorted by latest comment".to_string(),
        });
    }

    fn jump_unresolved(&mut self, dir: i64) {
        let Some(detail) = &self.detail else { return };
        let unresolved: Vec<usize> = detail
            .threads
            .iter()
            .enumerate()
            .filter(|(_, t)| !t.is_resolved)
            .map(|(i, _)| i)
            .collect();
        if unresolved.is_empty() {
            self.status = Some("no unresolved threads".to_string());
            return;
        }
        self.pane = Pane::Threads;
        let cur = self.thread_state.selected().unwrap_or(0);
        let next = if dir > 0 {
            unresolved
                .iter()
                .find(|&&i| i > cur)
                .or_else(|| unresolved.first())
        } else {
            unresolved
                .iter()
                .rev()
                .find(|&&i| i < cur)
                .or_else(|| unresolved.last())
        };
        self.thread_state.select(next.copied());
        self.reset_body_view();
    }

    fn start_compose_conversation(&mut self) {
        if self.detail.is_none() {
            return;
        }
        self.compose = Some(Compose {
            target: ComposeTarget::Conversation,
            textarea: TextArea::default(),
            sending: false,
            confirm_discard: false,
        });
    }

    fn start_compose_reply(&mut self) {
        self.pane = Pane::Threads;
        let Some(thread) = self.selected_thread() else {
            self.status = Some("no thread selected".to_string());
            return;
        };
        let Some(comment_db_id) = thread.reply_to_db_id else {
            self.error = Some("cannot reply: thread has no root comment id".to_string());
            return;
        };
        let path = thread.path.clone();
        self.compose = Some(Compose {
            target: ComposeTarget::ThreadReply {
                comment_db_id,
                path,
            },
            textarea: TextArea::default(),
            sending: false,
            confirm_discard: false,
        });
    }

    fn toggle_resolve(&mut self) {
        self.pane = Pane::Threads;
        let Some(thread) = self.selected_thread() else {
            return;
        };
        let id = thread.id.clone();
        let target = !thread.is_resolved;
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let res = client.set_thread_resolved(&id, target).await;
            let _ = tx.send(ApiEvent {
                req: None,
                payload: ApiResult::ThreadResolved(res),
            });
        });
    }

    fn handle_key_compose(&mut self, key: KeyEvent) {
        let Some(compose) = &mut self.compose else {
            return;
        };
        if compose.sending {
            return;
        }
        match key.code {
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.submit_compose();
            }
            KeyCode::Esc => {
                let empty = compose.textarea.lines().join("").trim().is_empty();
                if empty || compose.confirm_discard {
                    self.compose = None;
                } else {
                    compose.confirm_discard = true;
                }
            }
            _ => {
                compose.confirm_discard = false;
                compose.textarea.input(key);
            }
        }
    }

    fn submit_compose(&mut self) {
        let Some(detail) = &self.detail else { return };
        let pr_id = detail.id.clone();
        let Some(compose) = &mut self.compose else {
            return;
        };
        let body = compose.textarea.lines().join("\n").trim().to_string();
        if body.is_empty() {
            return;
        }
        compose.sending = true;
        let client = self.client.clone();
        let tx = self.tx.clone();
        match &compose.target {
            ComposeTarget::Conversation => {
                tokio::spawn(async move {
                    let res = client.add_comment(&pr_id, &body).await;
                    let _ = tx.send(ApiEvent {
                        req: None,
                        payload: ApiResult::Posted(res),
                    });
                });
            }
            ComposeTarget::ThreadReply { comment_db_id, .. } => {
                let comment_db_id = *comment_db_id;
                let Some(pr) = self.current_pr.clone() else {
                    return;
                };
                tokio::spawn(async move {
                    let res = client.reply_to_comment(&pr, comment_db_id, &body).await;
                    let _ = tx.send(ApiEvent {
                        req: None,
                        payload: ApiResult::Posted(res),
                    });
                });
            }
        }
    }
}
