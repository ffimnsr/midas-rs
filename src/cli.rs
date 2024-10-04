use clap::{Arg, Command};
use log::debug;
#[allow(unused_imports)]
use std::env;
use std::path::Path;
use std::time::Instant;
use url::Url;

const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
const PKG_DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");

use super::commander::Migrator;
use super::sequel::mysql::Mysql;
use super::sequel::postgres::Postgres;
use super::sequel::sqlite::Sqlite;
use super::sequel::Driver as SequelDriver;

pub(crate) type GenericError = Box<dyn std::error::Error + Send + Sync>;

pub(crate) fn midas_entry(
    command_name: &str,
    sub_command: bool,
) -> Result<(), GenericError> {
    dotenv::dotenv().or_else(|_| dotenv::from_filename(".env.midas")).ok();

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
                .arg(
                    Arg::new("name")
                        .help("The migration action name")
                        .required(true),
                ),
        )
        .subcommand(
            Command::new("status")
                .about("Checks the status of the migration"),
        )
        .subcommand(
            Command::new("up")
                .about("Apply all non-applied migrations"),
        )
        .subcommand(
            Command::new("down")
                .about("Remove all applied migrations"),
        )
        .subcommand(
            Command::new("redo").about("Redo the last migration"),
        )
        .subcommand(
            Command::new("revert")
                .about("Reverts the last migration"),
        )
        .subcommand(
            Command::new("init")
                .about("Setups and creates initial file directory and env"),
        )
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
            .ok_or(format!(
                "cargo-{command_name} not invoked via cargo command",
            ))?
            .clone()
    } else {
        cli_app.get_matches()
    };

    let raw_env_db_url = env::var("DATABASE_URL").ok();
    let raw_db_url = matches
        .get_one::<String>("database")
        .or(raw_env_db_url.as_ref())
        .expect("msg: No database connection url was provided");

    debug!("Using DSN: {}", raw_db_url);
    let default_source_path = Some("migrations".to_string());
    let env_source_path = env::var("MIGRATIONS_ROOT").ok();
    let source = matches
        .get_one::<String>("source")
        .or(env_source_path.as_ref())
        .or(default_source_path.as_ref())
        .expect("msg: No migration source path was provided");

    let source_path = Path::new(&source);
    let migrations = super::lookup::build_migration_list(source_path)?;

    let start = Instant::now();

    let executor = get_executor(raw_db_url);
    if executor.is_none() {
        return Err("Unable to initialize executor".into());
    }
    let mut migrator = Migrator::new(executor.unwrap(), migrations);
    match matches.subcommand_name() {
        Some("create") => {
            let slug = matches
                .subcommand_matches("create")
                .ok_or("No slug was detected")?
                .get_one::<String>("name")
                .ok_or("Slug is either malformed or undecipherable")?;

            migrator.create(source_path, slug)?;
        }
        Some("status") => migrator.status()?,
        Some("up") => migrator.up()?,
        Some("down") => migrator.down()?,
        Some("redo") => migrator.redo()?,
        Some("revert") => migrator.revert()?,
        Some("init") => migrator.init(source_path, source, raw_db_url)?,
        Some("drop") => migrator.drop(raw_db_url)?,
        None => println!("No subcommand provided"),
        _ => println!("Invalid subcommand provided"),
    }

    let duration = start.elapsed();
    let minutes = duration.as_secs() / 60;
    let seconds = duration.as_secs() % 60;

    if minutes == 0 && seconds == 0 {
        debug!("Operation took less than 1 second.");
    } else {
        debug!("Operation took {} minutes and {} seconds.", minutes, seconds);
    }

    Ok(())
}

fn get_executor(raw_db_url: &str) -> Option<Box<dyn SequelDriver>> {
    let db_url = Url::parse(raw_db_url).ok();
    if let Some(db_url) = db_url {
        debug!("Connecting to database scheme: {}", db_url.scheme());

        let driver: Box<dyn SequelDriver> = match db_url.scheme() {
            "file" => Box::new(
                Sqlite::new(raw_db_url)
                    .expect("Failed to create Sqlite driver"),
            ),
            "mysql" => Box::new(
                Mysql::new(raw_db_url).expect("Failed to create Mysql driver"),
            ),
            "postgres" => Box::new(
                Postgres::new(raw_db_url)
                    .expect("Failed to create Postgres driver"),
            ),
            _ => return None,
        };

        Some(driver)
    } else {
        Some(Box::new(
            Sqlite::new(raw_db_url).expect("Failed to create Sqlite driver"),
        ))
    }
}
