use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::schema::{
    DecentringEvidence, DerivedNumericMeasurement, ExclusionCount, ExclusionReason,
    FrameMeasurement, LeftRightDecentring, PairId, PairSummary, ReliabilityBlocker,
    ZoneMeasurement,
};

use super::target_qa::{TargetQaError, aggregate_target_quality};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DecentringError {
    NonFiniteAcutance { value: f32 },
    NonFiniteDerivedValue { value: f32 },
    TargetQa(TargetQaError),
}

impl Display for DecentringError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteAcutance { value } => write!(formatter, "non-finite acutance {value}"),
            Self::NonFiniteDerivedValue { value } => {
                write!(formatter, "non-finite derived decentring value {value}")
            }
            Self::TargetQa(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for DecentringError {}

impl From<TargetQaError> for DecentringError {
    fn from(error: TargetQaError) -> Self {
        Self::TargetQa(error)
    }
}

pub fn aggregate_left_right_decentring(
    frames: &[FrameMeasurement],
) -> Result<DecentringEvidence, DecentringError> {
    let mut top_pair = PairAccumulator::new(PairId::TopLeftMinusTopRight);
    let mut bottom_pair = PairAccumulator::new(PairId::BottomLeftMinusBottomRight);

    for frame in frames {
        top_pair.push(
            frame.aggregation_eligible,
            &frame.measurements.sharpness.zones.top_left,
            &frame.measurements.sharpness.zones.top_right,
        )?;
        bottom_pair.push(
            frame.aggregation_eligible,
            &frame.measurements.sharpness.zones.bottom_left,
            &frame.measurements.sharpness.zones.bottom_right,
        )?;
    }

    Ok(DecentringEvidence {
        method: crate::schema::DecentringMethod::DerivedFromMeasuredAcutance,
        target_quality: aggregate_target_quality(frames)?,
        left_right: LeftRightDecentring {
            top_pair: top_pair.finish()?,
            bottom_pair: bottom_pair.finish()?,
        },
    })
}

struct PairAccumulator {
    id: PairId,
    deltas: Vec<f32>,
    unknown_corrections: usize,
    low_texture: usize,
}

impl PairAccumulator {
    fn new(id: PairId) -> Self {
        Self {
            id,
            deltas: Vec::new(),
            unknown_corrections: 0,
            low_texture: 0,
        }
    }

    fn push(
        &mut self,
        aggregation_eligible: bool,
        left: &ZoneMeasurement,
        right: &ZoneMeasurement,
    ) -> Result<(), DecentringError> {
        let left_acutance = finite_acutance(left)?;
        let right_acutance = finite_acutance(right)?;
        if !aggregation_eligible {
            self.unknown_corrections += 1;
            return Ok(());
        }
        if !left.texture_usable.value || !right.texture_usable.value {
            self.low_texture += 1;
            return Ok(());
        }

        let delta = left_acutance - right_acutance;
        if !delta.is_finite() {
            return Err(DecentringError::NonFiniteDerivedValue { value: delta });
        }
        self.deltas.push(delta);
        Ok(())
    }

    fn finish(self) -> Result<PairSummary, DecentringError> {
        let included_samples = self.deltas.len();
        let mean_delta = mean(&self.deltas).map(derived_measurement).transpose()?;
        let scatter = sample_std(&self.deltas)
            .map(derived_measurement)
            .transpose()?;
        let mut excluded = Vec::new();
        if self.unknown_corrections > 0 {
            excluded.push(ExclusionCount {
                reason: ExclusionReason::UnknownCorrections,
                count: self.unknown_corrections,
            });
        }
        if self.low_texture > 0 {
            excluded.push(ExclusionCount {
                reason: ExclusionReason::LowTexture,
                count: self.low_texture,
            });
        }
        let excluded_samples = excluded.iter().map(|count| count.count).sum();
        let reliability_blockers = if included_samples < 2 {
            vec![ReliabilityBlocker::InsufficientSamples]
        } else {
            Vec::new()
        };

        Ok(PairSummary {
            id: self.id,
            included_samples,
            excluded_samples,
            mean_delta,
            scatter,
            reliability_blockers,
            excluded,
        })
    }
}

fn finite_acutance(zone: &ZoneMeasurement) -> Result<f32, DecentringError> {
    let value = zone.acutance.value;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(DecentringError::NonFiniteAcutance { value })
    }
}

