use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::schema::{
    AnalyseGroup, CopyAssessmentBlocker, CopyAssessmentEvidence, CopyAssessmentGate,
    CopyAssessmentState, CopyAssessmentSupport, DerivedNumericMeasurement, ExclusionReason,
    FieldCurvatureBlocker, FieldCurvatureEvidence, FieldCurvatureStatus, PairSummary,
    ReliabilityBlocker, TargetQualityStatus,
};

const CENTRED_MAX_ABS_DELTA: f32 = 0.08;
const DECENTRED_MIN_ABS_DELTA: f32 = 0.18;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CopyAssessmentError {
    NonFiniteFocalLength { value: f32 },
    NonFiniteAperture { value: f32 },
    NonFiniteDerivedValue { value: f32 },
}

impl Display for CopyAssessmentError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteFocalLength { value } => {
                write!(formatter, "non-finite focal length {value}")
            }
            Self::NonFiniteAperture { value } => write!(formatter, "non-finite aperture {value}"),
            Self::NonFiniteDerivedValue { value } => {
                write!(formatter, "non-finite copy assessment value {value}")
            }
        }
    }
}

impl Error for CopyAssessmentError {}

pub fn assess_copy_support(
    groups: &[AnalyseGroup],
    field_curvature: &FieldCurvatureEvidence,
) -> Result<CopyAssessmentSupport, CopyAssessmentError> {
    if groups
        .iter()
        .any(|group| group.lens_model.is_none() || group.focal_length_mm.is_none())
    {
        return Ok(inconclusive(vec![
            CopyAssessmentBlocker::MissingLensFocalIdentity,
        ]));
    }

    let partitions = collect_partitions(groups)?;
    if partitions.is_empty() {
        return Ok(inconclusive(vec![
            CopyAssessmentBlocker::NoControlledTargetSeries,
        ]));
    }
    if partitions.len() > 1 {
        return Ok(inconclusive(vec![
            CopyAssessmentBlocker::MixedLensFocalIdentity,
        ]));
    }

    let partition = partitions.into_iter().next().expect("one partition");
    partition.finish(field_curvature)
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
    groups: Vec<GroupCandidate>,
}

impl Partition {
    fn finish(
        self,
        field_curvature: &FieldCurvatureEvidence,
    ) -> Result<CopyAssessmentSupport, CopyAssessmentError> {
        let mut blockers = Vec::new();
        let mut candidates = Vec::new();
        for group in self.groups {
            match group.candidate()? {
                Some(candidate) => candidates.push(candidate),
                None => blockers.extend(group.blockers.iter().copied()),
            }
        }
        candidates.sort_by(|left, right| left.f_number.total_cmp(&right.f_number));
        if candidates.len() < 2 {
            push_blocker(
                &mut blockers,
                CopyAssessmentBlocker::InsufficientApertureSeries,
            );
        }

        let field_gate = field_curvature_gate(field_curvature, &self.key);
        if blockers.is_empty() && candidates.len() >= 2 {
            let summary = SeriesSummary::from_candidates(&candidates)?;
            let (state, support_blockers) = support_state(&summary, &field_gate);
            blockers.extend(support_blockers);
            return Ok(CopyAssessmentSupport {
                state,
                method: crate::schema::CopyAssessmentMethod::DerivedFromTargetQaAcutanceAndFieldCurvature,
                hard_support_eligible: state != CopyAssessmentState::Inconclusive,
                lens_model: Some(self.key.lens_model),
                focal_length_mm: Some(self.key.focal_length_mm),
                included_aperture_groups: candidates.iter().map(|candidate| candidate.f_number).collect(),
                evidence: CopyAssessmentEvidence {
                    target_quality: CopyAssessmentGate::passed(),
                    correction_provenance: CopyAssessmentGate::passed(),
                    aperture_series: CopyAssessmentGate::passed(),
                    left_right_consistency: consistency_gate(state, &blockers),
                    field_curvature_counterevidence: field_gate_for_state(state, field_gate),
                    mean_top_pair_delta: measurement(summary.top_mean)?,
                    mean_bottom_pair_delta: measurement(summary.bottom_mean)?,
                    max_abs_pair_delta: measurement(summary.max_abs)?,
                    centred_threshold: CENTRED_MAX_ABS_DELTA,
                    decentred_threshold: DECENTRED_MIN_ABS_DELTA,
                },
                blockers: blockers.clone(),
                reshoot: crate::schema::reshoot_for_blockers(&blockers),
            });
        }

        let normalized_blockers = normalize_blockers(blockers);
        Ok(CopyAssessmentSupport {
            state: CopyAssessmentState::Inconclusive,
            method:
                crate::schema::CopyAssessmentMethod::DerivedFromTargetQaAcutanceAndFieldCurvature,
            hard_support_eligible: false,
            lens_model: Some(self.key.lens_model),
            focal_length_mm: Some(self.key.focal_length_mm),
            included_aperture_groups: candidates
                .iter()
                .map(|candidate| candidate.f_number)
                .collect(),
            evidence: CopyAssessmentEvidence {
                target_quality: gate_for(
                    &normalized_blockers,
                    &[
                        CopyAssessmentBlocker::TargetQualityNotPassed,
                        CopyAssessmentBlocker::UnknownCorrections,
                    ],
                ),
                correction_provenance: gate_for(
                    &normalized_blockers,
                    &[CopyAssessmentBlocker::UnknownCorrections],
                ),
                aperture_series: gate_for(
                    &normalized_blockers,
                    &[
                        CopyAssessmentBlocker::MissingAperture,
                        CopyAssessmentBlocker::InsufficientApertureSeries,
                    ],
                ),
                left_right_consistency: gate_for(
                    &normalized_blockers,
                    &[
                        CopyAssessmentBlocker::LowTexture,
                        CopyAssessmentBlocker::InsufficientSamples,
                        CopyAssessmentBlocker::InconsistentAsymmetry,
                        CopyAssessmentBlocker::AsymmetryAboveCentredThreshold,
                        CopyAssessmentBlocker::AsymmetryBelowDecentredThreshold,
                    ],
                ),
                field_curvature_counterevidence: field_gate,
                mean_top_pair_delta: None,
                mean_bottom_pair_delta: None,
                max_abs_pair_delta: None,
                centred_threshold: CENTRED_MAX_ABS_DELTA,
                decentred_threshold: DECENTRED_MIN_ABS_DELTA,
            },
            blockers: normalized_blockers.clone(),
            reshoot: crate::schema::reshoot_for_blockers(&normalized_blockers),
        })
    }
}

