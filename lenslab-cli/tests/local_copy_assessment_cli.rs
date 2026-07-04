use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

#[test]
#[ignore = "requires LENSLAB_LOCAL_COPY_ASSESSMENT_FIXTURES with local real DNG captures"]
fn local_real_dng_copy_assessment_emits_support_evidence() {
    let Some(root) = env::var_os("LENSLAB_LOCAL_COPY_ASSESSMENT_FIXTURES") else {
        eprintln!(
            "skipping local copy assessment fixtures: LENSLAB_LOCAL_COPY_ASSESSMENT_FIXTURES is not set"
        );
        return;
    };
    let root = PathBuf::from(root);
    assert!(
        root.is_dir(),
        "LENSLAB_LOCAL_COPY_ASSESSMENT_FIXTURES must point to a directory: {}",
        root.display()
    );

    let inputs = dng_inputs(&root);
    assert!(
        inputs.len() >= 2,
        "LENSLAB_LOCAL_COPY_ASSESSMENT_FIXTURES must contain at least two direct .dng files: {}",
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
    let tmp_output = workspace_tmp().join("local-copy-assessment-analysis.json");
    fs::write(&tmp_output, &output.stdout).expect("write transient local analysis JSON");

    let groups = report["groups"].as_array().expect("groups array");
    assert!(groups.len() >= 2, "expected at least two aperture groups");
    assert_no_verdict_fields(&report);
    assert_copy_support_shape(&report);
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

fn assert_copy_support_shape(report: &Value) {
    let support = &report["copy_assessment"];
    assert!(
        matches!(
            support["state"].as_str(),
            Some("supports_centred" | "supports_decentred" | "inconclusive")
        ),
        "{support}"
    );
    assert_eq!(
        support["method"],
        "derived_from_target_qa_acutance_and_field_curvature"
    );
    assert!(support["hard_support_eligible"].is_boolean());
    assert!(support["included_aperture_groups"].as_array().is_some());
    assert!(
        support["blockers"]
            .as_array()
            .expect("copy blockers")
            .iter()
            .all(Value::is_string)
    );
    assert!(
        support["reshoot"]
            .as_array()
            .expect("copy reshoot")
            .iter()
            .all(Value::is_string)
    );

    let evidence = &support["evidence"];
    for gate in [
        "target_quality",
        "correction_provenance",
        "aperture_series",
        "left_right_consistency",
        "field_curvature_counterevidence",
    ] {
        assert!(
            matches!(
                evidence[gate]["status"].as_str(),
                Some("passed" | "blocked")
            ),
            "{gate}: {}",
            evidence[gate]
        );
    }
    assert!(evidence["centred_threshold"].as_f64().is_some());
    assert!(evidence["decentred_threshold"].as_f64().is_some());
}
