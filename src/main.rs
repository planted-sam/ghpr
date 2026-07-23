mod app;
mod auth;
mod cli;
mod event;
mod github;
mod ui;
mod update;

use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use crossterm::event::EventStream;
use futures_util::StreamExt;
use tokio::sync::mpsc;

use app::App;
use cli::{Cli, PrRef};
use event::AppEvent;
use github::GhClient;

enum Target {
    List,
    Pr(PrRef),
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let target = match cli.target.as_deref() {
        None | Some("prs") => Target::List,
        Some(s) => Target::Pr(s.parse().map_err(|e: String| anyhow!(e))?),
    };

    let token = auth::resolve_token().await?;
    let client = GhClient::new(token)?;

    if cli.dump {
        return dump(&client, &target).await;
    }

    let viewer = client
        .viewer()
        .await
        .context("GitHub auth check failed (is your token valid?)")?;

    let direct = match target {
        Target::List => None,
        Target::Pr(pr) => Some(pr),
    };
    run_tui(client, viewer, direct).await
}

/// Debug mode: print raw JSON to stdout and parsed domain types to stderr.
async fn dump(client: &GhClient, target: &Target) -> Result<()> {
    match target {
        Target::List => {
            let raw = client.search_involved_prs_raw().await?;
            println!("{}", serde_json::to_string_pretty(&raw)?);
            let parsed = client.search_involved_prs().await?;
            eprintln!("\n---- parsed ({} PRs) ----\n{parsed:#?}", parsed.len());
        }
        Target::Pr(pr) => {
            let raw = client.fetch_pr_raw(pr).await?;
            println!("{}", serde_json::to_string_pretty(&raw)?);
            let parsed = client.fetch_pr(pr).await?;
            eprintln!("\n---- parsed ----\n{parsed:#?}");
        }
    }
    Ok(())
}

async fn run_tui(client: GhClient, viewer: String, direct: Option<PrRef>) -> Result<()> {
    let token = client.token().to_string();
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, client, viewer, direct).await;
    ratatui::restore();
    match result? {
        None => Ok(()),
        Some(version) => {
            println!("updating ghpr to v{version}…");
            let status = tokio::task::spawn_blocking(move || update::run_updater(&token)).await??;
            match status {
                self_update::Status::Updated(v) => println!("ghpr updated to v{v}"),
                self_update::Status::UpToDate(v) => println!("ghpr is already up to date (v{v})"),
            }
            Ok(())
        }
    }
}

/// Runs the TUI until quit; returns Some(version) if the user requested an
/// update install on exit.
async fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    client: GhClient,
    viewer: String,
    direct: Option<PrRef>,
) -> Result<Option<String>> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut app = App::new(client, tx, viewer, direct);
    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(120));

    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;
        let ev = tokio::select! {
            maybe = events.next() => match maybe {
                Some(Ok(e)) => AppEvent::Term(e),
                Some(Err(e)) => return Err(e.into()),
                None => return Ok(None),
            },
            Some(msg) = rx.recv() => AppEvent::Api(msg),
            _ = tick.tick() => AppEvent::Tick,
        };
        app.handle_event(ev);
        if app.should_quit {
            return Ok(app
                .update_requested
                .then(|| app.update_available.clone())
                .flatten());
        }
    }
}
