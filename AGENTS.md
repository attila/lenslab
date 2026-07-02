# lenslab — agent guide

Conventions for any agent (Claude Code local or cloud, Codex, others) working in this repository.
Repository-level rules here govern agent behaviour for this project; the design documents are the
source of truth for _what_ to build.

## What lenslab is

`lenslab` characterises a camera lens from a folder of DNG/TIFF frames and flags a decentred or
tilted copy — sharpness vs aperture, decentring, vignetting, lateral CA, distortion, field curvature
— on uncorrected linear sensor data, with an honest split between _measured_ and _inferred_. A Rust
CLI for deterministic measurement plus a Claude plugin that coaches the test shots and reads the
numbers into a verdict.

Status: pre-implementation. The design is the source of truth — start at `docs/GENESIS.md`, then
`docs/SPEC.md`, `docs/ALGORITHMS.md`, `docs/DECISIONS.md`, `docs/SKILL_PLUGIN.md`. A validated
prototype lives under `reference/`.

## Least code that earns its place

The strongest version of any change is the smallest one that fully and correctly solves the task.
Before writing code, take the cheapest option that holds:

1. Does it need to exist at all? If not, don't write it.
2. Does the codebase, the standard library, a platform feature, or an existing dependency already do
   it? Reuse it rather than rebuild it.
3. Only then write new code — the minimum the task genuinely needs.

This is deliberate, authored minimalism — not golf, and not laziness. Correctness, the typed crate
boundaries (codec-style transforms between layers), input validation, and explicit error handling
are never what gets cut: the code is small because every line is necessary, not because lines were
shaved. A smaller design that needs an enabling refactor first is a "stop and ask" — see Scope &
autonomy.

## Layout & boundaries

Planned Cargo workspace:

- `lenslab-core` — measurement and analysis. Permissive (MIT/Apache-2.0). No LGPL or copyleft
  dependencies.
- `lenslab-cli` — the binary: decode → normalise → measure → emit. Permissive.
- `lenslab-decode` — raw decode via `rawler`/`rawloader` (LGPL-2.1). **All LGPL-linked code stays
  confined to this crate**, so the rest of the workspace can be swapped to a permissive
  DNG/TIFF-only backend without touching `core` or `cli` (see `NOTICE`, `docs/DECISIONS.md`).
  `cargo-deny` gates the licence allowlist; `LGPL-2.1` is permitted only because of this crate.

Other load-bearing constraints:

- **Measured vs inferred.** Keep the honest split throughout. Never present an inferred value as a
  measured one.
- **Output contract.** Results are deterministic. Machine-readable output (canonical JSON) goes to
  stdout; human-facing diagnostics and progress go to stderr; exit codes are meaningful (`0`
  success, non-zero distinct failure modes). Output that a script or agent consumes is the _purpose_
  of the command and belongs on stdout.

## Build, test, format

- `just ci` runs the full gate: `fmt` (dprint check) · `clippy` (`-D warnings`) · `test` · `deny` ·
  `doc`. Run it before declaring work done.
- `just test-fixtures` downloads the real-camera DNG fixtures from the pinned GitHub Release assets
  and runs the `RawlerDecoder` fixture tests. Run it after decode, fixture, or CI changes; CI runs
  it in the test job. Plain `cargo test`/`just test` stays offline and does not fetch fixtures.
- Formatting is dprint; the Rust toolchain is pinned by `rust-toolchain.toml`.
- A pre-commit hook runs `dprint check`. Activate it once with `just setup` (cloud sessions wire it
  automatically via the SessionStart hook in `.claude/settings.json`).

## Working model

Built cloud-first and in the open. Anything that must persist across sessions is committed to this
repository — a cloud session only ever sees the clone, so there is no private or uncommitted state
to rely on. Planning and status live in `docs/ROADMAP.md`. Throwaway files go in `./tmp/`
(git-ignored); never leave working state outside a commit.

Solo and open-source but not open-collaboration: external issues and pull requests are not accepted
yet (see `CONTRIBUTING.md`), so don't propose soliciting contributors. Detailed Rust conventions
live in `.ai/rules/rust.md` (symlinked to `.claude/rules/`); situational procedures live in
`.ai/skills/` (symlinked to `.claude/skills/`).

## Pinned rules

