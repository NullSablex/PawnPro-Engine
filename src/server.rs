use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::analyzer::diagnostic::Severity;
use crate::intellisense;
use crate::workspace::{uri_to_path, WorkspaceState};

pub struct PawnProServer {
    client: Client,
    state: Arc<RwLock<WorkspaceState>>,
}

impl PawnProServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            state: Arc::new(RwLock::new(WorkspaceState::new())),
        }
    }

    async fn publish_diagnostics_for(&self, uri: Url) {
        let state = self.state.read().await;
        let raw_diags = state.analyze(uri.as_str());
        drop(state);

        let diagnostics: Vec<Diagnostic> = raw_diags
            .into_iter()
            .map(|d| {
                let range = Range {
                    start: Position { line: d.line, character: d.col_start },
                    end: Position { line: d.line, character: d.col_end },
                };
                let severity = match d.severity {
                    Severity::Error => DiagnosticSeverity::ERROR,
                    Severity::Warning => DiagnosticSeverity::WARNING,
                    Severity::Info => DiagnosticSeverity::INFORMATION,
                };
                let mut diag = Diagnostic {
                    range,
                    severity: Some(severity),
                    code: Some(NumberOrString::String(d.code.to_string())),
                    source: Some("pawnpro".to_string()),
                    message: d.message,
                    ..Default::default()
                };
                if d.unnecessary {
                    diag.tags = Some(vec![DiagnosticTag::UNNECESSARY]);
                }
                diag
            })
            .collect();

        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for PawnProServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // Configura o workspace root
        let root = params
            .workspace_folders
            .as_deref()
            .and_then(|f| f.first())
            .and_then(|f| uri_to_path(f.uri.as_str()))
            .or_else(|| {
                #[allow(deprecated)]
                params.root_uri.as_ref().and_then(|u| uri_to_path(u.as_str()))
            })
            .or_else(|| {
                #[allow(deprecated)]
                params.root_path.as_deref().map(PathBuf::from)
            });

        if let Some(root) = root {
            let mut state = self.state.write().await;
            state.set_workspace_root(root);
        }

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "pawnpro-engine".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".to_string(), "#".to_string()]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    retrigger_characters: Some(vec![",".to_string()]),
                    ..Default::default()
                }),
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "PawnPro engine initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    // ── Document sync ──────────────────────────────────────────────────────

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        {
            let state = self.state.read().await;
            state.open_document(
                uri.to_string(),
                params.text_document.text,
                params.text_document.version,
            );
        }
        self.publish_diagnostics_for(uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        if let Some(change) = params.content_changes.into_iter().last() {
            let state = self.state.read().await;
            state.change_document(
                uri.as_str(),
                change.text,
                params.text_document.version,
            );
        }
        self.publish_diagnostics_for(uri).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        {
            let state = self.state.read().await;
            state.close_document(uri.as_str());
        }
        // Limpa os diagnósticos ao fechar o documento
        self.client
            .publish_diagnostics(uri, vec![], None)
            .await;
    }

    // ── IntelliSense (Fase 4) ─────────────────────────────────────────────

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let state = self.state.read().await;
        let items = intellisense::get_completions(&state, uri.as_str());
        if items.is_empty() {
            Ok(None)
        } else {
            Ok(Some(CompletionResponse::Array(items)))
        }
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let state = self.state.read().await;
        Ok(intellisense::get_hover(&state, uri.as_str(), position))
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let state = self.state.read().await;
        Ok(intellisense::get_signature_help(&state, uri.as_str(), position))
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri = params.text_document.uri;
        let state = self.state.read().await;
        let lenses = intellisense::get_code_lens(&state, uri.as_str());
        if lenses.is_empty() {
            Ok(None)
        } else {
            Ok(Some(lenses))
        }
    }
}
