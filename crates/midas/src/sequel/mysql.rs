//! MySQL backend.

use std::time::Duration;

use mysql::prelude::Queryable;
use mysql::{
  params,
  Pool,
  PooledConn,
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

const LOCK_NAME: &str = "midas-migrator";

type MigrationRow = (i64, String, String, i64, bool);

/// MySQL connection implementing every backend capability.
pub struct Mysql {
  conn: PooledConn,
  metadata_table: String,
  lock_timeout: Duration,
}

impl Mysql {
  /// Connect with default metadata settings.
  pub fn new(database_url: &str) -> Result<Self> {
    Self::connect(database_url, DEFAULT_METADATA_TABLE, DEFAULT_LOCK_TIMEOUT)
  }

  /// Connect using resolved configuration.
  pub fn with_config(config: &Config) -> Result<Self> {
    let database_url = config.require_database_url()?;
    Self::connect(database_url, &config.metadata_table, config.lock_timeout)
  }

  fn connect(database_url: &str, metadata_table: &str, lock_timeout: Duration) -> Result<Self> {
    let url = url::Url::parse(database_url)
      .map_err(|error| Error::config(format!("invalid database URL: {error}")))?;
    let _database_name = url
      .path_segments()
      .and_then(|mut segments| segments.next_back())
      .filter(|name| !name.is_empty())
      .ok_or_else(|| Error::config("database name not found in URL"))?;

    let pool = Pool::new(database_url)?;
    let conn = pool.get_conn()?;
    let mut backend = Self {
      conn,
      metadata_table: metadata_table.to_string(),
      lock_timeout,
    };
    backend.ensure_table()?;
    Ok(backend)
  }

  fn table(&self) -> String {
    quote_ident_with(&self.metadata_table, '`')
  }
}

impl Executor for Mysql {
  fn execute(&mut self, sql: &str) -> Result<()> {
    self.conn.query_drop(sql)?;
    Ok(())
  }

  fn execute_batch(&mut self, sql: &str) -> Result<()> {
    self.conn.query_drop(sql)?;
    Ok(())
  }

  fn begin(&mut self) -> Result<()> {
    self.conn.query_drop("START TRANSACTION")?;
    Ok(())
  }

  fn commit(&mut self) -> Result<()> {
    self.conn.query_drop("COMMIT")?;
    Ok(())
  }

  fn rollback(&mut self) -> Result<()> {
    self.conn.query_drop("ROLLBACK")?;
    Ok(())
  }
}

impl MigrationStore for Mysql {
  fn ensure_table(&mut self) -> Result<()> {
    let table = self.table();
    self.conn.query_drop(format!(
      "CREATE TABLE IF NOT EXISTS {table} (
        version BIGINT NOT NULL PRIMARY KEY,
        description TEXT NOT NULL,
        checksum TEXT NOT NULL,
        execution_time_ms BIGINT NOT NULL,
        success BOOLEAN NOT NULL
      )"
    ))?;
    Ok(())
  }

  fn list_applied(&mut self) -> Result<Vec<AppliedMigration>> {
    let table = self.table();
    let rows: Vec<MigrationRow> = self.conn.query(format!(
      "SELECT version, description, checksum, execution_time_ms, success \
       FROM {table} ORDER BY version ASC"
    ))?;
    Ok(rows.into_iter().map(row_to_migration).collect())
  }

  fn get_last(&mut self) -> Result<Option<AppliedMigration>> {
    let table = self.table();
    let row: Option<MigrationRow> = self.conn.query_first(format!(
      "SELECT version, description, checksum, execution_time_ms, success \
       FROM {table} ORDER BY version DESC LIMIT 1"
    ))?;
    Ok(row.map(row_to_migration))
  }

  fn insert(&mut self, migration: &AppliedMigration) -> Result<()> {
    let table = self.table();
    self.conn.exec_drop(
      format!(
        "INSERT INTO {table} (version, description, checksum, execution_time_ms, success) \
         VALUES (:version, :description, :checksum, :execution_time_ms, :success)"
      ),
      params! {
        "version" => migration.version,
        "description" => migration.description.as_str(),
        "checksum" => migration.checksum.as_str(),
        "execution_time_ms" => migration.execution_time_ms,
        "success" => migration.success,
      },
    )?;
    Ok(())
  }

  fn delete(&mut self, version: i64) -> Result<()> {
    let table = self.table();
    self.conn.exec_drop(
      format!("DELETE FROM {table} WHERE version = :version"),
      params! { "version" => version },
    )?;
    Ok(())
  }

  fn mark_failed(&mut self, version: i64) -> Result<()> {
    let table = self.table();
    self.conn.exec_drop(
      format!("UPDATE {table} SET success = FALSE WHERE version = :version"),
      params! { "version" => version },
    )?;
    Ok(())
  }

  fn repair_checksum(&mut self, version: i64, checksum: &str) -> Result<()> {
    let table = self.table();
    self.conn.exec_drop(
      format!("UPDATE {table} SET checksum = :checksum WHERE version = :version"),
      params! { "version" => version, "checksum" => checksum },
    )?;
    Ok(())
  }
}

