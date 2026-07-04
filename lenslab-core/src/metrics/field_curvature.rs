use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::schema::{
    AnalyseGroup, ExclusionCount, ExclusionReason, FieldCurvatureBlocker, FieldCurvatureEvidence,
    FieldCurvatureMethod, FieldCurvatureStatus, FieldCurvatureSummary, ZoneMeasurement,
};

pub const LAG_THRESHOLD_STOPS: f32 = 1.75;
const PEAK_AMBIGUITY_RELATIVE_THRESHOLD: f32 = 0.98;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FieldCurvatureError {
    NonFiniteAcutance { value: f32 },
    NonFiniteDerivedValue { value: f32 },
}

impl Display for FieldCurvatureError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteAcutance { value } => write!(formatter, "non-finite acutance {value}"),
            Self::NonFiniteDerivedValue { value } => {
                write!(formatter, "non-finite field-curvature value {value}")
            }
        }
    }
}

impl Error for FieldCurvatureError {}

pub fn infer_field_curvature(
    groups: &[AnalyseGroup],
) -> Result<FieldCurvatureEvidence, FieldCurvatureError> {
    let mut partitions = Vec::new();
    let mut partition_indexes = HashMap::new();
    let mut summaries = Vec::new();

    for group in groups {
        let sample = group_sample(group)?;
        let (Some(lens_model), Some(focal_length_mm)) =
            (group.lens_model.clone(), group.focal_length_mm)
        else {
            summaries.push(blocked_summary(
                group.lens_model.clone(),
                group.focal_length_mm,
                &sample,
                FieldCurvatureBlocker::MissingLensFocalIdentity,
            ));
            continue;
        };
        if !focal_length_mm.is_finite() {
            summaries.push(blocked_summary(
                Some(lens_model),
                Some(focal_length_mm),
                &sample,
                FieldCurvatureBlocker::MissingLensFocalIdentity,
            ));
            continue;
        }

        let Some(f_number) = group.f_number else {
            push_partition_group(
                &mut partitions,
                &mut partition_indexes,
                PartitionKey {
                    lens_model,
                    focal_length_mm,
                },
                PartitionGroup {
                    f_number: None,
                    sample,
                },
            );
            continue;
        };

        push_partition_group(
            &mut partitions,
            &mut partition_indexes,
            PartitionKey {
                lens_model,
                focal_length_mm,
            },
            PartitionGroup {
                f_number: (f_number.is_finite() && f_number > 0.0).then_some(f_number),
                sample,
            },
        );
    }

    for partition in partitions {
        summaries.push(partition.finish()?);
    }

    Ok(FieldCurvatureEvidence {
        method: FieldCurvatureMethod::InferredApertureLagFromMeasuredAcutance,
        summaries,
    })
}

