use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

const EXPECTED_SCHEMA_VERSION: &str = "0.1-copy-assessment-support";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lenslab-cli has a workspace parent")
        .to_path_buf()
}

fn read_text(root: &Path, relative: &str) -> String {
    fs::read_to_string(root.join(relative)).unwrap_or_else(|error| {
        panic!("read {relative}: {error}");
    })
}

fn read_json(root: &Path, relative: &str) -> Value {
    let text = read_text(root, relative);
    serde_json::from_str(&text).unwrap_or_else(|error| {
        panic!("parse {relative}: {error}");
    })
}

fn assert_file(root: &Path, relative: &str) {
    assert!(root.join(relative).is_file(), "missing file: {relative}");
}

fn assert_missing_lens_identity_example(report: &Value) {
    assert!(
        !report["inputs"]
            .as_array()
            .expect("inputs is an array")
            .is_empty(),
        "missing-identity example has CLI inputs"
    );
    let groups = report["groups"].as_array().expect("groups is an array");
    assert!(
        groups
            .iter()
            .any(|group| group["lens_model"].is_null() || group["focal_length_mm"].is_null()),
        "missing-identity example has a group without complete lens identity"
    );
    assert_eq!(
        report["copy_assessment"]["blockers"][0],
        "missing_lens_focal_identity"
    );
    assert!(
        report["copy_assessment"]["reshoot"]
            .as_array()
            .expect("reshoot is an array")
            .is_empty(),
        "missing-identity example has no CLI-provided reshoot advice"
    );
}

fn is_missing_lens_identity_example(path: &Path) -> bool {
    path.file_stem().and_then(|stem| stem.to_str()) == Some("inconclusive-missing-lens-identity")
}

fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|error| {
        panic!("read directory {}: {error}", dir.display());
    }) {
        let path = entry.expect("read directory entry").path();
        if path.is_dir() {
            collect_files(&path, files);
        } else {
            files.push(path);
        }
    }
}

#[test]
fn claude_plugin_is_a_thin_adapter_over_the_shared_skill() {
    let root = repo_root();

    assert_file(&root, "agent-skills/lens-test/SKILL.md");
    assert_file(&root, "plugin/skills/lens-test/SKILL.md");
    assert_file(&root, "plugin/.claude-plugin/plugin.json");
    assert!(
        !root.join(".ai/skills/lens-test").exists(),
        "product skill core must stay outside maintainer-only .ai/skills"
    );

    let manifest = read_json(&root, "plugin/.claude-plugin/plugin.json");
    assert_eq!(manifest["name"], "lenslab");
    assert!(
        manifest["version"].is_string(),
        "manifest version is a string"
    );
    assert!(
        manifest["description"].is_string(),
        "manifest description is a string"
    );

    let adapter = read_text(&root, "plugin/skills/lens-test/SKILL.md");
    assert!(
        adapter.contains("${CLAUDE_PLUGIN_ROOT}/../agent-skills/lens-test/SKILL.md"),
        "Claude adapter points at the shared skill core"
    );
    assert!(
        adapter.contains("${CLAUDE_PLUGIN_ROOT}/../agent-skills/lens-test/references/"),
        "Claude adapter points at the shared skill references"
    );
    assert!(
        !adapter.contains("${CLAUDE_PROJECT_DIR}/agent-skills"),
        "Claude adapter must resolve shared files relative to the plugin"
    );
    for duplicated_core_rule in [
        "## Interpretation",
        "## Reshoot Coaching",
        "copy_assessment.blockers",
        "copy_assessment.reshoot",
    ] {
        assert!(
            !adapter.contains(duplicated_core_rule),
            "Claude adapter must not duplicate shared-core rule: {duplicated_core_rule}"
        );
    }
}

