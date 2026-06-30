# List available recipes
default:
    @just --list

# Configure git hooks (run once after clone)
setup:
    git config core.hooksPath .githooks

# Check formatting
fmt:
    dprint check

# Fix formatting
fmt-fix:
    dprint fmt

# Run clippy lints
clippy:
    cargo clippy --all-targets -- -D warnings

# Run tests
test:
    cargo test

# Run dependency audits
#
# `GIT_CONFIG_*` overrides neutralise URL rewrites before gix (used by
# cargo-deny) fetches the RustSec advisory DB. Without them, a `[url] insteadOf`
# rule in `~/.gitconfig` or sandbox-injected `GIT_CONFIG_KEY_*` rewrites can
# turn the HTTPS clone into an SSH one and fail in environments where port 22
# is blocked.
deny:
    GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_COUNT=0 cargo deny check

# Build documentation
doc:
    cargo doc --no-deps

# Install lenslab to ~/.cargo/bin
install:
    cargo install --path .

# Run the full CI pipeline
ci: fmt clippy test deny doc
