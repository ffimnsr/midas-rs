//! CLI command registration and dispatch.
//!
//! Every command module owns exactly one file so later phases can edit their
//! own scope without touching unrelated commands.

pub mod db_lifecycle;
pub mod db_schema;
pub mod doctor;
pub mod generate;
pub mod migrate;
pub mod migrate_dev;
pub mod schema;
pub mod sqlx;

use anyhow::Result;
use clap::{
  ArgMatches,
  Command,
};
use midas::config::Config;

/// Runtime context shared by command handlers.
#[derive(Debug, Clone)]
pub struct Context {
  /// Fully resolved configuration.
  pub config: Config,
  /// Print SQL instead of executing it.
  pub dry_run: bool,
  /// Skip advisory locking.
  pub no_lock: bool,
  /// Ignore applied migrations that are no longer present on disk.
  pub ignore_missing: bool,
}

impl Context {
  /// Build a context from resolved configuration.
  pub fn new(config: Config) -> Self {
    Self {
      config,
      dry_run: false,
      no_lock: false,
      ignore_missing: false,
    }
  }

  /// Set global runner flags.
  pub fn with_flags(mut self, dry_run: bool, no_lock: bool, ignore_missing: bool) -> Self {
    self.dry_run = dry_run;
    self.no_lock = no_lock;
    self.ignore_missing = ignore_missing;
    self
  }
}

/// Register every command group on the root command.
pub fn register(app: Command) -> Command {
  let app = migrate::register(app);
  let app = migrate_dev::register(app);
  let app = schema::register(app);
  let app = app.subcommand(db_command());
  let app = generate::register(app);
  let app = doctor::register(app);
  sqlx::register(app)
}

/// Dispatch a parsed subcommand.
pub fn dispatch(matches: &ArgMatches, context: &Context) -> Result<()> {
  let Some((name, sub)) = matches.subcommand() else {
    return Ok(());
  };

  if migrate::owns(name) {
    return migrate::dispatch(name, sub, context);
  }
  if migrate_dev::owns(name) {
    return migrate_dev::dispatch(name, sub, context);
  }
  if schema::owns(name) {
    return schema::dispatch(name, sub, context);
  }
  if generate::owns(name) {
    return generate::dispatch(name, sub, context);
  }
  if doctor::owns(name) {
    return doctor::dispatch(name, sub, context);
  }
  if sqlx::owns(name) {
    return sqlx::dispatch(name, sub, context);
  }
  if name == "db" {
    return dispatch_db(sub, context);
  }

  anyhow::bail!("unknown subcommand `{name}`")
}

fn db_command() -> Command {
  let mut command = Command::new("db")
    .about("Database lifecycle and schema commands")
    .arg_required_else_help(true);

  for subcommand in db_lifecycle::subcommands() {
    command = command.subcommand(subcommand);
  }
  for subcommand in db_schema::subcommands() {
    command = command.subcommand(subcommand);
  }

  command
}

fn dispatch_db(matches: &ArgMatches, context: &Context) -> Result<()> {
  let Some((name, sub)) = matches.subcommand() else {
    return Ok(());
  };

  if db_lifecycle::owns(name) {
    return db_lifecycle::dispatch(name, sub, context);
  }
  if db_schema::owns(name) {
    return db_schema::dispatch(name, sub, context);
  }

  anyhow::bail!("unknown db subcommand `{name}`")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn root_registers_expected_subcommands() {
    let app = register(Command::new("midas"));
    let names = app
      .get_subcommands()
      .map(|command| command.get_name())
      .collect::<Vec<_>>();

    for expected in [
      "init", "run", "info", "db", "schema", "generate", "sqlx", "doctor",
    ] {
      assert!(names.contains(&expected), "missing `{expected}` in {names:?}");
    }
  }

  #[test]
  fn db_registers_lifecycle_and_schema_commands() {
    let db = db_command();
    let names = db
      .get_subcommands()
      .map(|command| command.get_name())
      .collect::<Vec<_>>();

    for expected in ["create", "drop", "reset", "exists", "pull", "push", "diff"] {
      assert!(names.contains(&expected), "missing `{expected}` in {names:?}");
    }
  }
}
