use ropey::Rope;
use tower_lsp::lsp_types::{Position, Range};

use crate::tokenizer::Span;

pub fn lsp_pos_to_offset(rope: &Rope, pos: &Position) -> Option<usize> {
    let line_byte = rope.try_line_to_byte(pos.line as usize).ok()?;
    let char_byte = rope
        .try_char_to_byte(rope.try_utf16_cu_to_char(pos.character as usize).ok()?)
        .ok()?;
    Some(line_byte + char_byte)
}

pub fn offset_to_lsp_pos(rope: &Rope, pos: usize) -> Option<Position> {
    let line = rope.byte_to_line(pos);
    let first = rope.char_to_utf16_cu(rope.line_to_char(line));
    let character = rope.char_to_utf16_cu(rope.byte_to_char(pos)) - first;
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
