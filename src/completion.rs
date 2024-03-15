use tower_lsp::lsp_types::{CompletionItem, Documentation, MarkupContent, MarkupKind};

use crate::bazel_flags::BazelFlags;

pub fn get_completion_items(bazel_flags: &BazelFlags) -> Vec<CompletionItem> {
    let mut completion_items = Vec::<CompletionItem>::new();
    // All the Bazel modes
    completion_items.extend(
        bazel_flags
            .flags_by_commands
            .keys()
            .map(|cmd| CompletionItem {
                label: cmd.clone(),
                commit_characters: Some(vec![':'.to_string(), ' '.to_string()]),
                ..Default::default()
            }),
    );

    let documented_flags = bazel_flags.flags.iter().filter(|f| {
        if f.effect_tags.contains(&"NO_OP".to_string()) {
            return false;
        }
        if f.documentation_category == Some("UNDOCUMENTED".to_string()) {
            return false;
        }
        return true;
    });

    // All the Bazel flags
    completion_items.extend(documented_flags.clone().map(|flag| CompletionItem {
        label: flag.name.clone(),
        documentation: get_documentation(flag),
        commit_characters: Some(vec!['='.to_string(), ' '.to_string()]),
        ..Default::default()
    }));

    // All the negated Bazel flags
    completion_items.extend(
        documented_flags
            .filter(|flag| flag.has_negative_flag())
            .map(|flag| CompletionItem {
                label: format!("no{}", flag.name.clone()),
                documentation: get_documentation(flag),
                commit_characters: Some(vec!['='.to_string(), ' '.to_string()]),
                ..Default::default()
            }),
    );

    completion_items
}

fn get_documentation(flag: &crate::bazel_flags_proto::FlagInfo) -> Option<Documentation> {
    let mc = MarkupContent{ kind: MarkupKind::Markdown, value: flag.get_documentation_markdown() };
    Some(Documentation::MarkupContent(mc))
}
