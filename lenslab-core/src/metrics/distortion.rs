use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::image::{Dimensions, LinearPatchView};
use crate::schema::{
    DistortionBlocker, DistortionCandidate, DistortionEvidence, DistortionMeasurement,
    DistortionMeasurements, DistortionOrientation, DistortionReferenceSide, ExclusionCount,
    ExclusionReason, FrameMeasurement,
};

const MIN_DIMENSION: usize = 16;
const MIN_CONTRAST: f32 = 0.15;
const MIN_TRACE_SPAN_COVERAGE: f32 = 0.25;
const MIN_MEASURED_SPAN_COVERAGE: f32 = 0.72;
const MAX_TRACE_GAP_FRACTION: f32 = 0.08;
const MAX_FIT_RESIDUAL_PX: f32 = 1.0;
const SIDE_BAND_FRACTION: f32 = 0.35;
const MAX_REFERENCE_SUPPORT_FRACTION: f32 = 0.75;
const INFERRED_CONFIDENCE: f32 = 0.4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DistortionError {
    NonFiniteSample { value: f32 },
    NonFiniteDerivedValue { value: f32 },
}

impl Display for DistortionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteSample { value } => {
                write!(formatter, "non-finite distortion sample {value}")
            }
            Self::NonFiniteDerivedValue { value } => {
                write!(formatter, "non-finite distortion value {value}")
            }
        }
    }
}

impl Error for DistortionError {}

pub fn measure_distortion(
    plane: LinearPatchView<'_>,
) -> Result<DistortionMeasurements, DistortionError> {
    validate_samples(plane)?;
    let dimensions = plane.dimensions();
    if dimensions.width() < MIN_DIMENSION || dimensions.height() < MIN_DIMENSION {
        return Ok(DistortionMeasurements::blocked(
            DistortionBlocker::ProfileTooShort,
        ));
    }

    let samples = patch_image(plane);
    classify_best_candidate(trace_candidates(&samples), dimensions)
}

pub fn aggregate_group_distortion(
    frames: &[FrameMeasurement],
) -> Result<DistortionEvidence, DistortionError> {
    let mut accumulator = DistortionAccumulator::default();
    for frame in frames {
        accumulator.push(frame)?;
    }
    accumulator.finish()
}

#[derive(Debug, Clone)]
struct PatchSamples {
    dimensions: Dimensions,
    samples: Vec<f32>,
}

#[derive(Debug, Clone)]
struct TraceCandidate {
    orientation: DistortionOrientation,
    points: Vec<(f32, f32)>,
    contrast: f32,
    largest_gap: usize,
}

#[derive(Debug, Clone)]
struct FittedCandidate {
    orientation: DistortionOrientation,
    reference_side: Option<DistortionReferenceSide>,
    bow_percent: f32,
    sagitta_px: f32,
    span_coverage: f32,
    fit_residual_px: f32,
}

#[derive(Debug, Clone, Copy)]
enum ReferencePolarity {
    Dark,
    Bright,
}

#[derive(Debug, Clone, Copy)]
enum MinorBand {
    Full,
    NearStart,
    NearEnd,
}

fn validate_samples(plane: LinearPatchView<'_>) -> Result<(), DistortionError> {
    for row in plane.rows() {
        for value in row {
            if !value.is_finite() {
                return Err(DistortionError::NonFiniteSample { value: *value });
            }
        }
    }
    Ok(())
}

fn patch_image(plane: LinearPatchView<'_>) -> PatchSamples {
    let dimensions = plane.dimensions();
    let mut samples = Vec::with_capacity(dimensions.sample_count());
    for row in plane.rows() {
        samples.extend_from_slice(row);
    }
    PatchSamples {
        dimensions,
        samples,
    }
}

fn trace_candidates(image: &PatchSamples) -> Vec<TraceCandidate> {
    let mut candidates = Vec::new();
    for orientation in [
        DistortionOrientation::Horizontal,
        DistortionOrientation::Vertical,
    ] {
        for band in [MinorBand::NearStart, MinorBand::NearEnd, MinorBand::Full] {
            for polarity in [ReferencePolarity::Dark, ReferencePolarity::Bright] {
                if let Some(candidate) = trace_candidate(image, orientation, band, polarity) {
                    candidates.push(candidate);
                }
            }
        }
    }
    candidates
}

