use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::image::LinearPatchView;
use crate::schema::{
    CaBlocker, CaCornerSummary, CaLateralEvidence, CaMeasurement, CaShift, CaZoneEvidence,
    ExclusionCount, ExclusionReason, FrameMeasurement,
};

const MIN_PROFILE_LEN: usize = 5;
const MIN_PROFILE_VARIANCE: f32 = 1.0e-8;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CaError {
    NonFiniteSample { value: f32 },
    NonFiniteDerivedValue { value: f32 },
}

impl Display for CaError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteSample { value } => write!(formatter, "non-finite CA sample {value}"),
            Self::NonFiniteDerivedValue { value } => {
                write!(formatter, "non-finite CA value {value}")
            }
        }
    }
}

impl Error for CaError {}

pub fn measure_lateral_ca(
    red: LinearPatchView<'_>,
    blue: LinearPatchView<'_>,
    full_resolution_scale: f32,
) -> Result<CaZoneEvidence, CaError> {
    let red_dimensions = red.dimensions();
    let blue_dimensions = blue.dimensions();
    if red_dimensions != blue_dimensions
        || red_dimensions.width() < MIN_PROFILE_LEN
        || red_dimensions.height() < MIN_PROFILE_LEN
    {
        return Ok(CaZoneEvidence::blocked(CaBlocker::ProfileTooShort));
    }

    let red_x = column_profile(red)?;
    let blue_x = column_profile(blue)?;
    let red_y = row_profile(red)?;
    let blue_y = row_profile(blue)?;
    let x = match measure_profile_shift(&red_x, &blue_x)? {
        ProfileShift::Measured(value) => value * full_resolution_scale,
        ProfileShift::Blocked(blocker) => return Ok(CaZoneEvidence::blocked(blocker)),
    };
    let y = match measure_profile_shift(&red_y, &blue_y)? {
        ProfileShift::Measured(value) => value * full_resolution_scale,
        ProfileShift::Blocked(blocker) => return Ok(CaZoneEvidence::blocked(blocker)),
    };
    let magnitude = x.hypot(y);
    let shift = CaShift {
        x: ca_measurement(x)?,
        y: ca_measurement(y)?,
        magnitude: ca_measurement(magnitude)?,
    };
    Ok(CaZoneEvidence::measured(shift))
}

pub fn aggregate_group_ca(frames: &[FrameMeasurement]) -> Result<CaLateralEvidence, CaError> {
    let mut top_left = CornerAccumulator::default();
    let mut top_right = CornerAccumulator::default();
    let mut bottom_left = CornerAccumulator::default();
    let mut bottom_right = CornerAccumulator::default();

    for frame in frames {
        let zones = frame.measurements.ca_lateral.zones.values();
        top_left.push(frame.aggregation_eligible, zones.top_left)?;
        top_right.push(frame.aggregation_eligible, zones.top_right)?;
        bottom_left.push(frame.aggregation_eligible, zones.bottom_left)?;
        bottom_right.push(frame.aggregation_eligible, zones.bottom_right)?;
    }

    Ok(CaLateralEvidence {
        top_left: top_left.finish()?,
        top_right: top_right.finish()?,
        bottom_left: bottom_left.finish()?,
        bottom_right: bottom_right.finish()?,
    })
}

#[derive(Default)]
struct CornerAccumulator {
    shifts: Vec<CaShift>,
    unknown_corrections: usize,
    flat_profile: usize,
    correlation_peak_not_found: usize,
    profile_too_short: usize,
    low_texture: usize,
}

impl CornerAccumulator {
    fn push(
        &mut self,
        aggregation_eligible: bool,
        evidence: &CaZoneEvidence,
    ) -> Result<(), CaError> {
        if let Some(shift) = evidence.shift {
            validate_shift(shift)?;
            if aggregation_eligible {
                self.shifts.push(shift);
            } else {
                self.unknown_corrections += 1;
            }
            return Ok(());
        }
        if !aggregation_eligible {
            self.unknown_corrections += 1;
            return Ok(());
        }

        for blocker in &evidence.blockers {
            match blocker {
                CaBlocker::UnknownCorrections => self.unknown_corrections += 1,
                CaBlocker::LowTexture => self.low_texture += 1,
                CaBlocker::FlatProfile => self.flat_profile += 1,
                CaBlocker::CorrelationPeakNotFound => self.correlation_peak_not_found += 1,
                CaBlocker::ProfileTooShort => self.profile_too_short += 1,
                CaBlocker::InsufficientSamples => {}
            }
        }
        Ok(())
    }

