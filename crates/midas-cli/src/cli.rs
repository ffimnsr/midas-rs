//! Midas CLI entry point and command composition.

use std::env;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;
use std::time::Instant;

use anyhow::{
  Context as _,
  Result,
};
use clap::parser::ValueSource;
use clap::{
  Arg,
  ArgMatches,
  Command,
};
use clap_complete::Shell;
use midas::config::{
  Config,
  ConfigOverrides,
  ProcessEnv,
};
use tracing_subscriber::EnvFilter;

use crate::commands;

/// The package version.
const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The package description.
const PKG_DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");

/// Entry point used by the binaries. Maps errors to documented exit codes.
pub fn main_entry(command_name: &str, is_subcommand: bool) -> ExitCode {
  match midas_entry(command_name, is_subcommand) {
    Ok(()) => ExitCode::SUCCESS,
    Err(error) => {
      eprintln!("error: {error:#}");
      ExitCode::from(exit_code(&error))
    },
  }
}

/// Map an error chain to the CLI exit code contract.
///
/// `0` success, `1` user/config error, `2` database error, `3` dirty state,
/// `4` drift.
pub fn exit_code(error: &anyhow::Error) -> u8 {
  for cause in error.chain() {
    let Some(error) = cause.downcast_ref::<midas::Error>() else {
      continue;
    };
    return match error {
      midas::Error::DirtyMigration { .. } => 3,
      midas::Error::Drift(_) => 4,
      midas::Error::Database(_) | midas::Error::Lock(_) | midas::Error::LockTimeout { .. } => 2,
      _ => 1,
    };
  }
  1
}

/// Entry point shared by the `midas` and `cargo-migrate` binaries.
pub fn midas_entry(command_name: &str, is_subcommand: bool) -> Result<()> {
  dotenvy::dotenv()
    .or_else(|_| dotenvy::from_filename(".env.midas"))
    .ok();

  if env::var("RUST_LOG").is_err() {
    env::set_var("RUST_LOG", "midas=info,midas_cli=info");
  }

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

  let no_color = no_color_requested(matches.get_flag("no-color"), env::var_os("NO_COLOR").as_deref());
  configure_color(no_color);
  tracing_subscriber::fmt()
    .with_env_filter(EnvFilter::from_default_env())
    .with_ansi(!no_color)
    .init();

  if matches.subcommand_name().is_none() {
    cli_app.print_long_help()?;
    return Ok(());
  }

  if let Some(("completion", sub)) = matches.subcommand() {
    return dispatch_completion(sub, command_name, is_subcommand);
  }

  let overrides = ConfigOverrides {
    database_url: command_line_value(&matches, "database"),
    migrations_dir: command_line_value::<String>(&matches, "source").map(PathBuf::from),
    ..ConfigOverrides::default()
  };
  let cwd = env::current_dir().context("failed to read current directory")?;
  // `init` repairs configuration, so it must not depend on parsing an existing
  // (possibly invalid) `midas.toml`.
  let config_dir = (matches.subcommand_name() != Some("init")).then_some(cwd.as_path());
  let config = Config::load(&ProcessEnv, &overrides, config_dir)?;
  let context = commands::Context::new(config).with_flags(
    matches.get_flag("dry-run"),
    matches.get_flag("no-lock"),
    matches.get_flag("ignore-missing"),
  );

  let start = Instant::now();
  commands::dispatch(&matches, &context)?;
  let duration = start.elapsed();
  tracing::trace!(
    seconds = duration.as_secs(),
    milliseconds = duration.subsec_millis(),
    "command completed"
  );

  Ok(())
}