struct GroupCandidate {
    f_number: Option<f32>,
    top_delta: Option<f32>,
    bottom_delta: Option<f32>,
    blockers: Vec<CopyAssessmentBlocker>,
}

impl GroupCandidate {
    fn candidate(&self) -> Result<Option<Candidate>, CopyAssessmentError> {
        if !self.blockers.is_empty() {
            return Ok(None);
        }
        let Some(f_number) = self.f_number else {
            return Ok(None);
        };
        let Some(top_delta) = self.top_delta else {
            return Ok(None);
        };
        let Some(bottom_delta) = self.bottom_delta else {
            return Ok(None);
        };
        validate_finite(top_delta)?;
        validate_finite(bottom_delta)?;
        Ok(Some(Candidate {
            f_number,
            top_delta,
            bottom_delta,
        }))
    }
}

#[derive(Clone, Copy)]
struct Candidate {
    f_number: f32,
    top_delta: f32,
    bottom_delta: f32,
}

struct SeriesSummary {
    top_mean: f32,
    bottom_mean: f32,
    max_abs: f32,
    top_sign_consistent: bool,
    bottom_sign_consistent: bool,
}

impl SeriesSummary {
    fn from_candidates(candidates: &[Candidate]) -> Result<Self, CopyAssessmentError> {
        let top_mean = mean(candidates.iter().map(|candidate| candidate.top_delta))?;
        let bottom_mean = mean(candidates.iter().map(|candidate| candidate.bottom_delta))?;
        let max_abs = candidates.iter().fold(0.0_f32, |max, candidate| {
            max.max(candidate.top_delta.abs())
                .max(candidate.bottom_delta.abs())
        });
        validate_finite(max_abs)?;
        Ok(Self {
            top_mean,
            bottom_mean,
            max_abs,
            top_sign_consistent: sign_consistent(
                candidates.iter().map(|candidate| candidate.top_delta),
            ),
            bottom_sign_consistent: sign_consistent(
                candidates.iter().map(|candidate| candidate.bottom_delta),
            ),
        })
    }
}

fn collect_partitions(groups: &[AnalyseGroup]) -> Result<Vec<Partition>, CopyAssessmentError> {
    let mut partitions = Vec::new();
    let mut partition_indexes = HashMap::new();

    for group in groups {
        let Some(lens_model) = group.lens_model.clone() else {
            continue;
        };
        let Some(focal_length_mm) = group.focal_length_mm else {
            continue;
        };
        if !focal_length_mm.is_finite() {
            return Err(CopyAssessmentError::NonFiniteFocalLength {
                value: focal_length_mm,
            });
        }
        let key = PartitionKey {
            lens_model,
            focal_length_mm,
        };
        let lookup = PartitionLookupKey::from(&key);
        let index = if let Some(index) = partition_indexes.get(&lookup).copied() {
            index
        } else {
            let index = partitions.len();
            partition_indexes.insert(lookup, index);
            partitions.push(Partition {
                key,
                groups: Vec::new(),
            });
            index
        };
        partitions[index].groups.push(group_candidate(group)?);
    }

    Ok(partitions)
}

