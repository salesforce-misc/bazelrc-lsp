use bazelrc_lsp::bazel_flags::{load_bazel_flags, BazelFlags, COMMAND_DOCS};
use bazelrc_lsp::completion::get_completion_items;
use bazelrc_lsp::diagnostic::{diagnostics_from_parser, diagnostics_from_rcconfig};
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
}

impl Backend {
    async fn on_change(&self, params: TextDocumentItem) {
        let rope = ropey::Rope::from_str(&params.text);
        let src = rope.to_string();

        let ParserResult {
            tokens: _,
            lines,
            errors,
        } = parse_from_str(&src);
        let semantic_tokens = semantic_tokens_from_lines(&lines);
        let indexed_lines = IndexedLines::from_lines(lines);

        let mut diagnostics: Vec<Diagnostic> = Vec::<Diagnostic>::new();
        diagnostics.extend(diagnostics_from_parser(&rope, &errors));
        diagnostics.extend(diagnostics_from_rcconfig(
            &rope,
            &indexed_lines.lines,
            &self.bazel_flags,
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
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "server initialized!")
            .await;
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
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        document_map: Default::default(),
        bazel_flags: load_bazel_flags(),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
