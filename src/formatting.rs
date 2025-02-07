use ropey::Rope;
use tower_lsp::lsp_types::TextEdit;

use crate::{
    bazel_flags::BazelFlags,
    lsp_utils::range_to_lsp,
    parser::{parse_from_str, Line, ParserResult},
};

pub fn format_token_into(out: &mut String, tok: &str) {
    if tok.is_empty() {
        out.push_str("\"\"")
    } else if tok
        .chars()
        .all(|c| (c.is_alphanumeric() || c.is_ascii_punctuation()) && c != '"' && c != '\\')
    {
        out.push_str(tok);
    } else {
        out.push('"');
        for c in tok.chars() {
            match c {
                '\\' => out.push_str("\\\\"),
                '\"' => out.push_str("\\\""),
                _ => out.push(c),
            }
        }
        out.push('"');
    }
}

pub fn format_token(tok: &str) -> String {
    let mut out = String::with_capacity(2 + tok.len());
    format_token_into(&mut out, tok);
    out
}

pub fn format_line_into(out: &mut String, line: &Line) {
    // Format the command + config
    let mut non_empty = false;
    if let Some(command) = &line.command {
        format_token_into(out, &command.0);
        non_empty = true;
    }
    if let Some(config) = &line.config {
        out.push(':');
        format_token_into(out, &config.0);
        non_empty = true;
    }

    // Format the flags
    for flag in &line.flags {
        if non_empty {
            out.push(' ');
        }
        non_empty = true;

        if let Some(name) = &flag.name {
            format_token_into(out, &name.0);
            if let Some(value) = &flag.value {
                out.push('=');
                if !value.0.is_empty() {
                    format_token_into(out, &value.0);
                }
            }
        } else if let Some(value) = &flag.value {
            format_token_into(out, &value.0);
        }
    }

    // Format the comments
    if let Some(comment) = &line.comment {
        if non_empty {
            out.push(' ');
        }

        let could_be_ascii_art =
            line.command.is_none() && line.config.is_none() && line.flags.is_empty();
        let stripped_comment = if could_be_ascii_art {
            comment.0.trim_end().to_string()
        } else {
            " ".to_string() + comment.0.trim()
        };
        let comment_contents = stripped_comment.replace('\n', "\\\n");
        out.push('#');
        out.push_str(&comment_contents);
    }
    out.push('\n')
}

pub fn format_line(line: &Line) -> String {
    let mut out = String::with_capacity(line.span.end - line.span.start);
    format_line_into(&mut out, line);
    out
}

