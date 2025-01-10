use clap::{
  Arg,
  Command,
};
use clap_complete::Shell;
use console::style;
use indoc::formatdoc;
use midas_core::lookup::MigrationFiles;
use prettytable::format::consts;
use prettytable::{
  row,
  Table,
};
use std::fs::File;
use std::io::Write as _;
use std::path::Path;
use std::str::FromStr;
use std::time::Instant;
use std::{
  env,
  fs,
};

const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
const PKG_DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");

use anyhow::{
  Context,
  Result as AnyhowResult,
};
use midas_core::commander::Migrator;
use midas_core::sequel::mysql::Mysql;
use midas_core::sequel::postgres::Postgres;
use midas_core::sequel::sqlite::Sqlite;
use midas_core::sequel::Driver as SequelDriver;
use midas_core::{
  ensure_migration_state_dir_exists,
  lookup,
};
use tracing_subscriber::EnvFilter;

pub fn midas_entry(command_name: &str, is_subcommand: bool) -> AnyhowResult<()> {
  dotenv::dotenv()
    .or_else(|_| dotenv::from_filename(".env.midas"))
    .ok();

  if env::var("RUST_LOG").is_err() {
    env::set_var("RUST_LOG", "midas=info");
  }

  tracing_subscriber::fmt()
    .with_env_filter(EnvFilter::from_default_env())
    .init();

  let mut cli_app = build_cli(command_name, is_subcommand);
  let matches = if is_subcommand {
    cli_app
      .clone()
      .get_matches()
      .subcommand_matches(command_name)
      .with_context(|| format!("cargo-{command_name} not invoked via cargo command"))?
      .to_owned()
  } else {
    cli_app.clone().get_matches()
  };

  let env_db_url_1 = env::var("DATABASE_URI").ok();
  let env_db_url_2 = env::var("DB_URL").ok();
  let env_db_url_3 = env::var("DSN").ok();
  let db_url = matches
    .get_one::<String>("database")
    .or(env_db_url_1.as_ref())
    .or(env_db_url_2.as_ref())
    .or(env_db_url_3.as_ref())
    .context("No database connection url was provided")?;
  log::trace!("Using database: {}", db_url);

  let default_source_path = Some("migrations".to_string());
  let env_source_path_1 = env::var("MIGRATIONS_ROOT").ok();
  let env_source_path_2 = env::var("MIGRATIONS_PATH").ok();
  let source = matches
    .get_one::<String>("source")
    .or(env_source_path_1.as_ref())
    .or(env_source_path_2.as_ref())
    .or(default_source_path.as_ref())
    .context("No migration source path was provided")?;
  log::trace!("Using migration : {}", source);

  let source_path = Path::new(&source);
  let migrations = lookup::build_migration_list(source_path)?;

  let start = Instant::now();

  match matches.subcommand_name() {
    Some("create") => {
      let slug = matches
        .subcommand_matches("create")
        .context("No subcommand name argument was detected")?
        .get_one::<String>("name")
        .context("Name argument was either malformed or undecipherable")?;

      create(source_path, slug)?;
    },
    Some("list") => {
      list_migration_files(&migrations)?;
    },
    Some("faker") => {
      unimplemented!();
    },
    Some("init") => init(source_path, source, db_url)?,
    Some("status") => {
      let executor = get_executor(db_url);
      let mut migrator = executor.map(|executor| Migrator::new(executor, migrations))?;
      migrator.status()?
    },
    Some("up") => {
      let executor = get_executor(db_url);
      let mut migrator = executor.map(|executor| Migrator::new(executor, migrations))?;
      migrator.up()?
    },
    Some("upto") => {
      let value = matches
        .subcommand_matches("upto")
        .context("No subcommand migration number was detected")?
        .get_one::<i64>("migration_number")
        .context("Migration number was invalid")?;
      let migration_number = *value;

      if migration_number < 0 {
        return Err(anyhow::anyhow!("Migration number must be greater than 0"));
      }

      if !migrations.contains_key(&migration_number) {
        return Err(anyhow::anyhow!(
          "Migration number {} does not exist",
          migration_number
        ));
      }

      let executor = get_executor(db_url);
      let mut migrator = executor.map(|executor| Migrator::new(executor, migrations))?;
      migrator.upto(migration_number)?
    },
    Some("down") => {
      let executor = get_executor(db_url);
      let mut migrator = executor.map(|executor| Migrator::new(executor, migrations))?;
      migrator.down()?
    },
    Some("redo") => {
      let executor = get_executor(db_url);
      let mut migrator = executor.map(|executor| Migrator::new(executor, migrations))?;
      migrator.redo()?
    },
    Some("revert") => {
      let executor = get_executor(db_url);
      let mut migrator = executor.map(|executor| Migrator::new(executor, migrations))?;

      let value = matches
        .subcommand_matches("revert")
        .context("No subcommand step was detected")?
        .get_one::<usize>("steps")
        .context("Steps number was invalid")?;

      for _ in 0usize..*value {
        migrator.revert()?;
      }
    },
    Some("drop") => {
      let executor = get_executor(db_url);
      let mut migrator = executor.map(|executor| Migrator::new(executor, migrations))?;
      migrator.drop(db_url)?
    },
    Some("update") => {
      unimplemented!();
    },
    Some("completion") => {
      let shell = matches
        .subcommand_matches("completion")
        .context("No subcommand shell was detected")?
        .get_one::<String>("shell")
        .context("Shell was invalid")?;

      write_completions(command_name, is_subcommand, shell)?;
    },
    None => cli_app.print_long_help()?,
    _ => println!("Invalid subcommand provided"),
  }

  let duration = start.elapsed();
  let minutes = duration.as_secs() / 60;
  let seconds = duration.as_secs() % 60;

  if minutes == 0 && seconds == 0 {
    log::trace!("Operation took less than 1 second.");
  } else {
    log::trace!("Operation took {} minutes and {} seconds.", minutes, seconds);
  }

  Ok(())
}

