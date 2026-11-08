//! Typed error contract shared by every crate consumer.
//!
//! Public APIs in this crate return [`Result`]. `anyhow` is intentionally not
//! exposed here; binaries and tests may still wrap these errors.

/// Convenient result alias used across the `midas` library.
pub type Result<T> = core::result::Result<T, Error>;

/// A plain message wrapped so it can be exposed as an error source.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct MessageError {
  message: String,
}

impl MessageError {
  /// Build a new message error.
  pub fn new(message: impl Into<String>) -> Self {
    Self {
      message: message.into(),
    }
  }
}

/// The error type returned by every fallible `midas` API.
#[derive(Debug, thiserror::Error)]
pub enum Error {
  /// Configuration is missing, malformed, or contradictory.
  #[error("configuration error: {0}")]
  Config(#[source] Box<dyn std::error::Error + Send + Sync>),

  /// Filesystem or other I/O failure.
  #[error("io error: {0}")]
  Io(#[from] std::io::Error),

  /// Driver-level database failure.
  #[error("database error: {0}")]
  Database(#[source] Box<dyn std::error::Error + Send + Sync>),

  /// Migration engine failure.
  #[error("migration error: {0}")]
  Migration(String),

  /// Checksum computation or verification failure.
  #[error("checksum error: {0}")]
  Checksum(String),

  /// Advisory lock failure.
  #[error("lock error: {0}")]
  Lock(String),

  /// Schema parsing, validation, or emission failure.
  #[error("schema error: {0}")]
  Schema(String),

  /// Drift detection failure.
  #[error("drift error: {0}")]
  Drift(String),

  /// Code generation failure.
  #[error("codegen error: {0}")]
  Codegen(String),

  /// Operation is not supported by this build or dialect.
  #[error("unsupported operation: {0}")]
  Unsupported(String),

  /// The requested migration version is not present on disk or in metadata.
  #[error("migration {version} not found")]
  MigrationNotFound { version: i64 },

  /// Stored checksum differs from the migration file checksum.
  #[error("checksum mismatch for migration {version}: expected {expected}, actual {actual}")]
  ChecksumMismatch {
    version: i64,
    expected: String,
    actual: String,
  },

  /// A migration previously failed and must be repaired before continuing.
  #[error("migration {version} is marked dirty")]
  DirtyMigration { version: i64 },

  /// The advisory lock could not be acquired before the configured timeout.
  #[error("could not acquire {backend} lock before timeout")]
  LockTimeout { backend: &'static str },

  /// The dialect does not implement the requested feature.
  #[error("unsupported {feature} feature for {dialect}")]
  UnsupportedFeature {
    dialect: &'static str,
    feature: &'static str,
  },
}

impl Error {
  /// Build a configuration error from a plain message.
  pub fn config(message: impl Into<String>) -> Self {
    Self::Config(Box::new(MessageError::new(message)))
  }

  /// Build a migration error from a plain message.
  pub fn migration(message: impl Into<String>) -> Self {
    Self::Migration(message.into())
  }

  /// Build an unsupported-feature error.
  pub fn unsupported_feature(dialect: &'static str, feature: &'static str) -> Self {
    Self::UnsupportedFeature { dialect, feature }
  }
}

impl From<toml::de::Error> for Error {
  fn from(error: toml::de::Error) -> Self {
    Self::Config(Box::new(error))
  }
}

#[cfg(feature = "postgres")]
impl From<postgres::Error> for Error {
  fn from(error: postgres::Error) -> Self {
    Self::Database(Box::new(error))
  }
}

#[cfg(feature = "mysql")]
impl From<mysql::Error> for Error {
  fn from(error: mysql::Error) -> Self {
    Self::Database(Box::new(error))
  }
}

#[cfg(feature = "sqlite")]
impl From<rusqlite::Error> for Error {
  fn from(error: rusqlite::Error) -> Self {
    Self::Database(Box::new(error))
  }
}

#[cfg(test)]
mod tests {
  use std::error::Error as _;

  use super::*;

  #[test]
  fn config_error_display_and_source() {
    let error = Error::config("missing database url");
    assert_eq!(error.to_string(), "configuration error: missing database url");
    assert!(error.source().is_some());
  }

  #[test]
  fn io_error_display_and_source() {
    let error = Error::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "gone"));
    assert_eq!(error.to_string(), "io error: gone");
    assert!(error.source().is_some());
  }

  #[test]
  fn database_error_display_and_source() {
    let error = Error::Database(Box::new(MessageError::new("connection refused")));
    assert_eq!(error.to_string(), "database error: connection refused");
    assert!(error.source().is_some());
  }

  #[test]
  fn plain_message_variants_display() {
    assert_eq!(
      Error::Migration("boom".into()).to_string(),
      "migration error: boom"
    );
    assert_eq!(Error::Checksum("boom".into()).to_string(), "checksum error: boom");
    assert_eq!(Error::Lock("boom".into()).to_string(), "lock error: boom");
    assert_eq!(Error::Schema("boom".into()).to_string(), "schema error: boom");
    assert_eq!(Error::Drift("boom".into()).to_string(), "drift error: boom");
    assert_eq!(Error::Codegen("boom".into()).to_string(), "codegen error: boom");
    assert_eq!(
      Error::Unsupported("boom".into()).to_string(),
      "unsupported operation: boom"
    );
  }

  #[test]
  fn user_facing_variants_display() {
    let error = Error::MigrationNotFound { version: 42 };
    assert_eq!(error.to_string(), "migration 42 not found");
    assert!(error.source().is_none());

    let error = Error::ChecksumMismatch {
      version: 1,
      expected: "abc".into(),
      actual: "def".into(),
    };
    assert_eq!(
      error.to_string(),
      "checksum mismatch for migration 1: expected abc, actual def"
    );
    assert!(error.source().is_none());

    let error = Error::DirtyMigration { version: 7 };
    assert_eq!(error.to_string(), "migration 7 is marked dirty");
    assert!(error.source().is_none());

    let error = Error::LockTimeout { backend: "postgres" };
    assert_eq!(
      error.to_string(),
      "could not acquire postgres lock before timeout"
    );
    assert!(error.source().is_none());

    let error = Error::UnsupportedFeature {
      dialect: "sqlite",
      feature: "schemas",
    };
    assert_eq!(error.to_string(), "unsupported schemas feature for sqlite");
    assert!(error.source().is_none());
  }

  #[test]
  fn toml_error_converts_to_config_error() {
    let error = toml::from_str::<toml::Value>("not = [valid").expect_err("invalid toml");
    let error = Error::from(error);
    assert!(matches!(error, Error::Config(_)));
    assert!(error.source().is_some());
  }
}
