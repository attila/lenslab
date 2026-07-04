use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::image::LinearPatchView;
use crate::schema::{
    FrameMeasurement, TargetQaEvidence, TargetQaMeasurement, TargetQaMethod, TargetQuality,
    TargetQualityBlocker, TargetQualityStatus, TiltAxis,
};

const MIN_DIMENSION: usize = 24;
const MIN_REFERENCES: usize = 5;
const MIN_CONTRAST: f32 = 0.18;
const MAX_PERIOD_VARIANCE: f32 = 0.18;
const MAX_GAP_MULTIPLIER: f32 = 1.8;
const AMBIGUOUS_AXIS_MARGIN: f32 = 0.2;
const MEASURED_CONFIDENCE: f32 = 0.8;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TargetQaError {
    NonFiniteSample { value: f32 },
    NonFiniteDerivedValue { value: f32 },
    MissingAssessedTargetField,
    InvalidAssessedTargetField,
}

impl Display for TargetQaError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteSample { value } => {
                write!(formatter, "non-finite target QA sample {value}")
            }
            Self::NonFiniteDerivedValue { value } => {
                write!(formatter, "non-finite target QA value {value}")
            }
            Self::MissingAssessedTargetField => {
                write!(formatter, "assessed target QA is missing required evidence")
            }
            Self::InvalidAssessedTargetField => {
                write!(formatter, "invalid assessed target QA evidence")
            }
        }
    }
}

impl Error for TargetQaError {}

pub fn measure_target_qa(plane: LinearPatchView<'_>) -> Result<TargetQaEvidence, TargetQaError> {
    let dimensions = plane.dimensions();
    if dimensions.width() < MIN_DIMENSION || dimensions.height() < MIN_DIMENSION {
        return Ok(TargetQaEvidence::blocked(
            TargetQualityBlocker::ProfileTooShort,
        ));
    }

    let profiles = extract_profiles(plane)?;
    classify_candidates([
        candidate(&profiles.rows, TiltAxis::Vertical),
        candidate(&profiles.columns, TiltAxis::Horizontal),
    ])
}

pub fn aggregate_target_quality(
    frames: &[FrameMeasurement],
) -> Result<TargetQuality, TargetQaError> {
    let mut aggregate = TargetQualityAccumulator::default();
    for frame in frames {
        aggregate.push(frame)?;
    }
    Ok(aggregate.finish())
}

#[derive(Default)]
struct TargetQualityAccumulator {
    passed: Option<FrameTarget>,
    gated: Option<FrameTarget>,
    assessed_frames: usize,
    blocked_frames: usize,
    blockers: Vec<TargetQualityBlocker>,
}

#[derive(Debug, Clone, Copy)]
struct FrameTarget {
    method: TargetQaMethod,
    measurement: TargetQaMeasurement,
    axis: TiltAxis,
}

impl TargetQualityAccumulator {
    fn push(&mut self, frame: &FrameMeasurement) -> Result<(), TargetQaError> {
        let target = &frame.qa.target;
        validate_frame_target(target)?;
        match target.status {
            TargetQualityStatus::Gated | TargetQualityStatus::Passed => {
                self.assessed_frames += 1;
                let frame_target = FrameTarget {
                    method: target
                        .method
                        .ok_or(TargetQaError::MissingAssessedTargetField)?,
                    measurement: target
                        .keystone_pct
                        .ok_or(TargetQaError::MissingAssessedTargetField)?,
                    axis: target
                        .tilt_axis
                        .ok_or(TargetQaError::MissingAssessedTargetField)?,
                };
                if !frame.aggregation_eligible {
                    push_blocker(&mut self.blockers, TargetQualityBlocker::UnknownCorrections);
                    self.blocked_frames += 1;
                    return Ok(());
                }
                if target.status == TargetQualityStatus::Gated {
                    push_blocker(
                        &mut self.blockers,
                        TargetQualityBlocker::KeystoneAboveThreshold,
                    );
                    self.gated = Some(strongest(self.gated, frame_target));
                } else {
                    self.passed = Some(strongest(self.passed, frame_target));
                }
            }
            TargetQualityStatus::Blocked => {
                self.blocked_frames += 1;
                if !frame.aggregation_eligible {
                    push_blocker(&mut self.blockers, TargetQualityBlocker::UnknownCorrections);
                }
                for blocker in &target.blockers {
                    push_blocker(&mut self.blockers, *blocker);
                }
            }
            TargetQualityStatus::NotAssessed => {
                push_blocker(
                    &mut self.blockers,
                    TargetQualityBlocker::KeystoneNotAssessed,
                );
            }
        }
        Ok(())
    }