#[derive(Debug, Clone, PartialEq)]
struct PartitionKey {
    lens_model: String,
    focal_length_mm: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PartitionLookupKey {
    lens_model: String,
    focal_length_bits: u32,
}

impl From<&PartitionKey> for PartitionLookupKey {
    fn from(key: &PartitionKey) -> Self {
        Self {
            lens_model: key.lens_model.clone(),
            focal_length_bits: key.focal_length_mm.to_bits(),
        }
    }
}

struct Partition {
    key: PartitionKey,
    groups: Vec<PartitionGroup>,
}

impl Partition {
    fn finish(self) -> Result<FieldCurvatureSummary, FieldCurvatureError> {
        let CandidateSet {
            candidates,
            excluded_groups,
            unknown_corrections,
            low_texture,
            blockers,
        } = collect_candidates(self.groups)?;

        let mut included_f_numbers = candidates
            .iter()
            .map(|candidate| candidate.f_number)
            .collect::<Vec<_>>();
        included_f_numbers.sort_by(f32::total_cmp);

        if !blockers.is_empty() {
            return Ok(FieldCurvatureSummary {
                lens_model: Some(self.key.lens_model),
                focal_length_mm: Some(self.key.focal_length_mm),
                status: FieldCurvatureStatus::Blocked,
                eligible_aperture_groups: candidates.len(),
                excluded_aperture_groups: excluded_groups,
                included_f_numbers,
                centre_peak_f_number: None,
                corner_mean_peak_f_number: None,
                lag_stops: None,
                lag_threshold_stops: LAG_THRESHOLD_STOPS,
                blockers,
                excluded: exclusions(unknown_corrections, low_texture),
            });
        }

        let Some(centre_peak) =
            unambiguous_peak(&candidates, |candidate| candidate.centre_acutance)
        else {
            return Ok(ambiguous_summary(
                self.key,
                candidates.len(),
                excluded_groups,
                included_f_numbers,
                unknown_corrections,
                low_texture,
            ));
        };
        let Some(corner_peak) =
            unambiguous_peak(&candidates, |candidate| candidate.corner_mean_acutance)
        else {
            return Ok(ambiguous_summary(
                self.key,
                candidates.len(),
                excluded_groups,
                included_f_numbers,
                unknown_corrections,
                low_texture,
            ));
        };

        let lag = aperture_lag_stops(centre_peak.f_number, corner_peak.f_number)?;
        let status = if lag >= LAG_THRESHOLD_STOPS {
            FieldCurvatureStatus::Supported
        } else {
            FieldCurvatureStatus::NotSupported
        };

        Ok(FieldCurvatureSummary {
            lens_model: Some(self.key.lens_model),
            focal_length_mm: Some(self.key.focal_length_mm),
            status,
            eligible_aperture_groups: candidates.len(),
            excluded_aperture_groups: excluded_groups,
            included_f_numbers,
            centre_peak_f_number: Some(centre_peak.f_number),
            corner_mean_peak_f_number: Some(corner_peak.f_number),
            lag_stops: Some(lag),
            lag_threshold_stops: LAG_THRESHOLD_STOPS,
            blockers: Vec::new(),
            excluded: exclusions(unknown_corrections, low_texture),
        })
    }
}

struct PartitionGroup {
    f_number: Option<f32>,
    sample: GroupSample,
}

struct CandidateSet {
    candidates: Vec<Candidate>,
    excluded_groups: usize,
    unknown_corrections: usize,
    low_texture: usize,
    blockers: Vec<FieldCurvatureBlocker>,
}

#[derive(Default)]
struct GroupSample {
    centre_values: Vec<f32>,
    corner_values: Vec<f32>,
    unknown_corrections: usize,
    low_texture: usize,
}

fn collect_candidates(groups: Vec<PartitionGroup>) -> Result<CandidateSet, FieldCurvatureError> {
    let mut candidates = Vec::new();
    let mut excluded_groups = 0;
    let mut unknown_corrections = 0;
    let mut low_texture = 0;
    let mut missing_aperture = false;

    for group in groups {
        unknown_corrections += group.sample.unknown_corrections;
        low_texture += group.sample.low_texture;
        let Some(f_number) = group.f_number else {
            missing_aperture = true;
            excluded_groups += 1;
            continue;
        };
        let Some(candidate) = group.sample.candidate(f_number)? else {
            excluded_groups += 1;
            continue;
        };
        candidates.push(candidate);
    }

    Ok(CandidateSet {
        blockers: candidate_blockers(
            candidates.len(),
            missing_aperture,
            unknown_corrections,
            low_texture,
        ),
        candidates,
        excluded_groups,
        unknown_corrections,
        low_texture,
    })
}

fn candidate_blockers(
    candidate_count: usize,
    missing_aperture: bool,
    unknown_corrections: usize,
    low_texture: usize,
) -> Vec<FieldCurvatureBlocker> {
    let mut blockers = Vec::new();
    if missing_aperture {
        push_blocker(&mut blockers, FieldCurvatureBlocker::MissingAperture);
    }
    if candidate_count < 2 {
        push_blocker(
            &mut blockers,
            FieldCurvatureBlocker::InsufficientApertureSeries,
        );
    }
    if candidate_count < 2 || missing_aperture {
        if unknown_corrections > 0 {
            push_blocker(&mut blockers, FieldCurvatureBlocker::UnknownCorrections);
        }
        if low_texture > 0 {
            push_blocker(&mut blockers, FieldCurvatureBlocker::LowTexture);
        }
    }
    blockers
}

impl GroupSample {
    fn candidate(&self, f_number: f32) -> Result<Option<Candidate>, FieldCurvatureError> {
        let Some(centre_acutance) = mean(&self.centre_values) else {
            return Ok(None);
        };
        let Some(corner_mean_acutance) = mean(&self.corner_values) else {
            return Ok(None);
        };
        if !centre_acutance.is_finite() {
            return Err(FieldCurvatureError::NonFiniteDerivedValue {
                value: centre_acutance,
            });
        }
        if !corner_mean_acutance.is_finite() {
            return Err(FieldCurvatureError::NonFiniteDerivedValue {
                value: corner_mean_acutance,
            });
        }
        Ok(Some(Candidate {
            f_number,
            centre_acutance,
            corner_mean_acutance,
        }))
    }
}

#[derive(Clone, Copy)]
struct Candidate {
    f_number: f32,
    centre_acutance: f32,
    corner_mean_acutance: f32,
}

fn group_sample(group: &AnalyseGroup) -> Result<GroupSample, FieldCurvatureError> {
    let mut sample = GroupSample::default();
    for frame in &group.frames {
        let zones = &frame.measurements.sharpness.zones;
        validate_acutance(&zones.centre)?;
        validate_acutance(&zones.top_left)?;
        validate_acutance(&zones.top_right)?;
        validate_acutance(&zones.bottom_left)?;
        validate_acutance(&zones.bottom_right)?;

        if !frame.aggregation_eligible {
            sample.unknown_corrections += 1;
            continue;
        }
        if !zones.centre.texture_usable.value
            || !zones.top_left.texture_usable.value
            || !zones.top_right.texture_usable.value
            || !zones.bottom_left.texture_usable.value
            || !zones.bottom_right.texture_usable.value
        {
            sample.low_texture += 1;
            continue;
        }

        sample.centre_values.push(zones.centre.acutance.value);
        let corner_mean = mean(&[
            zones.top_left.acutance.value,
            zones.top_right.acutance.value,
            zones.bottom_left.acutance.value,
            zones.bottom_right.acutance.value,
        ])
        .ok_or(FieldCurvatureError::NonFiniteDerivedValue { value: f32::NAN })?;
        if !corner_mean.is_finite() {
            return Err(FieldCurvatureError::NonFiniteDerivedValue { value: corner_mean });
        }
        sample.corner_values.push(corner_mean);
    }
    Ok(sample)
}

fn validate_acutance(zone: &ZoneMeasurement) -> Result<(), FieldCurvatureError> {
    let value = zone.acutance.value;
    if value.is_finite() {
        Ok(())
    } else {
        Err(FieldCurvatureError::NonFiniteAcutance { value })
    }
}

fn push_partition_group(
    partitions: &mut Vec<Partition>,
    indexes: &mut HashMap<PartitionLookupKey, usize>,
    key: PartitionKey,
    group: PartitionGroup,
) {
    let lookup_key = PartitionLookupKey::from(&key);
    if let Some(index) = indexes.get(&lookup_key) {
        partitions[*index].groups.push(group);
        return;
    }

    let index = partitions.len();
    partitions.push(Partition {
        key,
        groups: vec![group],
    });
    indexes.insert(lookup_key, index);
}

fn blocked_summary(
    lens_model: Option<String>,
    focal_length_mm: Option<f32>,
    sample: &GroupSample,
    blocker: FieldCurvatureBlocker,
) -> FieldCurvatureSummary {
    let mut blockers = vec![blocker];
    if sample.unknown_corrections > 0 {
        push_blocker(&mut blockers, FieldCurvatureBlocker::UnknownCorrections);
    }
    if sample.low_texture > 0 {
        push_blocker(&mut blockers, FieldCurvatureBlocker::LowTexture);
    }
    FieldCurvatureSummary {
        lens_model,
        focal_length_mm,
        status: FieldCurvatureStatus::Blocked,
        eligible_aperture_groups: 0,
        excluded_aperture_groups: 1,
        included_f_numbers: Vec::new(),
        centre_peak_f_number: None,
        corner_mean_peak_f_number: None,
        lag_stops: None,
        lag_threshold_stops: LAG_THRESHOLD_STOPS,
        blockers,
        excluded: exclusions(sample.unknown_corrections, sample.low_texture),
    }
}

fn ambiguous_summary(
    key: PartitionKey,
    eligible_aperture_groups: usize,
    excluded_aperture_groups: usize,
    included_f_numbers: Vec<f32>,
    unknown_corrections: usize,
    low_texture: usize,
) -> FieldCurvatureSummary {
    FieldCurvatureSummary {
        lens_model: Some(key.lens_model),
        focal_length_mm: Some(key.focal_length_mm),
        status: FieldCurvatureStatus::Blocked,
        eligible_aperture_groups,
        excluded_aperture_groups,
        included_f_numbers,
        centre_peak_f_number: None,
        corner_mean_peak_f_number: None,
        lag_stops: None,
        lag_threshold_stops: LAG_THRESHOLD_STOPS,
        blockers: vec![FieldCurvatureBlocker::AmbiguousPeak],
        excluded: exclusions(unknown_corrections, low_texture),
    }
}

fn unambiguous_peak(
    candidates: &[Candidate],
    value: impl Fn(&Candidate) -> f32,
) -> Option<Candidate> {
    let mut ranked = candidates.to_vec();
    ranked.sort_by(|left, right| {
        value(right)
            .total_cmp(&value(left))
            .then_with(|| left.f_number.total_cmp(&right.f_number))
    });
    let peak = ranked.first().copied()?;
    let runner_up = ranked.get(1).copied();
    if let Some(runner_up) = runner_up {
        let peak_value = value(&peak);
        let runner_up_value = value(&runner_up);
        if runner_up_value >= peak_value * PEAK_AMBIGUITY_RELATIVE_THRESHOLD {
            return None;
        }
    }
    Some(peak)
}

fn aperture_lag_stops(
    centre_f_number: f32,
    corner_f_number: f32,
) -> Result<f32, FieldCurvatureError> {
    let lag = 2.0 * (corner_f_number / centre_f_number).log2();
    if lag.is_finite() {
        Ok(lag)
    } else {
        Err(FieldCurvatureError::NonFiniteDerivedValue { value: lag })
    }
}

fn exclusions(unknown_corrections: usize, low_texture: usize) -> Vec<ExclusionCount> {
    let mut excluded = Vec::new();
    if unknown_corrections > 0 {
        excluded.push(ExclusionCount {
            reason: ExclusionReason::UnknownCorrections,
            count: unknown_corrections,
        });
    }
    if low_texture > 0 {
        excluded.push(ExclusionCount {
            reason: ExclusionReason::LowTexture,
            count: low_texture,
        });
    }
    excluded
}

fn push_blocker(blockers: &mut Vec<FieldCurvatureBlocker>, blocker: FieldCurvatureBlocker) {
    if !blockers.contains(&blocker) {
        blockers.push(blocker);
    }
}

fn mean(values: &[f32]) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let len = values.len() as f32;
    Some(values.iter().sum::<f32>() / len)
}

