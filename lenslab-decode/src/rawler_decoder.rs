use std::path::Path;

use lenslab_core::image::{
    BayerPattern, BlackWhiteLevels, CfaImage, CfaPattern, CfaSamples, CorrectionProvenance,
    Dimensions, LinearityProvenance, Provenance, RgbImage,
};
use rawler::decoders::pef::PefMakernote;
use rawler::decoders::{Decoder as RawlerTrait, RawDecodeParams, RawMetadata, WellKnownIFD};
use rawler::formats::tiff::ifd::OffsetMode;
use rawler::formats::tiff::{Rational, Value};
use rawler::lens::LensResolver;
use rawler::rawimage::RawPhotometricInterpretation;
use rawler::rawsource::RawSource;
use rawler::tags::DngTag;

use crate::frame_info::{Corrections, ExposureInfo, FrameInfo, SourceKind};
use crate::{DecodeError, DecodedFrame, DecodedPixels, Decoder};

/// Decodes DNG and other camera raws via `rawler` (LGPL-2.1, see `NOTICE`).
pub struct RawlerDecoder;

impl Decoder for RawlerDecoder {
    fn inspect(&self, path: &Path) -> Result<FrameInfo, DecodeError> {
        let source = RawSource::new(path).map_err(|source| DecodeError::io(path, source))?;
        let decoder =
            rawler::get_decoder(&source).map_err(|source| DecodeError::rawler(path, source))?;
        let params = RawDecodeParams::default();

        // `raw_image` decompresses the full pixel array even though we only read its header
        // fields below — for DNG specifically, `rawler` ignores the `dummy` flag and always
        // decompresses (verified against rawler 0.7.2's `plain_image_from_ifd`). The (future)
        // pixel-consuming stages reuse this same call rather than decoding twice.
        let image = decoder
            .raw_image(&source, &params, false)
            .map_err(|source| DecodeError::rawler(path, source))?;
        let metadata = decoder
            .raw_metadata(&source, &params)
            .map_err(|source| DecodeError::rawler(path, source))?;

        Ok(rawler_frame_info(
            &source,
            decoder.as_ref(),
            &image,
            &metadata,
            dng_corrections(decoder.as_ref()),
        ))
    }

    fn decode(&self, path: &Path) -> Result<DecodedFrame, DecodeError> {
        let source = RawSource::new(path).map_err(|source| DecodeError::io(path, source))?;
        let decoder =
            rawler::get_decoder(&source).map_err(|source| DecodeError::rawler(path, source))?;
        let params = RawDecodeParams::default();
        let image = decoder
            .raw_image(&source, &params, false)
            .map_err(|source| DecodeError::rawler(path, source))?;
        let metadata = decoder
            .raw_metadata(&source, &params)
            .map_err(|source| DecodeError::rawler(path, source))?;
        let corrections = dng_corrections(decoder.as_ref());
        let info = rawler_frame_info(
            &source,
            decoder.as_ref(),
            &image,
            &metadata,
            corrections.clone(),
        );
        let provenance = rawler_provenance(&corrections);
        let dimensions = Dimensions::new(image.width, image.height)
            .map_err(|source| DecodeError::image(path, source))?;

        let pixels = match &image.photometric {
            RawPhotometricInterpretation::Cfa(cfg) => {
                let pattern = cfa_pattern(&cfg.cfa.to_string());
                let black_levels = image.blacklevel.as_vec();
                let white_levels = image.whitelevel.as_vec();
                let samples = match image.data {
                    rawler::RawImageData::Integer(samples) => CfaSamples::U16(samples),
                    rawler::RawImageData::Float(samples) => CfaSamples::F32(samples),
                };
                let image = match pattern {
                    CfaPattern::Bayer(_) => {
                        let levels = BlackWhiteLevels::new(&black_levels, &white_levels)
                            .map_err(|source| DecodeError::image(path, source))?;
                        CfaImage::from_pattern(dimensions, pattern, samples, levels, provenance)
                    }
                    CfaPattern::Unsupported(_) => CfaImage::from_raw_levels(
                        dimensions,
                        pattern,
                        samples,
                        black_levels,
                        white_levels,
                        provenance,
                    ),
                }
                .map_err(|source| DecodeError::image(path, source))?;
                DecodedPixels::Cfa(image)
            }
            RawPhotometricInterpretation::BlackIsZero | RawPhotometricInterpretation::LinearRaw => {
                let samples = match image.data {
                    rawler::RawImageData::Integer(samples) => normalise_raw_rgb_samples(
                        samples,
                        image.cpp,
                        &image.blacklevel.as_vec(),
                        &image.whitelevel.as_vec(),
                        path,
                    )?,
                    rawler::RawImageData::Float(samples) => samples,
                };
                DecodedPixels::Rgb(
                    RgbImage::new(dimensions, image.cpp, samples, provenance)
                        .map_err(|source| DecodeError::image(path, source))?,
                )
            }
        };

        Ok(DecodedFrame { info, pixels })
    }
}

