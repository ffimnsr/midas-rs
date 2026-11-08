//! Database backends and the capability traits they expose.

pub mod admin;
pub mod dialect;
pub mod executor;
pub mod introspect;
pub mod memory;
pub mod store;

#[cfg(feature = "postgres")]
pub mod postgres;
#[cfg(feature = "sqlite")]
pub mod sqlite;

pub use admin::DatabaseAdmin;
pub use dialect::{
  Dialect,
  Feature,
  SqlLiteral,
};
pub use executor::Executor;
pub use introspect::{
  Introspector,
  ObjectKind,
  SchemaObject,
  SchemaSnapshot,
};
pub use store::{
  Locker,
  MigrationStore,
};

use crate::config::Config;
use crate::{
  Error,
  Result,
};

/// URL schemes accepted by [`DatabaseEngine::from_scheme`].
const SUPPORTED_SCHEMES: &[&str] = &[
  "postgres",
  "postgresql",
  "cockroach",
  "cockroachdb",
  "sqlite",
  "sqlite3",
  "file",
];

/// Database engine selected by a connection URL.
///
/// CockroachDB uses the Postgres wire protocol and the Postgres SQL dialect.
/// Keep it as a separate engine so capability checks (for example, advisory
/// locks) can distinguish it from a regular Postgres server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseEngine {
  /// Postgres or a Postgres-wire compatible server.
  Postgres,
  /// CockroachDB, served by the Postgres wire protocol.
  CockroachDb,
  /// SQLite, including local filesystem paths without a URL scheme.
  Sqlite,
}

impl DatabaseEngine {
  /// Resolve an engine from a database URL or a local SQLite path.
  pub fn from_url(raw: &str) -> Result<Self> {
    // Local paths and SQLite's in-memory sentinel are accepted without a scheme.
    if raw.starts_with('.') || raw.starts_with('/') || raw == ":memory:" {
      return Ok(Self::Sqlite);
    }

    let url =
      url::Url::parse(raw).map_err(|error| Error::config(format!("invalid database URL: {error}")))?;
    Self::from_scheme(url.scheme())
  }

  /// Resolve an engine from a URL scheme.
  pub fn from_scheme(scheme: &str) -> Result<Self> {
    match scheme.to_ascii_lowercase().as_str() {
      "postgres" | "postgresql" => Ok(Self::Postgres),
      "cockroach" | "cockroachdb" => Ok(Self::CockroachDb),
      "sqlite" | "sqlite3" | "file" => Ok(Self::Sqlite),
      other => Err(Error::config(format!(
        "unsupported database scheme `{other}`; expected one of {}",
        SUPPORTED_SCHEMES.join(", ")
      ))),
    }
  }

  /// Canonical dialect name used in generated configuration.
  pub fn dialect_name(self) -> &'static str {
    match self {
      Self::Postgres | Self::CockroachDb => "postgres",
      Self::Sqlite => "sqlite",
    }
  }
}

/// Composite capability set required by the migration runner.
pub trait Backend: Executor + MigrationStore + Locker + DatabaseAdmin + Dialect + Introspector {}

impl<T> Backend for T where T: Executor + MigrationStore + Locker + DatabaseAdmin + Dialect + Introspector {}

/// Extract the target database name from a connection URL.
pub fn database_name(database_url: &str) -> Result<String> {
  if DatabaseEngine::from_url(database_url)? == DatabaseEngine::Sqlite {
    return Ok("sqlite".to_string());
  }

  let url =
    url::Url::parse(database_url).map_err(|error| Error::config(format!("invalid database URL: {error}")))?;
  url
    .path_segments()
    .and_then(|mut segments| segments.next_back())
    .filter(|name| !name.is_empty())
    .map(ToString::to_string)
    .ok_or_else(|| Error::config("database name not found in URL"))
}

/// Open a backend for the configured database URL.
pub fn connect(config: &Config) -> Result<Box<dyn Backend>> {
  let url = config.require_database_url()?.to_string();

  match DatabaseEngine::from_url(&url)? {
    DatabaseEngine::Postgres | DatabaseEngine::CockroachDb => connect_postgres(config),
    DatabaseEngine::Sqlite => connect_sqlite(config, &url),
  }
}

/// Open a maintenance backend that can create or drop the configured database.
pub fn connect_admin(config: &Config) -> Result<Box<dyn DatabaseAdmin>> {
  let url = config.require_database_url()?.to_string();

  match DatabaseEngine::from_url(&url)? {
    DatabaseEngine::Postgres | DatabaseEngine::CockroachDb => connect_postgres_admin(config),
    DatabaseEngine::Sqlite => connect_sqlite_admin(&url),
  }
}

