use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Context, bail};
use image::ColorType;
use image::ImageEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use lenslab_core::channels::{extract_green, extract_luma};
use lenslab_core::image::{Dimensions, LinearImage, RgbImage};
use lenslab_decode::{DecodedFrame, DecodedPixels, FrameInfo};

const THUMB_WIDTH: usize = 96;
const THUMB_HEIGHT: usize = 72;
const LABEL_HEIGHT: usize = 12;
const TILE_PADDING: usize = 6;
const TILE_WIDTH: usize = THUMB_WIDTH + (TILE_PADDING * 2);
const TILE_HEIGHT: usize = THUMB_HEIGHT + LABEL_HEIGHT + (TILE_PADDING * 2);
const MAX_COLUMNS: usize = 4;
const GLYPH_WIDTH: usize = 3;
const GLYPH_HEIGHT: usize = 5;
const GLYPH_SPACING: usize = 1;
const BACKGROUND: [u8; 3] = [18, 20, 22];
const TILE_BACKGROUND: [u8; 3] = [34, 37, 40];
const TEXT: [u8; 3] = [226, 230, 232];

static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub fn write_contact_sheet(paths: &[PathBuf], out: &Path) -> anyhow::Result<()> {
    if paths.is_empty() {
        bail!("contact requires at least one input path");
    }
    reject_output_over_input(paths, out)?;
    preflight_output_path(out)?;

    let mut frames = Vec::with_capacity(paths.len());
    for path in paths {
        let decoder = lenslab_decode::decoder_for(path)?;
        let decoded_frame = decoder.decode(path)?;
        let label = label_for(path, &decoded_frame.info);
        let display = DisplayFrame::from_decoded(decoded_frame).map_err(|source| {
            anyhow::anyhow!(
                "failed to prepare display frame for {}: {source}",
                path.display()
            )
        })?;
        let thumbnail = ThumbnailFrame::from_display(&display);
        frames.push(ContactFrame { label, thumbnail });
    }

    let png = render_contact_sheet(&frames)?;
    write_atomic(out, &png)?;
    Ok(())
}

fn reject_output_over_input(inputs: &[PathBuf], out: &Path) -> anyhow::Result<()> {
    let out_absolute = absolute_path(out)?;
    let out_canonical = out.canonicalize().ok();
    for input in inputs {
        if absolute_path(input)? == out_absolute {
            bail!(
                "output path {} must not be one of the input paths",
                out.display()
            );
        }
        if let (Some(input_canonical), Some(out_canonical)) =
            (input.canonicalize().ok(), out_canonical.as_ref())
            && input_canonical == *out_canonical
        {
            bail!(
                "output path {} must not be one of the input paths",
                out.display()
            );
        }
    }
    Ok(())
}

fn absolute_path(path: &Path) -> anyhow::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_owned())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn write_atomic(out: &Path, png: &[u8]) -> anyhow::Result<()> {
    write_atomic_with_temp(out, png, &temp_output_path(out))
}

fn write_atomic_with_temp(out: &Path, png: &[u8], temp: &Path) -> anyhow::Result<()> {
    preflight_output_path(out)?;

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp)
        .with_context(|| format!("failed to create temporary output {}", temp.display()))?;
    let result = file
        .write_all(png)
        .with_context(|| format!("failed to write temporary output {}", temp.display()))
        .and_then(|()| {
            drop(file);
            std::fs::rename(temp, out)
                .with_context(|| format!("failed to move contact sheet to {}", out.display()))
        });
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}

fn preflight_output_path(out: &Path) -> anyhow::Result<()> {
    let parent = out.parent().filter(|parent| !parent.as_os_str().is_empty());
    if let Some(parent) = parent
        && !parent.exists()
    {
        bail!(
            "output parent directory does not exist: {}",
            parent.display()
        );
    }
    if out.is_dir() {
        bail!("output path is a directory: {}", out.display());
    }
    Ok(())
}

fn temp_output_path(out: &Path) -> PathBuf {
    let suffix = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = out
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("contact.png");
    out.with_file_name(format!(
        ".{file_name}.lenslab-tmp-{}-{suffix}",
        std::process::id()
    ))
}

