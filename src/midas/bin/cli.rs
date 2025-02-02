//! Midas Logic

use anyhow::{
  Context,
  Result as AnyhowResult,
};
use clap::{
  Arg,
  Command,
};
use clap_complete::Shell;
use console::style;
use indoc::formatdoc;
use midas_core::commander::Migrator;
use midas_core::lookup::MigrationFiles;
use midas_core::sequel::mysql::Mysql;
use midas_core::sequel::postgres::Postgres;
use midas_core::sequel::sqlite::Sqlite;
use midas_core::sequel::Driver as SequelDriver;
use midas_core::{
  ensure_migration_state_dir_exists,
  lookup,
};
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
use tracing_subscriber::EnvFilter;

/// The package version
const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The package description
const PKG_DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");

/// The entry point for the midas binary
/// This function is responsible for parsing the command line arguments
/// and executing the appropriate subcommand
pub fn midas_entry(command_name: &str, is_subcommand: bool) -> AnyhowResult<()> {
  dotenv::dotenv()
    .or_else(|_| dotenv::from_filename(".env.midas"))
    .ok();

  // Set the default log level
  if env::var("RUST_LOG").is_err() {
    env::set_var("RUST_LOG", "midas=info");
  }

  // Initialize the logger
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

  // Set the database connection url
  // The order of precedence is:
  // 1. CLI argument
  // 2. Environment variable DATABASE_URI
  // 3. Environment variable DB_URL
  // 4. Environment variable DSN
  // 5. Default value "file://./data.db3"
  // 6. Error
  let env_db_url_1 = env::var("DATABASE_URI").ok();
  let env_db_url_2 = env::var("DB_URL").ok();
  let env_db_url_3 = env::var("DSN").ok();
  let default_db_url = "file://./data.db3".to_string();
  let db_url = matches
    .get_one::<String>("database")
    .or(env_db_url_1.as_ref())
    .or(env_db_url_2.as_ref())
    .or(env_db_url_3.as_ref())
    .or(Some(&default_db_url))
    .context("No database connection url was provided")?;
  log::trace!("Using database: {}", db_url);

  // Set the migration source path
  // The order of precedence is:
  // 1. CLI argument
  // 2. Environment variable MIGRATIONS_ROOT
  // 3. Environment variable MIGRATIONS_PATH
  // 4. Default value "migrations"
  // 5. Error
  let env_source_path_1 = env::var("MIGRATIONS_ROOT").ok();
  let env_source_path_2 = env::var("MIGRATIONS_PATH").ok();
  let default_source_path = Some("migrations".to_string());
  let source = matches
    .get_one::<String>("source")
    .or(env_source_path_1.as_ref())
    .or(env_source_path_2.as_ref())
    .or(default_source_path.as_ref())
    .context("No migration source path was provided")?;
  log::trace!("Using migration : {}", source);

  let source_path = Path::new(&source);

  // Create the migrations directory if it doesn't exist
  if !source_path.exists() {
    log::trace!("Creating new migrations directory: {:?}", source_path);
    fs::create_dir_all(source_path)?;
  }

  // Create the migrations tree list
  let migrations = lookup::build_migration_list(source_path)?;

  // Start the timer for monitoring the operation duration
  let start = Instant::now();

  // Match the subcommand and execute the appropriate action
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
    Some("init") => init(source, db_url)?,
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
      // Get the shell argument
      let shell = matches
        .subcommand_matches("completion")
        .context("No subcommand shell was detected")?
        .get_one::<String>("shell")
        .context("Shell was invalid")?;

      // Write the completion script to stdout
      write_completions(command_name, is_subcommand, shell)?;
    },
    None => cli_app.print_long_help()?,
    _ => println!("Invalid subcommand provided"),
  }

  // Calculate the operation duration
  let duration = start.elapsed();
  let minutes = duration.as_secs() / 60;
  let seconds = duration.as_secs() % 60;

  // Log the operation duration
  if minutes == 0 && seconds == 0 {
    log::trace!("Operation took less than 1 second.");
  } else {
    log::trace!("Operation took {} minutes and {} seconds.", minutes, seconds);
  }

  Ok(())
}

/// Builds the midas CLI
/// This function builds the midas CLI by setting the version, description, and subcommands
/// # Arguments
/// * `command_name` - The name of the command
/// * `is_subcommand` - A boolean indicating if the command is a subcommand
/// # Returns
/// A `Command` instance representing the midas CLI
/// # Example
/// ```rust
/// use midas_core::commander::build_cli;
/// let cli = build_cli("midas", false);
/// ```
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
    .subcommand(Command::new("seed").about("Seed the database with data"))
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

/// Creates a new migration file
/// This function creates a new migration file with the given slug
/// # Arguments
/// * `path` - The migration source directory
/// * `slug` - The migration slug
/// # Returns
/// An `AnyhowResult` indicating the success or failure of the operation
/// # Errors
/// This function will return an error if the migration file could not be created
/// # Example
/// ```rust
/// use midas_core::commander::create;
/// create("migrations", "create_users_table").unwrap();
/// ```
fn create(path: &Path, slug: &str) -> AnyhowResult<()> {
  let fixed_slug = slug.to_ascii_lowercase().replace(' ', "_");
  lookup::create_migration_file(path, &fixed_slug)?;
  Ok(())
}

