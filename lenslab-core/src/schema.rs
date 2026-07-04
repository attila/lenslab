use serde::Serialize;

pub const ANALYSE_SCHEMA_VERSION: &str = "0.1-vignetting-control";
const TEXTURE_USABLE_THRESHOLD: f32 = 0.15;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AnalyseReport {
    pub schema_version: &'static str,
    pub tool_version: String,
    pub inputs: Vec<AnalyseInput>,
    pub field_curvature: FieldCurvatureEvidence,
    pub groups: Vec<AnalyseGroup>,
}

impl AnalyseReport {
    #[must_use]
    pub fn new(
        tool_version: impl Into<String>,
        inputs: Vec<AnalyseInput>,
        field_curvature: FieldCurvatureEvidence,
        groups: Vec<AnalyseGroup>,
    ) -> Self {
        Self {
            schema_version: ANALYSE_SCHEMA_VERSION,
            tool_version: tool_version.into(),
            inputs,
            field_curvature,
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
    pub vignetting: VignettingEvidence,
    pub ca_lateral: CaLateralEvidence,
    pub distortion: DistortionEvidence,
    pub frames: Vec<FrameMeasurement>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldCurvatureEvidence {
    pub method: FieldCurvatureMethod,
    pub summaries: Vec<FieldCurvatureSummary>,
}

impl FieldCurvatureEvidence {
    #[must_use]
    pub fn not_assessed() -> Self {
        Self {
            method: FieldCurvatureMethod::InferredApertureLagFromMeasuredAcutance,
            summaries: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldCurvatureSummary {
    pub lens_model: Option<String>,
    pub focal_length_mm: Option<f32>,
    pub status: FieldCurvatureStatus,
    pub eligible_aperture_groups: usize,
    pub excluded_aperture_groups: usize,
    pub included_f_numbers: Vec<f32>,
    pub centre_peak_f_number: Option<f32>,
    pub corner_mean_peak_f_number: Option<f32>,
    pub lag_stops: Option<f32>,
    pub lag_threshold_stops: f32,
    pub blockers: Vec<FieldCurvatureBlocker>,
    pub excluded: Vec<ExclusionCount>,
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
    pub method: Option<TargetQaMethod>,
    pub keystone_pct: Option<TargetQaMeasurement>,
    pub tilt_axis: Option<TiltAxis>,
    pub gate_threshold_pct: f32,
    pub assessed_frames: usize,
    pub blocked_frames: usize,
    pub blockers: Vec<TargetQualityBlocker>,
}

impl TargetQuality {
    pub const GATE_THRESHOLD_PCT: f32 = 1.5;

    #[must_use]
    pub fn not_assessed() -> Self {
        Self {
            status: TargetQualityStatus::NotAssessed,
            method: None,
            keystone_pct: None,
            tilt_axis: None,
            gate_threshold_pct: Self::GATE_THRESHOLD_PCT,
            assessed_frames: 0,
            blocked_frames: 0,
            blockers: vec![TargetQualityBlocker::KeystoneNotAssessed],
        }
    }

    #[must_use]
    pub fn assessed(
        status: TargetQualityStatus,
        method: TargetQaMethod,
        keystone_pct: TargetQaMeasurement,
        tilt_axis: TiltAxis,
        assessed_frames: usize,
        blocked_frames: usize,
        blockers: Vec<TargetQualityBlocker>,
    ) -> Self {
        Self {
            status,
            method: Some(method),
            keystone_pct: Some(keystone_pct),
            tilt_axis: Some(tilt_axis),
            gate_threshold_pct: Self::GATE_THRESHOLD_PCT,
            assessed_frames,
            blocked_frames,
            blockers,
        }
    }

    #[must_use]
    pub fn blocked(
        assessed_frames: usize,
        blocked_frames: usize,
        blockers: Vec<TargetQualityBlocker>,
    ) -> Self {
        Self {
            status: TargetQualityStatus::Blocked,
            method: None,
            keystone_pct: None,
            tilt_axis: None,
            gate_threshold_pct: Self::GATE_THRESHOLD_PCT,
            assessed_frames,
            blocked_frames,
            blockers,
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
    pub qa: FrameQuality,
    pub measurements: Measurements,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FrameQuality {
    pub target: TargetQaEvidence,
}

impl FrameQuality {
    #[must_use]
    pub fn target_blocked(blocker: TargetQualityBlocker) -> Self {
        Self {
            target: TargetQaEvidence::blocked(blocker),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TargetQaEvidence {
    pub method: Option<TargetQaMethod>,
    pub status: TargetQualityStatus,
    pub keystone_pct: Option<TargetQaMeasurement>,
    pub tilt_axis: Option<TiltAxis>,
    pub gate_threshold_pct: f32,
    pub blockers: Vec<TargetQualityBlocker>,
}

impl TargetQaEvidence {
    #[must_use]
    pub fn passed(
        method: TargetQaMethod,
        keystone_pct: TargetQaMeasurement,
        tilt_axis: TiltAxis,
    ) -> Self {
        Self::assessed(TargetQualityStatus::Passed, method, keystone_pct, tilt_axis)
    }

    #[must_use]
    pub fn gated(
        method: TargetQaMethod,
        keystone_pct: TargetQaMeasurement,
        tilt_axis: TiltAxis,
    ) -> Self {
        Self::assessed(TargetQualityStatus::Gated, method, keystone_pct, tilt_axis)
    }

    #[must_use]
    pub fn blocked(blocker: TargetQualityBlocker) -> Self {
        Self {
            method: None,
            status: TargetQualityStatus::Blocked,
            keystone_pct: None,
            tilt_axis: None,
            gate_threshold_pct: TargetQuality::GATE_THRESHOLD_PCT,
            blockers: vec![blocker],
        }
    }

    #[must_use]
    pub fn blocked_with(blockers: Vec<TargetQualityBlocker>) -> Self {
        Self {
            method: None,
            status: TargetQualityStatus::Blocked,
            keystone_pct: None,
            tilt_axis: None,
            gate_threshold_pct: TargetQuality::GATE_THRESHOLD_PCT,
            blockers,
        }
    }

    fn assessed(
        status: TargetQualityStatus,
        method: TargetQaMethod,
        keystone_pct: TargetQaMeasurement,
        tilt_axis: TiltAxis,
    ) -> Self {
        Self {
            method: Some(method),
            status,
            keystone_pct: Some(keystone_pct),
            tilt_axis: Some(tilt_axis),
            gate_threshold_pct: TargetQuality::GATE_THRESHOLD_PCT,
            blockers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct TargetQaMeasurement {
    pub value: f32,
    pub unit: NumericUnit,
    pub method: TargetQaMethod,
    pub confidence: f32,
}

impl TargetQaMeasurement {
    #[must_use]
    pub fn measured_percent(value: f32, method: TargetQaMethod, confidence: f32) -> Option<Self> {
        (value.is_finite() && confidence.is_finite()).then_some(Self {
            value,
            unit: NumericUnit::Percent,
            method,
            confidence,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Measurements {
    pub sharpness: SharpnessMeasurements,
    pub vignetting: VignettingMeasurements,
    pub ca_lateral: CaLateralMeasurements,
    pub distortion: DistortionMeasurements,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DistortionMeasurements {
    pub candidate: Option<DistortionCandidate>,
    pub blockers: Vec<DistortionBlocker>,
}

impl DistortionMeasurements {
    #[must_use]
    pub fn blocked(blocker: DistortionBlocker) -> Self {
        Self {
            candidate: None,
            blockers: vec![blocker],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DistortionCandidate {
    pub orientation: DistortionOrientation,
    pub reference_side: Option<DistortionReferenceSide>,
    pub bow: DistortionMeasurement,
    pub sagitta_px: f32,
    pub span_coverage: f32,
    pub fit_residual_px: f32,
}

impl DistortionCandidate {
    #[must_use]
    pub fn new(
        orientation: DistortionOrientation,
        reference_side: Option<DistortionReferenceSide>,
        bow: DistortionMeasurement,
        sagitta_px: f32,
        span_coverage: f32,
        fit_residual_px: f32,
    ) -> Option<Self> {
        (sagitta_px.is_finite() && span_coverage.is_finite() && fit_residual_px.is_finite())
            .then_some(Self {
                orientation,
                reference_side,
                bow,
                sagitta_px,
                span_coverage,
                fit_residual_px,
            })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DistortionEvidence {
    pub included_samples: usize,
    pub excluded_samples: usize,
    pub mean_bow: Option<DistortionMeasurement>,
    pub scatter: Option<DistortionMeasurement>,
    pub blockers: Vec<DistortionBlocker>,
    pub excluded: Vec<ExclusionCount>,
}

impl DistortionEvidence {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            included_samples: 0,
            excluded_samples: 0,
            mean_bow: None,
            scatter: None,
            blockers: vec![DistortionBlocker::InsufficientSamples],
            excluded: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct DistortionMeasurement {
    pub value: f32,
    pub unit: NumericUnit,
    pub method: DistortionMethod,
    pub confidence: f32,
}

impl DistortionMeasurement {
    #[must_use]
    pub fn measured_percent_frame(value: f32) -> Option<Self> {
        Self::percent_frame(value, DistortionMethod::MeasuredStraightLineBow, 0.9)
    }

    #[must_use]
    pub fn inferred_percent_frame(value: f32) -> Option<Self> {
        Self::percent_frame(value, DistortionMethod::InferredWeakReferenceBow, 0.4)
    }

    #[must_use]
    pub fn percent_frame(value: f32, method: DistortionMethod, confidence: f32) -> Option<Self> {
        (value.is_finite() && confidence.is_finite()).then_some(Self {
            value,
            unit: NumericUnit::PercentFrame,
            method,
            confidence,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaLateralMeasurements {
    pub zones: CaZoneMeasurements,
}

impl CaLateralMeasurements {
    #[must_use]
    pub fn blocked_all(blocker: CaBlocker) -> Self {
        Self {
            zones: CaZoneMeasurements {
                top_left: CaZoneEvidence::blocked(blocker),
                top_right: CaZoneEvidence::blocked(blocker),
                bottom_left: CaZoneEvidence::blocked(blocker),
                bottom_right: CaZoneEvidence::blocked(blocker),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaLateralEvidence {
    pub top_left: CaCornerSummary,
    pub top_right: CaCornerSummary,
    pub bottom_left: CaCornerSummary,
    pub bottom_right: CaCornerSummary,
}

impl CaLateralEvidence {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            top_left: CaCornerSummary::empty(),
            top_right: CaCornerSummary::empty(),
            bottom_left: CaCornerSummary::empty(),
            bottom_right: CaCornerSummary::empty(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaCornerSummary {
    pub included_samples: usize,
    pub excluded_samples: usize,
    pub mean_shift: Option<CaShift>,
    pub scatter: Option<CaShift>,
    pub blockers: Vec<CaBlocker>,
    pub excluded: Vec<ExclusionCount>,
}

impl CaCornerSummary {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            included_samples: 0,
            excluded_samples: 0,
            mean_shift: None,
            scatter: None,
            blockers: vec![CaBlocker::InsufficientSamples],
            excluded: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaZoneMeasurements {
    pub top_left: CaZoneEvidence,
    pub top_right: CaZoneEvidence,
    pub bottom_left: CaZoneEvidence,
    pub bottom_right: CaZoneEvidence,
}

impl CaZoneMeasurements {
    #[must_use]
    pub const fn values(&self) -> CaCornerValues<'_> {
        CaCornerValues {
            top_left: &self.top_left,
            top_right: &self.top_right,
            bottom_left: &self.bottom_left,
            bottom_right: &self.bottom_right,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CaCornerValues<'a> {
    pub top_left: &'a CaZoneEvidence,
    pub top_right: &'a CaZoneEvidence,
    pub bottom_left: &'a CaZoneEvidence,
    pub bottom_right: &'a CaZoneEvidence,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaZoneEvidence {
    pub shift: Option<CaShift>,
    pub blockers: Vec<CaBlocker>,
}

impl CaZoneEvidence {
    #[must_use]
    pub fn measured(shift: CaShift) -> Self {
        Self {
            shift: Some(shift),
            blockers: Vec::new(),
        }
    }

    #[must_use]
    pub fn blocked(blocker: CaBlocker) -> Self {
        Self {
            shift: None,
            blockers: vec![blocker],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CaShift {
    pub x: CaMeasurement,
    pub y: CaMeasurement,
    pub magnitude: CaMeasurement,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CaMeasurement {
    pub value: f32,
    pub unit: NumericUnit,
    pub method: CaMethod,
    pub confidence: f32,
}

impl CaMeasurement {
    #[must_use]
    pub fn measured_channel_correlation(value: f32) -> Option<Self> {
        value.is_finite().then_some(Self {
            value,
            unit: NumericUnit::PxFullres,
            method: CaMethod::MeasuredChannelCorrelation,
            confidence: 1.0,
        })
    }
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
    pub luminance: NumericMeasurement,
    pub texture_usable: TextureUsable,
}

impl ZoneMeasurement {
    #[must_use]
    pub fn measured(
        acutance: f32,
        contrast: f32,
        luminance: f32,
        aggregation_eligible: bool,
    ) -> Option<Self> {
        if !acutance.is_finite() || !contrast.is_finite() || !luminance.is_finite() {
            return None;
        }
        let texture_usable = contrast >= TEXTURE_USABLE_THRESHOLD;
        let sharpness_confidence = if aggregation_eligible && texture_usable {
            1.0
        } else {
            0.0
        };
        let luminance_confidence = if aggregation_eligible { 1.0 } else { 0.0 };
        Some(Self {
            acutance: NumericMeasurement {
                value: acutance,
                unit: NumericUnit::Acutance,
                method: MeasurementMethod::Measured,
                confidence: sharpness_confidence,
            },
            contrast: NumericMeasurement {
                value: contrast,
                unit: NumericUnit::Ratio,
                method: MeasurementMethod::Measured,
                confidence: sharpness_confidence,
            },
            luminance: NumericMeasurement {
                value: luminance,
                unit: NumericUnit::LinearLuminance,
                method: MeasurementMethod::Measured,
                confidence: luminance_confidence,
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
pub struct VignettingMeasurements {
    pub zones: VignettingZoneMeasurements,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VignettingZoneMeasurements {
    pub top_left: CornerFalloff,
    pub top_right: CornerFalloff,
    pub bottom_left: CornerFalloff,
    pub bottom_right: CornerFalloff,
}

impl VignettingZoneMeasurements {
    #[must_use]
    pub const fn values(&self) -> VignettingCornerValues {
        VignettingCornerValues {
            top_left: self.top_left.falloff,
            top_right: self.top_right.falloff,
            bottom_left: self.bottom_left.falloff,
            bottom_right: self.bottom_right.falloff,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CornerFalloff {
    pub falloff: VignettingNumericMeasurement,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct VignettingCornerValues {
    pub top_left: VignettingNumericMeasurement,
    pub top_right: VignettingNumericMeasurement,
    pub bottom_left: VignettingNumericMeasurement,
    pub bottom_right: VignettingNumericMeasurement,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VignettingEvidence {
    pub method: VignettingMethod,
    pub included_samples: usize,
    pub excluded_samples: usize,
    pub reference_f_number: Option<f32>,
    pub raw_corner_mean_stops: Option<VignettingCornerValues>,
    pub optical_delta_from_reference_stops: Option<VignettingCornerValues>,
    pub blockers: Vec<VignettingBlocker>,
    pub warnings: Vec<VignettingWarning>,
    pub excluded: Vec<ExclusionCount>,
    pub symmetry: VignettingSymmetry,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VignettingSymmetry {
    pub status: VignettingSymmetryStatus,
    pub mean_optical_delta_stops: Option<VignettingNumericMeasurement>,
    pub max_corner_deviation_stops: Option<VignettingNumericMeasurement>,
    pub left_right_residual_stops: Option<VignettingNumericMeasurement>,
    pub top_bottom_residual_stops: Option<VignettingNumericMeasurement>,
    pub persistent_raw_bias_stops: Option<VignettingNumericMeasurement>,
    pub repeat_scatter_stops: Option<VignettingNumericMeasurement>,
    pub blockers: Vec<VignettingBlocker>,
}

impl VignettingSymmetry {
    #[must_use]
    pub fn not_assessed() -> Self {
        Self {
            status: VignettingSymmetryStatus::NotAssessed,
            mean_optical_delta_stops: None,
            max_corner_deviation_stops: None,
            left_right_residual_stops: None,
            top_bottom_residual_stops: None,
            persistent_raw_bias_stops: None,
            repeat_scatter_stops: None,
            blockers: vec![VignettingBlocker::SymmetryNotAssessed],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct VignettingNumericMeasurement {
    pub value: f32,
    pub unit: NumericUnit,
    pub method: VignettingMethod,
}

impl VignettingNumericMeasurement {
    #[must_use]
    pub fn measured_stops(value: f32) -> Option<Self> {
        Self::stops(value, VignettingMethod::MeasuredLuminanceRatio)
    }

    #[must_use]
    pub fn stops(value: f32, method: VignettingMethod) -> Option<Self> {
        value.is_finite().then_some(Self {
            value,
            unit: NumericUnit::Stops,
            method,
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
    Percent,
    Ratio,
    LinearLuminance,
    PxFullres,
    Stops,
    PercentFrame,
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
pub enum VignettingMethod {
    MeasuredLuminanceRatio,
    ReferenceRelativeApertureDifference,
    DerivedResidual,
    DerivedRepeatScatter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaMethod {
    MeasuredChannelCorrelation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DistortionMethod {
    MeasuredStraightLineBow,
    InferredWeakReferenceBow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldCurvatureMethod {
    InferredApertureLagFromMeasuredAcutance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetQaMethod {
    MeasuredPeriodicReferenceScale,
    MeasuredOrthogonalReferenceScale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldCurvatureStatus {
    Supported,
    NotSupported,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldCurvatureBlocker {
    InsufficientApertureSeries,
    MissingLensFocalIdentity,
    MissingAperture,
    AmbiguousPeak,
    LowTexture,
    UnknownCorrections,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DistortionOrientation {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DistortionReferenceSide {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DistortionBlocker {
    InsufficientSamples,
    NoStraightReference,
    WeakReferenceGeometry,
    LowContrast,
    LineDiscontinuous,
    FitResidualTooHigh,
    ProfileTooShort,
    UnknownCorrections,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaBlocker {
    InsufficientSamples,
    LowTexture,
    FlatProfile,
    CorrelationPeakNotFound,
    ProfileTooShort,
    UnsupportedColourChannels,
    UnknownCorrections,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VignettingBlocker {
    InsufficientApertureSeries,
    MissingLensFocalIdentity,
    MissingAperture,
    MixedLensFocalIdentity,
    ControlledApertureSeriesNotAssessed,
    UnknownCorrections,
    UnstableCentreLuminance,
    UnstableRepeatScatter,
    ContradictoryApertureTrend,
    SymmetryNotAssessed,
    ReferenceAperture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VignettingWarning {
    MissingLensFocalIdentity,
    UnstableCentreLuminance,
    UnstableRepeatOutlierExcluded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VignettingSymmetryStatus {
    NotAssessed,
    RadiallySymmetric,
    LightingBiased,
    MixedOrUnstable,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetQualityStatus {
    Passed,
    Gated,
    Blocked,
    NotAssessed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetQualityBlocker {
    KeystoneNotAssessed,
    KeystoneAboveThreshold,
    NoSuitableTargetReference,
    WeakTargetGeometry,
    AmbiguousTiltAxis,
    LowContrast,
    LineDiscontinuous,
    ProfileTooShort,
    UnknownCorrections,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TiltAxis {
    Vertical,
    Horizontal,
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
    FlatProfile,
    CorrelationPeakNotFound,
    ProfileTooShort,
    NoStraightReference,
    WeakReferenceGeometry,
    LowContrast,
    LineDiscontinuous,
    FitResidualTooHigh,
    UnsupportedColourChannels,
    UnstableRepeatOutlier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TextureMethod {
    DerivedThreshold,
}

#[cfg(test)]
mod tests {
    use super::{
        AnalyseGroup, AnalyseInput, AnalyseReport, CaBlocker, CaLateralEvidence,
        CaLateralMeasurements, CorrectionStatus, DecentringEvidence, DerivedNumericMeasurement,
        DistortionBlocker, DistortionCandidate, DistortionEvidence, DistortionMeasurement,
        DistortionMeasurements, DistortionOrientation, DistortionReferenceSide, ExclusionCount,
        ExclusionReason, FieldCurvatureEvidence, FieldCurvatureMethod, FieldCurvatureStatus,
        FieldCurvatureSummary, FrameMeasurement, FrameQuality, LeftRightDecentring, Measurements,
        PairId, PairSummary, ReliabilityBlocker, SharpnessMeasurements, SourceKind,
        TargetQaEvidence, TargetQaMeasurement, TargetQaMethod, TargetQuality, TargetQualityBlocker,
        TargetQualityStatus, TiltAxis, VignettingBlocker, VignettingCornerValues,
        VignettingEvidence, VignettingMeasurements, VignettingMethod, VignettingNumericMeasurement,
        VignettingSymmetry, VignettingZoneMeasurements, ZoneMeasurement, ZoneMeasurements,
    };

    fn zone(acutance: f32, contrast: f32, eligible: bool) -> ZoneMeasurement {
        ZoneMeasurement::measured(acutance, contrast, 0.8, eligible).unwrap()
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
            qa: FrameQuality::target_blocked(TargetQualityBlocker::NoSuitableTargetReference),
            measurements: Measurements {
                sharpness: SharpnessMeasurements {
                    zones: zones(eligible),
                },
                vignetting: VignettingMeasurements {
                    zones: VignettingZoneMeasurements {
                        top_left: corner(-1.0),
                        top_right: corner(-0.8),
                        bottom_left: corner(-0.9),
                        bottom_right: corner(-0.7),
                    },
                },
                ca_lateral: CaLateralMeasurements::blocked_all(CaBlocker::FlatProfile),
                distortion: DistortionMeasurements::blocked(DistortionBlocker::NoStraightReference),
            },
        }
    }

    fn corner(value: f32) -> super::CornerFalloff {
        super::CornerFalloff {
            falloff: VignettingNumericMeasurement::measured_stops(value).expect("finite falloff"),
        }
    }

    fn corner_values(value: f32) -> VignettingCornerValues {
        VignettingCornerValues {
            top_left: VignettingNumericMeasurement::measured_stops(value).expect("finite falloff"),
            top_right: VignettingNumericMeasurement::measured_stops(value).expect("finite falloff"),
            bottom_left: VignettingNumericMeasurement::measured_stops(value)
                .expect("finite falloff"),
            bottom_right: VignettingNumericMeasurement::measured_stops(value)
                .expect("finite falloff"),
        }
    }

    fn vignetting() -> VignettingEvidence {
        VignettingEvidence {
            method: VignettingMethod::MeasuredLuminanceRatio,
            included_samples: 1,
            excluded_samples: 0,
            reference_f_number: None,
            raw_corner_mean_stops: Some(corner_values(-0.85)),
            optical_delta_from_reference_stops: None,
            blockers: vec![
                VignettingBlocker::InsufficientApertureSeries,
                VignettingBlocker::SymmetryNotAssessed,
            ],
            warnings: vec![],
            excluded: vec![],
            symmetry: VignettingSymmetry::not_assessed(),
        }
    }

    fn measured_distortion_candidate() -> DistortionCandidate {
        DistortionCandidate::new(
            DistortionOrientation::Horizontal,
            Some(DistortionReferenceSide::Top),
            DistortionMeasurement::measured_percent_frame(0.18).expect("finite bow"),
            1.8,
            0.82,
            0.12,
        )
        .expect("finite candidate")
    }

    fn inferred_distortion_candidate() -> DistortionCandidate {
        DistortionCandidate::new(
            DistortionOrientation::Vertical,
            None,
            DistortionMeasurement::inferred_percent_frame(-0.08).expect("finite bow"),
            -0.8,
            0.45,
            0.1,
        )
        .expect("finite candidate")
    }

    fn distortion() -> DistortionEvidence {
        DistortionEvidence {
            included_samples: 1,
            excluded_samples: 0,
            mean_bow: Some(
                DistortionMeasurement::measured_percent_frame(0.18).expect("finite mean bow"),
            ),
            scatter: None,
            blockers: vec![DistortionBlocker::InsufficientSamples],
            excluded: vec![],
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

    fn target_measurement(value: f32) -> TargetQaMeasurement {
        TargetQaMeasurement::measured_percent(
            value,
            TargetQaMethod::MeasuredPeriodicReferenceScale,
            0.8,
        )
        .expect("finite target QA measurement")
    }

    fn target_quality(status: TargetQualityStatus, value: f32) -> TargetQuality {
        TargetQuality::assessed(
            status,
            TargetQaMethod::MeasuredPeriodicReferenceScale,
            target_measurement(value),
            TiltAxis::Vertical,
            1,
            0,
            if status == TargetQualityStatus::Gated {
                vec![TargetQualityBlocker::KeystoneAboveThreshold]
            } else {
                vec![]
            },
        )
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

    fn field_curvature() -> FieldCurvatureEvidence {
        FieldCurvatureEvidence {
            method: FieldCurvatureMethod::InferredApertureLagFromMeasuredAcutance,
            summaries: vec![FieldCurvatureSummary {
                lens_model: Some("50mm".to_owned()),
                focal_length_mm: Some(50.0),
                status: FieldCurvatureStatus::Supported,
                eligible_aperture_groups: 3,
                excluded_aperture_groups: 1,
                included_f_numbers: vec![5.6, 8.0, 11.0],
                centre_peak_f_number: Some(5.6),
                corner_mean_peak_f_number: Some(11.0),
                lag_stops: Some(1.949_637),
                lag_threshold_stops: 1.75,
                blockers: vec![],
                excluded: vec![ExclusionCount {
                    reason: ExclusionReason::UnknownCorrections,
                    count: 1,
                }],
            }],
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
            field_curvature(),
            vec![AnalyseGroup {
                lens_model: Some("50mm".to_owned()),
                focal_length_mm: Some(50.0),
                f_number: Some(8.0),
                decentring: two_sample_decentring(),
                vignetting: vignetting(),
                ca_lateral: CaLateralEvidence::empty(),
                distortion: distortion(),
                frames: vec![frame(0, "a.dng", true)],
            }],
        );

        let json = serde_json::to_string_pretty(&report).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(json.starts_with("{\n  \"schema_version\": \"0.1-vignetting-control\","));
        assert_eq!(value["schema_version"], "0.1-vignetting-control");
        assert_eq!(value["inputs"][0]["source_kind"], "cfa");
        assert_eq!(value["inputs"][0]["corrections"], "confirmed_uncorrected");
        assert_report_field_order(&json);
        assert_group_field_order(&json);
        assert_field_curvature_json(&value);
        assert_decentring_json(&value);
        assert_frame_measurement_json(&value);
        assert_vignetting_json(&value);
        assert_ca_json(&value);
        assert_distortion_json(&value);
    }

    fn assert_report_field_order(json: &str) {
        assert!(
            json.find("\"inputs\"").expect("inputs are serialised")
                < json
                    .find("\"field_curvature\"")
                    .expect("field curvature is serialised")
        );
        assert!(
            json.find("\"field_curvature\"")
                .expect("field curvature is serialised")
                < json.find("\"groups\"").expect("groups are serialised")
        );
    }

    fn assert_field_curvature_json(value: &serde_json::Value) {
        let evidence = &value["field_curvature"];
        assert_eq!(
            evidence["method"],
            "inferred_aperture_lag_from_measured_acutance"
        );
        let summary = &evidence["summaries"][0];
        assert_eq!(summary["status"], "supported");
        assert_eq!(summary["lens_model"], "50mm");
        assert_eq!(summary["focal_length_mm"], 50.0);
        assert_eq!(summary["eligible_aperture_groups"], 3);
        assert_eq!(summary["excluded_aperture_groups"], 1);
        assert_eq!(summary["included_f_numbers"][0], 5.6);
        assert_eq!(summary["centre_peak_f_number"], 5.6);
        assert_eq!(summary["corner_mean_peak_f_number"], 11.0);
        assert_eq!(summary["lag_threshold_stops"], 1.75);
        assert_eq!(summary["excluded"][0]["reason"], "unknown_corrections");
        assert!(summary["blockers"].as_array().unwrap().is_empty());
    }

    fn assert_group_field_order(json: &str) {
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
                    .find("\"vignetting\"")
                    .expect("group vignetting is serialised")
        );
        assert!(
            json.find("\"vignetting\"")
                .expect("group vignetting is serialised")
                < json.find("\"ca_lateral\"").expect("group CA is serialised")
        );
        assert!(
            json.find("\"ca_lateral\"").expect("group CA is serialised")
                < json
                    .find("\"distortion\"")
                    .expect("group distortion is serialised")
        );
        assert!(
            json.find("\"distortion\"")
                .expect("group distortion is serialised")
                < json
                    .find("\"frames\"")
                    .expect("group frames are serialised")
        );
        assert!(
            json.find("\"qa\"").expect("frame QA is serialised")
                < json
                    .find("\"measurements\"")
                    .expect("frame measurements are serialised")
        );
    }

    fn assert_decentring_json(value: &serde_json::Value) {
        assert_eq!(
            value["groups"][0]["decentring"]["method"],
            "derived_from_measured_acutance"
        );
        assert_eq!(
            value["groups"][0]["decentring"]["target_quality"]["status"],
            "not_assessed"
        );
        assert_eq!(
            value["groups"][0]["decentring"]["target_quality"]["method"],
            serde_json::Value::Null
        );
        assert_eq!(
            value["groups"][0]["decentring"]["target_quality"]["keystone_pct"],
            serde_json::Value::Null
        );
        assert_eq!(
            value["groups"][0]["decentring"]["target_quality"]["tilt_axis"],
            serde_json::Value::Null
        );
        assert_eq!(
            value["groups"][0]["decentring"]["target_quality"]["gate_threshold_pct"],
            1.5
        );
        assert_eq!(
            value["groups"][0]["decentring"]["target_quality"]["assessed_frames"],
            0
        );
        assert_eq!(
            value["groups"][0]["decentring"]["target_quality"]["blocked_frames"],
            0
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
    }

    fn assert_frame_measurement_json(value: &serde_json::Value) {
        assert_eq!(value["groups"][0]["frames"][0]["input_index"], 0);
        assert_eq!(
            value["groups"][0]["frames"][0]["qa"]["target"]["status"],
            "blocked"
        );
        assert_eq!(
            value["groups"][0]["frames"][0]["qa"]["target"]["keystone_pct"],
            serde_json::Value::Null
        );
        assert_eq!(
            value["groups"][0]["frames"][0]["qa"]["target"]["tilt_axis"],
            serde_json::Value::Null
        );
        assert_eq!(
            value["groups"][0]["frames"][0]["qa"]["target"]["gate_threshold_pct"],
            1.5
        );
        assert_eq!(
            value["groups"][0]["frames"][0]["qa"]["target"]["blockers"][0],
            "no_suitable_target_reference"
        );
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
            value["groups"][0]["frames"][0]["measurements"]["sharpness"]["zones"]["centre"]["luminance"]
                ["unit"],
            "linear_luminance"
        );
        assert_eq!(
            value["groups"][0]["frames"][0]["measurements"]["vignetting"]["zones"]["top_left"]["falloff"]
                ["unit"],
            "stops"
        );
        assert_eq!(
            value["groups"][0]["frames"][0]["measurements"]["sharpness"]["zones"]["top_left"]["acutance"]
                ["confidence"],
            0.0
        );
        assert_eq!(
            value["groups"][0]["frames"][0]["measurements"]["sharpness"]["zones"]["top_left"]["luminance"]
                ["confidence"],
            1.0
        );
        assert_eq!(
            value["groups"][0]["frames"][0]["measurements"]["sharpness"]["zones"]["centre"]["texture_usable"]
                ["method"],
            "derived_threshold"
        );
    }

    fn assert_vignetting_json(value: &serde_json::Value) {
        assert_eq!(
            value["groups"][0]["vignetting"]["raw_corner_mean_stops"]["top_left"]["method"],
            "measured_luminance_ratio"
        );
        assert_eq!(
            value["groups"][0]["vignetting"]["symmetry"]["status"],
            "not_assessed"
        );
        assert_eq!(
            value["groups"][0]["vignetting"]["symmetry"]["mean_optical_delta_stops"],
            serde_json::Value::Null
        );
        assert_eq!(
            value["groups"][0]["vignetting"]["symmetry"]["max_corner_deviation_stops"],
            serde_json::Value::Null
        );
        assert_eq!(
            value["groups"][0]["vignetting"]["symmetry"]["left_right_residual_stops"],
            serde_json::Value::Null
        );
        assert_eq!(
            value["groups"][0]["vignetting"]["symmetry"]["top_bottom_residual_stops"],
            serde_json::Value::Null
        );
        assert_eq!(
            value["groups"][0]["vignetting"]["symmetry"]["persistent_raw_bias_stops"],
            serde_json::Value::Null
        );
        assert_eq!(
            value["groups"][0]["vignetting"]["symmetry"]["repeat_scatter_stops"],
            serde_json::Value::Null
        );
    }

    fn assert_ca_json(value: &serde_json::Value) {
        assert_eq!(
            value["groups"][0]["frames"][0]["measurements"]["ca_lateral"]["zones"]["top_left"]["blockers"]
                [0],
            "flat_profile"
        );
        assert_eq!(
            value["groups"][0]["ca_lateral"]["top_left"]["blockers"][0],
            "insufficient_samples"
        );
        assert_eq!(
            value["groups"][0]["ca_lateral"]["top_left"]["mean_shift"],
            serde_json::Value::Null
        );
    }

    fn assert_distortion_json(value: &serde_json::Value) {
        assert_eq!(
            value["groups"][0]["distortion"]["mean_bow"]["unit"],
            "percent_frame"
        );
        assert_eq!(
            value["groups"][0]["distortion"]["mean_bow"]["method"],
            "measured_straight_line_bow"
        );
        assert_eq!(
            value["groups"][0]["distortion"]["blockers"][0],
            "insufficient_samples"
        );

        let measurements = &value["groups"][0]["frames"][0]["measurements"];
        assert_eq!(
            measurements["distortion"]["candidate"],
            serde_json::Value::Null
        );
        assert_eq!(
            measurements["distortion"]["blockers"][0],
            "no_straight_reference"
        );
    }

    #[test]
    fn serialises_distortion_frame_candidate_and_inferred_method_codes() {
        let mut measured_frame = frame(0, "measured.tif", true);
        measured_frame.measurements.distortion = DistortionMeasurements {
            candidate: Some(measured_distortion_candidate()),
            blockers: vec![],
        };
        let mut inferred_frame = frame(1, "inferred.tif", true);
        inferred_frame.measurements.distortion = DistortionMeasurements {
            candidate: Some(inferred_distortion_candidate()),
            blockers: vec![DistortionBlocker::WeakReferenceGeometry],
        };
        let report = AnalyseReport::new(
            "0.1.0",
            vec![],
            FieldCurvatureEvidence::not_assessed(),
            vec![AnalyseGroup {
                lens_model: None,
                focal_length_mm: None,
                f_number: None,
                decentring: two_sample_decentring(),
                vignetting: vignetting(),
                ca_lateral: CaLateralEvidence::empty(),
                distortion: distortion(),
                frames: vec![measured_frame, inferred_frame],
            }],
        );

        let value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string_pretty(&report).unwrap()).unwrap();
        let measured = &value["groups"][0]["frames"][0]["measurements"]["distortion"]["candidate"];
        assert_eq!(measured["orientation"], "horizontal");
        assert_eq!(measured["reference_side"], "top");
        assert_eq!(measured["bow"]["unit"], "percent_frame");
        assert_eq!(measured["bow"]["method"], "measured_straight_line_bow");
        assert_eq!(measured["span_coverage"], 0.82);
        assert_eq!(measured["fit_residual_px"], 0.12);

        let inferred = &value["groups"][0]["frames"][1]["measurements"]["distortion"];
        assert_eq!(inferred["candidate"]["orientation"], "vertical");
        assert_eq!(
            inferred["candidate"]["reference_side"],
            serde_json::Value::Null
        );
        assert_eq!(
            inferred["candidate"]["bow"]["method"],
            "inferred_weak_reference_bow"
        );
        assert_eq!(inferred["blockers"][0], "weak_reference_geometry");
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn serialises_target_qa_statuses_measurement_and_blockers() {
        let mut gated_frame = frame(0, "gated.tif", true);
        gated_frame.qa.target = TargetQaEvidence::gated(
            TargetQaMethod::MeasuredPeriodicReferenceScale,
            target_measurement(3.0),
            TiltAxis::Vertical,
        );
        let mut passed_frame = frame(1, "passed.tif", true);
        passed_frame.qa.target = TargetQaEvidence::passed(
            TargetQaMethod::MeasuredPeriodicReferenceScale,
            target_measurement(0.4),
            TiltAxis::Horizontal,
        );
        let blocked_frame = frame(2, "blocked.tif", true);
        let report = AnalyseReport::new(
            "0.1.0",
            vec![],
            FieldCurvatureEvidence::not_assessed(),
            vec![
                AnalyseGroup {
                    lens_model: None,
                    focal_length_mm: None,
                    f_number: None,
                    decentring: DecentringEvidence {
                        method: super::DecentringMethod::DerivedFromMeasuredAcutance,
                        target_quality: target_quality(TargetQualityStatus::Gated, 3.0),
                        left_right: LeftRightDecentring {
                            top_pair: pair(PairId::TopLeftMinusTopRight, 0, vec![], None, None),
                            bottom_pair: pair(
                                PairId::BottomLeftMinusBottomRight,
                                0,
                                vec![],
                                None,
                                None,
                            ),
                        },
                    },
                    vignetting: vignetting(),
                    ca_lateral: CaLateralEvidence::empty(),
                    distortion: DistortionEvidence::empty(),
                    frames: vec![gated_frame, passed_frame, blocked_frame],
                },
                AnalyseGroup {
                    lens_model: None,
                    focal_length_mm: None,
                    f_number: Some(8.0),
                    decentring: DecentringEvidence {
                        method: super::DecentringMethod::DerivedFromMeasuredAcutance,
                        target_quality: target_quality(TargetQualityStatus::Passed, 0.4),
                        left_right: LeftRightDecentring {
                            top_pair: pair(PairId::TopLeftMinusTopRight, 0, vec![], None, None),
                            bottom_pair: pair(
                                PairId::BottomLeftMinusBottomRight,
                                0,
                                vec![],
                                None,
                                None,
                            ),
                        },
                    },
                    vignetting: vignetting(),
                    ca_lateral: CaLateralEvidence::empty(),
                    distortion: DistortionEvidence::empty(),
                    frames: vec![],
                },
                AnalyseGroup {
                    lens_model: None,
                    focal_length_mm: None,
                    f_number: Some(11.0),
                    decentring: DecentringEvidence {
                        method: super::DecentringMethod::DerivedFromMeasuredAcutance,
                        target_quality: TargetQuality::blocked(
                            0,
                            1,
                            vec![TargetQualityBlocker::NoSuitableTargetReference],
                        ),
                        left_right: LeftRightDecentring {
                            top_pair: pair(PairId::TopLeftMinusTopRight, 0, vec![], None, None),
                            bottom_pair: pair(
                                PairId::BottomLeftMinusBottomRight,
                                0,
                                vec![],
                                None,
                                None,
                            ),
                        },
                    },
                    vignetting: vignetting(),
                    ca_lateral: CaLateralEvidence::empty(),
                    distortion: DistortionEvidence::empty(),
                    frames: vec![],
                },
            ],
        );

        let value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string_pretty(&report).unwrap()).unwrap();
        let frame_target = &value["groups"][0]["frames"][0]["qa"]["target"];
        assert_eq!(frame_target["method"], "measured_periodic_reference_scale");
        assert_eq!(frame_target["status"], "gated");
        assert_eq!(frame_target["keystone_pct"]["value"], 3.0);
        assert_eq!(frame_target["keystone_pct"]["unit"], "percent");
        assert_eq!(
            frame_target["keystone_pct"]["method"],
            "measured_periodic_reference_scale"
        );
        assert_eq!(frame_target["keystone_pct"]["confidence"], 0.8);
        assert_eq!(frame_target["tilt_axis"], "vertical");
        assert!(frame_target["blockers"].as_array().unwrap().is_empty());
        assert_eq!(
            value["groups"][0]["frames"][1]["qa"]["target"]["status"],
            "passed"
        );
        assert_eq!(
            value["groups"][0]["frames"][1]["qa"]["target"]["tilt_axis"],
            "horizontal"
        );
        assert_eq!(
            value["groups"][2]["decentring"]["target_quality"]["status"],
            "blocked"
        );
        assert_eq!(
            value["groups"][2]["decentring"]["target_quality"]["blockers"][0],
            "no_suitable_target_reference"
        );
    }

    #[test]
    fn serialises_no_future_verdict_or_artifact_fields() {
        let report = AnalyseReport::new(
            "0.1.0",
            vec![],
            FieldCurvatureEvidence::not_assessed(),
            vec![],
        );
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
            "mtf50",
            "target_role",
            "checkerboard_calibration",
            "edge_distortion",
        ] {
            assert!(value.get(key).is_none(), "{key}");
        }
    }

    #[test]
    fn serialises_one_sample_decentring_without_scatter() {
        let report = AnalyseReport::new(
            "0.1.0",
            vec![],
            FieldCurvatureEvidence::not_assessed(),
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
                vignetting: vignetting(),
                ca_lateral: CaLateralEvidence::empty(),
                distortion: DistortionEvidence::empty(),
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
    #[allow(clippy::too_many_lines)]
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
            FieldCurvatureEvidence::not_assessed(),
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
                vignetting: VignettingEvidence {
                    included_samples: 0,
                    excluded_samples: 1,
                    raw_corner_mean_stops: None,
                    blockers: vec![
                        VignettingBlocker::UnknownCorrections,
                        VignettingBlocker::SymmetryNotAssessed,
                    ],
                    excluded: vec![ExclusionCount {
                        reason: ExclusionReason::UnknownCorrections,
                        count: 1,
                    }],
                    ..vignetting()
                },
                ca_lateral: CaLateralEvidence::empty(),
                distortion: DistortionEvidence {
                    included_samples: 0,
                    excluded_samples: 1,
                    mean_bow: None,
                    scatter: None,
                    blockers: vec![DistortionBlocker::UnknownCorrections],
                    excluded: vec![ExclusionCount {
                        reason: ExclusionReason::UnknownCorrections,
                        count: 1,
                    }],
                },
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
        assert!(ZoneMeasurement::measured(f32::NAN, 0.1, 1.0, true).is_none());
        assert!(ZoneMeasurement::measured(0.1, f32::INFINITY, 1.0, true).is_none());
        assert!(ZoneMeasurement::measured(0.1, 0.1, f32::NAN, true).is_none());
        assert!(DerivedNumericMeasurement::acutance_delta(f32::NAN).is_none());
        assert!(DerivedNumericMeasurement::acutance_delta(f32::NEG_INFINITY).is_none());
        assert!(VignettingNumericMeasurement::measured_stops(f32::NAN).is_none());
        assert!(DistortionMeasurement::measured_percent_frame(f32::NAN).is_none());
        assert!(
            DistortionCandidate::new(
                DistortionOrientation::Horizontal,
                Some(DistortionReferenceSide::Bottom),
                DistortionMeasurement::measured_percent_frame(0.1).unwrap(),
                f32::INFINITY,
                0.8,
                0.1,
            )
            .is_none()
        );
    }

    #[test]
    fn grouping_shape_preserves_first_seen_order_and_null_equality() {
        let report = AnalyseReport::new(
            "0.1.0",
            vec![],
            FieldCurvatureEvidence::not_assessed(),
            vec![
                AnalyseGroup {
                    lens_model: None,
                    focal_length_mm: None,
                    f_number: None,
                    decentring: two_sample_decentring(),
                    vignetting: vignetting(),
                    ca_lateral: CaLateralEvidence::empty(),
                    distortion: DistortionEvidence::empty(),
                    frames: vec![frame(0, "first.tif", false), frame(2, "third.tif", false)],
                },
                AnalyseGroup {
                    lens_model: Some("50mm".to_owned()),
                    focal_length_mm: None,
                    f_number: None,
                    decentring: two_sample_decentring(),
                    vignetting: vignetting(),
                    ca_lateral: CaLateralEvidence::empty(),
                    distortion: DistortionEvidence::empty(),
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