fn derived_measurement(value: f32) -> Result<DerivedNumericMeasurement, DecentringError> {
    DerivedNumericMeasurement::acutance_delta(value)
        .ok_or(DecentringError::NonFiniteDerivedValue { value })
}

fn mean(samples: &[f32]) -> Option<f32> {
    if samples.is_empty() {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let len = samples.len() as f32;
    Some(samples.iter().sum::<f32>() / len)
}

fn sample_std(samples: &[f32]) -> Option<f32> {
    if samples.len() < 2 {
        return None;
    }
    let mean = mean(samples)?;
    #[allow(clippy::cast_precision_loss)]
    let denominator = (samples.len() - 1) as f32;
    Some(
        (samples
            .iter()
            .map(|sample| {
                let delta = sample - mean;
                delta * delta
            })
            .sum::<f32>()
            / denominator)
            .sqrt(),
    )
}

#[cfg(test)]
mod tests {
    use super::{DecentringError, aggregate_left_right_decentring};
    use crate::schema::{
        CaBlocker, CaLateralMeasurements, ExclusionReason, FrameMeasurement, FrameQuality,
        MeasurementMethod, Measurements, NumericMeasurement, NumericUnit, SharpnessMeasurements,
        TargetQaEvidence, TargetQaMeasurement, TargetQaMethod, TargetQualityBlocker,
        TargetQualityStatus, TextureMethod, TextureUsable, TiltAxis, ZoneMeasurement,
        ZoneMeasurements,
    };

    fn zone(acutance: f32, contrast: f32) -> ZoneMeasurement {
        ZoneMeasurement::measured(acutance, contrast, 1.0, true).expect("finite zone")
    }

    fn forced_zone(acutance: f32, texture_usable: bool) -> ZoneMeasurement {
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
                confidence: if texture_usable { 1.0 } else { 0.0 },
            },
            texture_usable: TextureUsable {
                value: texture_usable,
                threshold: 0.15,
                method: TextureMethod::DerivedThreshold,
            },
        }
    }

    fn frame(
        aggregation_eligible: bool,
        top_left: ZoneMeasurement,
        top_right: ZoneMeasurement,
        bottom_left: ZoneMeasurement,
        bottom_right: ZoneMeasurement,
    ) -> FrameMeasurement {
        FrameMeasurement {
            input_index: 0,
            path: "frame.dng".to_owned(),
            aggregation_eligible,
            qa: FrameQuality::target_blocked(TargetQualityBlocker::NoSuitableTargetReference),
            measurements: Measurements {
                sharpness: SharpnessMeasurements {
                    zones: ZoneMeasurements {
                        centre: zone(2.0, 0.2),
                        top_left,
                        top_right,
                        bottom_left,
                        bottom_right,
                    },
                },
                vignetting: crate::schema::VignettingMeasurements {
                    zones: crate::schema::VignettingZoneMeasurements {
                        top_left: corner_falloff(-0.5),
                        top_right: corner_falloff(-0.5),
                        bottom_left: corner_falloff(-0.5),
                        bottom_right: corner_falloff(-0.5),
                    },
                },
                ca_lateral: CaLateralMeasurements::blocked_all(CaBlocker::FlatProfile),
                distortion: crate::schema::DistortionMeasurements::blocked(
                    crate::schema::DistortionBlocker::NoStraightReference,
                ),
            },
        }
    }

    fn corner_falloff(value: f32) -> crate::schema::CornerFalloff {
        crate::schema::CornerFalloff {
            falloff: crate::schema::VignettingNumericMeasurement::measured_stops(value)
                .expect("finite falloff"),
        }
    }

    fn eligible_frame(top_delta: f32, bottom_delta: f32) -> FrameMeasurement {
        frame(
            true,
            zone(1.0 + top_delta, 0.2),
            zone(1.0, 0.2),
            zone(1.0 + bottom_delta, 0.2),
            zone(1.0, 0.2),
        )
    }

    fn target_measurement(value: f32) -> TargetQaMeasurement {
        TargetQaMeasurement::measured_percent(
            value,
            TargetQaMethod::MeasuredPeriodicReferenceScale,
            0.8,
        )
        .expect("finite target QA measurement")
    }

    fn frame_with_target(aggregation_eligible: bool, target: TargetQaEvidence) -> FrameMeasurement {
        let mut frame = frame(
            aggregation_eligible,
            zone(1.2, 0.2),
            zone(1.0, 0.2),
            zone(1.3, 0.2),
            zone(1.0, 0.2),
        );
        frame.qa.target = target;
        frame
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1.0e-6,
            "actual {actual} expected {expected}"
        );
    }

    #[test]
    fn one_eligible_frame_produces_signed_deltas_without_scatter() {
        let evidence =
            aggregate_left_right_decentring(&[eligible_frame(0.2, -0.3)]).expect("aggregate");

        let top = evidence.left_right.top_pair;
        let bottom = evidence.left_right.bottom_pair;
        assert_eq!(top.included_samples, 1);
        assert_close(top.mean_delta.unwrap().value, 0.2);
        assert!(top.scatter.is_none());
        assert_eq!(top.reliability_blockers.len(), 1);
        assert_close(bottom.mean_delta.unwrap().value, -0.3);
        assert!(bottom.scatter.is_none());
    }

    #[test]
    fn multiple_eligible_frames_aggregate_mean_and_sample_scatter() {
        let evidence = aggregate_left_right_decentring(&[
            eligible_frame(0.2, -0.1),
            eligible_frame(0.4, -0.3),
        ])
        .expect("aggregate");

        let top = evidence.left_right.top_pair;
        let bottom = evidence.left_right.bottom_pair;
        assert_eq!(top.included_samples, 2);
        assert_close(top.mean_delta.unwrap().value, 0.3);
        assert_close(top.scatter.unwrap().value, 0.141_421_35);
        assert!(top.reliability_blockers.is_empty());
        assert_close(bottom.mean_delta.unwrap().value, -0.2);
        assert_close(bottom.scatter.unwrap().value, 0.141_421_36);
    }

    #[test]
    fn low_texture_excludes_only_the_affected_pair() {
        let evidence = aggregate_left_right_decentring(&[frame(
            true,
            zone(1.2, 0.2),
            zone(1.0, 0.2),
            forced_zone(1.3, false),
            zone(1.0, 0.2),
        )])
        .expect("aggregate");

        assert_eq!(evidence.left_right.top_pair.included_samples, 1);
        assert_eq!(evidence.left_right.top_pair.excluded_samples, 0);
        assert_eq!(evidence.left_right.bottom_pair.included_samples, 0);
        assert_eq!(evidence.left_right.bottom_pair.excluded_samples, 1);
        assert_eq!(
            evidence.left_right.bottom_pair.excluded[0].reason,
            ExclusionReason::LowTexture
        );
        assert!(evidence.left_right.bottom_pair.mean_delta.is_none());
        assert!(evidence.left_right.bottom_pair.scatter.is_none());
    }

    #[test]
    fn unknown_corrections_exclude_both_pairs() {
        let evidence = aggregate_left_right_decentring(&[frame(
            false,
            zone(1.2, 0.2),
            zone(1.0, 0.2),
            zone(1.3, 0.2),
            zone(1.0, 0.2),
        )])
        .expect("aggregate");

        assert_eq!(evidence.left_right.top_pair.included_samples, 0);
        assert_eq!(evidence.left_right.top_pair.excluded_samples, 1);
        assert_eq!(
            evidence.left_right.top_pair.excluded[0].reason,
            ExclusionReason::UnknownCorrections
        );
        assert_eq!(evidence.left_right.bottom_pair.included_samples, 0);
        assert_eq!(evidence.left_right.bottom_pair.excluded_samples, 1);
    }

    #[test]
    fn empty_group_produces_blocked_zero_sample_evidence() {
        let evidence = aggregate_left_right_decentring(&[]).expect("aggregate");

        assert_eq!(evidence.left_right.top_pair.included_samples, 0);
        assert_eq!(evidence.left_right.top_pair.excluded_samples, 0);
        assert!(evidence.left_right.top_pair.mean_delta.is_none());
        assert!(evidence.left_right.top_pair.scatter.is_none());
        assert_eq!(evidence.left_right.top_pair.reliability_blockers.len(), 1);
    }

    #[test]
    fn non_finite_acutance_is_rejected() {
        let err = aggregate_left_right_decentring(&[frame(
            true,
            forced_zone(f32::NAN, true),
            zone(1.0, 0.2),
            zone(1.0, 0.2),
            zone(1.0, 0.2),
        )])
        .expect_err("non-finite acutance rejected");

        assert!(matches!(err, DecentringError::NonFiniteAcutance { .. }));
    }

    #[test]
    fn non_finite_low_texture_acutance_is_rejected_before_exclusion() {
        let err = aggregate_left_right_decentring(&[frame(
            true,
            forced_zone(f32::NAN, false),
            zone(1.0, 0.2),
            zone(1.0, 0.2),
            zone(1.0, 0.2),
        )])
        .expect_err("non-finite excluded acutance rejected");

        assert!(matches!(err, DecentringError::NonFiniteAcutance { .. }));
    }

    #[test]
    fn non_finite_ineligible_acutance_is_rejected_before_exclusion() {
        let err = aggregate_left_right_decentring(&[frame(
            false,
            zone(1.0, 0.2),
            zone(1.0, 0.2),
            forced_zone(f32::INFINITY, true),
            zone(1.0, 0.2),
        )])
        .expect_err("non-finite ineligible acutance rejected");

        assert!(matches!(err, DecentringError::NonFiniteAcutance { .. }));
    }

    #[test]
    fn passed_target_qa_makes_group_target_quality_pass() {
        let evidence = aggregate_left_right_decentring(&[frame_with_target(
            true,
            TargetQaEvidence::passed(
                TargetQaMethod::MeasuredPeriodicReferenceScale,
                target_measurement(0.4),
                TiltAxis::Vertical,
            ),
        )])
        .expect("aggregate");

        assert_eq!(evidence.target_quality.status, TargetQualityStatus::Passed);
        assert_eq!(evidence.target_quality.assessed_frames, 1);
        assert_eq!(evidence.target_quality.blocked_frames, 0);
        assert_close(evidence.target_quality.keystone_pct.unwrap().value, 0.4);
    }

    #[test]
    fn gated_target_qa_takes_precedence_over_passed_and_blocked_frames() {
        let evidence = aggregate_left_right_decentring(&[
            frame_with_target(
                true,
                TargetQaEvidence::passed(
                    TargetQaMethod::MeasuredPeriodicReferenceScale,
                    target_measurement(0.4),
                    TiltAxis::Vertical,
                ),
            ),
            frame_with_target(
                true,
                TargetQaEvidence::gated(
                    TargetQaMethod::MeasuredPeriodicReferenceScale,
                    target_measurement(3.0),
                    TiltAxis::Vertical,
                ),
            ),
            frame_with_target(
                true,
                TargetQaEvidence::blocked(TargetQualityBlocker::NoSuitableTargetReference),
            ),
        ])
        .expect("aggregate");

        assert_eq!(evidence.target_quality.status, TargetQualityStatus::Gated);
        assert_eq!(evidence.target_quality.assessed_frames, 2);
        assert_eq!(evidence.target_quality.blocked_frames, 1);
        assert_close(evidence.target_quality.keystone_pct.unwrap().value, 3.0);
        assert!(
            evidence
                .target_quality
                .blockers
                .contains(&TargetQualityBlocker::KeystoneAboveThreshold)
        );
    }

    #[test]
    fn blocked_target_geometry_preserves_blocker_counts() {
        let evidence = aggregate_left_right_decentring(&[frame_with_target(
            true,
            TargetQaEvidence::blocked(TargetQualityBlocker::NoSuitableTargetReference),
        )])
        .expect("aggregate");

        assert_eq!(evidence.target_quality.status, TargetQualityStatus::Blocked);
        assert_eq!(evidence.target_quality.assessed_frames, 0);
        assert_eq!(evidence.target_quality.blocked_frames, 1);
        assert_eq!(
            evidence.target_quality.blockers,
            vec![TargetQualityBlocker::NoSuitableTargetReference]
        );
    }

    #[test]
    fn unknown_correction_target_observations_cannot_pass_group_quality() {
        let evidence = aggregate_left_right_decentring(&[frame_with_target(
            false,
            TargetQaEvidence::passed(
                TargetQaMethod::MeasuredPeriodicReferenceScale,
                target_measurement(0.4),
                TiltAxis::Vertical,
            ),
        )])
        .expect("aggregate");

        assert_eq!(evidence.target_quality.status, TargetQualityStatus::Blocked);
        assert_eq!(evidence.target_quality.assessed_frames, 1);
        assert_eq!(evidence.target_quality.blocked_frames, 1);
        assert_eq!(
            evidence.target_quality.blockers,
            vec![TargetQualityBlocker::UnknownCorrections]
        );
    }

    #[test]
    fn unknown_correction_blocked_target_preserves_geometry_and_provenance_blockers() {
        let evidence = aggregate_left_right_decentring(&[frame_with_target(
            false,
            TargetQaEvidence::blocked(TargetQualityBlocker::NoSuitableTargetReference),
        )])
        .expect("aggregate");

        assert_eq!(evidence.target_quality.status, TargetQualityStatus::Blocked);
        assert_eq!(evidence.target_quality.assessed_frames, 0);
        assert_eq!(evidence.target_quality.blocked_frames, 1);
        assert_eq!(
            evidence.target_quality.blockers,
            vec![
                TargetQualityBlocker::UnknownCorrections,
                TargetQualityBlocker::NoSuitableTargetReference,
            ]
        );
    }

    #[test]
    fn inconsistent_assessed_target_qa_is_rejected_before_aggregation() {
        let target = TargetQaEvidence::passed(
            TargetQaMethod::MeasuredPeriodicReferenceScale,
            target_measurement(3.0),
            TiltAxis::Vertical,
        );
        let err = aggregate_left_right_decentring(&[frame_with_target(true, target)])
            .expect_err("inconsistent assessed target QA rejected");

        assert!(matches!(err, DecentringError::TargetQa(_)));
    }

    #[test]
    fn non_finite_target_qa_is_rejected_before_unknown_correction_blocking() {
        let mut target = TargetQaEvidence::passed(
            TargetQaMethod::MeasuredPeriodicReferenceScale,
            target_measurement(0.4),
            TiltAxis::Vertical,
        );
        target.keystone_pct = Some(TargetQaMeasurement {
            value: f32::NAN,
            unit: NumericUnit::Percent,
            method: TargetQaMethod::MeasuredPeriodicReferenceScale,
            confidence: 0.8,
        });
        let err = aggregate_left_right_decentring(&[frame_with_target(false, target)])
            .expect_err("non-finite target QA rejected");

        assert!(matches!(err, DecentringError::TargetQa(_)));
    }
}
