---
name: lenslab-pr-readiness
description: Use before opening, updating, or merging a lenslab PR, especially after decode, core, dependency, binary-size, roadmap, or CLI-output changes. Checks roadmap freshness, crate boundaries, release-size impact, and PR verification evidence.
---

# Lenslab PR readiness

Run this before a PR is considered merge-ready.

## Required checks

1. Roadmap freshness:
   - Read `docs/ROADMAP.md`.
   - Confirm the merged work is reflected under `Done`.
   - Confirm `Up Next` names the next concrete task.
   - Confirm deferred gaps are explicit.
2. Crate boundary:
   - For decode/core changes, confirm LGPL-linked dependencies remain confined to `lenslab-decode`.
   - Confirm `lenslab-core` stays dependency-free unless the PR intentionally changes that boundary.
   - Useful checks:
     - `cargo tree -p lenslab-core`
     - `rg "rawler|tiff" lenslab-core -n`
3. Release binary size:
   - For dependency, decode, CLI, or release-profile changes, build release and record the binary
     size.
   - Useful checks:
     - `cargo build --release -p lenslab-cli`
     - `ls -lh target/release/lenslab`
   - Treat unexpected growth as a design issue. Explain the reason or reduce it.
4. Output contract:
   - For CLI output changes, verify machine-readable output stays on stdout and diagnostics stay on
     stderr.
5. Gates:
   - Run `just ci` before claiming done.
   - Run `just test-fixtures` after decode, fixture, or CI changes.
   - For integration-test targets, use Cargo's target selector rather than a bare test-name filter,
     which can compile the integration test binary while running zero tests:
     ```sh
     cargo test -p lenslab-cli --test analyse_cli
     ```

## Reporting

Report the command and summary/exit line for every verification claim. For PRs, keep the body terse
and include only the checks that explain risk.
