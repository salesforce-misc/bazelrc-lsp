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

pub fn diagnostics_from_rcconfig(
    rope: &Rope,
    lines: &[Line],
    bazel_flags: &BazelFlags,
) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = Vec::<Diagnostic>::new();

    for l in lines {
        if let Some((command, span)) = &l.command {
            let flags_for_command = bazel_flags.flags_by_commands.get(command);
            if flags_for_command.is_none() {
                diagnostics.push(Diagnostic::new_simple(
                    range_to_lsp(rope, span).unwrap(),
                    format!("unknown command `{:?}`", command),
                ));
            }
        } else if !l.flags.is_empty() {
            diagnostics.push(Diagnostic::new_simple(
                range_to_lsp(rope, &l.span).unwrap(),
                "missing command".to_string(),
            ));
        }
    }
    diagnostics
}
