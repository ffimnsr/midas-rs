//! Configuration contract shared by the CLI and every driver.
//!
//! Resolution order is `CLI flags > environment > midas.toml > defaults`.
//! Environment access is injectable through [`EnvSource`] so tests never mutate
//! process-global state.

use std::collections::BTreeMap;
use std::path::{
  Path,
  PathBuf,
};
use std::time::Duration;

use serde::{
  Deserialize,
  Serialize,
};
use url::Url;

use crate::{
  Error,
  Result,
};

/// Name of the project configuration file.
pub const CONFIG_FILE: &str = "midas.toml";

/// Default metadata table used when the configuration does not override it.
pub const DEFAULT_METADATA_TABLE: &str = "__schema_migrations";

/// Default directory that stores migration files.
pub const DEFAULT_MIGRATIONS_DIR: &str = "migrations";

/// Default time to wait for an advisory lock.
pub const DEFAULT_LOCK_TIMEOUT: Duration = Duration::from_secs(10);

/// Placeholder written instead of a real password when redacting URLs.
pub const REDACTED: &str = "***";

const ALLOWED_SCHEMES: &[&str] = &["postgres", "postgresql", "mysql", "sqlite", "sqlite3", "file"];

/// How migrations are wrapped in transactions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransactionMode {
  /// Wrap each migration in its own transaction.
  #[default]
  PerMigration,
  /// Do not wrap migrations in transactions.
  None,
}

/// Transport security preference for database connections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TlsMode {
  /// Never negotiate TLS.
  #[default]
  Disable,
  /// Negotiate TLS when the server supports it.
  Prefer,
  /// Fail when TLS is unavailable.
  Require,
}

/// Fully resolved runtime configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
  /// Connection URL. `None` for commands that do not touch a database.
  pub database_url: Option<String>,
  /// Directory containing migration files.
  pub migrations_dir: PathBuf,
  /// Optional metadata schema (Postgres only).
  pub metadata_schema: Option<String>,
  /// Metadata table name.
  pub metadata_table: String,
  /// Optional shadow database used for drift detection.
  pub shadow_database_url: Option<String>,
  /// Advisory lock timeout.
  pub lock_timeout: Duration,
  /// Statement timeout. `None` disables it.
  pub statement_timeout: Option<Duration>,
  /// Transaction policy.
  pub transaction: TransactionMode,
  /// TLS policy.
  pub tls: TlsMode,
  /// Value reported as the connection `application_name`.
  pub application_name: String,
  /// Optional search path applied after connecting.
  pub search_path: Option<String>,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      database_url: None,
      migrations_dir: PathBuf::from(DEFAULT_MIGRATIONS_DIR),
      metadata_schema: None,
      metadata_table: DEFAULT_METADATA_TABLE.to_string(),
      shadow_database_url: None,
      lock_timeout: DEFAULT_LOCK_TIMEOUT,
      statement_timeout: None,
      transaction: TransactionMode::default(),
      tls: TlsMode::default(),
      application_name: "midas".to_string(),
      search_path: None,
    }
  }
}

/// Values supplied directly by CLI flags, which win over every other source.
#[derive(Debug, Clone, Default)]
pub struct ConfigOverrides {
  /// Database connection URL.
  pub database_url: Option<String>,
  /// Migration directory.
  pub migrations_dir: Option<PathBuf>,
  /// Metadata schema.
  pub metadata_schema: Option<String>,
  /// Metadata table.
  pub metadata_table: Option<String>,
  /// Shadow database URL.
  pub shadow_database_url: Option<String>,
  /// Advisory lock timeout.
  pub lock_timeout: Option<Duration>,
  /// Statement timeout.
  pub statement_timeout: Option<Duration>,
  /// Transaction policy.
  pub transaction: Option<TransactionMode>,
  /// TLS policy.
  pub tls: Option<TlsMode>,
  /// Application name.
  pub application_name: Option<String>,
  /// Search path.
  pub search_path: Option<String>,
}

/// Read-only environment abstraction used during configuration resolution.
pub trait EnvSource {
  /// Return the value of `key`, if it is set.
  fn var(&self, key: &str) -> Option<String>;
}

impl EnvSource for BTreeMap<String, String> {
  fn var(&self, key: &str) -> Option<String> {
    self.get(key).cloned()
  }
}

/// Reads variables from the current process environment.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessEnv;

