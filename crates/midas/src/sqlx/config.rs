//! `sqlx.toml` parsing.
//!
//! SQLx 0.9 stores migration settings in a TOML file with kebab-case keys.
//! Midas reads the migration subset so projects can adopt SQLx settings
//! without duplicating them in `midas.toml`. Fields Midas does not use are
//! ignored; unknown `[migrate]` fields produce a warning instead of an error so
//! newer SQLx releases do not break Midas.

use std::collections::{
  BTreeSet,
  HashSet,
};
use std::path::{
  Path,
  PathBuf,
};

use crate::{
  Error,
  Result,
};

/// Name of the SQLx configuration file.
pub const SQLX_CONFIG_FILE: &str = "sqlx.toml";

/// Default migration type requested by `sqlx migrate add`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SqlxMigrationType {
  /// Infer from the latest migration.
  #[default]
  Inferred,
  /// Create one-way migrations.
  Simple,
  /// Create reversible `.up.sql` / `.down.sql` pairs.
  Reversible,
}

/// Default version scheme requested by `sqlx migrate add`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SqlxVersioning {
  /// Infer from existing migrations.
  #[default]
  Inferred,
  /// Use UTC timestamps.
  Timestamp,
  /// Use consecutive integers.
  Sequential,
}

/// How migration directories treat `.sql` files that do not match the naming
/// convention.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UnrecognizedSqlFiles {
  /// Skip the file silently.
  #[default]
  Ignore,
  /// Skip the file and log a warning.
  Warn,
  /// Fail discovery.
  Error,
}

/// Migration settings read from `sqlx.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SqlxConfig {
  /// Directory containing migration files.
  pub migrations_dir: Option<PathBuf>,
  /// Schema-qualified metadata table name from `[migrate] table-name`.
  pub table_name: Option<String>,
  /// Schemas that must exist before the metadata table is created.
  pub create_schemas: BTreeSet<String>,
  /// Characters removed when computing SQLx-compatible checksums.
  pub ignored_chars: BTreeSet<char>,
  /// Policy for files that are not valid migrations.
  pub unrecognized_sql_files: UnrecognizedSqlFiles,
  /// Default migration type for `midas add`.
  pub default_migration_type: SqlxMigrationType,
  /// Default version scheme for `midas add`.
  pub default_versioning: SqlxVersioning,
}

impl SqlxConfig {
  /// Read `sqlx.toml` from a directory when it exists.
  pub fn load(dir: &Path) -> Result<Option<Self>> {
    let path = dir.join(SQLX_CONFIG_FILE);
    if !path.is_file() {
      return Ok(None);
    }
    Self::from_path(&path).map(Some)
  }

  /// Parse a `sqlx.toml` file.
  pub fn from_path(path: &Path) -> Result<Self> {
    let raw = std::fs::read_to_string(path)?;
    Self::from_str(&raw, path)
  }

  /// Parse `sqlx.toml` content.
  pub fn from_str(raw: &str, path: &Path) -> Result<Self> {
    let value: toml::Value = toml::from_str(raw)?;
    let Some(migrate) = value.get("migrate") else {
      return Ok(Self::default());
    };
    let table = migrate.as_table().ok_or_else(|| {
      Error::config(format!(
        "`migrate` in `{}` must be a table",
        path.to_string_lossy()
      ))
    })?;

    let mut config = Self::default();
    for (key, value) in table {
      match key.as_str() {
        "table-name" => config.table_name = Some(value_as_str(key, value, path)?.to_string()),
        "migrations-dir" => {
          config.migrations_dir = Some(PathBuf::from(value_as_str(key, value, path)?));
        },
        "create-schemas" => {
          config.create_schemas = value_as_string_set(key, value, path)?.into_iter().collect();
        },
        "ignored-chars" => {
          config.ignored_chars = value_as_chars(key, value, path)?.into_iter().collect();
        },
        "unrecognized-sql-files" => {
          config.unrecognized_sql_files = parse_unrecognized(value_as_str(key, value, path)?)?;
        },
        "defaults" => config.apply_defaults(value, path)?,
        other => {
          tracing::warn!(
            field = other,
            file = %path.to_string_lossy(),
            "ignoring unsupported sqlx.toml [migrate] field"
          );
        },
      }
    }

    Ok(config)
  }

  /// Resolve the configured metadata table into an optional schema and a
  /// table name. Quoted identifiers and schema qualification are understood.
  pub fn table_location(&self) -> Result<Option<(Option<String>, String)>> {
    self.table_name.as_deref().map(split_qualified_table).transpose()
  }

  fn apply_defaults(&mut self, value: &toml::Value, path: &Path) -> Result<()> {
    let table = value.as_table().ok_or_else(|| {
      Error::config(format!(
        "`migrate.defaults` in `{}` must be a table",
        path.to_string_lossy()
      ))
    })?;

    for (key, value) in table {
      match key.as_str() {
        "migration-type" => {
          self.default_migration_type = parse_migration_type(value_as_str(key, value, path)?)?;
        },
        "migration-versioning" => {
          self.default_versioning = parse_versioning(value_as_str(key, value, path)?)?;
        },
        other => {
          tracing::warn!(
            field = other,
            file = %path.to_string_lossy(),
            "ignoring unsupported sqlx.toml [migrate.defaults] field"
          );
        },
      }
    }

    Ok(())
  }
}

