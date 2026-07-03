use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::image::LinearPatchView;

const ZERO_SIGNAL_EPSILON: f32 = 1.0e-6;
const MID_FREQ_EPSILON: f32 = 1.0e-6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AcutanceMeasurement {
    pub acutance: f32,
    pub contrast: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AcutanceError {
    EmptyPatch,
    NonFiniteSample { value: f32 },
}

impl Display for AcutanceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyPatch => write!(formatter, "patch is empty"),
            Self::NonFiniteSample { value } => write!(formatter, "non-finite sample {value}"),
        }
    }
}

impl Error for AcutanceError {}

pub fn measure_acutance(patch: LinearPatchView<'_>) -> Result<AcutanceMeasurement, AcutanceError> {
    let dimensions = patch.dimensions();
    let width = dimensions.width();
    let height = dimensions.height();
    let count = width.checked_mul(height).ok_or(AcutanceError::EmptyPatch)?;
    if count == 0 {
        return Err(AcutanceError::EmptyPatch);
    }

    let mut samples = Vec::with_capacity(count);
    for row in patch.rows() {
        for sample in row {
            if !sample.is_finite() {
                return Err(AcutanceError::NonFiniteSample { value: *sample });
            }
            samples.push(*sample);
        }
    }

    let sample_std = population_std(&samples);
    let blur_one = gaussian_blur(&samples, width, height, 1.0);
    let blur_two = gaussian_blur(&samples, width, height, 2.5);
    let high = samples
        .iter()
        .zip(&blur_one)
        .map(|(sample, blurred)| sample - blurred)
        .collect::<Vec<_>>();
    let mid = blur_one
        .into_iter()
        .zip(blur_two)
        .map(|(sharp, soft)| sharp - soft)
        .collect::<Vec<_>>();

    let mid_std = population_std(&mid);
    let acutance = if sample_std <= ZERO_SIGNAL_EPSILON || mid_std <= MID_FREQ_EPSILON {
        0.0
    } else {
        population_std(&high) / mid_std
    };
    let mean = mean(&samples);
    let contrast = if mean <= 0.0 { 0.0 } else { sample_std / mean };

    Ok(AcutanceMeasurement { acutance, contrast })
}

fn gaussian_blur(samples: &[f32], width: usize, height: usize, sigma: f32) -> Vec<f32> {
    let kernel = gaussian_kernel(sigma);
    let radius = kernel.len() / 2;
    let mut vertical = vec![0.0; samples.len()];
    for y in 0..height {
        for x in 0..width {
            let mut value = 0.0;
            for (offset, weight) in kernel.iter().enumerate() {
                if let Some(source_y) = shifted_index(y, offset, radius, height) {
                    value += samples[(source_y * width) + x] * weight;
                }
            }
            vertical[(y * width) + x] = value;
        }
    }

    let mut blurred = vec![0.0; samples.len()];
    for y in 0..height {
        for x in 0..width {
            let mut value = 0.0;
            for (offset, weight) in kernel.iter().enumerate() {
                if let Some(source_x) = shifted_index(x, offset, radius, width) {
                    value += vertical[(y * width) + source_x] * weight;
                }
            }
            blurred[(y * width) + x] = value;
        }
    }
    blurred
}

fn shifted_index(index: usize, offset: usize, radius: usize, limit: usize) -> Option<usize> {
    let shifted = index.checked_add(offset)?.checked_sub(radius)?;
    (shifted < limit).then_some(shifted)
}

fn gaussian_kernel(sigma: f32) -> Vec<f32> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let radius = (sigma * 3.0) as i32;
    let mut kernel = (-radius..=radius)
        .map(|x| {
            #[allow(clippy::cast_precision_loss)]
            let squared = (x * x) as f32;
            (-squared / (2.0 * sigma * sigma)).exp()
        })
        .collect::<Vec<_>>();
    let sum = kernel.iter().sum::<f32>();
    for value in &mut kernel {
        *value /= sum;
    }
    kernel
}

fn mean(samples: &[f32]) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let len = samples.len() as f32;
    samples.iter().sum::<f32>() / len
}

fn population_std(samples: &[f32]) -> f32 {
    let mean = mean(samples);
    #[allow(clippy::cast_precision_loss)]
    let len = samples.len() as f32;
    (samples
        .iter()
        .map(|sample| {
            let delta = sample - mean;
            delta * delta
        })
        .sum::<f32>()
        / len)
        .sqrt()
}

#[cfg(test)]
mod tests {
    use super::{AcutanceError, measure_acutance};
    use crate::channels::{extract_green, extract_luma};
    use crate::image::{
        BayerPattern, BlackWhiteLevels, CfaImage, CfaSamples, Dimensions, LinearImage, Provenance,
        Rect, RgbImage,
    };
    use crate::zones::{default_zones, project_zone};

    fn image(width: usize, height: usize, samples: Vec<f32>) -> LinearImage {
        LinearImage::new(Dimensions::new(width, height).unwrap(), samples).unwrap()
    }

