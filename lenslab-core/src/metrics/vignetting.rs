use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::image::LinearPatchView;
use crate::schema::{
    AnalyseGroup, CornerFalloff, ExclusionCount, ExclusionReason, FrameMeasurement,
    VignettingBlocker, VignettingCornerValues, VignettingEvidence, VignettingMethod,
    VignettingNumericMeasurement, VignettingSymmetry, VignettingSymmetryStatus,
};

const CENTRE_LUMINANCE_DRIFT_LIMIT_STOPS: f32 = 0.25;
const SAME_APERTURE_CORNER_SCATTER_LIMIT_STOPS: f32 = 0.10;
const APERTURE_TREND_ALLOWANCE_STOPS: f32 = 0.12;
const RADIAL_SYMMETRY_MAX_CORNER_RESIDUAL_STOPS: f32 = 0.12;
const PERSISTENT_LIGHTING_BIAS_FLOOR_STOPS: f32 = 0.15;
const LIGHTING_BIAS_OPTICAL_RESIDUAL_CEILING_STOPS: f32 = 0.08;

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

    let assessment = assess_controlled_aperture_series(groups)?;
    if !assessment.blockers.is_empty() {
        for group in groups {
            remove_blocker(
                &mut group.vignetting.blockers,
                VignettingBlocker::SymmetryNotAssessed,
            );
            for blocker in &assessment.blockers {
                add_blocker(&mut group.vignetting.blockers, *blocker);
            }
            group.vignetting.symmetry = VignettingSymmetry {
                status: VignettingSymmetryStatus::Blocked,
                blockers: assessment.blockers.clone(),
                ..VignettingSymmetry::not_assessed()
            };
        }
        return Ok(());
    }

    let Some(reference_index) = assessment.reference_index else {
        for group in groups {
            remove_blocker(
                &mut group.vignetting.blockers,
                VignettingBlocker::SymmetryNotAssessed,
            );
            if !group.vignetting.blockers.is_empty() {
                group.vignetting.symmetry = VignettingSymmetry {
                    status: VignettingSymmetryStatus::Blocked,
                    blockers: group.vignetting.blockers.clone(),
                    ..VignettingSymmetry::not_assessed()
                };
            }
        }
        return Ok(());
    };
    let reference_f_number = groups[reference_index]
        .f_number
        .expect("assessed reference has aperture");
    let reference_values = groups[reference_index]
        .vignetting
        .raw_corner_mean_stops
        .expect("assessed reference has raw falloff");

    for member in assessment.members {
        let group = &mut groups[member.index];
        remove_blocker(
            &mut group.vignetting.blockers,
            VignettingBlocker::SymmetryNotAssessed,
        );
        group.vignetting.reference_f_number = Some(reference_f_number);
        if member.index == reference_index {
            add_blocker(
                &mut group.vignetting.blockers,
                VignettingBlocker::ReferenceAperture,
            );
            group.vignetting.symmetry = VignettingSymmetry {
                status: VignettingSymmetryStatus::NotAssessed,
                blockers: vec![VignettingBlocker::ReferenceAperture],
                ..VignettingSymmetry::not_assessed()
            };
            continue;
        }
        let delta = delta_values(
            member.raw_corner_mean,
            reference_values,
            VignettingMethod::ReferenceRelativeApertureDifference,
        )?;
        group.vignetting.optical_delta_from_reference_stops = Some(delta);
        group.vignetting.symmetry = classify_symmetry(
            delta,
            member.raw_corner_mean,
            assessment.repeat_scatter_stops,
        )?;
    }

    Ok(())
}