#[cfg(test)]
mod tests {
    use super::{FieldCurvatureError, LAG_THRESHOLD_STOPS, infer_field_curvature};
    use crate::schema::{
        AnalyseGroup, CaBlocker, CaLateralEvidence, CaLateralMeasurements, DecentringEvidence,
        DistortionEvidence, ExclusionReason, FieldCurvatureBlocker, FieldCurvatureStatus,
        FrameMeasurement, FrameQuality, LeftRightDecentring, MeasurementMethod, Measurements,
        NumericMeasurement, NumericUnit, PairId, PairSummary, ReliabilityBlocker,
        SharpnessMeasurements, TargetQualityBlocker, TextureMethod, TextureUsable,
        VignettingEvidence, VignettingMeasurements, VignettingMethod, VignettingSymmetry,
        VignettingZoneMeasurements, ZoneMeasurement, ZoneMeasurements,
    };

    fn zone(acutance: f32, texture_usable: bool) -> ZoneMeasurement {
        ZoneMeasurement {
            acutance: NumericMeasurement {
                value: acutance,
                unit: NumericUnit::Acutance,
                method: MeasurementMethod::Measured,
                confidence: if texture_usable { 1.0 } else { 0.0 },
            },
            contrast: NumericMeasurement {
                value: if texture_usable { 0.2 } else { 0.1 },
                unit: NumericUnit::Ratio,
                method: MeasurementMethod::Measured,
                confidence: if texture_usable { 1.0 } else { 0.0 },
            },
            luminance: NumericMeasurement {
                value: 1.0,
                unit: NumericUnit::LinearLuminance,
                method: MeasurementMethod::Measured,
                confidence: 1.0,
            },
            texture_usable: TextureUsable {
                value: texture_usable,
                threshold: 0.15,
                method: TextureMethod::DerivedThreshold,
            },
        }
    }

