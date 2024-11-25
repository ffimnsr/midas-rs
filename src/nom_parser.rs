use nom::{
    branch::alt,
    bytes::complete::{tag, tag_no_case, take_until, take_while},
    character::complete::{
        multispace0, multispace1, newline, not_line_ending,
    },
    combinator::{opt, recognize},
    error::Error as NomError,
    multi::separated_list0,
    sequence::{preceded, tuple},
    IResult,
};

fn is_alphanumeric(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn table_name(input: &str) -> IResult<&str, &str> {
    take_while(is_alphanumeric)(input)
}

fn comment(input: &str) -> IResult<&str, &str> {
    recognize(tuple((tag("--"), not_line_ending)))(input)
}

fn until_comment(input: &str) -> IResult<&str, &str> {
    take_until("--")(input)
}

fn capture_create_table_name(input: &str) -> IResult<&str, &str> {
    let (input, _) = multispace0(input)?;
    let (input, _) = tag_no_case("create")(input)?;
    let (input, _) = opt(preceded(
        multispace1,
        alt((tag_no_case("global"), tag_no_case("local"))),
    ))(input)?;
    let (input, _) = opt(preceded(
        multispace1,
        alt((tag_no_case("temporary"), tag_no_case("temp"))),
    ))(input)?;
    let (input, _) =
        opt(preceded(multispace1, tag_no_case("unlogged")))(input)?;
    let (input, _) = multispace1(input)?;
    let (input, _) = tag_no_case("table")(input)?;
    let (input, _) =
        opt(preceded(multispace1, tag_no_case("if not exists")))(input)?;
    let (input, _) = multispace1(input)?;
    table_name(input)
}

fn remove_comments_and_newline_chars(input: &str) -> String {
    let mut result = String::new();
    let mut remaining = input;

    while !remaining.is_empty() {
        match until_comment(remaining) {
            Ok((rest, chunk)) => {
                result.push_str(chunk);
                remaining = rest;
            }
            Err(_) => {}
        }

        match comment(remaining) {
            Ok((rest, _)) => {
                remaining = rest;
            }
            Err(_) => {}
        }

        match newline::<_, NomError<&str>>(remaining) {
            Ok((rest, _)) => {
                remaining = rest;
            }
            Err(_) => {
                if let Ok((rest, chunk)) =
                    not_line_ending::<_, NomError<&str>>(remaining)
                {
                    result.push_str(chunk);
                    remaining = rest;
                }
            }
        }
    }

    result.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn statement_until_semicolon(input: &str) -> IResult<&str, &str> {
    recognize(tuple((multispace0, take_while(|c| c != ';'))))(input)
}

pub fn parse_sql(input: &str) -> IResult<&str, Vec<&str>> {
    let cleaned_input = remove_comments_and_newline_chars(input);
    let static_str: &str = Box::leak(cleaned_input.into_boxed_str());
    let (i, statements) =
        separated_list0(tag(";"), statement_until_semicolon)(static_str)?;

    let mut table_names = Vec::new();
    for statement in statements {
        if let Ok((_, table_name)) = capture_create_table_name(statement) {
            table_names.push(table_name);
        }
    }

    Ok((i, table_names))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_comment() {
        let input = "CREATE TABLE; -- comment\n";
        let expected = "-- comment";
        let (r1, _) = until_comment(input).unwrap();
        let (_, c2) = comment(r1).unwrap();
        assert_eq!(c2, expected);
    }

    #[test]
    fn test_remove_comments_and_newline() {
        let input = "CREATE TABLE foo; -- comment\nCREATE TABLE bar;";
        let expected = "CREATE TABLE foo; CREATE TABLE bar;";
        let output: &str = &remove_comments_and_newline_chars(input);
        assert_eq!(output, expected);
    }

    #[test]
    fn test_remove_comments_and_newline_2() {
        let input = "CREATE TABLE foo; -- comment\n\n\nCREATE TABLE bar;\n CREATE TABLE baz;";
        let expected = "CREATE TABLE foo; CREATE TABLE bar; CREATE TABLE baz;";
        let output: &str = &remove_comments_and_newline_chars(input);
        assert_eq!(output, expected);
    }

    #[test]
    fn test_remove_comments_and_newline_3() {
        let input = "CREATE TABLE foo;-- comment\n\n\nCREATE TABLE bar;\nCREATE TABLE baz;";
        let expected = "CREATE TABLE foo;CREATE TABLE bar;CREATE TABLE baz;";
        let output: &str = &remove_comments_and_newline_chars(input);
        assert_eq!(output, expected);
    }

    #[test]
    fn test_remove_comments_and_newline_4() {
        let input = r#"
        -- CREATE TABLE IF NOT EXISTS users (id SERIAL PRIMARY KEY);
        -- CREATE TABLE IF NOT EXISTS users (id SERIAL PRIMARY KEY);

        -- CREATE TABLE IF NOT EXISTS users (id SERIAL PRIMARY KEY);
        -- CREATE TABLE IF NOT EXISTS users (id SERIAL PRIMARY KEY);
        create table if not exists countries (
        id serial primary key,
        name text,
        code varchar(4) unique,
        idd_code varchar(4),
        currency varchar(4),
        status smallint default 1,
        created_at timestamptz default current_timestamp,
        updated_at timestamptz default current_timestamp
        );
        create table hello (id serial primary key, name text);
        "#;

        let expected = r#"
        create table if not exists countries (
        id serial primary key,
        name text,
        code varchar(4) unique,
        idd_code varchar(4),
        currency varchar(4),
        status smallint default 1,
        created_at timestamptz default current_timestamp,
        updated_at timestamptz default current_timestamp
        );
        create table hello (id serial primary key, name text);
        "#;
        let expected =
            expected.split_whitespace().collect::<Vec<_>>().join(" ");
        let output: &str = &remove_comments_and_newline_chars(input);
        assert_eq!(output, expected);
    }

    #[test]
    fn test_parse_sql() {
        let input = r#"
        create table if not exists countries (id serial primary key);
        create table if not exists hello (id serial primary key);
        create table hello2 (id serial primary key);
        create temp table hello3 (id serial primary key);
        "#;

        let (_, j) = parse_sql(input).unwrap();
        assert_eq!(j, vec!["countries", "hello", "hello2", "hello3",]);
    }
}
