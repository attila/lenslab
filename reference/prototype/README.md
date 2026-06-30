# Reference prototype (origin session)

Working Python + C that proved the whole `lenslab` pipeline on real Pentax 645D DNGs. **Reference
only** — production is Rust + `rawler`. Port the _logic_, not the language. See
`../../docs/ALGORITHMS.md` for the spec these implement.

Built in a constrained sandbox: no `rawpy`/`exiftool`, no network, only `gcc` + numpy/PIL. That
constraint is why a from-scratch decoder exists; you almost certainly do not need it (use `rawler`).

| file         | what it is                                                                                                                                          | port target                                                                                             |
| ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `ljpeg.c`    | from-scratch DNG lossless-JPEG (SOF3) decoder → 16-bit CFA. Handles SOF3, shared Huffman table, N interleaved components, predictor 1, FF-stuffing. | `lenslab-decode` permissive fallback only; otherwise reference for the SOF3 notes in ALGORITHMS §Decode |
| `dng.py`     | TIFF/DNG IFD parser; extracts raw strip, black/white levels, CFAPattern, ActiveArea; drives `ljpeg`; returns CFA ndarray + metadata.                | `lenslab-core::image` + EXIF/metadata; `lenslab-decode`                                                 |
| `analyze.py` | green-plane extraction, zone geometry, **acutance** metric, vignetting helpers. The core measurement engine.                                        | `lenslab-core::{demosaic,zones,metrics::acutance,metrics::vignette}`                                    |
| `vign.py`    | vignetting per aperture incl. the aperture-difference / lighting-cancellation method.                                                               | `metrics::vignette`                                                                                     |
| `batch.py`   | batch over a folder; per-zone table; texture-gated decentring aggregate (left/right, top/bottom); sharpness-by-aperture.                            | `group` + `metrics::decentre` + `synth`                                                                 |
| `contact.py` | contact-sheet generator (thumbnails + labels).                                                                                                      | `lenslab-cli contact`                                                                                   |

Notes carried only in the session (not in a file here) but captured in `ALGORITHMS.md`: the CA
sub-pixel cross-correlation, the keystone/squareness check, and the distortion line-bow trace.
Implement those from the doc.

`../sample_outputs/` holds example artifacts: `crops_f8.png` (5-zone 100% corner-crop montage — the
decentring evidence) and `wall_new.png` (the brick-wall aperture ladder used for the copy verdict).
`contact.py` also renders a contact sheet; it is not bundled here, as the example was built from
personal frames.
