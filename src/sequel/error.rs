pub use ::mysql::Error as MysqlError;
pub use ::postgres::Error as PostgresError;
pub use ::rusqlite::Error as SqliteError;

use std::{
  error,
  fmt,
};

use crate::GenericError;

#[derive(Debug, PartialEq)]
enum Kind {
  Mysql,
  Sqlite,
  Postgres,
}

struct ErrorInner {
  kind: Kind,
  cause: Option<GenericError>,
}

pub struct Error(Box<ErrorInner>);

impl fmt::Debug for Error {
  fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt
      .debug_struct("Error")
      .field("kind", &self.0.kind)
      .field("cause", &self.0.cause)
      .finish()
  }
}

impl fmt::Display for Error {
  fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
    match &self.0.kind {
      Kind::Postgres => fmt.write_str("postgres error")?,
      Kind::Sqlite => fmt.write_str("sqlite error")?,
      Kind::Mysql => fmt.write_str("mysql error")?,
    };
    if let Some(ref cause) = self.0.cause {
      write!(fmt, " => {}", cause)?;
    }
    Ok(())
  }
}

impl error::Error for Error {
  fn source(&self) -> Option<&(dyn error::Error + 'static)> {
    self.0.cause.as_ref().map(|e| &**e as _)
  }
}

impl From<PostgresError> for Error {
  fn from(value: PostgresError) -> Self {
    let message = value
      .as_db_error()
      .map(|e| format!("{} [{}:{}]", e.message(), e.code().code(), e.severity(),));
    Error::new(Kind::Postgres, message.map(GenericError::from))
  }
}

impl From<SqliteError> for Error {
  fn from(value: SqliteError) -> Self {
    Error::new(Kind::Sqlite, Some(value.into()))
  }
}

impl From<MysqlError> for Error {
  fn from(value: MysqlError) -> Self {
    Error::new(Kind::Mysql, Some(value.into()))
  }
}

impl Error {
  /// Consumes the error, returning its cause.
  #[allow(dead_code)]
  pub fn into_source(self) -> Option<GenericError> {
    self.0.cause
  }

  fn new(kind: Kind, cause: Option<GenericError>) -> Error {
    Error(Box::new(ErrorInner { kind, cause }))
  }
}
