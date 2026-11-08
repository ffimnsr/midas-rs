//! PostgreSQL backend.

use std::time::Duration;

use postgres::{
  Client,
  NoTls,
};
use url::Url;

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

const DEFAULT_METADATA_SCHEMA: &str = "midas";

/// PostgreSQL connection implementing every backend capability.
pub struct Postgres {
  client: Client,
  metadata_schema: String,
  metadata_table: String,
  lock_timeout: Duration,
  lock_key: i64,
}

impl Postgres {
  /// Connect with default metadata settings.
  pub fn new(database_url: &str) -> Result<Self> {
    Self::connect(
      database_url,
      DEFAULT_METADATA_SCHEMA,
      DEFAULT_METADATA_TABLE,
      DEFAULT_LOCK_TIMEOUT,
    )
  }

  /// Connect using resolved configuration.
  pub fn with_config(config: &Config) -> Result<Self> {
    let database_url = config.require_database_url()?;
    let metadata_schema = config
      .metadata_schema
      .clone()
      .unwrap_or_else(|| DEFAULT_METADATA_SCHEMA.to_string());
    Self::connect(
      database_url,
      &metadata_schema,
      &config.metadata_table,
      config.lock_timeout,
    )
  }

  fn connect(
    database_url: &str,
    metadata_schema: &str,
    metadata_table: &str,
    lock_timeout: Duration,
  ) -> Result<Self> {
    let url =
      Url::parse(database_url).map_err(|error| Error::config(format!("invalid database URL: {error}")))?;
    let _database_name = url
      .path_segments()
      .and_then(|mut segments| segments.next_back())
      .filter(|name| !name.is_empty())
      .ok_or_else(|| Error::config("database name not found in URL"))?;

    let client = Client::connect(database_url, NoTls)?;
    let mut backend = Self {
      client,
      metadata_schema: metadata_schema.to_string(),
      metadata_table: metadata_table.to_string(),
      lock_timeout,
      lock_key: lock_key(),
    };
    backend.ensure_table()?;
    Ok(backend)
  }

  fn qualified_table(&self) -> String {
    format!(
      "{}.{}",
      quote_ident_with(&self.metadata_schema, '"'),
      quote_ident_with(&self.metadata_table, '"')
    )
  }
}

impl Executor for Postgres {
  fn execute(&mut self, sql: &str) -> Result<()> {
    self.client.batch_execute(sql)?;
    Ok(())
  }

  fn execute_batch(&mut self, sql: &str) -> Result<()> {
    self.client.batch_execute(sql)?;
    Ok(())
  }

  fn begin(&mut self) -> Result<()> {
    self.client.batch_execute("BEGIN")?;
    Ok(())
  }

  fn commit(&mut self) -> Result<()> {
    self.client.batch_execute("COMMIT")?;
    Ok(())
  }

  fn rollback(&mut self) -> Result<()> {
    self.client.batch_execute("ROLLBACK")?;
    Ok(())
  }
}