    fn finish(self) -> TargetQuality {
        if let Some(target) = self.gated {
            return TargetQuality::assessed(
                TargetQualityStatus::Gated,
                target.method,
                target.measurement,
                target.axis,
                self.assessed_frames,
                self.blocked_frames,
                self.blockers,
            );
        }
        if let Some(target) = self.passed {
            return TargetQuality::assessed(
                TargetQualityStatus::Passed,
                target.method,
                target.measurement,
                target.axis,
                self.assessed_frames,
                self.blocked_frames,
                self.blockers,
            );
        }
        if self.blocked_frames > 0 || !self.blockers.is_empty() {
            return TargetQuality::blocked(
                self.assessed_frames,
                self.blocked_frames,
                if self.blockers.is_empty() {
                    vec![TargetQualityBlocker::NoSuitableTargetReference]
                } else {
                    self.blockers
                },
            );
        }
        TargetQuality::not_assessed()
    }
}

fn strongest(current: Option<FrameTarget>, candidate: FrameTarget) -> FrameTarget {
    match current {
        Some(current) if current.measurement.value >= candidate.measurement.value => current,
        _ => candidate,
    }
}

fn validate_frame_target(target: &TargetQaEvidence) -> Result<(), TargetQaError> {
    if !target.gate_threshold_pct.is_finite() {
        return Err(TargetQaError::NonFiniteDerivedValue {
            value: target.gate_threshold_pct,
        });
    }
    let Some(measurement) = target.keystone_pct else {
        return Ok(());
    };
    if !measurement.value.is_finite() {
        return Err(TargetQaError::NonFiniteDerivedValue {
            value: measurement.value,
        });
    }
    if !measurement.confidence.is_finite() {
        return Err(TargetQaError::NonFiniteDerivedValue {
            value: measurement.confidence,
        });
    }
    if !(0.0..=1.0).contains(&measurement.confidence) {
        return Err(TargetQaError::InvalidAssessedTargetField);
    }
    if let Some(method) = target.method
        && method != measurement.method
    {
        return Err(TargetQaError::InvalidAssessedTargetField);
    }
    match target.status {
        TargetQualityStatus::Passed if measurement.value > TargetQuality::GATE_THRESHOLD_PCT => {
            Err(TargetQaError::InvalidAssessedTargetField)
        }
        TargetQualityStatus::Gated if measurement.value <= TargetQuality::GATE_THRESHOLD_PCT => {
            Err(TargetQaError::InvalidAssessedTargetField)
        }
        _ => Ok(()),
    }
}

#[derive(Debug)]
struct Profiles {
    rows: Vec<f32>,
    columns: Vec<f32>,
}

fn extract_profiles(plane: LinearPatchView<'_>) -> Result<Profiles, TargetQaError> {
    let dimensions = plane.dimensions();
    let mut rows = Vec::with_capacity(dimensions.height());
    let mut columns = vec![0.0; dimensions.width()];
    for row in plane.rows() {
        let mut row_sum = 0.0;
        for (x, value) in row.iter().enumerate() {
            if !value.is_finite() {
                return Err(TargetQaError::NonFiniteSample { value: *value });
            }
            row_sum += *value;
            columns[x] += *value;
        }
        #[allow(clippy::cast_precision_loss)]
        let width = dimensions.width() as f32;
        rows.push(row_sum / width);
    }
    #[allow(clippy::cast_precision_loss)]
    let height = dimensions.height() as f32;
    for value in &mut columns {
        *value /= height;
    }
    Ok(Profiles { rows, columns })
}

#[derive(Debug, Clone, Copy)]
struct Candidate {
    axis: TiltAxis,
    keystone_pct: f32,
    contrast: f32,
}

