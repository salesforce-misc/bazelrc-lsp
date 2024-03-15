use bazelrc_lsp::bazel_flags::{load_bazel_flags, BazelFlags};
use bazelrc_lsp::completion::get_completion_items;
use bazelrc_lsp::diagnostic::{diagnostics_from_parser, diagnostics_from_rcconfig};
use bazelrc_lsp::parser::{parse_from_str, ParserResult};
use bazelrc_lsp::semantic_token::{
    convert_to_lsp_tokens, semantic_tokens_from_lines, RCSemanticToken, LEGEND_TYPE,
};
use dashmap::DashMap;
use ropey::Rope;
use tower_lsp::jsonrpc::Result;
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

        let mut diagnostics: Vec<Diagnostic> = Vec::<Diagnostic>::new();
        diagnostics.extend(diagnostics_from_parser(&rope, &errors));
        diagnostics.extend(diagnostics_from_rcconfig(&rope, &lines, &self.bazel_flags));

        self.document_map.insert(
            params.uri.to_string(),
            AnalyzedDocument {
                rope,
                semantic_tokens,
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
        let lsp_tokens = || -> Option<Vec<SemanticToken>> {
            let doc = self.document_map.get(&uri)?;
            let lsp_tokens = convert_to_lsp_tokens(&doc.rope, &doc.semantic_tokens);
            Some(lsp_tokens)
        }();
        self.client
            .log_message(MessageType::INFO, format!("tokens {:?}", &lsp_tokens))
            .await;
        if let Some(semantic_token) = lsp_tokens {
            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: semantic_token,
            })));
        }
        Ok(None)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        Ok(Some(CompletionResponse::Array(get_completion_items(
            &self.bazel_flags,
        ))))
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
