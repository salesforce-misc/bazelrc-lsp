use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemTag, Documentation, MarkupContent, MarkupKind,
};

use crate::{
    bazel_flags::{BazelFlags, COMMAND_DOCS},
    line_index::{IndexEntryKind, IndexedLines},
};

fn complete_bazel_command(bazel_flags: &BazelFlags) -> Vec<CompletionItem> {
    bazel_flags
        .commands
        .iter()
        .map(|cmd| CompletionItem {
            label: cmd.clone(),
            commit_characters: Some(vec![':'.to_string()]),
            documentation: get_command_documentation(cmd),
            ..Default::default()
        })
        .collect::<Vec<_>>()
}

fn complete_bazel_flag(bazel_flags: &BazelFlags, command: &str) -> Vec<CompletionItem> {
    let exisiting_flags = bazel_flags.flags_by_commands.get(command);

    if exisiting_flags.is_none() {
        return vec![];
    }

    let relevant_flags = exisiting_flags
        .unwrap()
        .iter()
        .map(|i| &bazel_flags.flags[*i])
        .filter(|f| {
            // Hide undocumented flags
            if f.documentation_category == Some("UNDOCUMENTED".to_string()) {
                return false;
            }
            true
        });

    // The Bazel flags themselves...
    let mut completion_items: Vec<CompletionItem> = Vec::<CompletionItem>::new();
    completion_items.extend(relevant_flags.clone().map(|flag| {
        let label = if flag.requires_value() {
            format!("{}=", flag.name)
        } else {
            flag.name.to_string()
        };
        CompletionItem {
            label,
            documentation: get_flag_documentation(flag),
            commit_characters: Some(vec!['='.to_string()]),
            tags: Some(deprecated_tag(flag)),
            deprecated: Some(flag.is_deprecated()),
            ..Default::default()
        }
    }));

    // ... and their negations
    completion_items.extend(
        relevant_flags
            .filter(|flag| flag.has_negative_flag())
            .map(|flag| CompletionItem {
                label: format!("no{}", flag.name.clone()),
                documentation: get_flag_documentation(flag),
                tags: Some(deprecated_tag(flag)),
                deprecated: Some(flag.is_deprecated()),
                ..Default::default()
            }),
    );

    completion_items
}

fn deprecated_tag(flag: &crate::bazel_flags_proto::FlagInfo) -> Vec<CompletionItemTag> {
    if flag.is_deprecated() {
        vec![CompletionItemTag::DEPRECATED]
    } else {
        vec![]
    }
}

pub fn get_completion_items(
    bazel_flags: &BazelFlags,
    index: &IndexedLines,
    pos: usize,
) -> Vec<CompletionItem> {
    // For completion, the indices point between characters and not
    // at characters. We are generally interested in the token so far
    // *before* the cursor. Hence, we lookup `pos - 1` and not `pos`.
    let lookup_pos = if pos == 0 { 0 } else { pos - 1 };
    if let Some(entry) = index.find_symbol_at_position(lookup_pos) {
        let line = index.lines.get(entry.line_nr).unwrap();
        // Complete the item which the user is currently typing
        match entry.kind {
            IndexEntryKind::Command => complete_bazel_command(bazel_flags),
            IndexEntryKind::Config => vec![],
            IndexEntryKind::FlagName(_) => {
                if let Some(cmd) = &line.command {
                    complete_bazel_flag(bazel_flags, &cmd.0)
                } else {
                    // A flag should never be on a line without a command
                    // Don't auto-complete in this case, to not worsen
                    // any mistakes already made.
                    vec![]
                }
            }
            IndexEntryKind::FlagValue(_) => vec![],
        }
    } else if let Some(line) = index.find_line_at_position(lookup_pos) {
        // Not within any item, but on an existing line.
        if let Some(cmd) = &line.command {
            complete_bazel_flag(bazel_flags, &cmd.0)
        } else {
            vec![]
        }
    } else {
        // Outside any existing line, i.e. on a completely empty line
        // Complete the bazel command since that has to be at the beginning
        // of every line
        complete_bazel_command(bazel_flags)
    }
}

fn get_flag_documentation(flag: &crate::bazel_flags_proto::FlagInfo) -> Option<Documentation> {
    let mc = MarkupContent {
        kind: MarkupKind::Markdown,
        value: flag.get_documentation_markdown(),
    };
    Some(Documentation::MarkupContent(mc))
}

fn get_command_documentation(command: &str) -> Option<Documentation> {
    COMMAND_DOCS.get(command).map(|docs| {
        Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: docs.to_string(),
        })
    })
}
