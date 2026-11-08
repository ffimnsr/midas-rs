//! Procedural macros for `midas`.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::ops::Range;
use std::path::{
  Path,
  PathBuf,
};

use proc_macro::TokenStream;

const DEFAULT_DIR: &str = "migrations";
const UP_MARKER: &str = "-- !UP";
const DOWN_MARKER: &str = "-- !DOWN";
const NO_TRANSACTION: &str = "-- no-transaction";

/// Embed migrations at compile time.
///
/// `migrate!()` reads `$CARGO_MANIFEST_DIR/migrations`. Pass a path to override:
/// `migrate!("db/migrations")`. Every `.sql` file contributes an `include_str!`
/// with an absolute path so Cargo rebuilds when a migration changes.
#[proc_macro]
pub fn migrate(input: TokenStream) -> TokenStream {
  match expand(input) {
    Ok(tokens) => tokens,
    Err(message) => compile_error(&message),
  }
}

fn expand(input: TokenStream) -> Result<TokenStream, String> {
  let manifest =
    std::env::var("CARGO_MANIFEST_DIR").map_err(|_| "CARGO_MANIFEST_DIR is not set".to_string())?;
  let directory = match parse_input(input)? {
    Some(directory) if directory.is_absolute() => directory,
    Some(directory) => Path::new(&manifest).join(directory),
    None => Path::new(&manifest).join(DEFAULT_DIR),
  };
  let directory = directory.canonicalize().map_err(|error| {
    format!(
      "cannot read migrations directory `{}`: {error}",
      directory.to_string_lossy()
    )
  })?;

  if !directory.is_dir() {
    return Err(format!(
      "migrations path `{}` is not a directory",
      directory.to_string_lossy()
    ));
  }

  let mut entries = discover(&directory)?;
  entries.sort_by_key(|entry| entry.version);

  let mut out = String::from("::std::vec![");
  for entry in entries {
    out.push_str(&render(entry)?);
    out.push(',');
  }
  out.push(']');

  out
    .parse::<TokenStream>()
    .map_err(|error| format!("failed to build migration expansion: {error}"))
}

fn parse_input(input: TokenStream) -> Result<Option<PathBuf>, String> {
  let text = input.to_string();
  let text = text.trim();
  if text.is_empty() {
    return Ok(None);
  }

  let value = parse_string_literal(text)
    .ok_or_else(|| "migrate! expects one string literal path argument".to_string())?;
  Ok(Some(PathBuf::from(value)))
}

fn parse_string_literal(text: &str) -> Option<String> {
  let text = text.trim();
  if let Some(rest) = text.strip_prefix('r') {
    let hashes = rest.chars().take_while(|character| *character == '#').count();
    let rest = rest.get(hashes..)?.strip_prefix('"')?;
    let suffix = format!("\"{}", "#".repeat(hashes));
    return rest.strip_suffix(&suffix).map(ToString::to_string);
  }

  let rest = text.strip_prefix('"')?.strip_suffix('"')?;
  let mut out = String::new();
  let mut chars = rest.chars();
  while let Some(character) = chars.next() {
    if character != '\\' {
      out.push(character);
      continue;
    }
    let escaped = chars.next()?;
    out.push(match escaped {
      'n' => '\n',
      'r' => '\r',
      't' => '\t',
      '\\' => '\\',
      '"' => '"',
      other => other,
    });
  }
  Some(out)
}

struct Entry {
  version: i64,
  description: String,
  up_path: PathBuf,
  up: Range<usize>,
  down: Option<(PathBuf, Range<usize>)>,
  no_tx: bool,
  format: &'static str,
}

