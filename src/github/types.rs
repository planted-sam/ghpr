//! Clean domain types used by the UI, plus conversions from the raw GraphQL
//! structs in [`super::queries`]. This layer is fixture-testable.

use jiff::Timestamp;

use crate::cli::PrRef;

use super::queries::{RawActor, RawPrSummary, RawPullRequest, RawThread, RawTimelineNode};

#[derive(Debug, Clone)]
pub struct PrSummary {
    /// "owner/repo"
    pub repo: String,
    pub number: u64,
    pub title: String,
    pub is_draft: bool,
    pub updated_at: Timestamp,
    pub author: String,
    pub review_decision: Option<String>,
    pub comment_count: u64,
}

impl PrSummary {
    pub fn pr_ref(&self) -> Option<PrRef> {
        let (owner, repo) = self.repo.split_once('/')?;
        Some(PrRef {
            owner: owner.to_string(),
            repo: repo.to_string(),
            number: self.number,
        })
    }
}

#[derive(Debug)]
pub struct PrDetail {
    /// GraphQL node id — the `subjectId` for addComment.
    pub id: String,
    pub number: u64,
    pub title: String,
    pub state: String,
    pub is_draft: bool,
    pub author: String,
    pub base_ref: String,
    pub head_ref: String,
    pub timeline: Vec<TimelineItem>,
    pub timeline_truncated: bool,
    pub threads: Vec<ReviewThread>,
}

/// How review threads are ordered in the Threads pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThreadSort {
    /// Unresolved first, then by file path and line.
    #[default]
    Position,
    /// Most recent comment first.
    Activity,
}

impl PrDetail {
    pub fn unresolved_count(&self) -> usize {
        self.threads.iter().filter(|t| !t.is_resolved).count()
    }

    pub fn sort_threads(&mut self, sort: ThreadSort) {
        match sort {
            ThreadSort::Position => self.threads.sort_by(|a, b| {
                (a.is_resolved, &a.path, a.line).cmp(&(b.is_resolved, &b.path, b.line))
            }),
            ThreadSort::Activity => self
                .threads
                .sort_by_key(|t| std::cmp::Reverse(t.last_activity)),
        }
    }
}