    fn finish(self) -> Result<CaCornerSummary, CaError> {
        let included_samples = self.shifts.len();
        let mut excluded = Vec::new();
        push_exclusion(
            &mut excluded,
            ExclusionReason::UnknownCorrections,
            self.unknown_corrections,
        );
        push_exclusion(&mut excluded, ExclusionReason::LowTexture, self.low_texture);
        push_exclusion(
            &mut excluded,
            ExclusionReason::FlatProfile,
            self.flat_profile,
        );
        push_exclusion(
            &mut excluded,
            ExclusionReason::CorrelationPeakNotFound,
            self.correlation_peak_not_found,
        );
        push_exclusion(
            &mut excluded,
            ExclusionReason::ProfileTooShort,
            self.profile_too_short,
        );
        let excluded_samples = excluded.iter().map(|count| count.count).sum();
        let mut blockers = Vec::new();
        if included_samples < 2 {
            blockers.push(CaBlocker::InsufficientSamples);
        }
        if self.unknown_corrections > 0 {
            blockers.push(CaBlocker::UnknownCorrections);
        }
        if self.low_texture > 0 {
            blockers.push(CaBlocker::LowTexture);
        }
        if self.flat_profile > 0 {
            blockers.push(CaBlocker::FlatProfile);
        }
        if self.correlation_peak_not_found > 0 {
            blockers.push(CaBlocker::CorrelationPeakNotFound);
        }
        if self.profile_too_short > 0 {
            blockers.push(CaBlocker::ProfileTooShort);
        }

        Ok(CaCornerSummary {
            included_samples,
            excluded_samples,
            mean_shift: mean_shift(&self.shifts)?,
            scatter: sample_std_shift(&self.shifts)?,
            blockers,
            excluded,
        })
    }
}

fn push_exclusion(excluded: &mut Vec<ExclusionCount>, reason: ExclusionReason, count: usize) {
    if count > 0 {
        excluded.push(ExclusionCount { reason, count });
    }
}

fn validate_shift(shift: CaShift) -> Result<(), CaError> {
    for value in [shift.x.value, shift.y.value, shift.magnitude.value] {
        if !value.is_finite() {
            return Err(CaError::NonFiniteDerivedValue { value });
        }
    }
    Ok(())
}

fn mean_shift(shifts: &[CaShift]) -> Result<Option<CaShift>, CaError> {
    if shifts.is_empty() {
        return Ok(None);
    }
    #[allow(clippy::cast_precision_loss)]
    let len = shifts.len() as f32;
    shift_from_values(
        shifts.iter().map(|shift| shift.x.value).sum::<f32>() / len,
        shifts.iter().map(|shift| shift.y.value).sum::<f32>() / len,
        shifts
            .iter()
            .map(|shift| shift.magnitude.value)
            .sum::<f32>()
            / len,
    )
    .map(Some)
}

fn sample_std_shift(shifts: &[CaShift]) -> Result<Option<CaShift>, CaError> {
    if shifts.len() < 2 {
        return Ok(None);
    }
    let mean = mean_shift(shifts)?.expect("two samples have a mean");
    #[allow(clippy::cast_precision_loss)]
    let denominator = (shifts.len() - 1) as f32;
    let std = |value: fn(CaShift) -> f32, mean: f32| {
        (shifts
            .iter()
            .map(|shift| {
                let delta = value(*shift) - mean;
                delta * delta
            })
            .sum::<f32>()
            / denominator)
            .sqrt()
    };
    shift_from_values(
        std(|shift| shift.x.value, mean.x.value),
        std(|shift| shift.y.value, mean.y.value),
        std(|shift| shift.magnitude.value, mean.magnitude.value),
    )
    .map(Some)
}

