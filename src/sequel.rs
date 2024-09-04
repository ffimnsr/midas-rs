use error::Error;

pub mod error;
pub mod mysql;
pub mod postgres;
pub mod sqlite;

pub type VecSerial = Vec<i64>;

pub trait Driver {
    fn ensure_migration_schema_exists(&mut self) -> Result<(), Error>;
    fn ensure_migration_table_exists(&mut self) -> Result<(), Error>;
    #[allow(dead_code)]
    fn drop_migration_table(&mut self) -> Result<(), Error>;
    fn drop_database(&mut self, db_name: &str) -> Result<(), Error>;
    fn count_migrations(&mut self) -> Result<i64, Error>;
    fn get_completed_migrations(&mut self) -> Result<VecSerial, Error>;
    fn get_last_completed_migration(&mut self) -> Result<i64, Error>;
    fn add_completed_migration(
        &mut self,
        migration_number: i64,
    ) -> Result<(), Error>;
    fn delete_completed_migration(
        &mut self,
        migration_number: i64,
    ) -> Result<(), Error>;
    fn delete_last_completed_migration(&mut self) -> Result<(), Error>;
    fn migrate(&mut self, query: &str) -> Result<(), Error>;
}
