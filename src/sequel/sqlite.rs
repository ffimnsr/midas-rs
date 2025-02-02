use std::fs;
use std::path::Path;

use indoc::indoc;
use rusqlite::Connection;

use super::{
  AnyhowResult,
  Driver as SequelDriver,
  VecSerial,
};

/// The Sqlite struct definition
pub struct Sqlite {
  /// Implement the Sqlite struct
  conn: Connection,
  /// The file URL of the SQLite database
  file_url: String,
}

/// Implement the Sqlite struct
impl Sqlite {
  /// Create a new instance of Sqlite
  /// # Arguments
  /// * `file_url` - The file URL of the SQLite database
  /// # Returns
  /// * An instance of Sqlite
  /// # Example
  /// ```
  /// let db = Sqlite::new("sqlite://./db.sqlite");
  /// ```
  pub fn new(file_url: &str) -> AnyhowResult<Self> {
    log::trace!("Opening SQLite database connection: {file_url}");

    // Strip file:// and file: prefix
    // Also, strip sqlite:// and sqlite: similar prefix
    let file_url = file_url
      .replace("file://", "")
      .replace("file:", "")
      .replace("sqlite://", "")
      .replace("sqlite:", "");

    // If the file_url starts with "/" it means it's an absolute path
    // Otherwise, it's a relative path
    let file_url: &str = if file_url.starts_with("/") {
      &file_url.to_string()
    } else {
      &format!("./{}", file_url)
    };

    // Open the connection
    let conn = Connection::open(file_url)?;
    let mut db: Sqlite = Sqlite {
      conn,
      file_url: file_url.to_string(),
    };

    // Ensure the midas schema migration table exists
    db.ensure_midas_schema()?;
    Ok(db)
  }
}

/// Implement the SequelDriver trait for Sqlite
impl SequelDriver for Sqlite {
  /// Ensure the __schema_migrations table exists
  /// If it doesn't exist, create it
  fn ensure_midas_schema(&mut self) -> AnyhowResult<()> {
    let payload = indoc! {"
      CREATE TABLE IF NOT EXISTS __schema_migrations (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        migration BIGINT
      );
    "};
    self.conn.execute(payload, ())?;
    Ok(())
  }

  /// Drop the __schema_migrations table
  fn drop_migration_table(&mut self) -> AnyhowResult<()> {
    let payload = "DROP TABLE __schema_migrations";
    self.conn.execute(payload, ())?;
    Ok(())
  }

  /// Drop the database
  fn drop_database(&mut self, _: &str) -> AnyhowResult<()> {
    // SQLite does not support dropping databases
    // Instead, we can delete the file
    let path = Path::new(&self.file_url);
    if path.exists() {
      fs::remove_file(path)?;
    }

    // Re-create file through connection
    Connection::open(path)?;
    Ok(())
  }

  /// Count the number of migrations
  fn count_migrations(&mut self) -> AnyhowResult<i64> {
    log::trace!("Retrieving migrations count");
    let payload = "SELECT COUNT(*) as count FROM __schema_migrations";
    let mut stmt = self.conn.prepare(payload)?;
    let result = stmt.query_row((), |row| row.get(0))?;
    Ok(result)
  }

  /// Get all completed migrations
  fn get_completed_migrations(&mut self) -> AnyhowResult<VecSerial> {
    log::trace!("Retrieving all completed migrations");
    let payload = "SELECT migration FROM __schema_migrations ORDER BY id ASC";
    let mut stmt = self.conn.prepare(payload)?;
    let it = stmt.query_map((), |row| row.get(0))?;
    let result = it.map(|r| r.unwrap()).collect::<VecSerial>();
    Ok(result)
  }

  /// Get the last completed migration
  fn get_last_completed_migration(&mut self) -> AnyhowResult<i64> {
    log::trace!("Checking and retrieving the last migration stored on migrations table");
    let payload = "SELECT migration FROM __schema_migrations ORDER BY id DESC LIMIT 1";
    let mut stmt = self.conn.prepare(payload)?;
    let result = stmt.query_row((), |row| row.get(0))?;
    Ok(result)
  }

  /// Add a completed migration
  fn add_completed_migration(&mut self, migration_number: i64) -> AnyhowResult<()> {
    log::trace!("Adding migration to migrations table");
    let payload = "INSERT INTO __schema_migrations (migration) VALUES ($1)";
    self.conn.execute(payload, [&migration_number])?;
    Ok(())
  }

  /// Delete a completed migration
  fn delete_completed_migration(&mut self, migration_number: i64) -> AnyhowResult<()> {
    log::trace!("Removing a migration in the migrations table");
    let payload = "DELETE FROM __schema_migrations WHERE migration = $1";
    self.conn.execute(payload, [&migration_number])?;
    Ok(())
  }

  /// Delete the last completed migration
  fn delete_last_completed_migration(&mut self) -> AnyhowResult<()> {
    let payload = "DELETE FROM __schema_migrations WHERE id=(SELECT MAX(id) FROM __schema_migrations);";
    self.conn.execute(payload, ())?;
    Ok(())
  }

  /// Run a migration
  fn migrate(&mut self, query: &str, _migration_number: i64) -> AnyhowResult<()> {
    self.conn.execute(query, ())?;
    Ok(())
  }

  /// Get the database name
  fn db_name(&self) -> &str {
    "sqlite"
  }
}
