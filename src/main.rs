use std::path::Path;
use std::fs;
use std::fs::File;

use clap::{App, Arg, SubCommand};
use postgres::{Client, NoTls};

const PKG_VERSION: &'static str = env!("CARGO_PKG_VERSION");
const PKG_AUTHORS: &'static str = env!("CARGO_PKG_AUTHORS");
const PKG_DESCRIPTION: &'static str = env!("CARGO_PKG_DESCRIPTION");
const PKG_NAME: &'static str = env!("CARGO_PKG_NAME");

fn establish(database_url: &str) -> bool {
    // Client::connect(database_url, NoTls).unwrap();
    true
}

fn create_migration(path: &Path, slug: &str, number: i32) {
    // File::create(path.join())
}

fn parse_filename(filename: &str) -> String {

}

fn check_migration_table_exists() -> bool {
    // "EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = '__schema_migrations')";
    true
}

fn main() {
    let matches = App::new(PKG_NAME)
        .version(PKG_VERSION)
        .author(PKG_AUTHORS)
        .about(PKG_DESCRIPTION)
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
            SubCommand::with_name("drop")
                .about("Drops everything inside the database")
        )
        .get_matches();

    // match matches.occurrences_of("v") {
    //     0 => println!("No verbose info"),
    //     1 => println!("Some verbose info"),
    //     2 => println!("Tons of verbose info"),
    //     3 | _ => println!("Don't be crazy"),
    // }

    let database_url = matches.value_of("database").unwrap_or("postgres://localhost:5432/postgres");
    let source = matches.value_of("source").unwrap_or("migrations");
    let source_path = Path::new(&source);

    match matches.subcommand_name() {
        Some("create") => println!("create"),
        Some("status") => println!("status"),
        Some("up") => println!("up"),
        Some("down") => println!("down"),
        Some("redo") => println!("redo"),
        Some("revert") => println!("revert"),
        Some("drop") => println!("drop"),
        None => println!("No subcommand provided"),
        _ => println!("Invalid subcommand provided"),
    }
}
