//! Migration and project commands.

use std::fs;
use std::path::Path;

use anyhow::{
  Context as _,
  Result,
};
use clap::{
  value_parser,
  Arg,
  ArgMatches,
  Command,
};
use console::style;
use midas::commander::Migrator;
use midas::config::Config;
use midas::sequel::DatabaseEngine;
use midas::{
  lookup,
  sequel,
};
use prettytable::format::consts;
use prettytable::{
  row,
  Table,
};

use crate::commands::Context;

const PROJECT_FILE: &str = "midas.toml";
const SCHEMA_FILE: &str = "schema.toml";

/// Register migration commands on the root command.
pub fn register(app: Command) -> Command {
  app
    .subcommand(
      Command::new("init")
        .about("Set up a midas project (midas.toml, schema.toml, migrations/)")
        .arg(
          Arg::new("force")
            .long("force")
            .help("Overwrite existing configuration files")
            .action(clap::ArgAction::SetTrue),
        ),
    )
    .subcommand(
      Command::new("create")
        .visible_alias("c")
        .about("Creates a timestamped migration file")
        .arg(Arg::new("name").help("The migration action name").required(true)),
    )
    .subcommand(
      Command::new("status")
        .visible_alias("s")
        .about("Checks the status of the migration"),
    )
    .subcommand(
      Command::new("list")
        .visible_alias("ls")
        .about("Lists all available migrations"),
    )
    .subcommand(
      Command::new("up")
        .visible_alias("u")
        .about("Apply all pending migrations"),
    )
    .subcommand(
      Command::new("upto")
        .about("Apply all migrations up to the given migration number")
        .arg(
          Arg::new("migration_number")
            .help("The migration number to apply up to")
            .num_args(1)
            .value_parser(value_parser!(i64))
            .required(true),
        ),
    )
    .subcommand(
      Command::new("down")
        .visible_alias("d")
        .about("Remove all applied migrations"),
    )
    .subcommand(
      Command::new("revert")
        .visible_alias("r")
        .about("Reverts the nth number of migration (defaults to the last migration)")
        .arg(
          Arg::new("steps")
            .value_name("num")
            .help("The number of migrations to revert")
            .num_args(1)
            .value_parser(value_parser!(usize))
            .default_value("1"),
        ),
    )
    .subcommand(Command::new("redo").about("Redo the last migration"))
    .subcommand(
      Command::new("drop")
        .about("Drops everything inside the database (NOTE: must have create/drop privilege)"),
    )
}

/// Whether this module owns the given subcommand name.
pub fn owns(name: &str) -> bool {
  matches!(
    name,
    "init" | "create" | "list" | "status" | "up" | "upto" | "down" | "redo" | "revert" | "drop"
  )
}

/// Dispatch a migration subcommand.
pub fn dispatch(name: &str, matches: &ArgMatches, context: &Context) -> Result<()> {
  match name {
    "init" => init(&context.config, matches.get_flag("force")),
    "create" => create(matches, &context.config),
    "list" => list(&context.config),
    "status" => with_migrator(context, |migrator| migrator.status()),
    "up" => with_migrator(context, |migrator| migrator.up()),
    "upto" => upto(matches, context),
    "down" => with_migrator(context, |migrator| migrator.down()),
    "redo" => with_migrator(context, |migrator| migrator.redo()),
    "revert" => revert(matches, context),
    "drop" => drop_database(context),
    other => anyhow::bail!("unknown migration subcommand `{other}`"),
  }
}

fn init(config: &Config, force: bool) -> Result<()> {
  let project_file = Path::new(PROJECT_FILE);
  let schema_file = Path::new(SCHEMA_FILE);

  if !force {
    for path in [project_file, schema_file] {
      if path.exists() {
        return Err(
          midas::Error::config(format!(
            "{} already exists; rerun with --force to overwrite",
            path.to_string_lossy()
          ))
          .into(),
        );
      }
    }
  }

  fs::create_dir_all(&config.migrations_dir).with_context(|| {
    format!(
      "failed to create migrations directory `{}`",
      config.migrations_dir.to_string_lossy()
    )
  })?;

  if !project_file.exists() || force {
    fs::write(project_file, project_template(config)).context("failed to write midas.toml")?;
  }
  if !schema_file.exists() || force {
    fs::write(schema_file, schema_template()).context("failed to write schema.toml")?;
  }

  println!("Initialized midas project.");
  Ok(())
}

fn create(matches: &ArgMatches, config: &Config) -> Result<()> {
  let slug = matches
    .get_one::<String>("name")
    .context("migration name is required")?;
  let slug = slug.to_ascii_lowercase().replace(' ', "_");
  lookup::create_migration_file(&config.migrations_dir, &slug)?;
  println!("Created migration `{slug}`.");
  Ok(())
}

