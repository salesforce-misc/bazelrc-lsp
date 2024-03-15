use ropey::Rope;
use tower_lsp::lsp_types::{Position, Range};

use crate::tokenizer::Span;

pub fn lsp_pos_to_offset(rope: &Rope, pos: &Position) -> Option<usize> {
    let char = rope.try_line_to_char(pos.line as usize).ok()?;
    Some(char + pos.character as usize)
}

pub fn offset_to_lsp_pos(rope: &Rope, pos: usize) -> Option<Position> {
    let line = rope.try_byte_to_line(pos).ok()?;
    let first = rope.try_line_to_char(line).ok()?;
    let character = rope.try_byte_to_char(pos).ok()? - first;
    Some(Position {
        line: line.try_into().ok()?,
        character: character.try_into().ok()?,
    })
}

pub fn range_to_lsp(rope: &Rope, span: &Span) -> Option<Range> {
    Some(Range {
        start: offset_to_lsp_pos(rope, span.start)?,
        end: offset_to_lsp_pos(rope, span.end)?,
    })
}