- **Never guess legal names.** Licence, copyright, or any legal text must use exact names — do not
  infer from git config, username, or email. The holder is **Attila Beregszaszi**. `NOTICE`
  deliberately reads "the lenslab authors"; leave legal text as-is unless told otherwise.
- **Never use the `AskUserQuestion` tool** (or any interactive question-tool wrapper), even when a
  skill suggests it. Present choices as numbered options in plain text and wait for the reply.
- **For "what's next" questions**, read `docs/ROADMAP.md` (plan and status), `docs/GENESIS.md`, the
  other `docs/` design documents, and recent git history — never answer from memory alone.

## Workflow discipline

- **Verbatim test output before claiming done.** Paste the command and its summary/exit line — never
  "all green". Ran a subset? Say which. Never assert success you didn't verify, never reuse stale
  output, and never edit a test to pass without flagging why.
- **Don't invent APIs, flags, or config keys** — verify against the source.
- **State a hypothesis before running commands**, and distinguish verified from inferred: "I believe
  X" is not "I confirmed X".
- **Chesterton's Fence.** Understand why something exists before changing or removing it. Can't
  explain it? Read more first.
- **Trace dependents before changing shared code.** "Nothing else uses this" is usually wrong —
  prove it.
- **Confirm before destructive or external actions:** `rm -rf`, force-push, history rewrites, branch
  deletes, killing processes, editing CI/release configuration, or anything that posts to an
  external service.

## Scope & autonomy

- Every changed line must trace to the task's success criterion.
- Touch only what the task needs. The only clean-up allowed is the mess your own change makes —
  never reformat or rename untouched code.
- No speculative abstraction, helpers, configuration, feature flags, compatibility shims, or partial
  implementations beyond the task.
- An enabling or prerequisite refactor — even when a clean implementation needs it — means **stop
  and ask first**. Never fold a refactor into a behaviour change.
- When scope grows irreversibly, crosses a module/API/schema boundary, or starts touching files the
  task didn't name, **stop and re-confirm**.

## Dependencies & tooling

- Adding a crate or tool needs justification: the real alternatives and the trade-off chosen against
  them. Prefer established crates; `cargo-deny` gates licences and advisories — keep the allowlist
  honest (`LGPL-2.1` only via `lenslab-decode`).
- Toolchain and library choices are the maintainer's call. Propose them; don't introduce them
  unilaterally inside an unrelated change.

## External actions

Don't post, approve, merge, comment on pull requests or issues, or trigger deploys, releases, or CI
reruns on a human's behalf without explicit instruction. Drafts are fine — keep them unpublished
until the exact content and destination are approved.

## Writing

Default for all human-facing written content — code comments, commit messages, pull-request and
issue text, documentation, and chat — is plain, mostly jargon-free British English. Use British
spelling; keep technical terms and code identifiers in their original form; preserve diacritics
(never substitute ASCII for accented characters — "não" not "nao", "für" not "fur", "löschen" not
"loeschen").

**Exception:** agent-only documents and the body of implementation plans are exempt from the
plain-prose requirement — they may be dense, structured, and jargon-heavy, since no human reads them
end to end. An implementation plan carries a short **human summary** that does follow the rule
above; the rest of the plan need not.

Don't flatter, hedge, or apologise reflexively. Disagree directly when the reasoning supports it,
and give the reason. Avoid AI-filler and marketing vocabulary; the following illustrate the
_category_ to avoid, not an exhaustive list: delve, leverage, utilise, seamlessly, robust,
comprehensive, streamline, facilitate, paramount, cutting-edge, game-changer, transformative,
innovative, synergy, holistic, nuanced, multifaceted, crucial, vital, foster, realm, "dive into",
"it's worth noting", "it's important to note", "it's essential to", "in today's world", "in the
ever-evolving", "at the end of the day", "in conclusion", furthermore, moreover, thus, hence,
indeed, certainly, absolutely, "of course", "great question", "excellent point", "I'd be happy to",
"I hope this helps", "feel free to", "as an AI", "let's explore", "let's dive in", "that being
said", "having said that", "in summary", "to summarise", "all in all".

## Don'ts

- Don't generate READMEs, design documents, summaries, or `*.md` files unless asked.
- Never inspect credentials, keychains, shell history, or unrelated dotfiles.