fn trace_candidate(
    image: &PatchSamples,
    orientation: DistortionOrientation,
    band: MinorBand,
    polarity: ReferencePolarity,
) -> Option<TraceCandidate> {
    let major_len = match orientation {
        DistortionOrientation::Horizontal => image.dimensions.width(),
        DistortionOrientation::Vertical => image.dimensions.height(),
    };
    let full_minor_len = match orientation {
        DistortionOrientation::Horizontal => image.dimensions.height(),
        DistortionOrientation::Vertical => image.dimensions.width(),
    };
    let (minor_start, minor_end) = minor_range(full_minor_len, band);
    let mut points = Vec::new();
    let mut total_contrast = 0.0;
    let mut largest_gap = 0;
    let mut previous_major = None;

    for major in 0..major_len {
        let mut min_value = f32::INFINITY;
        let mut max_value = f32::NEG_INFINITY;
        for minor in minor_start..minor_end {
            let value = sample(image, orientation, major, minor);
            min_value = min_value.min(value);
            max_value = max_value.max(value);
        }
        let contrast = max_value - min_value;
        if contrast < MIN_CONTRAST {
            continue;
        }

        let threshold = match polarity {
            ReferencePolarity::Dark => min_value + contrast * 0.45,
            ReferencePolarity::Bright => max_value - contrast * 0.45,
        };
        let mut weighted_sum = 0.0;
        let mut weight_total = 0.0;
        let mut support_count = 0;
        for minor in minor_start..minor_end {
            let value = sample(image, orientation, major, minor);
            let weight = match polarity {
                ReferencePolarity::Dark if value <= threshold => threshold - value,
                ReferencePolarity::Bright if value >= threshold => value - threshold,
                _ => continue,
            };
            #[allow(clippy::cast_precision_loss)]
            {
                weighted_sum += minor as f32 * weight;
            }
            weight_total += weight;
            support_count += 1;
        }
        if weight_total == 0.0 {
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        if support_count as f32 / (minor_end - minor_start) as f32 > MAX_REFERENCE_SUPPORT_FRACTION
        {
            continue;
        }

        if let Some(previous_major) = previous_major {
            largest_gap = largest_gap.max(major - previous_major - 1);
        }
        previous_major = Some(major);
        #[allow(clippy::cast_precision_loss)]
        let major = major as f32;
        points.push((major, weighted_sum / weight_total));
        total_contrast += contrast;
    }

    if points.is_empty() {
        return None;
    }

    #[allow(clippy::cast_precision_loss)]
    let contrast = total_contrast / points.len() as f32;
    Some(TraceCandidate {
        orientation,
        points,
        contrast,
        largest_gap,
    })
}

fn minor_range(minor_len: usize, band: MinorBand) -> (usize, usize) {
    match band {
        MinorBand::Full => (0, minor_len),
        MinorBand::NearStart => (0, side_band_len(minor_len)),
        MinorBand::NearEnd => (minor_len - side_band_len(minor_len), minor_len),
    }
}

fn side_band_len(minor_len: usize) -> usize {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss
    )]
    let len = (minor_len as f32 * SIDE_BAND_FRACTION).ceil() as usize;
    len.clamp(1, minor_len)
}

fn sample(
    image: &PatchSamples,
    orientation: DistortionOrientation,
    major: usize,
    minor: usize,
) -> f32 {
    let (x, y) = match orientation {
        DistortionOrientation::Horizontal => (major, minor),
        DistortionOrientation::Vertical => (minor, major),
    };
    image.samples[y * image.dimensions.width() + x]
}

fn classify_best_candidate(
    candidates: Vec<TraceCandidate>,
    dimensions: Dimensions,
) -> Result<DistortionMeasurements, DistortionError> {
    let mut blockers = Vec::new();
    let mut fitted = Vec::new();
    for candidate in candidates {
        match fit_candidate(&candidate, dimensions)? {
            CandidateFit::Fitted(candidate) => fitted.push(candidate),
            CandidateFit::Blocked(blocker) => push_blocker(&mut blockers, blocker),
        }
    }

    fitted.sort_by(|left, right| {
        measured_eligible(right)
            .cmp(&measured_eligible(left))
            .then_with(|| right.span_coverage.total_cmp(&left.span_coverage))
            .then_with(|| right.bow_percent.abs().total_cmp(&left.bow_percent.abs()))
    });

    if let Some(candidate) = fitted.into_iter().next() {
        if measured_eligible(&candidate) {
            return Ok(DistortionMeasurements {
                candidate: Some(schema_candidate(&candidate, true)?),
                blockers: Vec::new(),
            });
        }
        return Ok(DistortionMeasurements {
            candidate: Some(schema_candidate(&candidate, false)?),
            blockers: vec![DistortionBlocker::WeakReferenceGeometry],
        });
    }

    if blockers.is_empty() {
        blockers.push(DistortionBlocker::NoStraightReference);
    }
    Ok(DistortionMeasurements {
        candidate: None,
        blockers,
    })
}

fn measured_eligible(candidate: &FittedCandidate) -> bool {
    candidate.span_coverage >= MIN_MEASURED_SPAN_COVERAGE
        && candidate.reference_side.is_some()
        && candidate.fit_residual_px <= MAX_FIT_RESIDUAL_PX
}

enum CandidateFit {
    Fitted(FittedCandidate),
    Blocked(DistortionBlocker),
}