fn candidate(profile: &[f32], axis: TiltAxis) -> Result<Candidate, TargetQualityBlocker> {
    let (min_value, max_value) = min_max(profile);
    let contrast = max_value - min_value;
    if contrast < MIN_CONTRAST {
        return Err(TargetQualityBlocker::LowContrast);
    }

    let dark = reference_centres(profile, min_value + contrast * 0.35, true);
    let bright = reference_centres(profile, max_value - contrast * 0.35, false);
    let centres = if dark.len() >= bright.len() {
        dark
    } else {
        bright
    };
    if centres.len() < MIN_REFERENCES {
        return Err(TargetQualityBlocker::NoSuitableTargetReference);
    }
    let periods = periods(&centres);
    if periods
        .iter()
        .any(|period| *period <= 0.0 || !period.is_finite())
    {
        return Err(TargetQualityBlocker::WeakTargetGeometry);
    }
    let mean_period = mean(&periods);
    if mean_period < 3.0 {
        return Err(TargetQualityBlocker::WeakTargetGeometry);
    }
    let largest_period = periods.iter().copied().fold(0.0, f32::max);
    if largest_period > mean_period * MAX_GAP_MULTIPLIER {
        return Err(TargetQualityBlocker::LineDiscontinuous);
    }
    if coefficient_of_variation(&periods, mean_period) > MAX_PERIOD_VARIANCE {
        return Err(TargetQualityBlocker::WeakTargetGeometry);
    }

    let half = periods.len() / 2;
    if half == 0 {
        return Err(TargetQualityBlocker::WeakTargetGeometry);
    }
    let near = mean(&periods[..half]);
    let far = mean(&periods[periods.len() - half..]);
    let scale_mean = f32::midpoint(near, far);
    let keystone_pct = ((far - near).abs() / scale_mean) * 100.0;
    if !keystone_pct.is_finite() {
        return Err(TargetQualityBlocker::WeakTargetGeometry);
    }
    Ok(Candidate {
        axis,
        keystone_pct,
        contrast,
    })
}

fn classify_candidates(
    candidates: [Result<Candidate, TargetQualityBlocker>; 2],
) -> Result<TargetQaEvidence, TargetQaError> {
    let mut blockers = Vec::new();
    let mut supported = Vec::new();
    for candidate in candidates {
        match candidate {
            Ok(candidate) => supported.push(candidate),
            Err(blocker) => push_blocker(&mut blockers, blocker),
        }
    }
    supported.sort_by(|left, right| {
        right
            .keystone_pct
            .total_cmp(&left.keystone_pct)
            .then_with(|| right.contrast.total_cmp(&left.contrast))
    });
    if supported.len() > 1 {
        let strongest = supported[0].keystone_pct;
        let next = supported[1].keystone_pct;
        let difference = (strongest - next).abs();
        if strongest == 0.0 || difference / strongest <= AMBIGUOUS_AXIS_MARGIN {
            return Ok(TargetQaEvidence::blocked(
                TargetQualityBlocker::AmbiguousTiltAxis,
            ));
        }
    }
    if let Some(candidate) = supported.first().copied() {
        let measurement = TargetQaMeasurement::measured_percent(
            candidate.keystone_pct,
            TargetQaMethod::MeasuredPeriodicReferenceScale,
            MEASURED_CONFIDENCE,
        )
        .ok_or(TargetQaError::NonFiniteDerivedValue {
            value: candidate.keystone_pct,
        })?;
        if candidate.keystone_pct > TargetQuality::GATE_THRESHOLD_PCT {
            return Ok(TargetQaEvidence::gated(
                TargetQaMethod::MeasuredPeriodicReferenceScale,
                measurement,
                candidate.axis,
            ));
        }
        return Ok(TargetQaEvidence::passed(
            TargetQaMethod::MeasuredPeriodicReferenceScale,
            measurement,
            candidate.axis,
        ));
    }
    if blockers.is_empty() {
        blockers.push(TargetQualityBlocker::NoSuitableTargetReference);
    }
    Ok(TargetQaEvidence::blocked_with(blockers))
}

fn min_max(samples: &[f32]) -> (f32, f32) {
    samples
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), value| {
            (min.min(*value), max.max(*value))
        })
}

