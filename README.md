# ghpr
[Screencast from 2026-07-23 14-23-00.webm](https://github.com/user-attachments/assets/13544853-6ffc-43a6-82ad-afa280c789a8)

A terminal UI for reading and replying to GitHub pull request comments, built with [ratatui](https://ratatui.rs).

GitHub's web UI sucks at PR's with hella files/comments. `ghpr` is a TUI app to make it easier to track all the specific threads of conversation

## Install

One-liner (macOS arm64, Linux x86_64/arm64 — installs to `/usr/local/bin` or `~/.local/bin`):

```sh
curl -fsSL https://raw.githubusercontent.com/planted-sam/ghpr/main/install.sh | sh
```

Or grab a prebuilt binary from the [releases page](https://github.com/planted-sam/ghpr/releases), or build from source:

```sh
cargo install --path .
```

`ghpr` checks for new releases on startup — when the header shows an update notice, press `U` to install it in place.

## Auth

No setup if you use the [gh CLI](https://cli.github.com): `ghpr` reuses your existing login via `gh auth token`. Otherwise set `GITHUB_TOKEN` to a token with `repo` scope.

## Usage

```sh
ghpr                    # list open PRs you're involved in (author/reviewer/mentioned)
ghpr owner/repo#123     # jump straight to a PR
ghpr --dump prs         # debug: print fetched JSON instead of the TUI
ghpr --dump owner/repo#123
```

## Keys

### PR list

| Key | Action |
|-----|--------|
| `j` / `k` | move selection |
| `g` / `G` | top / bottom |
| `Enter` | open PR |
| `r` | refresh |
| `o` | open in browser |
| `q` | quit |

### PR detail

Two panes: **Conversation** (timeline: description, comments, review verdicts) and **Threads** (inline review threads with diff hunks).

| Key | Action |
|-----|--------|
| `Tab`, `1` / `2` | switch pane |
| `j` / `k` | select item |
| `]` / `[` | next / previous unresolved thread |
| `s` | toggle thread sort: by latest comment (default) / by file (unresolved first) |
| `c` | new conversation comment |
| `a` | reply to selected thread |
| `x` | resolve / unresolve selected thread |
| `Ctrl-d` / `Ctrl-u`, `PgDn` / `PgUp` | scroll body pane |
| `r` | refresh |
| `o` | open in browser |
| `Esc` | back to list |

### Compose

| Key | Action |
|-----|--------|
| `Ctrl-S` | send |
| `Esc` | cancel (press twice to discard unsaved text) |

## Notes

- Data comes from GitHub's GraphQL API (review threads and their resolution state aren't available over REST). The one exception: thread replies post via REST, because the GraphQL reply mutation can attach the reply to a pending review that stays invisible to everyone else until submitted.
- Dependency pins: `ratatui` is held at 0.29 (the latest line `tui-textarea` supports) and `tui-markdown` at `=0.3.3` (later versions target the new `ratatui-core` split). Bump all three together.

## Development

Tasks run through [just](https://github.com/casey/just):

```sh
just ci     # fmt-check + clippy -D warnings + tests — the commit gate
just run    # cargo run
just dump   # fetch JSON without the TUI
just gql    # validate a GraphQL query against the live API via gh
```
