---
name: ghpr
description: Fetch a GitHub pull request's review threads and conversation comments as JSON using the ghpr CLI, then plan how to address each one. Use when the user says "use ghpr", asks to get/read/list PR comments or review feedback, wants to address or respond to reviewer comments, plan fixes for review threads, or check unresolved threads on the current branch's PR.
argument-hint: "[owner/repo#123 | PR URL | PR number]"
allowed-tools: Bash(ghpr *) Bash(gh pr view *) Bash(gh repo view *)
---

# ghpr: PR review comments → action plan

## 1. Resolve the PR reference

Target given: `$ARGUMENTS`

- `owner/repo#123` → use as-is.
- `https://github.com/owner/repo/pull/123` → `owner/repo#123`.
- Bare number `123` → get `owner/repo` from `gh repo view --json nameWithOwner -q .nameWithOwner`.
- Empty → the current branch's PR: `gh pr view --json url -q .url`, then convert the URL as above.

If none of these work, ask the user which PR they mean. Do not guess.

## 2. Fetch

```sh
ghpr --json owner/repo#123
```

Never use `--dump` (raw GraphQL, first page only, debug text on stderr).

- If `ghpr` is not installed, tell the user:
  `curl -fsSL https://raw.githubusercontent.com/planted-sam/ghpr/main/install.sh | sh`
- Auth comes from `gh auth token` or `GITHUB_TOKEN`. If ghpr exits with an auth error, report it and stop.

## 3. Output shape

Top level: `pr`, `url`, `title`, `state`, `is_draft`, `author`, `base_ref`, `head_ref`,
`unresolved_count`, `thread_count`, `timeline_truncated`, `timeline[]`, `threads[]`.

- `timeline[]` — PR description first, then conversation comments and review verdicts.
  `kind` is `comment` or `review`; reviews also carry `verdict`
  (`approved`, `changes_requested`, `commented`, `dismissed`, `other`).
- `threads[]` — inline review threads, sorted unresolved-first, then by `path`/`line`:
  `url`, `is_resolved`, `is_outdated`, `path`, `line`, `diff_hunk`,
  `comments[{author, body, created_at}]`, `hidden_count`, `last_activity`.
- Timestamps are RFC 3339.

## 4. Build the plan

Default scope: unresolved threads, plus timeline comments and `changes_requested` reviews
that ask for something. Skip resolved threads unless the user asks for them.

Before judging any thread, read the current code at `path:line` and compare with `diff_hunk`.
If `is_outdated` is true the code has moved since the comment; find where it lives now.

Group by file. For each thread give:

- **`path:line`** with the thread `url`. Mark `(outdated)` when `is_outdated`.
- **Discussion**: one line per participant summarizing what they said, ending with the
  latest state (open question, agreed change, pushback).
- **Recommendation**: one of `address` | `push back` | `already fixed` | `needs clarification`,
  with a one-sentence reason.
- **Proposed change**: the concrete edit (file, function, what changes). Do not edit yet.

Then a short section for conversation-level asks (non-thread comments, `changes_requested`
review bodies) with the same fields minus `path:line`.

Finish with a summary table (file, thread count, how many address / push back / fixed /
question) and any caveats:

- A thread with `hidden_count > 0` has earlier comments that were not fetched; say so and
  point at its `url`.
- `timeline_truncated: true` means the conversation is incomplete; offer `gh pr view --comments`.

## 5. Confirm before editing

Ask the user which items to address (or "all recommended") before changing any files.
When implementing, work file by file and cite the thread `url` in your notes so the user can
reply on GitHub.

ghpr is read-only here. Replying to or resolving threads happens in the ghpr TUI
(`ghpr owner/repo#123`) or on GitHub.