fn assess_controlled_aperture_series(
    groups: &mut [AnalyseGroup],
) -> Result<ControlledAssessment, VignettingError> {
    let mut blockers = Vec::new();
    let partitions = collect_partitions(groups);
    if partitions.len() > 1 {
        blockers.push(VignettingBlocker::MixedLensFocalIdentity);
    }
    for partition in &partitions {
        if partition.members.len() < 2 {
            add_blocker(&mut blockers, VignettingBlocker::InsufficientApertureSeries);
        }
    }
    if partitions.len() != 1 || !blockers.is_empty() {
        return Ok(ControlledAssessment {
            blockers,
            members: Vec::new(),
            reference_index: None,
            repeat_scatter_stops: None,
        });
    }

    let mut members = partitions
        .into_iter()
        .next()
        .expect("one partition")
        .members;
    members.sort_by(|left, right| left.f_number.total_cmp(&right.f_number));
    let mut sanity_blockers = Vec::new();
    let repeat_scatter = repeat_scatter_stops(groups, &members)?;
    if repeat_scatter.is_some_and(|scatter| scatter > SAME_APERTURE_CORNER_SCATTER_LIMIT_STOPS) {
        sanity_blockers.push(VignettingBlocker::UnstableRepeatScatter);
    }
    let centre_drift = centre_luminance_drift_stops(groups, &members)?;
    if centre_drift > CENTRE_LUMINANCE_DRIFT_LIMIT_STOPS {
        sanity_blockers.push(VignettingBlocker::UnstableCentreLuminance);
    }
    if contradicts_aperture_trend(&members) {
        sanity_blockers.push(VignettingBlocker::ContradictoryApertureTrend);
    }
    if !sanity_blockers.is_empty() {
        return Ok(ControlledAssessment {
            blockers: sanity_blockers,
            members: Vec::new(),
            reference_index: None,
            repeat_scatter_stops: repeat_scatter,
        });
    }

    let reference_index = members.last().map(|member| member.index);
    Ok(ControlledAssessment {
        blockers: Vec::new(),
        members,
        reference_index,
        repeat_scatter_stops: repeat_scatter,
    })
}

