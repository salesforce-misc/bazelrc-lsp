use std::collections::BTreeMap;

use crate::{
    parser::{parse_from_str, Line},
    tokenizer::Span,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IndexEntryKind {
    Command,
    Config,
    FlagValue(usize),
    FlagName(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexEntry {
    pub span: Span,
    pub line_nr: usize,
    pub kind: IndexEntryKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexedLines {
    pub lines: Vec<Line>,
    reverse_token_idx: BTreeMap<usize, IndexEntry>,
    reverse_line_idx: BTreeMap<usize, usize>,
}

impl IndexedLines {
    pub fn from_lines(lines: Vec<Line>) -> IndexedLines {
        let mut reverse_token_idx_entries = Vec::<(usize, IndexEntry)>::new();
        let mut reverse_line_idx_entries = Vec::<(usize, usize)>::new();

        for (line_nr, line) in lines.iter().enumerate() {
            reverse_line_idx_entries.push((line.span.start, line_nr));

            // Helper function to add a token to the index
            let mut add_token_to_idx = |span: &Span, kind: IndexEntryKind| {
                reverse_token_idx_entries.push((
                    span.start,
                    IndexEntry {
                        span: span.clone(),
                        line_nr,
                        kind,
                    },
                ));
            };

            // Index the command
            if let Some(cmd) = &line.command {
                add_token_to_idx(&cmd.1, IndexEntryKind::Command);
            }
            // Index the config
            if let Some(config) = &line.config {
                add_token_to_idx(&config.1, IndexEntryKind::Config);
            }
            // Index the flags
            for (flag_nr, flag) in line.flags.iter().enumerate() {
                if let Some(name) = &flag.name {
                    add_token_to_idx(&name.1, IndexEntryKind::FlagName(flag_nr));
                }
                if let Some(value) = &flag.value {
                    add_token_to_idx(&value.1, IndexEntryKind::FlagValue(flag_nr));
                }
            }
        }
        IndexedLines {
            lines,
            reverse_token_idx: BTreeMap::from_iter(reverse_token_idx_entries),
            reverse_line_idx: BTreeMap::from_iter(reverse_line_idx_entries),
        }
    }

    pub fn find_linenr_at_position(&self, pos: usize) -> Option<usize> {
        self.reverse_line_idx
            .values()
            .find(|e| self.lines[**e].span.contains(&pos))
            .map(|e| *e)
        /* TODO use 'upper_bound'
        self.reverse_idx
            .upper_bound(Bound::Included(&pos))
            .value()
            .filter(|s| s.span.contains(&pos))
        */
    }

    pub fn find_line_at_position(&self, pos: usize) -> Option<&Line> {
        self.find_linenr_at_position(pos).and_then(|i| self.lines.get(i))
    }

    pub fn find_symbol_at_position(&self, pos: usize) -> Option<&IndexEntry> {
        self.reverse_token_idx
            .values()
            .find(|e| e.span.contains(&pos))
        /* TODO use 'upper_bound' */
    }
}

#[test]
#[rustfmt::skip]
fn test_command_specifier() {
    let index = IndexedLines::from_lines(
        parse_from_str(
            "# config
common --remote_cache= --disk_cache=
build:opt --upload_results=false
    ",
        )
        .lines,
    );

    // Test the line index
    assert_eq!(index.reverse_line_idx, BTreeMap::<usize, usize>::from([
        (0, 0), (9, 1), (46, 2),
    ]));

    assert_eq!(index.find_linenr_at_position(0), Some(0));
    assert_eq!(index.find_linenr_at_position(1), Some(0));
    assert_eq!(index.find_linenr_at_position(9), Some(1));
    assert_eq!(index.find_linenr_at_position(10), Some(1));
    assert_eq!(index.find_linenr_at_position(40), Some(1));
    assert_eq!(index.find_linenr_at_position(48), Some(2));

    // Test the token index
    assert_eq!(index.reverse_token_idx, BTreeMap::<usize, IndexEntry>::from([
        (9, IndexEntry { span: 9..15, line_nr: 1, kind: IndexEntryKind::Command }),
        (16, IndexEntry { span: 16..30, line_nr: 1, kind: IndexEntryKind::FlagName(0) }),
        (30, IndexEntry { span: 30..31, line_nr: 1, kind: IndexEntryKind::FlagValue(0) }),
        (32, IndexEntry { span: 32..44, line_nr: 1, kind: IndexEntryKind::FlagName(1) }),
        (44, IndexEntry { span: 44..45, line_nr: 1, kind: IndexEntryKind::FlagValue(1) }),
        (46, IndexEntry { span: 46..51, line_nr: 2, kind: IndexEntryKind::Command }),
        (51, IndexEntry { span: 51..55, line_nr: 2, kind: IndexEntryKind::Config }),
        (56, IndexEntry { span: 56..72, line_nr: 2, kind: IndexEntryKind::FlagName(0) }),
        (72, IndexEntry { span: 72..78, line_nr: 2, kind: IndexEntryKind::FlagValue(0) }),
    ]));

    assert_eq!(index.find_symbol_at_position(20).unwrap().kind, IndexEntryKind::FlagName(0));
}