#[derive(Debug)]
pub struct TimelineItem {
    pub author: String,
    pub body: String,
    pub created_at: Timestamp,
    pub kind: TimelineKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineKind {
    Comment,
    Review(ReviewVerdict),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewVerdict {
    Approved,
    ChangesRequested,
    Commented,
    Dismissed,
    Other,
}

impl ReviewVerdict {
    fn from_state(state: &str) -> Self {
        match state {
            "APPROVED" => Self::Approved,
            "CHANGES_REQUESTED" => Self::ChangesRequested,
            "COMMENTED" => Self::Commented,
            "DISMISSED" => Self::Dismissed,
            _ => Self::Other,
        }
    }
}

#[derive(Debug)]
pub struct ReviewThread {
    /// GraphQL node id — the `threadId` for resolve/unresolve.
    pub id: String,
    pub is_resolved: bool,
    pub is_outdated: bool,
    pub path: String,
    pub line: Option<u64>,
    /// REST id of the thread's root comment — reply target.
    pub reply_to_db_id: Option<u64>,
    /// Diff hunk context from the root comment.
    pub diff_hunk: String,
    pub comments: Vec<ThreadComment>,
    /// Comments beyond the first page (rendered as "+N earlier").
    pub hidden_count: u64,
    pub last_activity: Timestamp,
}

#[derive(Debug)]
pub struct ThreadComment {
    pub author: String,
    pub body: String,
    pub created_at: Timestamp,
}

fn login(actor: Option<RawActor>) -> String {
    actor
        .map(|a| a.login)
        .unwrap_or_else(|| "ghost".to_string())
}

fn login_ref(actor: &Option<RawActor>) -> String {
    actor
        .as_ref()
        .map(|a| a.login.clone())
        .unwrap_or_else(|| "ghost".to_string())
}

fn parse_ts(s: &str) -> Timestamp {
    s.parse().unwrap_or(Timestamp::UNIX_EPOCH)
}

impl From<RawPrSummary> for PrSummary {
    fn from(raw: RawPrSummary) -> Self {
        PrSummary {
            repo: raw.repository.name_with_owner,
            number: raw.number,
            title: raw.title,
            is_draft: raw.is_draft,
            updated_at: parse_ts(&raw.updated_at),
            author: login(raw.author),
            review_decision: raw.review_decision,
            comment_count: raw.comments.total_count,
        }
    }
}

impl From<RawThread> for ReviewThread {
    fn from(raw: RawThread) -> Self {
        let hidden_count = raw
            .comments
            .total_count
            .saturating_sub(raw.comments.nodes.len() as u64);
        let reply_to_db_id = raw.comments.nodes.first().and_then(|c| c.database_id);
        let diff_hunk = raw
            .comments
            .nodes
            .first()
            .map(|c| c.diff_hunk.clone())
            .unwrap_or_default();
        let comments: Vec<ThreadComment> = raw
            .comments
            .nodes
            .into_iter()
            .map(|c| ThreadComment {
                author: login(c.author),
                body: c.body,
                created_at: parse_ts(&c.created_at),
            })
            .collect();
        let last_activity = comments
            .last()
            .map(|c| c.created_at)
            .unwrap_or(Timestamp::UNIX_EPOCH);
        ReviewThread {
            id: raw.id,
            is_resolved: raw.is_resolved,
            is_outdated: raw.is_outdated,
            path: raw.path,
            line: raw.line,
            reply_to_db_id,
            diff_hunk,
            comments,
            hidden_count,
            last_activity,
        }
    }
}

impl PrDetail {
    /// Build the domain view: filter noise out of the timeline (empty-body
    /// COMMENTED review stubs that only carry inline comments) and sort
    /// threads unresolved-first, then by file position.
    pub fn from_raw(raw: RawPullRequest, raw_threads: Vec<RawThread>) -> Self {
        let timeline_truncated = raw.timeline_items.page_info.has_next_page;
        // The PR description leads the conversation, like the web UI.
        let description = TimelineItem {
            author: login_ref(&raw.author),
            body: if raw.body.trim().is_empty() {
                "(no description)".to_string()
            } else {
                raw.body.clone()
            },
            created_at: parse_ts(&raw.created_at),
            kind: TimelineKind::Comment,
        };
        let timeline: Vec<TimelineItem> = std::iter::once(description)
            .chain(
                raw.timeline_items
                    .nodes
                    .into_iter()
                    .filter_map(|node| match node {
                        RawTimelineNode::IssueComment(c) => Some(TimelineItem {
                            author: login(c.author),
                            body: c.body,
                            created_at: parse_ts(&c.created_at),
                            kind: TimelineKind::Comment,
                        }),
                        RawTimelineNode::PullRequestReview(r) => {
                            let verdict = ReviewVerdict::from_state(&r.state);
                            if r.body.trim().is_empty() && verdict == ReviewVerdict::Commented {
                                return None;
                            }
                            Some(TimelineItem {
                                author: login(r.author),
                                body: r.body,
                                created_at: parse_ts(&r.created_at),
                                kind: TimelineKind::Review(verdict),
                            })
                        }
                        RawTimelineNode::Other => None,
                    }),
            )
            .collect();

        let threads: Vec<ReviewThread> = raw_threads.into_iter().map(Into::into).collect();

        let mut detail = PrDetail {
            id: raw.id,
            number: raw.number,
            title: raw.title,
            state: raw.state,
            is_draft: raw.is_draft,
            author: login(raw.author),
            base_ref: raw.base_ref_name,
            head_ref: raw.head_ref_name,
            timeline,
            timeline_truncated,
            threads,
        };
        detail.sort_threads(ThreadSort::default());
        detail
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::queries::{PrDetailData, SearchData};

    fn data<T: serde::de::DeserializeOwned>(fixture: &str) -> T {
        let envelope: serde_json::Value = serde_json::from_str(fixture).unwrap();
        serde_json::from_value(envelope["data"].clone()).unwrap()
    }

    #[test]
    fn deserializes_search_fixture() {
        let data: SearchData = data(include_str!("../../tests/fixtures/search_prs.json"));
        let prs: Vec<PrSummary> = data.search.nodes.into_iter().map(Into::into).collect();
        assert!(!prs.is_empty());
        let first = &prs[0];
        assert_eq!(first.repo, "ratatui/ratatui");
        assert!(first.number > 0);
        assert!(!first.title.is_empty());
        assert!(first.updated_at > jiff::Timestamp::UNIX_EPOCH);
        let pr_ref = first.pr_ref().unwrap();
        assert_eq!(pr_ref.owner, "ratatui");
        assert_eq!(pr_ref.repo, "ratatui");
    }

    #[test]
    fn converts_pr_detail_fixture() {
        let data: PrDetailData = data(include_str!("../../tests/fixtures/pr_detail.json"));
        let mut raw = data.repository.unwrap().pull_request.unwrap();
        let raw_threads = std::mem::take(&mut raw.review_threads.nodes);
        let detail = PrDetail::from_raw(raw, raw_threads);

        // Fixture: ratatui/ratatui#2424 — 16 threads (3 unresolved), 11 raw
        // timeline items.
        assert_eq!(detail.number, 2424);
        assert_eq!(detail.threads.len(), 16);
        assert_eq!(detail.unresolved_count(), 3);

        // Threads sort unresolved-first.
        let first_resolved = detail.threads.iter().position(|t| t.is_resolved).unwrap();
        assert!(
            detail.threads[..first_resolved]
                .iter()
                .all(|t| !t.is_resolved),
            "unresolved threads must sort before resolved ones"
        );
        assert!(
            detail.threads[first_resolved..]
                .iter()
                .all(|t| t.is_resolved)
        );

        // Every thread carries a reply target and comments.
        for t in &detail.threads {
            assert!(
                t.reply_to_db_id.is_some(),
                "thread {} missing databaseId",
                t.id
            );
            assert!(!t.comments.is_empty());
            assert!(t.last_activity > jiff::Timestamp::UNIX_EPOCH);
        }

        // PR description leads the timeline.
        let first = &detail.timeline[0];
        assert_eq!(first.kind, TimelineKind::Comment);
        assert_eq!(first.author, detail.author);

        // Empty-body COMMENTED review stubs are filtered out.
        assert!(detail.timeline.iter().all(|item| {
            item.kind != TimelineKind::Review(ReviewVerdict::Commented)
                || !item.body.trim().is_empty()
        }));
    }

    #[test]
    fn sorts_threads_by_activity() {
        let data: PrDetailData = data(include_str!("../../tests/fixtures/pr_detail.json"));
        let mut raw = data.repository.unwrap().pull_request.unwrap();
        let raw_threads = std::mem::take(&mut raw.review_threads.nodes);
        let mut detail = PrDetail::from_raw(raw, raw_threads);

        detail.sort_threads(ThreadSort::Activity);
        assert!(
            detail
                .threads
                .windows(2)
                .all(|w| w[0].last_activity >= w[1].last_activity),
            "activity sort must be newest-first"
        );

        detail.sort_threads(ThreadSort::Position);
        let first_resolved = detail.threads.iter().position(|t| t.is_resolved).unwrap();
        assert!(
            detail.threads[..first_resolved]
                .iter()
                .all(|t| !t.is_resolved)
        );
    }
}
