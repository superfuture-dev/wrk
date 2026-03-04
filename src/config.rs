use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

const DEFAULT_LOG_DIR_NAME: &str = "wrk";
const DEFAULT_TYPE: &str = "note";

#[derive(Debug, Clone)]
pub struct Config {
    pub log_dir: PathBuf,
    pub default_project: Option<String>,
    pub default_type: String,
    pub types: Vec<String>,
    pub editor: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    log_dir: Option<String>,
    default_project: Option<String>,
    default_type: Option<String>,
    types: Option<Vec<String>>,
    editor: Option<String>,
}

impl Config {
    pub fn load(
        config_override: Option<PathBuf>,
        log_dir_override: Option<PathBuf>,
    ) -> Result<Self> {
        let config_path = config_override.unwrap_or_else(default_config_path);

        let file_config = if config_path.exists() {
            let raw = fs::read_to_string(&config_path)
                .with_context(|| format!("failed to read {}", config_path.display()))?;
            toml::from_str::<FileConfig>(&raw)
                .with_context(|| format!("failed to parse {}", config_path.display()))?
        } else {
            FileConfig::default()
        };

        let log_dir = match log_dir_override {
            Some(path) => path,
            None => file_config
                .log_dir
                .as_deref()
                .map(expand_tilde)
                .transpose()?
                .unwrap_or_else(default_log_dir),
        };

        let default_project = file_config
            .default_project
            .filter(|value| !value.is_empty());
        let default_type = file_config
            .default_type
            .unwrap_or_else(|| DEFAULT_TYPE.to_owned());
        let mut types = file_config
            .types
            .unwrap_or_else(|| vec![default_type.clone()]);

        validate_project(default_project.as_deref())?;
        validate_kind(&default_type)?;

        if !types.iter().any(|kind| kind == &default_type) {
            types.push(default_type.clone());
        }

        for kind in &types {
            validate_kind(kind)?;
        }

        Ok(Self {
            log_dir,
            default_project,
            default_type,
            types,
            editor: file_config.editor.or_else(resolve_editor_from_env),
        })
    }

    pub fn resolve_project(&self, project: Option<&str>) -> Result<Option<String>> {
        let value = project
            .map(ToOwned::to_owned)
            .or_else(|| self.default_project.clone());
        validate_project(value.as_deref())?;
        Ok(value)
    }

    pub fn resolve_kind(&self, kind: Option<&str>) -> Result<String> {
        let resolved = kind.unwrap_or(&self.default_type).to_owned();
        validate_kind(&resolved)?;
        if !self.types.iter().any(|allowed| allowed == &resolved) {
            bail!(
                "unknown type `{resolved}`; allowed types: {}",
                self.types.join(", ")
            );
        }
        Ok(resolved)
    }
}

pub fn default_config_path() -> PathBuf {
    home_dir().join(".wrkrc")
}

fn default_log_dir() -> PathBuf {
    home_dir().join(DEFAULT_LOG_DIR_NAME)
}

fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"))
}

fn resolve_editor_from_env() -> Option<String> {
    env::var("VISUAL")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| env::var("EDITOR").ok().filter(|value| !value.is_empty()))
}

fn expand_tilde(raw: &str) -> Result<PathBuf> {
    if raw == "~" {
        return Ok(home_dir());
    }
    if let Some(path) = raw.strip_prefix("~/") {
        return Ok(home_dir().join(path));
    }
    Ok(Path::new(raw).to_path_buf())
}

fn validate_project(project: Option<&str>) -> Result<()> {
    if let Some(project) = project {
        validate_token("project", project)?;
    }
    Ok(())
}

fn validate_kind(kind: &str) -> Result<()> {
    validate_token("type", kind)
}

fn validate_token(label: &str, value: &str) -> Result<()> {
    if value.is_empty() {
        bail!("{label} cannot be empty");
    }
    if value.contains(':') || value.chars().any(char::is_whitespace) {
        bail!("{label} `{value}` cannot contain whitespace or `:`");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_tilde_prefix() {
        let path = expand_tilde("~/logs").unwrap();
        assert!(path.ends_with("logs"));
    }

    #[test]
    fn rejects_bad_type_tokens() {
        let err = validate_kind("status update").unwrap_err();
        assert!(err.to_string().contains("cannot contain whitespace"));
    }
}
