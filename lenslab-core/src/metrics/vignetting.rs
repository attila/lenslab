use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::image::LinearPatchView;
use crate::schema::{
    AnalyseGroup, CornerFalloff, ExclusionCount, ExclusionReason, FrameMeasurement,
    VignettingBlocker, VignettingCornerValues, VignettingEvidence, VignettingMethod,
    VignettingNumericMeasurement, VignettingSymmetry,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VignettingError {
    EmptyPatch,
    NonFiniteSample { value: f32 },
    NonPositiveLuminance { value: f32 },
    NonFiniteLuminance { value: f32 },
    NonFiniteDerivedValue { value: f32 },
}

impl Display for VignettingError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyPatch => write!(formatter, "patch is empty"),
            Self::NonFiniteSample { value } => write!(formatter, "non-finite sample {value}"),
            Self::NonPositiveLuminance { value } => {
                write!(formatter, "non-positive luminance {value}")
            }
            Self::NonFiniteLuminance { value } => write!(formatter, "non-finite luminance {value}"),
            Self::NonFiniteDerivedValue { value } => {
                write!(formatter, "non-finite vignetting value {value}")
            }
        }
    }
}

impl Error for VignettingError {}

pub fn median_luminance(patch: LinearPatchView<'_>) -> Result<f32, VignettingError> {
    let dimensions = patch.dimensions();
    let count = dimensions.width() * dimensions.height();
    if count == 0 {
        return Err(VignettingError::EmptyPatch);
    }

    let mut samples = Vec::with_capacity(count);
    for row in patch.rows() {
        for sample in row {
            if !sample.is_finite() {
                return Err(VignettingError::NonFiniteSample { value: *sample });
            }
            samples.push(*sample);
        }
    }
    let midpoint = samples.len() / 2;
    let median = if samples.len() % 2 == 0 {
        let (lower, upper_middle, _) = samples.select_nth_unstable_by(midpoint, f32::total_cmp);
        let lower_middle = lower
            .iter()
            .copied()
            .max_by(f32::total_cmp)
            .ok_or(VignettingError::EmptyPatch)?;
        f32::midpoint(lower_middle, *upper_middle)
    } else {
        let (_, median, _) = samples.select_nth_unstable_by(midpoint, f32::total_cmp);
        *median
    };
    validate_positive_luminance(median)?;
    Ok(median)
}

pub fn measured_falloff(
    centre_luminance: f32,
    corner_luminance: f32,
) -> Result<CornerFalloff, VignettingError> {
    validate_positive_luminance(centre_luminance)?;
    validate_positive_luminance(corner_luminance)?;
    let value = (corner_luminance / centre_luminance).log2();
    VignettingNumericMeasurement::measured_stops(value)
        .map(|falloff| CornerFalloff { falloff })
        .ok_or(VignettingError::NonFiniteDerivedValue { value })
}

pub fn aggregate_group_vignetting(
    frames: &[FrameMeasurement],
) -> Result<VignettingEvidence, VignettingError> {
    let mut accumulator = GroupAccumulator::default();
    for frame in frames {
        accumulator.push(frame)?;
    }
    accumulator.finish()
}

pub fn apply_reference_relative_vignetting(
    groups: &mut [AnalyseGroup],
    controlled_aperture_series: bool,
) -> Result<(), VignettingError> {
    for group in groups.iter_mut() {
        validate_group_vignetting(group)?;
        group.vignetting.reference_f_number = None;
        group.vignetting.optical_delta_from_reference_stops = None;
    }

    if !controlled_aperture_series {
        for group in groups {
            add_blocker(
                &mut group.vignetting.blockers,
                VignettingBlocker::ControlledApertureSeriesNotAssessed,
            );
        }
        return Ok(());
    }

    let partitions = collect_partitions(groups);

    for partition in partitions {
        if partition.members.len() < 2 {
            for member in partition.members {
                add_blocker(
                    &mut groups[member.index].vignetting.blockers,
                    VignettingBlocker::InsufficientApertureSeries,
                );
            }
            continue;
        }
        let Some(reference) = partition
            .members
            .iter()
            .max_by(|left, right| left.f_number.total_cmp(&right.f_number))
        else {
            continue;
        };
        let reference_index = reference.index;
        let reference_f_number = reference.f_number;
        let reference_values = reference.raw_corner_mean;
        for member in partition.members {
            let group = &mut groups[member.index];
            group.vignetting.reference_f_number = Some(reference_f_number);
            if member.index == reference_index {
                add_blocker(
                    &mut group.vignetting.blockers,
                    VignettingBlocker::ReferenceAperture,
                );
                continue;
            }
            group.vignetting.optical_delta_from_reference_stops = Some(delta_values(
                member.raw_corner_mean,
                reference_values,
                VignettingMethod::ReferenceRelativeApertureDifference,
            )?);
        }
    }

    Ok(())
}

