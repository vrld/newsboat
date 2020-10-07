use nom::{
    branch::alt,
    bytes::complete::{escaped_transform, is_not, tag, take},
    character::complete::{none_of, space0},
    combinator::{complete, map, recognize, value},
    error::{ErrorKind, ParseError},
    multi::{many0, many1, separated_list, separated_nonempty_list},
    sequence::{delimited, tuple},
    Err, IResult, InputLength,
};

/// returns its input if it is at the end of input data
///
/// When we're at the end of the data, this combinator
/// will succeed
///
// Code lifted from https://github.com/Geal/nom/pull/1112
fn eof<I: InputLength + Copy, E: ParseError<I>>(input: I) -> IResult<I, I, E> {
    if input.input_len() == 0 {
        Ok((input, input))
    } else {
        Err(Err::Error(E::from_error_kind(input, ErrorKind::Eof)))
    }
}

fn unquoted_token(input: &str) -> IResult<&str, String> {
    let parser = tuple((none_of("\";"), is_not(" ;")));
    let parser = map(recognize(parser), String::from);

    parser(input)
}

fn quoted_token(input: &str) -> IResult<&str, String> {
    let parser = escaped_transform(is_not(r#""\"#), '\\', |control_char: &str| {
        alt((
            value(r#"""#, tag(r#"""#)),
            value(r#"\"#, tag(r#"\"#)),
            value("\r", tag("r")),
            value("\n", tag("n")),
            value("\t", tag("t")),
            take(1usize), // all other escaped characters are passed through, unmodified
        ))(control_char)
    });

    let double_quote = tag("\"");
    let parser = delimited(&double_quote, parser, alt((&double_quote, eof)));

    parser(input)
}

fn token(input: &str) -> IResult<&str, String> {
    let parser = alt((quoted_token, unquoted_token));
    parser(input)
}

fn operation_with_args(input: &str) -> IResult<&str, Vec<String>> {
    let parser = separated_nonempty_list(many1(tag(" ")), token);
    parser(input)
}

fn operation_sequence(input: &str) -> IResult<&str, Vec<Vec<String>>> {
    let semicolon = delimited(space0, tag(";"), space0);

    let parser = separated_list(many1(&semicolon), operation_with_args);
    let parser = delimited(many0(&semicolon), parser, many0(&semicolon));

    let parser = complete(parser);

    parser(input)
}

/// Split a semicolon-separated list of operations into a vector. Each operation is represented by
/// a non-empty sub-vector, where the first element is the name of the operation, and the rest of
/// the elements are operation's arguments.
///
/// Tokens can be double-quoted. Such tokens can contain spaces and C-like escaped sequences: `\n`
/// for newline, `\r` for carriage return, `\t` for tab, `\"` for double quote, `\\` for backslash.
/// Unsupported sequences are stripped of the escaping, i.e. `\e` turns into `e`.
///
/// This function assumes that the input string:
/// 1. doesn't contain a comment;
/// 2. doesn't contain backticks that need to be processed.
///
/// Returns `None` if the input could not be parsed.
pub fn tokenize_operation_sequence(input: &str) -> Option<Vec<Vec<String>>> {
    match operation_sequence(input) {
        Ok((_leftovers, tokens)) => Some(tokens),
        Err(_error) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::tokenize_operation_sequence;

    #[test]
    fn t_tokenize_operation_sequence_works_for_all_cpp_inputs() {
        assert_eq!(
            tokenize_operation_sequence("").unwrap(),
            Vec::<Vec<String>>::new()
        );
        assert_eq!(
            tokenize_operation_sequence("open").unwrap(),
            vec![vec!["open"]]
        );
        assert_eq!(
            tokenize_operation_sequence("open-all-unread-in-browser-and-mark-read").unwrap(),
            vec![vec!["open-all-unread-in-browser-and-mark-read"]]
        );
        assert_eq!(
            tokenize_operation_sequence("; ; ; ;").unwrap(),
            Vec::<Vec<String>>::new()
        );
        assert_eq!(
            tokenize_operation_sequence("open ; next").unwrap(),
            vec![vec!["open"], vec!["next"]]
        );
        assert_eq!(
            tokenize_operation_sequence("open ; next ; prev").unwrap(),
            vec![vec!["open"], vec!["next"], vec!["prev"]]
        );
        assert_eq!(
            tokenize_operation_sequence("open ; next ; prev ; quit").unwrap(),
            vec![vec!["open"], vec!["next"], vec!["prev"], vec!["quit"]]
        );
        assert_eq!(
            tokenize_operation_sequence(r#"set "arg 1""#).unwrap(),
            vec![vec!["set", "arg 1"]]
        );
        assert_eq!(
            tokenize_operation_sequence(r#"set "arg 1" ; set "arg 2" "arg 3""#).unwrap(),
            vec![vec!["set", "arg 1"], vec!["set", "arg 2", "arg 3"]]
        );
        assert_eq!(
            tokenize_operation_sequence(r#"set browser "firefox"; open-in-browser"#).unwrap(),
            vec![vec!["set", "browser", "firefox"], vec!["open-in-browser"]]
        );
        assert_eq!(
            tokenize_operation_sequence("set browser firefox; open-in-browser").unwrap(),
            vec![vec!["set", "browser", "firefox"], vec!["open-in-browser"]]
        );
        assert_eq!(
            tokenize_operation_sequence("open-in-browser; quit").unwrap(),
            vec![vec!["open-in-browser"], vec!["quit"]]
        );
        assert_eq!(
            tokenize_operation_sequence(r#"open; set browser "firefox --private-window"; quit"#)
                .unwrap(),
            vec![
                vec!["open"],
                vec!["set", "browser", "firefox --private-window"],
                vec!["quit"]
            ]
        );
        assert_eq!(
            tokenize_operation_sequence(r#"open ;set browser "firefox --private-window" ;quit"#)
                .unwrap(),
            vec![
                vec!["open"],
                vec!["set", "browser", "firefox --private-window"],
                vec!["quit"]
            ]
        );
        assert_eq!(
            tokenize_operation_sequence(r#"open;set browser "firefox --private-window";quit"#)
                .unwrap(),
            vec![
                vec!["open"],
                vec!["set", "browser", "firefox --private-window"],
                vec!["quit"]
            ]
        );
        assert_eq!(
            tokenize_operation_sequence("; ;; ; open",).unwrap(),
            vec![vec!["open"]]
        );
        assert_eq!(
            tokenize_operation_sequence(";;; ;; ; open",).unwrap(),
            vec![vec!["open"]]
        );
        assert_eq!(
            tokenize_operation_sequence(";;; ;; ; open ;",).unwrap(),
            vec![vec!["open"]]
        );
        assert_eq!(
            tokenize_operation_sequence(";;; ;; ; open ;; ;",).unwrap(),
            vec![vec!["open"]]
        );
        assert_eq!(
            tokenize_operation_sequence(";;; ;; ; open ; ;;;;",).unwrap(),
            vec![vec!["open"]]
        );
        assert_eq!(
            tokenize_operation_sequence(";;; open ; ;;;;",).unwrap(),
            vec![vec!["open"]]
        );
        assert_eq!(
            tokenize_operation_sequence("; open ;; ;; ;",).unwrap(),
            vec![vec!["open"]]
        );
        assert_eq!(
            tokenize_operation_sequence("open ; ;;; ;;",).unwrap(),
            vec![vec!["open"]]
        );
        assert_eq!(
            tokenize_operation_sequence(
                r#"set browser "sleep 3; do-something ; echo hi"; open-in-browser"#
            )
            .unwrap(),
            vec![
                vec!["set", "browser", "sleep 3; do-something ; echo hi"],
                vec!["open-in-browser"]
            ]
        );
    }

    #[test]
    fn t_tokenize_operation_sequence_ignores_escaped_sequences_outside_double_quotes() {
        assert_eq!(
            tokenize_operation_sequence(r#"\t"#).unwrap(),
            vec![vec![r#"\t"#]]
        );
        assert_eq!(
            tokenize_operation_sequence(r#"\r"#).unwrap(),
            vec![vec![r#"\r"#]]
        );
        assert_eq!(
            tokenize_operation_sequence(r#"\n"#).unwrap(),
            vec![vec![r#"\n"#]]
        );
        assert_eq!(
            tokenize_operation_sequence(r#"\v"#).unwrap(),
            vec![vec![r#"\v"#]]
        );
        assert_eq!(
            tokenize_operation_sequence(r#"\""#).unwrap(),
            vec![vec![r#"\""#]]
        );
        assert_eq!(
            tokenize_operation_sequence(r#"\\"#).unwrap(),
            vec![vec![r#"\\"#]]
        );
    }

    #[test]
    fn t_tokenize_operation_sequence_expands_escaped_sequences_inside_double_quotes() {
        assert_eq!(
            tokenize_operation_sequence(r#""\t""#).unwrap(),
            vec![vec!["\t"]]
        );
        assert_eq!(
            tokenize_operation_sequence(r#""\r""#).unwrap(),
            vec![vec!["\r"]]
        );
        assert_eq!(
            tokenize_operation_sequence(r#""\n""#).unwrap(),
            vec![vec!["\n"]]
        );
        assert_eq!(
            tokenize_operation_sequence(r#""\"""#).unwrap(),
            vec![vec!["\""]]
        );
        assert_eq!(
            tokenize_operation_sequence(r#""\\""#).unwrap(),
            vec![vec!["\\"]]
        );
    }

    #[test]
    fn t_tokenize_operation_sequence_passes_through_unsupported_escaped_chars_inside_double_quotes()
    {
        assert_eq!(
            tokenize_operation_sequence(r#""\1""#).unwrap(),
            vec![vec!["1"]]
        );
        assert_eq!(
            tokenize_operation_sequence(r#""\W""#).unwrap(),
            vec![vec!["W"]]
        );
        assert_eq!(
            tokenize_operation_sequence(r#""\b""#).unwrap(),
            vec![vec!["b"]]
        );
        assert_eq!(
            tokenize_operation_sequence(r#""\d""#).unwrap(),
            vec![vec!["d"]]
        );
        assert_eq!(
            tokenize_operation_sequence(r#""\x""#).unwrap(),
            vec![vec!["x"]]
        );
    }

    #[test]
    fn t_tokenize_operation_sequence_implicitly_closes_double_quotes_at_end_of_input() {
        assert_eq!(
            tokenize_operation_sequence(r#"set "arg 1"#).unwrap(),
            vec![vec!["set", "arg 1"]]
        );
    }
}