fn list(config: &Config) -> Result<()> {
  let migrations = lookup::build_migration_list(&config.migrations_dir)?;
  if migrations.is_empty() {
    println!("There are no available migration files.");
    return Ok(());
  }

  let mut table = Table::new();
  table.set_titles(row![Fbb->"Migration No.", Fbb->"Filename"]);
  table.set_format(*consts::FORMAT_CLEAN);

  for (number, migration) in migrations.iter() {
    table.add_row(row![b->&format!("{number:013}"), Fgb->&migration.filename]);
  }

  let msg = style("Available migrations:").bold().cyan();
  println!();
  println!("{msg}");
  println!();
  table.printstd();
  println!();

  Ok(())
}

fn upto(matches: &ArgMatches, context: &Context) -> Result<()> {
  let version = *matches
    .get_one::<i64>("migration_number")
    .context("migration number is required")?;
  if version < 0 {
    anyhow::bail!("migration number must be non-negative");
  }
  with_migrator(context, move |migrator| migrator.upto(version))
}

fn revert(matches: &ArgMatches, context: &Context) -> Result<()> {
  let steps = *matches
    .get_one::<usize>("steps")
    .context("revert steps are required")?;
  with_migrator(context, move |migrator| {
    for _ in 0..steps {
      migrator.revert()?;
    }
    Ok(())
  })
}

fn drop_database(context: &Context) -> Result<()> {
  let database_url = context.config.require_database_url()?.to_string();
  with_migrator(context, move |migrator| migrator.drop(&database_url))
}

fn with_migrator<F>(context: &Context, action: F) -> Result<()>
where
  F: FnOnce(&mut Migrator) -> midas::Result<()>,
{
  context.config.require_database_url()?;
  let migrations = lookup::build_migration_list(&context.config.migrations_dir)?;
  let backend = sequel::connect(&context.config)?;
  let mut migrator = Migrator::new(backend, migrations);
  action(&mut migrator)?;
  Ok(())
}

fn project_template(config: &Config) -> String {
  let dir = config.migrations_dir.to_string_lossy();
  let dialect = configured_dialect(config);
  let (default_schema, metadata_schema) = match dialect {
    "sqlite" => ("default_schema = \"main\"\n".to_string(), String::new()),
    _ => (
      "default_schema = \"public\"\n".to_string(),
      "metadata_schema = \"midas\"\n".to_string(),
    ),
  };
  let managed_schema = if dialect == "sqlite" { "main" } else { "public" };

  format!(
    r#"# midas project configuration
[project]
dialect = "{dialect}"
{default_schema}
[database]
url_env = "DATABASE_URL"
{metadata_schema}metadata_table = "__schema_migrations"
shadow_url_env = "SHADOW_DATABASE_URL"

[migrations]
dir = "{dir}"
transaction = "per-migration"

[managed]
schemas = ["{managed_schema}"]
ignore_objects = []
"#
  )
}

fn configured_dialect(config: &Config) -> &'static str {
  config
    .database_url
    .as_deref()
    .and_then(|url| DatabaseEngine::from_url(url).ok())
    .map(DatabaseEngine::dialect_name)
    .unwrap_or("postgres")
}

fn schema_template() -> &'static str {
  "# midas schema DSL file.\n\
   # Define tables, enums, and other schema objects here.\n"
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn owners_are_explicit() {
    for name in [
      "init", "create", "list", "status", "up", "upto", "down", "redo", "revert", "drop",
    ] {
      assert!(owns(name), "expected owner for `{name}`");
    }
    assert!(!owns("db"));
    assert!(!owns("schema"));
  }

  #[test]
  fn project_template_contains_migrations_dir() {
    let config = Config {
      migrations_dir: "custom_migrations".into(),
      ..Config::default()
    };
    let template = project_template(&config);
    assert!(template.contains("dir = \"custom_migrations\""));
    assert!(template.contains("metadata_table = \"__schema_migrations\""));
  }

  #[test]
  fn project_template_follows_the_configured_dialect() {
    let sqlite = Config {
      database_url: Some("sqlite://./app.db3".to_string()),
      ..Config::default()
    };
    let template = project_template(&sqlite);
    assert!(template.contains("dialect = \"sqlite\""));
    assert!(!template.contains("metadata_schema"));

    let cockroach = Config {
      database_url: Some("cockroachdb://root@localhost:26257/app".to_string()),
      ..Config::default()
    };
    let template = project_template(&cockroach);
    assert!(template.contains("dialect = \"postgres\""));
    assert!(template.contains("metadata_schema = \"midas\""));
  }

  #[test]
  fn schema_template_is_comment_only() {
    assert!(schema_template().lines().all(|line| line.starts_with('#')));
  }
}
