use std::fs::File;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;
use tiff::encoder::TiffEncoder;
use tiff::encoder::colortype::{Gray16, RGB16};

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
        .new_image::<Gray16>(width, height)
        .expect("new gray TIFF");
    let samples = (0..width * height)
        .map(|sample| offset.wrapping_add(u16::try_from(sample % 65_535).unwrap()))
        .collect::<Vec<_>>();
    image.write_data(&samples).expect("write gray TIFF data");
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
    write_gray_tiff(&input, 80, 60, 0);

    let output = lenslab(&["analyse", input.to_str().unwrap()]);
    let json = assert_success_json(&output);

    assert_eq!(json["schema_version"], "0.1-acutance");
    assert_eq!(json["inputs"][0]["source_kind"], "rgb");
    assert_eq!(
        json["inputs"][0]["corrections"],
        "accepted_unknown_corrections"
    );
    assert_eq!(json["groups"][0]["frames"][0]["input_index"], 0);
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

    assert_eq!(json["schema_version"], "0.1-acutance");
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
        "artifacts",
        "vignetting",
        "distortion",
        "ca_lateral",
        "mtf50",
    ] {
        assert!(json.get(key).is_none(), "{key}");
    }
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
