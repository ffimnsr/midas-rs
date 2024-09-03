use log::trace;
use mysql::{prelude::Queryable, params, Pool, PooledConn};

use super::{Driver as SequelDriver, Error, VecSerial};

pub struct Mysql {
    conn: PooledConn,
}

impl Mysql {
    pub fn new(database_url: &str) -> Result<Self, mysql::Error> {
        let pool = Pool::new(database_url)?;
        let conn = pool.get_conn()?;
        let db = Mysql { conn };
        Ok(db)
    }
}

impl SequelDriver for Mysql {
    fn ensure_migration_schema_exists(&mut self) -> Result<(), Error> {
        Ok(())
    }

    fn ensure_migration_table_exists(&mut self) -> Result<(), Error> {
        let payload = "CREATE TABLE IF NOT EXISTS __schema_migrations (id SERIAL PRIMARY KEY, migration BIGINT)";
        self.conn.query_drop(payload)?;
        Ok(())
    }

    fn drop_migration_table(&mut self) -> Result<(), Error> {
        let payload = "DROP TABLE __schema_migrations";
        self.conn.query_drop(payload)?;
        Ok(())
    }

    fn count_migrations(&mut self) -> Result<i64, Error> {
        trace!("Retrieving migrations count");
        let payload = "SELECT COUNT(*) as count FROM __schema_migrations";
        let row = self.conn.query_first(payload)?;
        let result = row.get::<_, i64>(0);
        let result = row.unwrap().get("count").unwrap();
        Ok(result)
    }

    fn get_completed_migrations(&mut self) -> Result<VecSerial, Error> {
        trace!("Retrieving all completed migrations");
        let payload = "SELECT migration FROM __schema_migrations ORDER BY id ASC";
        let mut stmt = self.conn.query(payload)?;
        let result = it.map(|r| r.unwrap()).collect::<VecSerial>();
        Ok(result)
    }

    fn get_last_completed_migration(&mut self) -> Result<i64, Error> {
        trace!("Checking and retrieving the last migration stored on migrations table");
        let payload = "SELECT migration FROM __schema_migrations ORDER BY id DESC LIMIT 1";
        let mut stmt = self.conn.prep(payload)?;
        let result = stmt.query_row([], |row| row.get(0))?;
        Ok(result)
    }

    fn add_completed_migration(
        &mut self,
        migration_number: i64,
    ) -> Result<(), Error> {
        trace!("Adding migration to migrations table");
        let payload =
            "INSERT INTO __schema_migrations (migration) VALUES (:migration_number)";
        self.conn.exec_drop(payload, params! { "migration_number" => migration_number })?;
        Ok(())
    }

    fn delete_completed_migration(
        &mut self,
        migration_number: i64,
    ) -> Result<(), Error> {
        trace!("Removing a migration in the migrations table");
        let payload =
            "DELETE FROM __schema_migrations WHERE migration = :migration_number";
        self.conn.exec_drop(payload, params! { "migration_number" => migration_number })?;
        Ok(())
    }

    fn delete_last_completed_migration(&mut self) -> Result<(), Error> {
        let payload =
            "DELETE FROM __schema_migrations WHERE id=(SELECT MAX(id) FROM __schema_migrations);";
        self.conn.query_drop(payload)?;
        Ok(())
    }

    fn migrate(&mut self, query: &str) -> Result<(), Error> {
        self.conn.query_drop(query)?;
        Ok(())
    }
}
