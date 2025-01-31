use phf::phf_map;
use prost::Message;
use std::{collections::HashMap, io::Cursor};

use crate::bazel_flags_proto::{FlagCollection, FlagInfo};

pub static COMMAND_DOCS: phf::Map<&'static str, &'static str> = phf_map! {
    // The command line docs, taken from the `bazel help`
    "analyze-profile" => "Analyzes build profile data.",
    "aquery" => "Analyzes the given targets and queries the action graph.",
    "build" => "Builds the specified targets.",
    "canonicalize-flags" => "Canonicalizes a list of bazel options.",
    "clean" => "Removes output files and optionally stops the server.",
    "coverage" => "Generates code coverage report for specified test targets.",
    "cquery" => "Loads, analyzes, and queries the specified targets w/ configurations.",
    "dump" => "Dumps the internal state of the bazel server process.",
    "fetch" => "Fetches external repositories that are prerequisites to the targets.",
    "help" => "Prints help for commands, or the index.",
    "info" => "Displays runtime info about the bazel server.",
    "license" => "Prints the license of this software.",
    "mobile-install" => "Installs targets to mobile devices.",
    "mod" => "Queries the Bzlmod external dependency graph",
    "print_action" => "Prints the command line args for compiling a file.",
    "query" => "Executes a dependency graph query.",
    "run" => "Runs the specified target.",
    "shutdown" => "Stops the bazel server.",
    "sync" => "Syncs all repositories specified in the workspace file",
    "test" => "Builds and runs the specified test targets.",
    "vendor" => "Fetches external repositories into a specific folder specified by the flag --vendor_dir.",
    "version" => "Prints version information for bazel.",
    // bazelrc specific commands. Taken from https://bazel.build/run/bazelrc
    "startup" => "Startup options, which go before the command, and are described in `bazel help startup_options`.",
    "common" => "Options that should be applied to all Bazel commands that support them. If a command does not support an option specified in this way, the option is ignored so long as it is valid for some other Bazel command. Note that this only applies to option names: If the current command accepts an option with the specified name, but doesn't support the specified value, it will fail.",
    "always" => "Options that apply to all Bazel commands. If a command does not support an option specified in this way, it will fail.",
    // Import. Documentation written by myself
    "import" => "Imports the given file. Fails if the file is not found.",
    "try-import" => "Tries to import the given file. Does not fail if the file is not found.",
};

#[derive(Debug)]
pub struct BazelFlags {
    pub commands: Vec<String>,
    pub flags: Vec<FlagInfo>,
    pub flags_by_commands: HashMap<String, Vec<usize>>,
    pub flags_by_name: HashMap<String, usize>,
    pub flags_by_abbreviation: HashMap<String, usize>,
}

impl BazelFlags {
    pub fn from_flags(flags: Vec<FlagInfo>, bazel_version: &str) -> BazelFlags {
        // Index the flags from the protobuf description
        let mut flags_by_commands = HashMap::<String, Vec<usize>>::new();
        let mut flags_by_name = HashMap::<String, usize>::new();
        let mut flags_by_abbreviation = HashMap::<String, usize>::new();
        for (i, f) in flags.iter().enumerate() {
            if !f.bazel_versions.iter().any(|v| v == bazel_version) {
                continue;
            }
            for c in &f.commands {
                let list = flags_by_commands.entry(c.clone()).or_default();
                list.push(i);
            }
            flags_by_name.insert(f.name.clone(), i);
            if let Some(abbreviation) = &f.abbreviation {
                flags_by_abbreviation.insert(abbreviation.clone(), i);
            }
        }

        // The `common` option is the union of all other options
        let mut common_flags = flags_by_commands
            .values()
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        common_flags.sort();
        common_flags.dedup();
        flags_by_commands.insert("common".to_string(), common_flags.clone());

        // For safe usage, the `always` option should be the intersection of all other options.
        // Using an option not supported by all commands would otherwise make some commands
        // unusable. But there are no options which are valid for *all* commands.
        // Hence, I am using the union of all flags.
        flags_by_commands.insert("always".to_string(), common_flags);

        // Determine the list of supported commands
        let mut commands = flags_by_commands.keys().cloned().collect::<Vec<_>>();
        commands.extend(["import".to_string(), "try-import".to_string()]);

        BazelFlags {
            commands,
            flags,
            flags_by_commands,
            flags_by_name,
            flags_by_abbreviation,
        }
    }

