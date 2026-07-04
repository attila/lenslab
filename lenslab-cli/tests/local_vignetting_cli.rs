use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

#[test]
#[ignore = "requires LENSLAB_LOCAL_VIGNETTING_FIXTURES with local real DNG captures"]
fn local_real_dng_vignetting_ladder_emits_controlled_trends() {
    let Some(root) = env::var_os("LENSLAB_LOCAL_VIGNETTING_FIXTURES") else {
        eprintln!(
            "skipping local vignetting fixtures: LENSLAB_LOCAL_VIGNETTING_FIXTURES is not set"
        );
        return;
    };
    let root = PathBuf::from(root);
    assert!(
        root.is_dir(),
        "LENSLAB_LOCAL_VIGNETTING_FIXTURES must point to a directory: {}",
        root.display()
    );

    let inputs = dng_inputs(&root);
    assert!(
        !inputs.is_empty(),
        "LENSLAB_LOCAL_VIGNETTING_FIXTURES contains no direct .dng files: {}",
        root.display()
    );

    let output = Command::new(env!("CARGO_BIN_EXE_lenslab"))
        .arg("analyse")
        .args(&inputs)
        .output()
        .expect("run lenslab analyse");
    assert!(
        output.status.success(),
        "lenslab analyse failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let report: Value = serde_json::from_slice(&output.stdout).expect("analysis JSON");
    let tmp_output = workspace_tmp().join("local-vignetting-analysis.json");
    fs::write(&tmp_output, &output.stdout).expect("write transient local analysis JSON");

    let groups = report["groups"].as_array().expect("groups array");
    assert!(groups.len() >= 2, "expected at least two aperture groups");
    assert_no_verdict_fields(&report);
    assert!(
        groups
            .iter()
            .any(|group| { group["vignetting"]["optical_delta_from_reference_stops"].is_object() })
    );

    let mut deltas = groups
        .iter()
        .filter_map(|group| {
            let f_number = group["f_number"].as_f64()?;
            let delta =
                group["vignetting"]["symmetry"]["mean_optical_delta_stops"]["value"].as_f64()?;
            Some((f_number, delta))
        })
        .collect::<Vec<_>>();
    deltas.sort_by(|left, right| left.0.total_cmp(&right.0));
    assert!(
        deltas.first().is_some_and(|(_, delta)| *delta < -0.1),
        "widest eligible aperture should vignette more than the reference: {deltas:?}"
    );
    assert!(
        deltas
            .windows(2)
            .all(|window| window[1].1 >= window[0].1 - 0.12),
        "optical deltas should broadly trend toward zero when stopped down: {deltas:?}"
    );
    assert!(groups.iter().all(|group| {
        group["vignetting"]["symmetry"]["status"]
            .as_str()
            .is_some_and(|status| {
                matches!(
                    status,
                    "radially_symmetric" | "lighting_biased" | "mixed_or_unstable" | "not_assessed"
                )
            })
    }));
}

fn dng_inputs(root: &Path) -> Vec<PathBuf> {
    let mut inputs = fs::read_dir(root)
        .expect("read fixture directory")
        .map(|entry| entry.expect("read fixture entry").path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("dng"))
        })
        .collect::<Vec<_>>();
    inputs.sort();
    inputs
}

fn workspace_tmp() -> PathBuf {
    let tmp = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tmp");
    fs::create_dir_all(&tmp).expect("create workspace tmp");
    tmp
}

fn assert_no_verdict_fields(report: &Value) {
    for key in ["verdict", "copy", "centred", "decentred"] {
        assert!(report.get(key).is_none(), "{key}");
    }
}
