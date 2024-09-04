use std::fs::{self, File};
use std::io::Write;
use std::iter::Iterator;
use std::path::Path;

use log::{trace, debug};
use indoc::formatdoc;
use url::Url;

use crate::lookup::{self, MigrationFiles, VecStr};
use crate::sequel::{Driver as SequelDriver, VecSerial};

macro_rules! get_content_string {
    ($content: ident) => {
        $content
            .iter()
            .filter(|&l| l != "")
            .map(|s| s.to_owned())
            .collect::<VecStr>()
            .join("\n")
    };
}

pub struct Migrator<T: ?Sized> {
    executor: Box<T>,
    migrations: MigrationFiles,
}

impl<T: SequelDriver + 'static + ?Sized> Migrator<T> {
    pub fn new(executor: Box<T>, migrations: MigrationFiles) -> Self {
        Self { executor, migrations }
    }

    pub fn create(
        &mut self,
        path: &Path,
        slug: &str,
    ) -> Result<(), super::GenericError> {
        let fixed_slug = slug.to_ascii_lowercase().replace(' ', "_");
        lookup::create_migration_file(path, &fixed_slug)?;
        Ok(())
    }

    pub fn status(&mut self) -> Result<(), super::GenericError> {
        let completed_migrations = self.executor.get_completed_migrations()?;
        let available_migrations =
            self.migrations.keys().copied().collect::<VecSerial>();

        if available_migrations.is_empty() {
            println!("There are no available migration files.");
            return Ok(());
        }

        println!("Building active migrations list...");
        if completed_migrations.is_empty() {
            for it in &available_migrations {
                println!("{it:013} = Inactive");
            }

            return Ok(());
        }

        for it in &available_migrations {
            let does_have = if completed_migrations.contains(it) {
                "Active"
            } else {
                "Inactive"
            };
            println!("{it:013} = {does_have}");
        }

        Ok(())
    }

    pub fn up(&mut self) -> Result<(), super::GenericError> {
        let completed_migrations = self.executor.get_completed_migrations()?;
        let available_migrations =
            self.migrations.keys().copied().collect::<VecSerial>();

        if available_migrations.is_empty() {
            println!("There are no available migration files.");
            return Ok(());
        }

        let filtered = available_migrations
            .iter()
            .filter(|s| !completed_migrations.contains(s))
            .map(std::borrow::ToOwned::to_owned)
            .collect::<VecSerial>();

        if filtered.is_empty() {
            println!("Migrations are all up-to-date.");
            return Ok(());
        }

        for it in &filtered {
            println!("[{it:013}] Applying migration in the database.");
            let migration = self.migrations.get(it).unwrap();
            let content_up = migration.content_up.as_ref().unwrap();
            let content_up = get_content_string!(content_up);

            trace!("Running the following up query: {:?}", content_up);

            self.executor.migrate(&content_up)?;
            self.executor.add_completed_migration(it.to_owned())?;
        }

        Ok(())
    }

    pub fn down(&mut self) -> Result<(), super::GenericError> {
        let completed_migrations = self.executor.get_completed_migrations()?;
        if completed_migrations.is_empty() {
            println!(
                "Migrations table is empty. No need to run down migrations."
            );
            return Ok(());
        }

        for it in completed_migrations.iter().rev() {
            println!("[{it:013}] Undo migration from database.");
            let migration = self.migrations.get(it).unwrap();
            let content_down = migration.content_down.as_ref().unwrap();
            let content_down = get_content_string!(content_down);

            trace!("Running the following down query: {:?}", content_down);

            self.executor.migrate(&content_down)?;

            if std::env::var("MIGRATIONS_SKIP_LAST").is_ok() {
                if !completed_migrations.first().eq(&Some(it)) {
                    self.executor.delete_completed_migration(it.to_owned())?;
                }
            } else {
                self.executor.delete_completed_migration(it.to_owned())?;
            }
        }

        Ok(())
    }

    pub fn redo(&mut self) -> Result<(), super::GenericError> {
        let mut current = self.executor.get_last_completed_migration()?;

        let current_state = current;
        if current_state == -1 {
            current = 0;
        }

        let migration = self.migrations.get(&current).unwrap();

        if current_state != -1 {
            println!(
                "[{current:013}] Clearing recent migration from database."
            );
            let content_down = migration.content_down.as_ref().unwrap();
            let content_down = get_content_string!(content_down);

            self.executor.migrate(&content_down)?;
            self.executor.delete_completed_migration(current)?;
        }

        trace!("Running the method `redo` {:?}", migration);

        println!("[{current:013}] Applying recent migration in the database.");
        let content_up = migration.content_up.as_ref().unwrap();
        let content_up = get_content_string!(content_up);

        self.executor.migrate(&content_up)?;
        self.executor.add_completed_migration(current)?;

        Ok(())
    }

    pub fn revert(&mut self) -> Result<(), super::GenericError> {
        let migrations_count = self.executor.count_migrations()?;
        let current = self.executor.get_last_completed_migration()?;
        if current == -1 {
            println!(
                "Migrations table is empty. No need to run revert migrations."
            );
            return Ok(());
        }

        println!("[{current:013}] Reverting actions from last migration.");
        let migration = self.migrations.get(&current).unwrap();
        let content_down = migration.content_down.as_ref().unwrap();
        let content_down = get_content_string!(content_down);

        self.executor.migrate(&content_down)?;
        if std::env::var("MIGRATIONS_SKIP_LAST").is_ok() {
            if migrations_count > 1 {
                self.executor.delete_last_completed_migration()?;
            }
        } else {
            self.executor.delete_last_completed_migration()?;
        }
        Ok(())
    }

    pub fn init(&self, source_path: &Path, source: &str, dsn: &str) -> Result<(), super::GenericError> {
        let filename = ".env.midas";
        let filepath = std::env::current_dir()?.join(filename);

        debug!("Creating new env file: {:?}", filepath);
        let mut f = File::create(filepath)?;
        let contents = formatdoc!("
            DSN={}
            MIGRATIONS_ROOT={}
        ", dsn, source);
        f.write_all(contents.as_bytes())?;
        f.sync_all()?;

        debug!("Creating new migrations directory: {:?}", source_path);
        fs::create_dir_all(source_path)?;
        Ok(())
    }

    pub fn drop(&mut self, raw_db_url: &str) -> Result<(), super::GenericError> {
        let db_url = Url::parse(raw_db_url).ok();
        if let Some(db_url) = db_url {
            let db_name = db_url.path().trim_start_matches('/');
            let _ = self.executor.drop_database(db_name)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_create() {}
}
