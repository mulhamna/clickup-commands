//! Config file (`~/.config/clickupc/config.toml`) with named profiles and
//! `CLICKUP_*` env overrides. Priority: env > selected profile > defaults.

use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{ClickUpError, Result};

pub const DEFAULT_BASE_URL: &str = "https://api.clickup.com/api";
pub const DEFAULT_PROFILE: &str = "default";
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ConfigFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_profile: Option<String>,
    #[serde(default)]
    pub profiles: BTreeMap<String, Profile>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_list: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_task_limit: Option<usize>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub aliases: BTreeMap<String, String>,
    /// Workspace id → token, for accounts whose tokens each reach a single
    /// workspace. `token` is the fallback for workspaces not listed here.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tokens: BTreeMap<String, String>,
}

impl Profile {
    /// Store `token` for `workspace_id` and make it the default. A legacy
    /// single token is kept under its own workspace so it keeps working.
    pub fn add_token(&mut self, workspace_id: &str, token: String) {
        if let (Some(old), Some(ws)) = (&self.token, &self.workspace_id) {
            self.tokens.entry(ws.clone()).or_insert_with(|| old.clone());
        }
        self.tokens.insert(workspace_id.to_string(), token.clone());
        self.token = Some(token);
        self.workspace_id = Some(workspace_id.to_string());
    }
}

fn env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn env_parse<T: std::str::FromStr>(key: &str) -> Result<Option<T>> {
    match env(key) {
        None => Ok(None),
        Some(v) => v
            .parse()
            .map(Some)
            .map_err(|_| ClickUpError::Config(format!("{key} has an invalid value '{v}'"))),
    }
}

impl ConfigFile {
    /// `$CLICKUP_CONFIG_DIR/config.toml`, else `<config dir>/clickupc/config.toml`.
    pub fn path() -> Result<PathBuf> {
        let dir = match env("CLICKUP_CONFIG_DIR") {
            Some(dir) => PathBuf::from(dir),
            None => dirs::config_dir()
                .ok_or_else(|| ClickUpError::Config("cannot locate a config directory".into()))?
                .join("clickupc"),
        };
        Ok(dir.join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        warn_if_world_readable(&path);
        let text = std::fs::read_to_string(&path)?;
        toml::from_str(&text).map_err(|e| ClickUpError::Config(format!("{}: {e}", path.display())))
    }

    pub fn save(&self) -> Result<PathBuf> {
        let path = Self::path()?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).map_err(|e| ClickUpError::Config(e.to_string()))?;
        std::fs::write(&path, text)?;
        restrict_permissions(&path)?;
        Ok(path)
    }

    /// `CLICKUP_PROFILE` > `current_profile` > `default`.
    pub fn selected_profile_name(&self) -> String {
        env("CLICKUP_PROFILE")
            .or_else(|| self.current_profile.clone())
            .unwrap_or_else(|| DEFAULT_PROFILE.to_string())
    }

    pub fn profile_mut(&mut self, name: &str) -> &mut Profile {
        self.profiles.entry(name.to_string()).or_default()
    }
}

/// `"Acme Corp (EU)"` → `"acme-corp-eu"`, for suggesting profile names.
pub fn profile_slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        DEFAULT_PROFILE.to_string()
    } else {
        out
    }
}

#[cfg(unix)]
fn restrict_permissions(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &std::path::Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn warn_if_world_readable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.permissions().mode() & 0o077 != 0 {
            tracing::warn!(
                "{} is readable by other users; run `chmod 600` on it",
                path.display()
            );
        }
    }
}

#[cfg(not(unix))]
fn warn_if_world_readable(_path: &std::path::Path) {}

/// Effective settings after merging the selected profile with env overrides.
#[derive(Debug, Clone)]
pub struct ClickUpConfig {
    pub profile: String,
    pub token: Option<String>,
    pub workspace_id: Option<String>,
    pub default_list: Option<String>,
    pub timezone: Option<String>,
    pub timeout_secs: u64,
    pub default_task_limit: Option<usize>,
    pub aliases: BTreeMap<String, String>,
    /// Per-workspace tokens of the profile; empty when `CLICKUP_TOKEN` is set.
    pub tokens: BTreeMap<String, String>,
    pub base_url: String,
    pub cache_dir: PathBuf,
    pub config_path: PathBuf,
}

impl ClickUpConfig {
    /// Load using the profile chosen by env / `current_profile`.
    pub fn load() -> Result<Self> {
        Self::load_profile(None)
    }

