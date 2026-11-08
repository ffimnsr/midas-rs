//! Feature wiring checks for the CLI crate.

const CLI_MANIFEST: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));
const WORKSPACE_MANIFEST: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml"));

fn cli_manifest() -> toml::Value {
  toml::from_str(CLI_MANIFEST).expect("valid midas-cli manifest")
}

fn workspace_manifest() -> toml::Value {
  toml::from_str(WORKSPACE_MANIFEST).expect("valid workspace manifest")
}

#[test]
fn cli_dependency_does_not_force_library_default_features() {
  let cli = cli_manifest();
  assert_eq!(
    cli["dependencies"]["midas"]["workspace"].as_bool(),
    Some(true),
    "midas must be inherited from the workspace"
  );

  let workspace = workspace_manifest();
  assert_eq!(
    workspace["workspace"]["dependencies"]["midas"]["default-features"].as_bool(),
    Some(false),
    "the workspace midas dependency must disable default features"
  );
}

#[test]
fn driver_features_propagate_to_the_library() {
  let cli = cli_manifest();
  let features = cli["features"].as_table().expect("feature table");

  for (feature, dependency_feature) in [
    ("postgres", "midas/postgres"),
    ("mysql", "midas/mysql"),
    ("sqlite", "midas/sqlite"),
    ("macros", "midas/macros"),
  ] {
    let values = features[feature]
      .as_array()
      .expect("feature entries")
      .iter()
      .filter_map(toml::Value::as_str)
      .collect::<Vec<_>>();
    assert!(
      values.contains(&dependency_feature),
      "`{feature}` must enable `{dependency_feature}`"
    );
  }
}

#[test]
fn binaries_require_the_cli_feature() {
  let cli = cli_manifest();
  for binary in ["midas", "cargo-migrate"] {
    let entry = cli["bin"]
      .as_array()
      .expect("bin entries")
      .iter()
      .find(|entry| entry["name"].as_str() == Some(binary))
      .expect("binary entry");
    let required = entry["required-features"]
      .as_array()
      .expect("required-features")
      .iter()
      .filter_map(toml::Value::as_str)
      .collect::<Vec<_>>();
    assert_eq!(required, vec!["cli"]);
  }
}