/// Split a possibly schema-qualified, possibly quoted identifier.
///
/// Examples: `_sqlx_migrations`, `foo._sqlx_migrations`,
/// `"Foo Schema"."My Table"`.
pub fn split_qualified_table(raw: &str) -> Result<(Option<String>, String)> {
  let raw = raw.trim();
  if raw.is_empty() {
    return Err(Error::config("metadata table name must not be empty"));
  }

  let mut parts = Vec::new();
  let mut current = String::new();
  let mut quoted = false;
  let mut in_quotes = false;
  let mut chars = raw.chars().peekable();

  while let Some(character) = chars.next() {
    match character {
      '"' if in_quotes => {
        if chars.peek() == Some(&'"') {
          chars.next();
          current.push('"');
        } else {
          in_quotes = false;
          quoted = true;
        }
      },
      '"' => in_quotes = true,
      '.' if !in_quotes => {
        if current.is_empty() {
          return Err(Error::config(format!("invalid metadata table name `{raw}`")));
        }
        parts.push((current, quoted));
        current = String::new();
        quoted = false;
      },
      _ => current.push(character),
    }
  }

  if in_quotes || current.is_empty() {
    return Err(Error::config(format!("invalid metadata table name `{raw}`")));
  }
  parts.push((current, quoted));

  if parts.len() > 2 {
    return Err(Error::config(format!(
      "metadata table name `{raw}` has more than one schema qualifier"
    )));
  }

  for (part, was_quoted) in &parts {
    if !was_quoted && part.trim() != part {
      return Err(Error::config(format!("invalid metadata table name `{raw}`")));
    }
    if part.trim().is_empty() {
      return Err(Error::config(format!("invalid metadata table name `{raw}`")));
    }
  }

  let mut parts = parts.into_iter().map(|(part, _)| part);
  match (parts.next(), parts.next()) {
    (Some(table), None) => Ok((None, table)),
    (Some(schema), Some(table)) => Ok((Some(schema), table)),
    _ => Err(Error::config(format!("invalid metadata table name `{raw}`"))),
  }
}

/// Render a metadata location the way it would appear in `sqlx.toml`.
pub fn qualified_table_name(schema: Option<&str>, table: &str) -> String {
  match schema {
    Some(schema) => format!("{schema}.{table}"),
    None => table.to_string(),
  }
}

fn value_as_str<'a>(key: &str, value: &'a toml::Value, path: &Path) -> Result<&'a str> {
  value.as_str().ok_or_else(|| {
    Error::config(format!(
      "`migrate.{key}` in `{}` must be a string",
      path.to_string_lossy()
    ))
  })
}

fn value_as_string_set(key: &str, value: &toml::Value, path: &Path) -> Result<HashSet<String>> {
  let values = value.as_array().ok_or_else(|| {
    Error::config(format!(
      "`migrate.{key}` in `{}` must be an array of strings",
      path.to_string_lossy()
    ))
  })?;

  values
    .iter()
    .map(|value| {
      value.as_str().map(ToString::to_string).ok_or_else(|| {
        Error::config(format!(
          "`migrate.{key}` in `{}` must be an array of strings",
          path.to_string_lossy()
        ))
      })
    })
    .collect()
}

fn value_as_chars(key: &str, value: &toml::Value, path: &Path) -> Result<Vec<char>> {
  let values = value.as_array().ok_or_else(|| {
    Error::config(format!(
      "`migrate.{key}` in `{}` must be an array of single-character strings",
      path.to_string_lossy()
    ))
  })?;

  values
    .iter()
    .map(|value| {
      let text = value.as_str().ok_or_else(|| {
        Error::config(format!(
          "`migrate.{key}` in `{}` must be an array of single-character strings",
          path.to_string_lossy()
        ))
      })?;
      let mut characters = text.chars();
      let character = characters.next().ok_or_else(|| {
        Error::config(format!(
          "`migrate.{key}` in `{}` must not contain empty strings",
          path.to_string_lossy()
        ))
      })?;
      if characters.next().is_some() {
        return Err(Error::config(format!(
          "`migrate.{key}` in `{}` must contain single characters, got `{text}`",
          path.to_string_lossy()
        )));
      }
      Ok(character)
    })
    .collect()
}

fn parse_migration_type(value: &str) -> Result<SqlxMigrationType> {
  match value {
    "inferred" => Ok(SqlxMigrationType::Inferred),
    "simple" => Ok(SqlxMigrationType::Simple),
    "reversible" => Ok(SqlxMigrationType::Reversible),
    other => Err(Error::config(format!(
      "`migrate.defaults.migration-type` must be `inferred`, `simple`, or `reversible`, got `{other}`"
    ))),
  }
}