fn fit_candidate(
    candidate: &TraceCandidate,
    dimensions: Dimensions,
) -> Result<CandidateFit, DistortionError> {
    if candidate.contrast < MIN_CONTRAST {
        return Ok(CandidateFit::Blocked(DistortionBlocker::LowContrast));
    }

    let major_len = match candidate.orientation {
        DistortionOrientation::Horizontal => dimensions.width(),
        DistortionOrientation::Vertical => dimensions.height(),
    };
    let minor_len = match candidate.orientation {
        DistortionOrientation::Horizontal => dimensions.height(),
        DistortionOrientation::Vertical => dimensions.width(),
    };
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss
    )]
    let max_allowed_gap = (major_len as f32 * MAX_TRACE_GAP_FRACTION).ceil() as usize;
    if candidate.largest_gap > max_allowed_gap.max(1) {
        return Ok(CandidateFit::Blocked(DistortionBlocker::LineDiscontinuous));
    }

    let (Some(first), Some(last)) = (candidate.points.first(), candidate.points.last()) else {
        return Ok(CandidateFit::Blocked(
            DistortionBlocker::NoStraightReference,
        ));
    };
    #[allow(clippy::cast_precision_loss)]
    let span_coverage = (last.0 - first.0 + 1.0) / major_len as f32;
    if span_coverage < MIN_TRACE_SPAN_COVERAGE {
        return Ok(CandidateFit::Blocked(DistortionBlocker::ProfileTooShort));
    }

    let fit = quadratic_fit(&candidate.points)?;
    let fit_residual_px = fit_residual(&candidate.points, fit)?;
    if fit_residual_px > MAX_FIT_RESIDUAL_PX {
        return Ok(CandidateFit::Blocked(DistortionBlocker::FitResidualTooHigh));
    }

    let mid = (first.0 + last.0) * 0.5;
    let chord_mid = first.1 + (last.1 - first.1) * ((mid - first.0) / (last.0 - first.0));
    let sagitta = eval_quadratic(fit, mid) - chord_mid;
    #[allow(clippy::cast_precision_loss)]
    let mean_minor = candidate
        .points
        .iter()
        .map(|(_, minor)| *minor)
        .sum::<f32>()
        / candidate.points.len() as f32;
    let reference_side = reference_side(candidate.orientation, mean_minor, minor_len);
    let signed_sagitta = signed_sagitta(sagitta, reference_side, candidate.orientation);
    #[allow(clippy::cast_precision_loss)]
    let bow_percent = signed_sagitta / minor_len as f32 * 100.0;

    validate_derived([sagitta, span_coverage, fit_residual_px, bow_percent])?;
    Ok(CandidateFit::Fitted(FittedCandidate {
        orientation: candidate.orientation,
        reference_side,
        bow_percent,
        sagitta_px: signed_sagitta,
        span_coverage,
        fit_residual_px,
    }))
}

fn reference_side(
    orientation: DistortionOrientation,
    mean_minor: f32,
    minor_len: usize,
) -> Option<DistortionReferenceSide> {
    #[allow(clippy::cast_precision_loss)]
    let minor_len = minor_len as f32;
    if mean_minor < minor_len * SIDE_BAND_FRACTION {
        return Some(match orientation {
            DistortionOrientation::Horizontal => DistortionReferenceSide::Top,
            DistortionOrientation::Vertical => DistortionReferenceSide::Left,
        });
    }
    if mean_minor > minor_len * (1.0 - SIDE_BAND_FRACTION) {
        return Some(match orientation {
            DistortionOrientation::Horizontal => DistortionReferenceSide::Bottom,
            DistortionOrientation::Vertical => DistortionReferenceSide::Right,
        });
    }
    None
}

fn signed_sagitta(
    sagitta: f32,
    reference_side: Option<DistortionReferenceSide>,
    orientation: DistortionOrientation,
) -> f32 {
    match (orientation, reference_side) {
        (DistortionOrientation::Horizontal, Some(DistortionReferenceSide::Top))
        | (DistortionOrientation::Vertical, Some(DistortionReferenceSide::Left)) => -sagitta,
        _ => sagitta,
    }
}

fn schema_candidate(
    candidate: &FittedCandidate,
    measured: bool,
) -> Result<DistortionCandidate, DistortionError> {
    let bow = if measured {
        DistortionMeasurement::measured_percent_frame(candidate.bow_percent)
    } else {
        DistortionMeasurement::percent_frame(
            candidate.bow_percent,
            crate::schema::DistortionMethod::InferredWeakReferenceBow,
            INFERRED_CONFIDENCE,
        )
    }
    .ok_or(DistortionError::NonFiniteDerivedValue {
        value: candidate.bow_percent,
    })?;
    DistortionCandidate::new(
        candidate.orientation,
        candidate.reference_side,
        bow,
        candidate.sagitta_px,
        candidate.span_coverage,
        candidate.fit_residual_px,
    )
    .ok_or(DistortionError::NonFiniteDerivedValue {
        value: candidate.bow_percent,
    })
}

