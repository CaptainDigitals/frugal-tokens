//! Provider framework: JSON manifests describing optional optimizers
//! (Graphify, token-compact, token-saver, DeepWiki, ...) with capabilities,
//! trust classes, and installation detection.
//!
//! Manifest layers (later overrides earlier by id):
//!   built-ins -> `<repo>/providers/*.json` bundled -> `~/.frugal/providers/*.json`

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub trust: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub conflicts: Vec<String>,
    #[serde(default)]
    pub priority: i64,
    #[serde(default)]
    pub detect: BTreeMap<String, Value>,
    #[serde(default)]
    pub install: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub fixed_context_tax_tokens: i64,
}

pub const EXCLUSIVE_CAPABILITIES: [&str; 4] = [
    "compression.document",
    "compression.output",
    "request_proxy",
    "compaction.manager",
];

fn builtin() -> Vec<Provider> {
    let raw = include_str!("builtin_providers.json");
    serde_json::from_str(raw).unwrap_or_default()
}

fn load_dir(dir: &Path) -> Vec<Provider> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(p) = serde_json::from_str::<Provider>(&text) {
                out.push(p);
            }
        }
    }
    out
}

pub fn registry() -> Vec<Provider> {
    let mut merged: BTreeMap<String, Provider> = BTreeMap::new();
    for p in builtin() {
        merged.insert(p.id.clone(), p);
    }
    for p in load_dir(&frugal_storage::frugal_dir().join("providers")) {
        merged.insert(p.id.clone(), p);
    }
    let mut list: Vec<Provider> = merged.into_values().collect();
    list.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.id.cmp(&b.id)));
    list
}

fn which(cmd: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
            .split(';')
            .map(|s| s.to_lowercase())
            .collect()
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&paths) {
        for ext in &exts {
            if dir.join(format!("{cmd}{ext}")).is_file() {
                return true;
            }
        }
    }
    false
}

fn skill_installed(skill: &str) -> bool {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".claude").join("skills").join(skill).is_dir()
        || Path::new(".claude").join("skills").join(skill).is_dir()
}

pub fn installed(provider: &Provider) -> bool {
    if let Some(Value::String(cmd)) = provider.detect.get("which") {
        return which(cmd);
    }
    if let Some(Value::String(skill)) = provider.detect.get("skill") {
        return skill_installed(skill);
    }
    if let Some(Value::String(name)) = provider.detect.get("mcp") {
        return mcp_configured(name);
    }
    false
}

fn mcp_configured(server: &str) -> bool {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    for path in [PathBuf::from(".mcp.json"), home.join(".claude.json")] {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if value["mcpServers"].get(server).is_some() {
            return true;
        }
        if let Some(projects) = value["projects"].as_object() {
            if projects
                .values()
                .any(|p| p["mcpServers"].get(server).is_some())
            {
                return true;
            }
        }
    }
    false
}

/// Active-set validation: one provider per exclusive capability, priority wins.
pub fn resolve(enabled: &[&Provider]) -> (Vec<String>, Vec<(String, String)>) {
    let mut sorted: Vec<&Provider> = enabled.to_vec();
    sorted.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.id.cmp(&b.id)));
    let mut active: Vec<&Provider> = Vec::new();
    let mut dropped: Vec<(String, String)> = Vec::new();
    'outer: for p in sorted {
        if p.trust == "BLOCKED" {
            dropped.push((p.id.clone(), "trust class BLOCKED".into()));
            continue;
        }
        for other in &active {
            if p.conflicts.contains(&other.id) || other.conflicts.contains(&p.id) {
                dropped.push((p.id.clone(), format!("explicit conflict with {}", other.id)));
                continue 'outer;
            }
            for cap in &p.capabilities {
                if EXCLUSIVE_CAPABILITIES.contains(&cap.as_str())
                    && other.capabilities.contains(cap)
                {
                    dropped.push((
                        p.id.clone(),
                        format!(
                            "exclusive capability {cap} already provided by {}",
                            other.id
                        ),
                    ));
                    continue 'outer;
                }
            }
        }
        active.push(p);
    }
    (active.iter().map(|p| p.id.clone()).collect(), dropped)
}

pub fn find_result(id: &str) -> Result<Provider> {
    registry()
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| anyhow::anyhow!("unknown provider {id:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(id: &str, caps: &[&str], priority: i64) -> Provider {
        Provider {
            id: id.into(),
            name: id.into(),
            trust: "COMMUNITY".into(),
            capabilities: caps.iter().map(|c| c.to_string()).collect(),
            conflicts: vec![],
            priority,
            detect: BTreeMap::new(),
            install: String::new(),
            repo: String::new(),
            fixed_context_tax_tokens: 0,
        }
    }

    #[test]
    fn exclusive_capability_drops_lower_priority() {
        let a = provider("a", &["compression.document"], 90);
        let b = provider("b", &["compression.document"], 60);
        let c = provider("c", &["navigation.ast"], 50);
        let (active, dropped) = resolve(&[&a, &b, &c]);
        assert_eq!(active, vec!["a", "c"]);
        assert_eq!(dropped.len(), 1);
        assert_eq!(dropped[0].0, "b");
    }

    #[test]
    fn builtin_registry_parses() {
        assert!(registry().iter().any(|p| p.id == "graphify"));
    }
}
