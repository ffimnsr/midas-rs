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
use midas::commander::{
  MigrationInfo,
  Migrator,
  MigratorOptions,
};
use midas::config::Config;
use midas::{
  migration,
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
      Command::new("add")
        .visible_alias("create")
        .visible_alias("c")
        .about("Create a timestamped migration file")
        .arg(Arg::new("name").help("The migration action name").required(true))
        .arg(
          Arg::new("reversible")
            .long("reversible")
            .help("Create a .up.sql/.down.sql pair")
            .action(clap::ArgAction::SetTrue),
        ),
    )
    .subcommand(
      Command::new("run")
        .visible_alias("up")
        .visible_alias("u")
        .about("Apply pending migrations")
        .arg(
          Arg::new("target")
            .long("target")
            .value_name("version")
            .help("Apply only migrations up to this version")
            .num_args(1)
            .value_parser(value_parser!(i64)),
        )
        .arg(json_flag()),
    )
    .subcommand(
      Command::new("info")
        .visible_alias("status")
        .visible_alias("s")
        .about("Show applied, pending, and dirty migrations")
        .arg(json_flag()),
    )
    .subcommand(
      Command::new("list")
        .visible_alias("ls")
        .about("List migration files without a database"),
    )
    .subcommand(
      Command::new("revert")
        .visible_alias("r")
        .about("Revert applied migrations")
        .arg(
          Arg::new("to")
            .long("to")
            .value_name("version")
            .help("Revert every migration above this version")
            .num_args(1)
            .value_parser(value_parser!(i64)),
        ),
    )
    .subcommand(Command::new("redo").about("Revert and re-apply the last migration"))
    .subcommand(
      Command::new("baseline")
        .about("Mark migrations up to a version as applied without running SQL")
        .arg(
          Arg::new("to")
            .long("to")
            .value_name("version")
            .help("Highest version to mark as applied")
            .num_args(1)
            .value_parser(value_parser!(i64))
            .required(true),
        ),
    )
    .subcommand(Command::new("repair").about("Recompute checksums for applied migrations"))
    .subcommand(
      Command::new("resolve")
        .about("Resolve a dirty migration")
        .arg(
          Arg::new("applied")
            .long("applied")
            .help("Mark the dirty migration as applied")
            .action(clap::ArgAction::SetTrue),
        )
        .arg(
          Arg::new("rolled-back")
            .long("rolled-back")
            .help("Remove the dirty migration record")
            .action(clap::ArgAction::SetTrue),
        )
        .arg(
          Arg::new("version")
            .long("version")
            .value_name("version")
            .help("Version to resolve (defaults to the lowest dirty version)")
            .num_args(1)
            .value_parser(value_parser!(i64)),
        ),
    )
    .subcommand(Command::new("reset").about("Drop metadata and replay every migration"))
    .subcommand(
      Command::new("down")
        .visible_alias("d")
        .about("Revert every applied migration")
        .arg(
          Arg::new("force")
            .long("force")
            .help("Required confirmation for reverting everything")
            .action(clap::ArgAction::SetTrue),
        ),
    )
}

fn json_flag() -> Arg {
  Arg::new("json")
    .long("json")
    .help("Emit machine-readable JSON")
    .action(clap::ArgAction::SetTrue)
}

/// Whether this module owns the given subcommand name.
pub fn owns(name: &str) -> bool {
  matches!(
    name,
    "init"
      | "add"
      | "create"
      | "c"
      | "run"
      | "up"
      | "u"
      | "info"
      | "status"
      | "s"
      | "list"
      | "ls"
      | "revert"
      | "r"
      | "redo"
      | "baseline"
      | "repair"
      | "resolve"
      | "reset"
      | "down"
      | "d"
  )
}