fn group_candidate(group: &AnalyseGroup) -> Result<GroupCandidate, CopyAssessmentError> {
    let mut blockers = Vec::new();
    let Some(f_number) = group.f_number else {
        push_blocker(&mut blockers, CopyAssessmentBlocker::MissingAperture);
        return Ok(GroupCandidate {
            f_number: None,
            top_delta: None,
            bottom_delta: None,
            blockers,
        });
    };
    if !f_number.is_finite() || f_number <= 0.0 {
        return Err(CopyAssessmentError::NonFiniteAperture { value: f_number });
    }

    if group.decentring.target_quality.status != TargetQualityStatus::Passed {
        push_blocker(&mut blockers, CopyAssessmentBlocker::TargetQualityNotPassed);
    }
    push_pair_blockers(&mut blockers, &group.decentring.left_right.top_pair);
    push_pair_blockers(&mut blockers, &group.decentring.left_right.bottom_pair);

    Ok(GroupCandidate {
        f_number: Some(f_number),
        top_delta: pair_delta(&group.decentring.left_right.top_pair)?,
        bottom_delta: pair_delta(&group.decentring.left_right.bottom_pair)?,
        blockers,
    })
}

fn push_pair_blockers(blockers: &mut Vec<CopyAssessmentBlocker>, pair: &PairSummary) {
    if pair.included_samples == 0 {
        push_blocker(blockers, CopyAssessmentBlocker::InsufficientSamples);
    }
    for blocker in &pair.reliability_blockers {
        match blocker {
            ReliabilityBlocker::InsufficientSamples => {
                push_blocker(blockers, CopyAssessmentBlocker::InsufficientSamples);
            }
        }
    }
    for exclusion in &pair.excluded {
        match exclusion.reason {
            ExclusionReason::UnknownCorrections => {
                push_blocker(blockers, CopyAssessmentBlocker::UnknownCorrections);
            }
            ExclusionReason::LowTexture => {
                push_blocker(blockers, CopyAssessmentBlocker::LowTexture);
            }
            ExclusionReason::FlatProfile
            | ExclusionReason::CorrelationPeakNotFound
            | ExclusionReason::ProfileTooShort
            | ExclusionReason::NoStraightReference
            | ExclusionReason::WeakReferenceGeometry
            | ExclusionReason::LowContrast
            | ExclusionReason::LineDiscontinuous
            | ExclusionReason::FitResidualTooHigh
            | ExclusionReason::UnsupportedColourChannels
            | ExclusionReason::UnstableRepeatOutlier => {}
        }
    }
}

fn pair_delta(pair: &PairSummary) -> Result<Option<f32>, CopyAssessmentError> {
    pair.mean_delta
        .as_ref()
        .map(|delta| {
            validate_finite(delta.value)?;
            Ok(delta.value)
        })
        .transpose()
}

fn support_state(
    summary: &SeriesSummary,
    field_gate: &CopyAssessmentGate,
) -> (CopyAssessmentState, Vec<CopyAssessmentBlocker>) {
    let mut blockers = Vec::new();
    let top_abs = summary.top_mean.abs();
    let bottom_abs = summary.bottom_mean.abs();
    if summary.max_abs <= CENTRED_MAX_ABS_DELTA {
        return (CopyAssessmentState::SupportsCentred, blockers);
    }
    if top_abs < DECENTRED_MIN_ABS_DELTA || bottom_abs < DECENTRED_MIN_ABS_DELTA {
        push_blocker(
            &mut blockers,
            CopyAssessmentBlocker::AsymmetryAboveCentredThreshold,
        );
        push_blocker(
            &mut blockers,
            CopyAssessmentBlocker::AsymmetryBelowDecentredThreshold,
        );
        return (CopyAssessmentState::Inconclusive, blockers);
    }
    if !summary.top_sign_consistent
        || !summary.bottom_sign_consistent
        || summary.top_mean.signum() != summary.bottom_mean.signum()
    {
        push_blocker(&mut blockers, CopyAssessmentBlocker::InconsistentAsymmetry);
        return (CopyAssessmentState::Inconclusive, blockers);
    }
    if field_gate.status == crate::schema::CopyAssessmentGateStatus::Blocked {
        blockers.extend(field_gate.blockers.iter().copied());
        return (
            CopyAssessmentState::Inconclusive,
            normalize_blockers(blockers),
        );
    }
    (CopyAssessmentState::SupportsDecentred, blockers)
}

