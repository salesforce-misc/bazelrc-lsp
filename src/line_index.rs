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
    reverse_idx: BTreeMap<usize, IndexEntry>,
}

impl IndexedLines {
    pub fn from_lines(lines: Vec<Line>) -> IndexedLines {
        let mut reverse_idx_entries = Vec::<(usize, IndexEntry)>::new();
        for (line_nr, line) in lines.iter().enumerate() {
            let mut add_to_idx = |span: &Span, kind: IndexEntryKind| {
                reverse_idx_entries.push((
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
                add_to_idx(&cmd.1, IndexEntryKind::Command);
            }
            // Index the config
            if let Some(config) = &line.config {
                add_to_idx(&config.1, IndexEntryKind::Config);
            }
            // Index the flags
            for (flag_nr, flag) in line.flags.iter().enumerate() {
                if let Some(name) = &flag.name {
                    add_to_idx(&name.1, IndexEntryKind::FlagName(flag_nr));
                }
                if let Some(value) = &flag.value {
                    add_to_idx(&value.1, IndexEntryKind::FlagValue(flag_nr));
                }
            }
        }
        IndexedLines {
            lines,
            reverse_idx: BTreeMap::<usize, IndexEntry>::from_iter(reverse_idx_entries),
        }
    }

    pub fn find_symbol_at_position(&self, pos: usize) -> Option<&IndexEntry> {
        self.reverse_idx
            .iter()
            .map(|e| e.1)
            .find(|e| e.span.contains(&pos))
        /* TODO use 'upper_bound'
        self.reverse_idx
            .upper_bound(Bound::Included(&pos))
            .value()
            .filter(|s| s.span.contains(&pos))
        */
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

    assert_eq!(index.reverse_idx, BTreeMap::<usize, IndexEntry>::from([
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
