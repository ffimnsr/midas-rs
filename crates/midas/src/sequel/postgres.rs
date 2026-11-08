//! PostgreSQL backend.

use std::collections::BTreeMap;
use std::time::Duration;

use postgres::error::SqlState;
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
  Locker,
  MigrationStore,
};
use super::DatabaseEngine;
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

const DEFAULT_METADATA_SCHEMA: &str = "midas";

/// URL schemes that identify Postgres-wire compatible engines.
const POSTGRES_SCHEME_ALIASES: &[&str] = &["cockroach", "cockroachdb"];

/// Engine flavor behind a Postgres-wire connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PostgresFlavor {
  Postgres,
  CockroachDb,
}

impl PostgresFlavor {
  fn supports(self, feature: Feature) -> bool {
    match feature {
      Feature::Schemas
      | Feature::TransactionalDdl
      | Feature::AlterColumnType
      | Feature::MultiStatementBatch
      | Feature::ReturningClause => true,
      // CockroachDB does not implement session-level advisory locks.
      Feature::AdvisoryLocks => self == Self::Postgres,
    }
  }
}

/// PostgreSQL connection implementing every backend capability.
pub struct Postgres {
  client: Client,
  database_url: String,
  metadata_schema: String,
  metadata_table: String,
  lock_timeout: Duration,
  lock_key: i64,
  flavor: PostgresFlavor,
}

impl Postgres {
  /// Connect with default metadata settings.
  pub fn new(database_url: &str) -> Result<Self> {
    Self::connect(
      database_url,
      DEFAULT_METADATA_SCHEMA,
      DEFAULT_METADATA_TABLE,
      DEFAULT_LOCK_TIMEOUT,
      true,
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
      true,
    )
  }

  /// Connect to the maintenance database without touching the target database.
  pub fn admin(config: &Config) -> Result<Self> {
    let database_url = config.require_database_url()?;
    let metadata_schema = config
      .metadata_schema
      .clone()
      .unwrap_or_else(|| DEFAULT_METADATA_SCHEMA.to_string());
    let normalized = normalize_database_url(database_url)?;
    let maintenance = maintenance_url(&normalized)?;
    Self::connect(
      &maintenance,
      &metadata_schema,
      &config.metadata_table,
      config.lock_timeout,
      false,
    )
  }

  fn connect(
    database_url: &str,
    metadata_schema: &str,
    metadata_table: &str,
    lock_timeout: Duration,
    bootstrap: bool,
  ) -> Result<Self> {
    let connection_url = normalize_database_url(database_url)?;
    let url =
      Url::parse(&connection_url).map_err(|error| Error::config(format!("invalid database URL: {error}")))?;
    let database_name = database_name(&url)?;
    let flavor = flavor_for_engine(DatabaseEngine::from_scheme(url.scheme())?)?;
    let client = Client::connect(&connection_url, NoTls)?;
    let mut backend = Self {
      client,
      database_url: connection_url,
      metadata_schema: metadata_schema.to_string(),
      metadata_table: metadata_table.to_string(),
      lock_timeout,
      lock_key: advisory_lock_key(&database_name),
      flavor,
    };
    if bootstrap {
      backend.ensure_table()?;
    }
    Ok(backend)
  }

  fn qualified_table(&self) -> String {
    format!(
      "{}.{}",
      quote_ident_with(&self.metadata_schema, '"'),
      quote_ident_with(&self.metadata_table, '"')
    )
  }

  fn admin_client_for(&self, target: &str) -> Result<Client> {
    let url = maintenance_url_for(&self.database_url, target)?;
    Ok(Client::connect(&url, NoTls)?)
  }