#[test]
fn skill_files_match_the_current_cli_contract() {
    let root = repo_root();
    let mut files = Vec::new();
    collect_files(&root.join("agent-skills/lens-test"), &mut files);
    collect_files(&root.join("plugin"), &mut files);

    for file in files {
        let text = fs::read_to_string(&file).unwrap_or_else(|error| {
            panic!("read {}: {error}", file.display());
        });
        for stale_contract in [
            "reshoot_guidance",
            "lenslab analyse <folder>",
            "--format json,md",
            "lenslab decentre",
        ] {
            assert!(
                !text.contains(stale_contract),
                "{} contains stale CLI/schema contract: {stale_contract}",
                file.display()
            );
        }
    }
}

#[test]
fn golden_examples_cover_supported_and_inconclusive_outcomes() {
    let root = repo_root();
    let examples_dir = root.join("agent-skills/lens-test/references/examples");
    let mut states = Vec::new();
    let mut json_examples = Vec::new();
    let mut has_inconclusive_without_reshoot = false;

    for entry in fs::read_dir(&examples_dir).expect("read examples directory") {
        let path = entry.expect("read examples entry").path();
        if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            json_examples.push(path);
        }
    }
    json_examples.sort();
    assert!(!json_examples.is_empty(), "golden examples exist");

    for example in json_examples {
        let relative = example
            .strip_prefix(&root)
            .expect("example is under repo root");
        let checklist = example.with_extension("json-checklist.md");
        let checklist = checklist.with_file_name(format!(
            "{}-checklist.md",
            example
                .file_stem()
                .expect("example has a stem")
                .to_string_lossy()
        ));
        let checklist_relative = checklist
            .strip_prefix(&root)
            .expect("checklist is under repo root");

        assert!(
            checklist.is_file(),
            "missing checklist for {}",
            relative.display()
        );
        let checklist_text = fs::read_to_string(&checklist).unwrap_or_else(|error| {
            panic!("read {}: {error}", checklist_relative.display());
        });
        assert!(
            checklist_text.contains("\n## Required Facts\n"),
            "{} missing Required Facts section",
            checklist_relative.display()
        );
        assert!(
            checklist_text.contains("\n## Forbidden Claims\n"),
            "{} missing Forbidden Claims section",
            checklist_relative.display()
        );

        let report: Value =
            serde_json::from_str(&fs::read_to_string(&example).unwrap_or_else(|error| {
                panic!("read {}: {error}", relative.display());
            }))
            .unwrap_or_else(|error| panic!("parse {}: {error}", relative.display()));

        if is_missing_lens_identity_example(&example) {
            assert_missing_lens_identity_example(&report);
        }

        assert_eq!(
            report["schema_version"],
            EXPECTED_SCHEMA_VERSION,
            "{} uses the current example schema marker",
            relative.display()
        );
        assert!(report["tool_version"].is_string());
        assert!(report["inputs"].is_array());
        assert!(report["field_curvature"].is_object());
        assert!(report["groups"].is_array());
        assert!(report["copy_assessment"].is_object());
        assert!(report["copy_assessment"]["state"].is_string());
        assert!(report["copy_assessment"]["evidence"].is_object());
        assert!(report["copy_assessment"]["blockers"].is_array());
        assert!(report["copy_assessment"]["reshoot"].is_array());

        let state = report["copy_assessment"]["state"]
            .as_str()
            .expect("state is a string");
        states.push(state.to_owned());
        if state == "inconclusive" {
            assert!(
                !report["copy_assessment"]["blockers"]
                    .as_array()
                    .expect("blockers is an array")
                    .is_empty(),
                "{} has blockers for inconclusive outcomes",
                relative.display()
            );
            has_inconclusive_without_reshoot |= report["copy_assessment"]["reshoot"]
                .as_array()
                .expect("reshoot is an array")
                .is_empty();
        }
    }

    states.sort();
    states.dedup();
    assert!(states.contains(&"supports_centred".to_owned()));
    assert!(states.contains(&"supports_decentred".to_owned()));
    assert!(states.contains(&"inconclusive".to_owned()));
    assert!(
        has_inconclusive_without_reshoot,
        "golden examples cover inconclusive outcomes without CLI-provided reshoot advice"
    );
}