fn field_curvature_gate(
    field_curvature: &FieldCurvatureEvidence,
    key: &PartitionKey,
) -> CopyAssessmentGate {
    let Some(summary) = field_curvature.summaries.iter().find(|summary| {
        summary.lens_model.as_deref() == Some(key.lens_model.as_str())
            && summary.focal_length_mm == Some(key.focal_length_mm)
    }) else {
        return CopyAssessmentGate::passed();
    };
    match summary.status {
        FieldCurvatureStatus::Supported => {
            CopyAssessmentGate::blocked(CopyAssessmentBlocker::FieldCurvatureCounterevidence)
        }
        FieldCurvatureStatus::Blocked => CopyAssessmentGate::blocked_with(normalize_blockers(
            summary
                .blockers
                .iter()
                .copied()
                .map(copy_blocker_for_field_curvature)
                .collect(),
        )),
        FieldCurvatureStatus::NotSupported => CopyAssessmentGate::passed(),
    }
}

fn copy_blocker_for_field_curvature(blocker: FieldCurvatureBlocker) -> CopyAssessmentBlocker {
    match blocker {
        FieldCurvatureBlocker::InsufficientApertureSeries => {
            CopyAssessmentBlocker::InsufficientApertureSeries
        }
        FieldCurvatureBlocker::MissingLensFocalIdentity => {
            CopyAssessmentBlocker::MissingLensFocalIdentity
        }
        FieldCurvatureBlocker::MissingAperture => CopyAssessmentBlocker::MissingAperture,
        FieldCurvatureBlocker::AmbiguousPeak => CopyAssessmentBlocker::AmbiguousFieldCurvature,
        FieldCurvatureBlocker::LowTexture => CopyAssessmentBlocker::LowTexture,
        FieldCurvatureBlocker::UnknownCorrections => CopyAssessmentBlocker::UnknownCorrections,
    }
}

fn field_gate_for_state(
    state: CopyAssessmentState,
    field_gate: CopyAssessmentGate,
) -> CopyAssessmentGate {
    if state == CopyAssessmentState::SupportsCentred {
        CopyAssessmentGate::passed()
    } else {
        field_gate
    }
}

fn consistency_gate(
    state: CopyAssessmentState,
    blockers: &[CopyAssessmentBlocker],
) -> CopyAssessmentGate {
    if blockers.is_empty()
        && matches!(
            state,
            CopyAssessmentState::SupportsCentred | CopyAssessmentState::SupportsDecentred
        )
    {
        CopyAssessmentGate::passed()
    } else {
        gate_for(
            blockers,
            &[
                CopyAssessmentBlocker::InconsistentAsymmetry,
                CopyAssessmentBlocker::AsymmetryAboveCentredThreshold,
                CopyAssessmentBlocker::AsymmetryBelowDecentredThreshold,
            ],
        )
    }
}

fn gate_for(
    blockers: &[CopyAssessmentBlocker],
    relevant: &[CopyAssessmentBlocker],
) -> CopyAssessmentGate {
    let gate_blockers = blockers
        .iter()
        .copied()
        .filter(|blocker| relevant.contains(blocker))
        .collect::<Vec<_>>();
    if gate_blockers.is_empty() {
        CopyAssessmentGate::passed()
    } else {
        CopyAssessmentGate::blocked_with(gate_blockers)
    }
}

fn measurement(value: f32) -> Result<Option<DerivedNumericMeasurement>, CopyAssessmentError> {
    DerivedNumericMeasurement::acutance_delta(value)
        .map(Some)
        .ok_or(CopyAssessmentError::NonFiniteDerivedValue { value })
}

fn mean(samples: impl IntoIterator<Item = f32>) -> Result<f32, CopyAssessmentError> {
    let samples = samples.into_iter().collect::<Vec<_>>();
    if samples.is_empty() {
        return Err(CopyAssessmentError::NonFiniteDerivedValue { value: f32::NAN });
    }
    for sample in &samples {
        validate_finite(*sample)?;
    }
    #[allow(clippy::cast_precision_loss)]
    let len = samples.len() as f32;
    let value = samples.iter().sum::<f32>() / len;
    validate_finite(value)?;
    Ok(value)
}

fn sign_consistent(samples: impl IntoIterator<Item = f32>) -> bool {
    let mut sign = None;
    for sample in samples {
        if sample == 0.0 {
            return false;
        }
        let sample_sign = sample.is_sign_positive();
        if let Some(sign) = sign {
            if sign != sample_sign {
                return false;
            }
        } else {
            sign = Some(sample_sign);
        }
    }
    true
}