  fn upgrade_table(&mut self) -> Result<()> {
    let table = self.qualified_table();
    let rows = self.client.query(
      "select column_name, data_type from information_schema.columns \
       where table_schema = $1 and table_name = $2",
      &[&self.metadata_schema, &self.metadata_table],
    )?;
    let columns = rows
      .iter()
      .map(|row| (row.get::<_, String>(0), row.get::<_, String>(1)))
      .collect::<BTreeMap<_, _>>();

    if columns.contains_key("migration") && !columns.contains_key("version") {
      self
        .client
        .batch_execute(&format!("alter table {table} rename column migration to version"))?;
    }

    let mut statements = Vec::new();
    if !columns.contains_key("description") {
      statements.push(format!(
        "alter table {table} add column description text not null default ''"
      ));
    }
    if !columns.contains_key("installed_on") {
      statements.push(format!(
        "alter table {table} add column installed_on timestamptz not null default now()"
      ));
    }
    if !columns.contains_key("success") {
      statements.push(format!(
        "alter table {table} add column success boolean not null default true"
      ));
    }
    if !columns.contains_key("checksum") {
      statements.push(format!(
        "alter table {table} add column checksum bytea not null default decode(repeat('00', {CHECKSUM_LEN}), 'hex')"
      ));
    }
    if !columns.contains_key("execution_time") {
      statements.push(format!(
        "alter table {table} add column execution_time bigint not null default -1"
      ));
    }
    if !columns.contains_key("down_checksum") {
      statements.push(format!("alter table {table} add column down_checksum bytea"));
    }

    for statement in statements {
      self.client.batch_execute(&statement)?;
    }

    if columns.get("checksum").is_some_and(|kind| kind == "text") {
      self.client.batch_execute(&format!(
        "alter table {table} alter column checksum type bytea using \
         (case when checksum ~ '^[0-9a-fA-F]+$' then decode(checksum, 'hex') \
         else decode(repeat('00', {CHECKSUM_LEN}), 'hex') end)"
      ))?;
    }

    if columns.contains_key("execution_time_ms") && !columns.contains_key("execution_time") {
      self.client.batch_execute(&format!(
        "update {table} set execution_time = coalesce(execution_time_ms, -1) * 1000000"
      ))?;
      self
        .client
        .batch_execute(&format!("alter table {table} drop column execution_time_ms"))?;
    }

    self.client.batch_execute(&format!(
      "update {table} set checksum = decode(repeat('00', {CHECKSUM_LEN}), 'hex') \
       where checksum is null or octet_length(checksum) <> {CHECKSUM_LEN}; \
       update {table} set down_checksum = null \
       where octet_length(down_checksum) <> {CHECKSUM_LEN}"
    ))?;

    Ok(())
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

    let exists: bool = self
      .client
      .query_one(
        "select exists(select 1 from information_schema.tables \
         where table_schema = $1 and table_name = $2)",
        &[&self.metadata_schema, &self.metadata_table],
      )?
      .get(0);

    if !exists {
      self.client.batch_execute(&format!(
        "create table {table} (
          version bigint primary key,
          description text not null,
          installed_on timestamptz not null default now(),
          success boolean not null,
          checksum bytea not null,
          execution_time bigint not null,
          down_checksum bytea
        )"
      ))?;
      return Ok(());
    }

