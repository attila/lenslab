use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use lenslab_core::channels::{ExtractedCaPlanes, extract_ca_planes, extract_green, extract_luma};
use lenslab_core::metrics::acutance::measure_acutance;
use lenslab_core::metrics::ca::{aggregate_group_ca, measure_lateral_ca};
use lenslab_core::metrics::decentring::aggregate_left_right_decentring;
use lenslab_core::metrics::distortion::{aggregate_group_distortion, measure_distortion};
use lenslab_core::metrics::field_curvature::infer_field_curvature;
use lenslab_core::metrics::vignetting::{
    aggregate_group_vignetting, apply_reference_relative_vignetting, measured_falloff,
    median_luminance,
};
use lenslab_core::schema::{
    AnalyseGroup, AnalyseInput, AnalyseReport, CaBlocker, CaLateralMeasurements, CaZoneEvidence,
    CaZoneMeasurements, CorrectionStatus, DistortionMeasurements, FrameMeasurement, Measurements,
    SharpnessMeasurements, SourceKind, VignettingMeasurements, VignettingZoneMeasurements,
    ZoneMeasurement, ZoneMeasurements,
};
use lenslab_core::zones::{ZoneId, default_zones, project_zone};
use lenslab_decode::{DecodedFrame, DecodedPixels, FrameInfo};

pub fn write_analysis(paths: &[PathBuf]) -> anyhow::Result<()> {
    if paths.is_empty() {
        bail!("analyse requires at least one input path");
    }
    preflight_inputs(paths)?;

    let mut inputs = Vec::with_capacity(paths.len());
    let mut frames = Vec::with_capacity(paths.len());
    for (index, path) in paths.iter().enumerate() {
        let backend = lenslab_decode::decoder_for(path)?;
        let decoded_frame = backend
            .decode(path)
            .with_context(|| format!("failed to decode analysis input {}", path.display()))?;
        let correction = correction_status(&decoded_frame.info, path)?;
        let source_kind = source_kind(&decoded_frame.info);
        let aggregation_eligible = correction == CorrectionStatus::ConfirmedUncorrected;
        let zones = measure_frame_zones(decoded_frame, path, aggregation_eligible)
            .with_context(|| format!("failed to measure {}", path.display()))?;
        inputs.push(analyse_input(
            index,
            path,
            source_kind,
            correction,
            &zones.info,
        ));
        frames.push(AnalysedFrame {
            key: GroupKey::from_info(&zones.info),
            measurement: FrameMeasurement {
                input_index: index,
                path: path.display().to_string(),
                aggregation_eligible,
                measurements: Measurements {
                    sharpness: SharpnessMeasurements {
                        zones: zones.sharpness_zones,
                    },
                    vignetting: zones.vignetting,
                    ca_lateral: zones.ca_lateral,
                    distortion: zones.distortion,
                },
            },
        });
    }

    let analyse_groups = group_frames(frames)?;
    let field_curvature = infer_field_curvature(&analyse_groups)
        .context("failed to infer field-curvature evidence")?;
    let report = AnalyseReport::new(
        env!("CARGO_PKG_VERSION"),
        inputs,
        field_curvature,
        analyse_groups,
    );
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer_pretty(&mut stdout, &report)?;
    writeln!(stdout)?;
    Ok(())
}

fn preflight_inputs(paths: &[PathBuf]) -> anyhow::Result<()> {
    for path in paths {
        if !path.exists() {
            bail!("analysis input does not exist: {}", path.display());
        }
        if path.is_dir() {
            bail!(
                "directory inputs are not supported by analyse: {}",
                path.display()
            );
        }
        if !path.is_file() {
            bail!("analysis input is not a regular file: {}", path.display());
        }
    }
    Ok(())
}

fn correction_status(info: &FrameInfo, path: &Path) -> anyhow::Result<CorrectionStatus> {
    match info.corrections.present {
        Some(false) => Ok(CorrectionStatus::ConfirmedUncorrected),
        Some(true) => bail!(
            "corrected inputs are not supported by analyse: {}",
            path.display()
        ),
        None => Ok(CorrectionStatus::AcceptedUnknownCorrections),
    }
}

fn source_kind(info: &FrameInfo) -> SourceKind {
    match info.source_kind {
        lenslab_decode::SourceKind::Cfa => SourceKind::Cfa,
        lenslab_decode::SourceKind::Rgb => SourceKind::Rgb,
    }
}