    fn frame(eligible: bool, centre: f32, corner_mean: f32) -> FrameMeasurement {
        FrameMeasurement {
            input_index: 0,
            path: "frame.dng".to_owned(),
            aggregation_eligible: eligible,
            qa: FrameQuality::target_blocked(TargetQualityBlocker::NoSuitableTargetReference),
            measurements: Measurements {
                sharpness: SharpnessMeasurements {
                    zones: ZoneMeasurements::from_ordered([
                        zone(centre, true),
                        zone(corner_mean, true),
                        zone(corner_mean, true),
                        zone(corner_mean, true),
                        zone(corner_mean, true),
                    ]),
                },
                vignetting: VignettingMeasurements {
                    zones: VignettingZoneMeasurements {
                        top_left: corner_falloff(),
                        top_right: corner_falloff(),
                        bottom_left: corner_falloff(),
                        bottom_right: corner_falloff(),
                    },
                },
                ca_lateral: CaLateralMeasurements::blocked_all(CaBlocker::FlatProfile),
                distortion: crate::schema::DistortionMeasurements::blocked(
                    crate::schema::DistortionBlocker::NoStraightReference,
                ),
            },
        }
    }

    fn low_texture_frame() -> FrameMeasurement {
        let mut frame = frame(true, 1.0, 1.0);
        frame.measurements.sharpness.zones.top_left = zone(1.0, false);
        frame
    }

