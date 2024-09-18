use indoc::formatdoc;
use log::trace;
use postgres::{Client, NoTls};

use super::{Driver as SequelDriver, Error, VecSerial};

pub struct Postgres {
    client: Client,
}

impl Postgres {
    pub fn new(database_url: &str) -> Result<Self, Error> {
        let client = Client::connect(database_url, NoTls)?;
        let mut db = Postgres { client };
        db.ensure_migration_schema_exists()?;
        db.ensure_migration_table_exists()?;
        Ok(db)
    }
}

impl SequelDriver for Postgres {
    fn ensure_migration_schema_exists(&mut self) -> Result<(), Error> {
        self.client.execute("CREATE SCHEMA IF NOT EXISTS public", &[])?;
        self.client.execute("GRANT ALL ON SCHEMA public TO public", &[])?;
        Ok(())
    }

    fn ensure_migration_table_exists(&mut self) -> Result<(), Error> {
        let payload = "CREATE TABLE IF NOT EXISTS public.__schema_migrations (id SERIAL PRIMARY KEY, migration BIGINT)";
        self.client.execute(payload, &[])?;
        Ok(())
    }

    fn drop_migration_table(&mut self) -> Result<(), Error> {
        let payload = "DROP TABLE public.__schema_migrations";
        self.client.execute(payload, &[])?;
        Ok(())
    }

    fn drop_database(&mut self, db_name: &str) -> Result<(), Error> {
        let payload = formatdoc! {"
            DROP DATABASE IF EXISTS `{db_name}`;
            CREATE DATABASE `{db_name}`;
        ", db_name = db_name };
        self.client.execute(&payload, &[])?;
        Ok(())
    }

    fn count_migrations(&mut self) -> Result<i64, Error> {
        trace!("Retrieving migrations count");
        let payload =
            "SELECT COUNT(*) as count FROM public.__schema_migrations";
        let row = self.client.query_one(payload, &[])?;
        let result = row.get::<_, i64>(0);
        Ok(result)
    }

    fn get_completed_migrations(&mut self) -> Result<VecSerial, Error> {
        trace!("Retrieving all completed migrations");
        let payload =
            "SELECT migration FROM public.__schema_migrations ORDER BY id ASC";
        let it = self.client.query(payload, &[])?;
        let result =
            it.iter().map(|r| r.get("migration")).collect::<VecSerial>();
        Ok(result)
    }

    fn get_last_completed_migration(&mut self) -> Result<i64, Error> {
        trace!("Checking and retrieving the last migration stored on migrations table");
        let payload =
            "SELECT migration FROM public.__schema_migrations ORDER BY id DESC LIMIT 1";
        let result = self.client.query(payload, &[])?;

        if result.is_empty() {
            Ok(-1)
        } else {
            Ok(result[0].get(0))
        }
    }

    fn add_completed_migration(
        &mut self,
        migration_number: i64,
    ) -> Result<(), Error> {
        trace!("Adding migration to migrations table");
        let payload =
            "INSERT INTO public.__schema_migrations (migration) VALUES ($1)";
        self.client.execute(payload, &[&migration_number])?;
        Ok(())
    }

    fn delete_completed_migration(
        &mut self,
        migration_number: i64,
    ) -> Result<(), Error> {
        trace!("Removing a migration in the migrations table");
        let payload =
            "DELETE FROM public.__schema_migrations WHERE migration = $1";
        self.client.execute(payload, &[&migration_number])?;
        Ok(())
    }

    fn delete_last_completed_migration(&mut self) -> Result<(), Error> {
        let payload =
            "DELETE FROM public.__schema_migrations WHERE id=(SELECT MAX(id) FROM __schema_migrations);";
        self.client.execute(payload, &[])?;
        Ok(())
    }

    fn migrate(&mut self, query: &str) -> Result<(), Error> {
        self.client.simple_query(query)?;
        Ok(())
    }
}