impl EnvSource for ProcessEnv {
  fn var(&self, key: &str) -> Option<String> {
    std::env::var(key).ok()
  }
}

/// Empty environment used when only a file is available.
#[derive(Debug, Clone, Copy, Default)]
struct EmptyEnv;

impl EnvSource for EmptyEnv {
  fn var(&self, _: &str) -> Option<String> {
    None
  }
}

/// `midas.toml` layout.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MidasConfig {
  /// Project-level settings.
  #[serde(default)]
  pub project: ProjectConfig,
  /// Database settings.
  #[serde(default)]
  pub database: DatabaseConfig,
  /// Migration settings.
  #[serde(default)]
  pub migrations: MigrationsConfig,
  /// Objects midas is allowed to manage.
  #[serde(default)]
  pub managed: ManagedConfig,
}

/// `[project]` table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
  /// SQL dialect used by the schema DSL.
  pub dialect: Option<String>,
  /// Schema treated as the default by the DSL.
  pub default_schema: Option<String>,
  /// Schema DSL file name.
  pub schema: Option<String>,
  /// Directory containing schema DSL files.
  pub schema_dir: Option<String>,
}

/// `[database]` table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseConfig {
  /// Environment variable that holds the connection URL.
  pub url_env: Option<String>,
  /// Metadata schema.
  pub metadata_schema: Option<String>,
  /// Metadata table.
  pub metadata_table: Option<String>,
  /// Environment variable that holds the shadow database URL.
  pub shadow_url_env: Option<String>,
}

/// `[migrations]` table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MigrationsConfig {
  /// Directory containing migration files.
  pub dir: Option<PathBuf>,
  /// Advisory lock timeout in milliseconds.
  pub lock_timeout_ms: Option<u64>,
  /// Statement timeout in milliseconds. Zero disables it.
  pub statement_timeout_ms: Option<u64>,
  /// Transaction policy.
  pub transaction: Option<TransactionMode>,
}

/// `[managed]` table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedConfig {
  /// Schemas midas may inspect or modify.
  #[serde(default)]
  pub schemas: Vec<String>,
  /// Object names excluded from managed schemas.
  #[serde(default)]
  pub ignore_objects: Vec<String>,
}

impl MidasConfig {
  /// Parse a `midas.toml` file.
  pub fn from_path(path: &Path) -> Result<Self> {
    let raw = std::fs::read_to_string(path)?;
    Ok(toml::from_str(&raw)?)
  }
}

impl Config {
  /// Resolve configuration from the current environment and `midas.toml`.
  pub fn from_env() -> Result<Self> {
    let cwd = std::env::current_dir()?;
    Self::load(&ProcessEnv, &ConfigOverrides::default(), Some(&cwd))
  }

  /// Parse a `midas.toml` file and merge it over the defaults.
  pub fn from_file(path: &Path) -> Result<Self> {
    let file = MidasConfig::from_path(path)?;
    let mut config = Self::default();
    config.apply_file(&file);
    config.apply_env(&EmptyEnv, Some(&file))?;
    config.apply_overrides(&ConfigOverrides::default());
    config.validate()?;
    Ok(config)
  }

  /// Resolve configuration from injectable sources.
  ///
  /// `dir` is the directory searched for `midas.toml`. Passing `None` skips the
  /// file layer, which is useful for tests and embedders.
  pub fn load(env: &dyn EnvSource, overrides: &ConfigOverrides, dir: Option<&Path>) -> Result<Self> {
    let file = match dir {
      Some(dir) => {
        let path = dir.join(CONFIG_FILE);
        if path.is_file() {
          Some(MidasConfig::from_path(&path)?)
        } else {
          None
        }
      },
      None => None,
    };

    let mut config = Self::default();
    if let Some(file) = &file {
      config.apply_file(file);
    }
    config.apply_env(env, file.as_ref())?;
    config.apply_overrides(overrides);
    config.validate()?;
    Ok(config)
  }

  /// Ensure the configuration is internally consistent.
  pub fn validate(&self) -> Result<()> {
    if self.migrations_dir.as_os_str().is_empty() {
      return Err(Error::config("migrations directory must not be empty"));
    }
    if self.metadata_table.trim().is_empty() {
      return Err(Error::config("metadata table must not be empty"));
    }
    if self.application_name.trim().is_empty() {
      return Err(Error::config("application name must not be empty"));
    }
    if let Some(url) = &self.database_url {
      validate_database_url(url)?;
    }
    if let Some(url) = &self.shadow_database_url {
      validate_database_url(url)?;
    }
    Ok(())
  }

