# lenslab — Decision Log

Lightweight ADRs. Each: decision, options weighed, rationale. All are **locked** for v0.1 unless
noted.

## D1 — Language: Rust

**Options:** Python (rich raw/imaging ecosystem) · Node · Go · Rust. **Decision:** Rust. **Why:**
Single statically-linked binary, no runtime package ecosystem (the explicit driver — Python's
packaging was a non-starter for the owner). Native raw decoders exist (rawler/rawloader/zenraw).
Strong numerics (`ndarray`, `rustfft`, `image`). Go has no credible native raw decoder (only
cgo→LibRaw, which forfeits the clean binary); Node's quality path is native LibRaw addons via
node-gyp (the same fragility we are avoiding).

## D2 — Decode backend: `rawler` behind a trait

**Options:** `rawloader` (≈200 cameras, Bayer only) · `rawler` (300+, X-Trans/CR3/JXL-DNG) ·
`zenraw` (safe Rust, scene-linear f32 output, swappable backends) · bind LibRaw. **Decision:**
`rawler`, accessed through a `Decoder` trait in `lenslab-decode`. **Why:** Broadest pure-Rust
coverage. The trait keeps the backend swappable (zenraw or a permissive fallback later) and
**confines the LGPL dependency to one crate**. zenraw's scene-linear f32 output is attractive and a
likely future backend.

## D3 — Licence: option (a), single static LGPL binary

**Options:**

- (a) Accept LGPL-2.1 on the distributed binary; statically link `rawler`; core stays MIT/Apache.
  **One static binary.**
- (b) Keep a permissive binary by splitting decode into a separate process or `cdylib`. **Not a
  single binary** (two executables, or binary + shared lib).
- (c) Permissive-only: own SOF3 decoder + `tiff` crate; DNG-lossless-JPEG + TIFF only, no
  proprietary raws.

**Decision:** (a). **Why:** For a fully open-source project the LGPL §6 relink obligation is
satisfied automatically — the complete buildable source is public, so any recipient can rebuild
against a modified `rawler`. (b) only matters when embedding the core in closed/permissive-only
software, which is not a goal ("deploy to multiple agents later" concerns plugin distribution, not
embedding the Rust core). (a) preserves single-binary ergonomics. The LGPL surface is still confined
to `lenslab-decode`, leaving (c) available to anyone who needs a fully-permissive build. **Note:**
The grey area (some lawyers dislike static-link LGPL even with full source) is the only reason to
revisit; revisit only if embedding-in-closed-software becomes a requirement.

## D4 — Name: `lenslab`

Chosen over optik / apertura / mtflab.

## D5 — Plugin first

**Decision:** Ship a Claude plugin (`plugin/`) first; keep the orchestration logic portable so other
agents can host it later. **Why:** Matches the owner's deployment path. The skill is thin
(orchestrate + narrate); portability comes free from keeping all real logic in the binary behind the
JSON contract.

## D6 — Division of labour: measurement in Rust, judgement in the plugin

**Decision:** The CLI does all deterministic measurement and emits versioned JSON. The plugin
coaches the shoot, runs the binary, interprets JSON into a verdict, and never re-measures. **Why:**
Determinism, testability and reproducibility belong in compiled code; shot coaching and narrative
judgement (verified/inferred framing, keep/return steer) are where an LLM adds value. The versioned
JSON schema is the contract between them.

## D7 — v0.1 scope: full measurement battery

**Decision:** v0.1 includes ingest/normalise (DNG+TIFF), `inspect`, `contact`, sharpness (MTF50 +
acutance), decentring, vignetting, **CA, distortion, field-curvature**. v0.2: HTML report,
focus-bracket support, additional decode backends. **Why:** Owner wants the whole battery usable
from the first release; the algorithms are already validated (see ALGORITHMS.md), so the risk is
implementation, not method.

## D8 — Measurement on uncorrected linear data

**Decision:** Always demosaic-free (single green plane) for sharpness, no WB/gamma/opcode
corrections; detect and refuse/warn on baked-in corrections (DNG opcodes / TIFF profile tags).
**Why:** Physical, reproducible numbers. Cooked input silently invalidates vignetting and sharpness.
This is non-negotiable and must be enforced at ingest.

## Open questions (not blocking v0.1 start)

- Camera→pixel-pitch source: derive vs small bundled DB vs config override. (ALGORITHMS §MTF50.)
- Auto edge-detection quality for MTF50 vs requiring `--roi`.
- Frame-role auto-classification heuristic thresholds (target vs scene).
- Sidecar format for per-frame tags (role, focus-bracket) — filename convention vs TOML sidecar.