fn shift_from_values(x: f32, y: f32, magnitude: f32) -> Result<CaShift, CaError> {
    Ok(CaShift {
        x: ca_measurement(x)?,
        y: ca_measurement(y)?,
        magnitude: ca_measurement(magnitude)?,
    })
}

fn ca_measurement(value: f32) -> Result<CaMeasurement, CaError> {
    CaMeasurement::measured_channel_correlation(value)
        .ok_or(CaError::NonFiniteDerivedValue { value })
}

enum ProfileShift {
    Measured(f32),
    Blocked(CaBlocker),
}

fn measure_profile_shift(red: &[f32], blue: &[f32]) -> Result<ProfileShift, CaError> {
    if red.len() < MIN_PROFILE_LEN || blue.len() < MIN_PROFILE_LEN {
        return Ok(ProfileShift::Blocked(CaBlocker::ProfileTooShort));
    }
    if variance(red)? < MIN_PROFILE_VARIANCE || variance(blue)? < MIN_PROFILE_VARIANCE {
        return Ok(ProfileShift::Blocked(CaBlocker::FlatProfile));
    }

    let max_shift = (red.len().min(blue.len()) / 4).clamp(1, 12);
    let max_shift = isize::try_from(max_shift).expect("profile shift window fits in isize");
    let mut best_shift: isize = 0;
    let mut best_score = f32::NEG_INFINITY;
    for shift in -max_shift..=max_shift {
        let Some(score) = correlation_score(red, blue, shift)? else {
            continue;
        };
        let score_order = score.total_cmp(&best_score);
        if score_order == std::cmp::Ordering::Greater
            || (score_order == std::cmp::Ordering::Equal && tie_breaks_shift(shift, best_shift))
        {
            best_score = score;
            best_shift = shift;
        }
    }
    if !best_score.is_finite() {
        return Ok(ProfileShift::Blocked(CaBlocker::CorrelationPeakNotFound));
    }

    let refined = refine_peak(red, blue, best_shift)?;
    Ok(ProfileShift::Measured(refined))
}

fn tie_breaks_shift(candidate: isize, current: isize) -> bool {
    candidate.abs() < current.abs() || (candidate.abs() == current.abs() && candidate < current)
}

fn refine_peak(red: &[f32], blue: &[f32], shift: isize) -> Result<f32, CaError> {
    let Some(left) = correlation_score(red, blue, shift - 1)? else {
        return Ok(shift_to_f32(shift));
    };
    let Some(centre) = correlation_score(red, blue, shift)? else {
        return Ok(shift_to_f32(shift));
    };
    let Some(right) = correlation_score(red, blue, shift + 1)? else {
        return Ok(shift_to_f32(shift));
    };
    let denominator = left - (2.0 * centre) + right;
    if denominator.abs() <= f32::EPSILON {
        return Ok(shift_to_f32(shift));
    }
    let offset = 0.5 * (left - right) / denominator;
    let refined = shift_to_f32(shift) + offset.clamp(-0.5, 0.5);
    if refined.is_finite() {
        Ok(refined)
    } else {
        Err(CaError::NonFiniteDerivedValue { value: refined })
    }
}

fn shift_to_f32(shift: isize) -> f32 {
    f32::from(i16::try_from(shift).expect("CA shift window fits in i16"))
}

fn correlation_score(red: &[f32], blue: &[f32], shift: isize) -> Result<Option<f32>, CaError> {
    let mut red_samples = Vec::new();
    let mut blue_samples = Vec::new();
    for (index, red_sample) in red.iter().copied().enumerate() {
        let Some(blue_index) = index.checked_add_signed(-shift) else {
            continue;
        };
        let Some(blue_sample) = blue.get(blue_index).copied() else {
            continue;
        };
        red_samples.push(red_sample);
        blue_samples.push(blue_sample);
    }
    if red_samples.len() < MIN_PROFILE_LEN {
        return Ok(None);
    }
    pearson(&red_samples, &blue_samples)
}

