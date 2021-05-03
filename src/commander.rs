use std::iter::Iterator;
use std::path::Path;

use failure::Error;
use log::debug;

use crate::lookup::{self, MigrationFiles, VecStr};
use crate::sequel::{SequelDriver, VecSerial};

macro_rules! get_content_string {
    ($content: ident) => {
        $content.iter().filter(|&l| l != "").map(|s| s.to_owned()).collect::<VecStr>().join("\n")
    };
}

pub struct Migrator<T> {
    executor: Box<T>,
    migrations: MigrationFiles,
}

impl<T: SequelDriver + 'static> Migrator<T> {
    pub fn new(executor: Box<T>, migrations: MigrationFiles) -> Self {
        Self { executor, migrations }
    }

    pub fn create(&mut self, path: &Path, slug: &str) -> Result<(), Error> {
        let fixed_slug = slug.to_ascii_lowercase().replace(" ", "_");
        let _ = lookup::create_migration_file(path, &fixed_slug)?;

        Ok(())
    }

    pub fn status(&mut self) -> Result<(), Error> {
        let completed_migrations = self.executor.get_completed_migrations()?;
        let available_migrations = self.migrations.keys().cloned().collect::<VecSerial>();

        if available_migrations.is_empty() {
            println!("There are no available migration files.");
            return Ok(());
        }

        println!("Building active migrations list...");
        if completed_migrations.is_empty() {
            for it in available_migrations.iter() {
                println!("{:013} = Inactive", it);
            }

            return Ok(());
        }

        for it in available_migrations.iter() {
            let does_have = match completed_migrations.contains(it) {
                true => "Active",
                _ => "Inactive",
            };
            println!("{:013} = {}", it, does_have);
        }

        Ok(())
    }

    pub fn up(&mut self) -> Result<(), Error> {
        let completed_migrations = self.executor.get_completed_migrations()?;
        let available_migrations = self.migrations.keys().cloned().collect::<VecSerial>();

        if available_migrations.is_empty() {
            println!("There are no available migration files.");
            return Ok(());
        }

        let filtered = available_migrations
            .iter()
            .filter(|s| completed_migrations.contains(s) == false)
            .map(|s| s.to_owned())
            .collect::<VecSerial>();

        if filtered.is_empty() {
            println!("Migrations are all up-to-date.");
            return Ok(());
        }

        for it in filtered.iter() {
            println!("[{:013}] Applying migration in the database.", it);
            let migration = self.migrations.get(&it).unwrap();
            let content_up = migration.content_up.as_ref().unwrap();
            let content_up = get_content_string!(content_up);

            debug!("{:?}", content_up);

            self.executor.migrate(&content_up)?;
            self.executor.add_completed_migration(it.to_owned())?;
        }

        Ok(())
    }

    pub fn down(&mut self) -> Result<(), Error> {
        let completed_migrations = self.executor.get_completed_migrations()?;
        if completed_migrations.is_empty() {
            println!("Migrations table is empty. No need to run down migrations.");
            return Ok(());
        }

        for it in completed_migrations.iter().rev() {
            println!("[{:013}] Undo migration from database.", it);
            let migration = self.migrations.get(&it).unwrap();
            let content_down = migration.content_down.as_ref().unwrap();
            let content_down = get_content_string!(content_down);

            debug!("{:?}", content_down);

            self.executor.migrate(&content_down)?;
            self.executor.delete_completed_migration(it.to_owned())?;
        }

        Ok(())
    }

    pub fn redo(&mut self) -> Result<(), Error> {
        let mut current = self.executor.get_last_completed_migration()?;

        let current_state = current;
        if current == -1 {
            current = 0;
        }

        let migration = self.migrations.get(&current).unwrap();

        if current_state != -1 {
            println!("[{:013}] Clearing recent migration from database.", current);
            let content_down = migration.content_down.as_ref().unwrap();
            let content_down = get_content_string!(content_down);

            self.executor.migrate(&content_down)?;
            self.executor.delete_completed_migration(current)?;
        }

        debug!("{:?}", migration);

        println!("[{:013}] Applying recent migration in the database.", current);
        let content_up = migration.content_up.as_ref().unwrap();
        let content_up = get_content_string!(content_up);

        self.executor.migrate(&content_up)?;
        self.executor.add_completed_migration(current)?;

        Ok(())
    }

    pub fn revert(&mut self) -> Result<(), Error> {
        let current = self.executor.get_last_completed_migration()?;
        if current == -1 {
            println!("Migrations table is empty. No need to run revert migrations.");
            return Ok(());
        }

        println!("[{:013}] Reverting actions from last migration.", current);
        let migration = self.migrations.get(&current).unwrap();
        let content_down = migration.content_down.as_ref().unwrap();
        let content_down = get_content_string!(content_down);

        self.executor.migrate(&content_down)?;
        self.executor.delete_last_completed_migration()?;
        Ok(())
    }

    pub fn setup(&self) -> Result<(), Error> {
        println!("Not Useable. Currently this is a placeholder command.");
        Ok(())
    }

    pub fn drop(&self) -> Result<(), Error> {
        println!("Not Useable. Currently this is a placeholder command.");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_create() {}
}
