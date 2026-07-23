use anyhow::{Context, Result};
use tokio::process::Command;

/// Resolve a GitHub token: `gh auth token` first, then $GITHUB_TOKEN.
pub async fn resolve_token() -> Result<String> {
    if let Ok(out) = Command::new("gh").args(["auth", "token"]).output().await
        && out.status.success()
    {
        let token = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !token.is_empty() {
            return Ok(token);
        }
    }
    std::env::var("GITHUB_TOKEN")
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .context("no GitHub token found: run `gh auth login` or set GITHUB_TOKEN")
}