  /// Return the database URL with any password replaced by [`REDACTED`].
  pub fn redacted_database_url(&self) -> Option<String> {
    self.database_url.as_deref().map(redact_database_url)
  }

  /// Fail unless a database URL is configured.
  pub fn require_database_url(&self) -> Result<&str> {
    self
      .database_url
      .as_deref()
      .ok_or_else(|| Error::config("no database URL provided; pass --database or set DATABASE_URL"))
  }

  fn apply_file(&mut self, file: &MidasConfig) {
    if let Some(default_schema) = non_empty(file.project.default_schema.clone()) {
      self.search_path = Some(default_schema);
    }
    if let Some(metadata_schema) = non_empty(file.database.metadata_schema.clone()) {
      self.metadata_schema = Some(metadata_schema);
    }
    if let Some(metadata_table) = non_empty(file.database.metadata_table.clone()) {
      self.metadata_table = metadata_table;
    }
    if let Some(dir) = &file.migrations.dir {
      if !dir.as_os_str().is_empty() {
        self.migrations_dir = dir.clone();
      }
    }
    if let Some(ms) = file.migrations.lock_timeout_ms {
      self.lock_timeout = Duration::from_millis(ms);
    }
    if let Some(ms) = file.migrations.statement_timeout_ms {
      self.statement_timeout = (ms > 0).then(|| Duration::from_millis(ms));
    }
    if let Some(transaction) = file.migrations.transaction {
      self.transaction = transaction;
    }
  }

  fn apply_env(&mut self, env: &dyn EnvSource, file: Option<&MidasConfig>) -> Result<()> {
    if let Some(url) = first_env(
      env,
      &[
        "MIDAS_DATABASE_URL",
        "DATABASE_URL",
        "DATABASE_URI",
        "DB_URL",
        "DSN",
      ],
    )
    .or_else(|| file.and_then(|file| env_named(env, file.database.url_env.as_deref())))
    {
      self.database_url = Some(url);
    }

    if let Some(dir) = first_env(
      env,
      &[
        "MIDAS_MIGRATIONS_DIR",
        "MIGRATIONS_DIR",
        "MIGRATIONS_ROOT",
        "MIGRATIONS_PATH",
      ],
    ) {
      self.migrations_dir = PathBuf::from(dir);
    }

    if let Some(value) = first_env(env, &["MIDAS_METADATA_SCHEMA"]) {
      self.metadata_schema = Some(value);
    }
    if let Some(value) = first_env(env, &["MIDAS_METADATA_TABLE"]) {
      self.metadata_table = value;
    }
    if let Some(value) = first_env(env, &["MIDAS_SHADOW_DATABASE_URL"])
      .or_else(|| file.and_then(|file| env_named(env, file.database.shadow_url_env.as_deref())))
    {
      self.shadow_database_url = Some(value);
    }
    if let Some(value) = first_env(env, &["MIDAS_LOCK_TIMEOUT_MS"]) {
      self.lock_timeout = Duration::from_millis(parse_u64("MIDAS_LOCK_TIMEOUT_MS", &value)?);
    }
    if let Some(value) = first_env(env, &["MIDAS_STATEMENT_TIMEOUT_MS"]) {
      let ms = parse_u64("MIDAS_STATEMENT_TIMEOUT_MS", &value)?;
      self.statement_timeout = (ms > 0).then(|| Duration::from_millis(ms));
    }
    if let Some(value) = first_env(env, &["MIDAS_TRANSACTION"]) {
      self.transaction = parse_transaction(&value)?;
    }
    if let Some(value) = first_env(env, &["MIDAS_TLS"]) {
      self.tls = parse_tls(&value)?;
    }
    if let Some(value) = first_env(env, &["MIDAS_APPLICATION_NAME"]) {
      self.application_name = value;
    }
    if let Some(value) = first_env(env, &["MIDAS_SEARCH_PATH"]) {
      self.search_path = Some(value);
    }

    Ok(())
  }

