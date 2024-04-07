use ropey::Rope;
use tower_lsp::lsp_types::{SemanticToken, SemanticTokenType};

use crate::{parser::Line, tokenizer::Span};

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
pub fn semantic_tokens_from_lines(lines: &[Line]) -> Vec<RCSemanticToken> {
    let mut tokens = Vec::<RCSemanticToken>::new();

    for line in lines {
        // Highlight commands
        if let Some(cmd) = &line.command {
            tokens.push(create_semantic_token(&cmd.1, &SemanticTokenType::KEYWORD))
        }

        // Highlight config names
        if let Some(config) = &line.config {
            tokens.push(create_semantic_token(
                &config.1,
                &SemanticTokenType::NAMESPACE,
            ))
        }

        // Highlight all the flags
        for flag in &line.flags {
            if let Some(name) = &flag.name {
                tokens.push(create_semantic_token(&name.1, &SemanticTokenType::VARIABLE))
            }
            if let Some(value) = &flag.value {
                tokens.push(create_semantic_token(&value.1, &SemanticTokenType::STRING))
            }
        }

        // Highlight comments
        if let Some(comment) = &line.comment {
            tokens.push(create_semantic_token(
                &comment.1,
                &SemanticTokenType::COMMENT,
            ))
        }
    }

    tokens
}

// Converts our internal semantic tokens to the LSP representation of tokens
pub fn convert_to_lsp_tokens(rope: &Rope, semtoks: &[RCSemanticToken]) -> Vec<SemanticToken> {
    let mut pre_line = 0;
    let mut pre_start = 0;
    let lsp_tokens = semtoks
        .iter()
        .filter_map(|token| {
            let start_line = rope.try_char_to_line(token.start).ok()?;
            let end_line = rope.try_char_to_line(token.end).ok()?;
            let tokens = (start_line..(end_line + 1))
                .filter_map(|line| {
                    // Figure out start and end offset within line
                    let first = rope.try_line_to_char(line).ok()? as u32;
                    let start: u32 = if line == start_line {
                        token.start as u32 - first
                    } else {
                        0
                    };
                    let end: u32 = if line == end_line {
                        token.end as u32 - first
                    } else {
                        rope.get_line(line).unwrap().len_chars() as u32
                    };
                    let length = end - start;
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
