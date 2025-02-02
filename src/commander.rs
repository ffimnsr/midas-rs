use std::iter::Iterator;
use std::thread;
use std::time::Duration;

use anyhow::{
  Context,
  Result as AnyhowResult,
};
use console::style;
use indicatif::ProgressBar;
use prettytable::format::consts;
use prettytable::{
  color,
  row,
  Attr,
  Cell,
  Row,
  Table,
};
use rand::Rng;
use url::Url;

use crate::lookup::{
  MigrationFiles,
  VecStr,
};
use crate::sequel::{
  Driver as SequelDriver,
  VecSerial,
};
use crate::{
  ensure_migration_state_dir_exists,
  progress_style,
};

/// Get the content string
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

/// The migrator struct
pub struct Migrator<T: ?Sized> {
  /// The executor instance
  executor: Box<T>,

  /// The migration files
  migrations: MigrationFiles,
}

impl<T: SequelDriver + 'static + ?Sized> Migrator<T> {
  /// Create a new migrator instance
  pub fn new(executor: Box<T>, migrations: MigrationFiles) -> Self {
    Self { executor, migrations }
  }

  /// Run the status command to show the current status of migrations
  pub fn status(&mut self) -> AnyhowResult<()> {
    // Get the completed migrations
    let completed_migrations = self.executor.get_completed_migrations()?;
    let available_migrations = self.migrations.keys().copied().collect::<VecSerial>();

    // If there are no available migrations, print a message and return
    if available_migrations.is_empty() {
      println!("There are no available migration files.");
      return Ok(());
    }

    // Create a new table instance
    let mut table = Table::new();
    table.set_titles(row![Fbb->"Migration No.", Fbb->"Status", Fbb->"Filename"]);
    table.set_format(*consts::FORMAT_CLEAN);

    // Iterate over the available migrations
    available_migrations.iter().for_each(|it| {
      // Set the color based on whether the migration is completed
      let temp_color = if completed_migrations.contains(it) {
        color::GREEN
      } else {
        color::RED
      };

      // Get the migration number and the migration file
      let migration_no = format!("{it:013}");
      if let Some(migration) = self.migrations.get(it) {
        let filename = &migration.filename;

        table.add_row(Row::new(vec![
          Cell::new(&migration_no).with_style(Attr::Bold),
          Cell::new(if temp_color == color::GREEN {
            "Active"
          } else {
            "Inactive"
          })
          .with_style(Attr::ForegroundColor(temp_color)),
          Cell::new(filename).with_style(Attr::ForegroundColor(temp_color)),
        ]));
      }
    });

    // Print the table
    let msg = style("Available migrations:").bold().cyan();
    println!();
    println!("{msg}");
    println!();
    table.printstd();
    println!();

    // Print the completed migrations count and the available migrations count
    let available_migrations_count = available_migrations.len();
    let completed_migrations_count = completed_migrations.len();
    let completed_migrations = style("Completed migrations:").bold().cyan();
    let total_migrations = style("Total migrations:").bold().cyan();
    println!("{completed_migrations}: {completed_migrations_count}");
    println!("{total_migrations}: {available_migrations_count}");

    Ok(())
  }

  /// Run up migrations
  pub fn up(&mut self) -> AnyhowResult<()> {
    // Ensure the migration state directory exists
    ensure_migration_state_dir_exists()?;

    // Get the completed migrations
    let completed_migrations = self.executor.get_completed_migrations()?;
    let available_migrations = self.migrations.keys().copied().collect::<VecSerial>();

    // If there are no available migrations, print a message and return
    if available_migrations.is_empty() {
      println!("There are no available migration files.");
      return Ok(());
    }

    // Filter the available migrations
    let filtered: Vec<_> = available_migrations
      .iter()
      .filter(|s| !completed_migrations.contains(s))
      .copied()
      .collect();

    // If there are no filtered migrations, print a message and return
    if filtered.is_empty() {
      println!("Migrations are all up-to-date.");
      return Ok(());
    }

    // Create a new progress bar instance
    let pb = ProgressBar::new(filtered.len() as u64);
    let tick_interval = Duration::from_millis(80);
    pb.set_style(progress_style()?);
    pb.enable_steady_tick(tick_interval);
    let mut rng = rand::thread_rng();

    // Iterate over the filtered migrations
    for it in &filtered {
      // Sleep for a random duration between 40 and 300 milliseconds
      // to simulate a delay and make the progress bar more interesting
      thread::sleep(Duration::from_millis(rng.gen_range(40..300)));

      // Set the progress bar prefix
      pb.set_prefix(format!("{it:013}"));

      // Get the migration file
      let migration = self.migrations.get(it).context("Migration file not found")?;
      let filename_parts: Vec<&str> = migration.filename.splitn(2, '_').collect();
      let migration_name = filename_parts
        .get(1)
        .and_then(|s| s.strip_suffix(".sql"))
        .context("Migration name not found")?;

      // Set the progress bar message
      pb.set_message(format!("Applying migration: {migration_name}"));

      // Get the migration up content and convert it to a string
      let content_up = migration
        .content_up
        .as_ref()
        .context("Migration content not found")?;
      let content_up = get_content_string!(content_up);

      // Run the migration content
      self.executor.migrate(&content_up, *it)?;

      // Add the completed migration
      self.executor.add_completed_migration(*it)?;
      pb.inc(1);
    }
    pb.finish();

    Ok(())
  }

  /// Run up migrations up to a specific migration number
  pub fn upto(&mut self, migration_number: i64) -> AnyhowResult<()> {
    // Ensure the migration state directory exists
    ensure_migration_state_dir_exists()?;

    // Get the completed migrations
    let completed_migrations = self.executor.get_completed_migrations()?;
    let available_migrations = self.migrations.keys().copied().collect::<VecSerial>();

    // If there are no available migrations, print a message and return
    if available_migrations.is_empty() {
      println!("There are no available migration files.");
      return Ok(());
    }

    // Filter the available migrations
    let filtered: Vec<_> = available_migrations
      .iter()
      .filter(|s| !completed_migrations.contains(s))
      .filter(|s| **s <= migration_number)
      .copied()
      .collect();

    // If there are no filtered migrations, print a message and return
    if filtered.is_empty() {
      println!("Migrations are all up-to-date.");
      return Ok(());
    }

    // Create a new progress bar instance
    let pb = ProgressBar::new(filtered.len() as u64);
    let tick_interval = Duration::from_millis(80);
    pb.set_style(progress_style()?);
    pb.enable_steady_tick(tick_interval);
    let mut rng = rand::thread_rng();

    // Iterate over the filtered migrations
    for it in &filtered {
      // Sleep for a random duration between 40 and 300 milliseconds
      // to simulate a delay and make the progress bar more interesting
      thread::sleep(Duration::from_millis(rng.gen_range(40..300)));
      pb.set_prefix(format!("{it:013}"));

      // Get the migration file
      let migration = self.migrations.get(it).context("Migration file not found")?;
      let filename_parts: Vec<&str> = migration.filename.splitn(2, '_').collect();
      let migration_name = filename_parts
        .get(1)
        .and_then(|s| s.strip_suffix(".sql"))
        .context("Migration name not found")?;

      // Set the progress bar message
      pb.set_message(format!("Applying migration: {migration_name}"));

      // Get the migration up content and convert it to a string
      let content_up = migration
        .content_up
        .as_ref()
        .context("Migration content not found")?;
      let content_up = get_content_string!(content_up);

      // Run the migration content
      self.executor.migrate(&content_up, *it)?;
      self.executor.add_completed_migration(*it)?;
      pb.inc(1);
    }
    pb.finish();

    Ok(())
  }

  /// Run down migrations
  pub fn down(&mut self) -> AnyhowResult<()> {
    // Ensure the migration state directory exists
    ensure_migration_state_dir_exists()?;

    // Get the completed migrations
    let completed_migrations = self.executor.get_completed_migrations()?;
    if completed_migrations.is_empty() {
      println!("Migrations table is empty. No need to run down migrations.");
      return Ok(());
    }

    // Create a new progress bar instance
    let pb = ProgressBar::new(completed_migrations.len() as u64);
    let tick_interval = Duration::from_millis(80);
    pb.set_style(progress_style()?);
    pb.enable_steady_tick(tick_interval);
    let mut rng = rand::thread_rng();

    // Iterate over the completed migrations
    for it in completed_migrations.iter().rev() {
      // Sleep for a random duration between 40 and 300 milliseconds
      // to simulate a delay and make the progress bar more interesting
      thread::sleep(Duration::from_millis(rng.gen_range(40..300)));
      pb.set_prefix(format!("{it:013}"));

      // Get the migration file
      let migration = self.migrations.get(it).context("Migration file not found")?;
      let filename_parts: Vec<&str> = migration.filename.splitn(2, '_').collect();
      let migration_name = filename_parts
        .get(1)
        .and_then(|s| s.strip_suffix(".sql"))
        .context("Migration name not found")?;

      // Set the progress bar message
      pb.set_message(format!("Undoing migration: {migration_name}"));

      // Get the migration down content and convert it to a string
      let content_down = migration
        .content_down
        .as_ref()
        .context("Migration content not found")?;
      let content_down = get_content_string!(content_down);

      // Run the migration content down
      self.executor.migrate(&content_down, *it)?;
      if std::env::var("MIGRATIONS_SKIP_LAST").is_err() || !completed_migrations.first().eq(&Some(it)) {
        self.executor.delete_completed_migration(it.to_owned())?;
      }
      pb.inc(1);
    }
    pb.finish();

    Ok(())
  }

  /// Redo the last migration
  /// This is equivalent to running down and then up
  /// on the last completed migration
  /// If there are no completed migrations, this will run the first migration
  pub fn redo(&mut self) -> AnyhowResult<()> {
    // Get the last completed migration
    let current = self.executor.get_last_completed_migration()?;
    let current = if current == -1 { 0 } else { current };

    // Get the migration file
    let migration = self
      .migrations
      .get(&current)
      .context("Migration file not found")?;

    // Get the migration name
    let filename_parts: Vec<&str> = migration.filename.splitn(2, '_').collect();
    let migration_name = filename_parts
      .get(1)
      .and_then(|s| s.strip_suffix(".sql"))
      .context("Migration name not found")?;

    // Create a new progress bar instance
    let pb = ProgressBar::new(1u64);
    let tick_interval = Duration::from_millis(80);
    pb.set_style(progress_style()?);
    pb.enable_steady_tick(tick_interval);
    pb.set_prefix(format!("{current:013}"));
    pb.tick();

    // If the current migration is not 0, run down
    if current != 0 {
      pb.set_message(format!("Undoing migration: {migration_name}"));

      // Get the migration down content and convert it to a string
      let content_down = migration
        .content_down
        .as_ref()
        .context("Migration content not found")?;
      let content_down = get_content_string!(content_down);

      // Run the migration down
      self.executor.migrate(&content_down, current)?;
      self.executor.delete_completed_migration(current)?;
    }

    log::trace!("Running the method `redo` {:?}", migration);

    // Set the progress bar message
    pb.set_message(format!("Applying migration: {migration_name}"));

    // Get the migration up content and convert it to a string
    let content_up = migration
      .content_up
      .as_ref()
      .context("Migration content not found")?;
    let content_up = get_content_string!(content_up);

    // Run the migration up
    self.executor.migrate(&content_up, current)?;
    self.executor.add_completed_migration(current)?;

    pb.inc(1);
    pb.finish();
    Ok(())
  }

  /// Revert the last migration
  /// This is equivalent to running down on the last completed migration
  /// If there are no completed migrations, this will do nothing
  pub fn revert(&mut self) -> AnyhowResult<()> {
    // Get the migrations count
    let migrations_count = self.executor.count_migrations()?;

    // Get the last completed migration
    let current = self.executor.get_last_completed_migration()?;

    // If there are no completed migrations, do nothing
    if current == -1 {
      println!("Migrations table is empty. No need to run revert migrations.");
      return Ok(());
    }

    // Get the migration file
    let migration = self
      .migrations
      .get(&current)
      .context("Migration file not found")?;

    // Get the migration name
    let filename_parts: Vec<&str> = migration.filename.splitn(2, '_').collect();
    let migration_name = filename_parts
      .get(1)
      .and_then(|s| s.strip_suffix(".sql"))
      .context("Migration name not found")?;

    // Create a new progress bar instance
    let pb = ProgressBar::new(1u64);
    let tick_interval = Duration::from_millis(80);
    pb.set_style(progress_style()?);
    pb.enable_steady_tick(tick_interval);
    pb.set_prefix(format!("{current:013}"));
    pb.tick();
    pb.set_message(format!("Reverting migration: {migration_name}"));

    // Get the migration down content and convert it to a string
    let content_down = migration
      .content_down
      .as_ref()
      .context("Migration content not found")?;
    let content_down = get_content_string!(content_down);

    // Run the migration down
    self.executor.migrate(&content_down, current)?;

    // Delete the last completed migration
    if migrations_count > 1 || std::env::var("MIGRATIONS_SKIP_LAST").is_err() {
      self.executor.delete_last_completed_migration()?;
    }

    pb.inc(1);
    pb.finish();
    Ok(())
  }

  /// Drop the database
  /// This will drop the database specified in the database URL
  /// The database URL should be in the format `dialect://user:password@host:port/database`
  /// For example, `postgres://user:password@localhost:5432/database`
  pub fn drop(&mut self, db_url: &str) -> AnyhowResult<()> {
    let db_url = Url::parse(db_url).ok();

    // If the database URL is not found, return an error
    if let Some(db_url) = db_url {
      let db_name = db_url.path().trim_start_matches('/');
      self.executor.drop_database(db_name)?;
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  #[test]
  fn test_create() {}
}
