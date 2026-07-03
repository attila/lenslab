---
title: Machine-readable commands validate before stdout
category: architecture
component: lenslab-cli
applies_when:
  - adding JSON-producing CLI commands
  - expanding analyse output
  - preserving stdout/stderr contracts
tags:
  - cli
  - json
  - output-contract
  - validation
---

# Machine-readable commands validate before stdout

## Context

`lenslab` uses stdout as the machine-readable API. Human diagnostics, progress, and failures belong
on stderr. For commands such as `analyse`, a consumer may pipe stdout straight into another program,
so partial or misleading JSON is worse than no JSON.

## Problem

The first `analyse` skeleton exposed a few traps that are easy to repeat:

- decoding the first valid input before noticing a later invalid input can tempt a command to leak
  partial output;
- directory checks are not enough for path preflight because FIFOs and device files can block in a
  decoder;
- successful JSON must not contain placeholder fields for future verdicts, QA, artefacts, or
  unimplemented metrics;
- correction provenance needs to be explicit so unknown-correction measurements are not mistaken for
  aggregation-ready raw measurements.

## Solution

For JSON-producing commands:

- Validate cheap path constraints before decode work starts.
- Reject non-regular files, not only directories.
- Build and validate the full report in memory before the first stdout write.
- Write diagnostics and failures only to stderr, with stdout empty on every failure path.
- Stream the already-built report to stdout with `serde_json::to_writer_pretty` when practical,
  avoiding a second full output string without weakening the no-partial-output rule.
- Emit only fields that the command can populate honestly. Do not add empty verdict, QA, artefact,
  or metric placeholders.
- Keep provenance and aggregation eligibility explicit when input correction status is unknown.

## Verification

Cover the command contract at process level:

- success emits parseable JSON on stdout and empty stderr;
- missing, invalid, corrected, directory, and non-regular inputs exit non-zero with empty stdout;
- a later failing input does not leak JSON from earlier valid inputs;
- repeated runs over the same inputs produce byte-identical stdout;
- real fixtures cover confirmed-uncorrected DNG success and corrected-input rejection.

Useful commands:

```sh
cargo test -p lenslab-cli analyse
just test-fixtures
```

For release readiness, also run the built binary against at least one success path and one failure
path:

```sh
cargo build -p lenslab-cli
target/debug/lenslab analyse tests/fixtures/dng/bayer_k1.dng
target/debug/lenslab analyse tests/fixtures/dng/xtrans_xt3.dng
```

## When to apply

Apply this to every command whose stdout is consumed by scripts or agents: `analyse`, future
`decentre`, JSON reports, and any command that emits measurement data. Use the artefact-output
pattern instead for commands whose primary output is a local file.