fn analyse_input(
    index: usize,
    path: &Path,
    source_kind: SourceKind,
    corrections: CorrectionStatus,
    info: &FrameInfo,
) -> AnalyseInput {
    AnalyseInput {
        index,
        path: path.display().to_string(),
        source_kind,
        camera_make: info.camera_make.clone(),
        camera_model: info.camera_model.clone(),
        lens_model: info.lens_model.clone(),
        focal_length_mm: info.exposure.focal_length_mm,
        f_number: info.exposure.f_number,
        corrections,
        correction_provenance: (corrections == CorrectionStatus::AcceptedUnknownCorrections)
            .then(|| correction_provenance(info)),
    }
}

fn correction_provenance(info: &FrameInfo) -> String {
    if info.corrections.detail.is_empty() {
        "metadata has no reliable correction flag".to_owned()
    } else {
        info.corrections.detail.join("; ")
    }
}

struct MeasuredZones {
    info: FrameInfo,
    sharpness_zones: ZoneMeasurements,
    vignetting: VignettingMeasurements,
    ca_lateral: CaLateralMeasurements,
    distortion: DistortionMeasurements,
}

fn measure_frame_zones(
    frame: DecodedFrame,
    path: &Path,
    aggregation_eligible: bool,
) -> anyhow::Result<MeasuredZones> {
    let info = frame.info;
    let (source_dimensions, plane, ca_planes) = match frame.pixels {
        DecodedPixels::Cfa(image) => (
            image.dimensions(),
            extract_green(&image)?,
            Some(extract_ca_planes(&image)?),
        ),
        DecodedPixels::Rgb(image) => {
            let ca_planes = (image.components() == 3)
                .then(|| extract_ca_planes(&image))
                .transpose()?;
            (image.dimensions(), extract_luma(&image)?, ca_planes)
        }
    };

    let zones = default_zones(source_dimensions)?;
    let mut measured = Vec::with_capacity(5);
    let mut ca_measured = Vec::with_capacity(4);
    for zone in zones {
        let rect = project_zone(zone.rect(), plane.grid)?;
        let patch_view = plane.image.patch(rect)?;
        let measurement = measure_acutance(patch_view)?;
        let luminance = median_luminance(patch_view)?;
        let zone_measurement = ZoneMeasurement::measured(
            measurement.acutance,
            measurement.contrast,
            luminance,
            aggregation_eligible,
        )
        .with_context(|| format!("non-finite measurement in {}", path.display()))?;
        let zone_index = zone_id_to_index(zone.id());
        measured.push((zone_index, zone_measurement));
        if let (Some(ca_planes), true) = (&ca_planes, zone.id() != ZoneId::Centre) {
            ca_measured.push((
                zone_index,
                measure_ca_zone(ca_planes, zone.rect()).with_context(|| {
                    format!("failed to measure lateral CA in {}", path.display())
                })?,
            ));
        }
    }
    let sharpness_zones = ordered_zone_measurements(measured)?;
    let vignetting = VignettingMeasurements {
        zones: vignetting_zones(&sharpness_zones)?,
    };
    let ca_lateral = if ca_planes.is_some() {
        CaLateralMeasurements {
            zones: ordered_ca_zone_measurements(ca_measured)?,
        }
    } else {
        CaLateralMeasurements::blocked_all(CaBlocker::UnsupportedColourChannels)
    };
    let full_plane = plane.image.patch(lenslab_core::image::Rect::new(
        0,
        0,
        plane.image.dimensions().width(),
        plane.image.dimensions().height(),
    )?)?;
    let distortion = measure_distortion(full_plane)
        .with_context(|| format!("failed to measure distortion in {}", path.display()))?;

    Ok(MeasuredZones {
        info,
        sharpness_zones,
        vignetting,
        ca_lateral,
        distortion,
    })
}

fn measure_ca_zone(
    planes: &ExtractedCaPlanes,
    source_rect: lenslab_core::image::Rect,
) -> anyhow::Result<CaZoneEvidence> {
    let rect = project_zone(source_rect, planes.grid)?;
    Ok(measure_lateral_ca(
        planes.red.patch(rect)?,
        planes.blue.patch(rect)?,
        planes.full_resolution_scale,
    )?)
}

fn zone_id_to_index(id: ZoneId) -> usize {
    match id {
        ZoneId::Centre => 0,
        ZoneId::TopLeft => 1,
        ZoneId::TopRight => 2,
        ZoneId::BottomLeft => 3,
        ZoneId::BottomRight => 4,
    }
}

