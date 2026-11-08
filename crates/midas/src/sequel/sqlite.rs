//! SQLite backend.

use std::path::Path;
use std::time::Duration;

use rusqlite::{
  params,
  Connection,
  OptionalExtension,
};

use super::admin::DatabaseAdmin;
use super::dialect::{
  emit_literal_with,
  quote_ident_with,
  quote_literal_with,
  Dialect,
  Feature,
  SqlLiteral,
};
use super::executor::Executor;
use super::introspect::{
  Introspector,
  SchemaSnapshot,
};
use super::store::{
  AppliedMigration,
  Locker,
  MigrationStore,
};
use crate::config::{
  Config,
  DEFAULT_LOCK_TIMEOUT,
  DEFAULT_METADATA_TABLE,
};
use crate::{
  Error,
  Result,
};

/// SQLite connection implementing every backend capability.
pub struct Sqlite {
  conn: Connection,
  file_path: String,
  metadata_table: String,
  lock_timeout: Duration,
}

impl Sqlite {
  /// Open a database with default metadata settings.
  pub fn new(file_url: &str) -> Result<Self> {
    Self::connect(file_url, DEFAULT_METADATA_TABLE, DEFAULT_LOCK_TIMEOUT)
  }

  /// Open a database using resolved configuration.
  pub fn with_config(config: &Config, database_url: &str) -> Result<Self> {
    Self::connect(database_url, &config.metadata_table, config.lock_timeout)
  }

  fn connect(file_url: &str, metadata_table: &str, lock_timeout: Duration) -> Result<Self> {
    let file_path = normalize_path(file_url);
    let conn = Connection::open(&file_path)?;
    let mut backend = Self {
      conn,
      file_path,
      metadata_table: metadata_table.to_string(),
      lock_timeout,
    };
    backend.ensure_table()?;
    Ok(backend)
  }

  /// Filesystem path backing this database.
  pub fn file_path(&self) -> &str {
    &self.file_path
  }

  fn table(&self) -> String {
    quote_ident_with(&self.metadata_table, '"')
  }
}

impl Executor for Sqlite {
  fn execute(&mut self, sql: &str) -> Result<()> {
    self.conn.execute_batch(sql)?;
    Ok(())
  }

  fn execute_batch(&mut self, sql: &str) -> Result<()> {
    self.conn.execute_batch(sql)?;
    Ok(())
  }

  fn begin(&mut self) -> Result<()> {
    self.conn.execute_batch("BEGIN")?;
    Ok(())
  }

  fn commit(&mut self) -> Result<()> {
    self.conn.execute_batch("COMMIT")?;
    Ok(())
  }

  fn rollback(&mut self) -> Result<()> {
    self.conn.execute_batch("ROLLBACK")?;
    Ok(())
  }
}

impl MigrationStore for Sqlite {
  fn ensure_table(&mut self) -> Result<()> {
    let table = self.table();
    self.conn.execute_batch(&format!(
      "CREATE TABLE IF NOT EXISTS {table} (
        version INTEGER NOT NULL PRIMARY KEY,
        description TEXT NOT NULL,
        checksum TEXT NOT NULL,
        execution_time_ms INTEGER NOT NULL,
        success BOOLEAN NOT NULL
      )"
    ))?;
    Ok(())
  }

  fn list_applied(&mut self) -> Result<Vec<AppliedMigration>> {
    let table = self.table();
    let mut statement = self.conn.prepare(&format!(
      "SELECT version, description, checksum, execution_time_ms, success \
       FROM {table} ORDER BY version ASC"
    ))?;
    let rows = statement.query_map((), row_to_migration)?;
    let mut migrations = Vec::new();
    for row in rows {
      migrations.push(row?);
    }
    Ok(migrations)
  }

  fn get_last(&mut self) -> Result<Option<AppliedMigration>> {
    let table = self.table();
    let mut statement = self.conn.prepare(&format!(
      "SELECT version, description, checksum, execution_time_ms, success \
       FROM {table} ORDER BY version DESC LIMIT 1"
    ))?;
    let migration = statement.query_row((), row_to_migration).optional()?;
    Ok(migration)
  }

  fn insert(&mut self, migration: &AppliedMigration) -> Result<()> {
    let table = self.table();
    self.conn.execute(
      &format!(
        "INSERT INTO {table} (version, description, checksum, execution_time_ms, success) \
         VALUES (?1, ?2, ?3, ?4, ?5)"
      ),
      params![
        migration.version,
        migration.description,
        migration.checksum,
        migration.execution_time_ms,
        migration.success
      ],
    )?;
    Ok(())
  }

  fn delete(&mut self, version: i64) -> Result<()> {
    let table = self.table();
    self.conn.execute(
      &format!("DELETE FROM {table} WHERE version = ?1"),
      params![version],
    )?;
    Ok(())
  }

  fn mark_failed(&mut self, version: i64) -> Result<()> {
    let table = self.table();
    self.conn.execute(
      &format!("UPDATE {table} SET success = 0 WHERE version = ?1"),
      params![version],
    )?;
    Ok(())
  }

  fn repair_checksum(&mut self, version: i64, checksum: &str) -> Result<()> {
    let table = self.table();
    self.conn.execute(
      &format!("UPDATE {table} SET checksum = ?2 WHERE version = ?1"),
      params![version, checksum],
    )?;
    Ok(())
  }
}

