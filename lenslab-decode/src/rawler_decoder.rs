use std::path::Path;

use rawler::decoders::{Decoder as RawlerTrait, RawDecodeParams, WellKnownIFD};
use rawler::formats::tiff::Rational;
use rawler::rawimage::RawPhotometricInterpretation;
use rawler::rawsource::RawSource;
use rawler::tags::DngTag;

use crate::frame_info::{Corrections, ExposureInfo, FrameInfo, SourceKind};
use crate::{DecodeError, Decoder};

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
            .or_else(|| metadata.exif.lens_model.as_deref().and_then(non_empty));

        Ok(FrameInfo {
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
            corrections: dng_corrections(decoder.as_ref()),
        })
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
    use super::non_empty;

    #[test]
    fn non_empty_trims_the_returned_value() {
        assert_eq!(non_empty("  PENTAX 645D  ").as_deref(), Some("PENTAX 645D"));
    }

    #[test]
    fn non_empty_treats_whitespace_only_as_absent() {
        assert_eq!(non_empty("   "), None);
        assert_eq!(non_empty(""), None);
    }
}
