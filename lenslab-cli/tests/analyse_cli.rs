use std::fs::File;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;
use tiff::encoder::TiffEncoder;
use tiff::encoder::colortype::RGB16;

fn lenslab(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lenslab"))
        .args(args)
        .output()
        .expect("run lenslab")
}

fn write_gray_tiff(path: &Path, width: u32, height: u32, offset: u16) {
    let file = File::create(path).expect("create TIFF");
    let mut encoder = TiffEncoder::new(file).expect("new TIFF encoder");
    let image = encoder
        .new_image::<RGB16>(width, height)
        .expect("new RGB TIFF");
    let mut samples = Vec::with_capacity(width as usize * height as usize * 3);
    for sample in 0..width * height {
        let value = offset.wrapping_add(u16::try_from(sample % 65_535).unwrap());
        samples.push(value);
        samples.push(value);
        samples.push(value);
    }
    image.write_data(&samples).expect("write RGB TIFF data");
}

fn write_gray_vignetting_tiff(path: &Path) {
    const WIDTH: u32 = 100;
    const HEIGHT: u32 = 100;
    let file = File::create(path).expect("create TIFF");
    let mut encoder = TiffEncoder::new(file).expect("new TIFF encoder");
    let image = encoder
        .new_image::<RGB16>(WIDTH, HEIGHT)
        .expect("new RGB TIFF");
    let mut mono = vec![30_000_u16; WIDTH as usize * HEIGHT as usize];
    paint_rect(&mut mono, WIDTH, 43, 43, 13, 13, 40_000);
    paint_rect(&mut mono, WIDTH, 5, 5, 13, 13, 20_000);
    paint_rect(&mut mono, WIDTH, 82, 5, 13, 13, 20_000);
    paint_rect(&mut mono, WIDTH, 5, 82, 13, 13, 20_000);
    paint_rect(&mut mono, WIDTH, 82, 82, 13, 13, 20_000);
    let mut samples = Vec::with_capacity(mono.len() * 3);
    for value in mono {
        samples.push(value);
        samples.push(value);
        samples.push(value);
    }
    image.write_data(&samples).expect("write RGB TIFF data");
}

fn paint_rect(
    samples: &mut [u16],
    image_width: u32,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    value: u16,
) {
    for row in y..y + height {
        for column in x..x + width {
            samples[(row * image_width + column) as usize] = value;
        }
    }
}

fn write_rgb_tiff(path: &Path, width: u32, height: u32) {
    let file = File::create(path).expect("create TIFF");
    let mut encoder = TiffEncoder::new(file).expect("new TIFF encoder");
    let image = encoder
        .new_image::<RGB16>(width, height)
        .expect("new RGB TIFF");
    let mut samples = Vec::with_capacity(width as usize * height as usize * 3);
    for index in 0..width * height {
        let high = u16::try_from(index % 65_535).expect("synthetic RGB sample fits in u16");
        let low = u16::try_from((index * 3) % 65_535).expect("synthetic RGB sample fits in u16");
        samples.push(high);
        samples.push(u16::MAX - high);
        samples.push(low);
    }
    image.write_data(&samples).expect("write RGB TIFF data");
}

fn write_ca_shift_tiff(path: &Path, x_shift: i32) {
    const WIDTH: u32 = 100;
    const HEIGHT: u32 = 100;
    let file = File::create(path).expect("create TIFF");
    let mut encoder = TiffEncoder::new(file).expect("new TIFF encoder");
    let image = encoder
        .new_image::<RGB16>(WIDTH, HEIGHT)
        .expect("new RGB TIFF");
    let mut samples = Vec::with_capacity(WIDTH as usize * HEIGHT as usize * 3);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let red_x = x.saturating_add_signed(-x_shift);
            let red = ca_texture(red_x, y);
            let blue = ca_texture(x, y);
            let green = ca_texture(x, y.saturating_add(1));
            samples.push(red);
            samples.push(green);
            samples.push(blue);
        }
    }
    image.write_data(&samples).expect("write RGB TIFF data");
}