fn validate_finite(value: f32) -> Result<(), CopyAssessmentError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(CopyAssessmentError::NonFiniteDerivedValue { value })
    }
}

fn normalize_blockers(blockers: Vec<CopyAssessmentBlocker>) -> Vec<CopyAssessmentBlocker> {
    let mut normalized = Vec::new();
    for blocker in blockers {
        push_blocker(&mut normalized, blocker);
    }
    normalized
}

fn push_blocker(blockers: &mut Vec<CopyAssessmentBlocker>, blocker: CopyAssessmentBlocker) {
    if !blockers.contains(&blocker) {
        blockers.push(blocker);
    }
}

fn inconclusive(blockers: Vec<CopyAssessmentBlocker>) -> CopyAssessmentSupport {
    CopyAssessmentSupport::inconclusive_with_evidence(
        blockers,
        CopyAssessmentEvidence::not_assessed_with_thresholds(
            CENTRED_MAX_ABS_DELTA,
            DECENTRED_MIN_ABS_DELTA,
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{
        CaLateralEvidence, DecentringEvidence, DecentringMethod, DistortionEvidence,
        ExclusionCount, FieldCurvatureMethod, FieldCurvatureSummary, LeftRightDecentring,
        NumericUnit, PairId, ReliabilityBlocker, TargetQaMeasurement, TargetQaMethod,
        TargetQuality, VignettingBlocker, VignettingEvidence, VignettingMethod, VignettingSymmetry,
    };

    #[test]
    fn supports_centred_when_controlled_series_has_small_pair_deltas() {
        let groups = vec![
            group(
                5.6,
                TargetQualityStatus::Passed,
                Some(0.02),
                Some(-0.03),
                vec![],
            ),
            group(
                8.0,
                TargetQualityStatus::Passed,
                Some(0.03),
                Some(-0.02),
                vec![],
            ),
        ];

        let support = assess_copy_support(
            &groups,
            &field_curvature(FieldCurvatureStatus::NotSupported, vec![]),
        )
        .expect("copy support");

        assert_eq!(support.state, CopyAssessmentState::SupportsCentred);
        assert!(support.hard_support_eligible);
        assert!(support.blockers.is_empty());
        assert_eq!(support.included_aperture_groups, vec![5.6, 8.0]);
        assert_eq!(
            support.evidence.field_curvature_counterevidence,
            CopyAssessmentGate::passed()
        );
        assert_delta(support.evidence.max_abs_pair_delta.unwrap().value, 0.03);
        assert_delta(support.evidence.centred_threshold, CENTRED_MAX_ABS_DELTA);
        assert_delta(
            support.evidence.decentred_threshold,
            DECENTRED_MIN_ABS_DELTA,
        );
    }

    #[test]
    fn supports_decentred_when_consistent_pair_deltas_exceed_threshold() {
        let groups = vec![
            group(
                5.6,
                TargetQualityStatus::Passed,
                Some(0.22),
                Some(0.2),
                vec![],
            ),
            group(
                8.0,
                TargetQualityStatus::Passed,
                Some(0.2),
                Some(0.24),
                vec![],
            ),
        ];

        let support = assess_copy_support(
            &groups,
            &field_curvature(FieldCurvatureStatus::NotSupported, vec![]),
        )
        .expect("copy support");

        assert_eq!(support.state, CopyAssessmentState::SupportsDecentred);
        assert!(support.hard_support_eligible);
        assert!(support.blockers.is_empty());
        assert_delta(support.evidence.mean_top_pair_delta.unwrap().value, 0.21);
        assert_delta(support.evidence.mean_bottom_pair_delta.unwrap().value, 0.22);
    }

    #[test]
    fn one_sample_pair_reliability_blocks_hard_support() {
        let groups = vec![
            group_with_reliability_blocker(5.6, TargetQualityStatus::Passed, Some(0.22), Some(0.2)),
            group_with_reliability_blocker(8.0, TargetQualityStatus::Passed, Some(0.2), Some(0.24)),
        ];

        let support = assess_copy_support(
            &groups,
            &field_curvature(FieldCurvatureStatus::NotSupported, vec![]),
        )
        .expect("copy support");

        assert_eq!(support.state, CopyAssessmentState::Inconclusive);
        assert!(!support.hard_support_eligible);
        assert_eq!(
            support.blockers,
            vec![
                CopyAssessmentBlocker::InsufficientSamples,
                CopyAssessmentBlocker::InsufficientApertureSeries,
            ]
        );
        assert_eq!(
            support.evidence.left_right_consistency.blockers,
            vec![CopyAssessmentBlocker::InsufficientSamples]
        );
    }

    #[test]
    fn aperture_sign_flip_blocks_decentred_support() {
        let groups = vec![
            group(
                5.6,
                TargetQualityStatus::Passed,
                Some(0.5),
                Some(0.5),
                vec![],
            ),
            group(
                8.0,
                TargetQualityStatus::Passed,
                Some(-0.1),
                Some(0.3),
                vec![],
            ),
        ];

        let support = assess_copy_support(
            &groups,
            &field_curvature(FieldCurvatureStatus::NotSupported, vec![]),
        )
        .expect("copy support");

        assert_eq!(support.state, CopyAssessmentState::Inconclusive);
        assert!(!support.hard_support_eligible);
        assert_eq!(
            support.blockers,
            vec![CopyAssessmentBlocker::InconsistentAsymmetry]
        );
        assert_eq!(
            support.evidence.left_right_consistency.blockers,
            vec![CopyAssessmentBlocker::InconsistentAsymmetry]
        );
    }

    #[test]
    fn blocks_scene_input_even_when_pair_deltas_are_small() {
        let groups = vec![
            group(
                5.6,
                TargetQualityStatus::NotAssessed,
                Some(0.02),
                Some(0.02),
                vec![],
            ),
            group(
                8.0,
                TargetQualityStatus::NotAssessed,
                Some(0.03),
                Some(0.01),
                vec![],
            ),
        ];

        let support = assess_copy_support(&groups, &FieldCurvatureEvidence::not_assessed())
            .expect("copy support");

        assert_eq!(support.state, CopyAssessmentState::Inconclusive);
        assert!(!support.hard_support_eligible);
        assert_eq!(
            support.blockers,
            vec![
                CopyAssessmentBlocker::TargetQualityNotPassed,
                CopyAssessmentBlocker::InsufficientApertureSeries,
            ]
        );
        assert_eq!(
            support.evidence.target_quality.blockers,
            vec![CopyAssessmentBlocker::TargetQualityNotPassed]
        );
    }

    #[test]
    fn reports_capture_blockers_from_pair_summaries() {
        let groups = vec![
            group(
                5.6,
                TargetQualityStatus::Passed,
                None,
                None,
                vec![ExclusionReason::UnknownCorrections],
            ),
            group(
                8.0,
                TargetQualityStatus::Passed,
                None,
                None,
                vec![ExclusionReason::LowTexture],
            ),
        ];

        let support = assess_copy_support(&groups, &FieldCurvatureEvidence::not_assessed())
            .expect("copy support");

        assert_eq!(support.state, CopyAssessmentState::Inconclusive);
        assert_eq!(
            support.blockers,
            vec![
                CopyAssessmentBlocker::InsufficientSamples,
                CopyAssessmentBlocker::UnknownCorrections,
                CopyAssessmentBlocker::LowTexture,
                CopyAssessmentBlocker::InsufficientApertureSeries,
            ]
        );
        assert_eq!(
            support.reshoot,
            vec![
                crate::schema::CopyAssessmentReshoot::AddRepeatFrames,
                crate::schema::CopyAssessmentReshoot::UseUncorrectedRawInput,
                crate::schema::CopyAssessmentReshoot::AddTexturedCornerCoverage,
                crate::schema::CopyAssessmentReshoot::AddApertureLadder,
            ]
        );
    }

    #[test]
    fn field_curvature_counterevidence_blocks_decentred_support() {
        let groups = vec![
            group(
                5.6,
                TargetQualityStatus::Passed,
                Some(0.22),
                Some(0.2),
                vec![],
            ),
            group(
                8.0,
                TargetQualityStatus::Passed,
                Some(0.2),
                Some(0.24),
                vec![],
            ),
        ];

        let support = assess_copy_support(
            &groups,
            &field_curvature(FieldCurvatureStatus::Supported, vec![]),
        )
        .expect("copy support");

        assert_eq!(support.state, CopyAssessmentState::Inconclusive);
        assert_eq!(
            support.blockers,
            vec![CopyAssessmentBlocker::FieldCurvatureCounterevidence]
        );
        assert_eq!(
            support.evidence.field_curvature_counterevidence.blockers,
            vec![CopyAssessmentBlocker::FieldCurvatureCounterevidence]
        );
    }

    #[test]
    fn ambiguous_field_curvature_blocks_decentred_support() {
        let groups = vec![
            group(
                5.6,
                TargetQualityStatus::Passed,
                Some(0.22),
                Some(0.2),
                vec![],
            ),
            group(
                8.0,
                TargetQualityStatus::Passed,
                Some(0.2),
                Some(0.24),
                vec![],
            ),
        ];

        let support = assess_copy_support(
            &groups,
            &field_curvature(
                FieldCurvatureStatus::Blocked,
                vec![FieldCurvatureBlocker::AmbiguousPeak],
            ),
        )
        .expect("copy support");

        assert_eq!(support.state, CopyAssessmentState::Inconclusive);
        assert_eq!(
            support.blockers,
            vec![CopyAssessmentBlocker::AmbiguousFieldCurvature]
        );
    }

    #[test]
    fn blocked_field_curvature_check_blocks_decentred_support() {
        let groups = vec![
            group(
                5.6,
                TargetQualityStatus::Passed,
                Some(0.22),
                Some(0.2),
                vec![],
            ),
            group(
                8.0,
                TargetQualityStatus::Passed,
                Some(0.2),
                Some(0.24),
                vec![],
            ),
        ];

        let support = assess_copy_support(
            &groups,
            &field_curvature(
                FieldCurvatureStatus::Blocked,
                vec![FieldCurvatureBlocker::LowTexture],
            ),
        )
        .expect("copy support");

        assert_eq!(support.state, CopyAssessmentState::Inconclusive);
        assert_eq!(support.blockers, vec![CopyAssessmentBlocker::LowTexture]);
        assert_eq!(
            support.evidence.field_curvature_counterevidence.blockers,
            vec![CopyAssessmentBlocker::LowTexture]
        );
    }

    #[test]
    fn mixed_lens_or_focal_identity_blocks_hard_support() {
        let mut groups = vec![
            group(
                5.6,
                TargetQualityStatus::Passed,
                Some(0.02),
                Some(0.02),
                vec![],
            ),
            group(
                8.0,
                TargetQualityStatus::Passed,
                Some(0.02),
                Some(0.02),
                vec![],
            ),
        ];
        groups[1].lens_model = Some("Different".to_owned());

        let support = assess_copy_support(&groups, &FieldCurvatureEvidence::not_assessed())
            .expect("copy support");

        assert_eq!(support.state, CopyAssessmentState::Inconclusive);
        assert_eq!(
            support.blockers,
            vec![CopyAssessmentBlocker::MixedLensFocalIdentity]
        );
        assert_eq!(support.lens_model, None);
        assert_eq!(support.focal_length_mm, None);
    }

    #[test]
    fn missing_lens_identity_blocks_without_fake_identity() {
        let mut groups = vec![group(
            5.6,
            TargetQualityStatus::Passed,
            Some(0.02),
            Some(0.02),
            vec![],
        )];
        groups[0].lens_model = None;

        let support = assess_copy_support(&groups, &FieldCurvatureEvidence::not_assessed())
            .expect("copy support");

        assert_eq!(support.state, CopyAssessmentState::Inconclusive);
        assert_eq!(
            support.blockers,
            vec![CopyAssessmentBlocker::MissingLensFocalIdentity]
        );
        assert_eq!(support.lens_model, None);
        assert_eq!(support.focal_length_mm, None);
    }

    #[test]
    fn partial_missing_lens_identity_blocks_hard_support() {
        let mut missing = group(
            11.0,
            TargetQualityStatus::Passed,
            Some(0.02),
            Some(0.02),
            vec![],
        );
        missing.focal_length_mm = None;
        let groups = vec![
            group(
                5.6,
                TargetQualityStatus::Passed,
                Some(0.02),
                Some(0.02),
                vec![],
            ),
            group(
                8.0,
                TargetQualityStatus::Passed,
                Some(0.02),
                Some(0.02),
                vec![],
            ),
            missing,
        ];

        let support = assess_copy_support(&groups, &FieldCurvatureEvidence::not_assessed())
            .expect("copy support");

        assert_eq!(support.state, CopyAssessmentState::Inconclusive);
        assert_eq!(
            support.blockers,
            vec![CopyAssessmentBlocker::MissingLensFocalIdentity]
        );
        assert_eq!(support.included_aperture_groups, Vec::<f32>::new());
    }

    #[test]
    fn rejects_non_finite_pair_delta() {
        let mut invalid_group = group(
            5.6,
            TargetQualityStatus::Passed,
            Some(0.02),
            Some(0.02),
            vec![],
        );
        invalid_group.decentring.left_right.top_pair.mean_delta = Some(DerivedNumericMeasurement {
            value: f32::NAN,
            unit: NumericUnit::AcutanceDelta,
            method: DecentringMethod::DerivedFromMeasuredAcutance,
        });
        let groups = vec![
            invalid_group,
            group(
                8.0,
                TargetQualityStatus::Passed,
                Some(0.02),
                Some(0.02),
                vec![],
            ),
        ];

        let error = assess_copy_support(&groups, &FieldCurvatureEvidence::not_assessed())
            .expect_err("non-finite pair delta rejected");

        assert!(matches!(
            error,
            CopyAssessmentError::NonFiniteDerivedValue { value } if value.is_nan()
        ));
    }

    fn group(
        f_number: f32,
        target_status: TargetQualityStatus,
        top_delta: Option<f32>,
        bottom_delta: Option<f32>,
        exclusions: Vec<ExclusionReason>,
    ) -> AnalyseGroup {
        AnalyseGroup {
            lens_model: Some("Lens 50".to_owned()),
            focal_length_mm: Some(50.0),
            f_number: Some(f_number),
            decentring: DecentringEvidence {
                method: DecentringMethod::DerivedFromMeasuredAcutance,
                target_quality: target_quality(target_status),
                left_right: LeftRightDecentring {
                    top_pair: pair(PairId::TopLeftMinusTopRight, top_delta, exclusions.clone()),
                    bottom_pair: pair(PairId::BottomLeftMinusBottomRight, bottom_delta, exclusions),
                },
            },
            vignetting: vignetting(),
            ca_lateral: CaLateralEvidence::empty(),
            distortion: DistortionEvidence::empty(),
            frames: Vec::new(),
        }
    }

    fn group_with_reliability_blocker(
        f_number: f32,
        target_status: TargetQualityStatus,
        top_delta: Option<f32>,
        bottom_delta: Option<f32>,
    ) -> AnalyseGroup {
        let mut group = group(f_number, target_status, top_delta, bottom_delta, vec![]);
        group.decentring.left_right.top_pair.reliability_blockers =
            vec![ReliabilityBlocker::InsufficientSamples];
        group.decentring.left_right.bottom_pair.reliability_blockers =
            vec![ReliabilityBlocker::InsufficientSamples];
        group
    }

    fn target_quality(status: TargetQualityStatus) -> TargetQuality {
        match status {
            TargetQualityStatus::Passed | TargetQualityStatus::Gated => TargetQuality::assessed(
                status,
                TargetQaMethod::MeasuredPeriodicReferenceScale,
                TargetQaMeasurement::measured_percent(
                    0.5,
                    TargetQaMethod::MeasuredPeriodicReferenceScale,
                    0.9,
                )
                .expect("finite keystone"),
                crate::schema::TiltAxis::Vertical,
                2,
                0,
                Vec::new(),
            ),
            TargetQualityStatus::Blocked | TargetQualityStatus::NotAssessed => {
                TargetQuality::not_assessed()
            }
        }
    }

    fn pair(id: PairId, delta: Option<f32>, exclusions: Vec<ExclusionReason>) -> PairSummary {
        let included_samples = usize::from(delta.is_some());
        PairSummary {
            id,
            included_samples,
            excluded_samples: exclusions.len(),
            mean_delta: delta.and_then(DerivedNumericMeasurement::acutance_delta),
            scatter: delta.and_then(|_| DerivedNumericMeasurement::acutance_delta(0.01)),
            reliability_blockers: Vec::<ReliabilityBlocker>::new(),
            excluded: exclusions
                .into_iter()
                .map(|reason| ExclusionCount { reason, count: 1 })
                .collect(),
        }
    }

    fn field_curvature(
        status: FieldCurvatureStatus,
        blockers: Vec<FieldCurvatureBlocker>,
    ) -> FieldCurvatureEvidence {
        FieldCurvatureEvidence {
            method: FieldCurvatureMethod::InferredApertureLagFromMeasuredAcutance,
            summaries: vec![FieldCurvatureSummary {
                lens_model: Some("Lens 50".to_owned()),
                focal_length_mm: Some(50.0),
                status,
                eligible_aperture_groups: 2,
                excluded_aperture_groups: 0,
                included_f_numbers: vec![5.6, 8.0],
                centre_peak_f_number: Some(5.6),
                corner_mean_peak_f_number: Some(8.0),
                lag_stops: Some(1.0),
                lag_threshold_stops: 1.75,
                blockers,
                excluded: Vec::new(),
            }],
        }
    }

    fn vignetting() -> VignettingEvidence {
        VignettingEvidence {
            method: VignettingMethod::MeasuredLuminanceRatio,
            included_samples: 0,
            excluded_samples: 0,
            reference_f_number: None,
            raw_corner_mean_stops: None,
            optical_delta_from_reference_stops: None,
            blockers: vec![VignettingBlocker::ControlledApertureSeriesNotAssessed],
            warnings: Vec::new(),
            excluded: Vec::new(),
            symmetry: VignettingSymmetry::not_assessed(),
        }
    }

    fn assert_delta(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.000_001,
            "{actual} != {expected}"
        );
    }
}
