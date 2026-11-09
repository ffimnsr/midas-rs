//! Diagnostics for project and database state.

use anyhow::Result;
use clap::{
  Arg,
  ArgMatches,
  Command,
};
use midas::sqlx;

use crate::commands::Context;

/// Register the `doctor` command.
pub fn register(app: Command) -> Command {
  app.subcommand(
    Command::new("doctor")
      .about("Diagnose database and migration state")
      .arg(
        Arg::new("sqlx")
          .long("sqlx")
          .help("Check for a SQLx-managed metadata table")
          .action(clap::ArgAction::SetTrue),
      )
      .arg(
        Arg::new("table")
          .long("table")
          .value_name("name")
          .help("SQLx metadata table to inspect (defaults to `_sqlx_migrations`)")
          .num_args(1),
      ),
  )
}

/// Whether this module owns the given subcommand name.
pub fn owns(name: &str) -> bool {
  name == "doctor"
}

/// Dispatch the command.
pub fn dispatch(name: &str, matches: &ArgMatches, context: &Context) -> Result<()> {
  if !matches.get_flag("sqlx") {
    anyhow::bail!("`{name}` requires `--sqlx` in this release");
  }

  context.config.require_database_url()?;
  let report = match matches.get_one::<String>("table") {
    Some(table) => sqlx::detect_table(&context.config, table)?,
    None => sqlx::detect(&context.config)?,
  };

  match report {
    Some(report) => {
      println!(
        "Found SQLx metadata table `{}` with {} applied migration(s).",
        sqlx::qualified_table_name(report.schema.as_deref(), &report.table),
        report.applied.len()
      );
      if report.has_down_checksum {
        println!("DOWN checksums are stored in the table.");
      } else {
        println!("DOWN checksums are validated from migration files on revert.");
      }
      println!(
        "Next: midas migrate adopt --table {}",
        sqlx::qualified_table_name(report.schema.as_deref(), &report.table)
      );
    },
    None => {
      println!("No SQLx metadata table found.");
      println!("A Midas project does not need one.");
    },
  }

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn owns_only_doctor() {
    assert!(owns("doctor"));
    assert!(!owns("sqlx"));
  }
}
