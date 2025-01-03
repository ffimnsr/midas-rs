use std::fs::{
  self,
  File,
};
use std::io::Write;
use std::iter::Iterator;
use std::path::Path;
use std::thread;
use std::time::Duration;

use anyhow::{
  Context,
  Result as AnyhowResult,
};
use console::style;
use indicatif::{
  ProgressBar,
  ProgressStyle,
};
use indoc::formatdoc;
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
  self,
  MigrationFiles,
  VecStr,
};
use crate::sequel::{
  Driver as SequelDriver,
  VecSerial,
};

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

fn ensure_migration_state_dir_exists() -> AnyhowResult<()> {
  let migration_dir = Path::new(".migrations-state");
  if !migration_dir.exists() {
    fs::create_dir_all(migration_dir).context("Failed to create migrations directory")?;
  }

  Ok(())
}

impl<T: SequelDriver + 'static + ?Sized> Migrator<T> {
  pub fn new(executor: Box<T>, migrations: MigrationFiles) -> Self {
    Self { executor, migrations }
  }

  pub fn create(&mut self, path: &Path, slug: &str) -> AnyhowResult<()> {
    let fixed_slug = slug.to_ascii_lowercase().replace(' ', "_");
    lookup::create_migration_file(path, &fixed_slug)?;
    Ok(())
  }

  pub fn status(&mut self) -> AnyhowResult<()> {
    let completed_migrations = self.executor.get_completed_migrations()?;
    let available_migrations = self.migrations.keys().copied().collect::<VecSerial>();

    if available_migrations.is_empty() {
      println!("There are no available migration files.");
      return Ok(());
    }

    let mut table = Table::new();
    table.set_titles(row![Fbb->"Migration No.", Fbb->"Status", Fbb->"Filename"]);
    table.set_format(*consts::FORMAT_CLEAN);

    available_migrations.iter().for_each(|it| {
      let temp_color = if completed_migrations.contains(it) {
        color::GREEN
      } else {
        color::RED
      };

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

    let msg = style("Available migrations:").bold().cyan();
    println!();
    println!("{msg}");
    println!();

    table.printstd();
    println!();

    let available_migrations_count = available_migrations.len();
    let completed_migrations_count = completed_migrations.len();

    let completed_migrations = style("Completed migrations:").bold().cyan();
    let total_migrations = style("Total migrations:").bold().cyan();
    println!("{completed_migrations}: {completed_migrations_count}");
    println!("{total_migrations}: {available_migrations_count}");

    Ok(())
  }

  pub fn up(&mut self) -> AnyhowResult<()> {
    ensure_migration_state_dir_exists()?;

    let completed_migrations = self.executor.get_completed_migrations()?;
    let available_migrations = self.migrations.keys().copied().collect::<VecSerial>();

    if available_migrations.is_empty() {
      println!("There are no available migration files.");
      return Ok(());
    }

    let filtered: Vec<_> = available_migrations
      .iter()
      .filter(|s| !completed_migrations.contains(s))
      .copied()
      .collect();

    if filtered.is_empty() {
      println!("Migrations are all up-to-date.");
      return Ok(());
    }

    let pb = ProgressBar::new(filtered.len() as u64);
    let tick_interval = Duration::from_millis(80);
    pb.set_style(
      ProgressStyle::with_template("{spinner:.green} [{prefix:.bold.dim}] {wide_msg:.cyan/blue} ")?
        .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏⦿"),
    );
    pb.enable_steady_tick(tick_interval);
    let mut rng = rand::thread_rng();
    for it in &filtered {
      thread::sleep(Duration::from_millis(rng.gen_range(40..300)));
      pb.set_prefix(format!("{it:013}"));

      let migration = self.migrations.get(it).context("Migration file not found")?;
      let filename_parts: Vec<&str> = migration.filename.splitn(2, '_').collect();
      let migration_name = filename_parts
        .get(1)
        .and_then(|s| s.strip_suffix(".sql"))
        .context("Migration name not found")?;

      pb.set_message(format!("Applying migration: {migration_name}"));

      let content_up = migration
        .content_up
        .as_ref()
        .context("Migration content not found")?;
      let content_up = get_content_string!(content_up);

      self.executor.migrate(&content_up, *it)?;
      self.executor.add_completed_migration(*it)?;
      pb.inc(1);
    }
    pb.finish();

    Ok(())
  }

  pub fn down(&mut self) -> AnyhowResult<()> {
    ensure_migration_state_dir_exists()?;

    let completed_migrations = self.executor.get_completed_migrations()?;
    if completed_migrations.is_empty() {
      println!("Migrations table is empty. No need to run down migrations.");
      return Ok(());
    }

    let pb = ProgressBar::new(completed_migrations.len() as u64);
    let tick_interval = Duration::from_millis(80);
    pb.set_style(
      ProgressStyle::with_template("{spinner:.green} [{prefix:.bold.dim}] {wide_msg:.cyan/blue} ")?
        .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏⦿"),
    );
    pb.enable_steady_tick(tick_interval);
    let mut rng = rand::thread_rng();
    for it in completed_migrations.iter().rev() {
      thread::sleep(Duration::from_millis(rng.gen_range(40..300)));
      pb.set_prefix(format!("{it:013}"));
      let migration = self.migrations.get(it).context("Migration file not found")?;
      let filename_parts: Vec<&str> = migration.filename.splitn(2, '_').collect();
      let migration_name = filename_parts
        .get(1)
        .and_then(|s| s.strip_suffix(".sql"))
        .context("Migration name not found")?;

      pb.set_message(format!("Undoing migration: {migration_name}"));

      let content_down = migration
        .content_down
        .as_ref()
        .context("Migration content not found")?;
      let content_down = get_content_string!(content_down);

      self.executor.migrate(&content_down, *it)?;
      if std::env::var("MIGRATIONS_SKIP_LAST").is_err() || !completed_migrations.first().eq(&Some(it)) {
        self.executor.delete_completed_migration(it.to_owned())?;
      }
      pb.inc(1);
    }
    pb.finish();

    Ok(())
  }

  pub fn redo(&mut self) -> AnyhowResult<()> {
    let current = self.executor.get_last_completed_migration()?;
    let current = if current == -1 { 0 } else { current };

    let migration = self
      .migrations
      .get(&current)
      .context("Migration file not found")?;

    let filename_parts: Vec<&str> = migration.filename.splitn(2, '_').collect();
    let migration_name = filename_parts
      .get(1)
      .and_then(|s| s.strip_suffix(".sql"))
      .context("Migration name not found")?;

    let pb = ProgressBar::new(1u64);
    let tick_interval = Duration::from_millis(80);
    pb.set_style(
      ProgressStyle::with_template("{spinner:.green} [{prefix:.bold.dim}] {wide_msg:.cyan/blue} ")?
        .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏⦿"),
    );
    pb.enable_steady_tick(tick_interval);
    pb.set_prefix(format!("{current:013}"));
    pb.tick();

    if current != 0 {
      pb.set_message(format!("Undoing migration: {migration_name}"));
      let content_down = migration
        .content_down
        .as_ref()
        .context("Migration content not found")?;
      let content_down = get_content_string!(content_down);

      self.executor.migrate(&content_down, current)?;
      self.executor.delete_completed_migration(current)?;
    }

    log::trace!("Running the method `redo` {:?}", migration);

    pb.set_message(format!("Applying migration: {migration_name}"));
    let content_up = migration
      .content_up
      .as_ref()
      .context("Migration content not found")?;
    let content_up = get_content_string!(content_up);

    self.executor.migrate(&content_up, current)?;
    self.executor.add_completed_migration(current)?;

    pb.inc(1);
    pb.finish();
    Ok(())
  }

  pub fn revert(&mut self) -> AnyhowResult<()> {
    let migrations_count = self.executor.count_migrations()?;
    let current = self.executor.get_last_completed_migration()?;
    if current == -1 {
      println!("Migrations table is empty. No need to run revert migrations.");
      return Ok(());
    }

    let migration = self
      .migrations
      .get(&current)
      .context("Migration file not found")?;

    let filename_parts: Vec<&str> = migration.filename.splitn(2, '_').collect();
    let migration_name = filename_parts
      .get(1)
      .and_then(|s| s.strip_suffix(".sql"))
      .context("Migration name not found")?;

    let pb = ProgressBar::new(1u64);
    let tick_interval = Duration::from_millis(80);
    pb.set_style(
      ProgressStyle::with_template("{spinner:.green} [{prefix:.bold.dim}] {wide_msg:.cyan/blue} ")?
        .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏⦿"),
    );
    pb.enable_steady_tick(tick_interval);
    pb.set_prefix(format!("{current:013}"));
    pb.tick();

    pb.set_message(format!("Reverting migration: {migration_name}"));
    let content_down = migration
      .content_down
      .as_ref()
      .context("Migration content not found")?;
    let content_down = get_content_string!(content_down);

    self.executor.migrate(&content_down, current)?;
    if migrations_count > 1 || std::env::var("MIGRATIONS_SKIP_LAST").is_err() {
      self.executor.delete_last_completed_migration()?;
    }

    pb.inc(1);
    pb.finish();
    Ok(())
  }

  pub fn init(&self, source_path: &Path, source: &str, db_url: &str) -> AnyhowResult<()> {
    let filename = ".env.midas";
    let filepath = std::env::current_dir()?.join(filename);

    log::trace!("Creating new env file: {:?}", filepath);
    let mut f = File::create(filepath)?;
    let contents = formatdoc! {"
      DATABASE_URL={}
      MIGRATIONS_ROOT={}
    ", db_url, source};
    f.write_all(contents.as_bytes())?;
    f.sync_all()?;

    log::trace!("Creating new migrations directory: {:?}", source_path);
    fs::create_dir_all(source_path)?;
    Ok(())
  }

  pub fn drop(&mut self, db_url: &str) -> AnyhowResult<()> {
    let db_url = Url::parse(db_url).ok();
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