fn rawler_frame_info(
    source: &RawSource,
    decoder: &dyn RawlerTrait,
    image: &rawler::RawImage,
    metadata: &RawMetadata,
    corrections: Corrections,
) -> FrameInfo {
    let (source_kind, cfa_pattern) = match &image.photometric {
        RawPhotometricInterpretation::Cfa(cfg) => (SourceKind::Cfa, Some(cfg.cfa.to_string())),
        RawPhotometricInterpretation::BlackIsZero | RawPhotometricInterpretation::LinearRaw => {
            (SourceKind::Rgb, None)
        }
    };

    let lens_model = metadata
        .lens
        .as_ref()
        .map(|lens| lens.lens_name.clone())
        .or_else(|| metadata.exif.lens_model.as_deref().and_then(non_empty))
        .or_else(|| pentax_makernote_lens_model(source, decoder));

    FrameInfo {
        source_kind,
        camera_make: non_empty(&metadata.make),
        camera_model: non_empty(&metadata.model),
        lens_model,
        width: image.width,
        height: image.height,
        bits_per_sample: image.bps,
        cfa_pattern,
        black_level: Some(
            image
                .blacklevel
                .levels
                .iter()
                .map(Rational::as_f32)
                .collect(),
        ),
        white_level: Some(image.whitelevel.as_vec()),
        exposure: ExposureInfo {
            focal_length_mm: metadata
                .exif
                .focal_length
                .as_ref()
                .map(Rational::as_f32)
                .filter(|value| value.is_finite()),
            f_number: metadata
                .exif
                .fnumber
                .as_ref()
                .map(Rational::as_f32)
                .filter(|value| value.is_finite()),
            exposure_time_s: metadata
                .exif
                .exposure_time
                .as_ref()
                .map(Rational::as_f32)
                .filter(|value| value.is_finite()),
            iso: metadata
                .exif
                .iso_speed
                .or_else(|| metadata.exif.iso_speed_ratings.map(u32::from)),
        },
        corrections,
    }
}

fn pentax_makernote_lens_model(source: &RawSource, decoder: &dyn RawlerTrait) -> Option<String> {
    if let Some(model) = pentax_dng_makernote_lens_model(decoder) {
        return Some(model);
    }

    let exif = decoder.ifd(WellKnownIFD::Exif).ok().flatten()?;
    let makernote = exif
        .parse_makernote(&mut source.reader(), OffsetMode::Absolute, &[])
        .ok()
        .flatten()?;
    let settings = match &makernote.get_entry(PefMakernote::LensRec)?.value {
        Value::Byte(settings) if settings.len() >= 2 => settings,
        _ => return None,
    };
    pentax_lens_model_from_lens_rec(settings[0], settings[1])
}

fn pentax_dng_makernote_lens_model(decoder: &dyn RawlerTrait) -> Option<String> {
    let root = decoder.ifd(WellKnownIFD::Root).ok().flatten()?;
    let (Value::Byte(data) | Value::Undefined(data)) = &root.get_entry(0xc634u16)?.value else {
        return None;
    };
    let (lens_id, lens_subid) = pentax_makernote_lens_rec(data)?;
    pentax_lens_model_from_lens_rec(lens_id, lens_subid)
}

fn pentax_makernote_lens_rec(data: &[u8]) -> Option<(u8, u8)> {
    if data.len() < 12 || !data.starts_with(b"PENTAX") {
        return None;
    }
    let little_endian = match data.get(8..10)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let entry_count = read_u16(data, 10, little_endian)? as usize;
    let entries_start = 12usize;

    for entry_index in 0..entry_count {
        let offset = entries_start.checked_add(entry_index.checked_mul(12)?)?;
        let tag = read_u16(data, offset, little_endian)?;
        let field_count = read_u32(data, offset + 4, little_endian)?;
        if tag == 0x003f && field_count >= 2 {
            let settings = data.get(offset + 8..offset + 12)?;
            return Some((settings[0], settings[1]));
        }
    }

    None
}

fn read_u16(data: &[u8], offset: usize, little_endian: bool) -> Option<u16> {
    let bytes: [u8; 2] = data.get(offset..offset + 2)?.try_into().ok()?;
    Some(if little_endian {
        u16::from_le_bytes(bytes)
    } else {
        u16::from_be_bytes(bytes)
    })
}

fn read_u32(data: &[u8], offset: usize, little_endian: bool) -> Option<u32> {
    let bytes: [u8; 4] = data.get(offset..offset + 4)?.try_into().ok()?;
    Some(if little_endian {
        u32::from_le_bytes(bytes)
    } else {
        u32::from_be_bytes(bytes)
    })
}

