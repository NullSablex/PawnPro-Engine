use std::path::PathBuf;

use dashmap::DashMap;

use crate::analyzer::PawnDiagnostic;
use crate::analyzer::{deprecated, hints, includes, semantic, undefined, unused};
use crate::analyzer::includes::collect_included_files;
use crate::config::EngineConfig;
use crate::parser::{parse_file, ParsedFile};
use crate::parser::lexer::decode_bytes;

#[derive(Debug, Clone)]
pub struct Document {
    pub text: String,
    pub version: i32,
}

pub struct WorkspaceState {
    pub workspace_root: Option<PathBuf>,
    pub config: EngineConfig,
    pub include_paths_override: Option<Vec<PathBuf>>,
    pub open_docs: DashMap<String, Document>,
    pub parsed_cache: DashMap<String, ParsedFile>,
    pub sdk_file: Option<PathBuf>,
    pub sdk_parsed: Option<ParsedFile>,
}

impl WorkspaceState {
    pub fn new() -> Self {
        Self {
            workspace_root: None,
            config: EngineConfig::default(),
            include_paths_override: None,
            open_docs: DashMap::new(),
            parsed_cache: DashMap::new(),
            sdk_file: None,
            sdk_parsed: None,
        }
    }

    pub fn set_sdk_file(&mut self, path: PathBuf) {
        self.sdk_parsed = parse_sdk(&path);
        self.sdk_file = Some(path);
    }

    pub fn set_sdk_file_opt(&mut self, path: Option<PathBuf>) {
        match path {
            Some(p) => self.set_sdk_file(p),
            None => { self.sdk_file = None; self.sdk_parsed = None; }
        }
    }

    pub fn set_workspace_root(&mut self, root: PathBuf) {
        self.config = EngineConfig::load(Some(&root));
        self.workspace_root = Some(root);
    }

    pub fn include_paths(&self) -> Vec<PathBuf> {
        if let Some(ref paths) = self.include_paths_override {
            return paths.clone();
        }
        self.config.resolved_include_paths(self.workspace_root.as_deref())
    }

    pub fn open_document(&self, uri: String, text: String, version: i32) {
        let key = uri_to_cache_key(&uri);
        self.parsed_cache.remove(&key);
        self.open_docs.insert(uri, Document { text, version });
    }

    pub fn change_document(&self, uri: &str, text: String, version: i32) {
        let key = uri_to_cache_key(uri);
        self.parsed_cache.remove(&key);
        if let Some(mut doc) = self.open_docs.get_mut(uri) {
            doc.text = text;
            doc.version = version;
        } else {
            self.open_docs.insert(uri.to_string(), Document { text, version });
        }
    }

    pub fn close_document(&self, uri: &str) {
        self.open_docs.remove(uri);
        let key = uri_to_cache_key(uri);
        self.parsed_cache.remove(&key);
    }

    pub fn get_text(&self, uri: &str) -> Option<String> {
        if let Some(doc) = self.open_docs.get(uri) {
            return Some(doc.text.clone());
        }
        let path = uri_to_path(uri)?;
        std::fs::read(&path).ok().map(|b| decode_bytes(&b))
    }

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

    pub fn get_parsed_by_path(&self, path: &std::path::Path) -> Option<ParsedFile> {
        let key = path.to_string_lossy().to_string();
        if let Some(cached) = self.parsed_cache.get(&key) {
            return Some(cached.value().clone());
        }
        let bytes = std::fs::read(path).ok()?;
        let text = decode_bytes(&bytes);
        let parsed = parse_file(&text);
        self.parsed_cache.insert(key, parsed.clone());
        Some(parsed)
    }

    pub fn analyze(&self, uri: &str) -> Vec<PawnDiagnostic> {
        let Some(text) = self.get_text(uri) else { return vec![] };
        let Some(file_path) = uri_to_path(uri) else { return vec![] };
        let parsed = parse_file(&text);
        let inc_paths = self.include_paths();
        let resolved = collect_included_files(&file_path, &inc_paths, &parsed.includes, 16, 1000);

        let mut diags = Vec::new();
        diags.extend(includes::analyze_includes(&parsed.includes, &file_path, &inc_paths, self.workspace_root.as_deref()));
        diags.extend(semantic::analyze_semantics(&text));
        diags.extend(unused::analyze_unused(
            &text, &file_path, &parsed, &resolved,
            self.config.analysis.warn_unused_in_inc,
            self.workspace_root.as_deref(),
        ));
        diags.extend(deprecated::analyze_deprecated(&text, &file_path, &parsed, &inc_paths, &resolved));
        diags.extend(hints::analyze_hints(&text, &parsed.symbols));
        diags.extend(undefined::analyze_undefined(&text, &file_path, &parsed, &resolved, self.sdk_parsed.as_ref()));

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

pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let path = uri.strip_prefix("file://")?;
    #[cfg(target_os = "windows")]
    let path = path.trim_start_matches('/');
    let decoded = percent_decode(path);
    let p = PathBuf::from(decoded);
    // Reject traversal attempts before canonicalization resolves them
    if p.components().any(|c| c == std::path::Component::ParentDir) {
        return None;
    }
    Some(p)
}

fn uri_to_cache_key(uri: &str) -> String {
    uri_to_path(uri)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| uri.to_string())
}

fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len()
            && let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3])
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            out.push(byte as char);
            i += 3;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn parse_sdk(path: &PathBuf) -> Option<ParsedFile> {
    let bytes = std::fs::read(path).ok()?;
    let text = decode_bytes(&bytes);
    let mut root = parse_file(&text);

    // Resolve transitive includes so SDK symbols from re-exported files are collected.
    // open.mp.inc itself has almost no symbols — they live in _open_mp and sub-includes.
    let inc_paths: Vec<PathBuf> = path.parent().map(|p| vec![p.to_path_buf()]).unwrap_or_default();
    let resolved = crate::analyzer::includes::collect_included_files(path, &inc_paths, &root.includes, 16, 1000);

    for inc_path in &resolved.paths {
        if let Some(entry) = resolved.files.get(inc_path) {
            root.symbols.extend(entry.parsed.symbols.clone());
            root.macro_names.extend(entry.parsed.macro_names.clone());
            root.func_macro_prefixes.extend(entry.parsed.func_macro_prefixes.clone());
        }
    }

    Some(root)
}
