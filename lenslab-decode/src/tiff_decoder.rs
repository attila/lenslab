use std::fs::File;
use std::io::{BufReader, Seek, SeekFrom};
use std::path::Path;

use lenslab_core::image::{Dimensions, Provenance, RgbImage};
use tiff::ColorType;
use tiff::decoder::Decoder as TiffFileDecoder;
use tiff::decoder::DecodingResult;
use tiff::tags::Tag as TiffTag;

use crate::frame_info::{Corrections, ExposureInfo, FrameInfo, SourceKind};
use crate::{DecodeError, DecodedFrame, DecodedPixels, Decoder};

/// Decodes TIFF, treated as already-demosaiced RGB (`docs/SPEC.md` §2) via the permissive `tiff`
/// crate — no `rawler`/LGPL dependency on this path.
pub struct TiffDecoder;

impl Decoder for TiffDecoder {
    fn inspect(&self, path: &Path) -> Result<FrameInfo, DecodeError> {
        let mut reader =
            BufReader::new(File::open(path).map_err(|source| DecodeError::io(path, source))?);

        let (width, height, bits_per_sample) = {
            let mut tiff = TiffFileDecoder::new(&mut reader)
                .map_err(|source| DecodeError::tiff(path, source))?;
            let (width, height) = tiff
                .dimensions()
                .map_err(|source| DecodeError::tiff(path, source))?;
            // BitsPerSample has one entry per channel (e.g. [16, 16, 16] for RGB16); they are
            // uniform in every TIFF flavour we read, so the first value stands for the sample.
            // TIFF 6.0 defines the tag's default as 1 (bilevel) when absent.
            let bits = tiff
                .find_tag_unsigned_vec::<u32>(TiffTag::BitsPerSample)
                .map_err(|source| DecodeError::tiff(path, source))?
                .and_then(|bits| bits.first().copied())
                .unwrap_or(1);
            (width as usize, height as usize, bits as usize)
        };

        reader
            .seek(SeekFrom::Start(0))
            .map_err(|source| DecodeError::io(path, source))?;
        let exif = exif::Reader::new()
            .read_from_container(&mut reader)
            .map_err(|source| DecodeError::exif(path, source))?;

        Ok(tiff_frame_info(width, height, bits_per_sample, &exif))
    }

    fn decode(&self, path: &Path) -> Result<DecodedFrame, DecodeError> {
        let reader =
            BufReader::new(File::open(path).map_err(|source| DecodeError::io(path, source))?);
        let mut tiff =
            TiffFileDecoder::new(reader).map_err(|source| DecodeError::tiff(path, source))?;
        let (width, height) = tiff
            .dimensions()
            .map_err(|source| DecodeError::tiff(path, source))?;
        let bits_per_sample = tiff
            .find_tag_unsigned_vec::<u32>(TiffTag::BitsPerSample)
            .map_err(|source| DecodeError::tiff(path, source))?
            .and_then(|bits| bits.first().copied())
            .unwrap_or(1) as usize;
        let color_type = tiff
            .colortype()
            .map_err(|source| DecodeError::tiff(path, source))?;
        let components = match color_type {
            ColorType::Gray(_) => 1,
            ColorType::RGB(_) => 3,
            _ => {
                return Err(DecodeError::UnsupportedTiffColor {
                    path: path.to_owned(),
                    color_type,
                });
            }
        };
        let samples = read_normalised_samples(path, &mut tiff)?;
        let dimensions = Dimensions::new(width as usize, height as usize)
            .map_err(|source| DecodeError::image(path, source))?;
        let pixels = RgbImage::new(dimensions, components, samples, Provenance::unknown())
            .map_err(|source| DecodeError::image(path, source))?;
        tiff.inner()
            .seek(SeekFrom::Start(0))
            .map_err(|source| DecodeError::io(path, source))?;
        let exif = exif::Reader::new()
            .read_from_container(tiff.inner())
            .map_err(|source| DecodeError::exif(path, source))?;

        Ok(DecodedFrame {
            info: tiff_frame_info(width as usize, height as usize, bits_per_sample, &exif),
            pixels: DecodedPixels::Rgb(pixels),
        })
    }
}

