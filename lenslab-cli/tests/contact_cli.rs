use std::fs::File;
use std::path::Path;
use std::process::{Command, Output};

use image::GenericImageView;
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
        .map(|sample| {
            offset.saturating_add(
                u16::try_from(sample * 4096).expect("synthetic gray sample fits in u16"),
            )
        })
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
        let high = u16::try_from(index * 2048).expect("synthetic RGB sample fits in u16");
        let low = u16::try_from(index * 1024).expect("synthetic RGB sample fits in u16");
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

#[test]
fn contact_writes_png_and_keeps_stdout_empty() {
    let dir = TempDir::new().expect("tempdir");
    let input = dir.path().join("frame.tif");
    let out = dir.path().join("contact.png");
    write_gray_tiff(&input, 4, 3, 0);

    let output = lenslab(&[
        "contact",
        input.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_empty_stdout(&output);
    let image = image::open(&out).expect("decode contact PNG");
    assert_eq!(image.dimensions(), (108, 96));
}

#[test]
fn contact_output_is_byte_deterministic_for_same_inputs() {
    let dir = TempDir::new().expect("tempdir");
    let first = dir.path().join("a.tif");
    let second = dir.path().join("b.tif");
    let out_a = dir.path().join("contact-a.png");
    let out_b = dir.path().join("contact-b.png");
    write_gray_tiff(&first, 4, 3, 0);
    write_rgb_tiff(&second, 3, 2);

    let first_run = lenslab(&[
        "contact",
        first.to_str().unwrap(),
        second.to_str().unwrap(),
        "--out",
        out_a.to_str().unwrap(),
    ]);
    let second_run = lenslab(&[
        "contact",
        first.to_str().unwrap(),
        second.to_str().unwrap(),
        "--out",
        out_b.to_str().unwrap(),
    ]);

    assert!(first_run.status.success());
    assert!(second_run.status.success());
    assert_eq!(
        std::fs::read(&out_a).expect("read first PNG"),
        std::fs::read(&out_b).expect("read second PNG")
    );
}

#[test]
fn contact_failure_keeps_stdout_empty_and_preserves_existing_output() {
    let dir = TempDir::new().expect("tempdir");
    let input = dir.path().join("valid.tif");
    let missing = dir.path().join("missing.tif");
    let out = dir.path().join("contact.png");
    write_gray_tiff(&input, 2, 2, 0);
    std::fs::write(&out, b"existing output").expect("seed output");

    let output = lenslab(&[
        "contact",
        input.to_str().unwrap(),
        missing.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);

    assert!(!output.status.success());
    assert_empty_stdout(&output);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("failed to read"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read(&out).expect("read existing output"),
        b"existing output"
    );
}

#[test]
fn contact_rejects_output_path_that_is_an_input_before_decode() {
    let dir = TempDir::new().expect("tempdir");
    let input = dir.path().join("frame.tif");
    std::fs::write(&input, b"not a TIFF").expect("seed input");

    let output = lenslab(&[
        "contact",
        input.to_str().unwrap(),
        "--out",
        input.to_str().unwrap(),
    ]);

    assert!(!output.status.success());
    assert_empty_stdout(&output);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("must not be one of the input paths"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read(&input).expect("read input"), b"not a TIFF");
}

#[test]
fn contact_requires_inputs_and_output_path() {
    let no_input = lenslab(&["contact", "--out", "contact.png"]);
    let no_output = lenslab(&["contact", "frame.tif"]);

    assert!(!no_input.status.success());
    assert!(!no_output.status.success());
    assert_empty_stdout(&no_input);
    assert_empty_stdout(&no_output);
}

#[test]
fn contact_reports_missing_output_parent() {
    let dir = TempDir::new().expect("tempdir");
    let input = dir.path().join("frame.tif");
    let out = dir.path().join("missing").join("contact.png");
    std::fs::write(&input, b"not a TIFF").expect("seed invalid input");

    let output = lenslab(&[
        "contact",
        input.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);

    assert!(!output.status.success());
    assert_empty_stdout(&output);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("parent directory does not exist"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("failed to decode"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!out.exists());
}

#[test]
fn labels_change_pixels_without_changing_geometry() {
    let dir = TempDir::new().expect("tempdir");
    let alpha = dir.path().join("alpha.tif");
    let beta = dir.path().join("beta.tif");
    let out_alpha = dir.path().join("alpha.png");
    let out_beta = dir.path().join("beta.png");
    write_gray_tiff(&alpha, 2, 2, 0);
    write_gray_tiff(&beta, 2, 2, 0);

    assert!(
        lenslab(&[
            "contact",
            alpha.to_str().unwrap(),
            "--out",
            out_alpha.to_str().unwrap(),
        ])
        .status
        .success()
    );
    assert!(
        lenslab(&[
            "contact",
            beta.to_str().unwrap(),
            "--out",
            out_beta.to_str().unwrap(),
        ])
        .status
        .success()
    );

    let alpha_png = image::open(&out_alpha).expect("decode alpha PNG");
    let beta_png = image::open(&out_beta).expect("decode beta PNG");
    assert_eq!(alpha_png.dimensions(), beta_png.dimensions());
    assert_ne!(
        std::fs::read(&out_alpha).expect("read alpha PNG"),
        std::fs::read(&out_beta).expect("read beta PNG")
    );
}

#[test]
fn inspect_still_writes_json_to_stdout() {
    let dir = TempDir::new().expect("tempdir");
    let input = dir.path().join("frame.tif");
    write_gray_tiff(&input, 2, 2, 0);

    let output = lenslab(&["inspect", input.to_str().unwrap()]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("inspect stdout is JSON");
    assert_eq!(json["source_kind"], "rgb");
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
fn contact_writes_png_from_real_bayer_dng_fixture() {
    let dir = TempDir::new().expect("tempdir");
    let input = fixture_path("bayer_k1.dng");
    let out = dir.path().join("bayer-contact.png");
    assert!(Path::new(&input).exists(), "missing fixture: {input}");

    let output = lenslab(&["contact", &input, "--out", out.to_str().unwrap()]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_empty_stdout(&output);
    image::open(&out).expect("decode contact PNG");
}

#[cfg(feature = "real-fixtures")]
#[test]
fn contact_rejects_real_xtrans_dng_fixture() {
    let dir = TempDir::new().expect("tempdir");
    let input = fixture_path("xtrans_xt3.dng");
    let out = dir.path().join("xtrans-contact.png");
    assert!(Path::new(&input).exists(), "missing fixture: {input}");

    let output = lenslab(&["contact", &input, "--out", out.to_str().unwrap()]);

    assert!(!output.status.success());
    assert_empty_stdout(&output);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unsupported CFA pattern"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!out.exists());
}