    /// Load an explicit profile (e.g. `--profile work`), falling back to the
    /// usual selection when `None`.
    pub fn load_profile(profile: Option<&str>) -> Result<Self> {
        let file = ConfigFile::load()?;
        Self::from_file(&file, profile)
    }

    pub fn from_file(file: &ConfigFile, profile: Option<&str>) -> Result<Self> {
        let name = profile
            .map(str::to_string)
            .unwrap_or_else(|| file.selected_profile_name());
        let p = file.profiles.get(&name).cloned().unwrap_or_default();
        let cache_dir = match env("CLICKUP_CACHE_DIR") {
            Some(dir) => PathBuf::from(dir),
            None => dirs::cache_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("clickupc"),
        };
        let env_token = env("CLICKUP_TOKEN");
        let tokens = if env_token.is_some() {
            BTreeMap::new()
        } else {
            p.tokens
        };
        let workspace_id = env("CLICKUP_WORKSPACE_ID").or(p.workspace_id);
        let token = env_token
            .or_else(|| workspace_id.as_ref().and_then(|w| tokens.get(w).cloned()))
            .or(p.token);
        Ok(Self {
            profile: name,
            token,
            workspace_id,
            tokens,
            default_list: env("CLICKUP_DEFAULT_LIST").or(p.default_list),
            timezone: env("CLICKUP_TIMEZONE").or(p.timezone),
            timeout_secs: env_parse("CLICKUP_TIMEOUT_SECS")?
                .or(p.timeout_secs)
                .unwrap_or(DEFAULT_TIMEOUT_SECS),
            default_task_limit: env_parse("CLICKUP_TASK_LIMIT")?.or(p.default_task_limit),
            aliases: p.aliases,
            base_url: env("CLICKUP_BASE_URL").unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
            cache_dir,
            config_path: ConfigFile::path()?,
        })
    }

    /// Minimal config for tests and embedding: token + base URL, no file.
    pub fn with_token(token: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            profile: DEFAULT_PROFILE.to_string(),
            token: Some(token.into()),
            workspace_id: None,
            default_list: None,
            timezone: None,
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            default_task_limit: None,
            aliases: BTreeMap::new(),
            tokens: BTreeMap::new(),
            base_url: base_url.into(),
            cache_dir: std::env::temp_dir().join("clickupc"),
            config_path: PathBuf::new(),
        }
    }

    pub fn token(&self) -> Result<&str> {
        self.token.as_deref().ok_or_else(|| {
            ClickUpError::Auth(
                "no API token configured; run `clickupc auth login` or set CLICKUP_TOKEN".into(),
            )
        })
    }

    pub fn workspace_id(&self) -> Result<&str> {
        self.workspace_id.as_deref().ok_or_else(|| {
            ClickUpError::Config(
                "no workspace selected; run `clickupc workspace use <name>` or set CLICKUP_WORKSPACE_ID"
                    .into(),
            )
        })
    }

    /// Switch to `workspace_id`, and to its own token when the profile has one.
    pub fn set_workspace(&mut self, workspace_id: impl Into<String>) {
        let id = workspace_id.into();
        if let Some(t) = self.tokens.get(&id) {
            self.token = Some(t.clone());
        }
        self.workspace_id = Some(id);
    }

    /// Every distinct token of the profile, the active one first.
    pub fn all_tokens(&self) -> Vec<String> {
        let mut out: Vec<String> = self.token.iter().cloned().collect();
        for t in self.tokens.values() {
            if !out.contains(t) {
                out.push(t.clone());
            }
        }
        out
    }

    /// `pk_12…AB` style preview that is safe to print.
    pub fn masked_token(&self) -> Option<String> {
        self.token.as_deref().map(mask_token)
    }
}

