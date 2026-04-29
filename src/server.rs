use std::path::PathBuf;
use std::sync::Arc;

use futures::future::join_all;
use serde_json::Value;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::analyzer::diagnostic::Severity;
use crate::intellisense;
use crate::messages::Locale;
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

        let raw_diags = tokio::task::spawn_blocking(move || {
            state.blocking_read().analyze(&uri_str)
        })
        .await
        .unwrap_or_default();

        let diagnostics = raw_diags.into_iter().map(lsp_diagnostic_from).collect();
        self.client.publish_diagnostics(uri, diagnostics, None).await;
    }

    async fn republish_all_open_docs(&self) {
        let uris: Vec<Url> = {
            self.state
                .read()
                .await
                .open_docs
                .iter()
                .filter_map(|e| Url::parse(e.key()).ok())
                .collect()
        };
        join_all(uris.into_iter().map(|uri| self.publish_diagnostics_for(uri))).await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for PawnProServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let root = resolve_workspace_root(&params);
        let opts = params.initialization_options.as_ref();
        let config = ConfigUpdate::from_init_options(opts);

        {
            let mut state = self.state.write().await;
            if let Some(root) = root {
                state.set_workspace_root(root);
            }
            config.apply_init(&mut state);
        }

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "pawnpro-engine".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: server_capabilities(),
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

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        let settings = params.settings.get("settings").unwrap_or(&params.settings);
        let update = ConfigUpdate::from_settings(settings);

        let changed = {
            let mut state = self.state.write().await;
            update.apply_change(&mut state)
        };

        if changed {
            self.republish_all_open_docs().await;
        }
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        self.state.read().await.open_document(
            uri.to_string(),
            params.text_document.text,
            params.text_document.version,
        );
        self.publish_diagnostics_for(uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        if let Some(change) = params.content_changes.into_iter().last() {
            self.state.read().await.change_document(
                uri.as_str(),
                change.text,
                params.text_document.version,
            );
        }

        let dependents = self.state.read().await.open_dependents(uri.as_str());
        let mut targets: Vec<Url> = dependents
            .into_iter()
            .filter_map(|u| Url::parse(&u).ok())
            .collect();
        if targets.is_empty() {
            targets.push(uri);
        }
        join_all(targets.into_iter().map(|u| self.publish_diagnostics_for(u))).await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let dependents = self.state.read().await.open_dependents(uri.as_str());
        let mut targets: Vec<Url> = dependents
            .into_iter()
            .filter_map(|u| Url::parse(&u).ok())
            .collect();
        if targets.is_empty() {
            targets.push(uri);
        }
        join_all(targets.into_iter().map(|u| self.publish_diagnostics_for(u))).await;
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        let changed_paths: Vec<Url> = params
            .changes
            .into_iter()
            .map(|c| c.uri)
            .collect();

        let mut to_republish: std::collections::HashSet<String> = std::collections::HashSet::new();

        for uri in &changed_paths {
            let state = self.state.read().await;
            if let Some(path) = uri_to_path(uri.as_str()) {
                state.evict_path_from_cache(&path);
            }
            let dependents = state.open_dependents(uri.as_str());
            if dependents.is_empty() {
                to_republish.insert(uri.to_string());
            } else {
                to_republish.extend(dependents);
            }
        }

        let targets: Vec<Url> = to_republish
            .into_iter()
            .filter_map(|u| Url::parse(&u).ok())
            .collect();
        join_all(targets.into_iter().map(|u| self.publish_diagnostics_for(u))).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        self.state.read().await.close_document(uri.as_str());
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let trigger = params
            .context
            .as_ref()
            .and_then(|c| c.trigger_character.as_deref())
            .unwrap_or("");

        if trigger == "@" {
            return Ok(Some(CompletionResponse::Array(
                self.at_completions(&params).await,
            )));
        }

        let uri_str = params.text_document_position.text_document.uri.to_string();
        let position = params.text_document_position.position;
        let state = Arc::clone(&self.state);

        let items = tokio::task::spawn_blocking(move || {
            intellisense::get_completions(&state.blocking_read(), &uri_str, position)
        })
        .await
        .unwrap_or_default();

        Ok((!items.is_empty()).then_some(CompletionResponse::Array(items)))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri_str = params.text_document_position_params.text_document.uri.to_string();
        let position = params.text_document_position_params.position;
        let state = Arc::clone(&self.state);

        let result = tokio::task::spawn_blocking(move || {
            intellisense::get_hover(&state.blocking_read(), &uri_str, position)
        })
        .await
        .unwrap_or(None);

        Ok(result)
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri_str = params.text_document_position_params.text_document.uri.to_string();
        let position = params.text_document_position_params.position;
        let state = Arc::clone(&self.state);

        let result = tokio::task::spawn_blocking(move || {
            intellisense::get_signature_help(&state.blocking_read(), &uri_str, position)
        })
        .await
        .unwrap_or(None);

        Ok(result)
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri_str = params.text_document_position.text_document.uri.to_string();
        let position = params.text_document_position.position;
        let state = Arc::clone(&self.state);

        let locations = tokio::task::spawn_blocking(move || {
            intellisense::get_references(&state.blocking_read(), &uri_str, position)
        })
        .await
        .unwrap_or_default();

        Ok((!locations.is_empty()).then_some(locations))
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri_str = params.text_document.uri.to_string();
        let state = Arc::clone(&self.state);

        let lenses = tokio::task::spawn_blocking(move || {
            intellisense::get_code_lens(&state.blocking_read(), &uri_str)
        })
        .await
        .unwrap_or_default();

        Ok((!lenses.is_empty()).then_some(lenses))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri_str = params.text_document.uri.to_string();
        let state = Arc::clone(&self.state);

        let result = tokio::task::spawn_blocking(move || {
            intellisense::get_semantic_tokens(&state.blocking_read(), &uri_str)
        })
        .await
        .unwrap_or(None);

        Ok(result.map(SemanticTokensResult::Tokens))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri_str = params.text_document.uri.to_string();
        let opts = intellisense::FormatOptions::from_lsp(&params.options);
        let state = Arc::clone(&self.state);

        let edits = tokio::task::spawn_blocking(move || {
            let text = state.blocking_read().get_text(&uri_str)?;
            Some(intellisense::format_document(&text, &opts))
        })
        .await
        .unwrap_or_default()
        .unwrap_or_default();

        Ok((!edits.is_empty()).then_some(edits))
    }

    async fn range_formatting(
        &self,
        params: DocumentRangeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let uri_str = params.text_document.uri.to_string();
        let opts = intellisense::FormatOptions::from_lsp(&params.options);
        let range = params.range;
        let state = Arc::clone(&self.state);

        let edits = tokio::task::spawn_blocking(move || {
            let text = state.blocking_read().get_text(&uri_str)?;
            Some(intellisense::format_range(&text, range, &opts))
        })
        .await
        .unwrap_or_default()
        .unwrap_or_default();

        Ok((!edits.is_empty()).then_some(edits))
    }
}

