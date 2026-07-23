//! Self-update: an async release check on startup plus a blocking installer
//! (self_update) that runs after the TUI has been torn down.

use crate::github::GhClient;

pub const REPO_OWNER: &str = "planted-sam";
pub const REPO_NAME: &str = "ghpr";
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Returns Some(new_version) if a newer release exists. All failures
/// (network, no releases, bad semver) are treated as "no update".
pub async fn check_for_update(client: &GhClient) -> Option<String> {
    let tag = client
        .latest_release_tag(REPO_OWNER, REPO_NAME)
        .await
        .ok()??;
    let latest = tag.trim_start_matches('v').to_string();
    self_update::version::bump_is_greater(CURRENT_VERSION, &latest)
        .ok()
        .filter(|&greater| greater)
        .map(|_| latest)
}

/// Downloads the latest release and replaces the current executable.
/// Blocking (reqwest::blocking inside self_update) — must run on a
/// spawn_blocking thread, after the terminal has been restored.
pub fn run_updater(token: &str) -> anyhow::Result<self_update::Status> {
    let status = self_update::backends::github::Update::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name("ghpr")
        .current_version(CURRENT_VERSION)
        .auth_token(token)
        .no_confirm(true) // user already confirmed via keypress in the TUI
        .show_download_progress(true)
        .build()?
        .update()?;
    Ok(status)
}
