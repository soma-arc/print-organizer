use jsonschema::Validator;
use serde_json::Value;
use std::path::Path;

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

fn load_schema() -> Value {
    let path = workspace_root().join("docs/schemas/project.v1.schema.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read schema {}: {e}", path.display()));
    serde_json::from_str(&text).expect("schema is not valid JSON")
}

fn validate_example(schema: &Validator, name: &str, json: &Value) {
    if let Err(err) = schema.validate(json) {
        panic!("example {name} failed schema validation: {err}");
    }
}

#[test]
fn test_all_examples_conform_to_schema() {
    let schema_value = load_schema();
    let validator = Validator::new(&schema_value).expect("schema itself is invalid");

    let examples_dir = workspace_root().join("examples");
    let mut validated = 0;

    for entry in std::fs::read_dir(&examples_dir).expect("cannot read examples dir") {
        let entry = entry.unwrap();
        if !entry.file_type().unwrap().is_dir() {
            continue;
        }
        let dir_name = entry.file_name();
        let dir_name = dir_name.to_string_lossy();

        // Each example directory has a project JSON named <dir_name>.json
        let json_path = entry.path().join(format!("{dir_name}.json"));
        if !json_path.exists() {
            continue;
        }

        let text = std::fs::read_to_string(&json_path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", json_path.display()));
        let value: Value = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", json_path.display()));

        validate_example(&validator, &dir_name, &value);
        validated += 1;
    }

    assert!(
        validated >= 8,
        "expected at least 8 example JSONs, found {validated}"
    );
}

#[test]
fn test_invalid_json_rejected_by_schema() {
    let schema_value = load_schema();
    let validator = Validator::new(&schema_value).expect("schema itself is invalid");

    // brick_size must be 32, 64, or 128
    let bad_brick: Value = serde_json::json!({
        "grid": { "brick_size": 50 }
    });
    assert!(
        validator.validate(&bad_brick).is_err(),
        "brick_size=50 should be rejected"
    );

    // aabb_size items must be > 0
    let bad_aabb: Value = serde_json::json!({
        "grid": { "aabb_size": [0, 64, 64] }
    });
    assert!(
        validator.validate(&bad_aabb).is_err(),
        "aabb_size with 0 should be rejected"
    );

    // dtype must be f32 or f16
    let bad_dtype: Value = serde_json::json!({
        "bake": { "dtype": "f64" }
    });
    assert!(
        validator.validate(&bad_dtype).is_err(),
        "dtype=f64 should be rejected"
    );
}
