use crate::image::{
    BayerPattern, CfaImage, CfaPattern, CfaSamples, ImageError, LinearImage, RgbImage,
};
use crate::zones::MeasurementGrid;

#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedPlane {
    pub image: LinearImage,
    pub grid: MeasurementGrid,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedCaPlanes {
    pub red: LinearImage,
    pub blue: LinearImage,
    pub grid: MeasurementGrid,
    pub full_resolution_scale: f32,
}

pub trait CaPlaneSource {
    fn extract_ca_planes(&self) -> Result<ExtractedCaPlanes, ImageError>;
}

pub fn extract_ca_planes(image: &impl CaPlaneSource) -> Result<ExtractedCaPlanes, ImageError> {
    image.extract_ca_planes()
}

pub fn extract_green(image: &CfaImage) -> Result<ExtractedPlane, ImageError> {
    let Some(pattern) = image.bayer_pattern() else {
        let pattern = match image.pattern() {
            CfaPattern::Bayer(_) => unreachable!("bayer pattern already matched"),
            CfaPattern::Unsupported(pattern) => pattern.clone(),
        };
        return Err(ImageError::UnsupportedCfaPattern { pattern });
    };

    let levels = image
        .bayer_levels()
        .ok_or_else(|| ImageError::UnsupportedCfaPattern {
            pattern: match image.pattern() {
                CfaPattern::Bayer(pattern) => format!("{pattern:?}"),
                CfaPattern::Unsupported(pattern) => pattern.clone(),
            },
        })?;
    let (x_phase, y_phase) = pattern.first_green_phase();
    let (black, white) = levels.at(x_phase, y_phase);
    if white <= black {
        return Err(ImageError::InvalidLevelRange { black, white });
    }

    let dimensions = image.dimensions();
    let plane_dimensions = crate::image::Dimensions::new(
        phase_axis_len("width", dimensions.width(), x_phase)?,
        phase_axis_len("height", dimensions.height(), y_phase)?,
    )?;
    let mut samples = Vec::with_capacity(plane_dimensions.sample_count());
    for y in (usize::from(y_phase)..dimensions.height()).step_by(2) {
        for x in (usize::from(x_phase)..dimensions.width()).step_by(2) {
            let index = y * dimensions.width() + x;
            let sample = match image.samples() {
                CfaSamples::U16(samples) => f32::from(samples[index]),
                CfaSamples::F32(samples) => samples[index],
            };
            samples.push((sample - black) / (white - black));
        }
    }

    Ok(ExtractedPlane {
        image: LinearImage::with_provenance(plane_dimensions, samples, image.provenance())?,
        grid: MeasurementGrid::BayerGreen { x_phase, y_phase },
    })
}

fn phase_axis_len(axis: &'static str, source_len: usize, phase: u8) -> Result<usize, ImageError> {
    let phase = usize::from(phase);
    if source_len <= phase {
        return Err(ImageError::ZeroDimension { axis });
    }
    Ok(((source_len - 1 - phase) / 2) + 1)
}

pub fn extract_luma(image: &RgbImage) -> Result<ExtractedPlane, ImageError> {
    let samples = match image.components() {
        1 => image.samples().to_vec(),
        3 => image
            .samples()
            .chunks_exact(3)
            .map(|rgb| 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2])
            .collect(),
        components => return Err(ImageError::UnsupportedComponentCount { components }),
    };

    Ok(ExtractedPlane {
        image: LinearImage::with_provenance(image.dimensions(), samples, image.provenance())?,
        grid: MeasurementGrid::FullResolution,
    })
}

impl CaPlaneSource for RgbImage {
    fn extract_ca_planes(&self) -> Result<ExtractedCaPlanes, ImageError> {
        if self.components() != 3 {
            return Err(ImageError::UnsupportedComponentCount {
                components: self.components(),
            });
        }

        let mut red = Vec::with_capacity(self.dimensions().sample_count());
        let mut blue = Vec::with_capacity(self.dimensions().sample_count());
        for rgb in self.samples().chunks_exact(3) {
            red.push(rgb[0]);
            blue.push(rgb[2]);
        }

        Ok(ExtractedCaPlanes {
            red: LinearImage::with_provenance(self.dimensions(), red, self.provenance())?,
            blue: LinearImage::with_provenance(self.dimensions(), blue, self.provenance())?,
            grid: MeasurementGrid::FullResolution,
            full_resolution_scale: 1.0,
        })
    }
}