  fn apply_overrides(&mut self, overrides: &ConfigOverrides) {
    if let Some(value) = non_empty(overrides.database_url.clone()) {
      self.database_url = Some(value);
    }
    if let Some(value) = &overrides.migrations_dir {
      self.migrations_dir = value.clone();
    }
    if let Some(value) = non_empty(overrides.metadata_schema.clone()) {
      self.metadata_schema = Some(value);
    }
    if let Some(value) = non_empty(overrides.metadata_table.clone()) {
      self.metadata_table = value;
    }
    if let Some(value) = non_empty(overrides.shadow_database_url.clone()) {
      self.shadow_database_url = Some(value);
    }
    if let Some(value) = overrides.lock_timeout {
      self.lock_timeout = value;
    }
    if let Some(value) = overrides.statement_timeout {
      self.statement_timeout = Some(value);
    }
    if let Some(value) = overrides.transaction {
      self.transaction = value;
    }
    if let Some(value) = overrides.tls {
      self.tls = value;
    }
    if let Some(value) = non_empty(overrides.application_name.clone()) {
      self.application_name = value;
    }
    if let Some(value) = non_empty(overrides.search_path.clone()) {
      self.search_path = Some(value);
    }
  }
}

fn non_empty(value: Option<String>) -> Option<String> {
  value.filter(|value| !value.trim().is_empty())
}

fn first_env(env: &dyn EnvSource, keys: &[&str]) -> Option<String> {
  keys.iter().find_map(|key| non_empty(env.var(key)))
}

fn env_named(env: &dyn EnvSource, key: Option<&str>) -> Option<String> {
  key.and_then(|key| non_empty(env.var(key)))
}

fn parse_u64(key: &str, value: &str) -> Result<u64> {
  value
    .parse::<u64>()
    .map_err(|_| Error::config(format!("{key} must be an unsigned integer, got `{value}`")))
}

fn parse_transaction(value: &str) -> Result<TransactionMode> {
  match value {
    "per-migration" | "per_migration" => Ok(TransactionMode::PerMigration),
    "none" => Ok(TransactionMode::None),
    other => Err(Error::config(format!(
      "MIDAS_TRANSACTION must be `per-migration` or `none`, got `{other}`"
    ))),
  }
}

fn parse_tls(value: &str) -> Result<TlsMode> {
  match value {
    "disable" | "disabled" | "none" => Ok(TlsMode::Disable),
    "prefer" => Ok(TlsMode::Prefer),
    "require" | "required" => Ok(TlsMode::Require),
    other => Err(Error::config(format!(
      "MIDAS_TLS must be `disable`, `prefer`, or `require`, got `{other}`"
    ))),
  }
}

fn validate_database_url(raw: &str) -> Result<()> {
  if raw.trim().is_empty() {
    return Err(Error::config("database URL must not be empty"));
  }

  // Local SQLite paths are accepted without a URL scheme.
  if raw.starts_with('.') || raw.starts_with('/') {
    return Ok(());
  }

  let url = Url::parse(raw).map_err(|error| Error::config(format!("invalid database URL: {error}")))?;
  let scheme = url.scheme().to_ascii_lowercase();
  if !ALLOWED_SCHEMES.contains(&scheme.as_str()) {
    return Err(Error::config(format!(
      "unsupported database scheme `{scheme}`; expected one of {}",
      ALLOWED_SCHEMES.join(", ")
    )));
  }

  Ok(())
}