    pub fn get_by_invocation(&self, s: &str) -> Option<&FlagInfo> {
        let stripped = s.strip_suffix('=').unwrap_or(s);
        // Long names
        if let Some(long_name) = stripped.strip_prefix("--") {
            if long_name.starts_with('-') {
                return None;
            }
            // Strip the `no` prefix, if any
            let stripped_no = long_name.strip_prefix("no").unwrap_or(long_name);
            return self
                .flags_by_name
                .get(stripped_no)
                .map(|i| self.flags.get(*i).unwrap());
        }
        // Short names
        if let Some(abbreviation) = stripped.strip_prefix('-') {
            if abbreviation.starts_with('-') {
                return None;
            }
            return self
                .flags_by_abbreviation
                .get(abbreviation)
                .map(|i| self.flags.get(*i).unwrap());
        }
        None
    }
}

pub fn load_bazel_flag_collection() -> FlagCollection {
    let bazel_flags_proto: &[u8] =
        include_bytes!(concat!(env!("OUT_DIR"), "/bazel-flags-combined.data.lz4"));
    let decompressed = lz4_flex::decompress_size_prepended(bazel_flags_proto).unwrap();
    FlagCollection::decode(&mut Cursor::new(decompressed)).unwrap()
}

pub fn load_bazel_flags(bazel_version: &str) -> BazelFlags {
    BazelFlags::from_flags(load_bazel_flag_collection().flag_infos, bazel_version)
}

fn escape_markdown(str: &str) -> String {
    let mut res = String::with_capacity(str.len());
    for c in str.chars() {
        match c {
            '\\' => res.push_str("\\\\"),
            '`' => res.push_str("\\`"),
            '*' => res.push_str("\\*"),
            '_' => res.push_str("\\_"),
            '#' => res.push_str("\\#"),
            '+' => res.push_str("\\+"),
            '-' => res.push_str("\\-"),
            '.' => res.push_str("\\."),
            '!' => res.push_str("\\!"),
            '~' => res.push_str("\\~"),
            '{' => res.push_str("\\{"),
            '}' => res.push_str("\\}"),
            '[' => res.push_str("\\["),
            ']' => res.push_str("\\]"),
            '(' => res.push_str("\\("),
            ')' => res.push_str("\\)"),
            '<' => res.push_str("\\<"),
            '>' => res.push_str("\\>"),
            _ => res.push(c),
        }
    }
    res
}

// Combines flags names with their values based on the `requires_value` metadata
pub fn combine_key_value_flags(lines: &mut [crate::parser::Line], bazel_flags: &BazelFlags) {
    use crate::parser::Flag;
    use crate::tokenizer::Spanned;
    for l in lines {
        let mut new_flags = Vec::<Flag>::with_capacity(l.flags.len());
        let mut i: usize = 0;
        while i < l.flags.len() {
            let flag = &l.flags[i];
            new_flags.push(
                || -> Option<Spanned<String>> {
                    let flag_name = &flag.name.as_ref()?.0;
                    let info = bazel_flags.get_by_invocation(flag_name)?;
                    if info.requires_value() {
                        // Combine with the next flag
                        let next_flag = &l.flags.get(i + 1)?;
                        i += 1;
                        if let Some(next_name) = &next_flag.name {
                            if let Some(next_value) = &next_flag.value {
                                let combined_str = next_name.0.clone() + "=" + &next_value.0;
                                let combined_span = crate::tokenizer::Span {
                                    start: next_name.1.start,
                                    end: next_value.1.end,
                                };
                                return Some((combined_str, combined_span));
                            } else {
                                return Some(next_name.clone());
                            }
                        } else if let Some(next_value) = &next_flag.value {
                            return Some(next_value.clone());
                        }
                    }
                    None
                }()
                .map(|new_value| Flag {
                    name: flag.name.clone(),
                    value: Some(new_value),
                })
                .unwrap_or_else(|| flag.clone()),
            );
            i += 1;
        }
        l.flags = new_flags;
    }
}