impl Locker for Sqlite {
  fn lock(&mut self) -> Result<()> {
    Ok(())
  }

  fn unlock(&mut self) -> Result<()> {
    Ok(())
  }

  fn lock_timeout(&self) -> Duration {
    self.lock_timeout
  }
}

impl Dialect for Sqlite {
  fn quote_ident(&self, ident: &str) -> String {
    quote_ident_with(ident, '"')
  }

  fn quote_literal(&self, value: &str) -> String {
    quote_literal_with(value, '\'')
  }

  fn emit_literal(&self, value: &SqlLiteral<'_>) -> String {
    emit_literal_with(value, '\'')
  }

  fn supports(&self, feature: Feature) -> bool {
    matches!(
      feature,
      Feature::TransactionalDdl | Feature::MultiStatementBatch | Feature::ReturningClause
    )
  }
}

impl Introspector for Sqlite {
  fn introspect_schema(&mut self, _: &str) -> Result<SchemaSnapshot> {
    Err(Error::unsupported_feature("sqlite", "introspection"))
  }

  fn list_schemas(&mut self) -> Result<Vec<String>> {
    Err(Error::unsupported_feature("sqlite", "introspection"))
  }
}

impl DatabaseAdmin for Sqlite {
  fn create_database(&mut self, _: &str) -> Result<()> {
    // Opening the connection already created the file when needed.
    Ok(())
  }

  fn drop_database(&mut self, _: &str) -> Result<()> {
    if self.file_path == ":memory:" {
      return Ok(());
    }
    match std::fs::remove_file(&self.file_path) {
      Ok(()) => Ok(()),
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
      Err(error) => Err(Error::Io(error)),
    }
  }

  fn database_exists(&mut self, _: &str) -> Result<bool> {
    if self.file_path == ":memory:" {
      return Ok(true);
    }
    Ok(Path::new(&self.file_path).exists())
  }

  fn force_drop_database(&mut self, name: &str) -> Result<()> {
    self.drop_database(name)
  }
}

fn row_to_migration(row: &rusqlite::Row<'_>) -> rusqlite::Result<AppliedMigration> {
  Ok(AppliedMigration {
    version: row.get(0)?,
    description: row.get(1)?,
    checksum: row.get(2)?,
    execution_time_ms: row.get(3)?,
    success: row.get(4)?,
  })
}

fn strip_scheme(raw: &str) -> &str {
  for prefix in [
    "file://",
    "sqlite://",
    "sqlite3://",
    "file:",
    "sqlite:",
    "sqlite3:",
  ] {
    if let Some(rest) = raw.strip_prefix(prefix) {
      return rest;
    }
  }
  raw
}

fn normalize_path(raw: &str) -> String {
  let rest = strip_scheme(raw);
  if rest.is_empty() || rest == ":memory:" {
    return ":memory:".to_string();
  }
  if rest.starts_with('/') || rest.starts_with("./") || rest.starts_with("../") {
    rest.to_string()
  } else {
    format!("./{rest}")
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn urls_are_normalized_to_paths() {
    assert_eq!(normalize_path("file:///data.db3"), "/data.db3");
    assert_eq!(normalize_path("file://./data.db3"), "./data.db3");
    assert_eq!(normalize_path("sqlite://./data.db3"), "./data.db3");
    assert_eq!(normalize_path("sqlite:data.db3"), "./data.db3");
    assert_eq!(normalize_path("data.db3"), "./data.db3");
    assert_eq!(normalize_path(":memory:"), ":memory:");
  }

  #[test]
  fn quoting_matches_sqlite_rules() {
    assert_eq!(quote_ident_with("a\"b", '"'), "\"a\"\"b\"");
    assert_eq!(quote_literal_with("a'b", '\''), "'a''b'");
  }
}
