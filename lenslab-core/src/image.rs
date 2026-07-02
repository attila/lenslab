use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dimensions {
    width: usize,
    height: usize,
}

impl Dimensions {
    pub const fn new(width: usize, height: usize) -> Result<Self, ImageError> {
        if width == 0 {
            return Err(ImageError::ZeroDimension { axis: "width" });
        }
        if height == 0 {
            return Err(ImageError::ZeroDimension { axis: "height" });
        }
        Ok(Self { width, height })
    }

    #[must_use]
    pub const fn width(self) -> usize {
        self.width
    }

    #[must_use]
    pub const fn height(self) -> usize {
        self.height
    }

    #[must_use]
    pub const fn sample_count(self) -> usize {
        self.width * self.height
    }

    fn checked_sample_count(self) -> Result<usize, ImageError> {
        self.width
            .checked_mul(self.height)
            .ok_or(ImageError::DimensionOverflow)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

impl Rect {
    pub const fn new(x: usize, y: usize, width: usize, height: usize) -> Result<Self, ImageError> {
        if width == 0 {
            return Err(ImageError::ZeroDimension { axis: "width" });
        }
        if height == 0 {
            return Err(ImageError::ZeroDimension { axis: "height" });
        }
        Ok(Self {
            x,
            y,
            width,
            height,
        })
    }

    #[must_use]
    pub const fn x(self) -> usize {
        self.x
    }

    #[must_use]
    pub const fn y(self) -> usize {
        self.y
    }

    #[must_use]
    pub const fn width(self) -> usize {
        self.width
    }

    #[must_use]
    pub const fn height(self) -> usize {
        self.height
    }

    #[must_use]
    pub const fn dimensions(self) -> Dimensions {
        Dimensions {
            width: self.width,
            height: self.height,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorrectionProvenance {
    Absent,
    Present,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinearityProvenance {
    Linear,
    NonLinear,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Provenance {
    corrections: CorrectionProvenance,
    linearity: LinearityProvenance,
}

impl Provenance {
    #[must_use]
    pub const fn new(corrections: CorrectionProvenance, linearity: LinearityProvenance) -> Self {
        Self {
            corrections,
            linearity,
        }
    }

    #[must_use]
    pub const fn measurement_ready() -> Self {
        Self {
            corrections: CorrectionProvenance::Absent,
            linearity: LinearityProvenance::Linear,
        }
    }

    #[must_use]
    pub const fn unknown() -> Self {
        Self {
            corrections: CorrectionProvenance::Unknown,
            linearity: LinearityProvenance::Unknown,
        }
    }

    #[must_use]
    pub const fn corrections(self) -> CorrectionProvenance {
        self.corrections
    }

    #[must_use]
    pub const fn linearity(self) -> LinearityProvenance {
        self.linearity
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BayerPattern {
    Rggb,
    Bggr,
    Gbrg,
    Grbg,
}

impl BayerPattern {
    #[must_use]
    pub const fn first_green_phase(self) -> (u8, u8) {
        match self {
            Self::Rggb | Self::Bggr => (1, 0),
            Self::Gbrg | Self::Grbg => (0, 0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CfaPattern {
    Bayer(BayerPattern),
    Unsupported(String),
}

impl From<BayerPattern> for CfaPattern {
    fn from(pattern: BayerPattern) -> Self {
        Self::Bayer(pattern)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlackWhiteLevels {
    black: [f32; 4],
    white: [f32; 4],
}

impl BlackWhiteLevels {
    pub fn new(black: &[f32], white: &[f32]) -> Result<Self, ImageError> {
        let levels = Self {
            black: levels_array("black", black)?,
            white: levels_array("white", white)?,
        };
        for (black, white) in levels.black.into_iter().zip(levels.white) {
            validate_level_range(black, white)?;
        }
        Ok(levels)
    }

    #[must_use]
    pub const fn black(self) -> [f32; 4] {
        self.black
    }

    #[must_use]
    pub const fn white(self) -> [f32; 4] {
        self.white
    }

    pub(crate) fn at(&self, x_phase: u8, y_phase: u8) -> (f32, f32) {
        let index = y_phase as usize * 2 + x_phase as usize;
        (self.black[index], self.white[index])
    }
}

fn levels_array(kind: &'static str, levels: &[f32]) -> Result<[f32; 4], ImageError> {
    match levels {
        [level] => Ok([*level; 4]),
        [a, b, c, d] => Ok([*a, *b, *c, *d]),
        _ => Err(ImageError::InvalidLevelCount {
            kind,
            count: levels.len(),
        }),
    }
}

fn validate_level_range(black: f32, white: f32) -> Result<(), ImageError> {
    if black.is_finite() && white.is_finite() && white > black {
        Ok(())
    } else {
        Err(ImageError::InvalidLevelRange { black, white })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CfaLevels {
    Bayer(BlackWhiteLevels),
    Raw { black: Vec<f32>, white: Vec<f32> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum CfaSamples {
    U16(Vec<u16>),
    F32(Vec<f32>),
}

impl CfaSamples {
    #[must_use]
    pub fn sample_count(&self) -> usize {
        match self {
            Self::U16(samples) => samples.len(),
            Self::F32(samples) => samples.len(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinearImage {
    dimensions: Dimensions,
    samples: Vec<f32>,
    provenance: Provenance,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RgbImage {
    dimensions: Dimensions,
    components: usize,
    samples: Vec<f32>,
    provenance: Provenance,
}

impl RgbImage {
    pub fn new(
        dimensions: Dimensions,
        components: usize,
        samples: Vec<f32>,
        provenance: Provenance,
    ) -> Result<Self, ImageError> {
        if components != 1 && components != 3 {
            return Err(ImageError::UnsupportedComponentCount { components });
        }
        let expected = dimensions
            .checked_sample_count()?
            .checked_mul(components)
            .ok_or(ImageError::DimensionOverflow)?;
        if expected != samples.len() {
            return Err(ImageError::SampleCountMismatch {
                expected,
                actual: samples.len(),
            });
        }
        Ok(Self {
            dimensions,
            components,
            samples,
            provenance,
        })
    }

    #[must_use]
    pub const fn dimensions(&self) -> Dimensions {
        self.dimensions
    }

    #[must_use]
    pub const fn components(&self) -> usize {
        self.components
    }

    #[must_use]
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    #[must_use]
    pub const fn provenance(&self) -> Provenance {
        self.provenance
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CfaImage {
    dimensions: Dimensions,
    pattern: CfaPattern,
    samples: CfaSamples,
    levels: CfaLevels,
    provenance: Provenance,
}

impl LinearImage {
    pub fn new(dimensions: Dimensions, samples: Vec<f32>) -> Result<Self, ImageError> {
        Self::with_provenance(dimensions, samples, Provenance::measurement_ready())
    }

    pub fn with_provenance(
        dimensions: Dimensions,
        samples: Vec<f32>,
        provenance: Provenance,
    ) -> Result<Self, ImageError> {
        validate_sample_count(dimensions, samples.len())?;
        Ok(Self {
            dimensions,
            samples,
            provenance,
        })
    }

    #[must_use]
    pub const fn dimensions(&self) -> Dimensions {
        self.dimensions
    }

    #[must_use]
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    #[must_use]
    pub const fn provenance(&self) -> Provenance {
        self.provenance
    }

    pub fn patch(&self, rect: Rect) -> Result<LinearPatchView<'_>, ImageError> {
        let Some(end_x) = rect.x.checked_add(rect.width) else {
            return Err(ImageError::PatchOutOfBounds {
                image: self.dimensions,
                patch: rect,
            });
        };
        let Some(end_y) = rect.y.checked_add(rect.height) else {
            return Err(ImageError::PatchOutOfBounds {
                image: self.dimensions,
                patch: rect,
            });
        };
        if end_x > self.dimensions.width || end_y > self.dimensions.height {
            return Err(ImageError::PatchOutOfBounds {
                image: self.dimensions,
                patch: rect,
            });
        }
        Ok(LinearPatchView { image: self, rect })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LinearPatchView<'a> {
    image: &'a LinearImage,
    rect: Rect,
}

impl<'a> LinearPatchView<'a> {
    #[must_use]
    pub const fn dimensions(self) -> Dimensions {
        self.rect.dimensions()
    }

    #[must_use]
    pub const fn rect(self) -> Rect {
        self.rect
    }

    #[must_use]
    pub fn rows(self) -> LinearPatchRows<'a> {
        LinearPatchRows {
            image: self.image,
            rect: self.rect,
            next_row: 0,
        }
    }
}

pub struct LinearPatchRows<'a> {
    image: &'a LinearImage,
    rect: Rect,
    next_row: usize,
}

impl<'a> Iterator for LinearPatchRows<'a> {
    type Item = &'a [f32];

    fn next(&mut self) -> Option<Self::Item> {
        if self.next_row == self.rect.height {
            return None;
        }
        let y = self.rect.y + self.next_row;
        self.next_row += 1;
        let row_start = y * self.image.dimensions.width + self.rect.x;
        Some(&self.image.samples[row_start..row_start + self.rect.width])
    }
}

impl CfaImage {
    pub fn new(
        dimensions: Dimensions,
        pattern: BayerPattern,
        samples: CfaSamples,
        levels: BlackWhiteLevels,
        provenance: Provenance,
    ) -> Result<Self, ImageError> {
        Self::from_pattern(dimensions, pattern.into(), samples, levels, provenance)
    }

    pub fn from_pattern(
        dimensions: Dimensions,
        pattern: CfaPattern,
        samples: CfaSamples,
        levels: BlackWhiteLevels,
        provenance: Provenance,
    ) -> Result<Self, ImageError> {
        Self::from_levels(
            dimensions,
            pattern,
            samples,
            CfaLevels::Bayer(levels),
            provenance,
        )
    }

    pub fn from_raw_levels(
        dimensions: Dimensions,
        pattern: CfaPattern,
        samples: CfaSamples,
        black_levels: Vec<f32>,
        white_levels: Vec<f32>,
        provenance: Provenance,
    ) -> Result<Self, ImageError> {
        if black_levels.is_empty() {
            return Err(ImageError::InvalidLevelCount {
                kind: "black",
                count: 0,
            });
        }
        if white_levels.is_empty() {
            return Err(ImageError::InvalidLevelCount {
                kind: "white",
                count: 0,
            });
        }
        Self::from_levels(
            dimensions,
            pattern,
            samples,
            CfaLevels::Raw {
                black: black_levels,
                white: white_levels,
            },
            provenance,
        )
    }

    fn from_levels(
        dimensions: Dimensions,
        pattern: CfaPattern,
        samples: CfaSamples,
        levels: CfaLevels,
        provenance: Provenance,
    ) -> Result<Self, ImageError> {
        validate_sample_count(dimensions, samples.sample_count())?;
        Ok(Self {
            dimensions,
            pattern,
            samples,
            levels,
            provenance,
        })
    }

    #[must_use]
    pub const fn dimensions(&self) -> Dimensions {
        self.dimensions
    }

    #[must_use]
    pub fn pattern(&self) -> &CfaPattern {
        &self.pattern
    }

    #[must_use]
    pub fn bayer_pattern(&self) -> Option<BayerPattern> {
        match &self.pattern {
            CfaPattern::Bayer(pattern) => Some(*pattern),
            CfaPattern::Unsupported(_) => None,
        }
    }

    #[must_use]
    pub fn bayer_levels(&self) -> Option<&BlackWhiteLevels> {
        match &self.levels {
            CfaLevels::Bayer(levels) => Some(levels),
            CfaLevels::Raw { .. } => None,
        }
    }

    #[must_use]
    pub const fn levels(&self) -> &CfaLevels {
        &self.levels
    }

    #[must_use]
    pub fn samples(&self) -> &CfaSamples {
        &self.samples
    }

    #[must_use]
    pub const fn provenance(&self) -> Provenance {
        self.provenance
    }
}

fn validate_sample_count(dimensions: Dimensions, actual: usize) -> Result<(), ImageError> {
    let expected = dimensions.checked_sample_count()?;
    if expected == actual {
        Ok(())
    } else {
        Err(ImageError::SampleCountMismatch { expected, actual })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImageError {
    ZeroDimension { axis: &'static str },
    SampleCountMismatch { expected: usize, actual: usize },
    InvalidLevelCount { kind: &'static str, count: usize },
    InvalidLevelRange { black: f32, white: f32 },
    DimensionOverflow,
    UnsupportedCfaPattern { pattern: String },
    UnsupportedComponentCount { components: usize },
    PatchOutOfBounds { image: Dimensions, patch: Rect },
}

impl Display for ImageError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroDimension { axis } => write!(formatter, "{axis} must be greater than zero"),
            Self::SampleCountMismatch { expected, actual } => {
                write!(formatter, "expected {expected} samples, got {actual}")
            }
            Self::InvalidLevelCount { kind, count } => {
                write!(formatter, "{kind} level count must be 1 or 4, got {count}")
            }
            Self::InvalidLevelRange { black, white } => {
                write!(
                    formatter,
                    "white level {white} must exceed black level {black}"
                )
            }
            Self::DimensionOverflow => write!(formatter, "image dimensions overflow sample count"),
            Self::UnsupportedCfaPattern { pattern } => {
                write!(formatter, "unsupported CFA pattern {pattern}")
            }
            Self::UnsupportedComponentCount { components } => {
                write!(formatter, "unsupported component count {components}")
            }
            Self::PatchOutOfBounds { image, patch } => write!(
                formatter,
                "patch {}x{} at {},{} exceeds image {}x{}",
                patch.width, patch.height, patch.x, patch.y, image.width, image.height
            ),
        }
    }
}

impl Error for ImageError {}

#[cfg(test)]
mod tests {
    use super::{
        BayerPattern, BlackWhiteLevels, CfaImage, CfaSamples, Dimensions, ImageError, LinearImage,
        Provenance, Rect,
    };

    #[test]
    fn constructs_linear_image_with_borrowed_data_access() {
        let data = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0];

        let image = LinearImage::new(Dimensions::new(4, 3).unwrap(), data.clone()).unwrap();

        assert_eq!(image.dimensions(), Dimensions::new(4, 3).unwrap());
        assert_eq!(image.samples(), data.as_slice());
    }

    #[test]
    fn rejects_zero_dimensions() {
        assert!(matches!(
            Dimensions::new(0, 3),
            Err(ImageError::ZeroDimension { axis: "width" })
        ));
        assert!(matches!(
            Dimensions::new(4, 0),
            Err(ImageError::ZeroDimension { axis: "height" })
        ));
    }

    #[test]
    fn rejects_linear_image_sample_count_mismatches() {
        let err = LinearImage::new(Dimensions::new(4, 3).unwrap(), vec![0.0; 11]).unwrap_err();

        assert!(matches!(
            err,
            ImageError::SampleCountMismatch {
                expected: 12,
                actual: 11
            }
        ));
    }

    #[test]
    fn stores_cfa_integer_samples_without_full_frame_float_conversion() {
        let samples = (0..16).collect::<Vec<u16>>();
        let levels = BlackWhiteLevels::new(&[0.0], &[1023.0]).unwrap();

        let image = CfaImage::new(
            Dimensions::new(4, 4).unwrap(),
            BayerPattern::Rggb,
            CfaSamples::U16(samples.clone()),
            levels,
            Provenance::measurement_ready(),
        )
        .unwrap();

        assert_eq!(image.samples(), &CfaSamples::U16(samples));
        assert_eq!(image.bayer_pattern(), Some(BayerPattern::Rggb));
    }

    #[test]
    fn rejects_bayer_levels_that_are_not_scalar_or_pattern_positioned() {
        let err = BlackWhiteLevels::new(&[0.0, 1.0], &[1023.0]).unwrap_err();

        assert!(matches!(
            err,
            ImageError::InvalidLevelCount {
                kind: "black",
                count: 2
            }
        ));
    }

    #[test]
    fn rejects_invalid_bayer_level_ranges() {
        assert!(matches!(
            BlackWhiteLevels::new(&[1.0], &[1.0]),
            Err(ImageError::InvalidLevelRange {
                black: 1.0,
                white: 1.0
            })
        ));
        assert!(matches!(
            BlackWhiteLevels::new(&[f32::NAN], &[1.0]),
            Err(ImageError::InvalidLevelRange { .. })
        ));
    }

    #[test]
    fn stores_decode_provenance_on_cfa_inputs() {
        let provenance = Provenance::unknown();
        let image = CfaImage::new(
            Dimensions::new(2, 2).unwrap(),
            BayerPattern::Bggr,
            CfaSamples::U16(vec![0, 1, 2, 3]),
            BlackWhiteLevels::new(&[0.0; 4], &[255.0; 4]).unwrap(),
            provenance,
        )
        .unwrap();

        assert_eq!(image.provenance(), provenance);
    }

    #[test]
    fn creates_borrowed_patch_views() {
        let image = LinearImage::new(
            Dimensions::new(4, 3).unwrap(),
            vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0],
        )
        .unwrap();

        let patch = image.patch(Rect::new(1, 1, 2, 2).unwrap()).unwrap();
        let rows = patch.rows().collect::<Vec<_>>();

        assert_eq!(patch.dimensions(), Dimensions::new(2, 2).unwrap());
        assert_eq!(rows, vec![&[5.0, 6.0][..], &[9.0, 10.0][..]]);
    }

    #[test]
    fn rejects_patch_views_that_exceed_image_bounds() {
        let image = LinearImage::new(Dimensions::new(4, 3).unwrap(), vec![0.0; 12]).unwrap();
        let err = image.patch(Rect::new(3, 1, 2, 2).unwrap()).unwrap_err();

        assert!(matches!(
            err,
            ImageError::PatchOutOfBounds { image: _, patch: _ }
        ));
    }
}
