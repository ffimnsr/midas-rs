//! Dependency and feature inventory checks.

const MANIFEST: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));

fn manifest() -> toml::Value {
  toml::from_str(MANIFEST).expect("valid Cargo.toml")
}

#[test]
fn default_features_enable_every_driver() {
  let manifest = manifest();
  let default = manifest["features"]["default"]
    .as_array()
    .expect("default feature list")
    .iter()
    .filter_map(toml::Value::as_str)
    .collect::<Vec<_>>();

  for feature in ["postgres", "sqlite"] {
    assert!(
      default.contains(&feature),
      "default features must include `{feature}`"
    );
  }
}

#[test]
fn mysql_backend_is_removed() {
  let manifest = manifest();
  let features = manifest["features"].as_table().expect("feature table");
  let dependencies = manifest["dependencies"].as_table().expect("dependency table");

  assert!(
    !features.contains_key("mysql"),
    "`mysql` feature must stay removed"
  );
  assert!(
    !dependencies.contains_key("mysql"),
    "`mysql` must not be a dependency"
  );
  assert!(
    !std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
      .join("src/sequel/mysql.rs")
      .exists(),
    "the MySQL backend module must stay removed"
  );
}

#[test]
fn tls_rustls_is_not_enabled_by_default() {
  let manifest = manifest();
  let features = manifest["features"].as_table().expect("feature table");
  assert!(features.contains_key("tls-rustls"));

  let default = manifest["features"]["default"]
    .as_array()
    .expect("default feature list")
    .iter()
    .filter_map(toml::Value::as_str)
    .collect::<Vec<_>>();

  for feature in ["tls-rustls", "tls-native-tls", "openssl-vendored"] {
    assert!(
      !default.contains(&feature),
      "default features must not include `{feature}`"
    );
  }
}

#[test]
fn no_default_dependency_pulls_openssl_or_rand() {
  let manifest = manifest();
  let dependencies = manifest["dependencies"].as_table().expect("dependency table");

  for dependency in ["openssl", "rand"] {
    assert!(
      !dependencies.contains_key(dependency),
      "`{dependency}` must not be a direct dependency"
    );
  }
}

#[test]
fn driver_features_are_wired_to_optional_dependencies() {
  let manifest = manifest();
  let features = manifest["features"].as_table().expect("feature table");

  for (feature, dependency) in [
    ("postgres", "postgres"),
    ("sqlite", "rusqlite"),
    ("macros", "midas-macros"),
  ] {
    let values = features[feature]
      .as_array()
      .expect("feature entries")
      .iter()
      .filter_map(toml::Value::as_str)
      .collect::<Vec<_>>();
    assert!(
      values.contains(&format!("dep:{dependency}").as_str()),
      "feature `{feature}` must enable `dep:{dependency}`"
    );
  }
}
