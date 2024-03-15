use ropey::Rope;
use tower_lsp::lsp_types::{SemanticToken, SemanticTokenType};

use crate::parser::{Line, Span};

pub const LEGEND_TYPE: &[SemanticTokenType] = &[
    SemanticTokenType::COMMENT,
    SemanticTokenType::KEYWORD, // For the `build`, `common`, `startup` commands
    SemanticTokenType::NAMESPACE, // For the `:opt` config name
    SemanticTokenType::VARIABLE, // For the flag names
    SemanticTokenType::STRING,  // For the flag values
];

#[derive(Debug)]
pub struct RCSemanticToken {
    pub start: usize,
    pub end: usize,
    pub token_type: usize,
}

pub fn create_semantic_token(span: &Span, ttype: &SemanticTokenType) -> RCSemanticToken {
    RCSemanticToken {
        start: span.start,
        end: span.end,
        token_type: LEGEND_TYPE.iter().position(|item| item == ttype).unwrap(),
    }
}

/// Creates semantic tokens from the lexer tokens
pub fn semantic_tokens_from_tokens(lines: &[Line]) -> Vec<RCSemanticToken> {
    let mut tokens = Vec::<RCSemanticToken>::new();

    tokens.extend(
        lines
            .iter()
            .filter_map(|line| {
                if let Some(comment) = &line.comment {
                    Some(create_semantic_token(
                        &comment.1,
                        &SemanticTokenType::COMMENT,
                    ))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>(),
    );
    tokens
}

// Converts our internal semantic tokens to the LSP representation of tokens
pub fn convert_to_lsp_tokens(rope: &Rope, semtoks: &[RCSemanticToken]) -> Vec<SemanticToken> {
    let mut pre_line = 0;
    let mut pre_start = 0;
    let lsp_tokens = semtoks
        .iter()
        .filter_map(|token| {
            let start_line = rope.try_byte_to_line(token.start).ok()?;
            let end_line = rope.try_byte_to_line(token.end).ok()?;
            let tokens = (start_line..(end_line + 1))
                .filter_map(|line| {
                    // Figure out start and end offset within line
                    let first = rope.try_line_to_char(line).ok()? as u32;
                    let start: u32;
                    let length: u32;
                    if line == start_line {
                        start = rope.try_byte_to_char(token.start).ok()? as u32 - first;
                    } else {
                        start = 0;
                    }
                    if line == end_line {
                        length = rope.try_byte_to_char(token.end).ok()? as u32 - first;
                    } else {
                        length = rope.get_line(line)?.len_chars() as u32;
                    }
                    // Compute deltas to previous token
                    assert!(line >= pre_line);
                    let delta_line = (line - pre_line) as u32;
                    pre_line = line;
                    let delta_start = if delta_line == 0 {
                        start - pre_start
                    } else {
                        start
                    };
                    pre_start = start;
                    // Build token
                    Some(SemanticToken {
                        delta_line,
                        delta_start,
                        length,
                        token_type: token.token_type as u32,
                        token_modifiers_bitset: 0,
                    })
                })
                .collect::<Vec<_>>();
            Some(tokens)
        })
        .flatten()
        .collect::<Vec<_>>();
    lsp_tokens
}