fn redact_database_url(raw: &str) -> String {
  if raw.starts_with('.') || raw.starts_with('/') {
    return raw.to_string();
  }

  match Url::parse(raw) {
    Ok(mut url) => {
      if url.password().is_some() && url.set_password(Some(REDACTED)).is_err() {
        return raw.to_string();
      }
      url.to_string()
    },
    Err(_) => raw.to_string(),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn write_config(dir: &Path, contents: &str) {
    std::fs::write(dir.join(CONFIG_FILE), contents).expect("write config");
  }

  fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
      .iter()
      .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
      .collect()
  }

  #[test]
  fn defaults_are_stable() {
    let config = Config::load(&env(&[]), &ConfigOverrides::default(), None).expect("defaults");
    assert_eq!(config.migrations_dir, PathBuf::from("migrations"));
    assert_eq!(config.metadata_table, "__schema_migrations");
    assert_eq!(config.lock_timeout, DEFAULT_LOCK_TIMEOUT);
    assert_eq!(config.transaction, TransactionMode::PerMigration);
    assert_eq!(config.tls, TlsMode::Disable);
    assert_eq!(config.application_name, "midas");
    assert!(config.database_url.is_none());
  }

  #[test]
  fn precedence_is_cli_env_file_default() {
    let dir = tempfile::tempdir().expect("temp dir");
    write_config(
      dir.path(),
      r#"
[project]
default_schema = "file_schema"

[database]
url_env = "FILE_DATABASE_URL"
metadata_table = "file_table"

[migrations]
dir = "file_migrations"
lock_timeout_ms = 5000
transaction = "none"
"#,
    );

    // File layer only.
    let config = Config::load(&env(&[]), &ConfigOverrides::default(), Some(dir.path())).expect("file");
    assert_eq!(config.migrations_dir, PathBuf::from("file_migrations"));
    assert_eq!(config.metadata_table, "file_table");
    assert_eq!(config.search_path.as_deref(), Some("file_schema"));
    assert_eq!(config.lock_timeout, Duration::from_millis(5000));
    assert_eq!(config.transaction, TransactionMode::None);
    assert!(config.database_url.is_none());

    // Environment beats the file.
    let env_map = env(&[
      ("FILE_DATABASE_URL", "postgres://file_env@localhost/file"),
      ("MIDAS_MIGRATIONS_DIR", "env_migrations"),
      ("MIDAS_LOCK_TIMEOUT_MS", "2500"),
    ]);
    let config = Config::load(&env_map, &ConfigOverrides::default(), Some(dir.path())).expect("env");
    assert_eq!(
      config.database_url.as_deref(),
      Some("postgres://file_env@localhost/file")
    );
    assert_eq!(config.migrations_dir, PathBuf::from("env_migrations"));
    assert_eq!(config.lock_timeout, Duration::from_millis(2500));

    // CLI overrides beat the environment.
    let overrides = ConfigOverrides {
      database_url: Some("postgres://cli@localhost/cli".to_string()),
      migrations_dir: Some(PathBuf::from("cli_migrations")),
      ..ConfigOverrides::default()
    };
    let config = Config::load(&env_map, &overrides, Some(dir.path())).expect("overrides");
    assert_eq!(
      config.database_url.as_deref(),
      Some("postgres://cli@localhost/cli")
    );
    assert_eq!(config.migrations_dir, PathBuf::from("cli_migrations"));
  }

  #[test]
  fn unknown_fields_are_rejected_with_line_number() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(CONFIG_FILE);
    std::fs::write(&path, "[project]\ndialect = \"postgres\"\nbogus = true\n").expect("write config");

    let error = Config::from_file(&path).expect_err("unknown field must fail");
    let message = error.to_string();
    assert!(message.contains("unknown field `bogus`"), "{message}");
    assert!(message.contains("line 3"), "{message}");
  }

  #[test]
  fn passwords_are_redacted() {
    let config = Config {
      database_url: Some("postgres://user:secret@host/db".to_string()),
      ..Config::default()
    };
    assert_eq!(
      config.redacted_database_url().as_deref(),
      Some("postgres://user:***@host/db")
    );
  }

  #[test]
  fn redaction_keeps_urls_without_passwords() {
    let config = Config {
      database_url: Some("sqlite://./data.db3".to_string()),
      ..Config::default()
    };
    assert_eq!(
      config.redacted_database_url().as_deref(),
      Some("sqlite://./data.db3")
    );
  }

  #[test]
  fn invalid_schemes_are_rejected() {
    let config = Config {
      database_url: Some("oracle://localhost/db".to_string()),
      ..Config::default()
    };
    let error = config.validate().expect_err("invalid scheme must fail");
    assert!(matches!(error, Error::Config(_)));
    assert!(error.to_string().contains("unsupported database scheme `oracle`"));
  }

  #[test]
  fn local_sqlite_paths_are_accepted() {
    let config = Config {
      database_url: Some("./data.db3".to_string()),
      ..Config::default()
    };
    config.validate().expect("local path is valid");
  }

  #[test]
  fn require_database_url_reports_config_error() {
    let config = Config::default();
    let error = config.require_database_url().expect_err("url is required");
    assert!(matches!(error, Error::Config(_)));
  }

  #[test]
  fn env_values_are_validated() {
    let error = Config::load(
      &env(&[("MIDAS_LOCK_TIMEOUT_MS", "nope")]),
      &ConfigOverrides::default(),
      None,
    )
    .expect_err("invalid timeout must fail");
    assert!(error.to_string().contains("MIDAS_LOCK_TIMEOUT_MS"));
  }
}
