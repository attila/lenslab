pub mod channels;
pub mod image;
pub mod zones;

#[cfg(test)]
mod tests {
    use crate::channels::{extract_green, extract_luma};
    use crate::image::{
        BayerPattern, BlackWhiteLevels, CfaImage, CfaSamples, Dimensions, ImageError, Provenance,
        Rect, RgbImage,
    };
    use crate::zones::{default_zones, project_zone};

    #[test]
    fn synthetic_cfa_frame_splits_into_projected_zone_views() {
        let image = CfaImage::new(
            Dimensions::new(100, 80).unwrap(),
            BayerPattern::Rggb,
            CfaSamples::U16((0_u16..8_000).collect()),
            BlackWhiteLevels::new(&[0.0], &[8_000.0]).unwrap(),
            Provenance::measurement_ready(),
        )
        .unwrap();
        let extracted = extract_green(&image).unwrap();

        let views = default_zones(image.dimensions())
            .unwrap()
            .into_iter()
            .map(|zone| project_zone(zone.rect(), extracted.grid))
            .map(|rect| extracted.image.patch(rect.unwrap()))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(
            extracted.image.dimensions(),
            Dimensions::new(50, 40).unwrap()
        );
        assert_eq!(views.len(), 5);
        assert!(views.iter().all(|view| view.dimensions().width() > 0));
        assert!(views.iter().all(|view| view.dimensions().height() > 0));
    }

    #[test]
    fn synthetic_rgb_frame_splits_luma_into_projected_zone_views() {
        let image = RgbImage::new(
            Dimensions::new(100, 80).unwrap(),
            3,
            vec![0.5; 100 * 80 * 3],
            Provenance::measurement_ready(),
        )
        .unwrap();
        let extracted = extract_luma(&image).unwrap();

        let views = default_zones(image.dimensions())
            .unwrap()
            .into_iter()
            .map(|zone| project_zone(zone.rect(), extracted.grid))
            .map(|rect| extracted.image.patch(rect.unwrap()))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(extracted.image.dimensions(), image.dimensions());
        assert_eq!(views.len(), 5);
        assert!(views.iter().all(|view| view.rect().width() == 13));
        assert!(views.iter().all(|view| view.rect().height() == 10));
    }

    #[test]
    fn cfa_and_rgb_zones_refer_to_the_same_source_frame_locations() {
        let source_zone = Rect::new(45, 36, 130, 104).unwrap();
        let cfa = CfaImage::new(
            Dimensions::new(1000, 800).unwrap(),
            BayerPattern::Rggb,
            CfaSamples::U16(vec![0; 1000 * 800]),
            BlackWhiteLevels::new(&[0.0], &[255.0]).unwrap(),
            Provenance::measurement_ready(),
        )
        .unwrap();
        let rgb = RgbImage::new(
            Dimensions::new(1000, 800).unwrap(),
            3,
            vec![0.0; 1000 * 800 * 3],
            Provenance::measurement_ready(),
        )
        .unwrap();

        let cfa_plane = extract_green(&cfa).unwrap();
        let rgb_plane = extract_luma(&rgb).unwrap();

        assert_eq!(
            project_zone(source_zone, cfa_plane.grid).unwrap(),
            Rect::new(22, 18, 65, 52).unwrap()
        );
        assert_eq!(
            project_zone(source_zone, rgb_plane.grid).unwrap(),
            source_zone
        );
    }

    #[test]
    fn too_small_cfa_frame_fails_before_metric_code_runs() {
        let image = CfaImage::new(
            Dimensions::new(1, 1).unwrap(),
            BayerPattern::Rggb,
            CfaSamples::U16(vec![0]),
            BlackWhiteLevels::new(&[0.0], &[255.0]).unwrap(),
            Provenance::measurement_ready(),
        )
        .unwrap();

        assert!(matches!(
            extract_green(&image),
            Err(ImageError::ZeroDimension { axis: "width" })
        ));
    }
}
