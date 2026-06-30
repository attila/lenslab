# lenslab — Validated Algorithms

The measurement methods, thresholds and gotchas, all proven in the origin session on real 645D
files. Port these into `lenslab-core`. Reference implementations are in `reference/prototype/`
(Python + C); this doc is the spec, the prototype is the worked example.

Guiding rule throughout: **measure on uncorrected, scene-linear data.** No demosaic interpolation in
the sharpness path, no white balance, no gamma, no lens-correction opcodes. Cooked data gives cooked
numbers.

---

## Decode (reference, not the production path)

Production uses `rawler`. This section documents what the bespoke decoder proved, useful for (a)
understanding the data and (b) the optional permissive DNG-only backend.

Pentax 645D DNG specifics (`reference/prototype/ljpeg.c`, `dng.py`):

- TIFF/EP container, little-endian. IFD0 carries a small RGB thumbnail and `SubIFDs`. One SubIFD is
  the CFA raw (`PhotometricInterpretation = 32803`), another a full-size YCbCr JPEG preview
  (`photometric = 6`) — **never measure on the preview**.
- Raw CFA: 7424×5552, 14-bit, `Compression = 7` (lossless JPEG, SOF3). `BlackLevel = 0`,
  `WhiteLevel ≈ 15718–15730`, `ActiveArea` = full frame.
- `CFAPattern` bytes `02 01 01 00` → BGGR: (0,0)=B, (0,1)=G, (1,0)=G, (1,1)=R. **Read this tag — do
  not assume RGGB.**
- Lossless JPEG: SOF3, precision 14, **2 interleaved components** (each H=V=1), one shared Huffman
  table, predictor selector `Ss = 1` (Ra/left), no restart interval. Component `c` maps to columns:
  `full_row[2*x + c]`. Predictors: first sample of scan = `1<<(P-1)`; first column of a line (x=0,
  y>0) = sample above (Rb); otherwise left neighbour (Ra). 16-bit wrap. Standard sign-extension of
  the magnitude.
- **Validation method**: after decode, correlation between the two green sub-mosaics (spatially
  shifted) was 0.995, and a demosaiced thumbnail was a coherent scene. Use this green-correlation
  check as a decode self-test in fixtures.

EXIF: `FNumber`, `FocalLength`, `ISO`, `ExposureTime` in the Exif IFD are reliable.
`LensModel (0xA434)` is **empty on Pentax** — lens identity is in MakerNotes. `SubjectDistance`
absent. Implication: lens id must come from `rawler`/MakerNote parsing or `--lens`.

---

## Channel selection

Sharpness/decentring measured on a **single green sub-mosaic** (one of G1/G2), i.e. native samples
on their own grid at half resolution (e.g. 3712×2776). Rationale: zero demosaic interpolation (no
synthetic detail), and avoiding the half-pixel error of averaging the two offset green sites. Full
bilinear RGB is used only for CA and for the visual crop montages.

`--channel luma` is offered for TIFF/RGB inputs (no CFA): use Rec.709 luma.

---

## Zones

Default **5-point**: centre + 4 corners. Patch ≈ 13% of frame dimension; corner inset ≈ 4.5% from
edges; corners at equal image radius. `3x3` and `9x9` grids optional. For every patch also compute
**contrast = std/mean**; this gates validity (see Decentring).

---

## Sharpness — acutance proxy (works on any scene; the fallback metric)

Scene-normalised relative sharpness, robust to differing content:

```
b1 = gaussian(patch, σ=1.0)
b2 = gaussian(patch, σ=2.5)
hp = patch - b1          # high frequency
mp = b1 - b2             # mid frequency
acutance = std(hp) / std(mp)
```

Higher = sharper (more energy retained at high frequency relative to mid). It is **relative**, not
absolute — valid for comparing zones, copies, and apertures of the _same_ lens, and (with care)
across lenses on the same body. Do not present it as MTF. Reference:
`reference/prototype/analyze.py::acutance`.

