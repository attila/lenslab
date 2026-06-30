use std::fs::File;
use std::io::{BufReader, Seek, SeekFrom};
use std::path::Path;

use tiff::decoder::Decoder as TiffFileDecoder;
use tiff::tags::Tag as TiffTag;

use crate::frame_info::{Corrections, ExposureInfo, FrameInfo, SourceKind};
use crate::{DecodeError, Decoder};

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

        Ok(FrameInfo {
            source_kind: SourceKind::Rgb,
            camera_make: ascii_field(&exif, exif::Tag::Make),
            camera_model: ascii_field(&exif, exif::Tag::Model),
            lens_model: ascii_field(&exif, exif::Tag::LensModel),
            width,
            height,
            bits_per_sample,
            cfa_pattern: None,
            black_level: None,
            white_level: None,
            exposure: ExposureInfo {
                focal_length_mm: rational_field(&exif, exif::Tag::FocalLength),
                f_number: rational_field(&exif, exif::Tag::FNumber),
                exposure_time_s: rational_field(&exif, exif::Tag::ExposureTime),
                iso: uint_field(&exif, exif::Tag::PhotographicSensitivity),
            },
            corrections: Corrections {
                present: None,
                detail: vec![
                    "TIFF input is already-demosaiced RGB; baked-in corrections cannot be ruled \
                     out from the container alone (docs/DECISIONS.md D8)"
                        .to_owned(),
                ],
            },
        })
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
    match &exif.get_field(tag, exif::In::PRIMARY)?.value {
        exif::Value::Rational(values) => values.first().map(exif::Rational::to_f32),
        _ => None,
    }
}

fn uint_field(exif: &exif::Exif, tag: exif::Tag) -> Option<u32> {
    exif.get_field(tag, exif::In::PRIMARY)?.value.get_uint(0)
}

#[cfg(test)]
mod tests {
    use tiff::encoder::TiffEncoder;
    use tiff::encoder::colortype::{Gray16, RGB16};
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
    fn surfaces_io_errors_for_missing_files() {
        let err = TiffDecoder
            .inspect(Path::new("/nonexistent/frame.tif"))
            .unwrap_err();
        assert!(matches!(err, DecodeError::Io { .. }));
    }
}