fn tiff_frame_info(
    width: usize,
    height: usize,
    bits_per_sample: usize,
    exif: &exif::Exif,
) -> FrameInfo {
    FrameInfo {
        source_kind: SourceKind::Rgb,
        camera_make: ascii_field(exif, exif::Tag::Make),
        camera_model: ascii_field(exif, exif::Tag::Model),
        lens_model: ascii_field(exif, exif::Tag::LensModel),
        width,
        height,
        bits_per_sample,
        cfa_pattern: None,
        black_level: None,
        white_level: None,
        exposure: ExposureInfo {
            focal_length_mm: rational_field(exif, exif::Tag::FocalLength),
            f_number: rational_field(exif, exif::Tag::FNumber),
            exposure_time_s: rational_field(exif, exif::Tag::ExposureTime),
            iso: uint_field(exif, exif::Tag::PhotographicSensitivity),
        },
        corrections: Corrections {
            present: None,
            detail: vec![
                "TIFF input is already-demosaiced RGB; baked-in corrections cannot be ruled out \
                 from the container alone (docs/DECISIONS.md D8)"
                    .to_owned(),
            ],
        },
    }
}

fn read_normalised_samples(
    path: &Path,
    tiff: &mut TiffFileDecoder<BufReader<File>>,
) -> Result<Vec<f32>, DecodeError> {
    match tiff
        .read_image()
        .map_err(|source| DecodeError::tiff(path, source))?
    {
        DecodingResult::U8(samples) => Ok(samples
            .into_iter()
            .map(|sample| f32::from(sample) / f32::from(u8::MAX))
            .collect()),
        DecodingResult::U16(samples) => Ok(samples
            .into_iter()
            .map(|sample| f32::from(sample) / f32::from(u16::MAX))
            .collect()),
        DecodingResult::F32(samples) => Ok(samples),
        DecodingResult::U32(_) => Err(DecodeError::UnsupportedTiffSamples {
            path: path.to_owned(),
            sample_format: "u32",
        }),
        DecodingResult::U64(_) => Err(DecodeError::UnsupportedTiffSamples {
            path: path.to_owned(),
            sample_format: "u64",
        }),
        DecodingResult::F16(_) => Err(DecodeError::UnsupportedTiffSamples {
            path: path.to_owned(),
            sample_format: "f16",
        }),
        DecodingResult::F64(_) => Err(DecodeError::UnsupportedTiffSamples {
            path: path.to_owned(),
            sample_format: "f64",
        }),
        DecodingResult::I8(_) => Err(DecodeError::UnsupportedTiffSamples {
            path: path.to_owned(),
            sample_format: "i8",
        }),
        DecodingResult::I16(_) => Err(DecodeError::UnsupportedTiffSamples {
            path: path.to_owned(),
            sample_format: "i16",
        }),
        DecodingResult::I32(_) => Err(DecodeError::UnsupportedTiffSamples {
            path: path.to_owned(),
            sample_format: "i32",
        }),
        DecodingResult::I64(_) => Err(DecodeError::UnsupportedTiffSamples {
            path: path.to_owned(),
            sample_format: "i64",
        }),
    }
}

fn ascii_field(exif: &exif::Exif, tag: exif::Tag) -> Option<String> {
    match &exif.get_field(tag, exif::In::PRIMARY)?.value {
        exif::Value::Ascii(values) => {
            let text = String::from_utf8_lossy(values.first()?)
                .trim_end_matches('\0')
                .to_owned();
            (!text.is_empty()).then_some(text)
        }
        _ => None,
    }
}

fn rational_field(exif: &exif::Exif, tag: exif::Tag) -> Option<f32> {
    let value = match &exif.get_field(tag, exif::In::PRIMARY)?.value {
        exif::Value::Rational(values) => values.first().map(exif::Rational::to_f32)?,
        _ => return None,
    };
    value.is_finite().then_some(value)
}

fn uint_field(exif: &exif::Exif, tag: exif::Tag) -> Option<u32> {
    exif.get_field(tag, exif::In::PRIMARY)?.value.get_uint(0)
}

#[cfg(test)]
mod tests {
    use lenslab_core::image::Provenance;
    use tiff::encoder::TiffEncoder;
    use tiff::encoder::colortype::{Gray16, RGB16, RGBA16};
    use tiff::tags::Tag;

    use super::*;

    fn write_synthetic_tiff(path: &Path) {
        let file = File::create(path).expect("create fixture");
        let mut encoder = TiffEncoder::new(file).expect("new encoder");
        let mut image = encoder.new_image::<Gray16>(4, 3).expect("new image");
        image
            .encoder()
            .write_tag(Tag::Make, "lenslab")
            .expect("write Make");
        image
            .encoder()
            .write_tag(Tag::Model, "synthetic")
            .expect("write Model");
        image.write_data(&[0u16; 12]).expect("write pixel data");
    }

    #[test]
    fn inspects_a_synthetic_tiff() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("frame.tif");
        write_synthetic_tiff(&path);

        let info = TiffDecoder.inspect(&path).expect("inspect synthetic TIFF");