fn discover(directory: &Path) -> Result<Vec<Entry>, String> {
  let mut singles: BTreeMap<i64, Entry> = BTreeMap::new();
  let mut ups: BTreeMap<i64, (String, PathBuf, String)> = BTreeMap::new();
  let mut downs: BTreeMap<i64, (String, PathBuf, String)> = BTreeMap::new();

  let mut paths = std::fs::read_dir(directory)
    .map_err(|error| format!("cannot read `{}`: {error}", directory.to_string_lossy()))?
    .filter_map(|entry| entry.ok())
    .map(|entry| entry.path())
    .filter(|path| path.is_file())
    .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("sql"))
    .collect::<Vec<_>>();
  paths.sort();

  for path in paths {
    let filename = path
      .file_name()
      .and_then(|name| name.to_str())
      .ok_or_else(|| format!("invalid migration filename `{}`", path.to_string_lossy()))?;
    let (stem, part) = if let Some(stem) = filename.strip_suffix(".up.sql") {
      (stem, Part::Up)
    } else if let Some(stem) = filename.strip_suffix(".down.sql") {
      (stem, Part::Down)
    } else if let Some(stem) = filename.strip_suffix(".sql") {
      (stem, Part::Single)
    } else {
      continue;
    };

    let (version, description) = parse_stem(stem, filename)?;
    let content = std::fs::read_to_string(&path)
      .map_err(|error| format!("cannot read `{}`: {error}", path.to_string_lossy()))?;

    match part {
      Part::Single => {
        let sections = split_sections(&content);
        let (up, format) = match sections.up {
          Some(up) => (up, "Midas"),
          None => (0..content.len(), "SqlxOneWay"),
        };
        let down = sections.down.map(|down| (path.clone(), down));
        if singles
          .insert(
            version,
            Entry {
              version,
              description,
              up_path: path,
              up,
              down,
              no_tx: detect_no_transaction(&content, sections.has_up_marker),
              format,
            },
          )
          .is_some()
        {
          return Err(duplicate(version));
        }
      },
      Part::Up => {
        if ups.insert(version, (description, path, content)).is_some() {
          return Err(duplicate(version));
        }
      },
      Part::Down => {
        if downs.insert(version, (description, path, content)).is_some() {
          return Err(duplicate(version));
        }
      },
    }
  }

  for (version, (description, up_path, up)) in ups {
    if singles.contains_key(&version) {
      return Err(duplicate(version));
    }
    let down = downs.remove(&version);
    if let Some((down_description, _, _)) = &down {
      if down_description != &description {
        return Err(format!("mismatched descriptions for migration {version:013}"));
      }
    }
    let up_range = 0..up.len();
    singles.insert(
      version,
      Entry {
        version,
        description,
        no_tx: detect_no_transaction(&up, false),
        up_path,
        up: up_range,
        down: down.map(|(_, path, content)| {
          let range = 0..content.len();
          (path, range)
        }),
        format: "SqlxUpDown",
      },
    );
  }

  if let Some(version) = downs.keys().next() {
    return Err(format!(
      "migration {version:013} has a DOWN file without an UP file"
    ));
  }

  Ok(singles.into_values().collect())
}

enum Part {
  Single,
  Up,
  Down,
}

fn parse_stem(stem: &str, filename: &str) -> Result<(i64, String), String> {
  let (version, description) = stem
    .split_once('_')
    .ok_or_else(|| format!("invalid migration filename `{filename}`"))?;
  let version = version
    .parse::<i64>()
    .map_err(|error| format!("invalid migration version in `{filename}`: {error}"))?;
  if version <= 0 {
    return Err(format!(
      "migration version must be greater than zero in `{filename}`"
    ));
  }
  if description.is_empty()
    || !description
      .chars()
      .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-'))
  {
    return Err(format!("invalid migration description in `{filename}`"));
  }
  Ok((version, description.to_string()))
}

fn duplicate(version: i64) -> String {
  format!("duplicate migration version {version:013}")
}

struct Sections {
  up: Option<Range<usize>>,
  down: Option<Range<usize>>,
  has_up_marker: bool,
}

