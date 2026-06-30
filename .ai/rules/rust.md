# Rust conventions

Project rules for Rust code in lenslab. Adapted from the maintainer's lore patterns and tuned to
this repository; lore remains the portable source of truth.

## Edition, toolchain, licensing

- `edition = "2024"`. Pin `rust-version` (MSRV) in `Cargo.toml` to match `rust-toolchain.toml`
  (currently `1.95`). Edition and toolchain bumps are deliberate, never automatic.
- Workspace crates are dual `MIT OR Apache-2.0`. The exception is `lenslab-decode` (LGPL-linked) —
  keep that boundary (see `AGENTS.md` and `docs/DECISIONS.md`).
- The release profile targets a small static binary; set all three together:
  ```toml
  [profile.release]
  strip = true
  lto = true
  opt-level = "z"
  ```

## Errors

- **Library crates (`lenslab-core`, `lenslab-decode`)** expose typed errors (e.g. via `thiserror`)
  so callers can match and the crate boundary stays codec-clean. No `anyhow` in a library's public
  API.
- **The binary (`lenslab-cli`)** uses `anyhow::Result` for top-level handling. `main()` prints
  `eprintln!("error: {e}")` and exits non-zero; machine output still goes to stdout (see the output
  contract in `AGENTS.md`).
- No `.unwrap()` / `.expect()` in non-test code — propagate with `?` and add context at low-level
  boundaries (`.map_err` / `.context`). Use `f64::total_cmp()` for float ordering, never
  `partial_cmp().unwrap()`.

## Lints

- Pedantic clippy at warn level, denied in CI:
  ```toml
  [lints.clippy]
  pedantic = { level = "warn", priority = -1 }
  missing_errors_doc = "allow"
  missing_panics_doc = "allow"
  module_name_repetitions = "allow"
  ```
  `just clippy` and CI run with `-D warnings`.
- `unsafe_code = "deny"` globally in `[lints.rust]`. If FFI ever needs `unsafe`, scope a targeted
  `#[allow(unsafe_code)]` on the smallest function, with a `// SAFETY:` comment stating the
  invariant.

## Tests

- Output is deterministic, so lean on **snapshot tests** (`insta::assert_json_snapshot!`) for the
  canonical JSON, with committed sample frames under `reference/` or `tests/fixtures/` as inputs.
- Real dependencies over mocks: real filesystem via `tempfile::tempdir()`, real decode on fixture
  frames. lenslab has no network service to fake.
- Unit tests inline as `#[cfg(test)] mod tests`; CLI integration tests in `tests/` with
  `assert_cmd` + `predicates`. Prefer hand-written fakes over `mockall`.