fn build_cli(command_name: &str, is_subcommand: bool) -> Command {
  let name = command_name.to_owned();
  let mut cli_app = if is_subcommand {
    Command::new(name)
  } else {
    Command::new(name.clone()).bin_name(name)
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
        .required(false)
        .global(true),
    )
    .arg(
      Arg::new("source")
        .short('s')
        .long("source")
        .env("MIGRATIONS_DIR")
        .value_name("path")
        .help("Sets the migration store directory")
        .num_args(1)
        .required(false)
        .global(true),
    )
    .arg(
      Arg::new("no-color")
        .long("no-color")
        .help("Disable colored output (honors NO_COLOR)")
        .action(clap::ArgAction::SetTrue)
        .global(true),
    )
    .arg(
      Arg::new("dry-run")
        .long("dry-run")
        .help("Print SQL without executing it")
        .action(clap::ArgAction::SetTrue)
        .global(true),
    )
    .arg(
      Arg::new("no-lock")
        .long("no-lock")
        .help("Skip the advisory migration lock")
        .action(clap::ArgAction::SetTrue)
        .global(true),
    )
    .arg(
      Arg::new("ignore-missing")
        .long("ignore-missing")
        .help("Ignore applied migrations whose files are no longer present")
        .action(clap::ArgAction::SetTrue)
        .global(true),
    );

  let cli_app = commands::register(cli_app).subcommand(
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

fn write_completions(command_name: &str, is_subcommand: bool, shell: &str) -> Result<()> {
  let shell = Shell::from_str(shell).map_err(|error| anyhow::anyhow!("invalid shell: {error}"))?;

  let mut app = build_cli(command_name, is_subcommand);
  if is_subcommand {
    clap_complete::generate(shell, &mut app, "cargo", &mut std::io::stdout().lock());
  } else {
    clap_complete::generate(shell, &mut app, command_name, &mut std::io::stdout().lock());
  }

  Ok(())
}

fn command_line_value<T>(matches: &ArgMatches, id: &str) -> Option<T>
where
  T: Clone + Send + Sync + 'static,
{
  if matches.value_source(id) == Some(ValueSource::CommandLine) {
    matches.get_one::<T>(id).cloned()
  } else {
    None
  }
}

fn no_color_requested(flag: bool, env_value: Option<&OsStr>) -> bool {
  flag || env_value.is_some()
}

fn configure_color(disabled: bool) {
  if disabled {
    console::set_colors_enabled(false);
    console::set_colors_enabled_stderr(false);
  }
}

/// Handle the `completion` subcommand, which needs to rebuild the CLI.
pub fn dispatch_completion(matches: &ArgMatches, command_name: &str, is_subcommand: bool) -> Result<()> {
  let shell = matches.get_one::<String>("shell").context("shell is required")?;
  write_completions(command_name, is_subcommand, shell)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn no_color_honors_flag_and_environment() {
    assert!(no_color_requested(true, None));
    assert!(no_color_requested(false, Some(OsStr::new("1"))));
    assert!(!no_color_requested(false, None));
  }

  #[test]
  fn build_cli_omits_removed_commands() {
    let app = build_cli("midas", false);
    let names = app
      .get_subcommands()
      .map(|command| command.get_name())
      .collect::<Vec<_>>();

    for removed in ["faker", "update", "seed", "upto", "drop"] {
      assert!(!names.contains(&removed), "`{removed}` must be removed");
    }
    for present in ["init", "add", "run", "info", "list", "resolve", "completion"] {
      assert!(names.contains(&present), "`{present}` must exist");
    }
  }

  #[test]
  fn exit_codes_follow_the_contract() {
    let error = anyhow::Error::new(midas::Error::DirtyMigration { version: 1 });
    assert_eq!(exit_code(&error), 3);

    let error = anyhow::Error::new(midas::Error::LockTimeout { backend: "postgres" });
    assert_eq!(exit_code(&error), 2);

    let error = anyhow::Error::new(midas::Error::Drift("boom".to_string()));
    assert_eq!(exit_code(&error), 4);

    let error = anyhow::Error::new(midas::Error::migration("boom"));
    assert_eq!(exit_code(&error), 1);

    let error = anyhow::anyhow!("plain");
    assert_eq!(exit_code(&error), 1);
  }
}
