use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use dashmap::DashMap;

use crate::analyzer::PawnDiagnostic;
use crate::analyzer::{deprecated, hints, includes, indentation, semantic, undefined, unused};
use crate::analyzer::includes::collect_included_files;
use crate::config::EngineConfig;
use crate::messages::Locale;
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
    pub locale: Locale,
    pub include_paths_override: Option<Vec<PathBuf>>,
    pub open_docs: DashMap<String, Document>,
    pub parsed_cache: DashMap<PathBuf, Arc<ParsedFile>>,
    /// Maps each include path to the set of file URIs that depend on it.
    /// Used for granular cache invalidation: when a single include changes,
    /// only dependents are evicted rather than the entire cache.
    pub dep_graph: DashMap<PathBuf, HashSet<String>>,
    // tabsize is compiler-global — a single `#pragma tabsize N` in any included file
    // affects all files compiled after it, so we cache the value workspace-wide.
    pub tabsize_cache: Mutex<Option<Option<u32>>>,
    pub sdk_file: Option<PathBuf>,
    pub sdk_parsed: Option<ParsedFile>,
}

impl WorkspaceState {
    pub fn new() -> Self {
        Self {
            workspace_root: None,
            config: EngineConfig::default(),
            locale: Locale::default(),
            include_paths_override: None,
            open_docs: DashMap::new(),
            parsed_cache: DashMap::new(),
            dep_graph: DashMap::new(),
            tabsize_cache: Mutex::new(None),
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
            None => {
                self.sdk_file = None;
                self.sdk_parsed = None;
            }
        }
    }

    pub fn set_workspace_root(&mut self, root: PathBuf) {
        self.config = EngineConfig::load(Some(&root));
        self.workspace_root = Some(root);
        self.invalidate_tabsize_cache();
    }

    pub fn include_paths(&self) -> Vec<PathBuf> {
        self.include_paths_override
            .as_deref()
            .map(|paths| paths.to_vec())
            .unwrap_or_else(|| self.config.resolved_include_paths(self.workspace_root.as_deref()))
    }

    pub fn invalidate_tabsize_cache(&self) {
        *self.tabsize_cache.lock().unwrap() = None;
    }

    pub fn open_document(&self, uri: String, text: String, version: i32) {
        self.evict_uri_from_cache(&uri);
        self.open_docs.insert(uri, Document { text, version });
    }

    pub fn change_document(&self, uri: &str, text: String, version: i32) {
        self.evict_uri_from_cache(uri);

        if let Some(mut doc) = self.open_docs.get_mut(uri) {
            doc.text = text;
            doc.version = version;
        } else {
            self.open_docs.insert(uri.to_string(), Document { text, version });
        }

        // If an include file changed, also evict every file that depends on it.
        if let Some(path) = uri_to_path(uri) {
            self.evict_dependents(&path);
        }
    }

    pub fn close_document(&self, uri: &str) {
        self.open_docs.remove(uri);
        self.evict_uri_from_cache(uri);
    }

    pub fn get_text(&self, uri: &str) -> Option<String> {
        if let Some(doc) = self.open_docs.get(uri) {
            return Some(doc.text.clone());
        }
        let path = uri_to_path(uri)?;
        std::fs::read(&path).ok().map(|b| decode_bytes(&b))
    }

    pub fn get_parsed(&self, uri: &str) -> Option<Arc<ParsedFile>> {
        let path = uri_to_path(uri)?;
        if let Some(cached) = self.parsed_cache.get(&path) {
            return Some(Arc::clone(cached.value()));
        }
        let text = self.get_text(uri)?;
        let parsed = Arc::new(parse_file(&text));
        self.parsed_cache.insert(path, Arc::clone(&parsed));
        Some(parsed)
    }

    pub fn get_parsed_by_path(&self, path: &Path) -> Option<Arc<ParsedFile>> {
        if let Some(cached) = self.parsed_cache.get(path) {
            return Some(Arc::clone(cached.value()));
        }
        let bytes = std::fs::read(path).ok()?;
        let text = decode_bytes(&bytes);
        let parsed = Arc::new(parse_file(&text));
        self.parsed_cache.insert(path.to_path_buf(), Arc::clone(&parsed));
        Some(parsed)
    }

