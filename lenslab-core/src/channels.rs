use crate::image::{CfaImage, CfaPattern, CfaSamples, ImageError, LinearImage, RgbImage};
use crate::zones::MeasurementGrid;

#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedPlane {
    pub image: LinearImage,
    pub grid: MeasurementGrid,
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

#[cfg(test)]
mod tests {
    use super::{extract_green, extract_luma};
    use crate::image::{
        BayerPattern, BlackWhiteLevels, CfaImage, CfaPattern, CfaSamples, Dimensions, ImageError,
        Provenance, RgbImage,
    };
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
}
