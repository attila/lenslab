---
title: Artefact commands validate output before expensive work
category: architecture
component: lenslab-cli
applies_when:
  - adding visual artefact commands
  - writing local output files
  - rendering decoded frames
tags:
  - cli
  - artefacts
  - output-safety
  - memory
---

# Artefact commands validate output before expensive work

## Context

`lenslab-cli` writes human-facing artefacts such as contact sheets, crop montages, plots, and future
reports. These commands take arbitrary local input paths and output paths, then may decode large
real-camera frames before writing a small final artefact.

## Problem

The first contact-sheet implementation exposed two traps that are easy to repeat:

- output validation after decode/render wastes time and memory before returning a deterministic
  local-path error;
- storing display-ready full-resolution frames until final composition makes a tiny artefact scale
  with the source image set, not with the output sheet.

Atomic writes also need collision discipline. A failed temporary-file creation must not delete a
pre-existing sibling that this invocation did not create.

## Solution

For artefact-writing commands:

- Validate output-path constraints before decode or render work starts.
- Reject output paths that target an input path.
- Reject missing output parents and directory destinations before expensive work.
- Write through a same-directory temporary file created with exclusive creation.
- Remove the temporary file only after this invocation has successfully created it.
- Keep retained render state proportional to the final artefact: thumbnails, crop tiles, plots, or
  encoded buffers, not full decoded frames.

## Verification

Cover both command-level and helper-level failure paths:

- invalid output parent beats invalid input/decode errors;
- output-over-input leaves the input unchanged;
- existing output bytes survive decode/render failure;
- temporary-file collision does not delete the pre-existing temp sibling;
- render state tests prove fixed-size retained buffers where the source image may be large.

Useful commands:

```sh
cargo test -p lenslab-cli
just ci
```

Run `just test-fixtures` when the artefact path touches real DNG decode or unsupported CFA handling.

## When to apply

Apply this to every CLI command that writes a local artefact: contact sheets, crop montages, curves,
reports, and any future export command. Treat local paths as untrusted input and make failure cheap
before decoding large frames.