/// Lists all available migration files
/// This function lists all the migration files in the migration directory
/// and prints them to the console in a tabular format
/// # Arguments
/// * `migrations` - The migration files to list
/// # Returns
/// An `AnyhowResult` indicating the success or failure of the operation
/// # Errors
/// This function will return an error if the migration files could not be listed
/// # Example
/// ```rust
/// use midas_core::lookup::MigrationFiles;
/// use midas_core::commander::list_migration_files;
/// let migrations = MigrationFiles::new();
/// list_migration_files(&migrations).unwrap();
/// ```
fn list_migration_files(migrations: &MigrationFiles) -> AnyhowResult<()> {
  let mut table = Table::new();
  table.set_titles(row![Fbb->"Migration No.", Fbb->"Filename"]);
  table.set_format(*consts::FORMAT_CLEAN);

  // Iterate over the migration files and add them to the table
  for (number, migration) in migrations.iter() {
    let migration_no = format!("{:013}", number);
    let filename = &migration.filename;

    table.add_row(row![
      b->&migration_no,
      Fgb->&filename,
    ]);
  }

  // Print the table to the console
  let msg = style("Available migrations:").bold().cyan();
  println!();
  println!("{msg}");
  println!();
  table.printstd();
  println!();

  Ok(())
}

/// Initializes the midas project
/// This function initializes the midas project by creating the migration directory,
/// the dotenv file, and the sqlite database file
/// # Arguments
/// * `source` - The migration source directory
/// * `db_url` - The database connection url
/// # Returns
/// An `AnyhowResult` indicating the success or failure of the operation
/// # Errors
/// This function will return an error if the migration directory, dotenv file, or sqlite database file could not be created
/// # Example
/// ```rust
/// use midas_core::commander::init;
/// init("migrations", "file://./data.db3").unwrap();
/// ```
/// # Note
/// This function is used to initialize the midas project by creating the migration directory,
/// the dotenv file, and the sqlite database file
/// ```shell
/// $ midas init
/// ```
/// This command will create the following files:
/// - .migration-state directory
/// - migrations directory
/// - .env.midas file
/// - data.db3 file
fn init(source: &str, db_url: &str) -> AnyhowResult<()> {
  let filename = ".env.midas";
  let filepath = std::env::current_dir()?.join(filename);

  // Create the dotenv file
  log::trace!("Creating new env file: {:?}", filepath);
  let mut f = File::create(filepath)?;
  let contents = formatdoc! {"
    DATABASE_URL={}
    MIGRATIONS_DIR={}
  ", db_url, source};
  f.write_all(contents.as_bytes())?;
  f.sync_all()?;

  // Create the sqlite database file
  if db_url.starts_with("file")
    || db_url.starts_with("sqlite")
    || db_url.starts_with("/")
    || db_url.starts_with(".")
  {
    let db_url: &str = &convert_local_file_path_to_file_scheme(db_url);
    Sqlite::new(db_url)?;
  }

  // Create the migrations state directory
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

fn convert_local_file_path_to_file_scheme(db_url: &str) -> String {
  if db_url.starts_with("/") || db_url.starts_with(".") {
    format!("file://{}", db_url)
  } else {
    db_url.to_string()
  }
}

fn get_executor(db_url: &str) -> AnyhowResult<Box<dyn SequelDriver>> {
  use url::Url;

  // Safeguard against empty database URL
  if db_url.is_empty() {
    anyhow::bail!("Database URL is empty");
  }

  // Convert local file path to file:// scheme
  let db_url: &str = &convert_local_file_path_to_file_scheme(db_url);

  // Parse the database URL
  let url = Url::parse(db_url).context("Failed to parse database URL")?;
  log::trace!("Connecting to database scheme: {}", url.scheme());

  // Match the database scheme and create the appropriate driver
  let driver: Box<dyn SequelDriver> = match url.scheme() {
    "file" | "sqlite" | "sqlite3" => Box::new(Sqlite::new(db_url).context("Failed to create Sqlite driver")?),
    "mysql" => Box::new(Mysql::new(db_url).context("Failed to create Mysql driver")?),
    "postgres" => Box::new(Postgres::new(db_url).context("Failed to create Postgres driver")?),
    _ => return Err(anyhow::anyhow!("Unsupported database scheme: {}", url.scheme())),
  };

  Ok(driver)
}

#[cfg(test)]
mod tests {
  use url::Url;

  #[test]
  fn it_should_show_correct_scheme() {
    let url = "file:///data.db3";
    let url = Url::parse(url).unwrap();
    assert_eq!(url.scheme(), "file");

    let url = "file://./data.db3";
    let url = Url::parse(url).unwrap();
    assert_eq!(url.scheme(), "file");

    let url = "file:data.db3";
    let url = Url::parse(url).unwrap();
    assert_eq!(url.scheme(), "file");

    let url = "sqlite:/data.db3";
    let url = Url::parse(url).unwrap();
    assert_eq!(url.scheme(), "sqlite");

    let url = "sqlite:data.db3";
    let url = Url::parse(url).unwrap();
    assert_eq!(url.scheme(), "sqlite");
  }
}