fn collect_partitions(groups: &mut [AnalyseGroup]) -> Vec<Partition> {
    let mut partitions = Vec::new();
    for (index, group) in groups.iter_mut().enumerate() {
        let (Some(lens_model), Some(focal_length_mm), Some(f_number)) = (
            group.lens_model.clone(),
            group.focal_length_mm,
            group.f_number,
        ) else {
            add_blocker(
                &mut group.vignetting.blockers,
                VignettingBlocker::MissingLensFocalIdentity,
            );
            continue;
        };
        if !focal_length_mm.is_finite() || !f_number.is_finite() {
            add_blocker(
                &mut group.vignetting.blockers,
                VignettingBlocker::MissingLensFocalIdentity,
            );
            continue;
        }
        let Some(raw_corner_mean) = group.vignetting.raw_corner_mean_stops else {
            add_blocker(
                &mut group.vignetting.blockers,
                VignettingBlocker::InsufficientApertureSeries,
            );
            continue;
        };
        push_partition_member(
            &mut partitions,
            PartitionKey {
                lens_model,
                focal_length_mm,
            },
            PartitionMember {
                index,
                f_number,
                raw_corner_mean,
            },
        );
    }
    partitions
}

fn push_partition_member(
    partitions: &mut Vec<Partition>,
    key: PartitionKey,
    member: PartitionMember,
) {
    if let Some(partition) = partitions.iter_mut().find(|partition| partition.key == key) {
        partition.members.push(member);
    } else {
        partitions.push(Partition {
            key,
            members: vec![member],
        });
    }
}

#[derive(Default)]
struct GroupAccumulator {
    values: Vec<VignettingCornerValues>,
    unknown_corrections: usize,
}

impl GroupAccumulator {
    fn push(&mut self, frame: &FrameMeasurement) -> Result<(), VignettingError> {
        let values = frame.measurements.vignetting.zones.values();
        validate_values(values)?;
        if frame.aggregation_eligible {
            self.values.push(values);
        } else {
            self.unknown_corrections += 1;
        }
        Ok(())
    }

