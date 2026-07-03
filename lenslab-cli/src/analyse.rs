use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use lenslab_core::channels::{extract_green, extract_luma};
use lenslab_core::metrics::acutance::measure_acutance;
use lenslab_core::metrics::decentring::aggregate_left_right_decentring;
use lenslab_core::schema::{
    AnalyseGroup, AnalyseInput, AnalyseReport, CorrectionStatus, FrameMeasurement, Measurements,
    SharpnessMeasurements, SourceKind, ZoneMeasurement, ZoneMeasurements,
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
                    sharpness: SharpnessMeasurements { zones: zones.zones },
                },
            },
        });
    }

    let report = AnalyseReport::new(env!("CARGO_PKG_VERSION"), inputs, group_frames(frames)?);
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
    zones: ZoneMeasurements,
}

fn measure_frame_zones(
    frame: DecodedFrame,
    path: &Path,
    aggregation_eligible: bool,
) -> anyhow::Result<MeasuredZones> {
    let info = frame.info;
    let (source_dimensions, plane) = match frame.pixels {
        DecodedPixels::Cfa(image) => (image.dimensions(), extract_green(&image)?),
        DecodedPixels::Rgb(image) => (image.dimensions(), extract_luma(&image)?),
    };

    let zones = default_zones(source_dimensions)?;
    let mut measured = Vec::with_capacity(5);
    for zone in zones {
        let rect = project_zone(zone.rect(), plane.grid)?;
        let patch_view = plane.image.patch(rect)?;
        let measurement = measure_acutance(patch_view)?;
        let zone_measurement = ZoneMeasurement::measured(
            measurement.acutance,
            measurement.contrast,
            aggregation_eligible,
        )
        .with_context(|| format!("non-finite measurement in {}", path.display()))?;
        measured.push((zone_id_to_index(zone.id()), zone_measurement));
    }

    Ok(MeasuredZones {
        info,
        zones: ordered_zone_measurements(measured)?,
    })
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
        analyse_groups.push(AnalyseGroup {
            lens_model: group.key.lens_model,
            focal_length_mm: group.key.focal_length_mm,
            f_number: group.key.f_number,
            decentring,
            frames: group.frames,
        });
    }
    Ok(analyse_groups)
}

struct GroupedFrames {
    key: GroupKey,
    frames: Vec<FrameMeasurement>,
}

#[cfg(test)]
mod tests {
    use super::{AnalysedFrame, GroupKey, group_frames};
    use lenslab_core::schema::{
        FrameMeasurement, Measurements, SharpnessMeasurements, ZoneMeasurement, ZoneMeasurements,
    };

    fn frame(input_index: usize) -> FrameMeasurement {
        let zone = ZoneMeasurement::measured(1.0, 0.2, true).unwrap();
        FrameMeasurement {
            input_index,
            path: format!("frame-{input_index}.tif"),
            aggregation_eligible: true,
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
            },
        }
    }

    fn analysed_frame(input_index: usize, key: GroupKey) -> AnalysedFrame {
        AnalysedFrame {
            key,
            measurement: frame(input_index),
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
        assert_eq!(groups[1].lens_model.as_deref(), Some("35mm"));
        assert_eq!(groups[1].frames[0].input_index, 1);
    }
}
