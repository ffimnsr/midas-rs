use clap::{
  Arg,
  Command,
};
use std::env;
use std::path::Path;
use std::time::Instant;

const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
const PKG_DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");

use anyhow::{
  Context,
  Result as AnyhowResult,
};
use midas_core::commander::Migrator;
use midas_core::lookup;
use midas_core::sequel::mysql::Mysql;
use midas_core::sequel::postgres::Postgres;
use midas_core::sequel::sqlite::Sqlite;
use midas_core::sequel::Driver as SequelDriver;

pub(crate) fn midas_entry(command_name: &str, sub_command: bool) -> AnyhowResult<()> {
  dotenv::dotenv()
    .or_else(|_| dotenv::from_filename(".env.midas"))
    .ok();

  if env::var("RUST_LOG").is_err() {
    env::set_var("RUST_LOG", "midas=info");
  }

  tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    .init();

  let cname = command_name.to_owned();
  let mut cli_app = if sub_command {
    Command::new(cname.clone())
  } else {
    Command::new(cname.clone()).bin_name(cname)
  };

  cli_app = cli_app
    .version(PKG_VERSION)
    .about(PKG_DESCRIPTION)
    .arg_required_else_help(true)
    .disable_help_subcommand(true)
    .propagate_version(true)
    .arg(
      Arg::new("database")
        .short('d')
        .long("database")
        .value_name("URL")
        .help("Sets the database connection url")
        .num_args(1),
    )
    .arg(
      Arg::new("source")
        .short('s')
        .long("source")
        .value_name("DIR")
        .help("Sets the migration store directory")
        .num_args(1),
    )
    .subcommand(
      Command::new("create")
        .about("Creates a timestamped migration file")
        .arg(Arg::new("name").help("The migration action name").required(true)),
    )
    .subcommand(Command::new("status").about("Checks the status of the migration"))
    .subcommand(Command::new("up").about("Apply all non-applied migrations"))
    .subcommand(Command::new("down").about("Remove all applied migrations"))
    .subcommand(Command::new("redo").about("Redo the last migration"))
    .subcommand(
      Command::new("revert")
        .about("Reverts the nth number of migration (defaults to the last migration)")
        .arg(
          Arg::new("steps")
            .value_name("N")
            .help("The number of migrations to revert")
            .num_args(1)
            .value_parser(clap::value_parser!(usize))
            .default_value("1"),
        ),
    )
    .subcommand(Command::new("init").about("Setups and creates initial file directory and env"))
    .subcommand(
      Command::new("drop")
        .about("Drops everything inside the database (NOTE: must have create/drop privilege)"),
    );

  let matches = if sub_command {
    let internal_matches = Command::new("cargo")
      .bin_name("cargo")
      .subcommand(cli_app)
      .get_matches();

    internal_matches
      .subcommand_matches(command_name)
      .with_context(|| format!("cargo-{command_name} not invoked via cargo command"))?
      .clone()
  } else {
    cli_app.get_matches()
  };

  // Read the database connection url from the environment variables
  // From the following possible sources:
  // 1. DATABASE_URL
  // 2. DB_URL
  // 3. DSN
  let env_db_url_1 = env::var("DATABASE_URL").ok();
  let env_db_url_2 = env::var("DB_URL").ok();
  let env_db_url_3 = env::var("DSN").ok();
  let db_url = matches
    .get_one::<String>("database")
    .or(env_db_url_1.as_ref())
    .or(env_db_url_2.as_ref())
    .or(env_db_url_3.as_ref())
    .context("No database connection url was provided")?;

  log::trace!("Using DSN: {}", db_url);
  let default_source_path = Some("migrations".to_string());
  let env_source_path_1 = env::var("MIGRATIONS_ROOT").ok();
  let env_source_path_2 = env::var("MIGRATIONS_DIR").ok();
  let source = matches
    .get_one::<String>("source")
    .or(env_source_path_1.as_ref())
    .or(env_source_path_2.as_ref())
    .or(default_source_path.as_ref())
    .context("No migration source path was provided")?;

  let source_path = Path::new(&source);
  let migrations = lookup::build_migration_list(source_path)?;

  let start = Instant::now();

  let executor = get_executor(db_url);
  let mut migrator = executor.map(|executor| Migrator::new(executor, migrations))?;

  match matches.subcommand_name() {
    Some("create") => {
      let slug = matches
        .subcommand_matches("create")
        .context("No subcommand name argument was detected")?
        .get_one::<String>("name")
        .context("Name argument was either malformed or undecipherable")?;

      migrator.create(source_path, slug)?;
    },
    Some("status") => migrator.status()?,
    Some("up") => migrator.up()?,
    Some("down") => migrator.down()?,
    Some("redo") => migrator.redo()?,
    Some("revert") => {
      let value = matches
        .subcommand_matches("revert")
        .context("No subcommand step was detected")?
        .get_one::<usize>("steps")
        .context("Steps number was invalid")?;

      for _ in 0usize..*value {
        migrator.revert()?;
      }
    },
    Some("init") => migrator.init(source_path, source, db_url)?,
    Some("drop") => migrator.drop(db_url)?,
    None => println!("No subcommand provided"),
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
