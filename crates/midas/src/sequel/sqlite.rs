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
  Locker,
  MigrationStore,
};
use crate::config::{
  Config,
  DEFAULT_LOCK_TIMEOUT,
  DEFAULT_METADATA_TABLE,
};
use crate::migration::{
  MigrationRecord,
  CHECKSUM_LEN,
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
    conn.busy_timeout(lock_timeout)?;
    let mut backend = Self {
      conn,
      file_path,
      metadata_table: metadata_table.to_string(),
      lock_timeout,
    };
    backend.ensure_table()?;
    Ok(backend)
  }

  /// Administrator handle that does not open the database.
  pub fn admin(file_url: &str) -> SqliteAdmin {
    SqliteAdmin::new(file_url)
  }

  /// Filesystem path backing this database.
  pub fn file_path(&self) -> &str {
    &self.file_path
  }

  fn table(&self) -> String {
    quote_ident_with(&self.metadata_table, '"')
  }

  fn upgrade_table(&mut self) -> Result<()> {
    let table = self.table();
    let mut statement = self
      .conn
      .prepare("select name, type from pragma_table_info(?1)")?;
    let columns = statement
      .query_map([&self.metadata_table], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
      })?
      .collect::<rusqlite::Result<Vec<_>>>()?;

    let has = |name: &str| columns.iter().any(|(column, _)| column == name);
    let compatible = has("version")
      && has("description")
      && has("installed_on")
      && has("success")
      && has("execution_time")
      && has("down_checksum")
      && columns
        .iter()
        .any(|(column, kind)| column == "checksum" && kind.eq_ignore_ascii_case("blob"));
    if compatible {
      return Ok(());
    }

    let version_column = if has("migration") && !has("version") {
      "migration"
    } else {
      "version"
    };
    let description = if has("description") {
      "coalesce(description, '')"
    } else {
      "''"
    };
    let success = if has("success") {
      "coalesce(success, 1)"
    } else {
      "1"
    };
    let execution_time = if has("execution_time") {
      "coalesce(execution_time, -1)".to_string()
    } else if has("execution_time_ms") {
      "coalesce(execution_time_ms, -1) * 1000000".to_string()
    } else {
      "-1".to_string()
    };

    let legacy = quote_ident_with(&format!("{}__legacy", self.metadata_table), '"');
    let zeros = "00".repeat(CHECKSUM_LEN);
    self.conn.execute_batch("BEGIN IMMEDIATE")?;
    let result = (|| -> Result<()> {
      self
        .conn
        .execute_batch(&format!("ALTER TABLE {table} RENAME TO {legacy}"))?;
      self.create_table()?;
      self.conn.execute_batch(&format!(
        "INSERT INTO {table} \
         (version, description, installed_on, success, checksum, execution_time, down_checksum) \
         SELECT {version_column}, {description}, \
           strftime('%Y-%m-%dT%H:%M:%fZ','now'), {success}, X'{zeros}', {execution_time}, NULL \
         FROM {legacy}"
      ))?;
      self.conn.execute_batch(&format!("DROP TABLE {legacy}"))?;
      Ok(())
    })();

    match result {
      Ok(()) => self.conn.execute_batch("COMMIT")?,
      Err(error) => {
        let _ = self.conn.execute_batch("ROLLBACK");
        return Err(error);
      },
    }

    Ok(())
  }

  fn create_table(&self) -> Result<()> {
    let table = self.table();
    self.conn.execute_batch(&format!(
      "CREATE TABLE IF NOT EXISTS {table} (
        version INTEGER NOT NULL PRIMARY KEY,
        description TEXT NOT NULL,
        installed_on TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
        success INTEGER NOT NULL,
        checksum BLOB NOT NULL,
        execution_time INTEGER NOT NULL,
        down_checksum BLOB
      )"
    ))?;
    Ok(())
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
    self.conn.execute_batch("BEGIN IMMEDIATE")?;
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
    let exists: bool = self.conn.query_row(
      "select exists(select 1 from sqlite_master where type = 'table' and name = ?1)",
      [&self.metadata_table],
      |row| row.get(0),
    )?;

    if !exists {
      self.create_table()?;
      return Ok(());
    }

    self.upgrade_table()
  }

  fn list_migrations(&mut self) -> Result<Vec<MigrationRecord>> {
    let table = self.table();
    let mut statement = self.conn.prepare(&format!(
      "SELECT version, description, installed_on, success, checksum, execution_time, down_checksum \
       FROM {table} ORDER BY version ASC"
    ))?;
    let rows = statement.query_map((), row_to_record)?;
    let mut migrations = Vec::new();
    for row in rows {
      migrations.push(row?);
    }
    Ok(migrations)
  }

  fn get_last(&mut self) -> Result<Option<MigrationRecord>> {
    let table = self.table();
    let mut statement = self.conn.prepare(&format!(
      "SELECT version, description, installed_on, success, checksum, execution_time, down_checksum \
       FROM {table} ORDER BY version DESC LIMIT 1"
    ))?;
    let migration = statement.query_row((), row_to_record).optional()?;
    Ok(migration)
  }

  fn insert(&mut self, record: &MigrationRecord) -> Result<()> {
    let table = self.table();
    self.conn.execute(
      &format!(
        "INSERT INTO {table} (version, description, success, checksum, execution_time, down_checksum) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
      ),
      params![
        record.version,
        record.description,
        record.success,
        record.checksum,
        record.execution_time,
        record.down_checksum
      ],
    )?;
    Ok(())
  }

  fn upsert(&mut self, record: &MigrationRecord) -> Result<()> {
    let table = self.table();
    self.conn.execute(
      &format!(
        "INSERT INTO {table} (version, description, success, checksum, execution_time, down_checksum) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT(version) DO UPDATE SET \
           description = excluded.description, \
           success = excluded.success, \
           checksum = excluded.checksum, \
           execution_time = excluded.execution_time, \
           down_checksum = excluded.down_checksum"
      ),
      params![
        record.version,
        record.description,
        record.success,
        record.checksum,
        record.execution_time,
        record.down_checksum
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

  fn set_success(&mut self, version: i64, success: bool) -> Result<()> {
    let table = self.table();
    self.conn.execute(
      &format!("UPDATE {table} SET success = ?2 WHERE version = ?1"),
      params![version, success],
    )?;
    Ok(())
  }

  fn repair(&mut self, record: &MigrationRecord) -> Result<()> {
    let table = self.table();
    self.conn.execute(
      &format!("UPDATE {table} SET description = ?2, checksum = ?3, down_checksum = ?4 WHERE version = ?1"),
      params![
        record.version,
        record.description,
        record.checksum,
        record.down_checksum
      ],
    )?;
    Ok(())
  }

  fn drop_table(&mut self) -> Result<()> {
    let table = self.table();
    self
      .conn
      .execute_batch(&format!("DROP TABLE IF EXISTS {table}"))?;
    Ok(())
  }
}

impl Locker for Sqlite {
  fn lock(&mut self) -> Result<()> {
    // The migration transaction itself uses `BEGIN IMMEDIATE`, which takes the
    // write lock before any SQL runs.
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
    remove_database_files(&self.file_path)
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

/// Administrator handle for SQLite databases that may not exist yet.
#[derive(Debug, Clone)]
pub struct SqliteAdmin {
  file_path: String,
}

impl SqliteAdmin {
  /// Build an administrator handle for a SQLite URL or path.
  pub fn new(file_url: &str) -> Self {
    Self {
      file_path: normalize_path(file_url),
    }
  }

  /// Path backing the database.
  pub fn file_path(&self) -> &str {
    &self.file_path
  }
}

impl DatabaseAdmin for SqliteAdmin {
  fn create_database(&mut self, _: &str) -> Result<()> {
    if self.file_path == ":memory:" {
      return Ok(());
    }
    let connection = Connection::open(&self.file_path)?;
    drop(connection);
    Ok(())
  }

  fn drop_database(&mut self, _: &str) -> Result<()> {
    remove_database_files(&self.file_path)
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

fn remove_database_files(file_path: &str) -> Result<()> {
  if file_path == ":memory:" {
    return Ok(());
  }
  for path in [
    file_path.to_string(),
    format!("{file_path}-wal"),
    format!("{file_path}-shm"),
  ] {
    match std::fs::remove_file(path) {
      Ok(()) => {},
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
      Err(error) => return Err(Error::Io(error)),
    }
  }
  Ok(())
}

fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<MigrationRecord> {
  Ok(MigrationRecord {
    version: row.get(0)?,
    description: row.get(1)?,
    installed_on: row.get(2)?,
    success: row.get(3)?,
    checksum: row.get(4)?,
    execution_time: row.get(5)?,
    down_checksum: row.get(6)?,
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
