//! Raw decode backend: the LGPL-2.1 boundary of the workspace (see `NOTICE`,
//! `docs/DECISIONS.md` D2/D3). Defines the [`Decoder`] trait and the two v0.1 implementations —
//! [`RawlerDecoder`] for DNG and other camera raws, [`TiffDecoder`] for already-demosaiced TIFF —
//! behind it, so the LGPL-linked `rawler` dependency stays confined to this crate.

mod frame_info;
mod rawler_decoder;
mod tiff_decoder;

use std::path::{Path, PathBuf};

pub use frame_info::{Corrections, ExposureInfo, FrameInfo, SourceKind};
pub use rawler_decoder::RawlerDecoder;
pub use tiff_decoder::TiffDecoder;

/// Reads EXIF and decode metadata for one frame. No pixel data — see crate docs.
pub trait Decoder {
    /// # Errors
    /// Returns an error if the file cannot be read or its container cannot be parsed.
    fn inspect(&self, path: &Path) -> Result<FrameInfo, DecodeError>;
}

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("failed to read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to decode {path}: {source}")]
    Rawler {
        path: PathBuf,
        #[source]
        source: rawler::RawlerError,
    },
    #[error("failed to decode {path}: {source}")]
    Tiff {
        path: PathBuf,
        #[source]
        source: tiff::TiffError,
    },
    #[error("failed to read EXIF in {path}: {source}")]
    Exif {
        path: PathBuf,
        #[source]
        source: exif::Error,
    },
    #[error("{path} has no recognised DNG/TIFF extension")]
    UnsupportedExtension { path: PathBuf },
}

impl DecodeError {
    fn io(path: &Path, source: std::io::Error) -> Self {
        Self::Io {
            path: path.to_owned(),
            source,
        }
    }

    fn rawler(path: &Path, source: rawler::RawlerError) -> Self {
        Self::Rawler {
            path: path.to_owned(),
            source,
        }
    }

    fn tiff(path: &Path, source: tiff::TiffError) -> Self {
        Self::Tiff {
            path: path.to_owned(),
            source,
        }
    }

    fn exif(path: &Path, source: exif::Error) -> Self {
        Self::Exif {
            path: path.to_owned(),
            source,
        }
    }
}

/// Picks a [`Decoder`] for `path` by extension: `.tif`/`.tiff` goes through [`TiffDecoder`];
/// anything in `rawler`'s own supported-extension list (DNG primarily, but any camera raw it
/// reads — see `docs/SPEC.md` §2) goes through [`RawlerDecoder`].
///
/// # Errors
/// Returns [`DecodeError::UnsupportedExtension`] if `path`'s extension is neither.
pub fn decoder_for(path: &Path) -> Result<Box<dyn Decoder>, DecodeError> {
    let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
        return Err(DecodeError::UnsupportedExtension {
            path: path.to_owned(),
        });
    };

    if ext.eq_ignore_ascii_case("tif") || ext.eq_ignore_ascii_case("tiff") {
        return Ok(Box::new(TiffDecoder));
    }
    if rawler::decoders::supported_extensions()
        .iter()
        .any(|known| known.eq_ignore_ascii_case(ext))
    {
        return Ok(Box::new(RawlerDecoder));
    }
    Err(DecodeError::UnsupportedExtension {
        path: path.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::{DecodeError, decoder_for};

    #[test]
    fn dispatches_tiff_extensions_to_tiff_decoder() {
        assert!(decoder_for("frame.tif".as_ref()).is_ok());
        assert!(decoder_for("frame.TIFF".as_ref()).is_ok());
    }

    #[test]
    fn dispatches_other_extensions_to_rawler() {
        assert!(decoder_for("frame.dng".as_ref()).is_ok());
        assert!(decoder_for("frame.NEF".as_ref()).is_ok());
    }

    #[test]
    fn rejects_paths_without_an_extension() {
        assert!(matches!(
            decoder_for("frame".as_ref()),
            Err(DecodeError::UnsupportedExtension { .. })
        ));
    }

    #[test]
    fn rejects_extensions_rawler_does_not_support() {
        assert!(matches!(
            decoder_for("frame.jpg".as_ref()),
            Err(DecodeError::UnsupportedExtension { .. })
        ));
    }
}
