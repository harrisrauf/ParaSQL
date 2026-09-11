use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceTable {
    pub name: String,
    /// Stored path: relative to the workspace file's directory when possible,
    /// otherwise absolute. Never relies on a machine-specific location.
    pub path: String,
    /// "file" | "folder"
    pub source: String,
    /// "query" | "editable"
    pub mode: String,
    pub compression: Option<String>,
    pub size: Option<u64>,
    pub mtime: Option<u64>,
    /// Resolved absolute path (filled by the backend; kept for round-trips)
    pub abs_path: Option<String>,
    pub missing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedQuery {
    pub name: String,
    pub sql: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Workspace {
    pub version: u32,
    pub name: String,
    pub tables: Vec<WorkspaceTable>,
    pub saved_queries: Vec<SavedQuery>,
    pub charts: Vec<serde_json::Value>,
    pub dashboards: Vec<serde_json::Value>,
    pub notebooks: Vec<serde_json::Value>,
    /// Tables larger than this (MB) require explicit confirmation to open for editing
    pub edit_size_limit_mb: Option<u64>,
    /// Directory of the workspace file (not persisted — derived on load)
    pub dir: Option<String>,
}

#[allow(dead_code)]
impl Workspace {
    pub fn new(name: &str) -> Self {
        Self {
            version: 1,
            name: name.to_string(),
            tables: Vec::new(),
            saved_queries: Vec::new(),
            charts: Vec::new(),
            dashboards: Vec::new(),
            notebooks: Vec::new(),
            edit_size_limit_mb: Some(2048),
            dir: None,
        }
    }

    pub fn table(&self, name: &str) -> Option<&WorkspaceTable> {
        self.tables.iter().find(|t| t.name == name)
    }

    pub fn remove_table(&mut self, name: &str) {
        self.tables.retain(|t| t.name != name);
    }

    pub fn add_table(&mut self, name: &str, path: &str, source: &str, mode: &str) {
        self.tables.push(WorkspaceTable {
            name: name.to_string(),
            path: path.to_string(),
            source: source.to_string(),
            mode: mode.to_string(),
            compression: None,
            size: None,
            mtime: None,
            abs_path: Some(path.to_string()),
            missing: false,
        });
    }

    /// Pick `base`, `base_2`, `base_3`, ... until unused
    pub fn unique_name(&self, base: &str) -> String {
        if self.table(base).is_none() {
            return base.to_string();
        }
        let mut i = 2;
        loop {
            let candidate = format!("{}_{}", base, i);
            if self.table(&candidate).is_none() {
                return candidate;
            }
            i += 1;
        }
    }
}

fn to_rel(dir: &Path, abs: &Path) -> Option<String> {
    let rel = abs.strip_prefix(dir).ok()?;
    Some(rel.to_string_lossy().replace('\\', "/"))
}

fn resolve(dir: &Path, stored: &str) -> PathBuf {
    let p = Path::new(stored);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        dir.join(p)
    }
}

fn stat(path: &Path) -> (bool, Option<u64>, Option<u64>) {
    match std::fs::metadata(path) {
        Ok(m) => (
            true,
            Some(m.len()),
            m.modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs()),
        ),
        Err(_) => (false, None, None),
    }
}

impl Workspace {
    /// Convert stored (possibly relative) paths to absolute against `dir`
    /// and refresh existence/size/mtime.
    pub fn resolve_paths(&mut self) {
        let dir = self.dir.as_ref().map(PathBuf::from);
        for t in &mut self.tables {
            let abs = match &dir {
                Some(d) => resolve(d, &t.path),
                None => PathBuf::from(&t.path),
            };
            t.abs_path = Some(abs.to_string_lossy().to_string());
            let (exists, size, mtime) = stat(&abs);
            t.missing = !exists;
            t.size = size;
            t.mtime = mtime;
        }
    }

    /// Convert absolute paths that live under `dir` back to relative, so the
    /// workspace stays portable.
    pub fn relativize_paths(&mut self) {
        if let Some(dir) = self.dir.as_deref() {
            let dir = PathBuf::from(dir);
            for t in &mut self.tables {
                let abs = PathBuf::from(t.abs_path.as_deref().unwrap_or(&t.path));
                if let Some(rel) = to_rel(&dir, &abs) {
                    t.path = rel;
                }
            }
        }
    }
}

pub fn load_workspace(path: &str) -> Result<Workspace, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read workspace: {}", e))?;
    let mut ws: Workspace =
        serde_json::from_str(&raw).map_err(|e| format!("Invalid workspace file: {}", e))?;
    ws.dir = Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().to_string());
    ws.resolve_paths();
    Ok(ws)
}

pub fn write_workspace(path: &str, ws: &Workspace) -> Result<(), String> {
    let mut doc = ws.clone();
    doc.dir = Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().to_string());
    doc.relativize_paths();
    let raw = serde_json::to_string_pretty(&doc)
        .map_err(|e| format!("Failed to serialize workspace: {}", e))?;
    // Write to a sibling temp file and atomically replace, so an interrupted
    // save can never leave a truncated .parasql behind.
    let tmp = format!("{}.tmp-{}", path, std::process::id());
    if let Err(e) = std::fs::write(&tmp, raw) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("Failed to write workspace: {}", e));
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("Failed to replace workspace: {}", e)
    })
}