    fn finish(self) -> Result<VignettingEvidence, VignettingError> {
        let raw_corner_mean_stops =
            mean_values(&self.values, VignettingMethod::MeasuredLuminanceRatio)?;
        let mut excluded = Vec::new();
        if self.unknown_corrections > 0 {
            excluded.push(ExclusionCount {
                reason: ExclusionReason::UnknownCorrections,
                count: self.unknown_corrections,
            });
        }
        let mut blockers = Vec::new();
        if raw_corner_mean_stops.is_none() {
            blockers.push(VignettingBlocker::InsufficientApertureSeries);
        }
        if self.unknown_corrections > 0 {
            blockers.push(VignettingBlocker::UnknownCorrections);
        }
        blockers.push(VignettingBlocker::SymmetryNotAssessed);

        Ok(VignettingEvidence {
            method: VignettingMethod::MeasuredLuminanceRatio,
            included_samples: self.values.len(),
            excluded_samples: excluded.iter().map(|count| count.count).sum(),
            reference_f_number: None,
            raw_corner_mean_stops,
            optical_delta_from_reference_stops: None,
            blockers,
            excluded,
            symmetry: VignettingSymmetry::not_assessed(),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
struct PartitionKey {
    lens_model: String,
    focal_length_mm: f32,
}

struct Partition {
    key: PartitionKey,
    members: Vec<PartitionMember>,
}

#[derive(Clone, Copy)]
struct PartitionMember {
    index: usize,
    f_number: f32,
    raw_corner_mean: VignettingCornerValues,
}

fn validate_positive_luminance(value: f32) -> Result<(), VignettingError> {
    if !value.is_finite() {
        return Err(VignettingError::NonFiniteLuminance { value });
    }
    if value <= 0.0 {
        return Err(VignettingError::NonPositiveLuminance { value });
    }
    Ok(())
}

fn validate_group_vignetting(group: &AnalyseGroup) -> Result<(), VignettingError> {
    if let Some(values) = group.vignetting.raw_corner_mean_stops {
        validate_values(values)?;
    }
    if let Some(values) = group.vignetting.optical_delta_from_reference_stops {
        validate_values(values)?;
    }
    Ok(())
}

fn validate_values(values: VignettingCornerValues) -> Result<(), VignettingError> {
    for value in [
        values.top_left.value,
        values.top_right.value,
        values.bottom_left.value,
        values.bottom_right.value,
    ] {
        if !value.is_finite() {
            return Err(VignettingError::NonFiniteDerivedValue { value });
        }
    }
    Ok(())
}

fn mean_values(
    values: &[VignettingCornerValues],
    method: VignettingMethod,
) -> Result<Option<VignettingCornerValues>, VignettingError> {
    if values.is_empty() {
        return Ok(None);
    }
    #[allow(clippy::cast_precision_loss)]
    let len = values.len() as f32;
    let top_left = values.iter().map(|value| value.top_left.value).sum::<f32>() / len;
    let top_right = values
        .iter()
        .map(|value| value.top_right.value)
        .sum::<f32>()
        / len;
    let bottom_left = values
        .iter()
        .map(|value| value.bottom_left.value)
        .sum::<f32>()
        / len;
    let bottom_right = values
        .iter()
        .map(|value| value.bottom_right.value)
        .sum::<f32>()
        / len;
    values_from_f32(top_left, top_right, bottom_left, bottom_right, method).map(Some)
}

fn delta_values(
    values: VignettingCornerValues,
    reference: VignettingCornerValues,
    method: VignettingMethod,
) -> Result<VignettingCornerValues, VignettingError> {
    values_from_f32(
        values.top_left.value - reference.top_left.value,
        values.top_right.value - reference.top_right.value,
        values.bottom_left.value - reference.bottom_left.value,
        values.bottom_right.value - reference.bottom_right.value,
        method,
    )
}

fn values_from_f32(
    top_left: f32,
    top_right: f32,
    bottom_left: f32,
    bottom_right: f32,
    method: VignettingMethod,
) -> Result<VignettingCornerValues, VignettingError> {
    Ok(VignettingCornerValues {
        top_left: vignetting_measurement(top_left, method)?,
        top_right: vignetting_measurement(top_right, method)?,
        bottom_left: vignetting_measurement(bottom_left, method)?,
        bottom_right: vignetting_measurement(bottom_right, method)?,
    })
}

fn vignetting_measurement(
    value: f32,
    method: VignettingMethod,
) -> Result<VignettingNumericMeasurement, VignettingError> {
    VignettingNumericMeasurement::stops(value, method)
        .ok_or(VignettingError::NonFiniteDerivedValue { value })
}

fn add_blocker(blockers: &mut Vec<VignettingBlocker>, blocker: VignettingBlocker) {
    if !blockers.contains(&blocker) {
        blockers.push(blocker);
    }
}

impl CornerFalloff {
    #[must_use]
    pub fn value(&self) -> f32 {
        self.falloff.value
    }
}

#[cfg(test)]
mod tests {
    use super::{
        VignettingError, aggregate_group_vignetting, apply_reference_relative_vignetting,
        measured_falloff, median_luminance,
    };
    use crate::image::{Dimensions, LinearImage, Rect};
    use crate::schema::{
        AnalyseGroup, CornerFalloff, DecentringEvidence, ExclusionReason, FrameMeasurement,
        LeftRightDecentring, Measurements, PairId, PairSummary, ReliabilityBlocker,
        SharpnessMeasurements, VignettingBlocker, VignettingCornerValues, VignettingMeasurements,
        VignettingNumericMeasurement, VignettingZoneMeasurements, ZoneMeasurement,
        ZoneMeasurements,
    };

    fn patch(samples: Vec<f32>) -> LinearImage {
        LinearImage::new(Dimensions::new(samples.len(), 1).unwrap(), samples).unwrap()
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1.0e-6,
            "actual {actual} expected {expected}"
        );
    }

    #[test]
    fn measured_falloff_reports_corner_ratio_in_stops() {
        let falloff = measured_falloff(1.0, 0.5).expect("falloff");

        assert_close(falloff.value(), -1.0);
        assert_eq!(falloff.falloff.unit, crate::schema::NumericUnit::Stops);
    }

    #[test]
    fn brighter_corner_remains_measured_positive_evidence() {
        let falloff = measured_falloff(0.5, 1.0).expect("falloff");

        assert_close(falloff.value(), 1.0);
    }

    #[test]
    fn median_luminance_uses_middle_sorted_sample_for_odd_patch() {
        let image = patch(vec![0.1, 0.9, 0.4, 0.3, 10.0]);
        let view = image.patch(Rect::new(0, 0, 5, 1).unwrap()).unwrap();

        assert_close(median_luminance(view).unwrap(), 0.4);
    }

    #[test]
    fn median_luminance_averages_two_middle_samples_for_even_patch() {
        let image = patch(vec![0.1, 0.9, 0.4, 0.3]);
        let view = image.patch(Rect::new(0, 0, 4, 1).unwrap()).unwrap();

        assert_close(median_luminance(view).unwrap(), 0.35);
    }

    #[test]
    fn invalid_luminance_is_rejected_before_ratio_conversion() {
        for value in [0.0, -0.1, f32::NAN, f32::INFINITY] {
            assert!(
                measured_falloff(1.0, value).is_err(),
                "value {value} should fail"
            );
        }
    }

    fn zone() -> ZoneMeasurement {
        ZoneMeasurement::measured(1.0, 0.2, 1.0, true).unwrap()
    }

    fn values(
        top_left: f32,
        top_right: f32,
        bottom_left: f32,
        bottom_right: f32,
    ) -> VignettingCornerValues {
        VignettingCornerValues {
            top_left: VignettingNumericMeasurement::measured_stops(top_left).unwrap(),
            top_right: VignettingNumericMeasurement::measured_stops(top_right).unwrap(),
            bottom_left: VignettingNumericMeasurement::measured_stops(bottom_left).unwrap(),
            bottom_right: VignettingNumericMeasurement::measured_stops(bottom_right).unwrap(),
        }
    }

    fn frame(eligible: bool, falloff: VignettingCornerValues) -> FrameMeasurement {
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
                        top_left: CornerFalloff {
                            falloff: falloff.top_left,
                        },
                        top_right: CornerFalloff {
                            falloff: falloff.top_right,
                        },
                        bottom_left: CornerFalloff {
                            falloff: falloff.bottom_left,
                        },
                        bottom_right: CornerFalloff {
                            falloff: falloff.bottom_right,
                        },
                    },
                },
            },
        }
    }

    fn decentring() -> DecentringEvidence {
        let pair = PairSummary {
            id: PairId::TopLeftMinusTopRight,
            included_samples: 0,
            excluded_samples: 0,
            mean_delta: None,
            scatter: None,
            reliability_blockers: vec![ReliabilityBlocker::InsufficientSamples],
            excluded: vec![],
        };
        DecentringEvidence::not_assessed(LeftRightDecentring {
            top_pair: pair.clone(),
            bottom_pair: PairSummary {
                id: PairId::BottomLeftMinusBottomRight,
                ..pair
            },
        })
    }

    fn group(f_number: f32, falloff: VignettingCornerValues) -> AnalyseGroup {
        let frames = vec![frame(true, falloff)];
        AnalyseGroup {
            lens_model: Some("50mm".to_owned()),
            focal_length_mm: Some(50.0),
            f_number: Some(f_number),
            decentring: decentring(),
            vignetting: aggregate_group_vignetting(&frames).unwrap(),
            frames,
        }
    }

    fn stale_delta_group() -> AnalyseGroup {
        let mut group = group(4.0, values(-1.0, -1.0, -1.0, -1.0));
        group.vignetting.reference_f_number = Some(11.0);
        group.vignetting.optical_delta_from_reference_stops = Some(values(-0.5, -0.5, -0.5, -0.5));
        group
    }

    #[test]
    fn aggregate_preserves_measured_raw_falloff_and_excludes_unknown_corrections() {
        let evidence = aggregate_group_vignetting(&[
            frame(true, values(-1.0, -0.8, -0.7, -0.9)),
            frame(false, values(-0.4, -0.4, -0.4, -0.4)),
        ])
        .expect("aggregate");

        assert_eq!(evidence.included_samples, 1);
        assert_eq!(evidence.excluded_samples, 1);
        assert_eq!(
            evidence.excluded[0].reason,
            ExclusionReason::UnknownCorrections
        );
        assert_eq!(evidence.blockers[0], VignettingBlocker::UnknownCorrections);
        assert_close(evidence.raw_corner_mean_stops.unwrap().top_left.value, -1.0);
    }

    #[test]
    fn controlled_reference_series_reports_aperture_delta() {
        let mut groups = vec![
            group(4.0, values(-1.2, -1.1, -1.0, -0.9)),
            group(11.0, values(-0.4, -0.4, -0.4, -0.4)),
        ];

        apply_reference_relative_vignetting(&mut groups, true).expect("reference deltas");

        assert_eq!(groups[0].vignetting.reference_f_number, Some(11.0));
        let delta = groups[0]
            .vignetting
            .optical_delta_from_reference_stops
            .unwrap();
        assert_close(delta.top_left.value, -0.8);
        assert_close(delta.bottom_right.value, -0.5);
        assert!(
            groups[1]
                .vignetting
                .blockers
                .contains(&VignettingBlocker::ReferenceAperture)
        );
        assert!(
            groups[1]
                .vignetting
                .optical_delta_from_reference_stops
                .is_none()
        );
    }

    #[test]
    fn controlled_series_with_missing_identity_blocks_without_optical_delta() {
        let mut missing_lens = stale_delta_group();
        missing_lens.lens_model = None;
        let mut missing_focal = stale_delta_group();
        missing_focal.focal_length_mm = None;
        let mut non_finite_focal = stale_delta_group();
        non_finite_focal.focal_length_mm = Some(f32::INFINITY);
        let mut missing_aperture = stale_delta_group();
        missing_aperture.f_number = None;
        let mut groups = vec![
            missing_lens,
            missing_focal,
            non_finite_focal,
            missing_aperture,
        ];

        apply_reference_relative_vignetting(&mut groups, true).expect("identity blockers");

        for group in groups {
            assert!(
                group
                    .vignetting
                    .blockers
                    .contains(&VignettingBlocker::MissingLensFocalIdentity)
            );
            assert!(group.vignetting.reference_f_number.is_none());
            assert!(
                group
                    .vignetting
                    .optical_delta_from_reference_stops
                    .is_none()
            );
        }
    }

    #[test]
    fn single_controlled_aperture_blocks_reference_relative_evidence() {
        let mut groups = vec![stale_delta_group()];

        apply_reference_relative_vignetting(&mut groups, true).expect("single aperture");

        assert!(
            groups[0]
                .vignetting
                .blockers
                .contains(&VignettingBlocker::InsufficientApertureSeries)
        );
        assert!(groups[0].vignetting.reference_f_number.is_none());
        assert!(
            groups[0]
                .vignetting
                .optical_delta_from_reference_stops
                .is_none()
        );
    }

    #[test]
    fn controlled_group_without_raw_falloff_blocks_reference_relative_evidence() {
        let frames = vec![frame(false, values(-1.0, -1.0, -1.0, -1.0))];
        let mut group = AnalyseGroup {
            lens_model: Some("50mm".to_owned()),
            focal_length_mm: Some(50.0),
            f_number: Some(4.0),
            decentring: decentring(),
            vignetting: aggregate_group_vignetting(&frames).unwrap(),
            frames,
        };
        group.vignetting.reference_f_number = Some(11.0);
        group.vignetting.optical_delta_from_reference_stops = Some(values(-0.5, -0.5, -0.5, -0.5));
        let mut groups = vec![group];

        apply_reference_relative_vignetting(&mut groups, true).expect("blocked raw falloff");

        assert!(
            groups[0]
                .vignetting
                .blockers
                .contains(&VignettingBlocker::InsufficientApertureSeries)
        );
        assert!(groups[0].vignetting.reference_f_number.is_none());
        assert!(
            groups[0]
                .vignetting
                .optical_delta_from_reference_stops
                .is_none()
        );
    }

    #[test]
    fn uncontrolled_series_blocks_reference_relative_evidence() {
        let mut groups = vec![
            stale_delta_group(),
            group(11.0, values(-0.4, -0.4, -0.4, -0.4)),
        ];

        apply_reference_relative_vignetting(&mut groups, false).expect("blocked");

        assert!(
            groups[0]
                .vignetting
                .blockers
                .contains(&VignettingBlocker::ControlledApertureSeriesNotAssessed)
        );
        assert!(
            groups[0]
                .vignetting
                .optical_delta_from_reference_stops
                .is_none()
        );
        assert!(groups[0].vignetting.reference_f_number.is_none());
    }

    #[test]
    fn invalid_schema_values_are_rejected_before_exclusion_handling() {
        let mut bad = frame(false, values(-1.0, -1.0, -1.0, -1.0));
        bad.measurements.vignetting.zones.top_left.falloff.value = f32::NAN;

        let err = aggregate_group_vignetting(&[bad]).expect_err("invalid value");

        assert!(matches!(err, VignettingError::NonFiniteDerivedValue { .. }));
    }
}