impl CaPlaneSource for CfaImage {
    fn extract_ca_planes(&self) -> Result<ExtractedCaPlanes, ImageError> {
        let Some(pattern) = self.bayer_pattern() else {
            let pattern = match self.pattern() {
                CfaPattern::Bayer(_) => unreachable!("bayer pattern already matched"),
                CfaPattern::Unsupported(pattern) => pattern.clone(),
            };
            return Err(ImageError::UnsupportedCfaPattern { pattern });
        };
        let levels = self
            .bayer_levels()
            .ok_or_else(|| ImageError::UnsupportedCfaPattern {
                pattern: format!("{pattern:?}"),
            })?;
        validate_bayer_levels(levels)?;

        let (red_phase, blue_phase) = bayer_colour_phases(pattern);
        let red = interpolate_bayer_colour(self, red_phase)?;
        let blue = interpolate_bayer_colour(self, blue_phase)?;

        Ok(ExtractedCaPlanes {
            red,
            blue,
            grid: MeasurementGrid::FullResolution,
            full_resolution_scale: 1.0,
        })
    }
}

fn validate_bayer_levels(levels: &crate::image::BlackWhiteLevels) -> Result<(), ImageError> {
    for y_phase in 0..=1 {
        for x_phase in 0..=1 {
            let (black, white) = levels.at(x_phase, y_phase);
            if white <= black {
                return Err(ImageError::InvalidLevelRange { black, white });
            }
        }
    }
    Ok(())
}

fn bayer_colour_phases(pattern: BayerPattern) -> ((u8, u8), (u8, u8)) {
    match pattern {
        BayerPattern::Rggb => ((0, 0), (1, 1)),
        BayerPattern::Bggr => ((1, 1), (0, 0)),
        BayerPattern::Gbrg => ((0, 1), (1, 0)),
        BayerPattern::Grbg => ((1, 0), (0, 1)),
    }
}

fn interpolate_bayer_colour(
    image: &CfaImage,
    target_phase: (u8, u8),
) -> Result<LinearImage, ImageError> {
    let dimensions = image.dimensions();
    let mut samples = Vec::with_capacity(dimensions.sample_count());
    for y in 0..dimensions.height() {
        for x in 0..dimensions.width() {
            samples.push(interpolated_bayer_sample(image, x, y, target_phase)?);
        }
    }
    LinearImage::with_provenance(dimensions, samples, image.provenance())
}

fn interpolated_bayer_sample(
    image: &CfaImage,
    x: usize,
    y: usize,
    target_phase: (u8, u8),
) -> Result<f32, ImageError> {
    if phase_matches(x, y, target_phase) {
        return normalised_bayer_sample(image, x, y);
    }

    let x_matches = x % 2 == usize::from(target_phase.0);
    let y_matches = y % 2 == usize::from(target_phase.1);
    let offsets: &[(isize, isize)] = if y_matches {
        &[(-1, 0), (1, 0)]
    } else if x_matches {
        &[(0, -1), (0, 1)]
    } else {
        &[(-1, -1), (1, -1), (-1, 1), (1, 1)]
    };

    let mut sum = 0.0;
    let mut count = 0;
    for (dx, dy) in offsets {
        let Some(nx) = x.checked_add_signed(*dx) else {
            continue;
        };
        let Some(ny) = y.checked_add_signed(*dy) else {
            continue;
        };
        if nx >= image.dimensions().width() || ny >= image.dimensions().height() {
            continue;
        }
        if !phase_matches(nx, ny, target_phase) {
            continue;
        }
        sum += normalised_bayer_sample(image, nx, ny)?;
        count += 1;
    }

    if count == 0 {
        return Err(ImageError::ZeroDimension { axis: "colour" });
    }
    #[allow(clippy::cast_precision_loss)]
    Ok(sum / count as f32)
}