fn quadratic_fit(points: &[(f32, f32)]) -> Result<[f32; 3], DistortionError> {
    let mut sx = 0.0;
    let mut sx2 = 0.0;
    let mut sx3 = 0.0;
    let mut sx4 = 0.0;
    let mut sy = 0.0;
    let mut linear_rhs = 0.0;
    let mut quadratic_rhs = 0.0;
    for (x, y) in points {
        let x2 = x * x;
        sx += x;
        sx2 += x2;
        sx3 += x2 * x;
        sx4 += x2 * x2;
        sy += y;
        linear_rhs += x * y;
        quadratic_rhs += x2 * y;
    }
    #[allow(clippy::cast_precision_loss)]
    let n = points.len() as f32;
    solve_3x3(
        [[n, sx, sx2], [sx, sx2, sx3], [sx2, sx3, sx4]],
        [sy, linear_rhs, quadratic_rhs],
    )
}

fn solve_3x3(mut matrix: [[f32; 3]; 3], mut rhs: [f32; 3]) -> Result<[f32; 3], DistortionError> {
    for pivot in 0..3 {
        let mut row = pivot;
        for candidate in pivot + 1..3 {
            if matrix[candidate][pivot].abs() > matrix[row][pivot].abs() {
                row = candidate;
            }
        }
        if matrix[row][pivot].abs() <= f32::EPSILON {
            return Err(DistortionError::NonFiniteDerivedValue { value: 0.0 });
        }
        matrix.swap(pivot, row);
        rhs.swap(pivot, row);
        let divisor = matrix[pivot][pivot];
        for value in matrix[pivot].iter_mut().skip(pivot) {
            *value /= divisor;
        }
        rhs[pivot] /= divisor;
        for row in 0..3 {
            if row == pivot {
                continue;
            }
            let factor = matrix[row][pivot];
            let pivot_row = matrix[pivot];
            for (value, pivot_value) in matrix[row].iter_mut().zip(pivot_row).skip(pivot) {
                *value -= factor * pivot_value;
            }
            rhs[row] -= factor * rhs[pivot];
        }
    }
    validate_derived(rhs)?;
    Ok(rhs)
}

fn eval_quadratic([a, b, c]: [f32; 3], x: f32) -> f32 {
    a + b * x + c * x * x
}

fn fit_residual(points: &[(f32, f32)], fit: [f32; 3]) -> Result<f32, DistortionError> {
    let squared = points
        .iter()
        .map(|(x, y)| {
            let residual = y - eval_quadratic(fit, *x);
            residual * residual
        })
        .sum::<f32>();
    #[allow(clippy::cast_precision_loss)]
    let residual = (squared / points.len() as f32).sqrt();
    validate_derived([residual])?;
    Ok(residual)
}

fn validate_derived(values: impl IntoIterator<Item = f32>) -> Result<(), DistortionError> {
    for value in values {
        if !value.is_finite() {
            return Err(DistortionError::NonFiniteDerivedValue { value });
        }
    }
    Ok(())
}

#[derive(Default)]
struct DistortionAccumulator {
    bow: Vec<DistortionMeasurement>,
    unknown_corrections: usize,
    no_straight_reference: usize,
    weak_reference_geometry: usize,
    low_contrast: usize,
    line_discontinuous: usize,
    fit_residual_too_high: usize,
    profile_too_short: usize,
}

impl DistortionAccumulator {
    fn push(&mut self, frame: &FrameMeasurement) -> Result<(), DistortionError> {
        let distortion = &frame.measurements.distortion;
        if let Some(candidate) = &distortion.candidate {
            validate_candidate(candidate)?;
            if candidate.bow.method == crate::schema::DistortionMethod::MeasuredStraightLineBow
                && frame.aggregation_eligible
            {
                self.bow.push(candidate.bow);
                return Ok(());
            }
            if frame.aggregation_eligible {
                self.weak_reference_geometry += 1;
            } else {
                self.unknown_corrections += 1;
            }
            return Ok(());
        }

        if !frame.aggregation_eligible {
            self.unknown_corrections += 1;
            return Ok(());
        }

        for blocker in &distortion.blockers {
            self.push_blocker(*blocker);
        }
        Ok(())
    }

    fn push_blocker(&mut self, blocker: DistortionBlocker) {
        match blocker {
            DistortionBlocker::UnknownCorrections => self.unknown_corrections += 1,
            DistortionBlocker::NoStraightReference => self.no_straight_reference += 1,
            DistortionBlocker::WeakReferenceGeometry => self.weak_reference_geometry += 1,
            DistortionBlocker::LowContrast => self.low_contrast += 1,
            DistortionBlocker::LineDiscontinuous => self.line_discontinuous += 1,
            DistortionBlocker::FitResidualTooHigh => self.fit_residual_too_high += 1,
            DistortionBlocker::ProfileTooShort => self.profile_too_short += 1,
            DistortionBlocker::InsufficientSamples => {}
        }
    }

