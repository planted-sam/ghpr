//! GraphQL query/mutation strings and the raw serde structs mirroring their
//! response shapes. Domain conversions live in [`super::types`].

use serde::Deserialize;

pub const VIEWER_QUERY: &str = "query { viewer { login } }";

pub const SEARCH_PRS_QUERY: &str = r#"
query($q: String!, $first: Int!) {
  search(type: ISSUE, query: $q, first: $first) {
    pageInfo { hasNextPage endCursor }
    nodes {
      ... on PullRequest {
        number title isDraft updatedAt reviewDecision
        author { login }
        repository { nameWithOwner }
        comments { totalCount }
      }
    }
  }
}"#;

pub const PR_DETAIL_QUERY: &str = r#"
query($owner: String!, $name: String!, $number: Int!, $threadsAfter: String) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      id number title body state isDraft baseRefName headRefName createdAt
      author { login }
      timelineItems(first: 100, itemTypes: [ISSUE_COMMENT, PULL_REQUEST_REVIEW]) {
        pageInfo { hasNextPage endCursor }
        nodes {
          __typename
          ... on IssueComment { author { login } body createdAt }
          ... on PullRequestReview { author { login } body state createdAt }
        }
      }
      reviewThreads(first: 50, after: $threadsAfter) {
        pageInfo { hasNextPage endCursor }
        nodes {
          id isResolved isOutdated path line
          comments(first: 50) {
            totalCount
            nodes { databaseId author { login } body createdAt diffHunk }
          }
        }
      }
    }
  }
}"#;

pub const ADD_COMMENT_MUTATION: &str = r#"
mutation($subjectId: ID!, $body: String!) {
  addComment(input: {subjectId: $subjectId, body: $body}) {
    commentEdge { node { id } }
  }
}"#;

pub const RESOLVE_THREAD_MUTATION: &str = r#"
mutation($id: ID!) {
  resolveReviewThread(input: {threadId: $id}) {
    thread { id isResolved }
  }
}"#;

pub const UNRESOLVE_THREAD_MUTATION: &str = r#"
mutation($id: ID!) {
  unresolveReviewThread(input: {threadId: $id}) {
    thread { id isResolved }
  }
}"#;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    pub has_next_page: bool,
    pub end_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RawActor {
    pub login: String,
}

// ---- SearchInvolvedPrs ----

#[derive(Debug, Deserialize)]
pub struct SearchData {
    pub search: RawSearch,
}

#[derive(Debug, Deserialize)]
pub struct RawSearch {
    pub nodes: Vec<RawPrSummary>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawPrSummary {
    pub number: u64,
    pub title: String,
    pub is_draft: bool,
    pub updated_at: String,
    pub review_decision: Option<String>,
    pub author: Option<RawActor>,
    pub repository: RawRepo,
    pub comments: RawCount,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawRepo {
    pub name_with_owner: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawCount {
    pub total_count: u64,
}

// ---- PrDetail ----

#[derive(Debug, Deserialize)]
pub struct PrDetailData {
    pub repository: Option<RawRepository>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawRepository {
    pub pull_request: Option<RawPullRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawPullRequest {
    pub id: String,
    pub number: u64,
    pub title: String,
    pub body: String,
    pub state: String,
    pub is_draft: bool,
    pub base_ref_name: String,
    pub head_ref_name: String,
    pub created_at: String,
    pub author: Option<RawActor>,
    pub timeline_items: RawTimeline,
    pub review_threads: RawThreads,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawTimeline {
    pub page_info: PageInfo,
    pub nodes: Vec<RawTimelineNode>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "__typename")]
pub enum RawTimelineNode {
    IssueComment(RawIssueComment),
    PullRequestReview(RawReview),
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawIssueComment {
    pub author: Option<RawActor>,
    pub body: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawReview {
    pub author: Option<RawActor>,
    pub body: String,
    pub state: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawThreads {
    pub page_info: PageInfo,
    pub nodes: Vec<RawThread>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawThread {
    pub id: String,
    pub is_resolved: bool,
    pub is_outdated: bool,
    pub path: String,
    pub line: Option<u64>,
    pub comments: RawThreadComments,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawThreadComments {
    pub total_count: u64,
    pub nodes: Vec<RawThreadComment>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawThreadComment {
    pub database_id: Option<u64>,
    pub author: Option<RawActor>,
    pub body: String,
    pub created_at: String,
    pub diff_hunk: String,
}

// ---- Mutation responses ----

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveThreadData {
    pub resolve_review_thread: ThreadPayload,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnresolveThreadData {
    pub unresolve_review_thread: ThreadPayload,
}

#[derive(Debug, Deserialize)]
pub struct ThreadPayload {
    pub thread: RawThreadState,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawThreadState {
    pub id: String,
    pub is_resolved: bool,
}

#[derive(Debug, Deserialize)]
pub struct ViewerData {
    pub viewer: RawActor,
}
