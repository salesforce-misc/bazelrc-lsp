use chumsky::error::Simple;
use ropey::Rope;
use tower_lsp::lsp_types::Diagnostic;

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

pub fn diagnostics_for_flags(
    rope: &Rope,
    line: &Line,
    bazel_flags: &BazelFlags,
) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = Vec::<Diagnostic>::new();
    for flag in &line.flags {
        if let Some(name) = &flag.name {
            if let Some(flag_description) = bazel_flags.get_by_invocation(&name.0) {
                // TODO: Check if valid for this command
            } else {
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
    let mut diagnostics: Vec<Diagnostic> = Vec::<Diagnostic>::new();

    for l in lines {
        if let Some((command, span)) = &l.command {
            if command == "import" || command == "try-import" {
                // TODO check that the imported file exists.
            } else {
                if let Some(_) = bazel_flags.flags_by_commands.get(command) {
                    diagnostics.extend(diagnostics_for_flags(rope, l,  bazel_flags))
                } else {
                    diagnostics.push(Diagnostic::new_simple(
                        range_to_lsp(rope, span).unwrap(),
                        format!("Unknown command {:?}", command),
                    ));
                }
            }
        } else if !l.flags.is_empty() {
            diagnostics.push(Diagnostic::new_simple(
                range_to_lsp(rope, &l.span).unwrap(),
                "Missing command".to_string(),
            ));
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
    assert_eq!(diagnose_string("build --remote_upload_local_results=false"), Vec::<&str>::new());
    // The command should be named `build`, not `built`
    assert_eq!(diagnose_string("built --remote_upload_local_results=false"), vec!["Unknown command \"built\""]);
    // Completely missing command
    assert_eq!(diagnose_string("--remote_upload_local_results=false"), vec!["Missing command"]);
}

#[test]
fn test_diagnose_flags() {
    // Diagnose unknown flags
    assert_eq!(diagnose_string("build --my_flag"), vec!["Unknown flag \"--my_flag\""]);
}
