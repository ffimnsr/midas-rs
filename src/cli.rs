use log::debug;
#[allow(unused_imports)]
use std::env;
use std::path::Path;
use std::time::Instant;

use clap::{Command, Arg};

const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
const PKG_DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");

use super::commander::Migrator;
use super::sequel::postgres::Postgres;
// use super::sequel::mysql::MySQL;

pub(crate) type GenericError = Box<dyn std::error::Error + Send + Sync>;

pub(crate) fn midas_entry(
    command_name: &str,
    sub_command: bool,
) -> Result<(), GenericError> {
    dotenv::dotenv().ok();

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
                .about("Drops everything inside the database"),
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

    let env_db_url = env::var("DSN").unwrap_or(
        "postgres://postgres@localhost:5432/postgres?sslmode=disable".into(),
    );

    let database_url = matches.get_one::<String>("database")
        .unwrap_or(&env_db_url);

    let env_source_path =
        env::var("MIGRATIONS_ROOT").unwrap_or("migrations".into());

    debug!("Using DSN: {}", database_url);

    let source = matches.get_one::<String>("source").unwrap_or(&env_source_path);
    let source_path = Path::new(&source);
    let migrations = super::lookup::build_migration_list(source_path)?;

    let start = Instant::now();

    let executor = Postgres::new(database_url)?;
    let mut migrator = Migrator::new(Box::new(executor), migrations);

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
        Some("init") => migrator.init()?,
        Some("drop") => migrator.drop()?,
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