impl MigrationStore for Postgres {
  fn ensure_table(&mut self) -> Result<()> {
    let schema = quote_ident_with(&self.metadata_schema, '"');
    let table = self.qualified_table();
    self
      .client
      .batch_execute(&format!("create schema if not exists {schema}"))?;
    self.client.batch_execute(&format!(
      "create table if not exists {table} (
        version bigint primary key,
        description text not null default '',
        checksum text not null default '',
        execution_time_ms bigint not null default 0,
        success boolean not null default true
      )"
    ))?;
    Ok(())
  }

  fn list_applied(&mut self) -> Result<Vec<AppliedMigration>> {
    let table = self.qualified_table();
    let rows = self.client.query(
      &format!(
        "select version, description, checksum, execution_time_ms, success \
         from {table} order by version asc"
      ),
      &[],
    )?;

    rows
      .iter()
      .map(|row| {
        Ok(AppliedMigration {
          version: row.get(0),
          description: row.get(1),
          checksum: row.get(2),
          execution_time_ms: row.get(3),
          success: row.get(4),
        })
      })
      .collect()
  }

  fn get_last(&mut self) -> Result<Option<AppliedMigration>> {
    let table = self.qualified_table();
    let rows = self.client.query(
      &format!(
        "select version, description, checksum, execution_time_ms, success \
         from {table} order by version desc limit 1"
      ),
      &[],
    )?;

    Ok(rows.first().map(|row| AppliedMigration {
      version: row.get(0),
      description: row.get(1),
      checksum: row.get(2),
      execution_time_ms: row.get(3),
      success: row.get(4),
    }))
  }

  fn insert(&mut self, migration: &AppliedMigration) -> Result<()> {
    let table = self.qualified_table();
    self.client.execute(
      &format!(
        "insert into {table} (version, description, checksum, execution_time_ms, success) \
         values ($1, $2, $3, $4, $5)"
      ),
      &[
        &migration.version,
        &migration.description,
        &migration.checksum,
        &migration.execution_time_ms,
        &migration.success,
      ],
    )?;
    Ok(())
  }

  fn delete(&mut self, version: i64) -> Result<()> {
    let table = self.qualified_table();
    self
      .client
      .execute(&format!("delete from {table} where version = $1"), &[&version])?;
    Ok(())
  }

  fn mark_failed(&mut self, version: i64) -> Result<()> {
    let table = self.qualified_table();
    self.client.execute(
      &format!("update {table} set success = false where version = $1"),
      &[&version],
    )?;
    Ok(())
  }

  fn repair_checksum(&mut self, version: i64, checksum: &str) -> Result<()> {
    let table = self.qualified_table();
    self.client.execute(
      &format!("update {table} set checksum = $2 where version = $1"),
      &[&version, &checksum],
    )?;
    Ok(())
  }
}

impl Locker for Postgres {
  fn lock(&mut self) -> Result<()> {
    let row = self
      .client
      .query_one("select pg_try_advisory_lock($1)", &[&self.lock_key])?;
    let acquired: bool = row.get(0);
    if acquired {
      Ok(())
    } else {
      Err(Error::LockTimeout { backend: "postgres" })
    }
  }

  fn unlock(&mut self) -> Result<()> {
    self
      .client
      .execute("select pg_advisory_unlock($1)", &[&self.lock_key])?;
    Ok(())
  }

  fn lock_timeout(&self) -> Duration {
    self.lock_timeout
  }
}

impl Dialect for Postgres {
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
      Feature::Schemas
        | Feature::TransactionalDdl
        | Feature::AdvisoryLocks
        | Feature::AlterColumnType
        | Feature::MultiStatementBatch
        | Feature::ReturningClause
    )
  }
}

impl Introspector for Postgres {
  fn introspect_schema(&mut self, _: &str) -> Result<SchemaSnapshot> {
    Err(Error::unsupported_feature("postgres", "introspection"))
  }

  fn list_schemas(&mut self) -> Result<Vec<String>> {
    Err(Error::unsupported_feature("postgres", "introspection"))
  }
}

impl DatabaseAdmin for Postgres {
  fn create_database(&mut self, name: &str) -> Result<()> {
    let name = quote_ident_with(name, '"');
    self.client.batch_execute(&format!("create database {name}"))?;
    Ok(())
  }

  fn drop_database(&mut self, name: &str) -> Result<()> {
    let name = quote_ident_with(name, '"');
    self
      .client
      .batch_execute(&format!("drop database if exists {name}"))?;
    Ok(())
  }

  fn database_exists(&mut self, name: &str) -> Result<bool> {
    let row = self.client.query_one(
      "select exists(select 1 from pg_database where datname = $1)",
      &[&name],
    )?;
    Ok(row.get(0))
  }

  fn force_drop_database(&mut self, name: &str) -> Result<()> {
    self.client.execute(
      "select pg_terminate_backend(pid) from pg_stat_activity \
       where datname = $1 and pid <> pg_backend_pid()",
      &[&name],
    )?;
    self.drop_database(name)
  }
}

fn lock_key() -> i64 {
  i64::from(crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC).checksum(b"midas-migrator"))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn lock_key_is_stable() {
    assert_eq!(lock_key(), lock_key());
    assert!(lock_key() > 0);
  }

  #[test]
  fn quoting_matches_postgres_rules() {
    assert_eq!(quote_ident_with("a\"b", '"'), "\"a\"\"b\"");
    assert_eq!(quote_literal_with("a'b", '\''), "'a''b'");
  }
}