impl PawnProServer {
    async fn at_completions(&self, params: &CompletionParams) -> Vec<CompletionItem> {
        let uri_str = params.text_document_position.text_document.uri.to_string();
        let pos = params.text_document_position.position;
        let state = Arc::clone(&self.state);

        tokio::task::spawn_blocking(move || {
            let state = state.blocking_read();
            let at_col = pos.character.saturating_sub(1);
            let in_comment = state.open_docs.get(&uri_str).map(|doc| {
                let line = doc.text.lines().nth(pos.line as usize).unwrap_or("");
                let col_bytes = (at_col as usize).min(line.len());
                let before = &line[..col_bytes];
                before.contains("//") || before.contains("/*") || line.trim_start().starts_with('*')
            }).unwrap_or(false);
            intellisense::get_at_completions(in_comment, pos.line, at_col, state.locale)
        })
        .await
        .unwrap_or_default()
    }
}

// --- Configuration update ---

struct ConfigUpdate {
    include_paths: Option<Vec<PathBuf>>,
    warn_unused: Option<bool>,
    suppress_in_inc: Option<bool>,
    sdk_file: Option<Option<PathBuf>>,
    locale: Option<Locale>,
}

impl ConfigUpdate {
    fn from_init_options(opts: Option<&Value>) -> Self {
        Self {
            include_paths: parse_include_paths(opts.and_then(|v| v.get("includePaths"))),
            warn_unused: opts.and_then(|v| v.get("warnUnusedInInc")).and_then(|v| v.as_bool()),
            suppress_in_inc: opts.and_then(|v| v.get("suppressDiagnosticsInInc")).and_then(|v| v.as_bool()),
            sdk_file: opts
                .and_then(|v| v.get("sdkFilePath"))
                .and_then(|v| v.as_str())
                .map(parse_sdk_path),
            locale: opts
                .and_then(|v| v.get("locale"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(Locale::from_str),
        }
    }

    fn from_settings(settings: &Value) -> Self {
        Self {
            include_paths: parse_include_paths(settings.get("includePaths")),
            warn_unused: settings.get("warnUnusedInInc").and_then(|v| v.as_bool()),
            suppress_in_inc: settings.get("suppressDiagnosticsInInc").and_then(|v| v.as_bool()),
            sdk_file: settings
                .get("sdkFilePath")
                .and_then(|v| v.as_str())
                .map(parse_sdk_path),
            locale: settings
                .get("locale")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(Locale::from_str),
        }
    }

    fn apply_init(self, state: &mut WorkspaceState) {
        if let Some(paths) = self.include_paths
            && !paths.is_empty() {
            state.include_paths_override = Some(paths);
            state.invalidate_tabsize_cache();
        }
        if let Some(warn) = self.warn_unused {
            state.config.analysis.warn_unused_in_inc = warn;
        }
        if let Some(suppress) = self.suppress_in_inc {
            state.config.analysis.suppress_diagnostics_in_inc = suppress;
        }
        if let Some(sdk) = self.sdk_file {
            state.set_sdk_file_opt(sdk);
        }
        if let Some(locale) = self.locale {
            state.locale = locale;
        }
    }

    /// Returns `true` if any field actually changed (so callers know to republish).
    fn apply_change(self, state: &mut WorkspaceState) -> bool {
        let mut changed = false;

        if let Some(paths) = self.include_paths {
            let new = if paths.is_empty() { None } else { Some(paths) };
            if state.include_paths_override != new {
                state.include_paths_override = new;
                state.invalidate_tabsize_cache();
                changed = true;
            }
        }
        if let Some(warn) = self.warn_unused
            && state.config.analysis.warn_unused_in_inc != warn {
            state.config.analysis.warn_unused_in_inc = warn;
            changed = true;
        }
        if let Some(suppress) = self.suppress_in_inc
            && state.config.analysis.suppress_diagnostics_in_inc != suppress {
            state.config.analysis.suppress_diagnostics_in_inc = suppress;
            changed = true;
        }
        if let Some(sdk_path) = self.sdk_file
            && state.sdk_file.as_deref() != sdk_path.as_deref() {
            state.set_sdk_file_opt(sdk_path);
            changed = true;
        }
        if let Some(locale) = self.locale
            && state.locale != locale {
            state.locale = locale;
            changed = true;
        }

        changed
    }
}

// --- Helpers ---

fn resolve_workspace_root(params: &InitializeParams) -> Option<PathBuf> {
    params
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
        })
}

