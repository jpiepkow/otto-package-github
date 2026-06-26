use serde_json::Value as JsonValue;
use std::path::{Path, PathBuf};
use toml::Value as TomlValue;

#[test]
fn github_package_manifest_runs_real_runtime_by_default() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest_path = root.join("otto.toml");
    let manifest = std::fs::read_to_string(&manifest_path).expect("read otto.toml");
    let manifest = toml::from_str::<TomlValue>(&manifest).expect("parse otto.toml");

    assert_eq!(manifest["package_id"].as_str(), Some("com.otto.github"));
    assert_eq!(
        manifest["protocol_version"].as_str(),
        Some("otto.extension.rpc.v1")
    );
    assert_file_exists(&root, manifest["icon"].as_str().expect("icon path"));

    let runtime = manifest["runtime"].as_table().expect("runtime section");
    assert_eq!(runtime["command"].as_str(), Some("bin/otto-tool-github"));
    assert_eq!(runtime["args"].as_array().map(Vec::len), Some(0));

    let provides = manifest["provides"].as_table().expect("provides section");
    assert_eq!(provides["tools"]["version"].as_integer(), Some(1));

    for schema in manifest["schemas"].as_array().expect("schemas array") {
        assert_file_exists(&root, schema["path"].as_str().expect("schema path"));
    }
}

#[test]
fn github_package_schemas_do_not_use_relative_cross_file_refs() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest_path = root.join("otto.toml");
    let manifest = std::fs::read_to_string(&manifest_path).expect("read otto.toml");
    let manifest = toml::from_str::<TomlValue>(&manifest).expect("parse otto.toml");

    for schema in manifest["schemas"].as_array().expect("schemas array") {
        let relative = schema["path"].as_str().expect("schema path");
        if !relative.ends_with(".json") {
            continue;
        }
        let path = root.join(relative);
        let document = std::fs::read_to_string(&path).expect("read schema");
        let document = serde_json::from_str::<JsonValue>(&document).expect("parse schema json");
        assert_no_relative_cross_file_refs(&document, relative);
    }
}

fn assert_file_exists(root: &Path, relative: &str) {
    let path = root.join(relative);
    assert!(path.is_file(), "{} should exist", path.display());
}

fn assert_no_relative_cross_file_refs(value: &JsonValue, schema_path: &str) {
    match value {
        JsonValue::Object(object) => {
            if let Some(JsonValue::String(reference)) = object.get("$ref") {
                assert!(
                    reference.starts_with('#')
                        || reference.starts_with("https://")
                        || reference.starts_with("http://"),
                    "{schema_path} uses relative cross-file $ref {reference:?}"
                );
            }
            for nested in object.values() {
                assert_no_relative_cross_file_refs(nested, schema_path);
            }
        }
        JsonValue::Array(items) => {
            for nested in items {
                assert_no_relative_cross_file_refs(nested, schema_path);
            }
        }
        JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) | JsonValue::String(_) => {}
    }
}