fn pentax_lens_model_from_lens_rec(lens_id: u8, lens_subid: u8) -> Option<String> {
    if [0, 1, 2].contains(&lens_id) {
        return None;
    }

    LensResolver::new()
        .with_lens_id((u32::from(lens_id), u32::from(lens_subid)))
        .with_mounts(&["k-mount".to_owned()])
        .resolve()
        .map(|lens| lens.lens_name.clone())
}

fn rawler_provenance(corrections: &Corrections) -> Provenance {
    let corrections = match corrections.present {
        Some(false) => CorrectionProvenance::Absent,
        Some(true) => CorrectionProvenance::Present,
        None => CorrectionProvenance::Unknown,
    };
    Provenance::new(corrections, LinearityProvenance::Linear)
}

fn cfa_pattern(pattern: &str) -> CfaPattern {
    match pattern {
        "RGGB" => CfaPattern::Bayer(BayerPattern::Rggb),
        "BGGR" => CfaPattern::Bayer(BayerPattern::Bggr),
        "GBRG" => CfaPattern::Bayer(BayerPattern::Gbrg),
        "GRBG" => CfaPattern::Bayer(BayerPattern::Grbg),
        other => CfaPattern::Unsupported(other.to_owned()),
    }
}

fn normalise_raw_rgb_samples(
    samples: Vec<u16>,
    components: usize,
    black_levels: &[f32],
    white_levels: &[f32],
    path: &Path,
) -> Result<Vec<f32>, DecodeError> {
    samples
        .into_iter()
        .enumerate()
        .map(|(index, sample)| {
            let component = index % components;
            let black = component_level("black", black_levels, component)
                .map_err(|source| DecodeError::image(path, source))?;
            let white = component_level("white", white_levels, component)
                .map_err(|source| DecodeError::image(path, source))?;
            if !(black.is_finite() && white.is_finite() && white > black) {
                return Err(DecodeError::image(
                    path,
                    lenslab_core::image::ImageError::InvalidLevelRange { black, white },
                ));
            }
            Ok((f32::from(sample) - black) / (white - black))
        })
        .collect()
}