    fn finish(self) -> Result<DistortionEvidence, DistortionError> {
        let included_samples = self.bow.len();
        let mut excluded = Vec::new();
        push_exclusion(
            &mut excluded,
            ExclusionReason::UnknownCorrections,
            self.unknown_corrections,
        );
        push_exclusion(
            &mut excluded,
            ExclusionReason::NoStraightReference,
            self.no_straight_reference,
        );
        push_exclusion(
            &mut excluded,
            ExclusionReason::WeakReferenceGeometry,
            self.weak_reference_geometry,
        );
        push_exclusion(
            &mut excluded,
            ExclusionReason::LowContrast,
            self.low_contrast,
        );
        push_exclusion(
            &mut excluded,
            ExclusionReason::LineDiscontinuous,
            self.line_discontinuous,
        );
        push_exclusion(
            &mut excluded,
            ExclusionReason::FitResidualTooHigh,
            self.fit_residual_too_high,
        );
        push_exclusion(
            &mut excluded,
            ExclusionReason::ProfileTooShort,
            self.profile_too_short,
        );
        let excluded_samples = excluded.iter().map(|count| count.count).sum();
        let mut blockers = Vec::new();
        if included_samples < 2 {
            blockers.push(DistortionBlocker::InsufficientSamples);
        }
        if self.unknown_corrections > 0 {
            blockers.push(DistortionBlocker::UnknownCorrections);
        }
        if self.no_straight_reference > 0 {
            blockers.push(DistortionBlocker::NoStraightReference);
        }
        if self.weak_reference_geometry > 0 {
            blockers.push(DistortionBlocker::WeakReferenceGeometry);
        }
        if self.low_contrast > 0 {
            blockers.push(DistortionBlocker::LowContrast);
        }
        if self.line_discontinuous > 0 {
            blockers.push(DistortionBlocker::LineDiscontinuous);
        }
        if self.fit_residual_too_high > 0 {
            blockers.push(DistortionBlocker::FitResidualTooHigh);
        }
        if self.profile_too_short > 0 {
            blockers.push(DistortionBlocker::ProfileTooShort);
        }

        Ok(DistortionEvidence {
            included_samples,
            excluded_samples,
            mean_bow: mean_bow(&self.bow)?,
            scatter: sample_std_bow(&self.bow)?,
            blockers,
            excluded,
        })
    }
}

fn validate_candidate(candidate: &DistortionCandidate) -> Result<(), DistortionError> {
    validate_derived([
        candidate.bow.value,
        candidate.bow.confidence,
        candidate.sagitta_px,
        candidate.span_coverage,
        candidate.fit_residual_px,
    ])
}

fn mean_bow(
    values: &[DistortionMeasurement],
) -> Result<Option<DistortionMeasurement>, DistortionError> {
    if values.is_empty() {
        return Ok(None);
    }
    #[allow(clippy::cast_precision_loss)]
    let len = values.len() as f32;
    distortion_measurement(values.iter().map(|value| value.value).sum::<f32>() / len).map(Some)
}

fn sample_std_bow(
    values: &[DistortionMeasurement],
) -> Result<Option<DistortionMeasurement>, DistortionError> {
    if values.len() < 2 {
        return Ok(None);
    }
    let mean = mean_bow(values)?
        .expect("mean exists for non-empty values")
        .value;
    let sum = values
        .iter()
        .map(|value| {
            let delta = value.value - mean;
            delta * delta
        })
        .sum::<f32>();
    #[allow(clippy::cast_precision_loss)]
    distortion_measurement((sum / (values.len() - 1) as f32).sqrt()).map(Some)
}

fn distortion_measurement(value: f32) -> Result<DistortionMeasurement, DistortionError> {
    DistortionMeasurement::measured_percent_frame(value)
        .ok_or(DistortionError::NonFiniteDerivedValue { value })
}

fn push_exclusion(excluded: &mut Vec<ExclusionCount>, reason: ExclusionReason, count: usize) {
    if count > 0 {
        excluded.push(ExclusionCount { reason, count });
    }
}

fn push_blocker(blockers: &mut Vec<DistortionBlocker>, blocker: DistortionBlocker) {
    if !blockers.contains(&blocker) {
        blockers.push(blocker);
    }
}

#[cfg(test)]
mod tests {
    use super::{DistortionError, aggregate_group_distortion, measure_distortion};
    use crate::image::{Dimensions, LinearImage, Rect};
    use crate::schema::{
        CaBlocker, CaLateralMeasurements, DistortionBlocker, DistortionCandidate,
        DistortionMeasurement, DistortionMeasurements, DistortionOrientation,
        DistortionReferenceSide, ExclusionReason, FrameMeasurement, Measurements,
        SharpnessMeasurements, VignettingMeasurements, VignettingZoneMeasurements, ZoneMeasurement,
        ZoneMeasurements,
    };