/// Ranges are byte offsets into the original content, BOM included.
fn split_sections(content: &str) -> Sections {
  let bom = content.len() - content.strip_prefix('\u{feff}').map_or(content.len(), str::len);
  let stripped = &content[bom..];
  let Some(up_marker_end) = find_marker_end(stripped, UP_MARKER) else {
    return Sections {
      up: None,
      down: None,
      has_up_marker: false,
    };
  };
  let up_start = up_marker_end + bom;

  match find_marker_start(&stripped[up_marker_end..], DOWN_MARKER) {
    Some(down_start) => {
      let down_start = up_marker_end + down_start;
      let down_content = find_marker_end(&stripped[down_start..], DOWN_MARKER)
        .map(|end| down_start + end)
        .unwrap_or(down_start);
      Sections {
        up: Some(up_start..(down_start + bom)),
        down: Some((down_content + bom)..content.len()),
        has_up_marker: true,
      }
    },
    None => Sections {
      up: Some(up_start..content.len()),
      down: None,
      has_up_marker: true,
    },
  }
}

fn find_marker_start(content: &str, marker: &str) -> Option<usize> {
  let mut offset = 0;
  for line in content.split_inclusive('\n') {
    if line.trim_end_matches(['\r', '\n']) == marker {
      return Some(offset);
    }
    offset += line.len();
  }
  None
}

fn find_marker_end(content: &str, marker: &str) -> Option<usize> {
  let mut offset = 0;
  for line in content.split_inclusive('\n') {
    offset += line.len();
    if line.trim_end_matches(['\r', '\n']) == marker {
      return Some(offset);
    }
  }
  None
}

fn detect_no_transaction(content: &str, has_up_marker: bool) -> bool {
  let content = content.strip_prefix('\u{feff}').unwrap_or(content);
  let mut lines = content.lines();
  if has_up_marker {
    for line in lines.by_ref() {
      let trimmed = line.trim();
      if trimmed == UP_MARKER {
        break;
      }
      if trimmed.is_empty() || trimmed.starts_with("-- #") {
        continue;
      }
      if trimmed == NO_TRANSACTION {
        return true;
      }
      break;
    }
    for line in lines {
      let trimmed = line.trim();
      if trimmed.is_empty() {
        continue;
      }
      return trimmed == NO_TRANSACTION;
    }
    false
  } else {
    for line in lines {
      let trimmed = line.trim();
      if trimmed.is_empty() {
        continue;
      }
      return trimmed == NO_TRANSACTION;
    }
    false
  }
}

fn render(entry: Entry) -> Result<String, String> {
  let up_path = entry
    .up_path
    .to_str()
    .ok_or_else(|| format!("non-UTF-8 migration path `{}`", entry.up_path.to_string_lossy()))?;
  let up = slice(up_path, &entry.up);
  let down = match &entry.down {
    Some((path, range)) => {
      let path = path
        .to_str()
        .ok_or_else(|| "non-UTF-8 down migration path".to_string())?;
      format!("::std::option::Option::Some({})", slice(path, range))
    },
    None => "::std::option::Option::None".to_string(),
  };

  Ok(format!(
    "::midas::migration::Migration::from_parts({version}, {description:?}, {up}, {down}, \
     {no_tx}, {up_path:?}, ::midas::migration::MigrationFormat::{format})",
    version = entry.version,
    description = entry.description,
    up = up,
    down = down,
    no_tx = entry.no_tx,
    up_path = up_path,
    format = entry.format,
  ))
}

fn slice(path: &str, range: &Range<usize>) -> String {
  if range.start == 0 && range.end == 0 {
    return "::std::string::String::new()".to_string();
  }
  format!(
    "include_str!({path:?})[{start}..{end}].to_string()",
    path = path,
    start = range.start,
    end = range.end
  )
}

fn compile_error(message: &str) -> TokenStream {
  format!("compile_error!({message:?})")
    .parse()
    .unwrap_or_else(|_| TokenStream::new())
}