Caveat that bit us: on casual scenes the four corners contain _different_ content (smooth sky vs
detailed foreground), so per-zone acutance is contaminated. Two defences: (1) measure decentring on
a **flat uniform target** where all zones share content; (2) for scene aggregates, **texture-gate**
(drop zones with contrast < 0.15) and aggregate across many frames so content noise averages out.

---

## Sharpness — slanted-edge MTF50 (target shots; the absolute metric)

ISO 12233 slanted-edge, the upgrade that makes results comparable across lenses. Not yet
implemented; method:

1. Locate/accept an edge ROI (`--roi`, or auto-detect a high-contrast near-vertical/horizontal edge
   in a target frame), ideally 2–10° off-axis.
2. Find sub-pixel edge location per scan line (centroid of the derivative), fit a line → edge angle.
3. Project pixels onto the edge normal, **super-sample** (typ. 4×) into the ESF; bin and average.
4. Differentiate ESF → LSF; window; FFT → MTF.
5. Read **MTF50**: frequency at 50% contrast, in cy/px. Convert to lp/mm with pixel pitch (see
   below). Report per zone.

Pixel pitch: derive from sensor mm ÷ active pixel count, or a small camera DB; **report cy/px when
pitch is unknown** rather than guessing lp/mm. 645D ≈ 6.05 µm (44 mm / 7264 effective).

`--metric auto`: MTF50 on target frames with a usable edge; acutance otherwise.

---

## Decentring — the copy verdict (read this carefully)

Corner softness is **normal and expected**; it is not evidence of a bad copy. The signal is
**asymmetry**, and even asymmetry has innocent causes that must be excluded.

Discriminators, in order of strength:

1. **Left/right symmetry** (strongest, and robust even on scenes). Horizontal decentring — the
   common manufacturing failure — shows as one side consistently softer at every aperture. Compute
   TL−TR and BL−BR; aggregate (texture-gated) across all available frames. A mean near zero with
   scatter ≫ |mean| ⇒ no horizontal decentring. _Origin result: TL−TR mean +0.006 over 27 varied
   frames ⇒ centred._
2. **Aperture consistency.** A real decentred element softens the **same** side/corner at **every**
   aperture. If the diagonal asymmetry **flips sign** between apertures, it is not a fixed optical
   fault. _Origin: diagonal asymmetry flipped between f/4 and f/5.6+ ⇒ not decentring._
3. **Visual corner character.** Decentring/coma produces **directional smear** (comet tails) in the
   worst corner; field curvature/defocus produces **symmetric blur**. Generate the 5-zone 100% crop
   montage and check. _Origin: all corners blurred symmetrically, no smear ⇒ centred._
4. **Field-curvature vs decentring.** Field curvature = symmetric corner softness that **improves as
   you stop down** (DoF covers the curved field); corners peaking ~2 stops later than centre is its
   signature. Decentring persists regardless of aperture. A focus-bracket (focus a corner; if it
   sharpens, it is curvature not decentring) settles ambiguous cases — v0.2 supports tagging these.

**QA gate (must run before trusting any corner asymmetry):** estimate **keystone/tilt**. On the
prototype, vertical brick-course spacing was 125 px (top) vs 129 px (bottom) ≈ **3%**, revealing a
slight downward tilt; the residual top/bottom sharpness gradient tracked that tilt axis, _not_ the
lens. Method: detect a periodic/straight reference and compare its scale top-vs-bottom and
left-vs-right (autocorrelation period, or line spacing). Report `keystone_pct` and `tilt_axis`; gate
target frames over `--gate-keystone` (default 1.5%) and attribute any asymmetry on the tilt axis to
tilt, not glass.

Verdict states: `centred`, `decentred`, `inconclusive` (needs a proper target). Always emit the
evidence list and confidence. Never upgrade to a hard `decentred` from scene frames alone — require
a gated target series.

---

## Vignetting

Measure on **raw linear green**, black-subtracted; corner-vs-centre **median** luminance ratio →
stops (`log2`).

Two essential refinements learned the hard way:

- **Scene frames are unreliable** — varying framing puts the centre patch on the sun in one frame
  and sea in the next; the numbers flip sign and are useless. Use a flat, evenly-lit target.
