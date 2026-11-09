//! SQLx interoperability commands.

use std::path::Path;

use anyhow::{
  Context as _,
  Result,
};
use clap::{
  Arg,
  ArgMatches,
  Command,
};
use midas::{
  migration,
  sqlx,
};

use crate::commands::Context;

/// Register the `sqlx` command group.
pub fn register(app: Command) -> Command {
  app.subcommand(
    Command::new("sqlx")
      .about("SQLx interoperability commands")
      .arg_required_else_help(true)
      .subcommand(export_command())
      .subcommand(import_command()),
  )
}

/// Whether this module owns the given subcommand name.
pub fn owns(name: &str) -> bool {
  name == "sqlx"
}

/// Dispatch the command.
pub fn dispatch(name: &str, matches: &ArgMatches, context: &Context) -> Result<()> {
  let Some((subcommand, sub)) = matches.subcommand() else {
    anyhow::bail!("`{name}` requires a subcommand");
  };

  match subcommand {
    "export" => export(sub, context),
    "import" => import(sub, context),
    other => anyhow::bail!("unknown sqlx subcommand `{other}`"),
  }
}

fn export_command() -> Command {
  Command::new("export")
    .about("Export Midas migrations as SQLx migration files")
    .arg(
      Arg::new("out")
        .long("out")
        .value_name("dir")
        .help("Directory to write SQLx migrations into")
        .required(true)
        .num_args(1),
    )
}

fn import_command() -> Command {
  Command::new("import")
    .about("Import SQLx migration files as Midas migrations")
    .arg(
      Arg::new("from")
        .long("from")
        .value_name("dir")
        .help("Directory containing SQLx migrations")
        .required(true)
        .num_args(1),
    )
    .arg(
      Arg::new("out")
        .long("out")
        .value_name("dir")
        .help("Directory to write Midas migrations into")
        .required(true)
        .num_args(1),
    )
}

fn export(matches: &ArgMatches, context: &Context) -> Result<()> {
  let out = matches
    .get_one::<String>("out")
    .context("output directory is required")?;
  let migrations = migration::discover(&context.config.migrations_dir)?;
  let written = sqlx::export_sqlx(&migrations, Path::new(out))?;
  println!("Exported {} migration file(s) to `{out}`.", written.len());
  Ok(())
}

fn import(matches: &ArgMatches, context: &Context) -> Result<()> {
  let from = matches
    .get_one::<String>("from")
    .context("input directory is required")?;
  let out = matches
    .get_one::<String>("out")
    .context("output directory is required")?;
  let migrations = sqlx::import_sqlx_with(Path::new(from), &context.config.sqlx)?;
  let written = sqlx::write_midas(&migrations, Path::new(out))?;
  println!(
    "Imported {} migration(s) into `{out}` ({} file(s)).",
    migrations.len(),
    written.len()
  );
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn owns_only_sqlx() {
    assert!(owns("sqlx"));
    assert!(!owns("generate"));
  }
}