    fn non_finite_frame() -> FrameMeasurement {
        let mut frame = frame(false, 1.0, 1.0);
        frame.measurements.sharpness.zones.bottom_right = zone(f32::NAN, true);
        frame
    }

    fn corner_falloff() -> crate::schema::CornerFalloff {
        crate::schema::CornerFalloff {
            falloff: crate::schema::VignettingNumericMeasurement::measured_stops(-1.0).unwrap(),
        }
    }

    fn group(f_number: Option<f32>, centre: f32, corner_mean: f32) -> AnalyseGroup {
        group_with_frames(f_number, vec![frame(true, centre, corner_mean)])
    }

    fn group_with_frames(f_number: Option<f32>, frames: Vec<FrameMeasurement>) -> AnalyseGroup {
        AnalyseGroup {
            lens_model: Some("50mm".to_owned()),
            focal_length_mm: Some(50.0),
            f_number,
            decentring: decentring(),
            vignetting: vignetting(),
            ca_lateral: CaLateralEvidence::empty(),
            distortion: DistortionEvidence::empty(),
            frames,
        }
    }

    fn group_for_lens(lens_model: &str, focal_length_mm: f32, f_number: f32) -> AnalyseGroup {
        AnalyseGroup {
            lens_model: Some(lens_model.to_owned()),
            focal_length_mm: Some(focal_length_mm),
            f_number: Some(f_number),
            decentring: decentring(),
            vignetting: vignetting(),
            ca_lateral: CaLateralEvidence::empty(),
            distortion: DistortionEvidence::empty(),
            frames: vec![frame(true, 1.0, 1.0)],
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

    fn vignetting() -> VignettingEvidence {
        VignettingEvidence {
            method: VignettingMethod::MeasuredLuminanceRatio,
            included_samples: 0,
            excluded_samples: 0,
            reference_f_number: None,
            raw_corner_mean_stops: None,
            optical_delta_from_reference_stops: None,
            blockers: Vec::new(),
            excluded: Vec::new(),
            symmetry: VignettingSymmetry::not_assessed(),
        }
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1.0e-5,
            "actual {actual} expected {expected}"
        );
    }

    #[test]
    fn two_stop_corner_lag_is_supported() {
        let evidence = infer_field_curvature(&[
            group(Some(5.6), 2.0, 1.0),
            group(Some(8.0), 1.5, 1.5),
            group(Some(11.0), 1.0, 2.0),
        ])
        .expect("field curvature");

        let summary = &evidence.summaries[0];
        assert_eq!(summary.status, FieldCurvatureStatus::Supported);
        assert_eq!(summary.centre_peak_f_number, Some(5.6));
        assert_eq!(summary.corner_mean_peak_f_number, Some(11.0));
        assert_close(summary.lag_stops.unwrap(), 2.0 * (11.0_f32 / 5.6).log2());
        assert_close(summary.lag_threshold_stops, LAG_THRESHOLD_STOPS);
        assert!(summary.blockers.is_empty());
    }

    #[test]
    fn same_aperture_peak_is_not_supported_not_blocked() {
        let evidence = infer_field_curvature(&[
            group(Some(5.6), 1.0, 1.0),
            group(Some(8.0), 2.0, 2.0),
            group(Some(11.0), 1.0, 1.0),
        ])
        .expect("field curvature");

        let summary = &evidence.summaries[0];
        assert_eq!(summary.status, FieldCurvatureStatus::NotSupported);
        assert_eq!(summary.lag_stops, Some(0.0));
        assert!(summary.blockers.is_empty());
    }

    #[test]
    fn insufficient_and_missing_metadata_paths_are_blocked() {
        let mut missing_identity = group(Some(5.6), 1.0, 1.0);
        missing_identity.lens_model = None;
        let mut non_finite_focal = group(Some(8.0), 1.0, 1.0);
        non_finite_focal.focal_length_mm = Some(f32::INFINITY);
        let evidence = infer_field_curvature(&[
            group(Some(4.0), 1.0, 1.0),
            group(None, 1.0, 1.0),
            missing_identity,
            non_finite_focal,
        ])
        .expect("field curvature");

        assert!(
            evidence.summaries[0]
                .blockers
                .contains(&FieldCurvatureBlocker::MissingLensFocalIdentity)
        );
        assert!(
            evidence.summaries[1]
                .blockers
                .contains(&FieldCurvatureBlocker::MissingLensFocalIdentity)
        );
        assert!(
            evidence.summaries[2]
                .blockers
                .contains(&FieldCurvatureBlocker::MissingAperture)
        );
        assert!(
            evidence.summaries[2]
                .blockers
                .contains(&FieldCurvatureBlocker::InsufficientApertureSeries)
        );
    }

    #[test]
    fn low_texture_and_unknown_corrections_exclude_groups() {
        let evidence = infer_field_curvature(&[
            group_with_frames(Some(5.6), vec![low_texture_frame()]),
            group_with_frames(Some(8.0), vec![frame(false, 2.0, 3.0)]),
            group(Some(11.0), 1.0, 1.0),
        ])
        .expect("field curvature");

        let summary = &evidence.summaries[0];
        assert_eq!(summary.status, FieldCurvatureStatus::Blocked);
        assert_eq!(summary.eligible_aperture_groups, 1);
        assert_eq!(summary.excluded_aperture_groups, 2);
        assert!(
            summary
                .blockers
                .contains(&FieldCurvatureBlocker::LowTexture)
        );
        assert!(
            summary
                .blockers
                .contains(&FieldCurvatureBlocker::UnknownCorrections)
        );
        assert!(
            summary
                .excluded
                .iter()
                .any(|count| count.reason == ExclusionReason::LowTexture && count.count == 1)
        );
        assert!(
            summary.excluded.iter().any(|count| count.reason
                == ExclusionReason::UnknownCorrections
                && count.count == 1)
        );
    }

    #[test]
    fn excluded_groups_do_not_block_when_enough_clean_apertures_remain() {
        let evidence = infer_field_curvature(&[
            group_with_frames(Some(4.0), vec![low_texture_frame()]),
            group(Some(5.6), 2.0, 1.0),
            group(Some(8.0), 1.5, 1.5),
            group(Some(11.0), 1.0, 2.0),
            group_with_frames(Some(16.0), vec![frame(false, 2.0, 3.0)]),
        ])
        .expect("field curvature");

        let summary = &evidence.summaries[0];
        assert_eq!(summary.status, FieldCurvatureStatus::Supported);
        assert_eq!(summary.eligible_aperture_groups, 3);
        assert_eq!(summary.excluded_aperture_groups, 2);
        assert!(summary.blockers.is_empty());
        assert!(
            summary
                .excluded
                .iter()
                .any(|count| count.reason == ExclusionReason::LowTexture && count.count == 1)
        );
        assert!(
            summary.excluded.iter().any(|count| count.reason
                == ExclusionReason::UnknownCorrections
                && count.count == 1)
        );
    }

    #[test]
    fn partition_lookup_preserves_first_seen_summary_order() {
        let mut groups = Vec::new();
        for focal_length_mm in 10_u16..140 {
            groups.push(group_for_lens("zoom", f32::from(focal_length_mm), 5.6));
        }
        groups.push(group_for_lens("first", 50.0, 5.6));
        groups.push(group_for_lens("second", 50.0, 5.6));
        groups.push(group_for_lens("first", 50.0, 8.0));
        groups.push(group_for_lens("second", 50.0, 8.0));

        let evidence = infer_field_curvature(&groups).expect("field curvature");

        assert_eq!(evidence.summaries[130].lens_model.as_deref(), Some("first"));
        assert_eq!(
            evidence.summaries[131].lens_model.as_deref(),
            Some("second")
        );
        assert_eq!(evidence.summaries[130].eligible_aperture_groups, 2);
        assert_eq!(evidence.summaries[131].eligible_aperture_groups, 2);
    }

    #[test]
    fn near_tied_peak_is_ambiguous() {
        let evidence = infer_field_curvature(&[
            group(Some(5.6), 2.0, 1.0),
            group(Some(8.0), 1.97, 1.5),
            group(Some(11.0), 1.0, 2.0),
        ])
        .expect("field curvature");

        let summary = &evidence.summaries[0];
        assert_eq!(summary.status, FieldCurvatureStatus::Blocked);
        assert_eq!(summary.blockers, vec![FieldCurvatureBlocker::AmbiguousPeak]);
        assert!(summary.lag_stops.is_none());
    }

    #[test]
    fn non_finite_acutance_is_rejected_before_exclusion() {
        let err = infer_field_curvature(&[group_with_frames(Some(5.6), vec![non_finite_frame()])])
            .expect_err("non-finite acutance");

        assert!(matches!(err, FieldCurvatureError::NonFiniteAcutance { .. }));
    }
}
