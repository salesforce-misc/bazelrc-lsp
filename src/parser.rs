use chumsky::{error::Rich, Parser};

use crate::tokenizer::{tokenizer, Span, Spanned, Token};

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Flag {
    pub name: Option<Spanned<String>>,
    pub value: Option<Spanned<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Line {
    pub command: Option<Spanned<String>>,
    pub config: Option<Spanned<String>>,
    pub flags: Vec<Flag>,
    pub comment: Option<Spanned<String>>,
    // The span of this line (without the comment)
    pub span: Span,
}

pub struct ParserResult<'a> {
    pub tokens: Vec<Spanned<Token>>,
    pub lines: Vec<Line>,
    pub errors: Vec<Rich<'a, char>>,
}

// Splits a token at a given separator, keeping the position tracking
fn split_token(
    str: &str,
    span: &Span,
    orig: &str,
    sep: char,
) -> Option<(Spanned<String>, Spanned<String>)> {
    if let Some(split_pos) = str.find(sep) {
        let orig_slice = &orig[span.start..span.end];
        let orig_offset = orig_slice.find(sep).unwrap();
        let (p1, p2_) = str.split_at(split_pos);
        let (_, p2) = p2_.split_at(1);
        Some((
            (p1.to_string(), span.start..span.start + orig_offset),
            (p2.to_string(), (span.start + orig_offset..span.end)),
        ))
    } else {
        None
    }
}

fn parse_flag(str: &str, span: &Span, orig: &str) -> Flag {
    if str.starts_with('-') {
        // This is flag. Try to split at `=`
        if let Some((name, value)) = split_token(str, span, orig, '=') {
            Flag {
                name: Some(name),
                value: Some(value),
            }
        } else {
            Flag {
                name: Some((str.to_string(), span.clone())),
                value: None,
            }
        }
    } else {
        // This is only a value
        Flag {
            name: None,
            value: Some((str.to_string(), span.clone())),
        }
    }
}

fn parse(tokens: &[(Token, Span)], orig: &str) -> Vec<Line> {
    let mut result_lines = Vec::<Line>::new();

    let mut current_line_start = 0;
    let mut current_line = Option::<Line>::None;
    for t in tokens {
        match &t.0 {
            Token::Token(s) => {
                let line = current_line.get_or_insert_with(Default::default);
                // The first token is the command name
                if line.command.is_none() && line.flags.is_empty() && !s.starts_with('-') {
                    if let Some((command, config)) = split_token(s, &t.1, orig, ':') {
                        line.command = if command.0.is_empty() {
                            None
                        } else {
                            Some(command)
                        };
                        line.config = Some(config);
                    } else {
                        line.command = Some((s.clone(), t.1.clone()));
                        line.config = None
                    }
                } else {
                    // All other tokens are flags
                    line.flags.push(parse_flag(s, &t.1, orig));
                }
            }
            Token::Comment(s) => {
                let line = current_line.get_or_insert_with(Default::default);
                assert!(line.comment.is_none());
                line.comment = Some((s.clone(), t.1.clone()));
            }
            Token::Newline => {
                let mut line = current_line.take().unwrap_or_default();
                line.span = current_line_start..t.1.end;
                result_lines.push(line);
                current_line_start = t.1.end;
            }
            Token::EscapedNewline => (),
        };
    }
    if let Some(mut l) = current_line.take() {
        let implicit_final_newline = orig.len();
        l.span = current_line_start..implicit_final_newline;
        result_lines.push(l);
    }

    result_lines
}

// Parser for bazelrc files.
pub fn parse_from_str(str: &str) -> ParserResult<'_> {
    // Tokenize
    let tokenizer_result = tokenizer().parse(str);
    let tokens = tokenizer_result.output().unwrap_or(&Vec::new()).clone();
    let errors = tokenizer_result.into_errors();

    // Parse
    let lines = parse(&tokens, str);

    ParserResult {
        tokens,
        lines,
        errors,
    }
}

