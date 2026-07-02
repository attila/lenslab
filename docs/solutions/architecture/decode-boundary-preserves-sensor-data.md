---
title: Decode boundaries preserve sensor data before analysis support exists
category: architecture
component: lenslab-decode
applies_when:
  - adding decoded frame types
  - adapting raw decoder output into core image types
  - handling unsupported CFA layouts
tags:
  - decode
  - cfa
  - rawler
  - crate-boundary
---

# Decode boundaries preserve sensor data before analysis support exists

## Context

`lenslab-decode` is the LGPL-linked adapter crate. `lenslab-core` must stay dependency-free and own
the measurement-facing image types. The decode boundary therefore acts as a codec transform:
external decoder data enters through `lenslab-decode`, and core receives validated, owned values.

## Problem

It is tempting to reject unsupported sensor layouts during decode, or to normalise raw samples
against container limits such as `u16::MAX`. Both choices lose information before analysis code has
a chance to decide what it can support.

The X-Trans fixture exposed the first failure mode: a decoder can provide a valid CFA frame whose
extraction path is unsupported by current Bayer-only code. Raw RGB and LinearRaw paths exposed the
second: decoder-provided black/white levels are the meaningful sample range, not the integer type
maximum.

## Solution

Decode should preserve sensor data and make unsupported analysis paths typed:

- Store CFA level data even when the pattern is not extractable yet.
- Return a typed unsupported-pattern error from extraction, not from frame decode.
- Normalise integer raw samples with decoder black/white levels per component.
- Preserve inspected metadata when returning decoded TIFF frames.
- Keep `rawler` and TIFF-specific code out of `lenslab-core`.

## Verification

Use real fixtures for this class of boundary:

- Bayer fixture proves normal decode and extraction still work.
- X-Trans fixture proves unsupported CFA data survives decode.
- Core dependency checks prove the LGPL boundary did not move.

Useful commands:

```sh
cargo tree -p lenslab-core
rg "rawler|tiff" lenslab-core -n
just test-fixtures
```

## When to apply

Apply this whenever a decoder, pixel model, channel model, or CFA path changes. Unsupported
downstream measurement is not a reason to discard upstream sensor data.