fn parse_versioning(value: &str) -> Result<SqlxVersioning> {
  match value {
    "inferred" => Ok(SqlxVersioning::Inferred),
    "timestamp" => Ok(SqlxVersioning::Timestamp),
    "sequential" => Ok(SqlxVersioning::Sequential),
    other => Err(Error::config(format!(
      "`migrate.defaults.migration-versioning` must be `inferred`, `timestamp`, or `sequential`, got `{other}`"
    ))),
  }
}

fn parse_unrecognized(value: &str) -> Result<UnrecognizedSqlFiles> {
  match value {
    "ignore" => Ok(UnrecognizedSqlFiles::Ignore),
    "warn" => Ok(UnrecognizedSqlFiles::Warn),
    "error" => Ok(UnrecognizedSqlFiles::Error),
    other => Err(Error::config(format!(
      "`migrate.unrecognized-sql-files` must be `ignore`, `warn`, or `error`, got `{other}`"
    ))),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn parse(raw: &str) -> SqlxConfig {
    SqlxConfig::from_str(raw, Path::new("sqlx.toml")).expect("valid sqlx.toml")
  }

  #[test]
  fn committed_fixture_parses() {
    let config = SqlxConfig::from_str(
      include_str!("../../tests/fixtures/sqlx-config/sqlx.toml"),
      Path::new("sqlx.toml"),
    )
    .expect("fixture");
    assert_eq!(config.table_name.as_deref(), Some("audit._sqlx_migrations"));
    assert_eq!(config.migrations_dir.as_deref(), Some(Path::new("db/migrations")));
    assert!(config.create_schemas.contains("audit"));
    assert_eq!(config.ignored_chars.len(), 2);
    assert_eq!(config.unrecognized_sql_files, UnrecognizedSqlFiles::Warn);
    assert_eq!(config.default_migration_type, SqlxMigrationType::Reversible);
    assert_eq!(config.default_versioning, SqlxVersioning::Sequential);
  }

  #[test]
  fn migration_settings_are_parsed() {
    let config = parse(
      r#"
[migrate]
table-name = "foo._sqlx_migrations"
migrations-dir = "db/migrations"
create-schemas = ["foo", "bar"]
ignored-chars = ["\r", "\n"]

[migrate.defaults]
migration-type = "reversible"
migration-versioning = "sequential"
"#,
    );

    assert_eq!(config.table_name.as_deref(), Some("foo._sqlx_migrations"));
    assert_eq!(config.migrations_dir.as_deref(), Some(Path::new("db/migrations")));
    assert_eq!(
      config.create_schemas,
      ["foo".to_string(), "bar".to_string()].into_iter().collect()
    );
    assert!(config.ignored_chars.contains(&'\r'));
    assert!(config.ignored_chars.contains(&'\n'));
    assert_eq!(config.default_migration_type, SqlxMigrationType::Reversible);
    assert_eq!(config.default_versioning, SqlxVersioning::Sequential);
  }

  #[test]
  fn non_migrate_sections_are_ignored() {
    let config = parse(
      r#"
[common]
database-url-var = "APP_DATABASE_URL"

[macros]
foo = "bar"

[migrate]
table-name = "custom_migrations"
"#,
    );
    assert_eq!(config.table_name.as_deref(), Some("custom_migrations"));
  }

  #[test]
  fn unsupported_migrate_fields_do_not_fail() {
    let config = parse(
      r#"
[migrate]
table-name = "_sqlx_migrations"
future-field = true
"#,
    );
    assert_eq!(config.table_name.as_deref(), Some("_sqlx_migrations"));
  }

  #[test]
  fn qualified_names_understand_quotes() {
    assert_eq!(
      split_qualified_table("_sqlx_migrations").expect("plain"),
      (None, "_sqlx_migrations".to_string())
    );
    assert_eq!(
      split_qualified_table("foo._sqlx_migrations").expect("qualified"),
      (Some("foo".to_string()), "_sqlx_migrations".to_string())
    );
    assert_eq!(
      split_qualified_table(r#""Foo Schema"."My Table""#).expect("quoted"),
      (Some("Foo Schema".to_string()), "My Table".to_string())
    );
    assert_eq!(
      split_qualified_table(r#""a.b""#).expect("quoted dot"),
      (None, "a.b".to_string())
    );
  }

  #[test]
  fn invalid_qualified_names_are_rejected() {
    for name in ["", ".", "a.", ".b", "a.b.c", "\"unterminated"] {
      assert!(split_qualified_table(name).is_err(), "`{name}` must fail");
    }
  }

  #[test]
  fn invalid_values_are_rejected() {
    for raw in [
      "[migrate]\ntable-name = 1\n",
      "[migrate]\nignored-chars = [\"ab\"]\n",
      "[migrate]\nunrecognized-sql-files = \"nope\"\n",
      "[migrate.defaults]\nmigration-type = \"nope\"\n",
      "[migrate.defaults]\nmigration-versioning = \"nope\"\n",
    ] {
      let error = SqlxConfig::from_str(raw, Path::new("sqlx.toml")).expect_err("invalid value");
      assert!(matches!(error, Error::Config(_)));
    }
  }
}
