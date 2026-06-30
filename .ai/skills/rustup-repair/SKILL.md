---
name: rustup-repair
description: Use when a rustup toolchain install is broken — `cargo`/`rustc` missing or failing to load (e.g. `librustc_driver-*` dylib/so not found), or `rustup toolchain install` reports "toolchain is already up to date" yet rustc errors. Repairs an interrupted toolchain install (common after a setup-script or container interruption).
---

# Repairing an interrupted rustup toolchain install

## Symptom

`~/.rustup/toolchains/<channel>-<host>/` exists but key binaries are missing — typically
`cargo`/`rustc` while `clippy`/`rustfmt` are present. A present binary fails with a missing-library
error: macOS `dyld: Library not loaded:
librustc_driver-*.dylib`; Linux
`error while loading shared libraries:
librustc_driver-*.so`. The library ships in the un-extracted
`rust` component.

Diagnostic tell: `rustup toolchain install <channel>` short-circuits with
`debug: toolchain is already up to date` while the next line admits `(error reading rustc version)`.

## Root cause

Rustup writes `~/.rustup/update-hashes/<channel>-<host>` **before** extracting components. An
interrupted install (Ctrl-C, network drop, sleep, container kill — including a cut-off cloud setup
script) leaves the hash file behind, which short-circuits every later `install`. There is no
filesystem integrity check.

## Fix

`install` alone cannot repair this. Uninstall (clearing the hash), then reinstall:

    rustup toolchain uninstall <channel>-<host>
    rustup toolchain install <channel>

Component archives in `~/.rustup/downloads/` are reused on hash-match, so the redownload cost is
usually zero.

## Don't be misled

`rustup show` lists the toolchain as installed from directory existence, not completeness.
`rustup component add` backfills peripheral components but cannot fix a missing `cargo`/`rustc`.
