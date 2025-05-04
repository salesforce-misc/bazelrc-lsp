use std::fmt::Write as _;
use std::{ops::Deref, path::Path};

use chumsky::error::Rich;
use regex::Regex;
use ropey::Rope;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, DiagnosticTag};

use crate::tokenizer::Span;
use crate::{
    bazel_flags::{combine_key_value_flags, BazelFlags, FlagLookupType},
    file_utils::resolve_bazelrc_path,
    lsp_utils::{encode_lsp_range, LspPositionEncoding},
    parser::{parse_from_str, Line, ParserResult},
};

pub fn diagnostics_from_parser<'a>(
    rope: &'a Rope,
    errors: &'a [Rich<'a, char>],
    encoding: LspPositionEncoding,
) -> impl Iterator<Item = Diagnostic> + 'a {
    errors.iter().filter_map(move |item| {
        let (message, err_span) = match item.reason() {
            chumsky::error::RichReason::ExpectedFound { expected, found } => {
                let mut s = String::new();
                if let Some(found) = found {
                    write!(s, "Found {}", found.deref()).unwrap();
                } else {
                    write!(&mut s, "Unexpected end of input").unwrap();
                }
                write!(&mut s, ", expected ").unwrap();
                match &expected[..] {
                    [] => {
                        write!(s, "something else").unwrap();
                    }
                    [expected] => {
                        write!(s, "{}", expected).unwrap();
                    }
                    _ => {
                        for expected in &expected[..expected.len() - 1] {
                            write!(s, "{}", expected).unwrap();
                            write!(s, ", ").unwrap();
                        }
                        write!(s, "or ").unwrap();
                        write!(s, "{}", expected.last().unwrap()).unwrap();
                    }
                }
                (s, item.span())
            }
            chumsky::error::RichReason::Custom(msg) => (msg.to_string(), item.span()),
        };

        let span = &Span {
            start: err_span.start,
            end: err_span.end,
        };
        || -> Option<Diagnostic> {
            Some(Diagnostic::new_simple(
                encode_lsp_range(rope, span, encoding)?,
                message,
            ))
        }()
    })
}

const SKIPPED_PREFIXES: [&str; 4] = ["--//", "--no//", "--@", "--no@"];

fn diagnostics_for_flags(
    rope: &Rope,
    line: &Line,
    bazel_flags: &BazelFlags,
    encoding: LspPositionEncoding,
) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = Vec::<Diagnostic>::new();
    let command = &line.command.as_ref().unwrap().0;
    for flag in &line.flags {
        if let Some(name) = &flag.name {
            if SKIPPED_PREFIXES
                .iter()
                .any(|prefix| name.0.starts_with(prefix))
            {
                // Don't diagnose custom settings at all
            } else if let Some((lookup_type, flag_description)) =
                bazel_flags.get_by_invocation(&name.0)
            {
                // Diagnose flags used on the wrong command
                if !flag_description.supports_command(command) {
                    diagnostics.push(Diagnostic::new_simple(
                        encode_lsp_range(rope, &name.1, encoding).unwrap(),
                        format!("The flag {:?} is not supported for {:?}. It is supported for {:?} commands, though.", name.0, command, flag_description.commands),
                    ))
                }
                // Diagnose deprecated options
                if flag_description.is_deprecated() {
                    diagnostics.push(Diagnostic {
                        range: encode_lsp_range(rope, &name.1, encoding).unwrap(),
                        message: format!("The flag {:?} is deprecated.", name.0),
                        severity: Some(DiagnosticSeverity::WARNING),
                        tags: Some(vec![DiagnosticTag::DEPRECATED]),
                        ..Default::default()
                    });
                } else if flag_description.is_noop() {
                    diagnostics.push(Diagnostic {
                        range: encode_lsp_range(rope, &name.1, encoding).unwrap(),
                        message: format!("The flag {:?} is a no-op.", name.0),
                        severity: Some(DiagnosticSeverity::WARNING),
                        ..Default::default()
                    });
                } else if lookup_type == FlagLookupType::OldName {
                    diagnostics.push(Diagnostic {
                        range: encode_lsp_range(rope, &name.1, encoding).unwrap(),
                        message: format!(
                            "The flag {:?} was renamed to \"--{}\".",
                            name.0, flag_description.name
                        ),
                        tags: Some(vec![DiagnosticTag::DEPRECATED]),
                        severity: Some(DiagnosticSeverity::WARNING),
                        ..Default::default()
                    });
                } else if lookup_type == FlagLookupType::Abbreviation {
                    diagnostics.push(Diagnostic {
                        range: encode_lsp_range(rope, &name.1, encoding).unwrap(),
                        message: format!(
                            "Use the full name {:?} instead of its abbreviation.",
                            flag_description.name
                        ),
                        severity: Some(DiagnosticSeverity::WARNING),
                        ..Default::default()
                    });
                }
            } else {
                // Diagnose unknown flags
                diagnostics.push(Diagnostic::new_simple(
                    encode_lsp_range(rope, &name.1, encoding).unwrap(),
                    format!("Unknown flag {:?}", name.0),
                ))
            }
        }
    }
    diagnostics
}