fn phase_matches(x: usize, y: usize, target_phase: (u8, u8)) -> bool {
    x % 2 == usize::from(target_phase.0) && y % 2 == usize::from(target_phase.1)
}

fn normalised_bayer_sample(image: &CfaImage, x: usize, y: usize) -> Result<f32, ImageError> {
    let x_phase = u8::try_from(x % 2).expect("phase fits in u8");
    let y_phase = u8::try_from(y % 2).expect("phase fits in u8");
    let levels = image
        .bayer_levels()
        .ok_or_else(|| ImageError::UnsupportedCfaPattern {
            pattern: match image.pattern() {
                CfaPattern::Bayer(pattern) => format!("{pattern:?}"),
                CfaPattern::Unsupported(pattern) => pattern.clone(),
            },
        })?;
    let (black, white) = levels.at(x_phase, y_phase);
    if white <= black {
        return Err(ImageError::InvalidLevelRange { black, white });
    }
    let index = y * image.dimensions().width() + x;
    let sample = match image.samples() {
        CfaSamples::U16(samples) => f32::from(samples[index]),
        CfaSamples::F32(samples) => samples[index],
    };
    Ok((sample - black) / (white - black))
}

#[cfg(test)]
mod tests {
    use super::{extract_ca_planes, extract_green, extract_luma};
    use crate::image::{
        BayerPattern, BlackWhiteLevels, CfaImage, CfaPattern, CfaSamples, Dimensions, ImageError,
        Provenance, Rect, RgbImage,
    };
    use crate::metrics::ca::measure_lateral_ca;
    use crate::zones::MeasurementGrid;

    fn cfa(pattern: BayerPattern, samples: Vec<u16>) -> CfaImage {
        CfaImage::new(
            Dimensions::new(4, 4).unwrap(),
            pattern,
            CfaSamples::U16(samples),
            BlackWhiteLevels::new(&[0.0], &[100.0]).unwrap(),
            Provenance::measurement_ready(),
        )
        .unwrap()
    }

    #[test]
    fn extracts_rggb_green_phase_without_averaging_green_sites() {
        let extracted = extract_green(&cfa(BayerPattern::Rggb, (0..16).collect())).unwrap();

        assert_eq!(extracted.image.dimensions(), Dimensions::new(2, 2).unwrap());
        assert_eq!(extracted.image.samples(), &[0.01, 0.03, 0.09, 0.11]);
        assert_eq!(
            extracted.grid,
            MeasurementGrid::BayerGreen {
                x_phase: 1,
                y_phase: 0,
            }
        );
    }

    #[test]
    fn extracts_all_bayer_green_phases() {
        let cases = [
            (
                BayerPattern::Bggr,
                MeasurementGrid::BayerGreen {
                    x_phase: 1,
                    y_phase: 0,
                },
                vec![0.01, 0.03, 0.09, 0.11],
            ),
            (
                BayerPattern::Gbrg,
                MeasurementGrid::BayerGreen {
                    x_phase: 0,
                    y_phase: 0,
                },
                vec![0.0, 0.02, 0.08, 0.10],
            ),
            (
                BayerPattern::Grbg,
                MeasurementGrid::BayerGreen {
                    x_phase: 0,
                    y_phase: 0,
                },
                vec![0.0, 0.02, 0.08, 0.10],
            ),
        ];

        for (pattern, grid, expected) in cases {
            let extracted = extract_green(&cfa(pattern, (0..16).collect())).unwrap();

            assert_eq!(extracted.grid, grid);
            assert_eq!(extracted.image.samples(), expected.as_slice());
        }
    }

    #[test]
    fn extracts_odd_dimension_bayer_green_phase_extents() {
        let image = CfaImage::new(
            Dimensions::new(3, 3).unwrap(),
            BayerPattern::Gbrg,
            CfaSamples::U16((0..9).collect()),
            BlackWhiteLevels::new(&[0.0], &[100.0]).unwrap(),
            Provenance::measurement_ready(),
        )
        .unwrap();

        let extracted = extract_green(&image).unwrap();

        assert_eq!(extracted.image.dimensions(), Dimensions::new(2, 2).unwrap());
        assert_eq!(extracted.image.samples(), &[0.0, 0.02, 0.06, 0.08]);
    }