#[test]
fn test_command_specifier() {
    // The first token is the command name
    assert_eq!(
        parse_from_str("cmd").lines,
        Vec::from([Line {
            command: Some(("cmd".to_string(), 0..3)),
            span: 0..3,
            ..Default::default()
        },])
    );

    // The command name might be followed by `:config-name`
    assert_eq!(
        parse_from_str("cmd:my-config").lines,
        vec!(Line {
            command: Some(("cmd".to_string(), 0..3)),
            config: Some(("my-config".to_string(), 3..13)),
            span: 0..13,
            ..Default::default()
        })
    );

    // The config might contain arbitrarily complex escaped tokens
    assert_eq!(
        parse_from_str("cmd:my-\\ con'f ig'").lines,
        vec!(Line {
            command: Some(("cmd".to_string(), 0..3)),
            config: Some(("my- conf ig".to_string(), 3..18)),
            span: 0..18,
            ..Default::default()
        })
    );

    // The command combined with some actual arguments
    assert_eq!(
        parse_from_str("bu'ild\\:o'pt --x=y").lines,
        vec!(Line {
            command: Some(("build".to_string(), 0..7)),
            config: Some(("opt".to_string(), 7..12)),
            flags: vec!(Flag {
                name: Some(("--x".to_string(), 13..16)),
                value: Some(("y".to_string(), 16..18)),
            }),
            span: 0..18,
            ..Default::default()
        })
    );

    // In case the leading command name is missing, parse flags
    assert_eq!(
        parse_from_str("--x y").lines,
        vec!(Line {
            command: None,
            flags: vec!(
                Flag {
                    name: Some(("--x".to_string(), 0..3)),
                    value: None
                },
                Flag {
                    name: None,
                    value: Some(("y".to_string(), 4..5)),
                }
            ),
            span: 0..5,
            ..Default::default()
        })
    );

    // Parse something useful if the leading command name is missing, but a config name is provided
    assert_eq!(
        parse_from_str(":opt --x").lines,
        vec!(Line {
            command: None,
            config: Some(("opt".to_string(), 0..4)),
            flags: vec!(Flag {
                name: Some(("--x".to_string(), 5..8)),
                value: None
            },),
            span: 0..8,
            ..Default::default()
        })
    );
}

#[test]
fn test_flag_parsing() {
    // An unnamed flag with only a value
    assert_eq!(
        parse_from_str("build foo").lines,
        vec!(Line {
            command: Some(("build".to_string(), 0..5)),
            flags: vec!(Flag {
                name: None,
                value: Some(("foo".to_string(), 6..9)),
            }),
            span: 0..9,
            ..Default::default()
        })
    );

    // A long flag
    assert_eq!(
        parse_from_str("--x").lines,
        vec!(Line {
            command: None,
            flags: vec!(Flag {
                name: Some(("--x".to_string(), 0..3)),
                value: None
            }),
            span: 0..3,
            ..Default::default()
        })
    );

    // An abbreviated flag
    assert_eq!(
        parse_from_str("-x").lines,
        vec!(Line {
            command: None,
            flags: vec!(Flag {
                name: Some(("-x".to_string(), 0..2)),
                value: None
            }),
            span: 0..2,
            ..Default::default()
        })
    );

    // An `=` flag
    assert_eq!(
        parse_from_str("--x=y").lines,
        vec!(Line {
            command: None,
            flags: vec!(Flag {
                name: Some(("--x".to_string(), 0..3)),
                value: Some(("y".to_string(), 3..5)),
            }),
            span: 0..5,
            ..Default::default()
        })
    );
}

#[test]
fn test_comments() {
    // Comments
    assert_eq!(
        parse_from_str(" # my comment\n#2nd comment").lines,
        vec!(
            Line {
                comment: Some((" my comment".to_string(), 1..13)),
                span: 0..14,
                ..Default::default()
            },
            Line {
                comment: Some(("2nd comment".to_string(), 14..26)),
                span: 14..26,
                ..Default::default()
            }
        )
    );
    // Comments can be continued across lines with `\`
    assert_eq!(
        parse_from_str(" # my\\\nco\\mment").lines,
        vec!(Line {
            comment: Some((" my\nco\\mment".to_string(), 1..15)),
            span: 0..15,
            ..Default::default()
        })
    );

    // Comments can even start in the middle of a token, without a whitespace
    assert_eq!(
        parse_from_str("cmd #comment").lines,
        vec!(Line {
            command: Some(("cmd".to_string(), 0..3)),
            comment: Some(("comment".to_string(), 4..12)),
            span: 0..12,
            ..Default::default()
        })
    );
}

#[test]
fn test_empty_lines() {
    // Check that we keep also keep a representation for empty lines
    assert_eq!(
        parse_from_str("build --x=y\n\ncommon --z=w\n\n\n").lines,
        vec!(
            Line {
                command: Some(("build".to_string(), 0..5)),
                config: None,
                flags: vec!(Flag {
                    name: Some(("--x".to_string(), 6..9)),
                    value: Some(("y".to_string(), 9..11))
                }),
                comment: None,
                span: 0..12
            },
            Line {
                span: 12..13,
                ..Default::default()
            },
            Line {
                command: Some(("common".to_string(), 13..19)),
                config: None,
                flags: vec!(Flag {
                    name: Some(("--z".to_string(), 20..23)),
                    value: Some(("w".to_string(), 23..25))
                }),
                comment: None,
                span: 13..26
            },
            Line {
                span: 26..27,
                ..Default::default()
            },
            Line {
                span: 27..28,
                ..Default::default()
            },
        )
    );
}

#[test]
fn test_unicode() {
    // Check that we keep also keep a representation for empty lines
    assert_eq!(
        parse_from_str("build:🔥 --❄️=🔥").lines,
        vec!(Line {
            command: Some(("build".to_string(), 0..5)),
            config: Some(("🔥".to_string(), 5..10)),
            flags: vec!(Flag {
                name: Some(("--❄️".to_string(), 11..19)),
                value: Some(("🔥".to_string(), 19..24))
            }),
            comment: None,
            span: 0..24
        })
    );
}