    fn plane(width: usize, height: usize, paint: impl Fn(usize, usize) -> f32) -> LinearImage {
        let dimensions = Dimensions::new(width, height).unwrap();
        let mut samples = Vec::with_capacity(dimensions.sample_count());
        for y in 0..height {
            for x in 0..width {
                samples.push(paint(x, y));
            }
        }
        LinearImage::new(dimensions, samples).unwrap()
    }

    fn view(image: &LinearImage) -> crate::image::LinearPatchView<'_> {
        let dimensions = image.dimensions();
        image
            .patch(Rect::new(0, 0, dimensions.width(), dimensions.height()).unwrap())
            .unwrap()
    }

    fn horizontal_line(width: usize, height: usize, base_y: f32, sagitta: f32) -> LinearImage {
        #[allow(clippy::cast_precision_loss)]
        plane(width, height, |x, y| {
            let t = x as f32 / (width - 1) as f32;
            let centre_bow = 4.0 * sagitta * t * (1.0 - t);
            let line_y = base_y + centre_bow;
            if (y as f32 - line_y).abs() <= 1.0 {
                0.05
            } else {
                0.85
            }
        })
    }

    fn vertical_line(width: usize, height: usize, base_x: f32, sagitta: f32) -> LinearImage {
        #[allow(clippy::cast_precision_loss)]
        plane(width, height, |x, y| {
            let t = y as f32 / (height - 1) as f32;
            let centre_bow = 4.0 * sagitta * t * (1.0 - t);
            let line_x = base_x + centre_bow;
            if (x as f32 - line_x).abs() <= 1.0 {
                0.05
            } else {
                0.85
            }
        })
    }

    fn short_horizontal_line(width: usize, height: usize) -> LinearImage {
        #[allow(clippy::cast_precision_loss)]
        plane(width, height, |x, y| {
            let start = width / 3;
            let end = width * 2 / 3;
            let t = (x.saturating_sub(start)) as f32 / (end - start) as f32;
            let line_y = 10.0 + 5.0 * 4.0 * t * (1.0 - t);
            if (start..=end).contains(&x) && (y as f32 - line_y).abs() <= 1.0 {
                0.05
            } else {
                0.85
            }
        })
    }

    fn discontinuous_line(width: usize, height: usize) -> LinearImage {
        #[allow(clippy::cast_precision_loss)]
        plane(width, height, |x, y| {
            if x > width / 3 && x < width / 2 {
                return 0.85;
            }
            let line_y = 10.0;
            if (y as f32 - line_y).abs() <= 1.0 {
                0.05
            } else {
                0.85
            }
        })
    }

    fn noisy_line(width: usize, height: usize) -> LinearImage {
        #[allow(clippy::cast_precision_loss)]
        plane(width, height, |x, y| {
            let line_y = 10.0 + if x % 9 == 0 { 8.0 } else { 0.0 };
            if (y as f32 - line_y).abs() <= 1.0 {
                0.05
            } else {
                0.85
            }
        })
    }

    fn bright_horizontal_line(
        width: usize,
        height: usize,
        base_y: f32,
        sagitta: f32,
    ) -> LinearImage {
        #[allow(clippy::cast_precision_loss)]
        plane(width, height, |x, y| {
            let t = x as f32 / (width - 1) as f32;
            let centre_bow = 4.0 * sagitta * t * (1.0 - t);
            let line_y = base_y + centre_bow;
            if (y as f32 - line_y).abs() <= 1.0 {
                0.85
            } else {
                0.05
            }
        })
    }

    fn paired_horizontal_lines(width: usize, height: usize) -> LinearImage {
        #[allow(clippy::cast_precision_loss)]
        plane(width, height, |x, y| {
            let t = x as f32 / (width - 1) as f32;
            let top_y = 8.0 - 4.0 * 4.0 * t * (1.0 - t);
            let bottom_y = height as f32 - 9.0 + 4.0 * 4.0 * t * (1.0 - t);
            if (y as f32 - top_y).abs() <= 1.0 || (y as f32 - bottom_y).abs() <= 1.0 {
                0.05
            } else {
                0.85
            }
        })
    }

    fn assert_close(actual: f32, expected: f32, tolerance: f32) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "actual {actual} expected {expected}"
        );
    }

    #[test]
    fn measures_horizontal_top_bow_as_positive_percent_of_frame_height() {
        let image = horizontal_line(80, 50, 8.0, -4.0);
        let evidence = measure_distortion(view(&image)).expect("measure");
        let candidate = evidence.candidate.expect("candidate");

        assert_eq!(candidate.orientation, DistortionOrientation::Horizontal);
        assert_eq!(candidate.reference_side, Some(DistortionReferenceSide::Top));
        assert_eq!(
            candidate.bow.method,
            crate::schema::DistortionMethod::MeasuredStraightLineBow
        );
        assert_close(candidate.bow.value, 8.0, 0.6);
        assert!(candidate.span_coverage > 0.9);
        assert!(candidate.fit_residual_px < 0.5);
        assert!(evidence.blockers.is_empty());
    }

    #[test]
    fn measures_vertical_right_bow_with_side_sign() {
        let image = vertical_line(60, 80, 50.0, 3.0);
        let evidence = measure_distortion(view(&image)).expect("measure");
        let candidate = evidence.candidate.expect("candidate");

        assert_eq!(candidate.orientation, DistortionOrientation::Vertical);
        assert_eq!(
            candidate.reference_side,
            Some(DistortionReferenceSide::Right)
        );
        assert_close(candidate.bow.value, 5.0, 0.6);
    }

    #[test]
    fn measures_bright_line_on_dark_background() {
        let image = bright_horizontal_line(80, 50, 8.0, -4.0);
        let evidence = measure_distortion(view(&image)).expect("measure");
        let candidate = evidence.candidate.expect("candidate");

        assert_eq!(candidate.orientation, DistortionOrientation::Horizontal);
        assert_eq!(candidate.reference_side, Some(DistortionReferenceSide::Top));
        assert_eq!(
            candidate.bow.method,
            crate::schema::DistortionMethod::MeasuredStraightLineBow
        );
        assert_close(candidate.bow.value, 8.0, 0.6);
    }

    #[test]
    fn paired_side_references_do_not_collapse_to_centre() {
        let image = paired_horizontal_lines(80, 50);
        let evidence = measure_distortion(view(&image)).expect("measure");
        let candidate = evidence.candidate.expect("candidate");

        assert_eq!(candidate.orientation, DistortionOrientation::Horizontal);
        assert!(candidate.reference_side.is_some());
        assert_eq!(
            candidate.bow.method,
            crate::schema::DistortionMethod::MeasuredStraightLineBow
        );
    }

    #[test]
    fn central_line_is_inferred_not_measured() {
        let image = horizontal_line(80, 50, 25.0, 3.0);
        let evidence = measure_distortion(view(&image)).expect("measure");
        let candidate = evidence.candidate.expect("candidate");

        assert_eq!(candidate.reference_side, None);
        assert_eq!(
            candidate.bow.method,
            crate::schema::DistortionMethod::InferredWeakReferenceBow
        );
        assert_eq!(
            evidence.blockers,
            vec![DistortionBlocker::WeakReferenceGeometry]
        );
    }

    #[test]
    fn short_traceable_line_is_inferred_weak_reference() {
        let image = short_horizontal_line(90, 50);
        let evidence = measure_distortion(view(&image)).expect("measure");
        let candidate = evidence.candidate.expect("candidate");

        assert_eq!(
            candidate.bow.method,
            crate::schema::DistortionMethod::InferredWeakReferenceBow
        );
        assert_eq!(
            evidence.blockers,
            vec![DistortionBlocker::WeakReferenceGeometry]
        );
    }

    #[test]
    fn no_reference_and_flat_profiles_block_without_zero_bow() {
        let image = plane(60, 40, |_, _| 0.5);
        let evidence = measure_distortion(view(&image)).expect("measure");

        assert!(evidence.candidate.is_none());
        assert_eq!(
            evidence.blockers,
            vec![DistortionBlocker::NoStraightReference]
        );
    }

    #[test]
    fn too_small_planes_report_profile_too_short() {
        let image = plane(15, 40, |_, _| 0.5);
        let evidence = measure_distortion(view(&image)).expect("measure");

        assert!(evidence.candidate.is_none());
        assert_eq!(evidence.blockers, vec![DistortionBlocker::ProfileTooShort]);
    }

    #[test]
    fn discontinuous_line_blocks_measurement() {
        let image = discontinuous_line(80, 50);
        let evidence = measure_distortion(view(&image)).expect("measure");

        assert!(evidence.candidate.is_none());
        assert!(
            evidence
                .blockers
                .contains(&DistortionBlocker::LineDiscontinuous)
        );
    }

    #[test]
    fn high_residual_line_blocks_measurement() {
        let image = noisy_line(80, 50);
        let evidence = measure_distortion(view(&image)).expect("measure");

        assert!(evidence.candidate.is_none());
        assert!(
            evidence
                .blockers
                .contains(&DistortionBlocker::FitResidualTooHigh)
        );
    }

    #[test]
    fn rejects_non_finite_samples() {
        let image = plane(60, 40, |x, y| if x == 1 && y == 1 { f32::NAN } else { 0.5 });

        assert!(matches!(
            measure_distortion(view(&image)),
            Err(DistortionError::NonFiniteSample { .. })
        ));
    }

    fn zone() -> ZoneMeasurement {
        ZoneMeasurement::measured(1.0, 0.2, 1.0, true).unwrap()
    }

    fn frame(eligible: bool, distortion: DistortionMeasurements) -> FrameMeasurement {
        let zone = zone();
        FrameMeasurement {
            input_index: 0,
            path: "frame.tif".to_owned(),
            aggregation_eligible: eligible,
            measurements: Measurements {
                sharpness: SharpnessMeasurements {
                    zones: ZoneMeasurements::from_ordered([
                        zone.clone(),
                        zone.clone(),
                        zone.clone(),
                        zone.clone(),
                        zone,
                    ]),
                },
                vignetting: VignettingMeasurements {
                    zones: VignettingZoneMeasurements {
                        top_left: crate::schema::CornerFalloff {
                            falloff: crate::schema::VignettingNumericMeasurement::measured_stops(
                                -1.0,
                            )
                            .unwrap(),
                        },
                        top_right: crate::schema::CornerFalloff {
                            falloff: crate::schema::VignettingNumericMeasurement::measured_stops(
                                -1.0,
                            )
                            .unwrap(),
                        },
                        bottom_left: crate::schema::CornerFalloff {
                            falloff: crate::schema::VignettingNumericMeasurement::measured_stops(
                                -1.0,
                            )
                            .unwrap(),
                        },
                        bottom_right: crate::schema::CornerFalloff {
                            falloff: crate::schema::VignettingNumericMeasurement::measured_stops(
                                -1.0,
                            )
                            .unwrap(),
                        },
                    },
                },
                ca_lateral: CaLateralMeasurements::blocked_all(CaBlocker::FlatProfile),
                distortion,
            },
        }
    }

    fn candidate(value: f32, method: crate::schema::DistortionMethod) -> DistortionCandidate {
        DistortionCandidate::new(
            DistortionOrientation::Horizontal,
            Some(DistortionReferenceSide::Top),
            DistortionMeasurement::percent_frame(value, method, 0.9).unwrap(),
            value,
            0.9,
            0.1,
        )
        .unwrap()
    }

    fn measured(value: f32) -> DistortionMeasurements {
        DistortionMeasurements {
            candidate: Some(candidate(
                value,
                crate::schema::DistortionMethod::MeasuredStraightLineBow,
            )),
            blockers: vec![],
        }
    }

    #[test]
    fn aggregate_group_distortion_reports_mean_and_scatter() {
        let evidence =
            aggregate_group_distortion(&[frame(true, measured(1.0)), frame(true, measured(3.0))])
                .expect("aggregate");

        assert_eq!(evidence.included_samples, 2);
        assert_close(evidence.mean_bow.unwrap().value, 2.0, 1.0e-6);
        assert_close(
            evidence.scatter.unwrap().value,
            std::f32::consts::SQRT_2,
            1.0e-6,
        );
        assert!(evidence.blockers.is_empty());
    }

    #[test]
    fn aggregate_group_distortion_marks_one_sample_as_insufficient_for_scatter() {
        let evidence =
            aggregate_group_distortion(&[frame(true, measured(1.0))]).expect("aggregate");

        assert_eq!(evidence.included_samples, 1);
        assert!(evidence.mean_bow.is_some());
        assert!(evidence.scatter.is_none());
        assert_eq!(
            evidence.blockers,
            vec![DistortionBlocker::InsufficientSamples]
        );
    }

    #[test]
    fn aggregate_group_distortion_excludes_inferred_and_unknown_corrections() {
        let inferred = DistortionMeasurements {
            candidate: Some(candidate(
                1.0,
                crate::schema::DistortionMethod::InferredWeakReferenceBow,
            )),
            blockers: vec![DistortionBlocker::WeakReferenceGeometry],
        };
        let evidence = aggregate_group_distortion(&[
            frame(true, inferred),
            frame(false, measured(2.0)),
            frame(
                true,
                DistortionMeasurements::blocked(DistortionBlocker::NoStraightReference),
            ),
        ])
        .expect("aggregate");

        assert_eq!(evidence.included_samples, 0);
        assert_eq!(evidence.excluded_samples, 3);
        assert_eq!(
            evidence.excluded[0].reason,
            ExclusionReason::UnknownCorrections
        );
        assert_eq!(
            evidence.excluded[1].reason,
            ExclusionReason::NoStraightReference
        );
        assert_eq!(
            evidence.excluded[2].reason,
            ExclusionReason::WeakReferenceGeometry
        );
        assert_eq!(
            evidence.blockers,
            vec![
                DistortionBlocker::InsufficientSamples,
                DistortionBlocker::UnknownCorrections,
                DistortionBlocker::NoStraightReference,
                DistortionBlocker::WeakReferenceGeometry,
            ]
        );
    }

    #[test]
    fn aggregate_group_distortion_rejects_non_finite_dto_before_exclusion() {
        let mut distortion = measured(1.0);
        distortion.candidate.as_mut().unwrap().bow.value = f32::NAN;

        assert!(matches!(
            aggregate_group_distortion(&[frame(false, distortion)]),
            Err(DistortionError::NonFiniteDerivedValue { .. })
        ));
    }
}
