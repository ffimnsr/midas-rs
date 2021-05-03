use log::debug;
use postgres::{Client, NoTls};

use super::{Error, SequelDriver, VecSerial};

pub struct Postgres {
    client: Client,
}

impl Postgres {
    pub fn new(database_url: &str) -> Result<Self, Error> {
        let client = Client::connect(database_url, NoTls)?;
        let mut pg = Postgres { client };
        pg.ensure_migration_table_exists()?;
        Ok(pg)
    }
}

impl SequelDriver for Postgres {
    fn ensure_migration_table_exists(&mut self) -> Result<(), Error> {
        let payload = "CREATE TABLE IF NOT EXISTS __schema_migrations (id SERIAL PRIMARY KEY, migration BIGINT)";
        self.client.execute(payload, &[])?;
        Ok(())
    }

    fn drop_migration_table(&mut self) -> Result<(), Error> {
        let payload = "DROP TABLE __schema_migrations";
        self.client.execute(payload, &[])?;
        Ok(())
    }

    fn get_completed_migrations(&mut self) -> Result<VecSerial, Error> {
        debug!("Retrieving all completed migrations");
        let payload = "SELECT migration FROM __schema_migrations ORDER BY id ASC";
        let it = self.client.query(payload, &[])?;
        let result = it.iter().map(|r| r.get("migration")).collect::<VecSerial>();
        Ok(result)
    }

    fn get_last_completed_migration(&mut self) -> Result<i64, Error> {
        debug!("Checking and retrieving the last migration stored on migrations table");
        let payload = "SELECT migration FROM __schema_migrations ORDER BY id DESC LIMIT 1";
        let result = self.client.query(payload, &[])?;

        if result.is_empty() {
            Ok(-1)
        } else {
            Ok(result[0].get(0))
        }
    }

    fn add_completed_migration(&mut self, migration_number: i64) -> Result<(), Error> {
        debug!("Adding migration to migrations table");
        let payload = "INSERT INTO __schema_migrations (migration) VALUES ($1)";
        self.client.execute(payload, &[&migration_number])?;
        Ok(())
    }

    fn delete_completed_migration(&mut self, migration_number: i64) -> Result<(), Error> {
        debug!("Removing a migration in the migrations table");
        let payload = "DELETE FROM __schema_migrations WHERE migration = $1";
        self.client.execute(payload, &[&migration_number])?;
        Ok(())
    }

    fn delete_last_completed_migration(&mut self) -> Result<(), Error> {
        let payload =
            "DELETE FROM __schema_migrations WHERE id=(SELECT MAX(id) FROM __schema_migrations);";
        self.client.execute(payload, &[])?;
        Ok(())
    }

    fn migrate(&mut self, query: &str) -> Result<(), Error> {
        self.client.simple_query(&query)?;
        Ok(())
    }
}
