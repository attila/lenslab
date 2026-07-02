use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::image::{Dimensions, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoneId {
    Centre,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Zone {
    id: ZoneId,
    rect: Rect,
}

impl Zone {
    #[must_use]
    pub const fn id(self) -> ZoneId {
        self.id
    }

    #[must_use]
    pub const fn rect(self) -> Rect {
        self.rect
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasurementGrid {
    FullResolution,
    BayerGreen { x_phase: u8, y_phase: u8 },
}

pub fn default_zones(dimensions: Dimensions) -> Result<[Zone; 5], ZoneError> {
    let patch_width = scaled_dimension(dimensions.width(), 13, 100)?;
    let patch_height = scaled_dimension(dimensions.height(), 13, 100)?;
    let inset_x = scaled_dimension(dimensions.width(), 45, 1000)?;
    let inset_y = scaled_dimension(dimensions.height(), 45, 1000)?;

    let centre_x = (dimensions.width() - patch_width) / 2;
    let centre_y = (dimensions.height() - patch_height) / 2;
    let right_x = dimensions
        .width()
        .saturating_sub(checked_add(inset_x, patch_width)?);
    let bottom_y = dimensions
        .height()
        .saturating_sub(checked_add(inset_y, patch_height)?);

    Ok([
        Zone {
            id: ZoneId::Centre,
            rect: Rect::new(centre_x, centre_y, patch_width, patch_height)?,
        },
        Zone {
            id: ZoneId::TopLeft,
            rect: Rect::new(inset_x, inset_y, patch_width, patch_height)?,
        },
        Zone {
            id: ZoneId::TopRight,
            rect: Rect::new(right_x, inset_y, patch_width, patch_height)?,
        },
        Zone {
            id: ZoneId::BottomLeft,
            rect: Rect::new(inset_x, bottom_y, patch_width, patch_height)?,
        },
        Zone {
            id: ZoneId::BottomRight,
            rect: Rect::new(right_x, bottom_y, patch_width, patch_height)?,
        },
    ])
}

fn scaled_dimension(
    dimension: usize,
    numerator: usize,
    denominator: usize,
) -> Result<usize, ZoneError> {
    let scaled = dimension
        .checked_mul(numerator)
        .and_then(|value| value.checked_add(denominator / 2))
        .ok_or(ZoneError::DimensionOverflow)?
        / denominator;
    Ok(scaled.clamp(1, dimension))
}

pub fn project_zone(source: Rect, grid: MeasurementGrid) -> Result<Rect, ZoneError> {
    match grid {
        MeasurementGrid::FullResolution => Ok(source),
        MeasurementGrid::BayerGreen { x_phase, y_phase } => {
            let x = project_axis(source.x(), source.width(), x_phase)?;
            let y = project_axis(source.y(), source.height(), y_phase)?;
            Rect::new(x.start, y.start, x.length, y.length).map_err(ZoneError::from)
        }
    }
}

fn project_axis(start: usize, length: usize, phase: u8) -> Result<ProjectedAxis, ZoneError> {
    if phase > 1 {
        return Err(ZoneError::InvalidBayerPhase { phase });
    }

    let phase = usize::from(phase);
    let end = checked_add(start, length)?;
    let first = if start % 2 == phase { start } else { start + 1 };
    if first >= end {
        return Err(ZoneError::EmptyProjection);
    }

    Ok(ProjectedAxis {
        start: (first - phase) / 2,
        length: ((end - 1 - first) / 2) + 1,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProjectedAxis {
    start: usize,
    length: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ZoneError {
    Image(crate::image::ImageError),
    DimensionOverflow,
    InvalidBayerPhase { phase: u8 },
    EmptyProjection,
}

impl Display for ZoneError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Image(source) => Display::fmt(source, formatter),
            Self::DimensionOverflow => write!(formatter, "zone dimensions overflow"),
            Self::InvalidBayerPhase { phase } => {
                write!(formatter, "Bayer phase must be 0 or 1, got {phase}")
            }
            Self::EmptyProjection => write!(formatter, "source rectangle does not cover the grid"),
        }
    }
}

impl Error for ZoneError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Image(source) => Some(source),
            Self::DimensionOverflow | Self::InvalidBayerPhase { .. } | Self::EmptyProjection => {
                None
            }
        }
    }
}

impl From<crate::image::ImageError> for ZoneError {
    fn from(source: crate::image::ImageError) -> Self {
        Self::Image(source)
    }
}

fn checked_add(lhs: usize, rhs: usize) -> Result<usize, ZoneError> {
    lhs.checked_add(rhs).ok_or(ZoneError::DimensionOverflow)
}

#[cfg(test)]
mod tests {
    use super::{MeasurementGrid, ZoneId, default_zones, project_zone};
    use crate::image::{Dimensions, Rect};

    fn signed(value: usize) -> i64 {
        i64::try_from(value).expect("test dimensions fit in i64")
    }

    #[test]
    fn returns_five_stable_zone_identifiers_for_default_layout() {
        let zones = default_zones(Dimensions::new(1000, 800).unwrap()).unwrap();
        let ids = zones.iter().map(|zone| zone.id()).collect::<Vec<_>>();

        assert_eq!(
            ids,
            vec![
                ZoneId::Centre,
                ZoneId::TopLeft,
                ZoneId::TopRight,
                ZoneId::BottomLeft,
                ZoneId::BottomRight,
            ]
        );
        assert_eq!(zones[0].rect(), Rect::new(435, 348, 130, 104).unwrap());
        assert_eq!(zones[1].rect(), Rect::new(45, 36, 130, 104).unwrap());
        assert_eq!(zones[2].rect(), Rect::new(825, 36, 130, 104).unwrap());
        assert_eq!(zones[3].rect(), Rect::new(45, 660, 130, 104).unwrap());
        assert_eq!(zones[4].rect(), Rect::new(825, 660, 130, 104).unwrap());
    }

    #[test]
    fn odd_dimensions_are_deterministic_and_in_bounds() {
        let dimensions = Dimensions::new(1001, 799).unwrap();
        let first = default_zones(dimensions).unwrap();
        let second = default_zones(dimensions).unwrap();

        assert_eq!(first, second);
        for zone in first {
            let rect = zone.rect();
            assert!(rect.x() + rect.width() <= dimensions.width());
            assert!(rect.y() + rect.height() <= dimensions.height());
            assert!(rect.width() > 0);
            assert!(rect.height() > 0);
        }
    }

    #[test]
    fn small_valid_frames_keep_non_empty_zones() {
        let zones = default_zones(Dimensions::new(3, 2).unwrap()).unwrap();

        assert_eq!(zones.len(), 5);
        assert!(zones.iter().all(|zone| zone.rect().width() == 1));
        assert!(zones.iter().all(|zone| zone.rect().height() == 1));
    }

    #[test]
    fn corner_zones_are_symmetric_with_integer_rounding_tolerance() {
        let zones = default_zones(Dimensions::new(1001, 799).unwrap()).unwrap();
        let centre = zones[0].rect();
        let centre_x = signed(centre.x()) + signed(centre.width()) / 2;
        let centre_y = signed(centre.y()) + signed(centre.height()) / 2;
        let offsets = zones[1..]
            .iter()
            .map(|zone| {
                let rect = zone.rect();
                (
                    signed(rect.x()) + signed(rect.width()) / 2 - centre_x,
                    signed(rect.y()) + signed(rect.height()) / 2 - centre_y,
                )
            })
            .collect::<Vec<_>>();

        assert!((offsets[0].0 + offsets[1].0).abs() <= 1);
        assert!((offsets[2].0 + offsets[3].0).abs() <= 1);
        assert!((offsets[0].1 + offsets[2].1).abs() <= 1);
        assert!((offsets[1].1 + offsets[3].1).abs() <= 1);
    }

    #[test]
    fn projects_source_zone_to_bayer_green_half_resolution_grid() {
        let source = Rect::new(45, 36, 130, 104).unwrap();
        let projected = project_zone(
            source,
            MeasurementGrid::BayerGreen {
                x_phase: 1,
                y_phase: 0,
            },
        )
        .unwrap();

        assert_eq!(projected, Rect::new(22, 18, 65, 52).unwrap());
    }

    #[test]
    fn repeated_calls_return_identical_rectangles() {
        let dimensions = Dimensions::new(640, 481).unwrap();

        assert_eq!(default_zones(dimensions), default_zones(dimensions));
    }
}
