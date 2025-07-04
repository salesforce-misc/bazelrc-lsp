use crate::bazel_flags::{combine_key_value_flags, BazelFlags, COMMAND_DOCS};
use crate::completion::get_completion_items;
use crate::definition::get_definitions;
use crate::diagnostic::{diagnostics_from_parser, diagnostics_from_rcconfig};
use crate::file_utils::resolve_bazelrc_path;
use crate::formatting::{get_text_edits_for_lines, FormatLineFlow};
use crate::line_index::{IndexEntry, IndexEntryKind, IndexedLines};
use crate::lsp_utils::{decode_lsp_pos, encode_lsp_range, LspPositionEncoding};
use crate::parser::{parse_from_str, Line, ParserResult};
use crate::semantic_token::{
    convert_to_lsp_tokens, semantic_tokens_from_lines, RCSemanticToken, LEGEND_TYPE,
};
use dashmap::DashMap;
use ropey::Rope;
use serde::{Deserialize, Serialize};
use tower_lsp::jsonrpc::{Error, Result};
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

struct TextDocumentItem {
    uri: Url,
    text: String,
    version: i32,
}

#[derive(Debug)]
pub struct AnalyzedDocument {
    rope: Rope,
    semantic_tokens: Vec<RCSemanticToken>,
    indexed_lines: IndexedLines,
    has_parser_errors: bool,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub format_lines: FormatLineFlow,
}

#[derive(Debug)]
pub struct Backend {
    pub client: Client,
    pub document_map: DashMap<String, AnalyzedDocument>,
    pub bazel_flags: BazelFlags,
    pub position_encoding: std::sync::RwLock<LspPositionEncoding>,
    pub settings: std::sync::RwLock<Settings>,
    // An optional message which should be displayed to the user on startup
    pub startup_warning: Option<String>,
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

        let position_encoding = *self.position_encoding.read().unwrap();
        let mut diagnostics: Vec<Diagnostic> = Vec::<Diagnostic>::new();
        diagnostics.extend(diagnostics_from_parser(&rope, &errors, position_encoding));
        diagnostics.extend(diagnostics_from_rcconfig(
            &rope,
            &indexed_lines.lines,
            &self.bazel_flags,
            file_path,
            position_encoding,
        ));

        self.document_map.insert(
            params.uri.to_string(),
            AnalyzedDocument {
                rope,
                semantic_tokens,
                indexed_lines,
                has_parser_errors: !errors.is_empty(),
            },
        );