fn ordered_zone_measurements(
    mut measured: Vec<(usize, ZoneMeasurement)>,
) -> anyhow::Result<ZoneMeasurements> {
    measured.sort_by_key(|(index, _)| *index);
    let zones = measured
        .into_iter()
        .map(|(_, zone)| zone)
        .collect::<Vec<_>>();
    let zones: [ZoneMeasurement; 5] = zones
        .try_into()
        .map_err(|_| anyhow::anyhow!("expected five zone measurements"))?;
    Ok(ZoneMeasurements::from_ordered(zones))
}

fn ordered_ca_zone_measurements(
    mut measured: Vec<(usize, CaZoneEvidence)>,
) -> anyhow::Result<CaZoneMeasurements> {
    measured.sort_by_key(|(index, _)| *index);
    let zones = measured
        .into_iter()
        .map(|(_, zone)| zone)
        .collect::<Vec<_>>();
    let [top_left, top_right, bottom_left, bottom_right]: [CaZoneEvidence; 4] = zones
        .try_into()
        .map_err(|_| anyhow::anyhow!("expected four CA zone measurements"))?;
    Ok(CaZoneMeasurements {
        top_left,
        top_right,
        bottom_left,
        bottom_right,
    })
}

fn vignetting_zones(zones: &ZoneMeasurements) -> anyhow::Result<VignettingZoneMeasurements> {
    let centre_luminance = zones.centre.luminance.value;
    Ok(VignettingZoneMeasurements {
        top_left: measured_falloff(centre_luminance, zones.top_left.luminance.value)?,
        top_right: measured_falloff(centre_luminance, zones.top_right.luminance.value)?,
        bottom_left: measured_falloff(centre_luminance, zones.bottom_left.luminance.value)?,
        bottom_right: measured_falloff(centre_luminance, zones.bottom_right.luminance.value)?,
    })
}

#[derive(Debug, Clone, PartialEq)]
struct GroupKey {
    lens_model: Option<String>,
    focal_length_mm: Option<f32>,
    f_number: Option<f32>,
}

impl GroupKey {
    fn from_info(info: &FrameInfo) -> Self {
        Self {
            lens_model: info.lens_model.clone(),
            focal_length_mm: info.exposure.focal_length_mm,
            f_number: info.exposure.f_number,
        }
    }
}

struct AnalysedFrame {
    key: GroupKey,
    measurement: FrameMeasurement,
}

fn group_frames(frames: Vec<AnalysedFrame>) -> anyhow::Result<Vec<AnalyseGroup>> {
    let mut groups: Vec<GroupedFrames> = Vec::new();
    for frame in frames {
        if let Some(group) = groups.iter_mut().find(|group| {
            group.key.lens_model == frame.key.lens_model
                && group.key.focal_length_mm == frame.key.focal_length_mm
                && group.key.f_number == frame.key.f_number
        }) {
            group.frames.push(frame.measurement);
        } else {
            groups.push(GroupedFrames {
                key: frame.key,
                frames: vec![frame.measurement],
            });
        }
    }
    let mut analyse_groups = Vec::with_capacity(groups.len());
    for group in groups {
        let decentring = aggregate_left_right_decentring(&group.frames)
            .context("failed to aggregate decentring evidence")?;
        let vignetting = aggregate_group_vignetting(&group.frames)
            .context("failed to aggregate vignetting evidence")?;
        let ca_lateral =
            aggregate_group_ca(&group.frames).context("failed to aggregate lateral CA evidence")?;
        let distortion = aggregate_group_distortion(&group.frames)
            .context("failed to aggregate distortion evidence")?;
        analyse_groups.push(AnalyseGroup {
            lens_model: group.key.lens_model,
            focal_length_mm: group.key.focal_length_mm,
            f_number: group.key.f_number,
            decentring,
            vignetting,
            ca_lateral,
            distortion,
            frames: group.frames,
        });
    }
    apply_reference_relative_vignetting(&mut analyse_groups, false)
        .context("failed to aggregate reference-relative vignetting evidence")?;
    Ok(analyse_groups)
}

struct GroupedFrames {
    key: GroupKey,
    frames: Vec<FrameMeasurement>,
}

#[cfg(test)]
mod tests {
    use super::{AnalysedFrame, GroupKey, group_frames};
    use lenslab_core::metrics::field_curvature::infer_field_curvature;
    use lenslab_core::schema::{
        CaBlocker, CaLateralMeasurements, CornerFalloff, FieldCurvatureStatus, FrameMeasurement,
        Measurements, SharpnessMeasurements, VignettingMeasurements, VignettingNumericMeasurement,
        VignettingZoneMeasurements, ZoneMeasurement, ZoneMeasurements,
    };