/// Dispatch a migration subcommand.
pub fn dispatch(name: &str, matches: &ArgMatches, context: &Context) -> Result<()> {
  match name {
    "init" => init(&context.config, matches.get_flag("force")),
    "add" | "create" | "c" => add(matches, context),
    "run" | "up" | "u" => run(matches, context),
    "info" | "status" | "s" => info(matches, context),
    "list" | "ls" => list(&context.config),
    "revert" | "r" => revert(matches, context),
    "redo" => with_migrator(context, false, |migrator| migrator.redo()).map(drop),
    "baseline" => baseline(matches, context),
    "repair" => with_migrator(context, false, |migrator| migrator.repair()).map(drop),
    "resolve" => resolve(matches, context),
    "reset" => with_migrator(context, false, |migrator| migrator.reset()).map(drop),
    "down" | "d" => with_migrator(context, false, |migrator| {
      migrator.down(matches.get_flag("force"))
    })
    .map(drop),
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

fn add(matches: &ArgMatches, context: &Context) -> Result<()> {
  let slug = matches
    .get_one::<String>("name")
    .context("migration name is required")?;
  let reversible = matches.get_flag("reversible");
  let version = migration::create_migration_file(&context.config.migrations_dir, slug, reversible)?;
  println!("Created migration {version:013}.");
  Ok(())
}

fn run(matches: &ArgMatches, context: &Context) -> Result<()> {
  let target = matches.get_one::<i64>("target").copied();
  if target.is_some_and(|target| target < 0) {
    anyhow::bail!("target version must be non-negative");
  }
  let info = with_migrator(context, matches.get_flag("json"), |migrator| migrator.up(target))?;
  output(matches, &info)
}

fn info(matches: &ArgMatches, context: &Context) -> Result<()> {
  let info = if matches.get_flag("json") {
    with_migrator(context, true, |migrator| migrator.info())?
  } else {
    with_migrator(context, false, |migrator| migrator.status())?
  };
  output(matches, &info)
}

fn list(config: &Config) -> Result<()> {
  let migrations = migration::discover(&config.migrations_dir)?;
  if migrations.is_empty() {
    println!("There are no available migration files.");
    return Ok(());
  }

  let mut table = Table::new();
  table.set_titles(row![Fbb->"Migration No.", Fbb->"Filename", Fbb->"Description"]);
  table.set_format(*consts::FORMAT_CLEAN);

  for migration in migrations.values() {
    let filename = migration
      .source_path
      .file_name()
      .map(|name| name.to_string_lossy().into_owned())
      .unwrap_or_default();
    table.add_row(row![
      b->&format!("{:013}", migration.version),
      Fgb->&filename,
      &migration.description
    ]);
  }

  let msg = style("Available migrations:").bold().cyan();
  println!();
  println!("{msg}");
  println!();
  table.printstd();
  println!();

  Ok(())
}

fn revert(matches: &ArgMatches, context: &Context) -> Result<()> {
  let target = matches.get_one::<i64>("to").copied();
  if target.is_some_and(|target| target < 0) {
    anyhow::bail!("revert target must be non-negative");
  }
  with_migrator(context, false, |migrator| migrator.revert_to(target)).map(drop)
}

fn baseline(matches: &ArgMatches, context: &Context) -> Result<()> {
  let target = *matches
    .get_one::<i64>("to")
    .context("baseline version is required")?;
  if target < 0 {
    anyhow::bail!("baseline version must be non-negative");
  }
  with_migrator(context, false, |migrator| migrator.baseline(target)).map(drop)
}

fn resolve(matches: &ArgMatches, context: &Context) -> Result<()> {
  let applied = matches.get_flag("applied");
  let rolled_back = matches.get_flag("rolled-back");
  if applied == rolled_back {
    anyhow::bail!("pass exactly one of --applied or --rolled-back");
  }
  let version = matches.get_one::<i64>("version").copied();
  if applied {
    with_migrator(context, false, |migrator| migrator.resolve_applied(version)).map(drop)
  } else {
    with_migrator(context, false, |migrator| migrator.resolve_rolled_back(version)).map(drop)
  }
}

fn output(matches: &ArgMatches, info: &MigrationInfo) -> Result<()> {
  if matches.get_flag("json") {
    println!("{}", serde_json::to_string(info)?);
  }
  Ok(())
}

fn with_migrator<F, T>(context: &Context, quiet: bool, action: F) -> Result<T>
where
  F: FnOnce(&mut Migrator) -> midas::Result<T>,
{
  context.config.require_database_url()?;
  let migrations = migration::discover(&context.config.migrations_dir)?;
  let backend = sequel::connect(&context.config)?;
  let options = MigratorOptions {
    dry_run: context.dry_run,
    no_lock: context.no_lock,
    transaction: context.config.transaction,
    quiet,
  };
  let mut migrator = Migrator::new(backend, migrations).with_options(options);
  Ok(action(&mut migrator)?)
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
    .and_then(|url| midas::sequel::DatabaseEngine::from_url(url).ok())
    .map(midas::sequel::DatabaseEngine::dialect_name)
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
      "init", "add", "create", "run", "up", "info", "status", "list", "revert", "redo", "baseline", "repair",
      "resolve", "reset", "down",
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
