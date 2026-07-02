use serde::Serialize;

/// Whether the decoded samples are a colour-filter-array mosaic (raw sensor data) or already
/// demosaiced RGB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Cfa,
    Rgb,
}

/// Shooting parameters read from EXIF, where present.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ExposureInfo {
    pub focal_length_mm: Option<f32>,
    pub f_number: Option<f32>,
    pub exposure_time_s: Option<f32>,
    pub iso: Option<u32>,
}

/// Whether corrections (lens, colour, or otherwise) appear to already be baked into the sample
/// data, and what was inspected to reach that conclusion. `present` is `None` when the format
/// gives no reliable signal either way — see `docs/DECISIONS.md` D8.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Corrections {
    pub present: Option<bool>,
    pub detail: Vec<String>,
}

/// EXIF dump, decode info, and corrections-present flag for one frame — the output of `lenslab
/// inspect`, a diagnostic command outside the versioned `analyse` JSON contract (`docs/SPEC.md`
/// §6). No pixel data — that is the job of the (future) `lenslab-core` image model.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FrameInfo {
    pub source_kind: SourceKind,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_model: Option<String>,
    pub width: usize,
    pub height: usize,
    pub bits_per_sample: usize,
    pub cfa_pattern: Option<String>,
    pub black_level: Option<Vec<f32>>,
    pub white_level: Option<Vec<f32>>,
    pub exposure: ExposureInfo,
    pub corrections: Corrections,
}
