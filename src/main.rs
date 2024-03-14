use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use dashmap::DashMap;


struct TextDocumentItem {
    uri: Url,
    text: String,
    version: i32,
}

#[derive(Debug)]
struct AnalyzedDocument {
    content: String,
}

#[derive(Debug)]
struct Backend {
    client: Client,
    document_map: DashMap<String, AnalyzedDocument>,
}

impl Backend {
    async fn on_change(&self, params: TextDocumentItem) {
        self.document_map.insert(
            params.uri.to_string(),
            AnalyzedDocument {
                content: params.text
            },
        );

        let mut diagnostics = Vec::<Diagnostic>::new();
        diagnostics.push(Diagnostic{ range: Range {start: Position{ line: 0, character: 0 }, end: Position{ line: 1, character: 0 }}, message: "LSP under development".to_string(), ..Default::default() });

        self.client
            .publish_diagnostics(params.uri.clone(), diagnostics, Some(params.version))
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult{
            server_info: Some(ServerInfo {
                name: "bazelrc Language Server".to_string(),
                version: Some("1".to_string()),
            }),
            offset_encoding: None,
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
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
        self.document_map.remove(&params.text_document.uri.to_string());
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend {client, document_map: Default::default() });
    Server::new(stdin, stdout, socket).serve(service).await;
}