pub fn get_text_edits_for_lines(lines: &[Line], rope: &Rope) -> Vec<TextEdit> {
    lines
        .iter()
        .filter_map(|line| {
            let formatted = format_line(line);
            if formatted != rope.slice(line.span.clone()) {
                Some(TextEdit {
                    range: range_to_lsp(rope, &line.span)?,
                    new_text: formatted,
                })
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
}

pub fn pretty_print(str: &str, bazel_flags: &BazelFlags) -> Result<String, Vec<String>> {
    let ParserResult {
        tokens: _,
        mut lines,
        errors,
    } = parse_from_str(str);
    if !errors.is_empty() {
        return Err(errors
            .into_iter()
            .map(|e| format!("{}", e))
            .collect::<Vec<_>>());
    }
    crate::bazel_flags::combine_key_value_flags(&mut lines, bazel_flags);
    // TODO also support "single flag per command" and "single flag per line"
    // TODO strip duplicated empty lines directly following each other
    // TODO strip trailing new lines
    let mut out = String::with_capacity(str.len());
    for line in lines {
        format_line_into(&mut out, &line);
    }
    Ok(out)
}

#[cfg(test)]
use crate::bazel_flags::load_packaged_bazel_flags;

#[test]
fn test_format_token() {
    // No escaping for common, unescaped versions
    assert_eq!(format_token("abc123"), "abc123");
    assert_eq!(format_token("--my_flag"), "--my_flag");
    assert_eq!(format_token("--my_flag=123"), "--my_flag=123");
    assert_eq!(format_token("//my/package:target"), "//my/package:target");
    assert_eq!(format_token("@a://b/c:*"), "@a://b/c:*");
    // Also, non-ASCII characters are formatted without quoting
    assert_eq!(format_token("Täst"), "Täst");
    // Whitespaces need to be escaped
    assert_eq!(format_token("--my_flag= "), "\"--my_flag= \"");
    assert_eq!(format_token("--my_flag= x"), "\"--my_flag= x\"");
    assert_eq!(format_token("a b c"), "\"a b c\"");
    // Escaping of quotes and backslashes
    assert_eq!(format_token("a\"b"), "\"a\\\"b\"");
    assert_eq!(format_token("a\\b"), "\"a\\\\b\"");
}

#[test]
fn test_pretty_print_command() {
    let flags = load_packaged_bazel_flags("7.4.0");

    // Command & config names
    assert_eq!(pretty_print("build", &flags).unwrap(), "build\n");
    assert_eq!(pretty_print("build:opt", &flags).unwrap(), "build:opt\n");
    assert_eq!(
        pretty_print("build:o\\ p\\ t", &flags).unwrap(),
        "build:\"o p t\"\n"
    );
    assert_eq!(
        pretty_print("buil\" d:o p\"\\ t", &flags).unwrap(),
        "\"buil d\":\"o p t\"\n"
    );
    // Invalid command & config names, but should still work
    assert_eq!(pretty_print(":opt", &flags).unwrap(), ":opt\n");
}

#[test]
fn test_pretty_print_flags() {
    let flags = load_packaged_bazel_flags("7.4.0");

    // Flags (also works without a command, although that is strictly speaking invalid)
    assert_eq!(pretty_print("--x", &flags).unwrap(), "--x\n");
    assert_eq!(pretty_print("--x=abc123", &flags).unwrap(), "--x=abc123\n");
    // Normalizes quoting and whitespaces
    assert_eq!(
        pretty_print("-\"-x=abc12\"3", &flags).unwrap(),
        "--x=abc123\n"
    );
    assert_eq!(pretty_print("--\\x=a\\bc", &flags).unwrap(), "--x=abc\n");
    assert_eq!(
        pretty_print("--x=a\\ bc\"1 2 3\"", &flags).unwrap(),
        "--x=\"a bc1 2 3\"\n"
    );
    assert_eq!(
        pretty_print("--x\\ =a\\ b", &flags).unwrap(),
        "\"--x \"=\"a b\"\n"
    );
    // Normalizes empty strings
    assert_eq!(pretty_print("--x=\"\"", &flags).unwrap(), "--x=\n");
    // Removes whitespaces between flags
    assert_eq!(
        pretty_print("--x=1    --y=2", &flags).unwrap(),
        "--x=1 --y=2\n"
    );
}

#[test]
fn test_pretty_print_combined_flags() {
    let flags = load_packaged_bazel_flags("7.4.0");

    // The `--copt` flag expects an argument and hence consumes the
    // following `--std=c++20`. `--std=c++20` should not raise
    // an error about an unrecognized Bazel flag.
    assert_eq!(
        pretty_print("build --copt --std=c++20", &flags).unwrap(),
        "build --copt=--std=c++20\n"
    );
    // On the other hand, `--keep_going` only takes an optional value.
    // Hence, the `true` is interpreted as a separate flag, which then triggers
    // an error.
    assert_eq!(
        pretty_print("build --keep_going --foobar", &flags).unwrap(),
        "build --keep_going --foobar\n"
    );

    // Handles empty parameters correctly
    assert_eq!(
        pretty_print("build --x \"\"", &flags).unwrap(),
        "build --x \"\"\n"
    );
    assert_eq!(
        pretty_print("build --x=\"\"", &flags).unwrap(),
        "build --x=\n"
    );
}

#[test]
fn test_pretty_print_comments() {
    // TODO
}

#[test]
fn test_pretty_print_whitespace() {
    let flags = load_packaged_bazel_flags("7.4.0");

    // Removes unnecessary whitespace
    assert_eq!(pretty_print("  build   ", &flags).unwrap(), "build\n");
    assert_eq!(
        pretty_print("  build   --x=1  --y", &flags).unwrap(),
        "build --x=1 --y\n"
    );
    assert_eq!(
        pretty_print("  build   --x=1  #   My comment   ", &flags).unwrap(),
        "build --x=1 # My comment\n"
    );
    // We keep whitespace if there are no commands / flags on the line.
    // The line might be part of an ASCII art and we don't want to destroy that
    assert_eq!(
        pretty_print("#   My comment   ", &flags).unwrap(),
        "#   My comment\n"
    );

    // We add a final new line, if it is missing
    assert_eq!(pretty_print("build", &flags).unwrap(), "build\n");

    // We keep empty lines
    assert_eq!(
        pretty_print("build\n\nbuild\n", &flags).unwrap(),
        "build\n\nbuild\n"
    );
}

#[test]
fn test_pretty_print_e2e() {
    let flags = load_packaged_bazel_flags("7.4.0");

    // TODO: More test cases

    // Does not mix separate lines together
    assert_eq!(
        pretty_print("build\n#a\ntest", &flags).unwrap(),
        "build\n#a\ntest\n"
    );
}
