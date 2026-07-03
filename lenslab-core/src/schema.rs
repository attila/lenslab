use serde::Serialize;

pub const ANALYSE_SCHEMA_VERSION: &str = "0.1-decentring";
const TEXTURE_USABLE_THRESHOLD: f32 = 0.15;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AnalyseReport {
    pub schema_version: &'static str,
    pub tool_version: String,
    pub inputs: Vec<AnalyseInput>,
    pub groups: Vec<AnalyseGroup>,
}

impl AnalyseReport {
    #[must_use]
    pub fn new(
        tool_version: impl Into<String>,
        inputs: Vec<AnalyseInput>,
        groups: Vec<AnalyseGroup>,
    ) -> Self {
        Self {
            schema_version: ANALYSE_SCHEMA_VERSION,
            tool_version: tool_version.into(),
            inputs,
            groups,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AnalyseInput {
    pub index: usize,
    pub path: String,
    pub source_kind: SourceKind,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_model: Option<String>,
    pub focal_length_mm: Option<f32>,
    pub f_number: Option<f32>,
    pub corrections: CorrectionStatus,
    pub correction_provenance: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AnalyseGroup {
    pub lens_model: Option<String>,
    pub focal_length_mm: Option<f32>,
    pub f_number: Option<f32>,
    pub decentring: DecentringEvidence,
    pub frames: Vec<FrameMeasurement>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DecentringEvidence {
    pub method: DecentringMethod,
    pub target_quality: TargetQuality,
    pub left_right: LeftRightDecentring,
}

impl DecentringEvidence {
    #[must_use]
    pub fn not_assessed(left_right: LeftRightDecentring) -> Self {
        Self {
            method: DecentringMethod::DerivedFromMeasuredAcutance,
            target_quality: TargetQuality::not_assessed(),
            left_right,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TargetQuality {
    pub status: TargetQualityStatus,
    pub blockers: Vec<TargetQualityBlocker>,
}

impl TargetQuality {
    #[must_use]
    pub fn not_assessed() -> Self {
        Self {
            status: TargetQualityStatus::NotAssessed,
            blockers: vec![TargetQualityBlocker::KeystoneNotAssessed],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LeftRightDecentring {
    pub top_pair: PairSummary,
    pub bottom_pair: PairSummary,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PairSummary {
    pub id: PairId,
    pub included_samples: usize,
    pub excluded_samples: usize,
    pub mean_delta: Option<DerivedNumericMeasurement>,
    pub scatter: Option<DerivedNumericMeasurement>,
    pub reliability_blockers: Vec<ReliabilityBlocker>,
    pub excluded: Vec<ExclusionCount>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExclusionCount {
    pub reason: ExclusionReason,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DerivedNumericMeasurement {
    pub value: f32,
    pub unit: NumericUnit,
    pub method: DecentringMethod,
}

impl DerivedNumericMeasurement {
    #[must_use]
    pub fn acutance_delta(value: f32) -> Option<Self> {
        value.is_finite().then_some(Self {
            value,
            unit: NumericUnit::AcutanceDelta,
            method: DecentringMethod::DerivedFromMeasuredAcutance,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FrameMeasurement {
    pub input_index: usize,
    pub path: String,
    pub aggregation_eligible: bool,
    pub measurements: Measurements,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Measurements {
    pub sharpness: SharpnessMeasurements,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SharpnessMeasurements {
    pub zones: ZoneMeasurements,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ZoneMeasurements {
    pub centre: ZoneMeasurement,
    pub top_left: ZoneMeasurement,
    pub top_right: ZoneMeasurement,
    pub bottom_left: ZoneMeasurement,
    pub bottom_right: ZoneMeasurement,
}

impl ZoneMeasurements {
    #[must_use]
    pub fn from_ordered(zones: [ZoneMeasurement; 5]) -> Self {
        let [centre, top_left, top_right, bottom_left, bottom_right] = zones;
        Self {
            centre,
            top_left,
            top_right,
            bottom_left,
            bottom_right,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ZoneMeasurement {
    pub acutance: NumericMeasurement,
    pub contrast: NumericMeasurement,
    pub texture_usable: TextureUsable,
}

impl ZoneMeasurement {
    #[must_use]
    pub fn measured(acutance: f32, contrast: f32, aggregation_eligible: bool) -> Option<Self> {
        if !acutance.is_finite() || !contrast.is_finite() {
            return None;
        }
        let texture_usable = contrast >= TEXTURE_USABLE_THRESHOLD;
        let confidence = if aggregation_eligible && texture_usable {
            1.0
        } else {
            0.0
        };
        Some(Self {
            acutance: NumericMeasurement {
                value: acutance,
                unit: NumericUnit::Acutance,
                method: MeasurementMethod::Measured,
                confidence,
            },
            contrast: NumericMeasurement {
                value: contrast,
                unit: NumericUnit::Ratio,
                method: MeasurementMethod::Measured,
                confidence,
            },
            texture_usable: TextureUsable {
                value: texture_usable,
                threshold: TEXTURE_USABLE_THRESHOLD,
                method: TextureMethod::DerivedThreshold,
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NumericMeasurement {
    pub value: f32,
    pub unit: NumericUnit,
    pub method: MeasurementMethod,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TextureUsable {
    pub value: bool,
    pub threshold: f32,
    pub method: TextureMethod,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Cfa,
    Rgb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CorrectionStatus {
    ConfirmedUncorrected,
    AcceptedUnknownCorrections,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NumericUnit {
    Acutance,
    AcutanceDelta,
    Ratio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementMethod {
    Measured,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DecentringMethod {
    DerivedFromMeasuredAcutance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetQualityStatus {
    NotAssessed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetQualityBlocker {
    KeystoneNotAssessed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PairId {
    TopLeftMinusTopRight,
    BottomLeftMinusBottomRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReliabilityBlocker {
    InsufficientSamples,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExclusionReason {
    UnknownCorrections,
    LowTexture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TextureMethod {
    DerivedThreshold,
}

#[cfg(test)]
mod tests {
    use super::{
        AnalyseGroup, AnalyseInput, AnalyseReport, CorrectionStatus, DecentringEvidence,
        DerivedNumericMeasurement, ExclusionCount, ExclusionReason, FrameMeasurement,
        LeftRightDecentring, Measurements, PairId, PairSummary, ReliabilityBlocker,
        SharpnessMeasurements, SourceKind, ZoneMeasurement, ZoneMeasurements,
    };

    fn zone(acutance: f32, contrast: f32, eligible: bool) -> ZoneMeasurement {
        ZoneMeasurement::measured(acutance, contrast, eligible).unwrap()
    }

    fn zones(eligible: bool) -> ZoneMeasurements {
        ZoneMeasurements::from_ordered([
            zone(2.0, 0.2, eligible),
            zone(1.0, 0.1, eligible),
            zone(1.1, 0.2, eligible),
            zone(1.2, 0.3, eligible),
            zone(1.3, 0.4, eligible),
        ])
    }

    fn frame(input_index: usize, path: &str, eligible: bool) -> FrameMeasurement {
        FrameMeasurement {
            input_index,
            path: path.to_owned(),
            aggregation_eligible: eligible,
            measurements: Measurements {
                sharpness: SharpnessMeasurements {
                    zones: zones(eligible),
                },
            },
        }
    }

    fn pair(
        id: PairId,
        included_samples: usize,
        excluded: Vec<ExclusionCount>,
        mean_delta: Option<f32>,
        scatter: Option<f32>,
    ) -> PairSummary {
        let reliability_blockers = if included_samples < 2 {
            vec![ReliabilityBlocker::InsufficientSamples]
        } else {
            vec![]
        };
        let excluded_samples = excluded.iter().map(|count| count.count).sum();
        PairSummary {
            id,
            included_samples,
            excluded_samples,
            mean_delta: mean_delta.map(|value| {
                DerivedNumericMeasurement::acutance_delta(value).expect("finite mean delta")
            }),
            scatter: scatter.map(|value| {
                DerivedNumericMeasurement::acutance_delta(value).expect("finite scatter")
            }),
            reliability_blockers,
            excluded,
        }
    }

    fn decentring(top_pair: PairSummary, bottom_pair: PairSummary) -> DecentringEvidence {
        DecentringEvidence::not_assessed(LeftRightDecentring {
            top_pair,
            bottom_pair,
        })
    }

    fn two_sample_decentring() -> DecentringEvidence {
        decentring(
            pair(
                PairId::TopLeftMinusTopRight,
                2,
                vec![],
                Some(0.05),
                Some(0.01),
            ),
            pair(
                PairId::BottomLeftMinusBottomRight,
                2,
                vec![],
                Some(-0.03),
                Some(0.02),
            ),
        )
    }

    #[test]
    fn serialises_confirmed_uncorrected_skeleton_in_field_order() {
        let report = AnalyseReport::new(
            "0.1.0",
            vec![AnalyseInput {
                index: 0,
                path: "a.dng".to_owned(),
                source_kind: SourceKind::Cfa,
                camera_make: Some("Pentax".to_owned()),
                camera_model: Some("K-1".to_owned()),
                lens_model: Some("50mm".to_owned()),
                focal_length_mm: Some(50.0),
                f_number: Some(8.0),
                corrections: CorrectionStatus::ConfirmedUncorrected,
                correction_provenance: None,
            }],
            vec![AnalyseGroup {
                lens_model: Some("50mm".to_owned()),
                focal_length_mm: Some(50.0),
                f_number: Some(8.0),
                decentring: two_sample_decentring(),
                frames: vec![frame(0, "a.dng", true)],
            }],
        );

        let json = serde_json::to_string_pretty(&report).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(json.starts_with("{\n  \"schema_version\": \"0.1-decentring\","));
        assert_eq!(value["schema_version"], "0.1-decentring");
        assert_eq!(value["inputs"][0]["source_kind"], "cfa");
        assert_eq!(value["inputs"][0]["corrections"], "confirmed_uncorrected");
        assert!(
            json.find("\"f_number\"")
                .expect("group f_number is serialised")
                < json
                    .find("\"decentring\"")
                    .expect("group decentring is serialised")
        );
        assert!(
            json.find("\"decentring\"")
                .expect("group decentring is serialised")
                < json
                    .find("\"frames\"")
                    .expect("group frames are serialised")
        );
        assert_eq!(
            value["groups"][0]["decentring"]["method"],
            "derived_from_measured_acutance"
        );
        assert_eq!(
            value["groups"][0]["decentring"]["target_quality"]["status"],
            "not_assessed"
        );
        assert_eq!(
            value["groups"][0]["decentring"]["target_quality"]["blockers"][0],
            "keystone_not_assessed"
        );
        assert_eq!(
            value["groups"][0]["decentring"]["left_right"]["top_pair"]["id"],
            "top_left_minus_top_right"
        );
        assert_eq!(
            value["groups"][0]["decentring"]["left_right"]["top_pair"]["mean_delta"]["unit"],
            "acutance_delta"
        );
        assert_eq!(
            value["groups"][0]["decentring"]["left_right"]["top_pair"]["mean_delta"]["method"],
            "derived_from_measured_acutance"
        );
        assert_eq!(
            value["groups"][0]["decentring"]["left_right"]["top_pair"]["reliability_blockers"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        assert_eq!(value["groups"][0]["frames"][0]["input_index"], 0);
        assert_eq!(
            value["groups"][0]["frames"][0]["measurements"]["sharpness"]["zones"]["centre"]["acutance"]
                ["unit"],
            "acutance"
        );
        assert_eq!(
            value["groups"][0]["frames"][0]["measurements"]["sharpness"]["zones"]["centre"]["contrast"]
                ["unit"],
            "ratio"
        );
        assert_eq!(
            value["groups"][0]["frames"][0]["measurements"]["sharpness"]["zones"]["top_left"]["acutance"]
                ["confidence"],
            0.0
        );
        assert_eq!(
            value["groups"][0]["frames"][0]["measurements"]["sharpness"]["zones"]["centre"]["texture_usable"]
                ["method"],
            "derived_threshold"
        );
    }

    #[test]
    fn serialises_no_future_verdict_or_artifact_fields() {
        let report = AnalyseReport::new("0.1.0", vec![], vec![]);
        let value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string_pretty(&report).unwrap()).unwrap();

        for key in [
            "generated_utc",
            "verdict",
            "copy",
            "centred",
            "decentred",
            "confidence",
            "artifacts",
        ] {
            assert!(value.get(key).is_none(), "{key}");
        }
    }

    #[test]
    fn serialises_one_sample_decentring_without_scatter() {
        let report = AnalyseReport::new(
            "0.1.0",
            vec![],
            vec![AnalyseGroup {
                lens_model: None,
                focal_length_mm: None,
                f_number: None,
                decentring: decentring(
                    pair(PairId::TopLeftMinusTopRight, 1, vec![], Some(0.05), None),
                    pair(
                        PairId::BottomLeftMinusBottomRight,
                        1,
                        vec![ExclusionCount {
                            reason: ExclusionReason::LowTexture,
                            count: 1,
                        }],
                        Some(-0.03),
                        None,
                    ),
                ),
                frames: vec![frame(0, "a.dng", true)],
            }],
        );

        let value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string_pretty(&report).unwrap()).unwrap();
        let top_pair = &value["groups"][0]["decentring"]["left_right"]["top_pair"];
        let bottom_pair = &value["groups"][0]["decentring"]["left_right"]["bottom_pair"];

        assert_eq!(top_pair["included_samples"], 1);
        assert_eq!(top_pair["scatter"], serde_json::Value::Null);
        assert_eq!(top_pair["reliability_blockers"][0], "insufficient_samples");
        assert_eq!(bottom_pair["excluded_samples"], 1);
        assert_eq!(bottom_pair["excluded"][0]["reason"], "low_texture");
        assert_eq!(bottom_pair["excluded"][0]["count"], 1);
    }

    #[test]
    fn serialises_unknown_corrections_as_ineligible_with_provenance() {
        let report = AnalyseReport::new(
            "0.1.0",
            vec![AnalyseInput {
                index: 0,
                path: "a.tif".to_owned(),
                source_kind: SourceKind::Rgb,
                camera_make: None,
                camera_model: None,
                lens_model: None,
                focal_length_mm: None,
                f_number: None,
                corrections: CorrectionStatus::AcceptedUnknownCorrections,
                correction_provenance: Some(
                    "TIFF metadata has no reliable correction flag".to_owned(),
                ),
            }],
            vec![AnalyseGroup {
                lens_model: None,
                focal_length_mm: None,
                f_number: None,
                decentring: decentring(
                    pair(
                        PairId::TopLeftMinusTopRight,
                        0,
                        vec![ExclusionCount {
                            reason: ExclusionReason::UnknownCorrections,
                            count: 1,
                        }],
                        None,
                        None,
                    ),
                    pair(
                        PairId::BottomLeftMinusBottomRight,
                        0,
                        vec![ExclusionCount {
                            reason: ExclusionReason::UnknownCorrections,
                            count: 1,
                        }],
                        None,
                        None,
                    ),
                ),
                frames: vec![frame(0, "a.tif", false)],
            }],
        );

        let value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string_pretty(&report).unwrap()).unwrap();

        assert_eq!(
            value["inputs"][0]["corrections"],
            "accepted_unknown_corrections"
        );
        assert_eq!(
            value["inputs"][0]["correction_provenance"],
            "TIFF metadata has no reliable correction flag"
        );
        assert_eq!(
            value["groups"][0]["frames"][0]["aggregation_eligible"],
            false
        );
        assert_eq!(
            value["groups"][0]["decentring"]["left_right"]["top_pair"]["included_samples"],
            0
        );
        assert_eq!(
            value["groups"][0]["decentring"]["left_right"]["top_pair"]["mean_delta"],
            serde_json::Value::Null
        );
        assert_eq!(
            value["groups"][0]["decentring"]["left_right"]["top_pair"]["scatter"],
            serde_json::Value::Null
        );
        assert_eq!(
            value["groups"][0]["decentring"]["left_right"]["top_pair"]["excluded"][0]["reason"],
            "unknown_corrections"
        );
        assert_eq!(
            value["groups"][0]["frames"][0]["measurements"]["sharpness"]["zones"]["centre"]["acutance"]
                ["confidence"],
            0.0
        );
    }

    #[test]
    fn rejects_non_finite_numeric_measurements() {
        assert!(ZoneMeasurement::measured(f32::NAN, 0.1, true).is_none());
        assert!(ZoneMeasurement::measured(0.1, f32::INFINITY, true).is_none());
        assert!(DerivedNumericMeasurement::acutance_delta(f32::NAN).is_none());
        assert!(DerivedNumericMeasurement::acutance_delta(f32::NEG_INFINITY).is_none());
    }

    #[test]
    fn grouping_shape_preserves_first_seen_order_and_null_equality() {
        let report = AnalyseReport::new(
            "0.1.0",
            vec![],
            vec![
                AnalyseGroup {
                    lens_model: None,
                    focal_length_mm: None,
                    f_number: None,
                    decentring: two_sample_decentring(),
                    frames: vec![frame(0, "first.tif", false), frame(2, "third.tif", false)],
                },
                AnalyseGroup {
                    lens_model: Some("50mm".to_owned()),
                    focal_length_mm: None,
                    f_number: None,
                    decentring: two_sample_decentring(),
                    frames: vec![frame(1, "second.tif", false)],
                },
            ],
        );

        let value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string_pretty(&report).unwrap()).unwrap();

        assert_eq!(value["groups"][0]["lens_model"], serde_json::Value::Null);
        assert_eq!(value["groups"][0]["frames"][0]["input_index"], 0);
        assert_eq!(value["groups"][0]["frames"][1]["input_index"], 2);
        assert_eq!(value["groups"][1]["lens_model"], "50mm");
    }
}