fn reference_centres(profile: &[f32], threshold: f32, dark: bool) -> Vec<f32> {
    let mut centres = Vec::new();
    let mut start = None;
    let mut weighted_sum = 0.0;
    let mut weight_total = 0.0;
    for (index, value) in profile.iter().enumerate() {
        let weight = if dark {
            (threshold - *value).max(0.0)
        } else {
            (*value - threshold).max(0.0)
        };
        if weight > 0.0 {
            start.get_or_insert(index);
            #[allow(clippy::cast_precision_loss)]
            {
                weighted_sum += index as f32 * weight;
            }
            weight_total += weight;
        } else if let Some(run_start) = start.take() {
            push_centre(&mut centres, run_start, index, weighted_sum, weight_total);
            weighted_sum = 0.0;
            weight_total = 0.0;
        }
    }
    if let Some(run_start) = start {
        push_centre(
            &mut centres,
            run_start,
            profile.len(),
            weighted_sum,
            weight_total,
        );
    }
    centres
}

fn push_centre(
    centres: &mut Vec<f32>,
    start: usize,
    end: usize,
    weighted_sum: f32,
    weight_total: f32,
) {
    if end - start < 2 || weight_total == 0.0 {
        return;
    }
    centres.push(weighted_sum / weight_total);
}

fn periods(centres: &[f32]) -> Vec<f32> {
    centres.windows(2).map(|pair| pair[1] - pair[0]).collect()
}

fn mean(samples: &[f32]) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let len = samples.len() as f32;
    samples.iter().sum::<f32>() / len
}

fn coefficient_of_variation(samples: &[f32], mean: f32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let len = samples.len() as f32;
    let variance = samples
        .iter()
        .map(|sample| {
            let delta = sample - mean;
            delta * delta
        })
        .sum::<f32>()
        / len;
    variance.sqrt() / mean
}

fn push_blocker(blockers: &mut Vec<TargetQualityBlocker>, blocker: TargetQualityBlocker) {
    if !blockers.contains(&blocker) {
        blockers.push(blocker);
    }
}

#[cfg(test)]
mod tests {
    use super::{TargetQaError, measure_target_qa};
    use crate::image::{Dimensions, LinearImage, Rect};
    use crate::schema::{TargetQualityBlocker, TargetQualityStatus, TiltAxis};

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

    fn horizontal_courses(top_period: usize, bottom_period: usize) -> LinearImage {
        plane(96, 96, |x, y| {
            let period = if y < 48 { top_period } else { bottom_period };
            if (y % period) < 3 {
                0.05
            } else {
                0.85 + f32::from(u16::try_from(x % 3).unwrap()) * 0.01
            }
        })
    }

    fn vertical_courses(left_period: usize, right_period: usize) -> LinearImage {
        plane(96, 96, |x, y| {
            let period = if x < 48 { left_period } else { right_period };
            if (x % period) < 3 {
                0.05
            } else {
                0.85 + f32::from(u16::try_from(y % 3).unwrap()) * 0.01
            }
        })
    }

    #[test]
    fn horizontal_period_scale_above_threshold_gates_vertical_tilt() {
        let evidence = measure_target_qa(view(&horizontal_courses(12, 16))).unwrap();

        assert_eq!(evidence.status, TargetQualityStatus::Gated);
        assert_eq!(evidence.tilt_axis, Some(TiltAxis::Vertical));
        assert!(evidence.keystone_pct.unwrap().value > 1.5);
    }

    #[test]
    fn matching_horizontal_periods_pass_vertical_tilt_gate() {
        let evidence = measure_target_qa(view(&horizontal_courses(12, 12))).unwrap();

        assert_eq!(evidence.status, TargetQualityStatus::Passed);
        assert_eq!(evidence.tilt_axis, Some(TiltAxis::Vertical));
        assert!(evidence.keystone_pct.unwrap().value <= 1.5);
    }

    #[test]
    fn vertical_period_scale_reports_horizontal_tilt_axis() {
        let evidence = measure_target_qa(view(&vertical_courses(12, 16))).unwrap();

        assert_eq!(evidence.status, TargetQualityStatus::Gated);
        assert_eq!(evidence.tilt_axis, Some(TiltAxis::Horizontal));
    }

