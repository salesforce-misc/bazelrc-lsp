use chumsky::error::Simple;
use regex::Regex;
use ropey::Rope;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticTag};

use crate::{bazel_flags::BazelFlags, lsp_utils::range_to_lsp, parser::Line};

pub fn diagnostics_from_parser<'a>(
    rope: &'a Rope,
    errors: &'a [Simple<char>],
) -> impl Iterator<Item = Diagnostic> + 'a {
    errors.iter().filter_map(move |item| {
        let (message, span) = match item.reason() {
            chumsky::error::SimpleReason::Unclosed { span, delimiter } => {
                (format!("Unclosed delimiter {}", delimiter), span.clone())
            }
            chumsky::error::SimpleReason::Unexpected => (
                format!(
                    "{}, expected {}",
                    if item.found().is_some() {
                        "Unexpected token in input"
                    } else {
                        "Unexpected end of input"
                    },
                    if item.expected().len() == 0 {
                        "something else".to_string()
                    } else {
                        item.expected()
                            .map(|expected| match expected {
                                Some(expected) => expected.to_string(),
                                None => "end of input".to_string(),
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                ),
                item.span(),
            ),
            chumsky::error::SimpleReason::Custom(msg) => (msg.to_string(), item.span()),
        };

        || -> Option<Diagnostic> {
            Some(Diagnostic::new_simple(range_to_lsp(rope, &span)?, message))
        }()
    })
}

fn diagnostics_for_flags(rope: &Rope, line: &Line, bazel_flags: &BazelFlags) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = Vec::<Diagnostic>::new();
    let command = &line.command.as_ref().unwrap().0;
    for flag in &line.flags {
        if let Some(name) = &flag.name {
            if let Some(flag_description) = bazel_flags.get_by_invocation(&name.0) {
                // Diagnose flags used on the wrong command
                if command != "common"
                    && command != "always"
                    && !flag_description.commands.contains(command)
                {
                    diagnostics.push(Diagnostic::new_simple(
                        range_to_lsp(rope, &name.1).unwrap(),
                        format!("The flag {:?} is not supported for {:?}. It is supported for {:?} commands, though.", name.0, command, flag_description.commands),
                    ))
                }
                // Diagnose deprecated options
                if flag_description.metadata_tags.contains(&"DEPRECATED".to_string()) {
                    diagnostics.push(Diagnostic {
                        range: range_to_lsp(rope, &name.1).unwrap(),
                        message: format!("The flag {:?} is deprecated.", name.0),
                        tags: Some(vec![DiagnosticTag::DEPRECATED]),
                        ..Default::default()
                    });
                }
            } else {
                // Diagnose unknown flags
                diagnostics.push(Diagnostic::new_simple(
                    range_to_lsp(rope, &name.1).unwrap(),
                    format!("Unknown flag {:?}", name.0),
                ))
            }
        }
    }
    diagnostics
}

pub fn diagnostics_from_rcconfig(
    rope: &Rope,
    lines: &[Line],
    bazel_flags: &BazelFlags,
) -> Vec<Diagnostic> {
    let config_regex = Regex::new(r"^[a-z_][a-z0-9]*(?:[-_][a-z0-9]+)*$").unwrap();
    let mut diagnostics: Vec<Diagnostic> = Vec::<Diagnostic>::new();

    for l in lines {
        // Command-specific diagnostics
        if let Some((command, span)) = &l.command {
            if command == "import" || command == "try-import" {
                // TODO check that the imported file exists.
            } else if bazel_flags.flags_by_commands.get(command).is_some() {
                diagnostics.extend(diagnostics_for_flags(rope, l, bazel_flags))
            } else {
                diagnostics.push(Diagnostic::new_simple(
                    range_to_lsp(rope, span).unwrap(),
                    format!("Unknown command {:?}", command),
                ));
            }
        } else if !l.flags.is_empty() {
            diagnostics.push(Diagnostic::new_simple(
                range_to_lsp(rope, &l.span).unwrap(),
                "Missing command".to_string(),
            ));
        }

        // Diagnostics for config names
        if let Some((config_name, span)) = &l.config {
            if config_name.is_empty() {
                // Empty config names make no sense
                diagnostics.push(Diagnostic::new_simple(
                    range_to_lsp(rope, span).unwrap(),
                    "Empty configuration names are pointless".to_string(),
                ));
            } else if !config_regex.is_match(config_name) {
                // Overly complex config names
                diagnostics.push(Diagnostic::new_simple(
                    range_to_lsp(rope, span).unwrap(),
                    "Overly complicated config name. Config names should consist only of lower-case ASCII characters.".to_string(),
                ));
            }
            if let Some((command, span)) = &l.command {
                if ["startup", "import", "try-import"].contains(&command.as_str()) {
                    diagnostics.push(Diagnostic::new_simple(
                        range_to_lsp(rope, span).unwrap(),
                        format!(
                            "Configuration names not supported on {:?} commands",
                            command
                        ),
                    ));
                }
            }
        }
    }
    diagnostics
}

#[cfg(test)]
fn diagnose_string(str: &str) -> Vec<String> {
    use crate::bazel_flags::load_bazel_flags;
    use crate::parser::parse_from_str;
    use crate::parser::ParserResult;

    let rope = Rope::from_str(str);
    let ParserResult {
        tokens: _,
        lines,
        errors,
    } = parse_from_str(str);
    assert!(errors.is_empty());

    let bazel_flags = load_bazel_flags();
    return diagnostics_from_rcconfig(&rope, &lines, &bazel_flags)
        .iter_mut()
        .map(|d| std::mem::take(&mut d.message))
        .collect::<Vec<_>>();
}

#[test]
fn test_diagnose_commands() {
    // Nothing wrong with this `build` command
    assert_eq!(
        diagnose_string("build --remote_upload_local_results=false"),
        Vec::<&str>::new()
    );
    // The command should be named `build`, not `built`
    assert_eq!(
        diagnose_string("built --remote_upload_local_results=false"),
        vec!["Unknown command \"built\""]
    );
    // Completely missing command
    assert_eq!(
        diagnose_string("--remote_upload_local_results=false"),
        vec!["Missing command"]
    );
    // Completely missing command
    assert_eq!(
        diagnose_string(":opt --remote_upload_local_results=false"),
        vec!["Missing command"]
    );
}

#[test]
fn test_diagnose_config_names() {
    // Diagnose empty config names
    assert_eq!(
        diagnose_string("build: --disk_cache="),
        vec!["Empty configuration names are pointless"]
    );

    // Diagnose config names on commands which don't support configs
    assert_eq!(
        diagnose_string("startup:opt --digest_function=blake3"),
        vec!["Configuration names not supported on \"startup\" commands"]
    );
    assert_eq!(
        diagnose_string("import:opt \"x.bazelrc\""),
        vec!["Configuration names not supported on \"import\" commands"]
    );
    assert_eq!(
        diagnose_string("try-import:opt \"x.bazelrc\""),
        vec!["Configuration names not supported on \"try-import\" commands"]
    );

    // Diagnose overly complicated config names
    let config_name_diag = "Overly complicated config name. Config names should consist only of lower-case ASCII characters.";
    assert_eq!(
        diagnose_string("common:Uncached --disk_cache="),
        vec![config_name_diag]
    );
    assert_eq!(
        diagnose_string("common:-opt --disk_cache="),
        vec![config_name_diag]
    );
    assert_eq!(
        diagnose_string("common:opt- --disk_cache="),
        vec![config_name_diag]
    );
    assert_eq!(
        diagnose_string("common:2opt --disk_cache="),
        vec![config_name_diag]
    );
    assert_eq!(
        diagnose_string("common:opt2 --disk_cache="),
        Vec::<String>::new()
    );
    assert_eq!(
        diagnose_string("common:opt-2 --disk_cache="),
        Vec::<String>::new()
    );
    assert_eq!(
        diagnose_string("common:opt--2 --disk_cache="),
        vec![config_name_diag]
    );
    // The Bazel documentation recommends to prefix all user-specific settings with an `_`.
    // As such, config names prefixed that way shouldn't be diagnosed as errors.
    assert_eq!(
        diagnose_string("common:_personal --disk_cache="),
        Vec::<String>::new()
    );
}

#[test]
fn test_diagnose_flags() {
    // Diagnose unknown flags
    assert_eq!(
        diagnose_string("build --unknown_flag"),
        vec!["Unknown flag \"--unknown_flag\""]
    );
    // Diagnose flags which are applied for the wrong command
    assert_eq!(
        diagnose_string("startup --disk_cache="),
        vec!["The flag \"--disk_cache\" is not supported for \"startup\". It is supported for [\"analyze-profile\", \"aquery\", \"build\", \"canonicalize-flags\", \"clean\", \"config\", \"coverage\", \"cquery\", \"dump\", \"fetch\", \"help\", \"info\", \"license\", \"mobile-install\", \"mod\", \"print_action\", \"query\", \"run\", \"shutdown\", \"sync\", \"test\", \"vendor\", \"version\"] commands, though."]
    );
    // Diagnose deprecated flags
    assert_eq!(
        diagnose_string("common --expand_configs_in_place"),
        vec!["The flag \"--expand_configs_in_place\" is deprecated."]
    );
}