- **Aperture-difference to isolate the optical component.** Any fixed lighting gradient (or target
  non-uniformity) is constant across an aperture series; optical vignetting changes with aperture.
  Reference each aperture's corner falloff to the most-stopped frame (e.g. f/11) to cancel the fixed
  part. _Origin: ~0.7 stop of pure optical vignetting at f/4, ~0 by f/8._
- **Symmetry check.** Optical vignetting is radially symmetric. A corner asymmetry that is
  **constant across all apertures** is lighting/target, not lens. _Origin: TR consistently dark, BR
  bright at every aperture ⇒ lighting from lower-right, not the lens._
- Caveat: patches inset ~4% understate the extreme-corner falloff; note this in output.

Reference: `reference/prototype/vign.py`.

---

## Lateral CA

Per corner, demosaic R and B planes; measure the **sub-pixel R−B shift** via 1-D cross-correlation
of row/column profiles with **parabolic peak refinement**. Report magnitude in px (×2 if measured on
a half-res plane → full-res). Expect radial direction. _Origin (f/8): ~1–2 px at the worst corners,
<0.6 px at the others; some scatter from differing per-corner edge content._ Also compute R−G if
useful. Reference: the CA block in the session notes / `analyze.py` planes helper.

---

## Distortion

Trace a near-straight reference line (mortar course, grid line) across the frame width: per-x
centroid of darkness within a tracking window → `y(x)`; quadratic fit; **sagitta** vs the straight
chord between endpoints → % of frame height. `+` sag for a top line bowing up / bottom line bowing
down = barrel; symmetric same-sign top and bottom = symmetric barrel.

Limitation observed: brick mortar lines are not perfect references and do not reach the extreme
corners where barrel peaks, so the prototype could only bound central-field bow to **<0.2%** (method
floor) and could not quantify edge barrel. For real distortion numbers use a proper
grid/checkerboard target; otherwise mark `method: inferred`. A course-spacing cross-check (period at
top/centre/bottom) exists but is confounded by tilt — use only as corroboration.

---

## Field curvature

Inferred in v0.1 from the aperture behaviour: corners that need ~2 stops more than centre to reach
peak sharpness indicate field curvature (the lens's known trait). State as `inferred`. v0.2: measure
directly from focus-bracket frames (a centre-focused and corner-focused frame at the same wide
aperture; if the corner sharpens when focused there, it is curvature, quantify the focus-shift).

---

## Worked example (regression target — DA 645 25mm f/4, 645D)

Acutance (relative metric), centre then 4-corner mean, by aperture:

| f-stop | centre | corner mean | corner/centre |
| ------ | ------ | ----------- | ------------- |
| f/4    | 0.95   | 1.01        | —*            |
| f/5.6  | 1.59   | 1.11        | 0.70          |
| f/8    | 1.58   | 1.25        | 0.79          |
| f/11   | 1.63   | 1.37        | 0.84          |
| f/16   | 1.56   | 1.33        | 0.85          |
| f/22   | 1.32   | 1.15        | 0.87          |
| f/32   | 1.06   | 0.98        | 0.92          |

\* f/4 ratio is unstable (centre also soft wide open; thin DoF interacts with field curvature).
Centre peaks f/5.6–f/11; diffraction softens from f/16, hard by f/32. Corners peak f/11.

- **Vignetting**: corner mean −1.06 st at f/4 → −0.33 by f/8 then flat; optical component
  (aperture-differenced) ~0.7 st at f/4, ~0 by f/8.
- **Lateral CA** (f/8): ~1–2 px worst corners (full-res).
- **Distortion**: central bow < 0.2% (floor-limited).
- **Decentring**: centred. Left/right corner asymmetry mean +0.006 over 27 frames; diagonal
  asymmetry flips with aperture; no directional smear; ~3% keystone explained the mild top/bottom
  gradient.
- **Verdict**: centred, sound copy; optimum f/8–f/11; corners ~16–21% behind centre at optimum.

Once `rawler` can read these same files, reproducing this table (within tolerance) is a good
high-level regression check. The frames are not committed (photographer's); use synthetic fixtures
for CI.