impl FlagInfo {
    pub fn is_deprecated(&self) -> bool {
        self.metadata_tags.contains(&"DEPRECATED".to_string())
    }

    pub fn supports_command(&self, command: &str) -> bool {
        command == "common" || command == "always" || self.commands.iter().any(|c| c == command)
    }

    pub fn get_documentation_markdown(&self) -> String {
        let mut result = String::new();

        // First line: Flag name and short hand (if any)
        result += format!("`--{}`", self.name).as_str();
        if let Some(abbr) = &self.abbreviation {
            result += format!(" [`-{}`]", abbr).as_str();
        }
        if self.has_negative_flag() {
            result += format!(", `--no{}`", self.name).as_str();
        }
        // Followed by the documentation text
        if let Some(doc) = &self.documentation {
            result += "\n\n";
            result += &escape_markdown(&doc.as_str().replace("%{product}", "Bazel"));
        }
        // And a list of tags
        result += "\n\n";
        if !self.effect_tags.is_empty() {
            result += "Effect tags: ";
            result += self
                .effect_tags
                .iter()
                .map(|t| t.to_lowercase())
                .collect::<Vec<_>>()
                .join(", ")
                .as_str();
            result += "\n";
        }
        if !self.metadata_tags.is_empty() {
            result += "Tags: ";
            result += self
                .metadata_tags
                .iter()
                .map(|t| t.to_lowercase())
                .collect::<Vec<_>>()
                .join(", ")
                .as_str();
            result += "\n";
        }
        if let Some(catgegory) = &self.documentation_category {
            result += format!("Category: {}\n", catgegory.to_lowercase()).as_str();
        }

        result
    }
}

#[test]
fn test_flags() {
    let flags = load_bazel_flags("7.1.0");
    let commands = flags.flags_by_commands.keys().cloned().collect::<Vec<_>>();
    assert!(commands.contains(&"build".to_string()));
    assert!(commands.contains(&"clean".to_string()));
    assert!(commands.contains(&"test".to_string()));
    assert!(commands.contains(&"common".to_string()));

    // Can lookup a flag by its invocation
    let preemptible_info = flags.get_by_invocation("--preemptible");
    assert_eq!(
        preemptible_info
            .unwrap()
            .commands
            .iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>(),
        vec!("startup")
    );

    // Supports both short and long forms
    assert_eq!(
        flags.get_by_invocation("-k"),
        flags.get_by_invocation("--keep_going")
    );

    // The `remote_cache` is valid for at least one command. Hence, it should be in `common`.
    assert!(flags
        .flags_by_commands
        .get("common")
        .unwrap()
        .iter()
        .any(|id| { flags.flags[*id].name == "remote_cache" }));
    assert!(flags
        .flags_by_commands
        .get("always")
        .unwrap()
        .iter()
        .any(|id| flags.flags[*id].name == "remote_cache"));
}

// Test that different flags are available in different Bazel versions
#[test]
fn test_flag_versions() {
    let bazel7_flags = load_bazel_flags("7.0.0");
    let bazel8_flags = load_bazel_flags("8.0.0");
    let bazel9_flags = load_bazel_flags("9.0.0");

    // `python3_path` was removed in Bazel 8
    assert!(bazel7_flags.flags_by_name.contains_key("python3_path"));
    assert!(!bazel8_flags.flags_by_name.contains_key("python3_path"));
    assert!(!bazel9_flags.flags_by_name.contains_key("python3_path"));
}
