use std::{borrow::Cow, rc::Rc};

use nom::{
    branch::alt,
    bytes::complete::{tag, tag_no_case, take_while, take_until},
    character::complete::{
        multispace0, multispace1, newline, not_line_ending,
    },
    combinator::{opt, recognize},
    error::Error as NomError,
    multi::{many0, separated_list0},
    sequence::{preceded, tuple},
    IResult,
};

fn is_alphanumeric(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn take_table_name(input: &str) -> IResult<&str, &str> {
    take_while(is_alphanumeric)(input)
}

fn comment(input: &str) -> IResult<&str, &str> {
    let (i, v) = recognize(tuple((tag("--"), not_line_ending)))(input)?;
    println!("comment: {:?}", v);
    Ok((i, v))
}

fn capture_comment(input: &str) -> IResult<&str, &str> {
    let (input, _) = take_until("--")(input)?;
    println!("capture_comment: {:?}", input);
    comment(input)
}

fn create_table(input: &str) -> IResult<&str, &str> {
    let (input, _) = tag_no_case("create")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, _) = tag_no_case("table")(input)?;
    let (input, _) = multispace1(input)?;
    let (input, _) =
        opt(preceded(multispace1, tag_no_case("if not exists")))(input)?;
    let (input, _) = multispace1(input)?;
    take_table_name(input)
}

fn remove_comments(input: &str) -> String {
    let mut result = String::new();
    let mut remaining = input;

    while !remaining.is_empty() {
        println!("result: {:?}", result);
        println!("remaining: {:?}", remaining);

        match comment(remaining) {
            Ok((rest, _)) => {
                remaining = rest;
            }
            Err(_) => {
                if let Ok((rest, chunk)) =
                    not_line_ending::<_, NomError<&str>>(remaining)
                {
                    result.push_str(chunk);
                    remaining = rest;
                } else {
                    break;
                }
            }
        }

        println!("result: {:?}", result);
        println!("remaining: {:?}", remaining);

        if let Ok((rest, _)) =
            tag::<&str, &str, NomError<&str>>("\n")(remaining)
        {
            result.push('\n');
            remaining = rest;
        }
    }

    println!("remove_comments: {:?}", result);

    result
}

fn remove_newlines(input: &str) -> String {
    input.replace('\n', " ")
}

fn sql_until_semicolon(input: &str) -> IResult<&str, &str> {
    recognize(tuple((multispace0, take_while(|c| c != ';'), tag(";"))))(input)
}

pub fn parse_sql(input: &str) -> IResult<&str, Vec<&str>> {
    let cleaned_input = remove_comments(input);
    log::info!("cleaned_input: {:?}", cleaned_input);
    let static_str: &str = Box::leak(cleaned_input.into_boxed_str());
    let (i, statements) = separated_list0(
        tag(";"),
        preceded(multispace0, sql_until_semicolon),
    )(static_str)?;
    log::info!("parse_sql: i: {:?}, statements: {:?}", i, statements);

    let mut table_names = Vec::new();
    for statement in statements {
        if let Ok((_, table_name)) = create_table(statement) {
            table_names.push(table_name);
        }
    }

    Ok((i, table_names))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_comment() {
        let input = "CREATE TABLE; -- comment\n";
        let expected = "-- comment";
        let output = comment(input).unwrap().1;
        assert_eq!(output, expected);
    }

    // #[test]
    // fn test_remove_comments_and_newline() {
    //     let input = "CREATE TABLE foo; -- comment\nCREATE TABLE bar;";
    //     let expected = "CREATE TABLE foo; CREATE TABLE bar;";
    //     let output: &str = &remove_comments(input);
    //     println!("output: {:?}", output);
    //     // assert_eq!(output, expected);
    // }

    // #[test]
    // fn test_parse_sql() {
    //     let input = "CREATE TABLE foo; -- comment\nCREATE TABLE bar;";
    //     let expected = vec!["foo", "bar"];
    //     // assert_eq!(parse_sql(input).unwrap().1, expected);
    // }
}