    pub fn analyze(&self, uri: &str) -> Vec<PawnDiagnostic> {
        let Some(text) = self.get_text(uri) else { return vec![] };
        let Some(file_path) = uri_to_path(uri) else { return vec![] };

        if self.config.analysis.suppress_diagnostics_in_inc && is_include_file(&file_path) {
            return vec![];
        }

        let parsed = Arc::new(parse_file(&text));
        let inc_paths = self.include_paths();
        let resolved = collect_included_files(&file_path, &inc_paths, &parsed.includes, 16, 1000);

        self.record_dependencies(uri, &resolved.reverse_deps);

        let locale = self.locale;
        let inc_texts: Vec<&str> = resolved.files.values().map(|e| e.text.as_str()).collect();
        let global_tabsize = self.cached_tabsize(&inc_paths);

        let mut diags = Vec::new();
        diags.extend(includes::analyze_includes(&parsed.includes, &file_path, &inc_paths, self.workspace_root.as_deref(), locale));
        diags.extend(semantic::analyze_semantics(&text, locale));
        diags.extend(unused::analyze_unused(
            &text, &file_path, &parsed, &resolved,
            self.config.analysis.warn_unused_in_inc,
            self.workspace_root.as_deref(),
            locale,
        ));
        diags.extend(deprecated::analyze_deprecated(&text, &file_path, &parsed, &inc_paths, &resolved, locale));
        diags.extend(hints::analyze_hints(&text, &parsed.symbols, locale));
        diags.extend(undefined::analyze_undefined(&text, &file_path, &parsed, &resolved, self.sdk_parsed.as_ref(), locale));
        diags.extend(indentation::analyze_indentation(&text, &inc_texts, global_tabsize, locale));

        self.parsed_cache.insert(file_path, parsed);

        diags
    }

    // --- private helpers ---

    fn evict_uri_from_cache(&self, uri: &str) {
        if let Some(path) = uri_to_path(uri) {
            self.parsed_cache.remove(&path);
        }
    }

    fn evict_dependents(&self, changed_include: &Path) {
        let canon = changed_include.canonicalize().unwrap_or_else(|_| changed_include.to_path_buf());
        if let Some(dependents) = self.dep_graph.get(&canon) {
            for uri in dependents.value() {
                self.evict_uri_from_cache(uri);
            }
        }
    }

    fn record_dependencies(
        &self,
        dependent_uri: &str,
        reverse_deps: &std::collections::HashMap<PathBuf, HashSet<PathBuf>>,
    ) {
        for include_path in reverse_deps.keys() {
            self.dep_graph
                .entry(include_path.clone())
                .or_default()
                .insert(dependent_uri.to_string());
        }
    }

    fn cached_tabsize(&self, inc_paths: &[PathBuf]) -> Option<u32> {
        let mut cache = self.tabsize_cache.lock().unwrap();
        *cache.get_or_insert_with(|| find_global_tabsize(inc_paths))
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
    if p.components().any(|c| c == std::path::Component::ParentDir) {
        return None;
    }
    Some(p)
}

fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
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

fn is_include_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("inc") | Some("p") | Some("pawn")
    )
}

// tabsize is compiler-global — an include that defines tabsize=4 affects all files
// compiled after it. We scan include dirs once and cache the result.
fn find_global_tabsize(inc_paths: &[PathBuf]) -> Option<u32> {
    let mut result = None;
    for dir in inc_paths {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !matches!(ext, "inc" | "p" | "pwn") {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else { continue };
            let text = decode_bytes(&bytes);
            for line in text.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("#pragma")
                    && let Some(rest) = rest.trim().strip_prefix("tabsize")
                    && let Ok(n) = rest.trim().parse::<u32>()
                {
                    result = Some(n);
                }
            }
        }
    }
    result
}

fn parse_sdk(path: &PathBuf) -> Option<ParsedFile> {
    let bytes = std::fs::read(path).ok()?;
    let text = decode_bytes(&bytes);
    let mut root = parse_file(&text);

    // open.mp.inc itself has almost no symbols — they live in _open_mp and sub-includes.
    // Resolve transitively so all SDK symbols are visible.
    let inc_paths: Vec<PathBuf> = path.parent().map(|p| vec![p.to_path_buf()]).unwrap_or_default();
    let resolved = collect_included_files(path, &inc_paths, &root.includes, 16, 1000);

    for inc_path in &resolved.paths {
        if let Some(entry) = resolved.files.get(inc_path) {
            root.symbols.extend(entry.parsed.symbols.clone());
            root.macro_names.extend(entry.parsed.macro_names.clone());
            root.func_macro_prefixes.extend(entry.parsed.func_macro_prefixes.clone());
        }
    }

    Some(root)
}
