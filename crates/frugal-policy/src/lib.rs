//! Local policy: optimization profile + budgets, stored at
//! `~/.frugal/config.json` (shared with the Python runtime).

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

pub const PROFILES: [&str; 5] = ["shadow", "conservative", "balanced", "aggressive", "off"];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Budgets {
    pub task_usd: Option<f64>,
    pub session_usd: Option<f64>,
    pub daily_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_profile")]
    pub profile: String,
    #[serde(default)]
    pub budgets: Budgets,
}

fn default_profile() -> String {
    "shadow".into()
}

impl Default for Config {
    fn default() -> Self {
        Config {
            profile: default_profile(),
            budgets: Budgets::default(),
        }
    }
}

pub fn load() -> Config {
    let path = frugal_storage::frugal_dir().join("config.json");
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(cfg: &Config) -> Result<()> {
    let dir = frugal_storage::frugal_dir();
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("config.json"), serde_json::to_string_pretty(cfg)?)?;
    Ok(())
}

pub fn set_profile(name: &str) -> Result<Config> {
    if !PROFILES.contains(&name) {
        bail!("profile must be one of {:?}", PROFILES);
    }
    let mut cfg = load();
    cfg.profile = name.to_string();
    save(&cfg)?;
    Ok(cfg)
}

pub fn set_budget(scope: &str, usd: Option<f64>) -> Result<Config> {
    let mut cfg = load();
    match scope {
        "task" => cfg.budgets.task_usd = usd,
        "session" => cfg.budgets.session_usd = usd,
        "daily" => cfg.budgets.daily_usd = usd,
        _ => bail!("scope must be task|session|daily"),
    }
    save(&cfg)?;
    Ok(cfg)
}

/// Budget health for a spend against an optional limit: '✓', '!', or 'X'.
pub fn budget_health(spend: Option<f64>, limit: Option<f64>) -> char {
    match (spend, limit) {
        (Some(s), Some(l)) if l > 0.0 && s >= l => 'X',
        (Some(s), Some(l)) if l > 0.0 && s >= 0.8 * l => '!',
        _ => '✓',
    }
}
