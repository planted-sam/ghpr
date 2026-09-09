//! `--json` output: the parsed domain model, serialized for scripts and agents.

use serde::Serialize;

use crate::cli::PrRef;
use crate::github::types::{PrDetail, PrSummary, ThreadSort};

#[derive(Serialize)]
struct PrJson<'a> {
    /// "owner/repo#123"
    pr: String,
    url: String,
    unresolved_count: usize,
    thread_count: usize,
    #[serde(flatten)]
    detail: &'a PrDetail,
}

/// Serialize a PR with threads sorted unresolved-first, then by file position —
/// the order that suits working through review feedback.
pub fn pr_to_json(pr: &PrRef, detail: &mut PrDetail) -> serde_json::Result<String> {
    detail.sort_threads(ThreadSort::Position);
    let out = PrJson {
        pr: pr.to_string(),
        url: pr.url(),
        unresolved_count: detail.unresolved_count(),
        thread_count: detail.threads.len(),
        detail,
    };
    serde_json::to_string_pretty(&out)
}

pub fn prs_to_json(prs: &[PrSummary]) -> serde_json::Result<String> {
    serde_json::to_string_pretty(prs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::queries::{PrDetailData, SearchData};
    use serde_json::Value;

    fn data<T: serde::de::DeserializeOwned>(fixture: &str) -> T {
        let envelope: Value = serde_json::from_str(fixture).unwrap();
        serde_json::from_value(envelope["data"].clone()).unwrap()
    }

    fn fixture_detail() -> PrDetail {
        let data: PrDetailData = data(include_str!("../tests/fixtures/pr_detail.json"));
        let mut raw = data.repository.unwrap().pull_request.unwrap();
        let raw_threads = std::mem::take(&mut raw.review_threads.nodes);
        PrDetail::from_raw(raw, raw_threads)
    }

    #[test]
    fn pr_json_shape() {
        let pr: PrRef = "ratatui/ratatui#2424".parse().unwrap();
        let mut detail = fixture_detail();
        let json = pr_to_json(&pr, &mut detail).unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();

        assert_eq!(v["pr"], "ratatui/ratatui#2424");
        assert_eq!(v["url"], "https://github.com/ratatui/ratatui/pull/2424");
        assert_eq!(v["number"], 2424);
        assert_eq!(v["unresolved_count"], 3);
        assert_eq!(v["thread_count"], 16);

        let threads = v["threads"].as_array().unwrap();
        assert_eq!(threads.len(), 16);
        // Position sort: unresolved threads lead.
        assert!(threads[..3].iter().all(|t| t["is_resolved"] == false));
        assert!(threads[3..].iter().all(|t| t["is_resolved"] == true));
        // Fixture predates the `url` field; key is present but null.
        assert!(threads[0].get("url").is_some());
        assert!(threads[0]["url"].is_null());
        assert!(threads[0]["comments"].as_array().unwrap()[0]["body"].is_string());

        // Timestamps are RFC 3339 strings.
        let ts = threads[0]["last_activity"].as_str().unwrap();
        ts.parse::<jiff::Timestamp>().unwrap();
        let ts = v["timeline"][0]["created_at"].as_str().unwrap();
        ts.parse::<jiff::Timestamp>().unwrap();

        // TimelineKind flattens to kind (+ verdict for reviews).
        let timeline = v["timeline"].as_array().unwrap();
        assert_eq!(timeline[0]["kind"], "comment");
        assert!(timeline[0].get("verdict").is_none());
        assert!(
            timeline
                .iter()
                .any(|item| { item["kind"] == "review" && item["verdict"] == "changes_requested" })
        );
    }

    #[test]
    fn prs_json_shape() {
        let data: SearchData = data(include_str!("../tests/fixtures/search_prs.json"));
        let prs: Vec<PrSummary> = data.search.nodes.into_iter().map(Into::into).collect();
        let json = prs_to_json(&prs).unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();
        let arr = v.as_array().unwrap();
        assert!(!arr.is_empty());
        assert_eq!(arr[0]["repo"], "ratatui/ratatui");
        assert!(arr[0]["updated_at"].is_string());
        assert!(arr[0]["number"].is_u64());
    }
}