fn collect_partitions(groups: &mut [AnalyseGroup]) -> Vec<Partition> {
    let mut partitions = Vec::new();
    for (index, group) in groups.iter_mut().enumerate() {
        let (Some(lens_model), Some(focal_length_mm)) =
            (group.lens_model.clone(), group.focal_length_mm)
        else {
            add_blocker(
                &mut group.vignetting.blockers,
                VignettingBlocker::MissingLensFocalIdentity,
            );
            continue;
        };
        if !focal_length_mm.is_finite() {
            add_blocker(
                &mut group.vignetting.blockers,
                VignettingBlocker::MissingLensFocalIdentity,
            );
            continue;
        }
        let Some(f_number) = group.f_number else {
            add_blocker(
                &mut group.vignetting.blockers,
                VignettingBlocker::MissingAperture,
            );
            continue;
        };
        if !f_number.is_finite() {
            add_blocker(
                &mut group.vignetting.blockers,
                VignettingBlocker::MissingAperture,
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

fn repeat_scatter_stops(
    groups: &[AnalyseGroup],
    members: &[PartitionMember],
) -> Result<Option<f32>, VignettingError> {
    let mut max_scatter: Option<f32> = None;
    for member in members {
        let group = &groups[member.index];
        let values = group
            .frames
            .iter()
            .filter(|frame| frame.aggregation_eligible)
            .map(|frame| frame.measurements.vignetting.zones.values())
            .collect::<Vec<_>>();
        for values in &values {
            validate_values(*values)?;
        }
        if values.len() < 2 {
            continue;
        }
        for corner_values in [
            values
                .iter()
                .map(|value| value.top_left.value)
                .collect::<Vec<_>>(),
            values
                .iter()
                .map(|value| value.top_right.value)
                .collect::<Vec<_>>(),
            values
                .iter()
                .map(|value| value.bottom_left.value)
                .collect::<Vec<_>>(),
            values
                .iter()
                .map(|value| value.bottom_right.value)
                .collect::<Vec<_>>(),
        ] {
            let scatter = range(&corner_values);
            max_scatter = Some(max_scatter.map_or(scatter, |max| max.max(scatter)));
        }
    }
    Ok(max_scatter)
}

fn centre_luminance_drift_stops(
    groups: &[AnalyseGroup],
    members: &[PartitionMember],
) -> Result<f32, VignettingError> {
    let mut centres = Vec::new();
    for member in members {
        for frame in groups[member.index]
            .frames
            .iter()
            .filter(|frame| frame.aggregation_eligible)
        {
            let luminance = frame.measurements.sharpness.zones.centre.luminance.value;
            validate_positive_luminance(luminance)?;
            centres.push(luminance);
        }
    }
    if centres.len() < 2 {
        return Ok(0.0);
    }
    let min = centres.iter().copied().min_by(f32::total_cmp).unwrap();
    let max = centres.iter().copied().max_by(f32::total_cmp).unwrap();
    Ok((max / min).log2())
}

fn contradicts_aperture_trend(members: &[PartitionMember]) -> bool {
    members.windows(2).any(|window| {
        let wider = corner_mean(window[0].raw_corner_mean);
        let stopped = corner_mean(window[1].raw_corner_mean);
        stopped + APERTURE_TREND_ALLOWANCE_STOPS < wider
    })
}

fn classify_symmetry(
    delta: VignettingCornerValues,
    raw: VignettingCornerValues,
    repeat_scatter_stops: Option<f32>,
) -> Result<VignettingSymmetry, VignettingError> {
    let delta_values = corner_array(delta);
    let mean_delta = delta_values.iter().sum::<f32>() / 4.0;
    let max_corner_deviation = delta_values
        .iter()
        .map(|value| (value - mean_delta).abs())
        .fold(0.0, f32::max);
    let left_right = ((delta.top_left.value + delta.bottom_left.value)
        - (delta.top_right.value + delta.bottom_right.value))
        / 2.0;
    let top_bottom = ((delta.top_left.value + delta.top_right.value)
        - (delta.bottom_left.value + delta.bottom_right.value))
        / 2.0;
    let raw_bias = raw_corner_bias(raw);
    let status = if max_corner_deviation <= LIGHTING_BIAS_OPTICAL_RESIDUAL_CEILING_STOPS
        && raw_bias >= PERSISTENT_LIGHTING_BIAS_FLOOR_STOPS
    {
        VignettingSymmetryStatus::LightingBiased
    } else if max_corner_deviation <= RADIAL_SYMMETRY_MAX_CORNER_RESIDUAL_STOPS {
        VignettingSymmetryStatus::RadiallySymmetric
    } else {
        VignettingSymmetryStatus::MixedOrUnstable
    };

    Ok(VignettingSymmetry {
        status,
        mean_optical_delta_stops: Some(vignetting_measurement(
            mean_delta,
            VignettingMethod::ReferenceRelativeApertureDifference,
        )?),
        max_corner_deviation_stops: Some(vignetting_measurement(
            max_corner_deviation,
            VignettingMethod::DerivedResidual,
        )?),
        left_right_residual_stops: Some(vignetting_measurement(
            left_right,
            VignettingMethod::DerivedResidual,
        )?),
        top_bottom_residual_stops: Some(vignetting_measurement(
            top_bottom,
            VignettingMethod::DerivedResidual,
        )?),
        persistent_raw_bias_stops: Some(vignetting_measurement(
            raw_bias,
            VignettingMethod::DerivedResidual,
        )?),
        repeat_scatter_stops: repeat_scatter_stops
            .map(|scatter| vignetting_measurement(scatter, VignettingMethod::DerivedRepeatScatter))
            .transpose()?,
        blockers: Vec::new(),
    })
}

fn corner_mean(values: VignettingCornerValues) -> f32 {
    corner_array(values).iter().sum::<f32>() / 4.0
}

fn raw_corner_bias(values: VignettingCornerValues) -> f32 {
    let values = corner_array(values);
    let mean = values.iter().sum::<f32>() / 4.0;
    values
        .iter()
        .map(|value| (value - mean).abs())
        .fold(0.0, f32::max)
}

fn corner_array(values: VignettingCornerValues) -> [f32; 4] {
    [
        values.top_left.value,
        values.top_right.value,
        values.bottom_left.value,
        values.bottom_right.value,
    ]
}

fn range(values: &[f32]) -> f32 {
    let min = values.iter().copied().min_by(f32::total_cmp).unwrap();
    let max = values.iter().copied().max_by(f32::total_cmp).unwrap();
    max - min
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

struct ControlledAssessment {
    blockers: Vec<VignettingBlocker>,
    members: Vec<PartitionMember>,
    reference_index: Option<usize>,
    repeat_scatter_stops: Option<f32>,
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

fn remove_blocker(blockers: &mut Vec<VignettingBlocker>, blocker: VignettingBlocker) {
    blockers.retain(|existing| *existing != blocker);
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
        AnalyseGroup, CaBlocker, CaLateralEvidence, CaLateralMeasurements, CornerFalloff,
        DecentringEvidence, ExclusionReason, FrameMeasurement, FrameQuality, LeftRightDecentring,
        Measurements, PairId, PairSummary, ReliabilityBlocker, SharpnessMeasurements,
        TargetQualityBlocker, VignettingBlocker, VignettingCornerValues, VignettingMeasurements,
        VignettingNumericMeasurement, VignettingSymmetryStatus, VignettingZoneMeasurements,
        ZoneMeasurement, ZoneMeasurements,
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
        zone_with_luminance(1.0)
    }

    fn zone_with_luminance(luminance: f32) -> ZoneMeasurement {
        ZoneMeasurement::measured(1.0, 0.2, luminance, true).unwrap()
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
        frame_with_centre_luminance(eligible, falloff, 1.0)
    }

    fn frame_with_centre_luminance(
        eligible: bool,
        falloff: VignettingCornerValues,
        centre_luminance: f32,
    ) -> FrameMeasurement {
        let zone = zone();
        let centre_zone = zone_with_luminance(centre_luminance);
        FrameMeasurement {
            input_index: 0,
            path: "frame.tif".to_owned(),
            aggregation_eligible: eligible,
            qa: FrameQuality::target_blocked(TargetQualityBlocker::NoSuitableTargetReference),
            measurements: Measurements {
                sharpness: SharpnessMeasurements {
                    zones: ZoneMeasurements::from_ordered([
                        centre_zone,
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
                ca_lateral: CaLateralMeasurements::blocked_all(CaBlocker::FlatProfile),
                distortion: crate::schema::DistortionMeasurements::blocked(
                    crate::schema::DistortionBlocker::NoStraightReference,
                ),
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
        group_with_frames(f_number, frames)
    }

    fn group_with_frames(f_number: f32, frames: Vec<FrameMeasurement>) -> AnalyseGroup {
        AnalyseGroup {
            lens_model: Some("50mm".to_owned()),
            focal_length_mm: Some(50.0),
            f_number: Some(f_number),
            decentring: decentring(),
            vignetting: aggregate_group_vignetting(&frames).unwrap(),
            ca_lateral: CaLateralEvidence::empty(),
            distortion: crate::schema::DistortionEvidence::empty(),
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
        assert_eq!(
            groups[0].vignetting.symmetry.status,
            VignettingSymmetryStatus::MixedOrUnstable
        );
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
            assert!(group.vignetting.reference_f_number.is_none());
            assert!(
                group
                    .vignetting
                    .optical_delta_from_reference_stops
                    .is_none()
            );
            assert_eq!(
                group.vignetting.symmetry.status,
                VignettingSymmetryStatus::Blocked
            );
            assert!(
                !group
                    .vignetting
                    .blockers
                    .contains(&VignettingBlocker::SymmetryNotAssessed)
            );
        }
    }

    #[test]
    fn mixed_lens_focal_candidate_set_blocks_controlled_vignetting() {
        let mut other_lens = group(11.0, values(-0.4, -0.4, -0.4, -0.4));
        other_lens.lens_model = Some("35mm".to_owned());
        let mut groups = vec![stale_delta_group(), other_lens];

        apply_reference_relative_vignetting(&mut groups, true).expect("mixed identity");

        for group in groups {
            assert!(
                group
                    .vignetting
                    .blockers
                    .contains(&VignettingBlocker::MixedLensFocalIdentity)
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
    fn missing_and_non_finite_aperture_use_aperture_blocker() {
        let mut missing_aperture = stale_delta_group();
        missing_aperture.f_number = None;
        let mut non_finite_aperture = stale_delta_group();
        non_finite_aperture.f_number = Some(f32::NAN);
        let mut groups = vec![missing_aperture, non_finite_aperture];

        apply_reference_relative_vignetting(&mut groups, true).expect("aperture blockers");

        for group in groups {
            assert!(
                group
                    .vignetting
                    .blockers
                    .contains(&VignettingBlocker::MissingAperture)
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
            ca_lateral: CaLateralEvidence::empty(),
            distortion: crate::schema::DistortionEvidence::empty(),
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
    fn radially_symmetric_optical_delta_emits_numeric_evidence() {
        let mut groups = vec![
            group(4.0, values(-1.2, -1.2, -1.2, -1.2)),
            group(11.0, values(-0.4, -0.4, -0.4, -0.4)),
        ];

        apply_reference_relative_vignetting(&mut groups, true).expect("reference deltas");

        let symmetry = &groups[0].vignetting.symmetry;
        assert_eq!(symmetry.status, VignettingSymmetryStatus::RadiallySymmetric);
        assert_close(symmetry.mean_optical_delta_stops.unwrap().value, -0.8);
        assert_close(symmetry.max_corner_deviation_stops.unwrap().value, 0.0);
        assert!(symmetry.blockers.is_empty());
    }

    #[test]
    fn fixed_raw_bias_that_cancels_in_delta_is_lighting_biased() {
        let mut groups = vec![
            group(4.0, values(-1.2, -1.0, -1.0, -1.0)),
            group(11.0, values(-0.4, -0.2, -0.2, -0.2)),
        ];

        apply_reference_relative_vignetting(&mut groups, true).expect("reference deltas");

        let symmetry = &groups[0].vignetting.symmetry;
        assert_eq!(symmetry.status, VignettingSymmetryStatus::LightingBiased);
        assert_close(symmetry.mean_optical_delta_stops.unwrap().value, -0.8);
        assert!(symmetry.persistent_raw_bias_stops.unwrap().value >= 0.15);
    }

    #[test]
    fn repeat_scatter_above_threshold_blocks_optical_delta() {
        let noisy = group_with_frames(
            4.0,
            vec![
                frame(true, values(-1.2, -1.2, -1.2, -1.2)),
                frame(true, values(-1.31, -1.2, -1.2, -1.2)),
            ],
        );
        let mut groups = vec![noisy, group(11.0, values(-0.4, -0.4, -0.4, -0.4))];

        apply_reference_relative_vignetting(&mut groups, true).expect("scatter blocker");

        for group in groups {
            assert!(
                group
                    .vignetting
                    .blockers
                    .contains(&VignettingBlocker::UnstableRepeatScatter)
            );
            assert_eq!(
                group.vignetting.symmetry.status,
                VignettingSymmetryStatus::Blocked
            );
            assert!(
                group
                    .vignetting
                    .optical_delta_from_reference_stops
                    .is_none()
            );
        }
    }

    #[test]
    fn centre_luminance_drift_above_threshold_blocks_optical_delta() {
        let mut groups = vec![
            group_with_frames(
                4.0,
                vec![frame_with_centre_luminance(
                    true,
                    values(-1.2, -1.2, -1.2, -1.2),
                    1.0,
                )],
            ),
            group_with_frames(
                11.0,
                vec![frame_with_centre_luminance(
                    true,
                    values(-0.4, -0.4, -0.4, -0.4),
                    1.3,
                )],
            ),
        ];

        apply_reference_relative_vignetting(&mut groups, true).expect("centre drift blocker");

        assert!(
            groups[0]
                .vignetting
                .blockers
                .contains(&VignettingBlocker::UnstableCentreLuminance)
        );
        assert!(
            groups[0]
                .vignetting
                .optical_delta_from_reference_stops
                .is_none()
        );
    }

    #[test]
    fn contradictory_aperture_trend_blocks_optical_delta() {
        let mut groups = vec![
            group(4.0, values(-0.4, -0.4, -0.4, -0.4)),
            group(11.0, values(-0.7, -0.7, -0.7, -0.7)),
        ];

        apply_reference_relative_vignetting(&mut groups, true).expect("trend blocker");

        assert!(
            groups[0]
                .vignetting
                .blockers
                .contains(&VignettingBlocker::ContradictoryApertureTrend)
        );
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
