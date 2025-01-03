use indoc::indoc;
use rusqlite::Connection;

use super::{
  AnyhowResult,
  Driver as SequelDriver,
  VecSerial,
};

pub struct Sqlite {
  conn: Connection,
}

impl Sqlite {
  pub fn new(file_url: &str) -> AnyhowResult<Self> {
    let conn = Connection::open(file_url)?;
    let mut db: Sqlite = Sqlite { conn };
    db.ensure_midas_schema()?;
    Ok(db)
  }
}

impl SequelDriver for Sqlite {
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

  fn drop_migration_table(&mut self) -> AnyhowResult<()> {
    let payload = "DROP TABLE __schema_migrations";
    self.conn.execute(payload, ())?;
    Ok(())
  }

  fn drop_database(&mut self, _: &str) -> AnyhowResult<()> {
    // Cannot drop database in SQLite
    Ok(())
  }

  fn count_migrations(&mut self) -> AnyhowResult<i64> {
    log::trace!("Retrieving migrations count");
    let payload = "SELECT COUNT(*) as count FROM __schema_migrations";
    let mut stmt = self.conn.prepare(payload)?;
    let result = stmt.query_row((), |row| row.get(0))?;
    Ok(result)
  }

  fn get_completed_migrations(&mut self) -> AnyhowResult<VecSerial> {
    log::trace!("Retrieving all completed migrations");
    let payload = "SELECT migration FROM __schema_migrations ORDER BY id ASC";
    let mut stmt = self.conn.prepare(payload)?;
    let it = stmt.query_map((), |row| row.get(0))?;
    let result = it.map(|r| r.unwrap()).collect::<VecSerial>();
    Ok(result)
  }

  fn get_last_completed_migration(&mut self) -> AnyhowResult<i64> {
    log::trace!("Checking and retrieving the last migration stored on migrations table");
    let payload = "SELECT migration FROM __schema_migrations ORDER BY id DESC LIMIT 1";
    let mut stmt = self.conn.prepare(payload)?;
    let result = stmt.query_row((), |row| row.get(0))?;
    Ok(result)
  }

  fn add_completed_migration(&mut self, migration_number: i64) -> AnyhowResult<()> {
    log::trace!("Adding migration to migrations table");
    let payload = "INSERT INTO __schema_migrations (migration) VALUES ($1)";
    self.conn.execute(payload, [&migration_number])?;
    Ok(())
  }

  fn delete_completed_migration(&mut self, migration_number: i64) -> AnyhowResult<()> {
    log::trace!("Removing a migration in the migrations table");
    let payload = "DELETE FROM __schema_migrations WHERE migration = $1";
    self.conn.execute(payload, [&migration_number])?;
    Ok(())
  }

  fn delete_last_completed_migration(&mut self) -> AnyhowResult<()> {
    let payload = "DELETE FROM __schema_migrations WHERE id=(SELECT MAX(id) FROM __schema_migrations);";
    self.conn.execute(payload, ())?;
    Ok(())
  }

  fn migrate(&mut self, query: &str, _migration_number: i64) -> AnyhowResult<()> {
    self.conn.execute(query, ())?;
    Ok(())
  }

  fn db_name(&self) -> &str {
    "sqlite"
  }
}