impl Locker for Mysql {
  fn lock(&mut self) -> Result<()> {
    // MySQL wants whole seconds; round up so sub-second timeouts still wait.
    let timeout = self
      .lock_timeout
      .as_secs()
      .saturating_add(u64::from(self.lock_timeout.subsec_nanos() > 0))
      .min(u64::from(u32::MAX)) as u32;
    let acquired: Option<Option<i64>> = self.conn.exec_first(
      "SELECT GET_LOCK(:name, :timeout)",
      params! { "name" => LOCK_NAME, "timeout" => timeout },
    )?;
    match acquired.flatten() {
      Some(1) => Ok(()),
      _ => Err(Error::LockTimeout { backend: "mysql" }),
    }
  }

  fn unlock(&mut self) -> Result<()> {
    let _: Option<Option<i64>> = self
      .conn
      .exec_first("SELECT RELEASE_LOCK(:name)", params! { "name" => LOCK_NAME })?;
    Ok(())
  }

  fn lock_timeout(&self) -> Duration {
    self.lock_timeout
  }
}

impl Dialect for Mysql {
  fn quote_ident(&self, ident: &str) -> String {
    quote_ident_with(ident, '`')
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
      Feature::AdvisoryLocks | Feature::AlterColumnType | Feature::MultiStatementBatch
    )
  }
}

impl Introspector for Mysql {
  fn introspect_schema(&mut self, _: &str) -> Result<SchemaSnapshot> {
    Err(Error::unsupported_feature("mysql", "introspection"))
  }

  fn list_schemas(&mut self) -> Result<Vec<String>> {
    Err(Error::unsupported_feature("mysql", "introspection"))
  }
}

impl DatabaseAdmin for Mysql {
  fn create_database(&mut self, name: &str) -> Result<()> {
    let name = quote_ident_with(name, '`');
    self.conn.query_drop(format!("CREATE DATABASE {name}"))?;
    Ok(())
  }

  fn drop_database(&mut self, name: &str) -> Result<()> {
    let name = quote_ident_with(name, '`');
    self.conn.query_drop(format!("DROP DATABASE IF EXISTS {name}"))?;
    Ok(())
  }

  fn database_exists(&mut self, name: &str) -> Result<bool> {
    let exists: Option<i64> = self.conn.exec_first(
      "SELECT EXISTS(SELECT 1 FROM information_schema.schemata WHERE schema_name = :name)",
      params! { "name" => name },
    )?;
    Ok(exists.unwrap_or(0) != 0)
  }

  fn force_drop_database(&mut self, name: &str) -> Result<()> {
    self.drop_database(name)
  }
}

fn row_to_migration(row: MigrationRow) -> AppliedMigration {
  AppliedMigration {
    version: row.0,
    description: row.1,
    checksum: row.2,
    execution_time_ms: row.3,
    success: row.4,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn rows_map_to_applied_migrations() {
    let migration = row_to_migration((3, "init".into(), "abc".into(), 12, true));
    assert_eq!(migration.version, 3);
    assert_eq!(migration.description, "init");
    assert_eq!(migration.checksum, "abc");
    assert_eq!(migration.execution_time_ms, 12);
    assert!(migration.success);
  }

  #[test]
  fn quoting_matches_mysql_rules() {
    assert_eq!(quote_ident_with("a`b", '`'), "`a``b`");
    assert_eq!(quote_literal_with("a'b", '\''), "'a''b'");
  }
}