    #[test]
    fn normalises_selected_green_phase_with_pattern_position_levels() {
        let image = CfaImage::new(
            Dimensions::new(4, 4).unwrap(),
            BayerPattern::Rggb,
            CfaSamples::U16((0..16).collect()),
            BlackWhiteLevels::new(&[0.0, 10.0, 20.0, 30.0], &[100.0, 110.0, 120.0, 130.0]).unwrap(),
            Provenance::measurement_ready(),
        )
        .unwrap();

        let extracted = extract_green(&image).unwrap();

        assert_eq!(extracted.image.samples(), &[-0.09, -0.07, -0.01, 0.01]);
    }

    #[test]
    fn rejects_non_bayer_cfa_patterns() {
        let image = CfaImage::from_pattern(
            Dimensions::new(2, 2).unwrap(),
            CfaPattern::Unsupported("x-trans".to_owned()),
            CfaSamples::U16(vec![0, 1, 2, 3]),
            BlackWhiteLevels::new(&[0.0], &[255.0]).unwrap(),
            Provenance::measurement_ready(),
        )
        .unwrap();

        assert!(matches!(
            extract_green(&image),
            Err(ImageError::UnsupportedCfaPattern { .. })
        ));
    }

    #[test]
    fn computes_rec709_luma_for_measurement_ready_rgb() {
        let image = RgbImage::new(
            Dimensions::new(3, 1).unwrap(),
            3,
            vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.5, 0.25],
            Provenance::measurement_ready(),
        )
        .unwrap();

        let extracted = extract_luma(&image).unwrap();

