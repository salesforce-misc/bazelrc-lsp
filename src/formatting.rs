use ropey::Rope;
use serde::{Deserialize, Serialize};
use tower_lsp::lsp_types::TextEdit;

use crate::{
    bazel_flags::BazelFlags,
    lsp_utils::range_to_lsp,
    parser::{parse_from_str, Line, ParserResult},
    tokenizer::Span,
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

pub fn format_line_into(out: &mut String, line: &Line, mut use_line_continuations: bool) {
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

    use_line_continuations =
        use_line_continuations && line.flags.len() >= 2 && line.comment.is_none();

    // Format the flags
    for flag in &line.flags {
        if non_empty {
            if use_line_continuations {
                out.push_str(" \\\n    ");
            } else {
                out.push(' ');
            }
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

pub fn format_line(line: &Line, use_line_continuations: bool) -> String {
    let mut out = String::with_capacity(line.span.end - line.span.start);
    format_line_into(&mut out, line, use_line_continuations);
    out
}

// Should lines be combined / split when formatting bazelrc files?
#[derive(PartialEq, Eq, Default, Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FormatLineFlow {
    // Do not reflow lines
    #[default]
    Keep,
    // Combine subsequent commands and use `\\` line continuations
    LineContinuations,
    // Put each flag on a separate line
    SeparateLines,
    // Put all flags on a single line
    SingleLine,
}

pub fn reflow_lines(lines: &[Line], line_flow: FormatLineFlow) -> Vec<Line> {
    let mut result1 = Vec::<Line>::with_capacity(lines.len());
    match line_flow {
        FormatLineFlow::Keep => result1.extend(lines.iter().cloned()),
        FormatLineFlow::SingleLine | FormatLineFlow::LineContinuations => {
            for l in lines {
                // Check if we should merge with the previous line
                if let Some(prev_line) = result1.last_mut() {
                    if l.command.as_ref().map(|c| &c.0) == prev_line.command.as_ref().map(|c| &c.0)
                        && l.config.as_ref().map(|c| &c.0)
                            == prev_line.config.as_ref().map(|c| &c.0)
                        && l.command
                            .as_ref()
                            .map(|c| c.0 != "import" && c.0 != "try-import")
                            .unwrap_or(true)
                        && l.comment.is_none()
                        && prev_line.comment.is_none()
                    {
                        // Merge with previous
                        prev_line.flags.extend(l.flags.iter().cloned());
                        prev_line.span.end = l.span.end;
                        continue;
                    }
                }
                result1.push(l.clone());
            }
        }
        FormatLineFlow::SeparateLines => {
            for l in lines {
                if l.flags.is_empty() {
                    result1.push(l.clone());
                }
                for (i, flag) in l.flags.iter().enumerate() {
                    let comment = if i == 0 { l.comment.clone() } else { None };
                    let span = if i == 0 {
                        l.span.clone()
                    } else {
                        Span {
                            start: l.span.end,
                            end: l.span.end,
                        }
                    };
                    result1.push(Line {
                        command: l.command.clone(),
                        config: l.config.clone(),
                        flags: vec![flag.clone()],
                        comment,
                        span,
                    })
                }
            }
        }
    }
    let mut result2 = Vec::<Line>::with_capacity(result1.len());
    let is_line_empty = |l: &Line| {
        l.command.is_none() && l.config.is_none() && l.flags.is_empty() && l.comment.is_none()
    };
    for l in result1.into_iter() {
        // Copy over all non-empty lines
        if !is_line_empty(&l) {
            result2.push(l);
            continue;
        }
        if let Some(prev_line) = result2.last_mut() {
            if is_line_empty(prev_line) {
                // Merge with previous line if it is also empty
                prev_line.span.end = l.span.end;
            } else {
                result2.push(l);
            }
        }
    }
    // We don't want to have an empty line at the end of the file
    if result2.last().map(is_line_empty).unwrap_or(false) {
        let removed_line = result2.pop().unwrap();
        if let Some(last_line) = result2.last_mut() {
            last_line.span.end = removed_line.span.end;
        }
    }
    result2
}

// Gets the LSP edits for reformatting a line range
pub fn get_text_edits_for_lines(
    lines: &[Line],
    rope: &Rope,
    line_flow: FormatLineFlow,
) -> Vec<TextEdit> {
    reflow_lines(lines, line_flow)
        .iter()
        .filter_map(|line| {
            let use_line_continuations = line_flow == FormatLineFlow::LineContinuations;
            let formatted = format_line(line, use_line_continuations);
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

// Parse and pretty-print the given string
pub fn pretty_print(
    str: &str,
    bazel_flags: &BazelFlags,
    line_flow: FormatLineFlow,
) -> Result<String, Vec<String>> {
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
    lines = reflow_lines(&lines, line_flow);
    let use_line_continuations = line_flow == FormatLineFlow::LineContinuations;
    let mut out = String::with_capacity(str.len());
    for line in lines {
        format_line_into(&mut out, &line, use_line_continuations);
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
    let lf = FormatLineFlow::Keep;

    // Command & config names
    assert_eq!(pretty_print("build", &flags, lf).unwrap(), "build\n");
    assert_eq!(
        pretty_print("build:opt", &flags, lf).unwrap(),
        "build:opt\n"
    );
    assert_eq!(
        pretty_print("build:o\\ p\\ t", &flags, lf).unwrap(),
        "build:\"o p t\"\n"
    );
    assert_eq!(
        pretty_print("buil\" d:o p\"\\ t", &flags, lf).unwrap(),
        "\"buil d\":\"o p t\"\n"
    );
    // Invalid command & config names, but should still work
    assert_eq!(pretty_print(":opt", &flags, lf).unwrap(), ":opt\n");
}

#[test]
fn test_pretty_print_flags() {
    let flags = load_packaged_bazel_flags("7.4.0");
    let lf = FormatLineFlow::Keep;

    // Flags (also works without a command, although that is strictly speaking invalid)
    assert_eq!(pretty_print("--x", &flags, lf).unwrap(), "--x\n");
    assert_eq!(
        pretty_print("--x=abc123", &flags, lf).unwrap(),
        "--x=abc123\n"
    );
    // Normalizes quoting and whitespaces
    assert_eq!(
        pretty_print("-\"-x=abc12\"3", &flags, lf).unwrap(),
        "--x=abc123\n"
    );
    assert_eq!(
        pretty_print("--\\x=a\\bc", &flags, lf).unwrap(),
        "--x=abc\n"
    );
    assert_eq!(
        pretty_print("--x=a\\ bc\"1 2 3\"", &flags, lf).unwrap(),
        "--x=\"a bc1 2 3\"\n"
    );
    assert_eq!(
        pretty_print("--x\\ =a\\ b", &flags, lf).unwrap(),
        "\"--x \"=\"a b\"\n"
    );
    // Normalizes empty strings
    assert_eq!(pretty_print("--x=\"\"", &flags, lf).unwrap(), "--x=\n");
    // Removes whitespaces between flags
    assert_eq!(
        pretty_print("--x=1    --y=2", &flags, lf).unwrap(),
        "--x=1 --y=2\n"
    );
}

#[test]
fn test_pretty_print_combined_flags() {
    let flags = load_packaged_bazel_flags("7.4.0");
    let lf = FormatLineFlow::Keep;

    // The `--copt` flag expects an argument and hence consumes the
    // following `--std=c++20`. `--std=c++20` should not raise
    // an error about an unrecognized Bazel flag.
    assert_eq!(
        pretty_print("build --copt --std=c++20", &flags, lf).unwrap(),
        "build --copt=--std=c++20\n"
    );
    // On the other hand, `--keep_going` only takes an optional value.
    // Hence, the `true` is interpreted as a separate flag, which then triggers
    // an error.
    assert_eq!(
        pretty_print("build --keep_going --foobar", &flags, lf).unwrap(),
        "build --keep_going --foobar\n"
    );

    // Leaves abbreviated flag names alone. `-cdbg` would not be valid.
    assert_eq!(
        pretty_print("build -c dbg", &flags, lf).unwrap(),
        "build -c dbg\n"
    );

    // Handles empty parameters correctly
    assert_eq!(
        pretty_print("build --x \"\"", &flags, lf).unwrap(),
        "build --x \"\"\n"
    );
    assert_eq!(
        pretty_print("build --x=\"\"", &flags, lf).unwrap(),
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
    let lf = FormatLineFlow::Keep;

    // Removes unnecessary whitespace
    assert_eq!(pretty_print("  build   ", &flags, lf).unwrap(), "build\n");
    assert_eq!(
        pretty_print("  build   --x=1  --y", &flags, lf).unwrap(),
        "build --x=1 --y\n"
    );
    assert_eq!(
        pretty_print("  build   --x=1  #   My comment   ", &flags, lf).unwrap(),
        "build --x=1 # My comment\n"
    );
    // We keep whitespace if there are no commands / flags on the line.
    // The line might be part of an ASCII art and we don't want to destroy that
    assert_eq!(
        pretty_print("#   My comment   ", &flags, lf).unwrap(),
        "#   My comment\n"
    );
}

#[test]
fn test_pretty_print_newlines() {
    let flags = load_packaged_bazel_flags("7.4.0");
    let lf = FormatLineFlow::Keep;

    // We add a final new line, if it is missing
    assert_eq!(pretty_print("build", &flags, lf).unwrap(), "build\n");

    // We keep empty lines
    assert_eq!(
        pretty_print("build\n\nbuild\n", &flags, lf).unwrap(),
        "build\n\nbuild\n"
    );

    // Multiple empty lines are combined into a single empty line
    assert_eq!(
        pretty_print("build\n\n\n\n\nbuild\n", &flags, lf).unwrap(),
        "build\n\nbuild\n"
    );

    // Empty lines at the end of the file are removed
    assert_eq!(pretty_print("build\n\n\n", &flags, lf).unwrap(), "build\n");

    // Comments are kept on separate lines
    assert_eq!(
        pretty_print("build\n#a\ntest", &flags, lf).unwrap(),
        "build\n#a\ntest\n"
    );
}

#[test]
fn test_pretty_print_line_styles() {
    let flags = load_packaged_bazel_flags("7.4.0");

    let input = "build:c1 --a=b\n\
        build:c1 --c=d\n\
        build:c2 --e=f  --g=h\n\
        build:c3 --xyz";

    assert_eq!(
        pretty_print(input, &flags, FormatLineFlow::LineContinuations).unwrap(),
        "build:c1 \\\n    --a=b \\\n    --c=d\n\
         build:c2 \\\n    --e=f \\\n    --g=h\n\
         build:c3 --xyz\n"
    );

    assert_eq!(
        pretty_print(input, &flags, FormatLineFlow::SeparateLines).unwrap(),
        "build:c1 --a=b\n\
         build:c1 --c=d\n\
         build:c2 --e=f\n\
         build:c2 --g=h\n\
         build:c3 --xyz\n"
    );

    assert_eq!(
        pretty_print(input, &flags, FormatLineFlow::SingleLine).unwrap(),
        "build:c1 --a=b --c=d\n\
         build:c2 --e=f --g=h\n\
         build:c3 --xyz\n"
    );

    assert_eq!(
        pretty_print(
            "import \"a.bazelrc\"\nimport \"b.bazelrc\"",
            &flags,
            FormatLineFlow::SingleLine
        )
        .unwrap(),
        "import a.bazelrc\n\
         import b.bazelrc\n"
    );
}
