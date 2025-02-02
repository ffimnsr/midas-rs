use anyhow::Context as _;
use indoc::{
  formatdoc,
  indoc,
};
use mysql::prelude::Queryable;
use mysql::{
  params,
  Pool,
  PooledConn,
};

use super::{
  AnyhowResult,
  Driver as SequelDriver,
  VecSerial,
};

/// The MySQL struct definition
pub struct Mysql {
  /// The MySQL connection
  conn: PooledConn,

  /// The database name
  database_name: String,
}

/// Implement the MySQL struct
impl Mysql {
  pub fn new(database_url: &str) -> AnyhowResult<Self> {
    // Open the connection
    let pool = Pool::new(database_url)?;
    let conn = pool.get_conn()?;

    // Get the database name from the URL
    let url = url::Url::parse(database_url)?;
    let database_name = url
      .path_segments()
      .and_then(|s| s.last())
      .context("Database name not found")?;

    // Create a new instance of MySQL
    let mut db = Mysql {
      conn,
      database_name: database_name.into(),
    };

    // Ensure the midas schema
    db.ensure_midas_schema()?;
    Ok(db)
  }
}

/// Implement the SequelDriver trait for MySQL
impl SequelDriver for Mysql {
  /// Implement the ensure_midas_schema method
  fn ensure_midas_schema(&mut self) -> AnyhowResult<()> {
    let payload = indoc! {"
      CREATE TABLE IF NOT EXISTS __schema_migrations (
        id INT NOT NULL AUTO_INCREMENT,
        migration BIGINT,
        PRIMARY KEY (id)
      ) AUTO_INCREMENT = 100;
    "};
    self.conn.query_drop(payload)?;
    Ok(())
  }

  /// Drop the migration table
  fn drop_migration_table(&mut self) -> AnyhowResult<()> {
    let payload = "DROP TABLE __schema_migrations";
    self.conn.query_drop(payload)?;
    Ok(())
  }

  /// Drop the database
  fn drop_database(&mut self, db_name: &str) -> AnyhowResult<()> {
    let payload = formatdoc! {"
      DROP DATABASE IF EXISTS `{db_name}`;
      CREATE DATABASE `{db_name}`;
    ", db_name = db_name};
    self.conn.exec_drop(payload, ())?;
    Ok(())
  }

  /// Count the number of migrations
  fn count_migrations(&mut self) -> AnyhowResult<i64> {
    log::trace!("Retrieving migrations count");
    let payload = "SELECT COUNT(*) as count FROM __schema_migrations";
    let row: Option<i64> = self.conn.query_first(payload)?;
    let result = row.unwrap();
    Ok(result)
  }

  /// Get all completed migrations
  fn get_completed_migrations(&mut self) -> AnyhowResult<VecSerial> {
    log::trace!("Retrieving all completed migrations");
    let payload = "SELECT migration FROM __schema_migrations ORDER BY id ASC";
    let result: VecSerial = self.conn.query(payload)?;
    Ok(result)
  }

  /// Get the last completed migration
  fn get_last_completed_migration(&mut self) -> AnyhowResult<i64> {
    log::trace!("Checking and retrieving the last migration stored on migrations table");
    let payload = "SELECT migration FROM __schema_migrations ORDER BY id DESC LIMIT 1";
    let row: Option<i64> = self.conn.query_first(payload)?;
    let result = row.unwrap();
    Ok(result)
  }

  /// Add a completed migration
  fn add_completed_migration(&mut self, migration_number: i64) -> AnyhowResult<()> {
    log::trace!("Adding migration to migrations table");
    let payload = "INSERT INTO __schema_migrations (migration) VALUES (:migration_number)";
    self
      .conn
      .exec_drop(payload, params! { "migration_number" => migration_number })?;
    Ok(())
  }

  /// Delete a completed migration
  fn delete_completed_migration(&mut self, migration_number: i64) -> AnyhowResult<()> {
    log::trace!("Removing a migration in the migrations table");
    let payload = "DELETE FROM __schema_migrations WHERE migration = :migration_number";
    self
      .conn
      .exec_drop(payload, params! { "migration_number" => migration_number })?;
    Ok(())
  }

  /// Delete the last completed migration
  fn delete_last_completed_migration(&mut self) -> AnyhowResult<()> {
    let payload = "DELETE FROM __schema_migrations WHERE id=(SELECT MAX(id) FROM __schema_migrations);";
    self.conn.query_drop(payload)?;
    Ok(())
  }

  /// Run a migration
  fn migrate(&mut self, query: &str, _migration_number: i64) -> AnyhowResult<()> {
    self.conn.query_drop(query)?;
    Ok(())
  }

  /// Get the database name
  fn db_name(&self) -> &str {
    &self.database_name
  }
}