struct ContactFrame {
    label: String,
    thumbnail: ThumbnailFrame,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DisplayFrame {
    dimensions: Dimensions,
    pixels: Vec<[u8; 3]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ThumbnailFrame {
    pixels: Vec<[u8; 3]>,
}

impl DisplayFrame {
    fn from_decoded(frame: DecodedFrame) -> anyhow::Result<Self> {
        match frame.pixels {
            DecodedPixels::Cfa(image) => Ok(Self::from_linear(&extract_green(&image)?.image)),
            DecodedPixels::Rgb(image) if image.components() == 3 => Ok(Self::from_rgb(&image)),
            DecodedPixels::Rgb(image) => Ok(Self::from_linear(&extract_luma(&image)?.image)),
        }
    }

    fn from_linear(image: &LinearImage) -> Self {
        let pixels = image
            .samples()
            .iter()
            .map(|sample| {
                let value = to_u8(*sample);
                [value, value, value]
            })
            .collect();
        Self {
            dimensions: image.dimensions(),
            pixels,
        }
    }

    fn from_rgb(image: &RgbImage) -> Self {
        let pixels = image
            .samples()
            .chunks_exact(3)
            .map(|rgb| [to_u8(rgb[0]), to_u8(rgb[1]), to_u8(rgb[2])])
            .collect();
        Self {
            dimensions: image.dimensions(),
            pixels,
        }
    }
}

impl ThumbnailFrame {
    fn from_display(frame: &DisplayFrame) -> Self {
        let source_width = frame.dimensions.width();
        let source_height = frame.dimensions.height();
        let mut pixels = Vec::with_capacity(THUMB_WIDTH * THUMB_HEIGHT);
        for target_y in 0..THUMB_HEIGHT {
            let source_y = target_y * source_height / THUMB_HEIGHT;
            for target_x in 0..THUMB_WIDTH {
                let source_x = target_x * source_width / THUMB_WIDTH;
                pixels.push(frame.pixels[(source_y * source_width) + source_x]);
            }
        }
        Self { pixels }
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn to_u8(sample: f32) -> u8 {
    (sample.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn render_contact_sheet(frames: &[ContactFrame]) -> anyhow::Result<Vec<u8>> {
    if frames.is_empty() {
        bail!("contact requires at least one display frame");
    }
    let columns = frames.len().min(MAX_COLUMNS);
    let rows = frames.len().div_ceil(columns);
    let width = columns
        .checked_mul(TILE_WIDTH)
        .context("contact sheet width overflow")?;
    let height = rows
        .checked_mul(TILE_HEIGHT)
        .context("contact sheet height overflow")?;
    let canvas_len = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(3))
        .context("contact sheet pixel buffer overflow")?;
    let mut canvas = vec![0; canvas_len];
    fill_rect(&mut canvas, width, 0, 0, width, height, BACKGROUND);

    for (index, frame) in frames.iter().enumerate() {
        let column = index % columns;
        let row = index / columns;
        let x = column * TILE_WIDTH;
        let y = row * TILE_HEIGHT;
        fill_rect(
            &mut canvas,
            width,
            x,
            y,
            TILE_WIDTH,
            TILE_HEIGHT,
            TILE_BACKGROUND,
        );
        draw_thumbnail(
            &mut canvas,
            width,
            x + TILE_PADDING,
            y + TILE_PADDING,
            &frame.thumbnail,
        );
        draw_label(
            &mut canvas,
            width,
            x + TILE_PADDING,
            y + TILE_PADDING + THUMB_HEIGHT + 3,
            &frame.label,
        );
    }

    let mut png = Vec::new();
    let encoder =
        PngEncoder::new_with_quality(&mut png, CompressionType::Fast, FilterType::NoFilter);
    encoder.write_image(
        &canvas,
        u32::try_from(width).context("contact sheet width exceeds PNG limit")?,
        u32::try_from(height).context("contact sheet height exceeds PNG limit")?,
        ColorType::Rgb8.into(),
    )?;
    Ok(png)
}

fn fill_rect(
    canvas: &mut [u8],
    canvas_width: usize,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    colour: [u8; 3],
) {
    for row in y..y + height {
        for column in x..x + width {
            let offset = ((row * canvas_width) + column) * 3;
            canvas[offset..offset + 3].copy_from_slice(&colour);
        }
    }
}

fn draw_thumbnail(
    canvas: &mut [u8],
    canvas_width: usize,
    x: usize,
    y: usize,
    frame: &ThumbnailFrame,
) {
    for target_y in 0..THUMB_HEIGHT {
        for target_x in 0..THUMB_WIDTH {
            let colour = frame.pixels[(target_y * THUMB_WIDTH) + target_x];
            let offset = (((y + target_y) * canvas_width) + x + target_x) * 3;
            canvas[offset..offset + 3].copy_from_slice(&colour);
        }
    }
}

fn draw_label(canvas: &mut [u8], canvas_width: usize, x: usize, y: usize, label: &str) {
    let max_glyphs = THUMB_WIDTH / (GLYPH_WIDTH + GLYPH_SPACING);
    let mut cursor = x;
    for glyph in sanitised_label(label).chars().take(max_glyphs) {
        draw_glyph(canvas, canvas_width, cursor, y, glyph);
        cursor += GLYPH_WIDTH + GLYPH_SPACING;
    }
}

fn sanitised_label(label: &str) -> String {
    label
        .chars()
        .map(|character| {
            if character.is_ascii_graphic() || character == ' ' {
                character.to_ascii_lowercase()
            } else {
                '?'
            }
        })
        .collect()
}

fn draw_glyph(canvas: &mut [u8], canvas_width: usize, x: usize, y: usize, glyph: char) {
    let bitmap = glyph_bitmap(glyph);
    for (row, bits) in bitmap.iter().enumerate() {
        for column in 0..GLYPH_WIDTH {
            if bits & (1 << (GLYPH_WIDTH - 1 - column)) != 0 {
                let offset = (((y + row) * canvas_width) + x + column) * 3;
                canvas[offset..offset + 3].copy_from_slice(&TEXT);
            }
        }
    }
}

fn label_for(path: &Path, info: &FrameInfo) -> String {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("frame");
    match info.exposure.f_number {
        Some(f_number) if f_number.is_finite() => format!("{stem} f/{f_number:.1}"),
        _ => stem.to_owned(),
    }
}

fn glyph_bitmap(glyph: char) -> [u8; GLYPH_HEIGHT] {
    match glyph {
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b010, 0b010, 0b010],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        'a' => [0b010, 0b101, 0b111, 0b101, 0b101],
        'b' => [0b110, 0b101, 0b110, 0b101, 0b110],
        'c' => [0b011, 0b100, 0b100, 0b100, 0b011],
        'd' => [0b110, 0b101, 0b101, 0b101, 0b110],
        'e' => [0b111, 0b100, 0b110, 0b100, 0b111],
        'f' => [0b111, 0b100, 0b110, 0b100, 0b100],
        'g' => [0b011, 0b100, 0b101, 0b101, 0b011],
        'h' => [0b101, 0b101, 0b111, 0b101, 0b101],
        'i' => [0b111, 0b010, 0b010, 0b010, 0b111],
        'j' => [0b001, 0b001, 0b001, 0b101, 0b010],
        'k' => [0b101, 0b101, 0b110, 0b101, 0b101],
        'l' => [0b100, 0b100, 0b100, 0b100, 0b111],
        'm' => [0b101, 0b111, 0b111, 0b101, 0b101],
        'n' => [0b110, 0b101, 0b101, 0b101, 0b101],
        'o' => [0b010, 0b101, 0b101, 0b101, 0b010],
        'p' => [0b110, 0b101, 0b110, 0b100, 0b100],
        'q' => [0b010, 0b101, 0b101, 0b011, 0b001],
        'r' => [0b110, 0b101, 0b110, 0b101, 0b101],
        's' => [0b011, 0b100, 0b010, 0b001, 0b110],
        't' => [0b111, 0b010, 0b010, 0b010, 0b010],
        'u' => [0b101, 0b101, 0b101, 0b101, 0b111],
        'v' => [0b101, 0b101, 0b101, 0b101, 0b010],
        'w' => [0b101, 0b101, 0b111, 0b111, 0b101],
        'x' => [0b101, 0b101, 0b010, 0b101, 0b101],
        'y' => [0b101, 0b101, 0b010, 0b010, 0b010],
        'z' => [0b111, 0b001, 0b010, 0b100, 0b111],
        '-' => [0b000, 0b000, 0b111, 0b000, 0b000],
        '_' => [0b000, 0b000, 0b000, 0b000, 0b111],
        '.' => [0b000, 0b000, 0b000, 0b000, 0b010],
        '/' => [0b001, 0b001, 0b010, 0b100, 0b100],
        ' ' => [0b000; GLYPH_HEIGHT],
        '?' => [0b111, 0b001, 0b010, 0b000, 0b010],
        _ => glyph_bitmap('?'),
    }
}

#[cfg(test)]
mod tests {
    use lenslab_core::image::{
        BayerPattern, BlackWhiteLevels, CfaImage, CfaPattern, CfaSamples, Dimensions, Provenance,
        RgbImage,
    };
    use lenslab_decode::{Corrections, DecodedFrame, ExposureInfo, SourceKind};

    use super::*;

    fn info() -> FrameInfo {
        FrameInfo {
            source_kind: SourceKind::Rgb,
            camera_make: None,
            camera_model: None,
            lens_model: None,
            width: 2,
            height: 2,
            bits_per_sample: 16,
            cfa_pattern: None,
            black_level: None,
            white_level: None,
            exposure: ExposureInfo {
                focal_length_mm: None,
                f_number: None,
                exposure_time_s: None,
                iso: None,
            },
            corrections: Corrections::default(),
        }
    }

    fn info_with_f_number(f_number: f32) -> FrameInfo {
        let mut info = info();
        info.exposure.f_number = Some(f_number);
        info
    }

    #[test]
    fn display_frame_extracts_bayer_green_and_clamps() {
        let image = CfaImage::new(
            Dimensions::new(4, 4).unwrap(),
            BayerPattern::Rggb,
            CfaSamples::F32(vec![
                0.0, -10.0, 0.0, 50.0, 0.0, 0.0, 0.0, 0.0, 0.0, 125.0, 0.0, 200.0, 0.0, 0.0, 0.0,
                0.0,
            ]),
            BlackWhiteLevels::new(&[0.0], &[100.0]).unwrap(),
            Provenance::measurement_ready(),
        )
        .unwrap();
        let frame = DecodedFrame {
            info: info(),
            pixels: DecodedPixels::Cfa(image),
        };

        let display = DisplayFrame::from_decoded(frame).unwrap();

        assert_eq!(display.dimensions, Dimensions::new(2, 2).unwrap());
        assert_eq!(
            display.pixels,
            vec![[0, 0, 0], [128, 128, 128], [255, 255, 255], [255, 255, 255]]
        );
    }

    #[test]
    fn display_frame_preserves_rgb_sample_order() {
        let image = RgbImage::new(
            Dimensions::new(2, 1).unwrap(),
            3,
            vec![1.0, 0.5, 0.0, 0.25, 0.75, 1.0],
            Provenance::unknown(),
        )
        .unwrap();
        let frame = DecodedFrame {
            info: info(),
            pixels: DecodedPixels::Rgb(image),
        };

        let display = DisplayFrame::from_decoded(frame).unwrap();

        assert_eq!(display.pixels, vec![[255, 128, 0], [64, 191, 255]]);
    }

    #[test]
    fn thumbnails_are_fixed_size_and_drop_source_dimensions() {
        let display = DisplayFrame {
            dimensions: Dimensions::new(2, 2).unwrap(),
            pixels: vec![[0, 0, 0], [64, 64, 64], [128, 128, 128], [255, 255, 255]],
        };

        let thumbnail = ThumbnailFrame::from_display(&display);

        assert_eq!(thumbnail.pixels.len(), THUMB_WIDTH * THUMB_HEIGHT);
        assert_eq!(thumbnail.pixels[0], [0, 0, 0]);
        assert_eq!(thumbnail.pixels[THUMB_WIDTH - 1], [64, 64, 64]);
        assert_eq!(
            thumbnail.pixels[(THUMB_HEIGHT - 1) * THUMB_WIDTH],
            [128, 128, 128]
        );
        assert_eq!(
            thumbnail.pixels[(THUMB_HEIGHT * THUMB_WIDTH) - 1],
            [255, 255, 255]
        );
    }

    #[test]
    fn display_frame_converts_gray_rgb_to_monochrome() {
        let image = RgbImage::new(
            Dimensions::new(2, 1).unwrap(),
            1,
            vec![0.0, 1.0],
            Provenance::unknown(),
        )
        .unwrap();
        let frame = DecodedFrame {
            info: info(),
            pixels: DecodedPixels::Rgb(image),
        };

        let display = DisplayFrame::from_decoded(frame).unwrap();

        assert_eq!(display.pixels, vec![[0, 0, 0], [255, 255, 255]]);
    }

    #[test]
    fn display_frame_rejects_unsupported_cfa() {
        let image = CfaImage::from_raw_levels(
            Dimensions::new(2, 2).unwrap(),
            CfaPattern::Unsupported("x-trans".to_owned()),
            CfaSamples::U16(vec![0, 1, 2, 3]),
            vec![0.0],
            vec![255.0],
            Provenance::measurement_ready(),
        )
        .unwrap();
        let frame = DecodedFrame {
            info: info(),
            pixels: DecodedPixels::Cfa(image),
        };

        let err = DisplayFrame::from_decoded(frame).unwrap_err();

        assert!(err.to_string().contains("unsupported CFA pattern"));
    }

    #[test]
    fn label_sanitisation_bounds_controls_and_fallbacks() {
        let label = sanitised_label("Frame\u{7} À");

        assert_eq!(label, "frame? ?");
        assert_eq!(glyph_bitmap('\u{e0}'), glyph_bitmap('?'));
    }

    #[test]
    fn labels_include_finite_aperture_only() {
        assert_eq!(
            label_for(Path::new("frame.tif"), &info_with_f_number(8.0)),
            "frame f/8.0"
        );
        assert_eq!(
            label_for(Path::new("frame.tif"), &info_with_f_number(f32::NAN)),
            "frame"
        );
    }

    #[test]
    fn rendered_label_stays_in_fixed_band() {
        let blank = ContactFrame {
            label: String::new(),
            thumbnail: ThumbnailFrame {
                pixels: vec![[255, 255, 255]; THUMB_WIDTH * THUMB_HEIGHT],
            },
        };
        let labelled = ContactFrame {
            label: "this-label-is-longer-than-the-render-band".to_owned(),
            thumbnail: ThumbnailFrame {
                pixels: vec![[255, 255, 255]; THUMB_WIDTH * THUMB_HEIGHT],
            },
        };

        let blank = image::load_from_memory(&render_contact_sheet(&[blank]).unwrap())
            .unwrap()
            .into_rgb8();
        let labelled = image::load_from_memory(&render_contact_sheet(&[labelled]).unwrap())
            .unwrap()
            .into_rgb8();

        assert_eq!(blank.dimensions(), labelled.dimensions());
        let mut label_pixels_changed = false;
        for y in 0..blank.height() {
            for x in 0..blank.width() {
                let changed = blank.get_pixel(x, y) != labelled.get_pixel(x, y);
                if (81..86).contains(&y) {
                    label_pixels_changed |= changed;
                } else {
                    assert!(!changed, "pixel changed outside label band at {x},{y}");
                }
            }
        }
        assert!(label_pixels_changed);
    }

    #[test]
    fn atomic_write_does_not_delete_existing_temp_collision() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("contact.png");
        let temp = temp_output_path(&out);
        std::fs::write(&temp, b"existing temp").unwrap();

        let err = write_atomic_with_temp(&out, b"png bytes", &temp).unwrap_err();

        assert!(
            err.to_string()
                .contains("failed to create temporary output")
        );
        assert_eq!(std::fs::read(&temp).unwrap(), b"existing temp");
        assert!(!out.exists());
    }

    #[test]
    fn atomic_write_cleans_up_temp_after_rename_failure() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("contact.png");
        std::fs::create_dir(&out).unwrap();
        let temp = temp_output_path(&out);

        let err = write_atomic_with_temp(&out, b"png bytes", &temp).unwrap_err();

        assert!(err.to_string().contains("output path is a directory"));
        assert!(!temp.exists());
    }
}
