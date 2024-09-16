use std::path::Path;

use tower_lsp::lsp_types::*;

use crate::{file_utils::resolve_bazelrc_path, line_index::IndexEntryKind, parser::Line};

pub fn get_definitions(
    file_path: &Path,
    kind: &IndexEntryKind,
    line: &Line,
) -> Option<GotoDefinitionResponse> {
    match kind {
        IndexEntryKind::FlagValue(flag_nr) => {
            let flag = &line.flags[*flag_nr];
            let command_name = &line.command?.0;
            if line.flags.len() != 1 {
                return None;
            }
            if *command_name != "import" && *command_name != "try-import" {
                return None;
            }

            let flag_value = &flag.value?.0;
            let path = resolve_bazelrc_path(file_path, flag_value)?;
            let url = Url::from_file_path(path).ok()?;
            Some(GotoDefinitionResponse::Scalar(Location {
                uri: url,
                range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 0,
                    },
                },
            }))
        }
        _ => None,
    }
}
