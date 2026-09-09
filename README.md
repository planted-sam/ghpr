# ghpr
[Screencast from 2026-07-23 14-23-00.webm](https://github.com/user-attachments/assets/13544853-6ffc-43a6-82ad-afa280c789a8)

A terminal UI for reading and replying to GitHub pull request comments, built with [ratatui](https://ratatui.rs). Also a `--json` mode for scripts and AI agents, with a bundled [Claude Code skill](#claude-code).

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
ghpr                        # list open PRs you're involved in (author/reviewer/mentioned)
ghpr owner/repo#123         # jump straight to a PR
ghpr --json owner/repo#123  # print the PR (threads, comments, review verdicts) as JSON
ghpr --json prs             # print the PR list as JSON
ghpr --dump owner/repo#123  # debug: raw GraphQL response (first page only)
```

`--json` output is the parsed model: `pr`, `url`, `unresolved_count`, `thread_count`, PR metadata, a `timeline` (description, comments, review verdicts) and `threads` (inline review threads with `path`, `line`, `is_resolved`, `is_outdated`, `diff_hunk`, `url` and their comments), sorted unresolved-first then by file. Threads are fully paginated; stdout is JSON only.

## Claude Code

The repo doubles as a Claude Code plugin with a `ghpr` skill. Once installed, in any repo with an open PR you can say:

> use ghpr to get comments for this PR, then let's make a plan for how each comment could be addressed

Claude resolves the PR from the current branch (or a URL / `owner/repo#123` / number you give it), runs `ghpr --json`, and writes a per-thread plan — file and line, what the reviewers said, a recommendation (address / push back / already fixed / needs clarification) and the proposed change — then asks which items to take on before editing anything.

Install options:

```sh
# as a plugin (inside Claude Code)
/plugin marketplace add planted-sam/ghpr
/plugin install ghpr@ghpr

# or: install.sh drops the skill into ~/.claude/skills/ghpr/ when ~/.claude exists
#     (skipped if the plugin is already installed; set GHPR_NO_SKILL=1 to opt out)

# or: from a checkout
ln -s "$(pwd)/skills/ghpr" ~/.claude/skills/ghpr
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
just json   # parsed PR JSON without the TUI, e.g. `just json owner/repo#123`
just dump   # debug: raw GraphQL JSON
just bump   # set version in Cargo.toml, Cargo.lock and .claude-plugin/plugin.json
just gql    # validate a GraphQL query against the live API via gh
```
