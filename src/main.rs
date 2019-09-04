#[allow(unused_imports)]
use std::env;
use std::path::Path;
use std::time::Instant;

use clap::{App, AppSettings, Arg, SubCommand};
use failure::Error;

const PKG_VERSION: &'static str = env!("CARGO_PKG_VERSION");
const PKG_DESCRIPTION: &'static str = env!("CARGO_PKG_DESCRIPTION");
const PKG_NAME: &'static str = env!("CARGO_PKG_NAME");

mod commander;
mod lookup;
mod sequel;

use commander::Migrator;
use sequel::postgres::Postgres;

fn main() -> Result<(), Error> {
    // env::set_var("RUST_LOG", "midas=debug");
    env_logger::init();

    let matches = App::new(PKG_NAME)
        .version(PKG_VERSION)
        .about(PKG_DESCRIPTION)
        .setting(AppSettings::ArgRequiredElseHelp)
        .setting(AppSettings::DisableHelpSubcommand)
        .setting(AppSettings::GlobalVersion)
        .arg(
            Arg::with_name("database")
                .short("d")
                .long("database")
                .value_name("URL")
                .help("Sets the database connection url")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("source")
                .short("s")
                .long("source")
                .value_name("DIR")
                .help("Sets the migration store directory")
                .takes_value(true)
        )
        .subcommand(
            SubCommand::with_name("create")
                .about("Creates a timestamped migration file")
                .arg(
                    Arg::with_name("name")
                        .help("The migration action name")
                        .required(true)
                )
        )
        .subcommand(
            SubCommand::with_name("status")
                .about("Checks the status of the migration")
        )
        .subcommand(
            SubCommand::with_name("up")
                .about("Apply all non-applied migrations")
        )
        .subcommand(
            SubCommand::with_name("down")
                .about("Remove all applied migrations")
        )
        .subcommand(
            SubCommand::with_name("redo")
                .about("Redo the last migration")
        )
        .subcommand(
            SubCommand::with_name("revert")
                .about("Reverts the last migration")
        )
        .subcommand(
            SubCommand::with_name("setup")
                .about("Setups and creates the database must have privilege user")
        )
        .subcommand(
            SubCommand::with_name("drop")
                .about("Drops everything inside the database")
        )
        .get_matches();

    let database_url = matches.value_of("database")
        .unwrap_or("postgres://postgres@localhost:5432/passport");

    let source = matches.value_of("source").unwrap_or("migrations");
    let source_path = Path::new(&source);
    let migrations = lookup::build_migration_list(source_path)?;

    let start = Instant::now();

    let executor = Postgres::new(database_url)?;
    let mut migrator = Migrator::new(Box::new(executor), migrations);

    match matches.subcommand_name() {
        Some("create") => {
            let slug = matches.subcommand_matches("create")
                .unwrap()
                .value_of("name")
                .unwrap();

            migrator.create(source_path, slug)?
        },
        Some("status") => migrator.status()?,
        Some("up") => migrator.up()?,
        Some("down") => migrator.down()?,
        Some("redo") => migrator.redo()?,
        Some("revert") => migrator.revert()?,
        Some("setup") => migrator.setup()?,
        Some("drop") => migrator.drop()?,
        None => println!("No subcommand provided"),
        _ => println!("Invalid subcommand provided"),
    }

    let duration = start.elapsed();
    let minutes = duration.as_secs() / 60;
    let seconds = duration.as_secs() % 60;

    if minutes == 0 && seconds == 0 {
        println!("Operation took less than 1 second.");
    } else {
        println!(
            "Operation took {} minutes and {} seconds.",
            minutes, seconds
        );
    }

    Ok(())
}
