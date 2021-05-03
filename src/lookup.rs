use std::collections::BTreeMap;
use std::fs::{read_dir, File};
use std::io::prelude::*;
use std::io::BufReader;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use failure::{bail, Error};
use log::debug;
use regex::Regex;

pub type VecStr = Vec<String>;

#[derive(Debug)]
pub struct MigrationFile {
    pub content_up: Option<VecStr>,
    pub content_down: Option<VecStr>,
    pub number: i64,
    pub filename: String,
}

impl MigrationFile {
    fn new(filename: &str, number: i64) -> Self {
        Self { content_up: None, content_down: None, filename: filename.to_owned(), number: number }
    }
}

pub type MigrationFiles = BTreeMap<i64, MigrationFile>;

fn parse_file(filename: &str) -> Result<MigrationFile, Error> {
    let re = Regex::new(r"^(?P<number>[0-9]{13})_(?P<name>[_0-9a-zA-Z]*)\.sql$").unwrap();

    let res = match re.captures(filename) {
        None => bail!("Invalid filename found on {}", filename),
        Some(c) => c,
    };

    let number = res.name("number").unwrap().as_str().parse::<i64>().unwrap();

    Ok(MigrationFile::new(filename, number))
}

pub fn build_migration_list(path: &Path) -> Result<MigrationFiles, Error> {
    let mut files: MigrationFiles = BTreeMap::new();

    for entry in read_dir(path)? {
        let entry = entry?;
        let filename = entry.file_name();
        let info = match parse_file(filename.to_str().unwrap()) {
            Ok(info) => info,
            Err(_) => continue,
        };

        let file = File::open(entry.path())?;
        let mut buf_reader = BufReader::new(file);
        let mut content = String::new();
        buf_reader.read_to_string(&mut content)?;

        let split_vec: Vec<String> = content.split("\n").map(|s| s.to_string()).collect();

        let pos_up = split_vec.iter().position(|s| s == "-- !UP" || s == "-- !UP\r").unwrap();
        let pos_down = split_vec.iter().position(|s| s == "-- !DOWN" || s == "-- !DOWN\r").unwrap();

        let content_up = &split_vec[(pos_up + 1)..pos_down];
        let content_down = &split_vec[(pos_down + 1)..];

        let migration = MigrationFile {
            content_up: Some(content_up.to_vec()),
            content_down: Some(content_down.to_vec()),
            ..info
        };

        debug!("{:?}", migration);
        files.insert(migration.number, migration);
    }

    Ok(files)
}

fn timestamp() -> String {
    let start = SystemTime::now();
    let since_the_epoch = start.duration_since(UNIX_EPOCH).unwrap();
    since_the_epoch.as_millis().to_string()
}

pub fn create_migration_file(path: &Path, slug: &str) -> Result<(), Error> {
    let filename = timestamp() + "_" + slug + ".sql";
    let filepath = path.join(filename.clone());

    debug!("{:?}", filepath);
    let mut f = File::create(filepath)?;
    let contents = "\
    -- # Put the your SQL below migration seperator.\n\
    -- !UP\n\n\
    -- !DOWN\n";

    f.write_all(contents.as_bytes())?;
    f.sync_all()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_file() {
        let result = parse_file("0000000000000_initial.sql").unwrap();
        assert_eq!(result.number, 0);
        assert_eq!(result.filename, "0000000000000_initial.sql");
    }
}
