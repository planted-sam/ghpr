//! GitHub API client: one GraphQL helper plus typed methods for everything
//! the app needs. The single REST call (review-thread replies) is deliberate:
//! the GraphQL reply mutation can attach replies to a pending review,
//! invisible to others until submitted — the REST endpoint posts immediately.

pub mod queries;
pub mod types;

use std::sync::Arc;

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::cli::PrRef;

use queries::{
    ADD_COMMENT_MUTATION, PR_DETAIL_QUERY, PrDetailData, RESOLVE_THREAD_MUTATION, RawPullRequest,
    RawThread, ResolveThreadData, SEARCH_PRS_QUERY, SearchData, UNRESOLVE_THREAD_MUTATION,
    UnresolveThreadData, VIEWER_QUERY, ViewerData,
};
use types::{PrDetail, PrSummary};

const GRAPHQL_URL: &str = "https://api.github.com/graphql";
const SEARCH_QUERY_STRING: &str = "is:pr is:open involves:@me sort:updated-desc";
const SEARCH_PAGE_SIZE: u32 = 50;

#[derive(Debug, thiserror::Error)]
pub enum GhError {
    #[error("network: {0}")]
    Http(#[from] reqwest::Error),
    #[error("GitHub: {0}")]
    Api(String),
}

#[derive(Clone)]
pub struct GhClient {
    http: reqwest::Client,
    token: Arc<String>,
}

impl GhClient {
    pub fn new(token: String) -> Result<Self, GhError> {
        let http = reqwest::Client::builder()
            .user_agent(concat!(
                env!("CARGO_PKG_NAME"),
                "/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()?;
        Ok(Self {
            http,
            token: Arc::new(token),
        })
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    /// Latest release tag of owner/repo (e.g. "v0.2.0"); None if the repo has
    /// no releases yet.
    pub async fn latest_release_tag(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<Option<String>, GhError> {
        #[derive(Deserialize)]
        struct Release {
            tag_name: String,
        }

        let url = format!("https://api.github.com/repos/{owner}/{repo}/releases/latest");
        let resp = self
            .http
            .get(url)
            .bearer_auth(self.token.as_str())
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await?;
        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(GhError::Api(format!("{status}: {body}")));
        }
        let release: Release = resp.json().await?;
        Ok(Some(release.tag_name))
    }

    async fn graphql<R: DeserializeOwned>(
        &self,
        query: &str,
        variables: Value,
    ) -> Result<R, GhError> {
        #[derive(Deserialize)]
        struct Envelope<T> {
            data: Option<T>,
            errors: Option<Vec<GqlError>>,
        }
        #[derive(Deserialize)]
        struct GqlError {
            message: String,
        }

        let resp = self
            .http
            .post(GRAPHQL_URL)
            .bearer_auth(self.token.as_str())
            .json(&json!({ "query": query, "variables": variables }))
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(GhError::Api(format!("{status}: {body}")));
        }
        let envelope: Envelope<R> = resp.json().await?;
        if let Some(errors) = envelope.errors.filter(|e| !e.is_empty()) {
            let msgs: Vec<String> = errors.into_iter().map(|e| e.message).collect();
            return Err(GhError::Api(msgs.join("; ")));
        }
        envelope
            .data
            .ok_or_else(|| GhError::Api("empty response".to_string()))
    }

    /// Auth smoke test; returns the logged-in login.
    pub async fn viewer(&self) -> Result<String, GhError> {
        let data: ViewerData = self.graphql(VIEWER_QUERY, json!({})).await?;
        Ok(data.viewer.login)
    }

    fn search_vars() -> Value {
        json!({ "q": SEARCH_QUERY_STRING, "first": SEARCH_PAGE_SIZE })
    }

    pub async fn search_involved_prs(&self) -> Result<Vec<PrSummary>, GhError> {
        let data: SearchData = self.graphql(SEARCH_PRS_QUERY, Self::search_vars()).await?;
        Ok(data.search.nodes.into_iter().map(Into::into).collect())
    }

    /// Raw JSON of the PR search — for `--dump`.
    pub async fn search_involved_prs_raw(&self) -> Result<Value, GhError> {
        self.graphql(SEARCH_PRS_QUERY, Self::search_vars()).await
    }

    fn pr_vars(pr: &PrRef, threads_after: &Option<String>) -> Value {
        json!({
            "owner": pr.owner,
            "name": pr.repo,
            "number": pr.number,
            "threadsAfter": threads_after,
        })
    }

    /// Fetch a PR, paginating review threads until exhausted.
    pub async fn fetch_pr(&self, pr: &PrRef) -> Result<PrDetail, GhError> {
        let mut after: Option<String> = None;
        let mut base: Option<RawPullRequest> = None;
        let mut threads: Vec<RawThread> = Vec::new();
        loop {
            let data: PrDetailData = self
                .graphql(PR_DETAIL_QUERY, Self::pr_vars(pr, &after))
                .await?;
            let mut raw = data
                .repository
                .and_then(|r| r.pull_request)
                .ok_or_else(|| GhError::Api(format!("{pr} not found")))?;
            threads.append(&mut raw.review_threads.nodes);
            let page = raw.review_threads.page_info.clone();
            if base.is_none() {
                base = Some(raw);
            }
            if page.has_next_page && page.end_cursor.is_some() {
                after = page.end_cursor;
            } else {
                break;
            }
        }
        let base = base.expect("loop ran at least once");
        Ok(PrDetail::from_raw(base, threads))
    }

    /// Raw JSON of a PR detail (first page only) — for `--dump`.
    pub async fn fetch_pr_raw(&self, pr: &PrRef) -> Result<Value, GhError> {
        self.graphql(PR_DETAIL_QUERY, Self::pr_vars(pr, &None))
            .await
    }

    /// Add a top-level conversation comment. `subject_id` is the PR node id.
    pub async fn add_comment(&self, subject_id: &str, body: &str) -> Result<(), GhError> {
        let vars = json!({ "subjectId": subject_id, "body": body });
        let _: Value = self.graphql(ADD_COMMENT_MUTATION, vars).await?;
        Ok(())
    }

    /// Resolve or unresolve a review thread; returns (thread id, new state).
    pub async fn set_thread_resolved(
        &self,
        thread_id: &str,
        resolved: bool,
    ) -> Result<(String, bool), GhError> {
        let vars = json!({ "id": thread_id });
        let state = if resolved {
            let data: ResolveThreadData = self.graphql(RESOLVE_THREAD_MUTATION, vars).await?;
            data.resolve_review_thread.thread
        } else {
            let data: UnresolveThreadData = self.graphql(UNRESOLVE_THREAD_MUTATION, vars).await?;
            data.unresolve_review_thread.thread
        };
        Ok((state.id, state.is_resolved))
    }

    /// Reply to a review thread via REST (posts immediately, no pending review).
    /// `comment_db_id` is the databaseId of the thread's root comment.
    pub async fn reply_to_comment(
        &self,
        pr: &PrRef,
        comment_db_id: u64,
        body: &str,
    ) -> Result<(), GhError> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/pulls/{}/comments/{}/replies",
            pr.owner, pr.repo, pr.number, comment_db_id
        );
        let resp = self
            .http
            .post(url)
            .bearer_auth(self.token.as_str())
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(&json!({ "body": body }))
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(GhError::Api(format!("{status}: {body}")));
        }
        Ok(())
    }
}