pub fn mask_token(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() <= 8 {
        return "***".into();
    }
    let head: String = chars[..5].iter().collect();
    let tail: String = chars[chars.len() - 2..].iter().collect();
    format!("{head}…{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    const VARS: &[&str] = &[
        "CLICKUP_CONFIG_DIR",
        "CLICKUP_TOKEN",
        "CLICKUP_WORKSPACE_ID",
        "CLICKUP_PROFILE",
        "CLICKUP_DEFAULT_LIST",
        "CLICKUP_TIMEOUT_SECS",
        "CLICKUP_TASK_LIMIT",
        "CLICKUP_BASE_URL",
        "CLICKUP_TIMEZONE",
    ];

    fn isolated() -> tempfile::TempDir {
        for v in VARS {
            std::env::remove_var(v);
        }
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("CLICKUP_CONFIG_DIR", dir.path());
        dir
    }

    #[test]
    #[serial]
    fn save_load_round_trip() {
        let _dir = isolated();
        let mut file = ConfigFile {
            current_profile: Some("work".into()),
            ..Default::default()
        };
        let p = file.profile_mut("work");
        p.token = Some("pk_abc".into());
        p.workspace_id = Some("901".into());
        p.aliases.insert("sprint".into(), "123".into());
        let _path = file.save().unwrap();
        assert_eq!(ConfigFile::load().unwrap(), file);

        #[cfg(unix)]
        {
            let path = _path;
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    #[serial]
    fn env_overrides_profile() {
        let _dir = isolated();
        let mut file = ConfigFile::default();
        file.profile_mut("default").token = Some("pk_file".into());
        file.profile_mut("default").workspace_id = Some("1".into());
        file.save().unwrap();

        std::env::set_var("CLICKUP_TOKEN", "pk_env");
        std::env::set_var("CLICKUP_TIMEOUT_SECS", "5");
        let cfg = ClickUpConfig::load().unwrap();
        assert_eq!(cfg.token.as_deref(), Some("pk_env"));
        assert_eq!(cfg.workspace_id.as_deref(), Some("1"));
        assert_eq!(cfg.timeout_secs, 5);
        assert_eq!(cfg.base_url, DEFAULT_BASE_URL);
        isolated();
    }

    #[test]
    #[serial]
    fn profile_selection_priority() {
        let _dir = isolated();
        let mut file = ConfigFile {
            current_profile: Some("a".into()),
            ..Default::default()
        };
        file.profile_mut("a").workspace_id = Some("A".into());
        file.profile_mut("b").workspace_id = Some("B".into());

        assert_eq!(
            ClickUpConfig::from_file(&file, None).unwrap().workspace_id,
            Some("A".into())
        );
        std::env::set_var("CLICKUP_PROFILE", "b");
        assert_eq!(
            ClickUpConfig::from_file(&file, None).unwrap().workspace_id,
            Some("B".into())
        );
        assert_eq!(
            ClickUpConfig::from_file(&file, Some("a"))
                .unwrap()
                .workspace_id,
            Some("A".into())
        );
        isolated();
    }

    #[test]
    #[serial]
    fn missing_token_is_auth_error() {
        let _dir = isolated();
        let cfg = ClickUpConfig::load().unwrap();
        assert!(matches!(cfg.token(), Err(ClickUpError::Auth(_))));
        assert!(matches!(cfg.workspace_id(), Err(ClickUpError::Config(_))));
    }

    #[test]
    #[serial]
    fn per_workspace_tokens() {
        let _dir = isolated();
        let mut file = ConfigFile::default();
        let p = file.profile_mut("default");
        p.token = Some("pk_one".into());
        p.workspace_id = Some("1".into());
        p.add_token("2", "pk_two".into());
        assert_eq!(p.tokens.get("1").map(String::as_str), Some("pk_one"));
        assert_eq!(p.token.as_deref(), Some("pk_two"));

        let mut cfg = ClickUpConfig::from_file(&file, None).unwrap();
        assert_eq!(cfg.token.as_deref(), Some("pk_two"));
        assert_eq!(cfg.all_tokens(), vec!["pk_two", "pk_one"]);
        cfg.set_workspace("1");
        assert_eq!(cfg.token.as_deref(), Some("pk_one"));
        cfg.set_workspace("3");
        assert_eq!(cfg.token.as_deref(), Some("pk_one"));

        std::env::set_var("CLICKUP_TOKEN", "pk_env");
        let mut cfg = ClickUpConfig::from_file(&file, None).unwrap();
        cfg.set_workspace("1");
        assert_eq!(cfg.token.as_deref(), Some("pk_env"));
        isolated();
    }

    #[test]
    fn slugs() {
        assert_eq!(profile_slug("Acme Corp (EU)"), "acme-corp-eu");
        assert_eq!(profile_slug("  Side Project  "), "side-project");
        assert_eq!(profile_slug("!!!"), "default");
    }

    #[test]
    fn masks_token() {
        assert_eq!(mask_token("pk_1234567890AB"), "pk_12…AB");
        assert_eq!(mask_token("short"), "***");
    }
}
