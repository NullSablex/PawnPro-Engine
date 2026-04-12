use std::path::PathBuf;

use dashmap::DashMap;

use crate::analyzer::PawnDiagnostic;
use crate::analyzer::{deprecated, includes, semantic, unused};
use crate::config::EngineConfig;
use crate::parser::{parse_file, ParsedFile};

/// Representa um documento aberto (pelo editor) ou lido do disco.
#[derive(Debug, Clone)]
pub struct Document {
    pub uri: String,
    pub text: String,
    pub version: i32,
}

/// Estado do workspace: documentos abertos + config.
pub struct WorkspaceState {
    pub workspace_root: Option<PathBuf>,
    pub config: EngineConfig,
    /// Documentos atualmente abertos no editor (uri → Document).
    pub open_docs: DashMap<String, Document>,
    /// Cache de ParsedFile por caminho normalizado.
    pub parsed_cache: DashMap<String, ParsedFile>,
}

impl WorkspaceState {
    pub fn new() -> Self {
        Self {
            workspace_root: None,
            config: EngineConfig::default(),
            open_docs: DashMap::new(),
            parsed_cache: DashMap::new(),
        }
    }

    pub fn set_workspace_root(&mut self, root: PathBuf) {
        self.config = EngineConfig::load(Some(&root));
        self.workspace_root = Some(root);
    }

    pub fn include_paths(&self) -> Vec<PathBuf> {
        self.config.resolved_include_paths(self.workspace_root.as_deref())
    }

    pub fn open_document(&self, uri: String, text: String, version: i32) {
        let key = uri_to_cache_key(&uri);
        self.parsed_cache.remove(&key);
        self.open_docs.insert(uri, Document { uri: key.clone(), text, version });
    }

    pub fn change_document(&self, uri: &str, text: String, version: i32) {
        let key = uri_to_cache_key(uri);
        self.parsed_cache.remove(&key);
        if let Some(mut doc) = self.open_docs.get_mut(uri) {
            doc.text = text;
            doc.version = version;
        } else {
            self.open_docs.insert(uri.to_string(), Document { uri: key, text, version });
        }
    }

    pub fn close_document(&self, uri: &str) {
        self.open_docs.remove(uri);
        let key = uri_to_cache_key(uri);
        self.parsed_cache.remove(&key);
    }

    /// Retorna o texto de um documento: abre do cache se aberto, senão lê do disco.
    pub fn get_text(&self, uri: &str) -> Option<String> {
        if let Some(doc) = self.open_docs.get(uri) {
            return Some(doc.text.clone());
        }
        let path = uri_to_path(uri)?;
        std::fs::read(&path).ok().map(|b| decode_text(&b))
    }

    /// Parseia (ou retorna do cache) o ParsedFile de um URI.
    pub fn get_parsed(&self, uri: &str) -> Option<ParsedFile> {
        let key = uri_to_cache_key(uri);
        if let Some(cached) = self.parsed_cache.get(&key) {
            return Some(cached.value().clone());
        }
        let text = self.get_text(uri)?;
        let parsed = parse_file(&text);
        self.parsed_cache.insert(key, parsed.clone());
        Some(parsed)
    }

    /// Parseia (ou retorna do cache) o ParsedFile de um caminho de arquivo.
    pub fn get_parsed_by_path(&self, path: &std::path::Path) -> Option<ParsedFile> {
        let key = path.to_string_lossy().to_string();
        if let Some(cached) = self.parsed_cache.get(&key) {
            return Some(cached.value().clone());
        }
        let bytes = std::fs::read(path).ok()?;
        let text = decode_text(&bytes);
        let parsed = parse_file(&text);
        self.parsed_cache.insert(key, parsed.clone());
        Some(parsed)
    }

    /// Roda todos os analyzers no documento e retorna os diagnósticos.
    pub fn analyze(&self, uri: &str) -> Vec<PawnDiagnostic> {
        let Some(text) = self.get_text(uri) else { return vec![] };
        let Some(file_path) = uri_to_path(uri) else { return vec![] };
        let parsed = parse_file(&text);
        let inc_paths = self.include_paths();

        let mut diags = Vec::new();
        diags.extend(includes::analyze_includes(&parsed.includes, &file_path, &inc_paths));
        diags.extend(semantic::analyze_semantics(&text));
        diags.extend(unused::analyze_unused(
            &text, &file_path, &parsed, &inc_paths,
            self.config.analysis.warn_unused_in_inc,
        ));
        diags.extend(deprecated::analyze_deprecated(&text, &file_path, &parsed, &inc_paths));

        // Atualiza o cache após análise completa
        let key = uri_to_cache_key(uri);
        self.parsed_cache.insert(key, parsed);

        diags
    }
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Helpers URI ──────────────────────────────────────────────────────────

/// Converte URI LSP (file:///path) para PathBuf.
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let path = uri.strip_prefix("file://")?;
    // No Windows: file:///C:/... → /C:/... → C:/...
    #[cfg(target_os = "windows")]
    let path = path.trim_start_matches('/');
    let decoded = percent_decode(path);
    Some(PathBuf::from(decoded))
}

/// Chave de cache normalizada a partir de um URI.
fn uri_to_cache_key(uri: &str) -> String {
    uri_to_path(uri)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| uri.to_string())
}

/// Decodifica %XX em URIs (apenas os mais comuns).
fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte as char);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn decode_text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}
