//! Database lifecycle commands composed under `midas db`.

use anyhow::{
  Context as _,
  Result,
};
use clap::{
  Arg,
  ArgMatches,
  Command,
};
use midas::commander::{
  Migrator,
  MigratorOptions,
};
use midas::config::Config;
use midas::{
  migration,
  sequel,
};

use crate::commands::Context;

/// Subcommands owned by the lifecycle module.
pub fn subcommands() -> Vec<Command> {
  vec![
    Command::new("create")
      .about("Create the configured database")
      .arg(
        Arg::new("from-migrations")
          .long("from-migrations")
          .value_name("dir")
          .help("Apply migrations from this directory after creating the database")
          .num_args(1),
      ),
    Command::new("drop").about("Drop the configured database"),
    Command::new("reset").about("Drop and recreate the configured database"),
    Command::new("exists").about("Check whether the configured database exists"),
  ]
}

/// Whether this module owns the given subcommand name.
pub fn owns(name: &str) -> bool {
  matches!(name, "create" | "drop" | "reset" | "exists")
}

/// Dispatch a lifecycle subcommand.
pub fn dispatch(name: &str, matches: &ArgMatches, context: &Context) -> Result<()> {
  match name {
    "create" => create(context, matches.get_one::<String>("from-migrations")),
    "drop" => drop_database(context),
    "reset" => {
      drop_database(context)?;
      create(context, None)
    },
    "exists" => exists(context),
    other => anyhow::bail!("unknown db lifecycle subcommand `{other}`"),
  }
}

fn create(context: &Context, from_migrations: Option<&String>) -> Result<()> {
  let database_url = context.config.require_database_url()?;
  let name = sequel::database_name(database_url)?;
  let mut admin = sequel::connect_admin(&context.config)?;

  if admin.database_exists(&name)? {
    println!("Database `{name}` already exists.");
  } else {
    admin
      .create_database(&name)
      .with_context(|| format!("failed to create database `{name}`"))?;
    println!("Created database `{name}`.");
  }

  if let Some(dir) = from_migrations {
    let mut config = context.config.clone();
    config.migrations_dir = dir.into();
    apply_migrations(&config, context)?;
  }

  Ok(())
}

fn drop_database(context: &Context) -> Result<()> {
  let database_url = context.config.require_database_url()?;
  let name = sequel::database_name(database_url)?;
  let mut admin = sequel::connect_admin(&context.config)?;

  if !admin.database_exists(&name)? {
    println!("Database `{name}` does not exist.");
    return Ok(());
  }

  admin
    .force_drop_database(&name)
    .with_context(|| format!("failed to drop database `{name}`"))?;
  println!("Dropped database `{name}`.");
  Ok(())
}

fn exists(context: &Context) -> Result<()> {
  let database_url = context.config.require_database_url()?;
  let name = sequel::database_name(database_url)?;
  let mut admin = sequel::connect_admin(&context.config)?;
  let exists = admin.database_exists(&name)?;
  if exists {
    println!("Database `{name}` exists.");
  } else {
    println!("Database `{name}` does not exist.");
  }
  Ok(())
}

fn apply_migrations(config: &Config, context: &Context) -> Result<()> {
  let migrations = migration::discover(&config.migrations_dir)?;
  let backend = sequel::connect(config)?;
  let options = MigratorOptions {
    dry_run: context.dry_run,
    no_lock: context.no_lock,
    transaction: config.transaction,
    quiet: false,
    ignore_missing: context.ignore_missing,
  };
  let mut migrator = Migrator::new(backend, migrations).with_options(options);
  migrator.up(None)?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn owns_lifecycle_commands() {
    for name in ["create", "drop", "reset", "exists"] {
      assert!(owns(name));
    }
    assert!(!owns("pull"));
  }
}
