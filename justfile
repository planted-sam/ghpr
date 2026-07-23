default: check

# Run the TUI (e.g. `just run` or `just run owner/repo#123`)
run *ARGS:
    cargo run -- {{ARGS}}

build:
    cargo build

check:
    cargo check --all-targets

clippy:
    cargo clippy --all-targets -- -D warnings

fmt:
    cargo fmt

fmt-check:
    cargo fmt --check

test:
    cargo test

# Everything a commit should pass
ci: fmt-check clippy test

# Debug: print fetched JSON without the TUI, e.g. `just dump prs` / `just dump owner/repo#123`
dump *ARGS:
    cargo run -- --dump {{ARGS}}

# Validate a GraphQL query against the live API, e.g. `just gql -f query='{ viewer { login } }'`
gql *ARGS:
    gh api graphql {{ARGS}}
