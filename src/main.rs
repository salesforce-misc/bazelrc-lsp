use std::env;

use bazelrc_lsp::bazel_flags::{
    combine_key_value_flags, load_bazel_flags_from_command, load_packaged_bazel_flags, BazelFlags,
    COMMAND_DOCS,
};
use bazelrc_lsp::bazel_version::{
    auto_detect_bazel_version, find_closest_version, AVAILABLE_BAZEL_VERSIONS,
};
use bazelrc_lsp::completion::get_completion_items;
use bazelrc_lsp::definition::get_definitions;
use bazelrc_lsp::diagnostic::{diagnostics_from_parser, diagnostics_from_rcconfig};
use bazelrc_lsp::file_utils::resolve_bazelrc_path;
use bazelrc_lsp::formatting::get_text_edits_for_lines;
use bazelrc_lsp::line_index::{IndexEntry, IndexEntryKind, IndexedLines};
use bazelrc_lsp::lsp_utils::{lsp_pos_to_offset, range_to_lsp};
use bazelrc_lsp::parser::{parse_from_str, Line, ParserResult};
use bazelrc_lsp::semantic_token::{
    convert_to_lsp_tokens, semantic_tokens_from_lines, RCSemanticToken, LEGEND_TYPE,
};
use dashmap::DashMap;
use ropey::Rope;
use tower_lsp::jsonrpc::{Error, Result};
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

struct TextDocumentItem {
    uri: Url,
    text: String,
    version: i32,
}

#[derive(Debug)]
struct AnalyzedDocument {
    rope: Rope,
    semantic_tokens: Vec<RCSemanticToken>,
    indexed_lines: IndexedLines,
    parser_errors: Vec<chumsky::prelude::Simple<char>>,
}

#[derive(Debug)]
struct Backend {
    client: Client,
    document_map: DashMap<String, AnalyzedDocument>,
    bazel_flags: BazelFlags,
    // An optional message which should be displayed to the user on startup
    startup_warning: Option<String>,
}