fn build_cli(command_name: &str, is_subcommand: bool) -> Command {
  let cname = command_name.to_owned();
  let mut cli_app = if is_subcommand {
    Command::new(cname)
  } else {
    Command::new(cname.clone()).bin_name(cname)
  };

  cli_app = cli_app
    .version(PKG_VERSION)
    .about(PKG_DESCRIPTION)
    .arg_required_else_help(true)
    .propagate_version(true)
    .arg(
      Arg::new("database")
        .short('d')
        .long("database")
        .env("DATABASE_URL")
        .value_name("url")
        .help("Sets the database connection url")
        .num_args(1)
        .required(false),
    )
    .arg(
      Arg::new("source")
        .short('s')
        .long("source")
        .env("MIGRATIONS_DIR")
        .value_name("path")
        .help("Sets the migration store directory")
        .num_args(1)
        .required(false),
    )
    .subcommand(Command::new("init").about("Setup and creates initial migration directory and a dotenv file"))
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
            .value_parser(clap::value_parser!(usize))
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
            .value_parser(clap::value_parser!(usize))
            .default_value("1"),
        ),
    )
    .subcommand(Command::new("redo").about("Redo the last migration"))
    .subcommand(
      Command::new("drop")
        .about("Drops everything inside the database (NOTE: must have create/drop privilege)"),
    )
    .subcommand(
      Command::new("faker")
        .visible_alias("f")
        .about("Generate fake data for the database or tables"),
    )
    .subcommand(Command::new("update").about("Update the midas binary to the latest version"))
    .subcommand(
      Command::new("completion")
        .visible_alias("comp")
        .about("Generates shell completion scripts")
        .arg(
          Arg::new("shell")
            .help("The shell to generate completion for")
            .required(true)
            .num_args(1),
        ),
    );

  if is_subcommand {
    Command::new("cargo").bin_name("cargo").subcommand(cli_app)
  } else {
    cli_app
  }
}

fn create(path: &Path, slug: &str) -> AnyhowResult<()> {
  let fixed_slug = slug.to_ascii_lowercase().replace(' ', "_");
  lookup::create_migration_file(path, &fixed_slug)?;
  Ok(())
}

fn list_migration_files(migrations: &MigrationFiles) -> AnyhowResult<()> {
  let mut table = Table::new();
  table.set_titles(row![Fbb->"Migration No.", Fbb->"Filename"]);
  table.set_format(*consts::FORMAT_CLEAN);

  for (number, migration) in migrations.iter() {
    let migration_no = format!("{:013}", number);
    let filename = &migration.filename;

    table.add_row(row![
      b->&migration_no,
      Fgb->&filename,
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

fn init(source_path: &Path, source: &str, db_url: &str) -> AnyhowResult<()> {
  let filename = ".env.midas";
  let filepath = std::env::current_dir()?.join(filename);

  log::trace!("Creating new env file: {:?}", filepath);
  let mut f = File::create(filepath)?;
  let contents = formatdoc! {"
    DATABASE_URL={}
    MIGRATIONS_DIR={}
  ", db_url, source};
  f.write_all(contents.as_bytes())?;
  f.sync_all()?;

  log::trace!("Creating new migrations directory: {:?}", source_path);
  if !source_path.exists() {
    fs::create_dir_all(source_path)?;
  }

  ensure_migration_state_dir_exists()?;
  Ok(())
}

fn write_completions(command_name: &str, is_subcommand: bool, shell: &str) -> anyhow::Result<()> {
  use std::io::stdout;

  let shell = Shell::from_str(shell).map_err(|e| anyhow::anyhow!("Invalid shell - {}", e))?;

  let mut app = build_cli(command_name, is_subcommand);
  if is_subcommand {
    clap_complete::generate(shell, &mut app, "cargo", &mut stdout().lock());
  } else {
    clap_complete::generate(shell, &mut app, command_name, &mut stdout().lock());
  }

  Ok(())
}

fn get_executor(db_url: &str) -> AnyhowResult<Box<dyn SequelDriver>> {
  use anyhow::{
    anyhow,
    Context,
  };
  use url::Url;

  let url = Url::parse(db_url).context("Failed to parse database URL")?;
  log::trace!("Connecting to database scheme: {}", url.scheme());

  let driver: Box<dyn SequelDriver> = match url.scheme() {
    "file" => Box::new(Sqlite::new(db_url).context("Failed to create Sqlite driver")?),
    "mysql" => Box::new(Mysql::new(db_url).context("Failed to create Mysql driver")?),
    "postgres" => Box::new(Postgres::new(db_url).context("Failed to create Postgres driver")?),
    _ => return Err(anyhow!("Unsupported database scheme: {}", url.scheme())),
  };

  Ok(driver)
}