        assert_eq!(info.source_kind, SourceKind::Rgb);
        assert_eq!(info.width, 4);
        assert_eq!(info.height, 3);
        assert_eq!(info.bits_per_sample, 16);
        assert_eq!(info.camera_make.as_deref(), Some("lenslab"));
        assert_eq!(info.camera_model.as_deref(), Some("synthetic"));
        assert_eq!(info.corrections.present, None);
    }

    #[test]
    fn inspects_a_synthetic_multi_channel_tiff() {
        // BitsPerSample carries one entry per channel (e.g. [16, 16, 16] for RGB16) — this
        // guards against treating it as a single scalar value.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("frame.tif");
        let file = File::create(&path).expect("create fixture");
        let mut encoder = TiffEncoder::new(file).expect("new encoder");
        let image = encoder.new_image::<RGB16>(2, 2).expect("new image");
        image
            .write_data(&[0u16; 2 * 2 * 3])
            .expect("write pixel data");

        let info = TiffDecoder
            .inspect(&path)
            .expect("inspect synthetic RGB TIFF");

        assert_eq!(info.bits_per_sample, 16);
    }

    #[test]
    fn decodes_a_synthetic_gray16_tiff() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("frame.tif");
        let file = File::create(&path).expect("create fixture");
        let mut encoder = TiffEncoder::new(file).expect("new encoder");
        let image = encoder.new_image::<Gray16>(2, 2).expect("new image");
        image
            .write_data(&[0, 32_768, 49_152, 65_535])
            .expect("write pixel data");

        let frame = TiffDecoder
            .decode(&path)
            .expect("decode synthetic Gray16 TIFF");
        let rgb = frame
            .pixels
            .rgb()
            .expect("TIFF should decode as RGB/luma input");

        assert_eq!(rgb.dimensions().width(), 2);
        assert_eq!(rgb.dimensions().height(), 2);
        assert_eq!(rgb.components(), 1);
        assert!(rgb.samples()[0].abs() < 0.000_001);
        assert!((rgb.samples()[1] - 0.500_007_6).abs() < 0.000_001);
        assert!((rgb.samples()[3] - 1.0).abs() < 0.000_001);
        assert_eq!(rgb.provenance(), Provenance::unknown());
    }

    #[test]
    fn decode_preserves_synthetic_tiff_metadata() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("frame.tif");
        write_synthetic_tiff(&path);

        let frame = TiffDecoder.decode(&path).expect("decode synthetic TIFF");

        assert_eq!(frame.info.camera_make.as_deref(), Some("lenslab"));
        assert_eq!(frame.info.camera_model.as_deref(), Some("synthetic"));
        assert_eq!(frame.info.width, 4);
        assert_eq!(frame.info.height, 3);
        assert_eq!(frame.info.bits_per_sample, 16);
    }

    #[test]
    fn decodes_a_synthetic_rgb16_tiff() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("frame.tif");
        let file = File::create(&path).expect("create fixture");
        let mut encoder = TiffEncoder::new(file).expect("new encoder");
        let image = encoder.new_image::<RGB16>(1, 2).expect("new image");
        image
            .write_data(&[0, 32_768, 65_535, 65_535, 0, 32_768])
            .expect("write pixel data");

        let frame = TiffDecoder
            .decode(&path)
            .expect("decode synthetic RGB16 TIFF");
        let rgb = frame
            .pixels
            .rgb()
            .expect("TIFF should decode as RGB/luma input");

        assert_eq!(rgb.components(), 3);
        assert!(rgb.samples()[0].abs() < 0.000_001);
        assert!((rgb.samples()[1] - 0.500_007_6).abs() < 0.000_001);
        assert!((rgb.samples()[2] - 1.0).abs() < 0.000_001);
        assert_eq!(frame.info.source_kind, SourceKind::Rgb);
    }

    #[test]
    fn rejects_unsupported_tiff_colour_types() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("frame.tif");
        let file = File::create(&path).expect("create fixture");
        let mut encoder = TiffEncoder::new(file).expect("new encoder");
        let image = encoder.new_image::<RGBA16>(1, 1).expect("new image");
        image
            .write_data(&[0, 0, 0, 65_535])
            .expect("write pixel data");

        assert!(matches!(
            TiffDecoder.decode(&path),
            Err(DecodeError::UnsupportedTiffColor { .. })
        ));
    }

    #[test]
    fn surfaces_io_errors_for_missing_files() {
        let err = TiffDecoder
            .inspect(Path::new("/nonexistent/frame.tif"))
            .unwrap_err();
        assert!(matches!(err, DecodeError::Io { .. }));
    }
}