fn ca_texture(x: u32, y: u32) -> u16 {
    let x_hash = x.wrapping_mul(2_654_435_761).rotate_left(13) % 24_000;
    let y_hash = y.wrapping_mul(2_246_822_519).rotate_left(7) % 16_000;
    u16::try_from(10_000 + x_hash + y_hash).expect("texture fits in u16")
}

fn ca_corner_names() -> [&'static str; 4] {
    ["top_left", "top_right", "bottom_left", "bottom_right"]
}

fn assert_empty_stdout(output: &Output) {
    assert!(
        output.stdout.is_empty(),
        "stdout was not empty: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

fn assert_success_json(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).expect("analyse stdout is JSON")
}

#[test]
fn analyse_writes_json_for_gray_tiff_to_stdout() {
    let dir = TempDir::new().expect("tempdir");
    let input = dir.path().join("gray.tif");
    write_gray_vignetting_tiff(&input);

    let output = lenslab(&["analyse", input.to_str().unwrap()]);
    let json = assert_success_json(&output);

    assert_eq!(json["schema_version"], "0.1-ca");
    assert_eq!(json["inputs"][0]["source_kind"], "rgb");
    assert_eq!(
        json["inputs"][0]["corrections"],
        "accepted_unknown_corrections"
    );
    assert_eq!(json["groups"][0]["frames"][0]["input_index"], 0);
    assert_eq!(
        json["groups"][0]["decentring"]["target_quality"]["status"],
        "not_assessed"
    );
    assert_eq!(
        json["groups"][0]["decentring"]["target_quality"]["blockers"][0],
        "keystone_not_assessed"
    );
    assert_eq!(
        json["groups"][0]["decentring"]["left_right"]["top_pair"]["included_samples"],
        0
    );
    assert_eq!(
        json["groups"][0]["decentring"]["left_right"]["top_pair"]["excluded"][0]["reason"],
        "unknown_corrections"
    );
    assert_eq!(
        json["groups"][0]["vignetting"]["excluded"][0]["reason"],
        "unknown_corrections"
    );
    assert!(
        json["groups"][0]["vignetting"]["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|blocker| blocker == "unknown_corrections")
    );
    assert_eq!(
        json["groups"][0]["frames"][0]["measurements"]["sharpness"]["zones"]["centre"]["acutance"]
            ["method"],
        "measured"
    );
    assert_eq!(
        json["groups"][0]["frames"][0]["measurements"]["sharpness"]["zones"]["centre"]["texture_usable"]
            ["method"],
        "derived_threshold"
    );
    let frame_falloff = json["groups"][0]["frames"][0]["measurements"]["vignetting"]["zones"]
        ["top_left"]["falloff"]["value"]
        .as_f64()
        .expect("falloff value");
    assert!((frame_falloff + 1.0).abs() < 1.0e-6, "{frame_falloff}");
    for corner in ca_corner_names() {
        assert_eq!(
            json["groups"][0]["frames"][0]["measurements"]["ca_lateral"]["zones"][corner]["blockers"]
                [0],
            "flat_profile"
        );
        assert_eq!(
            json["groups"][0]["ca_lateral"][corner]["excluded"][0]["reason"],
            "unknown_corrections"
        );
    }
}

#[test]
fn analyse_writes_json_for_rgb_tiff_to_stdout() {
    let dir = TempDir::new().expect("tempdir");
    let input = dir.path().join("rgb.tif");
    write_rgb_tiff(&input, 80, 60);

    let output = lenslab(&["analyse", input.to_str().unwrap()]);
    let json = assert_success_json(&output);

    assert_eq!(json["inputs"][0]["source_kind"], "rgb");
    assert!(json["groups"][0]["frames"][0]["measurements"]["sharpness"]["zones"]["top_left"]
        ["contrast"]["value"]
        .as_f64()
        .unwrap()
        .is_finite());
}

#[test]
fn analyse_reports_synthetic_lateral_ca_shift() {
    let dir = TempDir::new().expect("tempdir");
    let input = dir.path().join("shifted.tif");
    write_ca_shift_tiff(&input, 2);

    let output = lenslab(&["analyse", input.to_str().unwrap()]);
    let json = assert_success_json(&output);

    assert_eq!(json["schema_version"], "0.1-ca");
    for corner in ca_corner_names() {
        let shift =
            &json["groups"][0]["frames"][0]["measurements"]["ca_lateral"]["zones"][corner]["shift"];
        assert_close_json(&shift["x"]["value"], 2.0, 0.25);
        assert_close_json(&shift["y"]["value"], 0.0, 0.25);
        assert_close_json(&shift["magnitude"]["value"], 2.0, 0.25);
        assert_eq!(shift["x"]["unit"], "px_fullres");
        assert_eq!(shift["x"]["method"], "measured_channel_correlation");
        assert_eq!(
            json["groups"][0]["ca_lateral"][corner]["included_samples"],
            0
        );
        assert_eq!(
            json["groups"][0]["ca_lateral"][corner]["excluded"][0]["reason"],
            "unknown_corrections"
        );
    }
}

#[test]
fn analyse_reports_near_zero_lateral_ca_for_unshifted_rgb() {
    let dir = TempDir::new().expect("tempdir");
    let input = dir.path().join("unshifted.tif");
    write_ca_shift_tiff(&input, 0);

    let output = lenslab(&["analyse", input.to_str().unwrap()]);
    let json = assert_success_json(&output);

    for corner in ca_corner_names() {
        let shift =
            &json["groups"][0]["frames"][0]["measurements"]["ca_lateral"]["zones"][corner]["shift"];
        assert_close_json(&shift["x"]["value"], 0.0, 0.05);
        assert_close_json(&shift["y"]["value"], 0.0, 0.05);
        assert_close_json(&shift["magnitude"]["value"], 0.0, 0.05);
    }
}

fn assert_close_json(value: &Value, expected: f64, tolerance: f64) {
    let actual = value.as_f64().expect("numeric JSON value");
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual {actual} expected {expected}"
    );
}

#[test]
fn analyse_output_is_byte_deterministic_for_same_inputs() {
    let dir = TempDir::new().expect("tempdir");
    let input = dir.path().join("frame.tif");
    write_gray_tiff(&input, 80, 60, 0);

    let first = lenslab(&["analyse", input.to_str().unwrap()]);
    let second = lenslab(&["analyse", input.to_str().unwrap()]);

    assert!(first.status.success());
    assert!(second.status.success());
    assert_eq!(first.stdout, second.stdout);
}

#[test]
fn analyse_failure_keeps_stdout_empty_for_missing_input() {
    let dir = TempDir::new().expect("tempdir");
    let missing = dir.path().join("missing.tif");

    let output = lenslab(&["analyse", missing.to_str().unwrap()]);

    assert!(!output.status.success());
    assert_empty_stdout(&output);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(missing.to_str().unwrap()),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn analyse_failure_keeps_stdout_empty_when_second_input_fails() {
    let dir = TempDir::new().expect("tempdir");
    let valid = dir.path().join("valid.tif");
    let invalid = dir.path().join("invalid.tif");
    write_gray_tiff(&valid, 80, 60, 0);
    std::fs::write(&invalid, b"not a TIFF").expect("write invalid TIFF");

    let output = lenslab(&[
        "analyse",
        valid.to_str().unwrap(),
        invalid.to_str().unwrap(),
    ]);

    assert!(!output.status.success());
    assert_empty_stdout(&output);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(invalid.to_str().unwrap()),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn analyse_rejects_directory_input_before_decode() {
    let dir = TempDir::new().expect("tempdir");

    let output = lenslab(&["analyse", dir.path().to_str().unwrap()]);

    assert!(!output.status.success());
    assert_empty_stdout(&output);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("directory inputs are not supported"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(unix)]
#[test]
fn analyse_rejects_fifo_input_before_decode() {
    let dir = TempDir::new().expect("tempdir");
    let fifo = dir.path().join("pipe.tif");
    let status = Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("run mkfifo");
    assert!(status.success(), "mkfifo failed");

    let output = lenslab(&["analyse", fifo.to_str().unwrap()]);

    assert!(!output.status.success());
    assert_empty_stdout(&output);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("not a regular file"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn analyse_preserves_cli_input_order() {
    let dir = TempDir::new().expect("tempdir");
    let first = dir.path().join("first.tif");
    let second = dir.path().join("second.tif");
    write_gray_tiff(&first, 80, 60, 0);
    write_gray_tiff(&second, 80, 60, 100);

    let output = lenslab(&["analyse", first.to_str().unwrap(), second.to_str().unwrap()]);
    let json = assert_success_json(&output);

    assert_eq!(json["inputs"][0]["path"], first.display().to_string());
    assert_eq!(json["inputs"][1]["path"], second.display().to_string());
    assert_eq!(json["groups"][0]["frames"][0]["input_index"], 0);
    assert_eq!(json["groups"][0]["frames"][1]["input_index"], 1);
}

#[test]
fn analyse_json_uses_skeleton_schema_not_spec_1_0() {
    let dir = TempDir::new().expect("tempdir");
    let input = dir.path().join("frame.tif");
    write_gray_tiff(&input, 80, 60, 0);

    let output = lenslab(&["analyse", input.to_str().unwrap()]);
    let json = assert_success_json(&output);

    assert_eq!(json["schema_version"], "0.1-ca");
    assert_ne!(json["schema_version"], "1.0");
}

#[test]
fn analyse_json_omits_generated_utc_and_unbuilt_verdict_fields() {
    let dir = TempDir::new().expect("tempdir");
    let input = dir.path().join("frame.tif");
    write_gray_tiff(&input, 80, 60, 0);

    let output = lenslab(&["analyse", input.to_str().unwrap()]);
    let json = assert_success_json(&output);

    for key in [
        "generated_utc",
        "verdict",
        "copy",
        "centred",
        "decentred",
        "confidence",
        "artifacts",
        "distortion",
        "ca_lateral",
        "field_curvature",
        "mtf50",
    ] {
        assert!(json.get(key).is_none(), "{key}");
    }
    assert!(
        json["groups"][0]["vignetting"]
            .as_object()
            .expect("group vignetting")
            .contains_key("raw_corner_mean_stops")
    );
    assert!(
        json["groups"][0]["frames"][0]["measurements"]["vignetting"]
            .as_object()
            .expect("frame vignetting")
            .contains_key("zones")
    );
}

#[test]
fn analyse_unknown_tiff_correction_provenance_is_visible() {
    let dir = TempDir::new().expect("tempdir");
    let input = dir.path().join("frame.tif");
    write_gray_tiff(&input, 80, 60, 0);

    let output = lenslab(&["analyse", input.to_str().unwrap()]);
    let json = assert_success_json(&output);

    assert_eq!(
        json["inputs"][0]["corrections"],
        "accepted_unknown_corrections"
    );
    assert!(
        json["inputs"][0]["correction_provenance"]
            .as_str()
            .unwrap()
            .contains("baked-in corrections cannot be ruled out")
    );
    assert_eq!(
        json["groups"][0]["frames"][0]["aggregation_eligible"],
        false
    );
    assert_eq!(
        json["groups"][0]["decentring"]["left_right"]["bottom_pair"]["included_samples"],
        0
    );
    assert_eq!(
        json["groups"][0]["decentring"]["left_right"]["bottom_pair"]["excluded"][0]["reason"],
        "unknown_corrections"
    );
    assert_eq!(
        json["groups"][0]["vignetting"]["optical_delta_from_reference_stops"],
        Value::Null
    );
    assert_eq!(
        json["groups"][0]["vignetting"]["excluded"][0]["reason"],
        "unknown_corrections"
    );
    assert!(
        json["groups"][0]["frames"][0]["measurements"]["vignetting"]["zones"]["top_left"]
            ["falloff"]["value"]
            .as_f64()
            .unwrap()
            .is_finite()
    );
    for corner in ca_corner_names() {
        assert!(
            json["groups"][0]["frames"][0]["measurements"]["ca_lateral"]["zones"][corner]
                .as_object()
                .unwrap()
                .contains_key("shift")
        );
        assert_eq!(
            json["groups"][0]["ca_lateral"][corner]["excluded"][0]["reason"],
            "unknown_corrections"
        );
    }
}

#[cfg(feature = "real-fixtures")]
fn fixture_path(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/dng")
        .join(name)
        .to_str()
        .expect("fixture path is UTF-8")
        .to_owned()
}

#[cfg(feature = "real-fixtures")]
#[test]
fn analyse_measures_real_bayer_dng_fixture() {
    let input = fixture_path("bayer_k1.dng");
    assert!(Path::new(&input).exists(), "missing fixture: {input}");

    let output = lenslab(&["analyse", &input]);
    let json = assert_success_json(&output);

    assert_eq!(json["inputs"][0]["source_kind"], "cfa");
    assert_eq!(json["inputs"][0]["corrections"], "confirmed_uncorrected");
    assert_eq!(json["groups"][0]["frames"][0]["aggregation_eligible"], true);
    let zones = &json["groups"][0]["frames"][0]["measurements"]["sharpness"]["zones"];
    let top_pair = &json["groups"][0]["decentring"]["left_right"]["top_pair"];
    let bottom_pair = &json["groups"][0]["decentring"]["left_right"]["bottom_pair"];

    assert_eq!(top_pair["id"], "top_left_minus_top_right");
    assert_eq!(top_pair["included_samples"], 1);
    assert_eq!(top_pair["excluded_samples"], 0);
    assert_eq!(top_pair["scatter"], Value::Null);
    assert_eq!(top_pair["reliability_blockers"][0], "insufficient_samples");
    assert!(top_pair["excluded"].as_array().unwrap().is_empty());
    let top_delta = zones["top_left"]["acutance"]["value"].as_f64().unwrap()
        - zones["top_right"]["acutance"]["value"].as_f64().unwrap();
    assert!((top_pair["mean_delta"]["value"].as_f64().unwrap() - top_delta).abs() < 1.0e-6);

    assert_eq!(bottom_pair["id"], "bottom_left_minus_bottom_right");
    assert_eq!(bottom_pair["included_samples"], 1);
    assert_eq!(bottom_pair["excluded_samples"], 0);
    assert_eq!(bottom_pair["scatter"], Value::Null);
    assert_eq!(
        bottom_pair["reliability_blockers"][0],
        "insufficient_samples"
    );
    assert!(bottom_pair["excluded"].as_array().unwrap().is_empty());
    let bottom_delta = zones["bottom_left"]["acutance"]["value"].as_f64().unwrap()
        - zones["bottom_right"]["acutance"]["value"].as_f64().unwrap();
    assert!((bottom_pair["mean_delta"]["value"].as_f64().unwrap() - bottom_delta).abs() < 1.0e-6);

    for corner in ca_corner_names() {
        let ca_zone =
            &json["groups"][0]["frames"][0]["measurements"]["ca_lateral"]["zones"][corner]["shift"];
        for axis in ["x", "y", "magnitude"] {
            assert_eq!(ca_zone[axis]["unit"], "px_fullres");
            assert_eq!(ca_zone[axis]["method"], "measured_channel_correlation");
            assert!(
                ca_zone[axis]["value"].as_f64().unwrap().is_finite(),
                "{corner} {axis}"
            );
        }
        assert_eq!(
            json["groups"][0]["ca_lateral"][corner]["included_samples"],
            1
        );
    }
}

#[cfg(feature = "real-fixtures")]
#[test]
fn analyse_rejects_real_corrected_xtrans_dng_fixture_without_stdout() {
    let input = fixture_path("xtrans_xt3.dng");
    assert!(Path::new(&input).exists(), "missing fixture: {input}");

    let output = lenslab(&["analyse", &input]);

    assert!(!output.status.success());
    assert_empty_stdout(&output);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("corrected inputs are not supported"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
