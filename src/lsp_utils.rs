use ropey::Rope;
use tower_lsp::lsp_types::{Position, Range};

use crate::tokenizer::Span;

#[derive(Clone, Copy, Debug)]
pub enum LspPositionEncoding {
    UTF8,
    UTF16,
    UTF32,
}

pub fn decode_lsp_pos(rope: &Rope, pos: &Position, encoding: LspPositionEncoding) -> Option<usize> {
    let line_byte = rope.try_line_to_byte(pos.line as usize).ok()?;
    let line_rope = rope.get_byte_slice(line_byte..)?;
    let char_byte = match encoding {
        LspPositionEncoding::UTF8 => pos.character as usize,
        LspPositionEncoding::UTF16 => line_rope
            .try_char_to_byte(
                line_rope
                    .try_utf16_cu_to_char(pos.character as usize)
                    .ok()?,
            )
            .ok()?,
        LspPositionEncoding::UTF32 => line_rope.try_char_to_byte(pos.character as usize).ok()?,
    };
    Some(line_byte + char_byte)
}

pub fn encode_lsp_pos(rope: &Rope, pos: usize, encoding: LspPositionEncoding) -> Option<Position> {
    let line = rope.byte_to_line(pos);
    let line_byte = rope.line_to_byte(line);
    let line_char_pos = pos - line_byte;
    let line_rope = rope.byte_slice(line_byte..);
    let character = match encoding {
        LspPositionEncoding::UTF8 => line_char_pos,
        LspPositionEncoding::UTF16 => {
            line_rope.char_to_utf16_cu(line_rope.byte_to_char(line_char_pos))
        }
        LspPositionEncoding::UTF32 => line_rope.byte_to_char(line_char_pos),
    };
    Some(Position {
        line: line.try_into().ok()?,
        character: character.try_into().ok()?,
    })
}

pub fn encode_lsp_range(rope: &Rope, span: &Span, encoding: LspPositionEncoding) -> Option<Range> {
    Some(Range {
        start: encode_lsp_pos(rope, span.start, encoding)?,
        end: encode_lsp_pos(rope, span.end, encoding)?,
    })
}

#[cfg(test)]
fn test_encode(str: &str, pos: usize, encoding: LspPositionEncoding) -> (u32, u32) {
    let rope = Rope::from_str(str);
    let pos = encode_lsp_pos(&rope, pos, encoding).unwrap();
    (pos.line, pos.character)
}

#[cfg(test)]
fn test_decode(str: &str, pos: (u32, u32), encoding: LspPositionEncoding) -> usize {
    let rope: Rope = Rope::from_str(str);
    return decode_lsp_pos(
        &rope,
        &Position {
            line: pos.0,
            character: pos.1,
        },
        encoding,
    )
    .unwrap();
}

#[test]
fn test_position_encoding() {
    assert_eq!(test_encode("a∂b", 0, LspPositionEncoding::UTF8), (0, 0));
    assert_eq!(test_encode("a∂b", 0, LspPositionEncoding::UTF16), (0, 0));
    assert_eq!(test_encode("a∂b", 0, LspPositionEncoding::UTF32), (0, 0));
    assert_eq!(test_encode("a∂b", 1, LspPositionEncoding::UTF8), (0, 1));
    assert_eq!(test_encode("a∂b", 1, LspPositionEncoding::UTF16), (0, 1));
    assert_eq!(test_encode("a∂b", 1, LspPositionEncoding::UTF32), (0, 1));
    assert_eq!(test_encode("a∂b", 4, LspPositionEncoding::UTF8), (0, 4));
    assert_eq!(test_encode("a∂b", 4, LspPositionEncoding::UTF16), (0, 2));
    assert_eq!(test_encode("a∂b", 4, LspPositionEncoding::UTF32), (0, 2));
}

#[test]
fn test_position_roundtrip() {
    let test_str = "aüé\na∂c\nfire 🔥🔥 fire";
    for encoding in [
        LspPositionEncoding::UTF8,
        LspPositionEncoding::UTF16,
        LspPositionEncoding::UTF32,
    ] {
        for idx in test_str.char_indices() {
            let lsp_pos = test_encode(test_str, idx.0, encoding);
            let decoded_idx = test_decode(test_str, lsp_pos, encoding);
            assert_eq!(
                idx.0, decoded_idx,
                "encoding={:?} idx={:?} lsp_pos={:?} char={:?}",
                encoding, idx.0, lsp_pos, idx.1
            );
        }
    }
}