impl Backend {
    async fn on_change(&self, params: TextDocumentItem) {
        let rope = ropey::Rope::from_str(&params.text);
        let src = rope.to_string();

        let file_path_buf = params.uri.to_file_path().ok();
        let file_path = file_path_buf.as_deref();

        let ParserResult {
            tokens: _,
            mut lines,
            errors,
        } = parse_from_str(&src);
        combine_key_value_flags(&mut lines, &self.bazel_flags);
        let semantic_tokens = semantic_tokens_from_lines(&lines);
        let indexed_lines = IndexedLines::from_lines(lines);

        let mut diagnostics: Vec<Diagnostic> = Vec::<Diagnostic>::new();
        diagnostics.extend(diagnostics_from_parser(&rope, &errors));
        diagnostics.extend(diagnostics_from_rcconfig(
            &rope,
            &indexed_lines.lines,
            &self.bazel_flags,
            file_path,
        ));

        self.document_map.insert(
            params.uri.to_string(),
            AnalyzedDocument {
                rope,
                parser_errors: errors,
                semantic_tokens,
                indexed_lines,
            },
        );

        self.client
            .publish_diagnostics(params.uri.clone(), diagnostics, Some(params.version))
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "bazelrc Language Server".to_string(),
                version: Some("1".to_string()),
            }),
            offset_encoding: None,
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensRegistrationOptions(
                        SemanticTokensRegistrationOptions {
                            text_document_registration_options: {
                                TextDocumentRegistrationOptions {
                                    document_selector: Some(vec![DocumentFilter {
                                        language: Some("bazelrc".to_string()),
                                        scheme: None,
                                        pattern: None,
                                    }]),
                                }
                            },
                            semantic_tokens_options: SemanticTokensOptions {
                                work_done_progress_options: WorkDoneProgressOptions::default(),
                                legend: SemanticTokensLegend {
                                    token_types: LEGEND_TYPE.into(),
                                    token_modifiers: vec![],
                                },
                                range: None,
                                full: Some(SemanticTokensFullOptions::Bool(true)),
                            },
                            static_registration_options: StaticRegistrationOptions::default(),
                        },
                    ),
                ),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec!["-".to_string()]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                document_range_formatting_provider: Some(OneOf::Left(true)),
                document_link_provider: Some(DocumentLinkOptions {
                    resolve_provider: None,
                    work_done_progress_options: Default::default(),
                }),
                definition_provider: Some(OneOf::Left(true)),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "server initialized!")
            .await;

        if let Some(warning) = &self.startup_warning {
            self.client
                .show_message(MessageType::WARNING, warning)
                .await;
        }
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.on_change(TextDocumentItem {
            uri: params.text_document.uri,
            text: params.text_document.text,
            version: params.text_document.version,
        })
        .await
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        self.on_change(TextDocumentItem {
            uri: params.text_document.uri,
            text: std::mem::take(&mut params.content_changes[0].text),
            version: params.text_document.version,
        })
        .await
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.document_map
            .remove(&params.text_document.uri.to_string());
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri.to_string();
        let doc = self
            .document_map
            .get(&uri)
            .ok_or(Error::invalid_params("Unknown document!"))?;
        let lsp_tokens = convert_to_lsp_tokens(&doc.rope, &doc.semantic_tokens);
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: lsp_tokens,
        })))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let text_document_position = params.text_document_position;
        let uri = text_document_position.text_document.uri.to_string();
        let doc = self
            .document_map
            .get(&uri)
            .ok_or(Error::invalid_params("Unknown document!"))?;
        let pos = lsp_pos_to_offset(&doc.rope, &text_document_position.position)
            .ok_or(Error::invalid_params("Position out of range"))?;

        Ok(Some(CompletionResponse::Array(get_completion_items(
            &self.bazel_flags,
            &doc.rope,
            &doc.indexed_lines,
            pos,
        ))))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let file_path = uri
            .to_file_path()
            .ok()
            .ok_or(Error::invalid_params("Unsupported URI scheme!"))?;
        let doc = self
            .document_map
            .get(&uri.to_string())
            .ok_or(Error::invalid_params("Unknown document!"))?;
        let pos = lsp_pos_to_offset(&doc.rope, &params.text_document_position_params.position)
            .ok_or(Error::invalid_params("Position out of range"))?;
        let IndexEntry { kind, line_nr, .. } =
            doc.indexed_lines.find_symbol_at_position(pos).unwrap();
        let definitions = get_definitions(&file_path, kind, &doc.indexed_lines.lines[*line_nr]);
        Ok(definitions)
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        // Find the right document and offset
        let text_document_position = params.text_document_position_params;
        let uri = text_document_position.text_document.uri.to_string();
        let doc = self
            .document_map
            .get(&uri)
            .ok_or(Error::invalid_params("Unknown document!"))?;
        let pos = lsp_pos_to_offset(&doc.rope, &text_document_position.position)
            .ok_or(Error::invalid_params("Position out of range"))?;

        Ok(|| -> Option<Hover> {
            // Find the symbol at the position and provide the hover documentation
            let IndexEntry {
                span,
                line_nr,
                kind,
            } = doc.indexed_lines.find_symbol_at_position(pos)?;
            match kind {
                IndexEntryKind::Command => {
                    let line = &doc.indexed_lines.lines[*line_nr];

                    line.command
                        .as_ref()
                        .and_then(|cmd| COMMAND_DOCS.get(cmd.0.as_str()))
                        .map(|docs| {
                            let contents =
                                HoverContents::Scalar(MarkedString::String(docs.to_string()));
                            Hover {
                                contents,
                                range: range_to_lsp(&doc.rope, span),
                            }
                        })
                }
                IndexEntryKind::Config => None,
                IndexEntryKind::FlagValue(flag_nr) | IndexEntryKind::FlagName(flag_nr) => {
                    let line = &doc.indexed_lines.lines[*line_nr];
                    let flag_name = &line.flags.get(*flag_nr)?.name.as_ref()?.0;
                    let flag_info = self.bazel_flags.get_by_invocation(flag_name)?;
                    let content = flag_info.get_documentation_markdown();
                    let contents = HoverContents::Scalar(MarkedString::String(content));
                    Some(Hover {
                        contents,
                        range: range_to_lsp(&doc.rope, span),
                    })
                }
            }
        }())
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        // Find the right document
        let uri = params.text_document.uri.to_string();
        let doc = self
            .document_map
            .get(&uri)
            .ok_or(Error::invalid_params("Unknown document!"))?;
        let rope = &doc.rope;

        if !doc.parser_errors.is_empty() {
            return Err(Error::invalid_params(
                "Formatting can only be applied if there are no parsing errors",
            ));
        }

        // Format all lines
        let lines = &doc.indexed_lines.lines;
        Ok(Some(get_text_edits_for_lines(lines, rope)))
    }

    async fn range_formatting(
        &self,
        params: DocumentRangeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        // Find the right document
        let uri = params.text_document.uri.to_string();
        let doc = self
            .document_map
            .get(&uri)
            .ok_or(Error::invalid_params("Unknown document!"))?;
        let rope = &doc.rope;

        if !doc.parser_errors.is_empty() {
            return Err(Error::invalid_params(
                "Formatting can only be applied if there are no parsing errors",
            ));
        }

        // Format the line range
        let all_lines = &doc.indexed_lines.lines;
        let start_offset = lsp_pos_to_offset(rope, &params.range.start)
            .ok_or(Error::invalid_params("Position out of range!"))?;
        let end_offset = lsp_pos_to_offset(rope, &params.range.end)
            .ok_or(Error::invalid_params("Position out of range!"))?;
        // XXX not correct, yet
        let first_idx = all_lines.partition_point(|l: &Line| l.span.start < start_offset);
        let last_idx = all_lines.partition_point(|l: &Line| l.span.end < end_offset) + 1;

        Ok(Some(get_text_edits_for_lines(
            &all_lines[first_idx..last_idx],
            rope,
        )))
    }

    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        // Find the right document
        let uri = params.text_document.uri.to_string();
        let doc = self
            .document_map
            .get(&uri)
            .ok_or(Error::invalid_params("Unknown document!"))?;
        let rope = &doc.rope;
        let file_path = params
            .text_document
            .uri
            .to_file_path()
            .ok()
            .ok_or(Error::invalid_params("Unsupported URI scheme!"))?;

        // Link all `import` and `try-import` lines
        let links = doc
            .indexed_lines
            .lines
            .iter()
            .filter_map(|line| {
                let command = line.command.as_ref()?;
                if command.0 != "import" && command.0 != "try-import" {
                    return None;
                }
                if line.flags.len() != 1 {
                    return None;
                }
                let flag = &line.flags[0];
                if flag.name.is_some() {
                    return None;
                }
                let value = flag.value.as_ref()?;
                let path = resolve_bazelrc_path(&file_path, &value.0)?;
                let url = Url::from_file_path(path).ok()?;
                Some(DocumentLink {
                    range: range_to_lsp(rope, &value.1)?,
                    target: Some(url),
                    tooltip: None,
                    data: None,
                })
            })
            .collect::<Vec<_>>();
        Ok(Some(links))
    }
}

fn load_bazel_flags() -> (BazelFlags, Option<String>) {
    if let Ok(bazel_command) = env::var("BAZELRC_LSP_RUN_BAZEL_PATH") {
        match load_bazel_flags_from_command(&bazel_command) {
            Ok(flags) => (flags, None),
            Err(msg) => {
                let bazel_version =
                    find_closest_version(AVAILABLE_BAZEL_VERSIONS.as_slice(), "latest");
                let message =
                    format!("Using flags from Bazel {bazel_version} because running `{bazel_command}` failed:\n{}\n", msg);
                (load_packaged_bazel_flags(&bazel_version), Some(message))
            }
        }
    } else if let Some(auto_detected) = auto_detect_bazel_version() {
        return (load_packaged_bazel_flags(&auto_detected.0), auto_detected.1);
    } else {
        let bazel_version = find_closest_version(AVAILABLE_BAZEL_VERSIONS.as_slice(), "latest");
        let message = format!(
            "Using flags from Bazel {bazel_version} because auto-detecting the Bazel version failed"        );
        (load_packaged_bazel_flags(&bazel_version), Some(message))
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (bazel_flags, version_message) = load_bazel_flags();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        document_map: Default::default(),
        bazel_flags,
        startup_warning: version_message,
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
