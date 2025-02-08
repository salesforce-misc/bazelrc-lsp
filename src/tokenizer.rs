use core::fmt;

use chumsky::prelude::*;
use chumsky::Parser;

pub type Span = std::ops::Range<usize>;
pub type Spanned<T> = (T, Span);

#[derive(Debug, PartialEq, Clone, Eq, Hash)]
pub enum Token {
    Token(String),
    Comment(String),
    Newline,
    EscapedNewline,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Token::Token(_) => write!(f, "token"),
            Token::Comment(_) => write!(f, "comment"),
            Token::Newline => write!(f, "\\n"),
            Token::EscapedNewline => write!(f, "escaped newline"),
        }
    }
}

// Tokenizer for bazelrc files.
//
// The syntax supported by bazelrc is primarily implementation-defined
// and it seems to be a bit ad-hoc.
//
// As such, rather exotic lines like
// > b"uil"d':o'pt --"x"='y'
// are valid. In this case, the line is equivalent to
// > build:opt --x=y
//
// See rc_file.cc and util/strings.cc from the Bazel source code
pub fn tokenizer() -> impl Parser<char, Vec<Spanned<Token>>, Error = Simple<char>> {
    // The token separators
    let specialchars = " \t\r\n\"\'#";

    // All characters except for separators and `\` characters are part of tokens
    let raw_token_char = filter(|c| *c != '\\' && !specialchars.contains(*c));

    // Characters can be escaped with a `\` (except for newlines; those are treated in escaped_newline)
    let escaped_char = just('\\').ignore_then(filter(|c| *c != '\n' && *c != '\r'));

    // A newline. Either a Windows or a Unix newline
    let newline_raw = just('\n').or(just('\r').ignore_then(just('\n')));
    let newline = newline_raw.map(|_| Token::Newline);

    // Newlines can be escaped using a `\`, but in contrast to other escaped parameters they
    // don't contribute any characters to the token value.
    let escaped_newline_raw = just('\\').ignore_then(newline_raw);
    let escaped_newline = escaped_newline_raw.map(|_| Token::EscapedNewline);

    // A token character can be either a raw character, an escaped character
    // or an escaped newline.
    let token_char = (raw_token_char.or(escaped_char))
        .map(Option::Some)
        .or(escaped_newline_raw.to(Option::<char>::None));

    // A token consists of multiple token_chars
    let unquoted_token_raw = token_char.repeated().at_least(1);

    // Quoted tokens with `"`
    let dquoted_token_raw = just('"')
        .ignore_then(token_char.or(one_of(" \t\'#").map(Option::Some)).repeated())
        .then_ignore(just('"'));

    // Quoted tokens with `'`
    let squoted_token_raw = just('\'')
        .ignore_then(token_char.or(one_of(" \t\"#").map(Option::Some)).repeated())
        .then_ignore(just('\''));

    // Quoted tokens. Either with `"` or with `'`
    let quoted_token_raw = dquoted_token_raw.or(squoted_token_raw);

    // Mixed tokens, consisting of both quoted and unquoted parts
    let mixed_token = unquoted_token_raw
        .or(quoted_token_raw)
        .repeated()
        .at_least(1)
        .flatten()
        .map(|v| Token::Token(v.iter().filter_map(|c| *c).collect::<String>()));

    // Comments go until the end of line.
    // However a newline might be escaped using `\`
    let comment = just('#')
        .ignore_then(escaped_newline_raw.or(one_of("\n\r").not()).repeated())
        .collect::<String>()
        .map(Token::Comment);

    // Detect `command` and `command:config` in the beginnig of a line
    let token = choice((comment, escaped_newline, newline, mixed_token))
        .recover_with(skip_then_retry_until([]))
        .map_with_span(|tok, span| (tok, span));

    token
        .padded_by(one_of(" \t").repeated())
        .repeated()
        .collect::<Vec<_>>()
        .then_ignore(end())
}