fn pearson(left: &[f32], right: &[f32]) -> Result<Option<f32>, CaError> {
    let left_mean = mean(left)?;
    let right_mean = mean(right)?;
    let mut numerator = 0.0;
    let mut left_energy = 0.0;
    let mut right_energy = 0.0;
    for (left, right) in left.iter().copied().zip(right.iter().copied()) {
        let left_delta = left - left_mean;
        let right_delta = right - right_mean;
        numerator += left_delta * right_delta;
        left_energy += left_delta * left_delta;
        right_energy += right_delta * right_delta;
    }
    if left_energy <= MIN_PROFILE_VARIANCE || right_energy <= MIN_PROFILE_VARIANCE {
        return Ok(None);
    }
    let score = numerator / (left_energy.sqrt() * right_energy.sqrt());
    if score.is_finite() {
        Ok(Some(score))
    } else {
        Err(CaError::NonFiniteDerivedValue { value: score })
    }
}

fn column_profile(patch: LinearPatchView<'_>) -> Result<Vec<f32>, CaError> {
    let dimensions = patch.dimensions();
    let mut profile = vec![0.0; dimensions.width()];
    for row in patch.rows() {
        for (index, sample) in row.iter().copied().enumerate() {
            if !sample.is_finite() {
                return Err(CaError::NonFiniteSample { value: sample });
            }
            profile[index] += sample;
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let height = dimensions.height() as f32;
    for sample in &mut profile {
        *sample /= height;
    }
    Ok(profile)
}

fn row_profile(patch: LinearPatchView<'_>) -> Result<Vec<f32>, CaError> {
    let dimensions = patch.dimensions();
    let mut profile = Vec::with_capacity(dimensions.height());
    #[allow(clippy::cast_precision_loss)]
    let width = dimensions.width() as f32;
    for row in patch.rows() {
        let mut sum = 0.0;
        for sample in row {
            if !sample.is_finite() {
                return Err(CaError::NonFiniteSample { value: *sample });
            }
            sum += *sample;
        }
        profile.push(sum / width);
    }
    Ok(profile)
}

fn mean(samples: &[f32]) -> Result<f32, CaError> {
    let mut sum = 0.0;
    for sample in samples {
        if !sample.is_finite() {
            return Err(CaError::NonFiniteSample { value: *sample });
        }
        sum += *sample;
    }
    #[allow(clippy::cast_precision_loss)]
    Ok(sum / samples.len() as f32)
}

fn variance(samples: &[f32]) -> Result<f32, CaError> {
    let mean = mean(samples)?;
    #[allow(clippy::cast_precision_loss)]
    let len = samples.len() as f32;
    Ok(samples
        .iter()
        .map(|sample| {
            let delta = sample - mean;
            delta * delta
        })
        .sum::<f32>()
        / len)
}

#[cfg(test)]
mod tests {
    use super::{CaBlocker, CaError, aggregate_group_ca, measure_lateral_ca};
    use crate::image::{Dimensions, LinearImage, Rect};
    use crate::schema::{
        CaLateralMeasurements, CaMeasurement, CaShift, CaZoneEvidence, CaZoneMeasurements,
        ExclusionReason, FrameMeasurement, Measurements, SharpnessMeasurements,
        VignettingMeasurements, VignettingZoneMeasurements, ZoneMeasurement, ZoneMeasurements,
    };

    fn image(width: usize, height: usize, samples: Vec<f32>) -> LinearImage {
        LinearImage::new(Dimensions::new(width, height).unwrap(), samples).unwrap()
    }

    fn shifted_pair(x_shift: isize, y_shift: isize) -> (LinearImage, LinearImage) {
        let width = 24;
        let height = 24;
        let mut blue = Vec::with_capacity(width * height);
        let mut red = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                blue.push(texture(x, y));
                red.push(texture(
                    x.checked_add_signed(-x_shift).unwrap_or(x),
                    y.checked_add_signed(-y_shift).unwrap_or(y),
                ));
            }
        }
        (image(width, height, red), image(width, height, blue))
    }

    fn texture(x: usize, y: usize) -> f32 {
        let x = f32::from(u16::try_from(x).expect("test x fits in u16"));
        let y = f32::from(u16::try_from(y).expect("test y fits in u16"));
        (x * 0.37).sin() + (y * 0.23).cos() + x * 0.01 + y * 0.02
    }

    fn measure(red: &LinearImage, blue: &LinearImage) -> crate::schema::CaZoneEvidence {
        let rect = Rect::new(0, 0, red.dimensions().width(), red.dimensions().height()).unwrap();
        measure_lateral_ca(red.patch(rect).unwrap(), blue.patch(rect).unwrap(), 1.0).unwrap()
    }

    fn assert_close(actual: f32, expected: f32, tolerance: f32) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "actual {actual} expected {expected}"
        );
    }

    fn ca_shift(x: f32, y: f32, magnitude: f32) -> CaShift {
        CaShift {
            x: CaMeasurement::measured_channel_correlation(x).unwrap(),
            y: CaMeasurement::measured_channel_correlation(y).unwrap(),
            magnitude: CaMeasurement::measured_channel_correlation(magnitude).unwrap(),
        }
    }

    fn ca_frame(eligible: bool, evidence: CaZoneEvidence) -> FrameMeasurement {
        let zone = ZoneMeasurement::measured(1.0, 0.2, 1.0, eligible).unwrap();
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
                ca_lateral: CaLateralMeasurements {
                    zones: CaZoneMeasurements {
                        top_left: evidence.clone(),
                        top_right: evidence.clone(),
                        bottom_left: evidence.clone(),
                        bottom_right: evidence,
                    },
                },
                distortion: crate::schema::DistortionMeasurements::blocked(
                    crate::schema::DistortionBlocker::NoStraightReference,
                ),
            },
        }
    }

    #[test]
    fn measures_integer_horizontal_channel_shift() {
        let (red, blue) = shifted_pair(2, 0);

        let evidence = measure(&red, &blue);

        let shift = evidence.shift.expect("measured shift");
        assert_close(shift.x.value, 2.0, 0.1);
        assert_close(shift.y.value, 0.0, 0.1);
        assert_close(shift.magnitude.value, 2.0, 0.1);
        assert!(evidence.blockers.is_empty());
    }

    #[test]
    fn parabolic_refinement_moves_closer_to_fractional_shift() {
        let injected_shift = 1.4;
        let blue = (0..32)
            .map(|index| smooth_profile_sample(f32::from(u16::try_from(index).unwrap())))
            .collect::<Vec<_>>();
        let red = (0..32)
            .map(|index| {
                smooth_profile_sample(f32::from(u16::try_from(index).unwrap()) - injected_shift)
            })
            .collect::<Vec<_>>();

        let shift = super::measure_profile_shift(&red, &blue).expect("profile search");
        let super::ProfileShift::Measured(measured) = shift else {
            panic!("expected measured shift");
        };

        assert!((measured - injected_shift).abs() < (1.0_f32 - injected_shift).abs());
    }

    fn smooth_profile_sample(x: f32) -> f32 {
        let centred = (x - 15.0) / 5.0;
        (-centred * centred).exp()
    }

    #[test]
    fn reports_near_zero_for_identical_textured_patches() {
        let (red, _) = shifted_pair(0, 0);
        let blue = red.clone();

        let shift = measure(&red, &blue).shift.expect("measured shift");

        assert_close(shift.x.value, 0.0, 0.01);
        assert_close(shift.y.value, 0.0, 0.01);
    }

    #[test]
    fn flat_profiles_return_blocker_without_numeric_shift() {
        let red = image(8, 8, vec![0.5; 64]);
        let blue = image(8, 8, vec![0.5; 64]);

        let evidence = measure(&red, &blue);

        assert!(evidence.shift.is_none());
        assert_eq!(evidence.blockers, vec![CaBlocker::FlatProfile]);
    }

    #[test]
    fn too_short_profiles_return_blocker() {
        let red = image(4, 8, vec![0.5; 32]);
        let blue = image(4, 8, vec![0.5; 32]);

        let evidence = measure(&red, &blue);

        assert_eq!(evidence.blockers, vec![CaBlocker::ProfileTooShort]);
    }

    #[test]
    fn non_finite_samples_are_errors() {
        let red = image(8, 8, vec![f32::NAN; 64]);
        let blue = image(8, 8, vec![0.5; 64]);
        let rect = Rect::new(0, 0, 8, 8).unwrap();

        assert!(matches!(
            measure_lateral_ca(red.patch(rect).unwrap(), blue.patch(rect).unwrap(), 1.0),
            Err(CaError::NonFiniteSample { .. })
        ));
    }

    #[test]
    fn flat_overlap_candidate_does_not_abort_profile_search() {
        let red = vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 2.0, 3.0];
        let blue = vec![3.0, 2.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];

        let shift = super::measure_profile_shift(&red, &blue).expect("profile search");

        assert!(matches!(shift, super::ProfileShift::Measured(_)));
    }

    #[test]
    fn deterministic_tie_prefers_zero_shift() {
        let red = image(
            8,
            8,
            (0..64)
                .map(|value| {
                    f32::from(u16::try_from(value % 2).expect("test value fits in u16"))
                        + f32::from(u16::try_from(value / 8).expect("test value fits in u16")) * 0.1
                })
                .collect(),
        );
        let blue = red.clone();

        let shift = measure(&red, &blue).shift.expect("measured shift");

        assert_close(shift.x.value, 0.0, 1.0e-6);
    }

    #[test]
    fn aggregate_group_ca_reports_mean_and_sample_scatter_per_corner() {
        let evidence = aggregate_group_ca(&[
            ca_frame(true, CaZoneEvidence::measured(ca_shift(1.0, 0.0, 1.0))),
            ca_frame(true, CaZoneEvidence::measured(ca_shift(3.0, 0.0, 3.0))),
        ])
        .expect("aggregate");

        assert_eq!(evidence.top_left.included_samples, 2);
        assert_close(evidence.top_left.mean_shift.unwrap().x.value, 2.0, 1.0e-6);
        assert_close(
            evidence.top_left.scatter.unwrap().x.value,
            std::f32::consts::SQRT_2,
            1.0e-6,
        );
        assert!(evidence.top_left.blockers.is_empty());
        assert!(evidence.top_left.excluded.is_empty());
    }

    #[test]
    fn aggregate_group_ca_marks_one_sample_as_insufficient_for_scatter() {
        let evidence = aggregate_group_ca(&[ca_frame(
            true,
            CaZoneEvidence::measured(ca_shift(1.0, 0.0, 1.0)),
        )])
        .expect("aggregate");

        assert_eq!(evidence.top_left.included_samples, 1);
        assert!(evidence.top_left.mean_shift.is_some());
        assert!(evidence.top_left.scatter.is_none());
        assert_eq!(
            evidence.top_left.blockers,
            vec![CaBlocker::InsufficientSamples]
        );
    }

    #[test]
    fn aggregate_group_ca_excludes_unknown_corrections_after_validating_frame_evidence() {
        let evidence = aggregate_group_ca(&[ca_frame(
            false,
            CaZoneEvidence::measured(ca_shift(1.0, 0.0, 1.0)),
        )])
        .expect("aggregate");

        assert_eq!(evidence.top_left.included_samples, 0);
        assert_eq!(evidence.top_left.excluded_samples, 1);
        assert_eq!(
            evidence.top_left.excluded[0].reason,
            ExclusionReason::UnknownCorrections
        );
        assert_eq!(
            evidence.top_left.blockers,
            vec![
                CaBlocker::InsufficientSamples,
                CaBlocker::UnknownCorrections
            ]
        );
    }

    #[test]
    fn aggregate_group_ca_excludes_blocked_frame_samples() {
        let evidence = aggregate_group_ca(&[ca_frame(
            true,
            CaZoneEvidence::blocked(CaBlocker::FlatProfile),
        )])
        .expect("aggregate");

        assert_eq!(evidence.top_left.included_samples, 0);
        assert_eq!(evidence.top_left.excluded_samples, 1);
        assert_eq!(
            evidence.top_left.excluded[0].reason,
            ExclusionReason::FlatProfile
        );
        assert_eq!(
            evidence.top_left.blockers,
            vec![CaBlocker::InsufficientSamples, CaBlocker::FlatProfile]
        );
    }

    #[test]
    fn aggregate_group_ca_rejects_non_finite_dto_before_exclusion() {
        let mut shift = ca_shift(1.0, 0.0, 1.0);
        shift.x.value = f32::NAN;

        assert!(matches!(
            aggregate_group_ca(&[ca_frame(false, CaZoneEvidence::measured(shift))]),
            Err(CaError::NonFiniteDerivedValue { .. })
        ));
    }
}