fn diagnostics_for_imports(
    rope: &Rope,
    line: &Line,
    base_path: Option<&Path>,
    encoding: LspPositionEncoding,
) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = Vec::<Diagnostic>::new();
    let command = line.command.as_ref().unwrap();
    if line.flags.is_empty() {
        diagnostics.push(Diagnostic::new_simple(
            encode_lsp_range(rope, &command.1, encoding).unwrap(),
            "Missing file path".to_string(),
        ))
    } else if line.flags.len() > 1 {
        diagnostics.push(Diagnostic::new_simple(
            encode_lsp_range(rope, &command.1, encoding).unwrap(),
            format!(
                "`{}` expects a single file name, but received multiple arguments",
                command.0
            ),
        ))
    } else {
        let flag = &line.flags[0];
        if flag.name.is_some() {
            diagnostics.push(Diagnostic::new_simple(
                encode_lsp_range(rope, &command.1, encoding).unwrap(),
                format!("`{}` expects a file name, not a flag name", command.0),
            ))
        }
        if let Some(act_base_path) = base_path {
            if let Some(value) = flag.value.as_ref() {
                let severity = if command.0 == "try-import" {
                    DiagnosticSeverity::WARNING
                } else {
                    DiagnosticSeverity::ERROR
                };
                let opt_path = resolve_bazelrc_path(act_base_path, &value.0);
                if let Some(path) = opt_path {
                    if !path.exists() {
                        diagnostics.push(Diagnostic {
                            range: encode_lsp_range(rope, &value.1, encoding).unwrap(),
                            message: "Imported file does not exist".to_string(),
                            severity: Some(severity),
                            ..Default::default()
                        })
                    } else if !path.is_file() {
                        diagnostics.push(Diagnostic {
                            range: encode_lsp_range(rope, &value.1, encoding).unwrap(),
                            message: "Imported path exists, but is not a file".to_string(),
                            severity: Some(severity),
                            ..Default::default()
                        })
                    }
                } else {
                    diagnostics.push(Diagnostic {
                        range: encode_lsp_range(rope, &value.1, encoding).unwrap(),
                        message: "Unable to resolve file name".to_string(),
                        severity: Some(severity),
                        ..Default::default()
                    })
                }
            }
        }
    }
    diagnostics
}