    #[test]
    fn no_reference_geometry_blocks_without_keystone() {
        let image = plane(96, 96, |x, y| {
            0.4 + f32::from(u16::try_from((x + y) % 3).unwrap()) * 0.001
        });
        let evidence = measure_target_qa(view(&image)).unwrap();

        assert_eq!(evidence.status, TargetQualityStatus::Blocked);
        assert!(evidence.keystone_pct.is_none());
        assert!(evidence.tilt_axis.is_none());
        assert!(
            evidence
                .blockers
                .contains(&TargetQualityBlocker::LowContrast)
        );
    }

    #[test]
    fn too_few_references_block_without_keystone() {
        let image = plane(96, 96, |_, y| {
            if (8..=10).contains(&y) || (24..=26).contains(&y) || (40..=42).contains(&y) {
                0.0
            } else {
                1.0
            }
        });
        let evidence = measure_target_qa(view(&image)).unwrap();

        assert_eq!(evidence.status, TargetQualityStatus::Blocked);
        assert!(evidence.keystone_pct.is_none());
        assert!(
            evidence
                .blockers
                .contains(&TargetQualityBlocker::NoSuitableTargetReference)
        );
    }

    #[test]
    fn high_variance_reference_periods_block_without_keystone() {
        let image = plane(96, 96, |_, y| {
            if (4..=6).contains(&y)
                || (14..=16).contains(&y)
                || (32..=34).contains(&y)
                || (44..=46).contains(&y)
                || (68..=70).contains(&y)
                || (82..=84).contains(&y)
            {
                0.0
            } else {
                1.0
            }
        });
        let evidence = measure_target_qa(view(&image)).unwrap();

        assert_eq!(evidence.status, TargetQualityStatus::Blocked);
        assert!(evidence.keystone_pct.is_none());
        assert!(
            evidence
                .blockers
                .contains(&TargetQualityBlocker::WeakTargetGeometry)
        );
    }

    #[test]
    fn short_profile_blocks_without_keystone() {
        let image = plane(16, 16, |_, y| if y % 4 == 0 { 0.0 } else { 1.0 });
        let evidence = measure_target_qa(view(&image)).unwrap();

        assert_eq!(evidence.status, TargetQualityStatus::Blocked);
        assert_eq!(
            evidence.blockers,
            vec![TargetQualityBlocker::ProfileTooShort]
        );
    }

    #[test]
    fn discontinuous_reference_blocks_without_keystone() {
        let image = plane(96, 128, |_, y| {
            if (4..=5).contains(&y)
                || (16..=17).contains(&y)
                || (28..=29).contains(&y)
                || (40..=41).contains(&y)
                || (112..=113).contains(&y)
            {
                0.0
            } else {
                1.0
            }
        });
        let evidence = measure_target_qa(view(&image)).unwrap();

        assert_eq!(evidence.status, TargetQualityStatus::Blocked);
        assert!(
            evidence
                .blockers
                .contains(&TargetQualityBlocker::LineDiscontinuous)
        );
    }

    #[test]
    fn ambiguous_supported_axes_block_without_axis_choice() {
        let image = plane(96, 96, |x, y| {
            let x_period = if x < 48 { 12 } else { 16 };
            let y_period = if y < 48 { 12 } else { 16 };
            if (x % x_period) < 3 || (y % y_period) < 3 {
                0.05
            } else {
                0.85
            }
        });
        let evidence = measure_target_qa(view(&image)).unwrap();

        assert_eq!(evidence.status, TargetQualityStatus::Blocked);
        assert_eq!(
            evidence.blockers,
            vec![TargetQualityBlocker::AmbiguousTiltAxis]
        );
    }

    #[test]
    fn zero_keystone_supported_axes_are_ambiguous() {
        let image = plane(96, 96, |x, y| {
            if (x % 12) < 3 || (y % 12) < 3 {
                0.05
            } else {
                0.85
            }
        });
        let evidence = measure_target_qa(view(&image)).unwrap();

        assert_eq!(evidence.status, TargetQualityStatus::Blocked);
        assert_eq!(
            evidence.blockers,
            vec![TargetQualityBlocker::AmbiguousTiltAxis]
        );
    }

    #[test]
    fn non_finite_samples_are_errors_before_blockers() {
        let image = plane(96, 96, |x, y| if x == 1 && y == 1 { f32::NAN } else { 0.5 });
        let err = measure_target_qa(view(&image)).expect_err("non-finite sample rejected");

        assert!(matches!(err, TargetQaError::NonFiniteSample { .. }));
    }
}
