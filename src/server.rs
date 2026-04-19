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
        let state = Arc::clone(&self.state);
        let uri_str = uri.to_string();

        // analyze() faz leituras de disco síncronas — spawn_blocking evita bloquear o runtime
        let raw_diags = tokio::task::spawn_blocking(move || {
            let state = state.blocking_read();
            state.analyze(&uri_str)
        })
        .await
        .unwrap_or_default();

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
                    Severity::Hint => DiagnosticSeverity::HINT,
                };
                let mut diag = Diagnostic {
                    range,
                    severity: Some(severity),
                    code: Some(NumberOrString::String(d.code.to_string())),
                    source: Some("pawnpro".to_string()),
                    message: d.message,
                    ..Default::default()
                };
                let mut tags: Vec<DiagnosticTag> = Vec::new();
                if d.unnecessary { tags.push(DiagnosticTag::UNNECESSARY); }
                if d.deprecated  { tags.push(DiagnosticTag::DEPRECATED); }
                if !tags.is_empty() { diag.tags = Some(tags); }
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

        let init_opts = params.initialization_options.as_ref();
        let opt_include_paths: Option<Vec<PathBuf>> = init_opts
            .and_then(|v| v.get("includePaths"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| s.as_str())
                    .filter(|s| !s.is_empty())
                    .map(PathBuf::from)
                    .filter(|p| !p.components().any(|c| c == std::path::Component::ParentDir))
                    .collect()
            });
        let opt_warn_unused: Option<bool> = init_opts
            .and_then(|v| v.get("warnUnusedInInc"))
            .and_then(|v| v.as_bool());
        let opt_sdk_file: Option<String> = init_opts
            .and_then(|v| v.get("sdkFilePath"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .filter(|s| !PathBuf::from(s).components().any(|c| c == std::path::Component::ParentDir))
            .map(|s| s.to_string());

        {
            let mut state = self.state.write().await;

            if let Some(root) = root {
                state.set_workspace_root(root);
            }
            if let Some(paths) = opt_include_paths
                && !paths.is_empty()
            {
                state.include_paths_override = Some(paths);
            }
            if let Some(warn) = opt_warn_unused {
                state.config.analysis.warn_unused_in_inc = warn;
            }
            if let Some(sdk) = opt_sdk_file {
                state.set_sdk_file(PathBuf::from(sdk));
            }
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
                    trigger_characters: Some(vec![
                        ".".to_string(),
                        "#".to_string(),
                        "@".to_string(),
                    ]),
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
                references_provider: Some(OneOf::Left(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: intellisense::semantic_tokens_legend(),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            ..Default::default()
                        },
                    ),
                ),
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: None,
                    file_operations: None,
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

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        let settings = &params.settings;

        let opt_include_paths: Option<Vec<PathBuf>> = settings
            .get("includePaths")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| s.as_str())
                    .filter(|s| !s.is_empty())
                    .map(PathBuf::from)
                    .filter(|p| !p.components().any(|c| c == std::path::Component::ParentDir))
                    .collect()
            });
        let opt_warn_unused: Option<bool> = settings
            .get("warnUnusedInInc")
            .and_then(|v| v.as_bool());
        let opt_sdk_file: Option<Option<PathBuf>> = settings
            .get("sdkFilePath")
            .and_then(|v| v.as_str())
            .map(|s| {
                if s.is_empty() {
                    None
                } else {
                    let p = PathBuf::from(s);
                    if p.components().any(|c| c == std::path::Component::ParentDir) {
                        None
                    } else {
                        Some(p)
                    }
                }
            });

        let changed = {
            let mut state = self.state.write().await;
            let mut changed = false;

            if let Some(paths) = opt_include_paths
                && state.include_paths_override.as_deref() != Some(&paths) {
                state.include_paths_override = if paths.is_empty() { None } else { Some(paths) };
                changed = true;
            }
            if let Some(warn) = opt_warn_unused
                && state.config.analysis.warn_unused_in_inc != warn {
                state.config.analysis.warn_unused_in_inc = warn;
                changed = true;
            }
            if let Some(sdk_path) = opt_sdk_file {
                let current = state.sdk_file.as_deref();
                if current != sdk_path.as_deref() {
                    state.set_sdk_file_opt(sdk_path);
                    changed = true;
                }
            }

            changed
        };

        if changed {
            let uris: Vec<Url> = {
                let state = self.state.read().await;
                state
                    .open_docs
                    .iter()
                    .filter_map(|e| Url::parse(&format!("file://{}", e.key())).ok())
                    .collect()
            };
            for uri in uris {
                self.publish_diagnostics_for(uri).await;
            }
        }
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri.clone();
        let position = params.text_document_position.position;
        let state = Arc::clone(&self.state);
        let uri_str = uri.to_string();

        let locations = tokio::task::spawn_blocking(move || {
            let state = state.blocking_read();
            intellisense::get_references(&state, &uri_str, position)
        })
        .await
        .unwrap_or_default();

        if locations.is_empty() {
            Ok(None)
        } else {
            Ok(Some(locations))
        }
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

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
        self.client
            .publish_diagnostics(uri, vec![], None)
            .await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let trigger = params
            .context
            .as_ref()
            .and_then(|c| c.trigger_character.as_deref())
            .unwrap_or("");

        if trigger == "@" {
            let uri_str = params.text_document_position.text_document.uri.to_string();
            let pos     = params.text_document_position.position;
            let state   = Arc::clone(&self.state);

            let items = tokio::task::spawn_blocking(move || {
                let state  = state.blocking_read();
                let at_col = pos.character.saturating_sub(1);
                let in_comment = state.open_docs.get(&uri_str).map(|doc| {
                    let ln = doc.text.lines().nth(pos.line as usize).unwrap_or("");
                    let col_bytes = (at_col as usize).min(ln.len());
                    let before = &ln[..col_bytes];
                    before.contains("//")
                        || before.contains("/*")
                        || ln.trim_start().starts_with('*')
                }).unwrap_or(false);
                intellisense::get_at_completions(in_comment, pos.line, at_col)
            })
            .await
            .unwrap_or_default();

            return Ok(Some(CompletionResponse::Array(items)));
        }

        let uri = params.text_document_position.text_document.uri;
        let state = Arc::clone(&self.state);
        let uri_str = uri.to_string();

        let items = tokio::task::spawn_blocking(move || {
            let state = state.blocking_read();
            intellisense::get_completions(&state, &uri_str)
        })
        .await
        .unwrap_or_default();

        if items.is_empty() {
            Ok(None)
        } else {
            Ok(Some(CompletionResponse::Array(items)))
        }
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let state = Arc::clone(&self.state);
        let uri_str = uri.to_string();

        let result = tokio::task::spawn_blocking(move || {
            let state = state.blocking_read();
            intellisense::get_hover(&state, &uri_str, position)
        })
        .await
        .unwrap_or(None);

        Ok(result)
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let state = Arc::clone(&self.state);
        let uri_str = uri.to_string();

        let result = tokio::task::spawn_blocking(move || {
            let state = state.blocking_read();
            intellisense::get_signature_help(&state, &uri_str, position)
        })
        .await
        .unwrap_or(None);

        Ok(result)
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;
        let state = Arc::clone(&self.state);
        let uri_str = uri.to_string();

        let result = tokio::task::spawn_blocking(move || {
            let state = state.blocking_read();
            intellisense::get_semantic_tokens(&state, &uri_str)
        })
        .await
        .unwrap_or(None);

        Ok(result.map(SemanticTokensResult::Tokens))
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri = params.text_document.uri;
        let state = Arc::clone(&self.state);
        let uri_str = uri.to_string();

        let lenses = tokio::task::spawn_blocking(move || {
            let state = state.blocking_read();
            intellisense::get_code_lens(&state, &uri_str)
        })
        .await
        .unwrap_or_default();

        if lenses.is_empty() {
            Ok(None)
        } else {
            Ok(Some(lenses))
        }
    }
}