pub fn diagnostics_from_rcconfig(
    rope: &Rope,
    lines: &[Line],
    bazel_flags: &BazelFlags,
    file_path: Option<&Path>,
    encoding: LspPositionEncoding,
) -> Vec<Diagnostic> {
    let config_regex = Regex::new(r"^[a-z_][a-z0-9]*(?:[-_][a-z0-9]+)*$").unwrap();
    let mut diagnostics: Vec<Diagnostic> = Vec::<Diagnostic>::new();

    for l in lines {
        // Command-specific diagnostics
        if let Some((command, span)) = &l.command {
            if command == "import" || command == "try-import" {
                diagnostics.extend(diagnostics_for_imports(rope, l, file_path, encoding))
            } else if bazel_flags.flags_by_commands.contains_key(command) {
                diagnostics.extend(diagnostics_for_flags(rope, l, bazel_flags, encoding))
            } else {
                diagnostics.push(Diagnostic::new_simple(
                    encode_lsp_range(rope, span, encoding).unwrap(),
                    format!("Unknown command {:?}", command),
                ));
            }
        } else if !l.flags.is_empty() {
            diagnostics.push(Diagnostic::new_simple(
                encode_lsp_range(rope, &l.span, encoding).unwrap(),
                "Missing command".to_string(),
            ));
        }

        // Diagnostics for config names
        if let Some((config_name, span)) = &l.config {
            if config_name.is_empty() {
                // Empty config names make no sense
                diagnostics.push(Diagnostic::new_simple(
                    encode_lsp_range(rope, span, encoding).unwrap(),
                    "Empty configuration names are pointless".to_string(),
                ));
            } else if !config_regex.is_match(config_name) {
                // Overly complex config names
                diagnostics.push(Diagnostic::new_simple(
                    encode_lsp_range(rope, span, encoding).unwrap(),
                    "Overly complicated config name. Config names should consist only of lower-case ASCII characters.".to_string(),
                ));
            }
            if let Some((command, _)) = &l.command {
                if ["startup", "import", "try-import"].contains(&command.as_str()) {
                    diagnostics.push(Diagnostic::new_simple(
                        encode_lsp_range(rope, span, encoding).unwrap(),
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

pub fn diagnostics_from_string(
    str: &str,
    bazel_flags: &BazelFlags,
    file_path: Option<&Path>,
    encoding: LspPositionEncoding,
) -> Vec<Diagnostic> {
    let rope = Rope::from_str(str);
    let ParserResult {
        tokens: _,
        mut lines,
        errors,
    } = parse_from_str(str);
    combine_key_value_flags(&mut lines, bazel_flags);

    let mut diagnostics: Vec<Diagnostic> = Vec::<Diagnostic>::new();
    diagnostics.extend(diagnostics_from_parser(&rope, &errors, encoding));
    diagnostics.extend(diagnostics_from_rcconfig(
        &rope,
        &lines,
        bazel_flags,
        file_path,
        encoding,
    ));
    diagnostics
}

#[cfg(test)]
fn test_diagnose_string(str: &str) -> Vec<String> {
    use crate::bazel_flags::load_packaged_bazel_flags;

    let bazel_flags = load_packaged_bazel_flags("8.0.0");
    return diagnostics_from_string(str, &bazel_flags, None, LspPositionEncoding::UTF32)
        .iter_mut()
        .map(|d| std::mem::take(&mut d.message))
        .collect::<Vec<_>>();
}

#[test]
fn test_diagnose_commands() {
    // Nothing wrong with this `build` command
    assert_eq!(
        test_diagnose_string("build --remote_upload_local_results=false"),
        Vec::<&str>::new()
    );
    // The command should be named `build`, not `built`
    assert_eq!(
        test_diagnose_string("built --remote_upload_local_results=false"),
        vec!["Unknown command \"built\""]
    );
    // Completely missing command
    assert_eq!(
        test_diagnose_string("--remote_upload_local_results=false"),
        vec!["Missing command"]
    );
    // Completely missing command
    assert_eq!(
        test_diagnose_string(":opt --remote_upload_local_results=false"),
        vec!["Missing command"]
    );
}

#[test]
fn test_diagnose_config_names() {
    // Diagnose empty config names
    assert_eq!(
        test_diagnose_string("build: --disk_cache="),
        vec!["Empty configuration names are pointless"]
    );

    // Diagnose config names on commands which don't support configs
    assert_eq!(
        test_diagnose_string("startup:opt --digest_function=blake3"),
        vec!["Configuration names not supported on \"startup\" commands"]
    );
    assert_eq!(
        test_diagnose_string("import:opt \"x.bazelrc\""),
        vec!["Configuration names not supported on \"import\" commands"]
    );
    assert_eq!(
        test_diagnose_string("try-import:opt \"x.bazelrc\""),
        vec!["Configuration names not supported on \"try-import\" commands"]
    );

    // Diagnose overly complicated config names
    let config_name_diag = "Overly complicated config name. Config names should consist only of lower-case ASCII characters.";
    assert_eq!(
        test_diagnose_string("common:Uncached --disk_cache="),
        vec![config_name_diag]
    );
    assert_eq!(
        test_diagnose_string("common:-opt --disk_cache="),
        vec![config_name_diag]
    );
    assert_eq!(
        test_diagnose_string("common:opt- --disk_cache="),
        vec![config_name_diag]
    );
    assert_eq!(
        test_diagnose_string("common:2opt --disk_cache="),
        vec![config_name_diag]
    );
    assert_eq!(
        test_diagnose_string("common:opt2 --disk_cache="),
        Vec::<String>::new()
    );
    assert_eq!(
        test_diagnose_string("common:opt-2 --disk_cache="),
        Vec::<String>::new()
    );
    assert_eq!(
        test_diagnose_string("common:opt--2 --disk_cache="),
        vec![config_name_diag]
    );
    // The Bazel documentation recommends to prefix all user-specific settings with an `_`.
    // As such, config names prefixed that way shouldn't be diagnosed as errors.
    assert_eq!(
        test_diagnose_string("common:_personal --disk_cache="),
        Vec::<String>::new()
    );
}

#[test]
fn test_diagnose_flags() {
    // Diagnose unknown flags
    assert_eq!(
        test_diagnose_string("build --unknown_flag"),
        vec!["Unknown flag \"--unknown_flag\""]
    );
    // Diagnose flags which are applied for the wrong command
    assert_eq!(
        test_diagnose_string("startup --disk_cache="),
        vec!["The flag \"--disk_cache\" is not supported for \"startup\". It is supported for [\"analyze-profile\", \"aquery\", \"build\", \"canonicalize-flags\", \"clean\", \"config\", \"coverage\", \"cquery\", \"dump\", \"fetch\", \"help\", \"info\", \"license\", \"mobile-install\", \"mod\", \"print_action\", \"query\", \"run\", \"shutdown\", \"sync\", \"test\", \"vendor\", \"version\"] commands, though."]
    );
    // Diagnose deprecated flags
    assert_eq!(
        test_diagnose_string("common --legacy_whole_archive"),
        vec!["The flag \"--legacy_whole_archive\" is deprecated."]
    );
    // Diagnose no_op flags
    assert_eq!(
        test_diagnose_string("common --incompatible_override_toolchain_transition"),
        vec!["The flag \"--incompatible_override_toolchain_transition\" is a no-op."]
    );
    // Diagnose abbreviated flag names
    assert_eq!(
        test_diagnose_string("build -k"),
        vec!["Use the full name \"keep_going\" instead of its abbreviation."]
    );

    // Don't diagnose custom flags
    assert_eq!(
        test_diagnose_string(
            "build --//my/package:setting=foobar
            build --no//my/package:bool_flag
            build --@dependency:my/package:bool_flag
            build --no@dependency:my/package:bool_flag"
        ),
        Vec::<String>::new()
    );
}

#[test]
fn test_diagnose_combined_flags() {
    // The `--copt` flag expects an argument and hence consumes the
    // following `--std=c++20`. `--std=c++20` should not raise
    // an error about an unrecognized Bazel flag.
    assert_eq!(
        test_diagnose_string("build --copt --std=c++20"),
        Vec::<&str>::new()
    );
    // On the other hand, `--keep_going` only takes an optional value.
    // Hence, the `true` is interpreted as a separate flag, which then triggers
    // an error.
    assert_eq!(
        test_diagnose_string("build --keep_going --foobar"),
        vec!["Unknown flag \"--foobar\""]
    );
}

#[test]
fn test_diagnose_import() {
    assert_eq!(test_diagnose_string("import"), vec!["Missing file path"]);
    assert_eq!(
        test_diagnose_string("try-import"),
        vec!["Missing file path"]
    );
    assert_eq!(
        test_diagnose_string("import --a"),
        vec!["`import` expects a file name, not a flag name"]
    );
    assert_eq!(
        test_diagnose_string("import a b"),
        vec!["`import` expects a single file name, but received multiple arguments"]
    );
}
