use std::fmt;
use std::str::FromStr;

use clap::Parser;

/// A terminal UI for reading and responding to GitHub PR comments.
#[derive(Parser, Debug)]
#[command(version, about)]
pub struct Cli {
    /// PR to open directly ("owner/repo#123"), or "prs" to list involved PRs (the default)
    pub target: Option<String>,

    /// Print fetched data as JSON instead of launching the TUI
    #[arg(long)]
    pub dump: bool,
}

/// A fully-qualified pull request reference: `owner/repo#number`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrRef {
    pub owner: String,
    pub repo: String,
    pub number: u64,
}

impl PrRef {
    pub fn url(&self) -> String {
        format!(
            "https://github.com/{}/{}/pull/{}",
            self.owner, self.repo, self.number
        )
    }
}

impl FromStr for PrRef {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || format!("expected owner/repo#number, got {s:?}");
        let (path, num) = s.split_once('#').ok_or_else(err)?;
        let (owner, repo) = path.split_once('/').ok_or_else(err)?;
        let number: u64 = num.parse().map_err(|_| err())?;
        if owner.is_empty() || repo.is_empty() || repo.contains('/') {
            return Err(err());
        }
        Ok(PrRef {
            owner: owner.to_string(),
            repo: repo.to_string(),
            number,
        })
    }
}

impl fmt::Display for PrRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}#{}", self.owner, self.repo, self.number)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_pr_ref() {
        let pr: PrRef = "rust-lang/cargo#1234".parse().unwrap();
        assert_eq!(pr.owner, "rust-lang");
        assert_eq!(pr.repo, "cargo");
        assert_eq!(pr.number, 1234);
        assert_eq!(pr.to_string(), "rust-lang/cargo#1234");
        assert_eq!(pr.url(), "https://github.com/rust-lang/cargo/pull/1234");
    }

    #[test]
    fn rejects_invalid_pr_refs() {
        for bad in [
            "",
            "prs",
            "owner/repo",
            "owner#123",
            "owner/repo#",
            "owner/repo#abc",
            "/repo#1",
            "owner/#1",
            "a/b/c#1",
        ] {
            assert!(bad.parse::<PrRef>().is_err(), "should reject {bad:?}");
        }
    }
}
