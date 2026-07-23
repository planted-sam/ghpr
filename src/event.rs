use crossterm::event::Event as TermEvent;

use crate::github::GhError;
use crate::github::types::{PrDetail, PrSummary};

pub enum AppEvent {
    Term(TermEvent),
    Tick,
    Api(ApiEvent),
}

/// A completed API call. `req` is Some(id) for fetches (stale results are
/// dropped when a newer request superseded them) and None for mutations,
/// which always apply.
pub struct ApiEvent {
    pub req: Option<u64>,
    pub payload: ApiResult,
}

pub enum ApiResult {
    PrList(Result<Vec<PrSummary>, GhError>),
    PrDetail(Result<PrDetail, GhError>),
    Posted(Result<(), GhError>),
    ThreadResolved(Result<(String, bool), GhError>),
    /// A newer release exists (version without the leading "v").
    UpdateAvailable(String),
}
