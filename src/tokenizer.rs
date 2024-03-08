use chumsky::prelude::*;
use chumsky::Parser;

pub type Span = std::ops::Range<usize>;
pub type Spanned<T> = (T, Span);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Token {
    Comment, // Started by `#`
    Token(String),
}

// See rc_file.cc and util/strings.cc
pub fn tokenizer() -> impl Parser<char, Vec<Vec<Spanned<Token>>>, Error = Simple<char>> {
    // The token separators
    let separators = " \n\t\r";

    // All characters except for separators and `\` characters are part of tokens
    let raw_token_char = filter(|c| *c != '\\' && !separators.contains(*c));

    // Characters can be escaped with a `\` (except for newlines; those are treated in escaped_newline)
    let escaped_char = just('\\').ignore_then(filter(|c| *c != '\n' && *c != '\r'));

    // A newline. Either a Windows or a Unix newline
    let newline = just('\n').or(just('\r').then_ignore(just('\n')));

    // Newlines can be escaped using a `\`, but in contrast to other escaped parameters they
    // don't contribute any characters to the token value.
    let escaped_newline = just('\\').then(newline).to(Option::<char>::None);

    // A token character can be either a raw character, an escaped character
    // or an escaped newline.
    let token_char = (raw_token_char.or(escaped_char))
        .map(Option::<char>::Some)
        .or(escaped_newline);

    // A token consists of multiple token_chars
    let token = token_char.repeated().at_least(1).map_with_span(|v, span| {
        (
            Token::Token(v.iter().filter_map(|c| *c).collect::<String>()),
            span,
        )
    });

    let separator = one_of(" \t").repeated().at_least(1);

    // A line is a list of tokens
    let line = token
        .separated_by(separator.clone())
        .then_ignore(newline)
        .or(token.separated_by(separator).at_least(1).then_ignore(end()))
        .collect::<Vec<_>>();

    // An rc file contains multiple lines
    line.repeated().collect::<Vec<_>>().then_ignore(end())
}

#[test]
fn test_tokenizer() {
    // Our parser accepts empty strings
    assert_eq!(tokenizer().parse(""), Ok(Vec::from([])));

    let tokens_only = |e: &str| {
        tokenizer().parse(e).map(|v| {
            v.iter()
                .map(|v2| v2.iter().map(|e| e.0.clone()).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        })
    };
    let single_line = |t: &[Token]| Ok(Vec::from([Vec::from(t)]));
    let single_token = |t: Token| single_line(&[t]);

    // A simple token without escaped characters
    assert_eq!(
        tokens_only("abc"),
        single_token(Token::Token("abc".to_string()))
    );
    // Characters inside tokens can be escaped using `\`
    assert_eq!(
        tokens_only("a\\bc\\d"),
        single_token(Token::Token("abcd".to_string()))
    );
    // A `\` is escaped using another `\`
    assert_eq!(
        tokens_only("a\\\\b"),
        single_token(Token::Token("a\\b".to_string()))
    );
    // A `\` can also be used to escape whitespaces or tabs
    assert_eq!(
        tokens_only("a\\ b\\\tc"),
        single_token(Token::Token("a b\tc".to_string()))
    );

    // A whitespace seperates two tokens
    assert_eq!(
        tokens_only("ab c"),
        single_line(&[Token::Token("ab".to_string()), Token::Token("c".to_string())])
    );
    // Instead of a whitespace, one can also use a tab
    assert_eq!(
        tokens_only("ab\tc"),
        single_line(&[Token::Token("ab".to_string()), Token::Token("c".to_string())])
    );
    // Two tokens can also be separated by multiple whitespaces
    assert_eq!(
        tokens_only("ab\t \t  c"),
        single_line(&[Token::Token("ab".to_string()), Token::Token("c".to_string())])
    );
}