        self.client
            .publish_diagnostics(params.uri.clone(), diagnostics, Some(params.version))
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, init_params: InitializeParams) -> Result<InitializeResult> {
        // Choose the position encoding format.
        let supported_encodings = init_params
            .capabilities
            .general
            .unwrap_or_default()
            .position_encodings
            .unwrap_or_default();
        let selected_encoding = supported_encodings
            .iter()
            .filter_map(|e| {
                if *e == PositionEncodingKind::UTF8 {
                    Some(LspPositionEncoding::UTF8)
                } else if *e == PositionEncodingKind::UTF16 {
                    Some(LspPositionEncoding::UTF16)
                } else if *e == PositionEncodingKind::UTF32 {
                    Some(LspPositionEncoding::UTF32)
                } else {
                    None
                }
            })
            .next()
            .unwrap_or(LspPositionEncoding::UTF16);
        *self.position_encoding.write().unwrap() = selected_encoding;

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "bazelrc Language Server".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
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

    async fn did_change_configuration(&self, mut params: DidChangeConfigurationParams) {
        let Some(bazelrc_settings) = params
            .settings
            .as_object_mut()
            .and_then(|o| o.remove("bazelrc"))
        else {
            return;
        };
        match serde_json::from_value::<Settings>(bazelrc_settings) {
            Ok(new_settings) => *self.settings.write().unwrap() = new_settings,
            Err(err) => {
                self.client
                    .show_message(MessageType::ERROR, format!("Invalid settings: {}", err))
                    .await;
                self.client
                    .log_message(MessageType::ERROR, format!("Invalid settings: {}", err))
                    .await;
            }
        }
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
        let position_encoding = *self.position_encoding.read().unwrap();
        let text_document_position = params.text_document_position;
        let uri = text_document_position.text_document.uri.to_string();
        let doc = self
            .document_map
            .get(&uri)
            .ok_or(Error::invalid_params("Unknown document!"))?;
        let pos = decode_lsp_pos(
            &doc.rope,
            &text_document_position.position,
            position_encoding,
        )
        .ok_or(Error::invalid_params("Position out of range"))?;

        Ok(Some(CompletionResponse::Array(get_completion_items(
            &self.bazel_flags,
            &doc.rope,
            &doc.indexed_lines,
            pos,
            position_encoding,
        ))))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let position_encoding = *self.position_encoding.read().unwrap();
        let uri = params.text_document_position_params.text_document.uri;
        let file_path = uri
            .to_file_path()
            .ok()
            .ok_or(Error::invalid_params("Unsupported URI scheme!"))?;
        let doc = self
            .document_map
            .get(&uri.to_string())
            .ok_or(Error::invalid_params("Unknown document!"))?;
        let pos = decode_lsp_pos(
            &doc.rope,
            &params.text_document_position_params.position,
            position_encoding,
        )
        .ok_or(Error::invalid_params("Position out of range"))?;
        let IndexEntry { kind, line_nr, .. } =
            doc.indexed_lines.find_symbol_at_position(pos).unwrap();
        let definitions = get_definitions(&file_path, kind, &doc.indexed_lines.lines[*line_nr]);
        Ok(definitions)
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        // Find the right document and offset
        let position_encoding = *self.position_encoding.read().unwrap();
        let text_document_position = params.text_document_position_params;
        let uri = text_document_position.text_document.uri.to_string();
        let doc = self
            .document_map
            .get(&uri)
            .ok_or(Error::invalid_params("Unknown document!"))?;
        let pos = decode_lsp_pos(
            &doc.rope,
            &text_document_position.position,
            position_encoding,
        )
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
                                range: encode_lsp_range(&doc.rope, span, position_encoding),
                            }
                        })
                }
                IndexEntryKind::Config => None,
                IndexEntryKind::FlagValue(flag_nr) | IndexEntryKind::FlagName(flag_nr) => {
                    let line = &doc.indexed_lines.lines[*line_nr];
                    let flag_name = &line.flags.get(*flag_nr)?.name.as_ref()?.0;
                    let (_, flag_info) = self.bazel_flags.get_by_invocation(flag_name)?;
                    let content = flag_info.get_documentation_markdown();
                    let contents = HoverContents::Scalar(MarkedString::String(content));
                    Some(Hover {
                        contents,
                        range: encode_lsp_range(&doc.rope, span, position_encoding),
                    })
                }
            }
        }())
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        // Find the right document
        let position_encoding = *self.position_encoding.read().unwrap();
        let uri = params.text_document.uri.to_string();
        let doc = self
            .document_map
            .get(&uri)
            .ok_or(Error::invalid_params("Unknown document!"))?;
        let rope = &doc.rope;

        if doc.has_parser_errors {
            return Err(Error::invalid_params(
                "Formatting can only be applied if there are no parsing errors",
            ));
        }

        // Format all lines
        let lines = &doc.indexed_lines.lines;
        Ok(Some(get_text_edits_for_lines(
            lines,
            rope,
            self.settings.read().unwrap().format_lines,
            position_encoding,
        )))
    }

    async fn range_formatting(
        &self,
        params: DocumentRangeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        // Find the right document
        let position_encoding = *self.position_encoding.read().unwrap();
        let uri = params.text_document.uri.to_string();
        let doc = self
            .document_map
            .get(&uri)
            .ok_or(Error::invalid_params("Unknown document!"))?;
        let rope = &doc.rope;

        if doc.has_parser_errors {
            return Err(Error::invalid_params(
                "Formatting can only be applied if there are no parsing errors",
            ));
        }

        // Format the line range
        let all_lines = &doc.indexed_lines.lines;
        let start_offset = decode_lsp_pos(rope, &params.range.start, position_encoding)
            .ok_or(Error::invalid_params("Position out of range!"))?;
        let end_offset = decode_lsp_pos(rope, &params.range.end, position_encoding)
            .ok_or(Error::invalid_params("Position out of range!"))?;
        // XXX not correct, yet
        let first_idx = all_lines.partition_point(|l: &Line| l.span.start < start_offset);
        let last_idx = all_lines.partition_point(|l: &Line| l.span.end < end_offset) + 1;

        Ok(Some(get_text_edits_for_lines(
            &all_lines[first_idx..last_idx],
            rope,
            self.settings.read().unwrap().format_lines,
            position_encoding,
        )))
    }

    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        // Find the right document
        let position_encoding = *self.position_encoding.read().unwrap();
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
                    range: encode_lsp_range(rope, &value.1, position_encoding)?,
                    target: Some(url),
                    tooltip: None,
                    data: None,
                })
            })
            .collect::<Vec<_>>();
        Ok(Some(links))
    }
}