fn component_level(
    kind: &'static str,
    levels: &[f32],
    component: usize,
) -> Result<f32, lenslab_core::image::ImageError> {
    match levels {
        [] => Err(lenslab_core::image::ImageError::InvalidLevelCount { kind, count: 0 }),
        [level] => Ok(*level),
        levels if component < levels.len() => Ok(levels[component]),
        levels => Err(lenslab_core::image::ImageError::InvalidLevelCount {
            kind,
            count: levels.len(),
        }),
    }
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// DNG opcode lists (tags 51008/51009/51022) carry baked-in per-pixel corrections — lens
/// vignette/CA/distortion fixes applied ahead of the raw data we measure on (`docs/DECISIONS.md`
/// D8). Other raw formats give `rawler` no equivalent signal, so `present` stays `None`.
fn dng_corrections(decoder: &dyn RawlerTrait) -> Corrections {
    let Ok(Some(ifd)) = decoder.ifd(WellKnownIFD::VirtualDngRawTags) else {
        return Corrections::default();
    };

    let mut detail = Vec::new();
    for (label, tag) in [
        ("OpcodeList1", DngTag::OpcodeList1),
        ("OpcodeList2", DngTag::OpcodeList2),
        ("OpcodeList3", DngTag::OpcodeList3),
    ] {
        if ifd.get_entry(tag).is_some() {
            detail.push(format!("DNG {label} present"));
        }
    }

    Corrections {
        present: Some(!detail.is_empty()),
        detail,
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "real-fixtures")]
    use std::path::{Path, PathBuf};

    #[cfg(feature = "real-fixtures")]
    use crate::{DecodedPixels, Decoder, SourceKind};

    #[cfg(feature = "real-fixtures")]
    use lenslab_core::image::{CfaLevels, CfaPattern};

    #[cfg(feature = "real-fixtures")]
    use super::RawlerDecoder;
    use super::{non_empty, pentax_lens_model_from_lens_rec, pentax_makernote_lens_rec};

    #[test]
    fn non_empty_trims_the_returned_value() {
        assert_eq!(non_empty("  PENTAX 645D  ").as_deref(), Some("PENTAX 645D"));
    }

    #[test]
    fn non_empty_treats_whitespace_only_as_absent() {
        assert_eq!(non_empty("   "), None);
        assert_eq!(non_empty(""), None);
    }

    #[test]
    fn pentax_lens_rec_resolves_k_mount_catalogue_name() {
        assert_eq!(
            pentax_lens_model_from_lens_rec(4, 3).as_deref(),
            Some("Pentax smc PENTAX-FA 43mm F1.9 Limited")
        );
    }

    #[test]
    fn pentax_makernote_lens_rec_reads_inline_lens_type() {
        let mut data = b"PENTAX \0II".to_vec();
        data.extend_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(&0x003fu16.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(&4u32.to_le_bytes());
        data.extend_from_slice(&[4, 3, 0, 0]);

        assert_eq!(pentax_makernote_lens_rec(&data), Some((4, 3)));
    }

    #[cfg(feature = "real-fixtures")]
    fn fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/dng")
            .join(name)
    }

    #[cfg(feature = "real-fixtures")]
    #[test]
    fn inspects_xtrans_dng_fixture_with_opcode_corrections() {
        let path = fixture_path("xtrans_xt3.dng");
        assert!(path.exists(), "missing fixture: {}", path.display());

        let info = RawlerDecoder.inspect(&path).expect("fixture should decode");

        assert_eq!(info.source_kind, SourceKind::Cfa);
        assert_eq!(info.camera_make.as_deref(), Some("Fujifilm"));
        assert_eq!(info.camera_model.as_deref(), Some("X-T3"));
        assert_eq!(info.lens_model, None);
        assert_eq!(info.width, 6240);
        assert_eq!(info.height, 4160);
        assert_eq!(info.bits_per_sample, 16);
        assert_eq!(
            info.cfa_pattern.as_deref(),
            Some("GGBGGRBRGRBGGGBGGRGGRGGBRBGBRGGGRGGB")
        );
        assert_eq!(info.black_level, Some(vec![1022.0; 36]));
        assert_eq!(info.white_level, Some(vec![16383.0]));
        assert_float_eq(info.exposure.focal_length_mm, Some(30.0));
        assert_float_eq(info.exposure.f_number, Some(8.0));
        assert_float_eq(info.exposure.exposure_time_s, Some(1.0 / 170.0));
        assert_eq!(info.exposure.iso, Some(160));
        assert_eq!(info.corrections.present, Some(true));
        assert_eq!(
            info.corrections.detail,
            ["DNG OpcodeList2 present", "DNG OpcodeList3 present"]
        );
    }

    #[cfg(feature = "real-fixtures")]
    #[test]
    fn decodes_xtrans_fixture_as_unsupported_cfa_without_losing_levels() {
        let path = fixture_path("xtrans_xt3.dng");
        assert!(path.exists(), "missing fixture: {}", path.display());

        let frame = RawlerDecoder.decode(&path).expect("fixture should decode");
        let DecodedPixels::Cfa(image) = frame.pixels else {
            panic!("expected CFA pixels");
        };

        assert!(matches!(image.pattern(), CfaPattern::Unsupported(pattern) if pattern.len() == 36));
        assert!(matches!(
            image.levels(),
            CfaLevels::Raw { black, white } if black.len() == 36 && white.len() == 1
        ));
    }

    #[cfg(feature = "real-fixtures")]
    #[test]
    fn inspects_bayer_dng_fixture_without_opcode_corrections() {
        let path = fixture_path("bayer_k1.dng");
        assert!(path.exists(), "missing fixture: {}", path.display());

        let info = RawlerDecoder.inspect(&path).expect("fixture should decode");

        assert_eq!(info.source_kind, SourceKind::Cfa);
        assert_eq!(info.camera_make.as_deref(), Some("Pentax"));
        assert_eq!(info.camera_model.as_deref(), Some("K-1"));
        assert_eq!(
            info.lens_model.as_deref(),
            Some("Pentax HD PENTAX-D FA* 50mm F1.4 SDM AW")
        );
        assert_eq!(info.width, 7392);
        assert_eq!(info.height, 4950);
        assert_eq!(info.bits_per_sample, 14);
        assert_eq!(info.cfa_pattern.as_deref(), Some("RGGB"));
        assert_eq!(info.black_level, Some(vec![64.0; 4]));
        assert_eq!(info.white_level, Some(vec![16316.0]));
        assert_float_eq(info.exposure.focal_length_mm, Some(50.0));
        assert_float_eq(info.exposure.f_number, Some(8.0));
        assert_float_eq(info.exposure.exposure_time_s, Some(0.01));
        assert_eq!(info.exposure.iso, Some(100));
        assert_eq!(info.corrections.present, Some(false));
        assert!(info.corrections.detail.is_empty());
    }

    #[cfg(feature = "real-fixtures")]
    fn assert_float_eq(actual: Option<f32>, expected: Option<f32>) {
        match (actual, expected) {
            (Some(actual), Some(expected)) => assert!(
                (actual - expected).abs() < 0.000_001,
                "expected {expected}, got {actual}"
            ),
            _ => assert_eq!(actual, expected),
        }
    }
}