    self.upgrade_table()
  }

  fn list_migrations(&mut self) -> Result<Vec<MigrationRecord>> {
    let table = self.qualified_table();
    let rows = self.client.query(
      &format!(
        "select version, description, installed_on::text, success, checksum, execution_time, down_checksum \
         from {table} order by version asc"
      ),
      &[],
    )?;

    Ok(rows.iter().map(row_to_record).collect())
  }

  fn get_last(&mut self) -> Result<Option<MigrationRecord>> {
    let table = self.qualified_table();
    let rows = self.client.query(
      &format!(
        "select version, description, installed_on::text, success, checksum, execution_time, down_checksum \
         from {table} order by version desc limit 1"
      ),
      &[],
    )?;

    Ok(rows.first().map(row_to_record))
  }

  fn insert(&mut self, record: &MigrationRecord) -> Result<()> {
    let table = self.qualified_table();
    self.client.execute(
      &format!(
        "insert into {table} (version, description, success, checksum, execution_time, down_checksum) \
         values ($1, $2, $3, $4, $5, $6)"
      ),
      &[
        &record.version,
        &record.description,
        &record.success,
        &record.checksum,
        &record.execution_time,
        &record.down_checksum.as_deref(),
      ],
    )?;
    Ok(())
  }

  fn upsert(&mut self, record: &MigrationRecord) -> Result<()> {
    let table = self.qualified_table();
    self.client.execute(
      &format!(
        "insert into {table} (version, description, success, checksum, execution_time, down_checksum) \
         values ($1, $2, $3, $4, $5, $6) \
         on conflict (version) do update set \
           description = excluded.description, \
           success = excluded.success, \
           checksum = excluded.checksum, \
           execution_time = excluded.execution_time, \
           down_checksum = excluded.down_checksum"
      ),
      &[
        &record.version,
        &record.description,
        &record.success,
        &record.checksum,
        &record.execution_time,
        &record.down_checksum.as_deref(),
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

  fn set_success(&mut self, version: i64, success: bool) -> Result<()> {
    let table = self.qualified_table();
    self.client.execute(
      &format!("update {table} set success = $2 where version = $1"),
      &[&version, &success],
    )?;
    Ok(())
  }

  fn repair(&mut self, record: &MigrationRecord) -> Result<()> {
    let table = self.qualified_table();
    self.client.execute(
      &format!("update {table} set description = $2, checksum = $3, down_checksum = $4 where version = $1"),
      &[
        &record.version,
        &record.description,
        &record.checksum,
        &record.down_checksum.as_deref(),
      ],
    )?;
    Ok(())
  }

  fn drop_table(&mut self) -> Result<()> {
    let table = self.qualified_table();
    self
      .client
      .batch_execute(&format!("drop table if exists {table}"))?;
    Ok(())
  }
}

impl Locker for Postgres {
  fn lock(&mut self) -> Result<()> {
    if !self.flavor.supports(Feature::AdvisoryLocks) {
      return Err(Error::Unsupported(
        "CockroachDB does not support session-level advisory locks".to_string(),
      ));
    }

    let timeout_ms = self.lock_timeout.as_millis().min(i64::MAX as u128) as i64;
    self
      .client
      .batch_execute(&format!("set lock_timeout = {timeout_ms}"))?;
    match self
      .client
      .execute("select pg_advisory_lock($1)", &[&self.lock_key])
    {
      Ok(_) => Ok(()),
      Err(error) if is_lock_timeout(&error) => Err(Error::LockTimeout { backend: "postgres" }),
      Err(error) => Err(error.into()),
    }
  }

  fn unlock(&mut self) -> Result<()> {
    if !self.flavor.supports(Feature::AdvisoryLocks) {
      return Err(Error::Unsupported(
        "CockroachDB does not support session-level advisory locks".to_string(),
      ));
    }

    self
      .client
      .execute("select pg_advisory_unlock($1)", &[&self.lock_key])?;
    self.client.batch_execute("reset lock_timeout")?;
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
    self.flavor.supports(feature)
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
    self
      .admin_client_for(&name)?
      .batch_execute(&format!("create database {name}"))?;
    Ok(())
  }

  fn drop_database(&mut self, name: &str) -> Result<()> {
    let name = quote_ident_with(name, '"');
    self
      .admin_client_for(&name)?
      .batch_execute(&format!("drop database if exists {name}"))?;
    Ok(())
  }

  fn database_exists(&mut self, name: &str) -> Result<bool> {
    let row = self.admin_client_for(name)?.query_one(
      "select exists(select 1 from pg_database where datname = $1)",
      &[&name],
    )?;
    Ok(row.get(0))
  }

  fn force_drop_database(&mut self, name: &str) -> Result<()> {
    let mut client = self.admin_client_for(name)?;
    client.execute(
      "select pg_terminate_backend(pid) from pg_stat_activity \
       where datname = $1 and pid <> pg_backend_pid()",
      &[&name],
    )?;
    let name = quote_ident_with(name, '"');
    client.batch_execute(&format!("drop database if exists {name}"))?;
    Ok(())
  }
}

fn row_to_record(row: &postgres::Row) -> MigrationRecord {
  MigrationRecord {
    version: row.get(0),
    description: row.get(1),
    installed_on: row.get(2),
    success: row.get(3),
    checksum: row.get(4),
    execution_time: row.get(5),
    down_checksum: row.get(6),
  }
}

fn advisory_lock_key(database_name: &str) -> i64 {
  let crc = crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC).checksum(database_name.as_bytes());
  i64::from(crc) * 0x3d32_ad9e
}

fn is_lock_timeout(error: &postgres::Error) -> bool {
  error
    .code()
    .is_some_and(|code| *code == SqlState::LOCK_NOT_AVAILABLE)
}

fn database_name(url: &Url) -> Result<String> {
  url
    .path_segments()
    .and_then(|mut segments| segments.next_back())
    .filter(|name| !name.is_empty())
    .map(ToString::to_string)
    .ok_or_else(|| Error::config("database name not found in URL"))
}

fn maintenance_url(raw: &str) -> Result<String> {
  let mut url = Url::parse(raw).map_err(|error| Error::config(format!("invalid database URL: {error}")))?;
  url.set_path("/postgres");
  Ok(url.to_string())
}

fn maintenance_url_for(raw: &str, target: &str) -> Result<String> {
  let mut url = Url::parse(raw).map_err(|error| Error::config(format!("invalid database URL: {error}")))?;
  let maintenance = if target == "postgres" {
    "template1"
  } else {
    "postgres"
  };
  url.set_path(&format!("/{maintenance}"));
  Ok(url.to_string())
}

/// Rewrite CockroachDB scheme aliases to a scheme the `postgres` crate accepts.
fn normalize_database_url(raw: &str) -> Result<String> {
  let mut url = Url::parse(raw).map_err(|error| Error::config(format!("invalid database URL: {error}")))?;
  let scheme = url.scheme().to_ascii_lowercase();
  if !POSTGRES_SCHEME_ALIASES.contains(&scheme.as_str()) {
    return Ok(raw.to_string());
  }

  url
    .set_scheme("postgresql")
    .map_err(|_| Error::config(format!("unsupported database scheme `{scheme}`")))?;
  Ok(url.to_string())
}

fn flavor_for_engine(engine: DatabaseEngine) -> Result<PostgresFlavor> {
  match engine {
    DatabaseEngine::Postgres => Ok(PostgresFlavor::Postgres),
    DatabaseEngine::CockroachDb => Ok(PostgresFlavor::CockroachDb),
    DatabaseEngine::Sqlite => Err(Error::config("SQLite URL passed to the Postgres backend")),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn advisory_lock_key_matches_sqlx_formula() {
    assert_eq!(advisory_lock_key("midas"), 475_516_534_381_858_968);
  }

  #[test]
  fn quoting_matches_postgres_rules() {
    assert_eq!(quote_ident_with("a\"b", '"'), "\"a\"\"b\"");
    assert_eq!(quote_literal_with("a'b", '\''), "'a''b'");
  }

  #[test]
  fn cockroach_schemes_are_normalized() {
    for alias in POSTGRES_SCHEME_ALIASES {
      let url = format!("{alias}://user:pass@localhost:26257/app?sslmode=disable");
      let normalized = normalize_database_url(&url).expect("alias is accepted");
      assert!(normalized.starts_with("postgresql://"), "got `{normalized}`");
      assert!(
        normalized.contains("user:pass@localhost:26257/app"),
        "host and path must survive normalization: `{normalized}`"
      );
      assert!(normalized.contains("?sslmode=disable"));
    }
  }

  #[test]
  fn maintenance_url_uses_postgres_or_template1() {
    assert_eq!(
      maintenance_url("postgres://user@localhost/app").expect("url"),
      "postgres://user@localhost/postgres"
    );
    assert_eq!(
      maintenance_url_for("postgres://user@localhost/app", "app").expect("url"),
      "postgres://user@localhost/postgres"
    );
    assert_eq!(
      maintenance_url_for("postgres://user@localhost/postgres", "postgres").expect("url"),
      "postgres://user@localhost/template1"
    );
  }

  #[test]
  fn cockroach_disables_advisory_locks() {
    let engine = DatabaseEngine::from_scheme("cockroachdb").expect("known scheme");
    let flavor = flavor_for_engine(engine).expect("cockroach flavor");
    assert_eq!(flavor, PostgresFlavor::CockroachDb);
    assert!(!flavor.supports(Feature::AdvisoryLocks));
    assert!(flavor.supports(Feature::Schemas));
  }

  #[test]
  fn postgres_supports_advisory_locks() {
    let engine = DatabaseEngine::from_scheme("postgres").expect("known scheme");
    let flavor = flavor_for_engine(engine).expect("postgres flavor");
    assert_eq!(flavor, PostgresFlavor::Postgres);
    assert!(flavor.supports(Feature::AdvisoryLocks));
  }

  #[test]
  fn sqlite_urls_are_rejected_by_the_postgres_backend() {
    let engine = DatabaseEngine::from_scheme("sqlite").expect("known scheme");
    assert!(flavor_for_engine(engine).is_err());
  }

  #[test]
  fn postgres_urls_are_not_rewritten() {
    let url = "postgres://user:pass@localhost/app";
    assert_eq!(normalize_database_url(url).expect("valid url"), url);
  }
}