    fn measure(width: usize, height: usize, samples: Vec<f32>) -> (f32, f32) {
        let image = image(width, height, samples);
        let measured = measure_acutance(
            image
                .patch(Rect::new(0, 0, width, height).unwrap())
                .unwrap(),
        )
        .unwrap();
        (measured.acutance, measured.contrast)
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 2.0e-5,
            "actual {actual} expected {expected}"
        );
    }

    #[test]
    fn constant_patch_has_zero_signal() {
        let measured = measure(15, 15, vec![0.5; 225]);

        assert_eq!(measured, (0.0, 0.0));
    }

    #[test]
    fn high_frequency_patch_scores_above_blurred_patch() {
        let checker = (0..256)
            .map(|index| {
                if (index + (index / 16)) % 2 == 0 {
                    0.0
                } else {
                    1.0
                }
            })
            .collect::<Vec<_>>();
        let ramp = (0..256)
            .map(|index| f32::from(u16::try_from(index).unwrap()) / 255.0)
            .collect::<Vec<_>>();

        let high = measure(16, 16, checker);
        let low = measure(16, 16, ramp);

        assert!(high.0 > low.0, "high {high:?} low {low:?}");
    }

    #[test]
    fn contrast_is_population_std_over_positive_mean() {
        let samples = vec![0.0, 1.0, 2.0, 3.0];
        let measured = measure(2, 2, samples);

        assert_close(measured.1, 1.118_034 / 1.5);
    }

    #[test]
    fn non_positive_mean_has_zero_contrast() {
        let samples = vec![-0.5, 0.5, -0.5, 0.5];
        let measured = measure(2, 2, samples);

        assert_close(measured.1, 0.0);
    }

    #[test]
    fn near_zero_negative_mean_has_finite_zero_contrast() {
        let samples = vec![-0.000_002, 0.0, 0.0, -0.000_002];
        let measured = measure(2, 2, samples);

        assert_close(measured.1, 0.0);
    }

    #[test]
    fn tiny_positive_mean_uses_std_over_mean() {
        let samples = vec![0.0, 0.0, 0.0, 0.000_002];
        let measured = measure(2, 2, samples);

        assert_close(measured.1, 1.732_050_8);
    }

    #[test]
    fn low_variance_patch_keeps_measured_contrast() {
        let samples = vec![0.5, 0.5, 0.5, 0.500_001_5];
        let measured = measure(2, 2, samples);

        assert!(measured.0.abs() <= f32::EPSILON);
        assert!(measured.1 > 0.0, "contrast was {}", measured.1);
    }

    #[test]
    fn matches_prototype_parity_patches() {
        let cases = [
            ("constant", 15, 15, vec![0.5; 225], 0.0, 0.0),
            (
                "ramp",
                15,
                15,
                (0..225)
                    .map(|index| f32::from(u16::try_from(index).unwrap()) / 224.0)
                    .collect(),
                1.069_431_1,
                0.579_920_8,
            ),
            (
                "checkerboard",
                16,
                16,
                (0..256)
                    .map(|index| {
                        if (index + (index / 16)) % 2 == 0 {
                            0.0
                        } else {
                            1.0
                        }
                    })
                    .collect(),
                11.247_48,
                0.999_998,
            ),
            (
                "impulse",
                15,
                15,
                (0..225)
                    .map(|index| if index / 15 == index % 15 { 1.0 } else { 0.0 })
                    .collect(),
                3.136_893_3,
                3.741_654,
            ),
        ];

        for (name, width, height, samples, acutance, contrast) in cases {
            let measured = measure(width, height, samples);
            assert_close(measured.0, acutance);
            assert_close(measured.1, contrast);
            assert!(measured.0.is_finite(), "{name}");
        }
    }

    #[test]
    fn border_policy_uses_zero_padding() {
        let centre_impulse = measure(
            15,
            15,
            (0..225)
                .map(|index| if index == 112 { 1.0 } else { 0.0 })
                .collect(),
        );
        let corner_impulse = measure(
            15,
            15,
            (0..225)
                .map(|index| if index == 0 { 1.0 } else { 0.0 })
                .collect(),
        );

        assert_close(centre_impulse.0, 3.965_870_1);
        assert_close(corner_impulse.0, 4.857_302_7);
    }

    #[test]
    fn tiny_valid_patches_do_not_panic() {
        assert!(measure(1, 1, vec![0.25]).0.is_finite());
        assert!(measure(1, 5, vec![0.0, 0.25, 0.5, 0.75, 1.0]).0.is_finite());
        assert!(measure(5, 1, vec![0.0, 0.25, 0.5, 0.75, 1.0]).0.is_finite());
    }

    #[test]
    fn non_finite_samples_are_errors() {
        let image = image(1, 1, vec![f32::NAN]);

        assert!(matches!(
            measure_acutance(image.patch(Rect::new(0, 0, 1, 1).unwrap()).unwrap()),
            Err(AcutanceError::NonFiniteSample { .. })
        ));
    }

    #[test]
    fn measures_projected_default_zones_for_cfa_and_luma() {
        let cfa = CfaImage::new(
            Dimensions::new(100, 80).unwrap(),
            BayerPattern::Rggb,
            CfaSamples::U16((0_u16..8_000).collect()),
            BlackWhiteLevels::new(&[0.0], &[8_000.0]).unwrap(),
            Provenance::measurement_ready(),
        )
        .unwrap();
        let cfa_plane = extract_green(&cfa).unwrap();
        let cfa_results = default_zones(cfa.dimensions())
            .unwrap()
            .into_iter()
            .map(|zone| project_zone(zone.rect(), cfa_plane.grid))
            .map(|rect| cfa_plane.image.patch(rect.unwrap()))
            .map(|patch| measure_acutance(patch.unwrap()))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        let rgb = RgbImage::new(
            Dimensions::new(100, 80).unwrap(),
            3,
            vec![0.5; 100 * 80 * 3],
            Provenance::measurement_ready(),
        )
        .unwrap();
        let luma = extract_luma(&rgb).unwrap();
        let luma_results = default_zones(rgb.dimensions())
            .unwrap()
            .into_iter()
            .map(|zone| project_zone(zone.rect(), luma.grid))
            .map(|rect| luma.image.patch(rect.unwrap()))
            .map(|patch| measure_acutance(patch.unwrap()))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(cfa_results.len(), 5);
        assert_eq!(luma_results.len(), 5);
    }
}