#[cfg(feature = "postgres")]
fn connect_postgres_admin(config: &Config) -> Result<Box<dyn DatabaseAdmin>> {
  Ok(Box::new(postgres::Postgres::admin(config)?))
}

#[cfg(not(feature = "postgres"))]
fn connect_postgres_admin(_: &Config) -> Result<Box<dyn DatabaseAdmin>> {
  Err(Error::unsupported_feature("postgres", "driver"))
}

#[cfg(feature = "sqlite")]
fn connect_sqlite_admin(url: &str) -> Result<Box<dyn DatabaseAdmin>> {
  Ok(Box::new(sqlite::Sqlite::admin(url)))
}

#[cfg(not(feature = "sqlite"))]
fn connect_sqlite_admin(_: &str) -> Result<Box<dyn DatabaseAdmin>> {
  Err(Error::unsupported_feature("sqlite", "driver"))
}

#[cfg(feature = "postgres")]
fn connect_postgres(config: &Config) -> Result<Box<dyn Backend>> {
  Ok(Box::new(postgres::Postgres::with_config(config)?))
}

#[cfg(not(feature = "postgres"))]
fn connect_postgres(_: &Config) -> Result<Box<dyn Backend>> {
  Err(Error::unsupported_feature("postgres", "driver"))
}

#[cfg(feature = "sqlite")]
fn connect_sqlite(config: &Config, url: &str) -> Result<Box<dyn Backend>> {
  Ok(Box::new(sqlite::Sqlite::with_config(config, url)?))
}

#[cfg(not(feature = "sqlite"))]
fn connect_sqlite(_: &Config, _: &str) -> Result<Box<dyn Backend>> {
  Err(Error::unsupported_feature("sqlite", "driver"))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn engines_are_resolved_from_schemes() {
    for (scheme, expected) in [
      ("postgres", DatabaseEngine::Postgres),
      ("POSTGRESQL", DatabaseEngine::Postgres),
      ("cockroach", DatabaseEngine::CockroachDb),
      ("cockroachdb", DatabaseEngine::CockroachDb),
      ("sqlite", DatabaseEngine::Sqlite),
      ("sqlite3", DatabaseEngine::Sqlite),
      ("file", DatabaseEngine::Sqlite),
    ] {
      assert_eq!(
        DatabaseEngine::from_scheme(scheme).expect("known scheme"),
        expected
      );
    }
  }

  #[test]
  fn local_paths_and_urls_resolve_to_engines() {
    assert_eq!(
      DatabaseEngine::from_url("./app.db3").expect("local path"),
      DatabaseEngine::Sqlite
    );
    assert_eq!(
      DatabaseEngine::from_url(":memory:").expect("memory sentinel"),
      DatabaseEngine::Sqlite
    );
    assert_eq!(
      DatabaseEngine::from_url("cockroachdb://root@localhost:26257/app").expect("url"),
      DatabaseEngine::CockroachDb
    );
  }

  #[test]
  fn unknown_schemes_report_the_supported_list() {
    let error = DatabaseEngine::from_scheme("mysql").expect_err("mysql is unsupported");
    let message = error.to_string();
    assert!(message.contains("unsupported database scheme `mysql`"));
    assert!(message.contains("postgres"));
    assert!(message.contains("sqlite"));
  }

  #[test]
  fn cockroach_uses_the_postgres_dialect_name() {
    assert_eq!(DatabaseEngine::CockroachDb.dialect_name(), "postgres");
    assert_eq!(DatabaseEngine::Sqlite.dialect_name(), "sqlite");
  }

  fn assert_object_safe(
    _: &dyn Executor,
    _: &dyn MigrationStore,
    _: &dyn Locker,
    _: &dyn Introspector,
    _: &dyn Dialect,
    _: &dyn DatabaseAdmin,
    _: &dyn Backend,
  ) {
  }

  #[test]
  fn every_capability_trait_is_object_safe() {
    let backend = memory::MemoryBackend::new();
    assert_object_safe(
      &backend, &backend, &backend, &backend, &backend, &backend, &backend,
    );
  }

  #[test]
  fn memory_backend_can_be_boxed_as_backend() {
    let _: Box<dyn Backend> = Box::new(memory::MemoryBackend::new());
  }
}
