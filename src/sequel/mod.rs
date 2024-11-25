use anyhow::Result as AnyhowResult;

pub mod error;
pub mod mysql;
pub mod postgres;
pub mod sqlite;

pub type VecSerial = Vec<i64>;

pub trait Driver {
    fn ensure_midas_schema(&mut self) -> AnyhowResult<()>;
    fn drop_migration_table(&mut self) -> AnyhowResult<()>;
    fn drop_database_schemas(&mut self) -> AnyhowResult<()>;
    fn count_migrations(&mut self) -> AnyhowResult<i64>;
    fn get_completed_migrations(&mut self) -> AnyhowResult<VecSerial>;
    fn get_last_completed_migration(&mut self) -> AnyhowResult<i64>;
    fn add_completed_migration(
        &mut self,
        migration_number: i64,
    ) -> AnyhowResult<()>;
    fn delete_completed_migration(
        &mut self,
        migration_number: i64,
    ) -> AnyhowResult<()>;
    fn delete_last_completed_migration(&mut self) -> AnyhowResult<()>;
    fn migrate(&mut self, query: &str) -> AnyhowResult<()>;
    fn get_db_table_state(&mut self, table_name: &str) -> AnyhowResult<()>;
    // fn get_db_function_state(&self, function_name: &str) -> AnyhowResult<()>;
    // fn get_db_trigger_state(&self, trigger_name: &str) -> AnyhowResult<()>;
    fn db_name(&self) -> &str;
}
