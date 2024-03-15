use tower_lsp::lsp_types::{CompletionItem, Documentation};

use crate::{bazel_flags::BazelFlags, bazel_flags_proto::FlagInfo};

fn get_documentation_string(flag: &FlagInfo) -> Option<Documentation> {
    let mut result = String::new();

    // First line: Flag name and short hand (if any)
    result += format!("--{}", flag.name).as_str();
    if let Some(abbr) = &flag.abbreviation {
        result += format!(" [-{}]", abbr).as_str();
    }
    // Followed by the documentation text
    if let Some(doc) = &flag.documentation {
        result += "\n\n";
        result += doc.as_str();
    }
    // And a list of tags
    result += "\n\n";
    if !flag.effect_tags.is_empty() {
        result += "Effect tags: ";
        result += flag
            .effect_tags
            .iter()
            .map(|t| t.to_lowercase())
            .collect::<Vec<_>>()
            .join(", ")
            .as_str();
        result += "\n";
    }
    if !flag.metadata_tags.is_empty() {
        result += "Tags: ";
        result += flag
            .metadata_tags
            .iter()
            .map(|t| t.to_lowercase())
            .collect::<Vec<_>>()
            .join(", ")
            .as_str();
        result += "\n";
    }
    if let Some(catgegory) = &flag.documentation_category {
        result += format!("Category: {}\n", catgegory.to_lowercase()).as_str();
    }

    //let docs = flag.documentation.as_ref()?;
    Some(Documentation::String(result.clone()))
}

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
        documentation: get_documentation_string(flag),
        commit_characters: Some(vec!['='.to_string(), ' '.to_string()]),
        ..Default::default()
    }));

    // All the negated Bazel flags
    completion_items.extend(
        documented_flags
            .filter(|flag| flag.has_negative_flag())
            .map(|flag| CompletionItem {
                label: format!("no{}", flag.name.clone()),
                documentation: get_documentation_string(flag),
                commit_characters: Some(vec!['='.to_string(), ' '.to_string()]),
                ..Default::default()
            }),
    );

    completion_items
}