    fn frame(input_index: usize) -> FrameMeasurement {
        frame_with_sharpness(input_index, 1.0, 1.0)
    }

    fn frame_with_sharpness(input_index: usize, centre: f32, corners: f32) -> FrameMeasurement {
        let centre_zone = ZoneMeasurement::measured(centre, 0.2, 1.0, true).unwrap();
        let corner_zone = ZoneMeasurement::measured(corners, 0.2, 1.0, true).unwrap();
        FrameMeasurement {
            input_index,
            path: format!("frame-{input_index}.tif"),
            aggregation_eligible: true,
            measurements: Measurements {
                sharpness: SharpnessMeasurements {
                    zones: ZoneMeasurements::from_ordered([
                        centre_zone,
                        corner_zone.clone(),
                        corner_zone.clone(),
                        corner_zone.clone(),
                        corner_zone,
                    ]),
                },
                vignetting: VignettingMeasurements {
                    zones: VignettingZoneMeasurements {
                        top_left: falloff(-1.0),
                        top_right: falloff(-1.0),
                        bottom_left: falloff(-1.0),
                        bottom_right: falloff(-1.0),
                    },
                },
                ca_lateral: CaLateralMeasurements::blocked_all(CaBlocker::FlatProfile),
                distortion: lenslab_core::schema::DistortionMeasurements::blocked(
                    lenslab_core::schema::DistortionBlocker::NoStraightReference,
                ),
            },
        }
    }

    fn falloff(value: f32) -> CornerFalloff {
        CornerFalloff {
            falloff: VignettingNumericMeasurement::measured_stops(value).unwrap(),
        }
    }

    fn analysed_frame(input_index: usize, key: GroupKey) -> AnalysedFrame {
        AnalysedFrame {
            key,
            measurement: frame(input_index),
        }
    }

    fn analysed_frame_with_sharpness(
        input_index: usize,
        key: GroupKey,
        centre: f32,
        corners: f32,
    ) -> AnalysedFrame {
        AnalysedFrame {
            key,
            measurement: frame_with_sharpness(input_index, centre, corners),
        }
    }

    #[test]
    fn grouping_preserves_first_seen_order_and_exact_non_null_equality() {
        let first_key = GroupKey {
            lens_model: Some("50mm".to_owned()),
            focal_length_mm: Some(50.0),
            f_number: Some(8.0),
        };
        let second_key = GroupKey {
            lens_model: Some("35mm".to_owned()),
            focal_length_mm: Some(35.0),
            f_number: Some(5.6),
        };

        let groups = group_frames(vec![
            analysed_frame(0, first_key.clone()),
            analysed_frame(1, second_key),
            analysed_frame(2, first_key),
        ])
        .unwrap();

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].lens_model.as_deref(), Some("50mm"));
        assert_eq!(groups[0].frames[0].input_index, 0);
        assert_eq!(groups[0].frames[1].input_index, 2);
        assert!(groups[0].vignetting.blockers.contains(
            &lenslab_core::schema::VignettingBlocker::ControlledApertureSeriesNotAssessed
        ));
        assert_eq!(groups[1].lens_model.as_deref(), Some("35mm"));
        assert_eq!(groups[1].frames[0].input_index, 1);
    }

    #[test]
    fn grouped_confirmed_uncorrected_aperture_lag_supports_field_curvature() {
        let lens = Some("50mm".to_owned());
        let groups = group_frames(vec![
            analysed_frame_with_sharpness(
                0,
                GroupKey {
                    lens_model: lens.clone(),
                    focal_length_mm: Some(50.0),
                    f_number: Some(5.6),
                },
                2.0,
                1.0,
            ),
            analysed_frame_with_sharpness(
                1,
                GroupKey {
                    lens_model: lens.clone(),
                    focal_length_mm: Some(50.0),
                    f_number: Some(8.0),
                },
                1.5,
                1.5,
            ),
            analysed_frame_with_sharpness(
                2,
                GroupKey {
                    lens_model: lens,
                    focal_length_mm: Some(50.0),
                    f_number: Some(11.0),
                },
                1.0,
                2.0,
            ),
        ])
        .unwrap();

        let evidence = infer_field_curvature(&groups).unwrap();

        assert_eq!(
            evidence.summaries[0].status,
            FieldCurvatureStatus::Supported
        );
        assert_eq!(evidence.summaries[0].centre_peak_f_number, Some(5.6));
        assert_eq!(evidence.summaries[0].corner_mean_peak_f_number, Some(11.0));
    }
}