fn parse_include_paths(value: Option<&Value>) -> Option<Vec<PathBuf>> {
    value?.as_array().map(|arr| {
        arr.iter()
            .filter_map(|s| s.as_str())
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .filter(|p| !p.components().any(|c| c == std::path::Component::ParentDir))
            .collect()
    })
}

fn parse_sdk_path(s: &str) -> Option<PathBuf> {
    if s.is_empty() {
        return None;
    }
    let p = PathBuf::from(s);
    if p.components().any(|c| c == std::path::Component::ParentDir) {
        return None;
    }
    Some(p)
}

fn lsp_diagnostic_from(d: crate::analyzer::PawnDiagnostic) -> Diagnostic {
    let range = Range {
        start: Position { line: d.line, character: d.col_start },
        end: Position { line: d.line, character: d.col_end },
    };
    let severity = match d.severity {
        Severity::Error   => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Hint    => DiagnosticSeverity::HINT,
    };
    let tags: Vec<DiagnosticTag> = [
        d.unnecessary.then_some(DiagnosticTag::UNNECESSARY),
        d.deprecated.then_some(DiagnosticTag::DEPRECATED),
    ]
    .into_iter()
    .flatten()
    .collect();

    Diagnostic {
        range,
        severity: Some(severity),
        code: Some(NumberOrString::String(d.code.to_string())),
        source: Some("pawnpro".to_string()),
        message: d.message,
        tags: (!tags.is_empty()).then_some(tags),
        ..Default::default()
    }
}

fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Options(TextDocumentSyncOptions {
            open_close: Some(true),
            change: Some(TextDocumentSyncKind::FULL),
            save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                include_text: Some(false),
            })),
            ..Default::default()
        })),
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
        code_lens_provider: Some(CodeLensOptions { resolve_provider: Some(false) }),
        references_provider: Some(OneOf::Left(true)),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                legend: intellisense::semantic_tokens_legend(),
                full: Some(SemanticTokensFullOptions::Bool(true)),
                ..Default::default()
            },
        )),
        document_formatting_provider: Some(OneOf::Left(true)),
        document_range_formatting_provider: Some(OneOf::Left(true)),
        workspace: Some(WorkspaceServerCapabilities {
            workspace_folders: None,
            file_operations: None,
        }),
        ..Default::default()
    }
}
