use chumsky::prelude::*;
use chumsky::Parser;

pub type Span = std::ops::Range<usize>;
pub type Spanned<T> = (T, Span);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Token {
    Token(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
struct Line {
    tokens: Vec<Spanned<Token>>,
    comment: Option<Spanned<String>>,
}

// See rc_file.cc and util/strings.cc
pub fn tokenizer() -> impl Parser<char, Vec<Line>, Error = Simple<char>> {
    // The token separators
    let specialchars = " \t\r\n\"\'#";

    // All characters except for separators and `\` characters are part of tokens
    let raw_token_char = filter(|c| *c != '\\' && !specialchars.contains(*c));

    // Characters can be escaped with a `\` (except for newlines; those are treated in escaped_newline)
    let escaped_char = just('\\').ignore_then(filter(|c| *c != '\n' && *c != '\r'));

    // A newline. Either a Windows or a Unix newline
    let newline = just('\n').or(just('\r').then_ignore(just('\n')));

    // Newlines can be escaped using a `\`, but in contrast to other escaped parameters they
    // don't contribute any characters to the token value.
    let escaped_newline = just('\\').ignore_then(newline);

    // A token character can be either a raw character, an escaped character
    // or an escaped newline.
    let token_char = (raw_token_char.or(escaped_char))
        .map(Option::Some)
        .or(escaped_newline.to(Option::<char>::None));

    let finalize_token = |v: Vec<Option<char>>, span| {
        (
            Token::Token(v.iter().filter_map(|c| *c).collect::<String>()),
            span,
        )
    };

    // A token consists of multiple token_chars
    let unquoted_token_raw = token_char.repeated().at_least(1);
    // let unquoted_token = unquoted_token_raw.map_with_span(finalize_token);

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
    //let quoted_token = quoted_token_raw.map_with_span(finalize_token);

    // Mixed tokens, consisting of both quoted and unquoted parts
    let mixed_token = unquoted_token_raw
        .or(quoted_token_raw)
        .repeated()
        .at_least(1)
        .flatten()
        .map_with_span(finalize_token);

    // Tokens are separated by whitespace
    let separator = one_of(" \t").repeated().at_least(1);

    // Comments go until the end of line.
    // However a newline might be escaped using `\`
    let comment = just('#')
        .ignore_then(escaped_newline.or(one_of("\n\r").not()).repeated())
        .collect::<String>()
        .map_with_span(|v, span| (v, span));

    // A line is a list of tokens
    let line_content = mixed_token
        .separated_by(separator)
        .allow_leading()
        .allow_trailing()
        .then(comment.or_not())
        .map(|(tokens, comment)| Line { tokens, comment });

    line_content
        .separated_by(newline)
        .collect::<Vec<_>>()
        .then_ignore(end())
}

#[test]
fn test_tokenizer() {
    let tokens_only = |e: &str| {
        tokenizer().parse(e).map(|v| {
            v.iter()
                // Remove empty lines
                .filter(|l| !l.tokens.is_empty() || l.comment.is_some())
                // Remove positions
                .map(|v2| v2.tokens.iter().map(|e| e.0.clone()).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        })
    };
    let single_line = |t: &[Token]| Ok(Vec::from([Vec::from(t)]));
    let single_token = |t: Token| single_line(&[t]);

    macro_rules! assert_single_token {
        ($a1:expr, $a2:expr) => {
            assert_eq!(tokens_only($a1), single_token($a2));
        };
    }

    // Our parser accepts empty strings
    assert_eq!(tokens_only(""), Ok(Vec::from([])));

    // A simple token without escaped characters
    assert_single_token!("abc", Token::Token("abc".to_string()));
    // Characters inside tokens can be escaped using `\`
    assert_single_token!("a\\bc\\d", Token::Token("abcd".to_string()));
    // A `\` is escaped using another `\`
    assert_single_token!("a\\\\b", Token::Token("a\\b".to_string()));
    // A `\` can also be used to escape whitespaces or tabs
    assert_single_token!("a\\ b\\\tc", Token::Token("a b\tc".to_string()));

    // A token can contain be escaped using `"`
    assert_single_token!("\"a b\tc\"", Token::Token("a b\tc".to_string()));
    // Instead of `"`, one can also use `'` to escape
    assert_single_token!("'a b\tc'", Token::Token("a b\tc".to_string()));
    // Inside `"`, other `"` can be escaped. `'` can be included unescaped
    assert_single_token!("\"a\\\"b'c\"", Token::Token("a\"b'c".to_string()));
    // Inside `'`, other `'` can be escaped. `"` can be included unescaped
    assert_single_token!("'a\"b\\'c'", Token::Token("a\"b'c".to_string()));

    // Quoted parts can also appear in the middle of tokens
    assert_single_token!(
        "abc' cd\t e\\''fg\"h i\"j",
        Token::Token("abc cd\t e'fgh ij".to_string())
    );

    // A whitespace seperates two tokens
    assert_eq!(
        tokens_only("ab c"),
        single_line(&[
            Token::Token("ab".to_string()),
            Token::Token("c".to_string())
        ])
    );
    // Instead of a whitespace, one can also use a tab
    assert_eq!(
        tokens_only("ab\tc"),
        single_line(&[
            Token::Token("ab".to_string()),
            Token::Token("c".to_string())
        ])
    );
    // Two tokens can also be separated by multiple whitespaces
    assert_eq!(
        tokens_only("ab\t \t  c"),
        single_line(&[
            Token::Token("ab".to_string()),
            Token::Token("c".to_string())
        ])
    );
    // Multiple quoted tokens
    assert_eq!(
        tokens_only("\"t 1\" 't 2'"),
        single_line(&[
            Token::Token("t 1".to_string()),
            Token::Token("t 2".to_string())
        ])
    );

    // `\n` and `\r\n``separate lines.
    // Lines can have leading and trailing whitespace.
    // We also preserve empty lines
    assert_eq!(
        tokenizer().parse("line1\n\r\n\n line2 x \n"),
        Ok(Vec::from([
            Line {
                tokens: Vec::from([(Token::Token("line1".to_string()), 0..5)]),
                comment: None
            },
            // The empty lines are recorded
            Line::default(),
            Line::default(),
            // This line has content again
            Line {
                tokens: Vec::from([
                    (Token::Token("line2".to_string()), 10..15),
                    (Token::Token("x".to_string()), 16..17)
                ]),
                comment: None
            },
            // The final newline is also preserved
            Line::default(),
        ]))
    );

    // Comments
    assert_eq!(
        tokenizer().parse(" # my comment\n#2nd comment"),
        Ok(Vec::from([
            Line {
                tokens: Vec::from([]),
                comment: Some((" my comment".to_string(), 1..13))
            },
            Line {
                tokens: Vec::from([]),
                comment: Some(("2nd comment".to_string(), 14..26))
            }
        ]))
    );
    // Comments can be continued across lines with `\`
    assert_eq!(
        tokenizer().parse(" # my\\\nco\\mment"),
        Ok(Vec::from([Line {
            tokens: Vec::from([]),
            comment: Some((" my\nco\\mment".to_string(), 1..15))
        }]))
    );

    // A token can be continued on the next line using a `\`
    assert_single_token!("a\\\nbc", Token::Token("abc".to_string()));
    // A quoted token does not continue across lines
    assert!(
        tokenizer().parse("'my\ntoken'").is_err()
    );
    // But a quoted token can contain escaped newlines
    assert_single_token!("'my\\\ntoken'", Token::Token("mytoken".to_string()));

    // `#` inside a quoted token does not start a token
    assert_single_token!("'a#c'", Token::Token("a#c".to_string()));
    // `#` can be escaped as part of a token
    assert_single_token!("a\\#c", Token::Token("a#c".to_string()));
}
