use serde::Serialize;

pub const ANALYSE_SCHEMA_VERSION: &str = "0.1-acutance";
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
    pub frames: Vec<FrameMeasurement>,
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
    Ratio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementMethod {
    Measured,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TextureMethod {
    DerivedThreshold,
}

#[cfg(test)]
mod tests {
    use super::{
        AnalyseGroup, AnalyseInput, AnalyseReport, CorrectionStatus, FrameMeasurement,
        Measurements, SharpnessMeasurements, SourceKind, ZoneMeasurement, ZoneMeasurements,
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
                frames: vec![frame(0, "a.dng", true)],
            }],
        );

        let json = serde_json::to_string_pretty(&report).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(json.starts_with("{\n  \"schema_version\": \"0.1-acutance\","));
        assert_eq!(value["schema_version"], "0.1-acutance");
        assert_eq!(value["inputs"][0]["source_kind"], "cfa");
        assert_eq!(value["inputs"][0]["corrections"], "confirmed_uncorrected");
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
        assert!(value.get("generated_utc").is_none());
        assert!(value.get("verdict").is_none());
        assert!(value.get("artifacts").is_none());
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
            value["groups"][0]["frames"][0]["measurements"]["sharpness"]["zones"]["centre"]["acutance"]
                ["confidence"],
            0.0
        );
    }

    #[test]
    fn rejects_non_finite_numeric_measurements() {
        assert!(ZoneMeasurement::measured(f32::NAN, 0.1, true).is_none());
        assert!(ZoneMeasurement::measured(0.1, f32::INFINITY, true).is_none());
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
                    frames: vec![frame(0, "first.tif", false), frame(2, "third.tif", false)],
                },
                AnalyseGroup {
                    lens_model: Some("50mm".to_owned()),
                    focal_length_mm: None,
                    f_number: None,
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
