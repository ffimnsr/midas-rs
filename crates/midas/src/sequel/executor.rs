//! Statement execution abstraction.

use crate::Result;

/// Executes SQL statements and controls transaction scope.
pub trait Executor {
  /// Execute a single statement.
  fn execute(&mut self, sql: &str) -> Result<()>;

  /// Execute a batch of statements in one round trip.
  fn execute_batch(&mut self, sql: &str) -> Result<()>;

  /// Open a transaction.
  fn begin(&mut self) -> Result<()>;

  /// Commit the current transaction.
  fn commit(&mut self) -> Result<()>;

  /// Roll back the current transaction.
  fn rollback(&mut self) -> Result<()>;
}
