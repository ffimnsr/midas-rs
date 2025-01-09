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

pub struct Mysql {
  conn: PooledConn,
}

impl Mysql {
  pub fn new(database_url: &str) -> AnyhowResult<Self> {
    let pool = Pool::new(database_url)?;
    let conn = pool.get_conn()?;
    let mut db = Mysql { conn };
    db.ensure_midas_schema()?;
    Ok(db)
  }
}

impl SequelDriver for Mysql {
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

  fn drop_migration_table(&mut self) -> AnyhowResult<()> {
    let payload = "DROP TABLE __schema_migrations";
    self.conn.query_drop(payload)?;
    Ok(())
  }

  fn drop_database(&mut self, db_name: &str) -> AnyhowResult<()> {
    let payload = formatdoc! {"
      DROP DATABASE IF EXISTS `{db_name}`;
      CREATE DATABASE `{db_name}`;
    ", db_name = db_name};
    self.conn.exec_drop(payload, ())?;
    Ok(())
  }

  fn count_migrations(&mut self) -> AnyhowResult<i64> {
    log::trace!("Retrieving migrations count");
    let payload = "SELECT COUNT(*) as count FROM __schema_migrations";
    let row: Option<i64> = self.conn.query_first(payload)?;
    let result = row.unwrap();
    Ok(result)
  }

  fn get_completed_migrations(&mut self) -> AnyhowResult<VecSerial> {
    log::trace!("Retrieving all completed migrations");
    let payload = "SELECT migration FROM __schema_migrations ORDER BY id ASC";
    let result: VecSerial = self.conn.query(payload)?;
    Ok(result)
  }

  fn get_last_completed_migration(&mut self) -> AnyhowResult<i64> {
    log::trace!("Checking and retrieving the last migration stored on migrations table");
    let payload = "SELECT migration FROM __schema_migrations ORDER BY id DESC LIMIT 1";
    let row: Option<i64> = self.conn.query_first(payload)?;
    let result = row.unwrap();
    Ok(result)
  }

  fn add_completed_migration(&mut self, migration_number: i64) -> AnyhowResult<()> {
    log::trace!("Adding migration to migrations table");
    let payload = "INSERT INTO __schema_migrations (migration) VALUES (:migration_number)";
    self
      .conn
      .exec_drop(payload, params! { "migration_number" => migration_number })?;
    Ok(())
  }

  fn delete_completed_migration(&mut self, migration_number: i64) -> AnyhowResult<()> {
    log::trace!("Removing a migration in the migrations table");
    let payload = "DELETE FROM __schema_migrations WHERE migration = :migration_number";
    self
      .conn
      .exec_drop(payload, params! { "migration_number" => migration_number })?;
    Ok(())
  }

  fn delete_last_completed_migration(&mut self) -> AnyhowResult<()> {
    let payload = "DELETE FROM __schema_migrations WHERE id=(SELECT MAX(id) FROM __schema_migrations);";
    self.conn.query_drop(payload)?;
    Ok(())
  }

  fn migrate(&mut self, query: &str, _migration_number: i64) -> AnyhowResult<()> {
    self.conn.query_drop(query)?;
    Ok(())
  }

  fn db_name(&self) -> &str {
    ""
  }
}
