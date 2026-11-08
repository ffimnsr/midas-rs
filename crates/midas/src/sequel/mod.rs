//! Database backends and the capability traits they expose.

pub mod admin;
pub mod dialect;
pub mod executor;
pub mod introspect;
pub mod memory;
pub mod store;

#[cfg(feature = "mysql")]
pub mod mysql;
#[cfg(feature = "postgres")]
pub mod postgres;
#[cfg(feature = "sqlite")]
pub mod sqlite;

use url::Url;

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
  AppliedMigration,
  Locker,
  MigrationStore,
};

use crate::config::Config;
use crate::{
  Error,
  Result,
};

/// Composite capability set required by the migration runner.
pub trait Backend: Executor + MigrationStore + Locker + DatabaseAdmin + Dialect + Introspector {}

impl<T> Backend for T where T: Executor + MigrationStore + Locker + DatabaseAdmin + Dialect + Introspector {}

/// Open a backend for the configured database URL.
pub fn connect(config: &Config) -> Result<Box<dyn Backend>> {
  let url = config.require_database_url()?.to_string();

  // Local paths are treated as SQLite databases without an explicit scheme.
  if url.starts_with('.') || url.starts_with('/') {
    return connect_sqlite(config, &url);
  }

  let parsed = Url::parse(&url).map_err(|error| Error::config(format!("invalid database URL: {error}")))?;
  match parsed.scheme().to_ascii_lowercase().as_str() {
    "postgres" | "postgresql" => connect_postgres(config),
    "mysql" => connect_mysql(config),
    "sqlite" | "sqlite3" | "file" => connect_sqlite(config, &url),
    other => Err(Error::config(format!("unsupported database scheme `{other}`"))),
  }
}

#[cfg(feature = "postgres")]
fn connect_postgres(config: &Config) -> Result<Box<dyn Backend>> {
  Ok(Box::new(postgres::Postgres::with_config(config)?))
}

#[cfg(not(feature = "postgres"))]
fn connect_postgres(_: &Config) -> Result<Box<dyn Backend>> {
  Err(Error::unsupported_feature("postgres", "driver"))
}

#[cfg(feature = "mysql")]
fn connect_mysql(config: &Config) -> Result<Box<dyn Backend>> {
  Ok(Box::new(mysql::Mysql::with_config(config)?))
}

#[cfg(not(feature = "mysql"))]
fn connect_mysql(_: &Config) -> Result<Box<dyn Backend>> {
  Err(Error::unsupported_feature("mysql", "driver"))
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