        assert!(extracted.image.samples()[0].abs() < 0.000_001);
        assert!((extracted.image.samples()[1] - 1.0).abs() < 0.000_001);
        assert!((extracted.image.samples()[2] - 0.58825).abs() < 0.000_001);
        assert_eq!(extracted.grid, MeasurementGrid::FullResolution);
    }

    #[test]
    fn unknown_rgb_provenance_stays_unknown_after_luma_extraction() {
        let image = RgbImage::new(
            Dimensions::new(1, 1).unwrap(),
            3,
            vec![1.0, 1.0, 1.0],
            Provenance::unknown(),
        )
        .unwrap();

        let extracted = extract_luma(&image).unwrap();

        assert_eq!(extracted.image.provenance(), Provenance::unknown());
    }

    #[test]
    fn rejects_unsupported_rgb_component_counts() {
        assert!(matches!(
            RgbImage::new(
                Dimensions::new(1, 1).unwrap(),
                2,
                vec![0.0, 0.0],
                Provenance::measurement_ready()
            ),
            Err(ImageError::UnsupportedComponentCount { components: 2 })
        ));
    }

    #[test]
    fn extracts_rgb_red_blue_ca_planes_at_full_resolution() {
        let image = RgbImage::new(
            Dimensions::new(2, 2).unwrap(),
            3,
            vec![1.0, 9.0, 5.0, 2.0, 9.0, 6.0, 3.0, 9.0, 7.0, 4.0, 9.0, 8.0],
            Provenance::measurement_ready(),
        )
        .unwrap();

        let extracted = extract_ca_planes(&image).unwrap();

        assert_eq!(extracted.red.samples(), &[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(extracted.blue.samples(), &[5.0, 6.0, 7.0, 8.0]);
        assert_eq!(extracted.grid, MeasurementGrid::FullResolution);
        assert!((extracted.full_resolution_scale - 1.0).abs() < 1.0e-6);
    }

    #[test]
    fn rejects_luma_rgb_as_ca_colour_evidence() {
        let image = RgbImage::new(
            Dimensions::new(1, 1).unwrap(),
            1,
            vec![1.0],
            Provenance::measurement_ready(),
        )
        .unwrap();

        assert!(matches!(
            extract_ca_planes(&image),
            Err(ImageError::UnsupportedComponentCount { components: 1 })
        ));
    }

    #[test]
    fn extracts_bayer_ca_planes_on_common_full_resolution_grid() {
        let image = CfaImage::new(
            Dimensions::new(4, 4).unwrap(),
            BayerPattern::Rggb,
            CfaSamples::U16(vec![
                10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160,
            ]),
            BlackWhiteLevels::new(&[0.0], &[100.0]).unwrap(),
            Provenance::measurement_ready(),
        )
        .unwrap();

        let extracted = extract_ca_planes(&image).unwrap();

        assert_eq!(extracted.grid, MeasurementGrid::FullResolution);
        assert!((extracted.full_resolution_scale - 1.0).abs() < 1.0e-6);
        assert_eq!(extracted.red.dimensions(), Dimensions::new(4, 4).unwrap());
        assert_eq!(extracted.blue.dimensions(), Dimensions::new(4, 4).unwrap());
        assert!((extracted.red.samples()[0] - 0.1).abs() < 1.0e-6);
        assert!((extracted.red.samples()[5] - 0.6).abs() < 1.0e-6);
        assert!((extracted.blue.samples()[5] - 0.6).abs() < 1.0e-6);
        assert!((extracted.blue.samples()[15] - 1.6).abs() < 1.0e-6);
    }

    #[test]
    fn bayer_ca_extraction_does_not_report_cfa_site_offset_as_shift() {
        for pattern in [
            BayerPattern::Rggb,
            BayerPattern::Bggr,
            BayerPattern::Gbrg,
            BayerPattern::Grbg,
        ] {
            let image = textured_bayer_image(pattern);
            let extracted = extract_ca_planes(&image).unwrap();
            let rect = Rect::new(4, 4, 24, 24).unwrap();

            let evidence = measure_lateral_ca(
                extracted.red.patch(rect).unwrap(),
                extracted.blue.patch(rect).unwrap(),
                extracted.full_resolution_scale,
            )
            .unwrap();

            let shift = evidence.shift.expect("measured CA shift");
            assert!(
                shift.x.value.abs() <= 0.05,
                "{pattern:?} x shift {}",
                shift.x.value
            );
            assert!(
                shift.y.value.abs() <= 0.05,
                "{pattern:?} y shift {}",
                shift.y.value
            );
        }
    }

    fn textured_bayer_image(pattern: BayerPattern) -> CfaImage {
        const X_TEXTURE: [u16; 32] = [
            0, 33, 87, 142, 191, 219, 217, 184, 132, 73, 24, 2, 15, 58, 116, 174, 212, 222, 201,
            154, 96, 42, 8, 4, 32, 83, 141, 190, 219, 217, 184, 132,
        ];
        const Y_TEXTURE: [u16; 32] = [
            0, 19, 55, 94, 128, 145, 139, 112, 73, 35, 8, 1, 17, 51, 91, 126, 144, 140, 114, 76,
            38, 10, 0, 15, 49, 88, 124, 144, 141, 116, 79, 41,
        ];
        let mut samples = Vec::with_capacity(32 * 32);
        for y_sample in Y_TEXTURE {
            for x_sample in X_TEXTURE {
                samples.push(500 + x_sample + y_sample);
            }
        }
        CfaImage::new(
            Dimensions::new(32, 32).unwrap(),
            pattern,
            CfaSamples::U16(samples),
            BlackWhiteLevels::new(&[0.0], &[1000.0]).unwrap(),
            Provenance::measurement_ready(),
        )
        .unwrap()
    }

    #[test]
    fn rejects_unsupported_bayer_ca_patterns() {
        let image = CfaImage::from_pattern(
            Dimensions::new(2, 2).unwrap(),
            CfaPattern::Unsupported("x-trans".to_owned()),
            CfaSamples::U16(vec![0, 1, 2, 3]),
            BlackWhiteLevels::new(&[0.0], &[255.0]).unwrap(),
            Provenance::measurement_ready(),
        )
        .unwrap();

        assert!(matches!(
            extract_ca_planes(&image),
            Err(ImageError::UnsupportedCfaPattern { .. })
        ));
    }
}