#[test]
fn test_newlines() {
    // Our tokenizer accepts empty strings
    assert_eq!(tokenizer().parse(""), Ok(Vec::from([])));

    // `\n` and `\r\n``separate lines.
    // Lines can have leading and trailing whitespace.
    // We also preserve empty lines
    assert_eq!(
        tokenizer().parse("cmd\n\r\n\ncmd -x \n"),
        Ok(Vec::from([
            (Token::Token("cmd".to_string()), 0..3),
            (Token::Newline, 3..4),
            (Token::Newline, 4..6),
            (Token::Newline, 6..7),
            (Token::Token("cmd".to_string()), 7..10),
            (Token::Token("-x".to_string()), 11..13),
            (Token::Newline, 14..15),
        ]))
    );

    // Newlines can be escaped
    assert_eq!(
        tokenizer().parse("cmd \\\n -x\n"),
        Ok(Vec::from([
            (Token::Token("cmd".to_string()), 0..3),
            (Token::EscapedNewline, 4..6),
            (Token::Token("-x".to_string()), 7..9),
            (Token::Newline, 9..10),
        ]))
    );
}

#[test]
fn test_tokens() {
    let flags_only = |e: &str| {
        tokenizer().parse(e).map(|v| {
            v.iter()
                // Remove positions
                .map(|v2| v2.0.clone())
                .collect::<Vec<Token>>()
        })
    };
    let token_vec = |t: &[String]| {
        Ok(t.iter()
            .map(|s| Token::Token(s.to_string()))
            .collect::<Vec<_>>())
    };

    macro_rules! assert_single_flag {
        ($a1:expr, $a2:expr) => {
            assert_eq!(flags_only($a1), token_vec(&[$a2.to_string()]));
        };
    }

    // A simple token without escaped characters
    assert_single_flag!("abc", "abc");
    // Characters inside tokens can be escaped using `\`
    assert_single_flag!("a\\bc\\d", "abcd");
    // A `\` is escaped using another `\`
    assert_single_flag!("a\\\\b", "a\\b");
    // A `\` can also be used to escape whitespaces or tabs
    assert_single_flag!("a\\ b\\\tc", "a b\tc");

    // A token can contain be escaped using `"`
    assert_single_flag!("\"a b\tc\"", "a b\tc");
    // Instead of `"`, one can also use `'` to escape
    assert_single_flag!("'a b\tc'", "a b\tc");
    // Inside `"`, other `"` can be escaped. `'` can be included unescaped
    assert_single_flag!("\"a\\\"b'c\"", "a\"b'c");
    // Inside `'`, other `'` can be escaped. `"` can be included unescaped
    assert_single_flag!("'a\"b\\'c'", "a\"b'c");

    // Quoted parts can also appear in the middle of tokens
    assert_single_flag!("abc' cd\t e\\''fg\"h i\"j", "abc cd\t e'fgh ij");

    // A whitespace separates two tokens
    assert_eq!(
        flags_only("ab c"),
        token_vec(&["ab".to_string(), "c".to_string()])
    );
    // Instead of a whitespace, one can also use a tab
    assert_eq!(
        flags_only("ab\tc"),
        token_vec(&["ab".to_string(), "c".to_string()])
    );
    // Two tokens can also be separated by multiple whitespaces
    assert_eq!(
        flags_only("ab\t \t  c"),
        token_vec(&["ab".to_string(), "c".to_string()])
    );
    // Multiple quoted tokens
    assert_eq!(
        flags_only("\"t 1\" 't 2'"),
        token_vec(&["t 1".to_string(), "t 2".to_string()])
    );

    // A token can be continued on the next line using a `\`
    assert_single_flag!("a\\\nbc", "abc".to_string());
    // A quoted token does not continue across lines
    assert!(tokenizer().parse("'my\ntoken'").is_err());
    // But a quoted token can contain escaped newlines
    assert_single_flag!("'my\\\ntoken'", "mytoken".to_string());

    // `#` inside a quoted token does not start a token
    assert_single_flag!("'a#c'", "a#c".to_string());
    // `#` can be escaped as part of a token
    assert_single_flag!("a\\#c", "a#c".to_string());
}

#[test]
fn test_comments() {
    // Comments
    assert_eq!(
        tokenizer().parse(" # my comment\n#2nd comment"),
        Ok(vec!(
            (Token::Comment(" my comment".to_string()), 1..13),
            (Token::Newline, 13..14),
            (Token::Comment("2nd comment".to_string()), 14..26)
        ))
    );
    // Comments can be continued across lines with `\`
    assert_eq!(
        tokenizer().parse(" # my\\\nco\\mment"),
        Ok(vec!((Token::Comment(" my\nco\\mment".to_string()), 1..15)))
    );

    // Comments can even start in the middle of a token, without a whitespace
    assert_eq!(
        tokenizer().parse("flag#comment"),
        Ok(vec!(
            (Token::Token("flag".to_string()), 0..4),
            (Token::Comment("comment".to_string()), 4..12)
        ))
    );
}
