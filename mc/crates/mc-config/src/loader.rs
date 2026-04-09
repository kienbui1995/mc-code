use std::path::{Path, PathBuf};

use crate::types::{ConfigError, ConfigLayer, RuntimeConfig};

#[allow(clippy::struct_field_names)]
/// Configloader.
pub struct ConfigLoader {
    global_path: PathBuf,
    project_path: PathBuf,
    local_path: PathBuf,
}

impl ConfigLoader {
    #[must_use]
    /// New.
    pub fn new(cwd: &Path) -> Self {
        let global_dir = dirs_global_config();
        Self {
            global_path: global_dir.join("config.toml"),
            project_path: cwd.join(".magic-code").join("config.toml"),
            local_path: cwd.join(".magic-code").join("config.local.toml"),
        }
    }

    /// Load.
    pub fn load(&self) -> Result<RuntimeConfig, ConfigError> {
        let mut layers = Vec::new();
        for path in [&self.global_path, &self.project_path, &self.local_path] {
            if let Some(layer) = read_optional_config(path)? {
                layers.push(layer);
            }
        }
        Ok(RuntimeConfig::from_layers(&layers))
    }
}

fn dirs_global_config() -> PathBuf {
    std::env::var_os("MAGIC_CODE_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_CONFIG_HOME").map(|p| PathBuf::from(p).join("magic-code"))
        })
        .or_else(|| {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("magic-code"))
        })
        .unwrap_or_else(|| PathBuf::from(".config").join("magic-code"))
}

/// Load a single config layer from a TOML file.
pub fn load_layer(path: &Path) -> Result<ConfigLayer, ConfigError> {
    let contents = std::fs::read_to_string(path).map_err(ConfigError::Io)?;
    Ok(toml::from_str(&contents)?)
}

/// Check if config file has been modified since last load.
/// Returns the new modification time if changed, None if unchanged.
pub fn config_changed(
    path: &std::path::Path,
    last_mtime: &std::time::SystemTime,
) -> Option<std::time::SystemTime> {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .filter(|mtime| mtime > last_mtime)
}

/// Reload config if the file has changed. Returns Some(new_config) if reloaded.
pub fn maybe_reload(
    path: &std::path::Path,
    last_mtime: &mut std::time::SystemTime,
) -> Option<super::types::ConfigLayer> {
    let new_mtime = config_changed(path, last_mtime)?;
    let config = load_layer(path).ok()?;
    *last_mtime = new_mtime;
    Some(config)
}

fn read_optional_config(path: &Path) -> Result<Option<ConfigLayer>, ConfigError> {
    match std::fs::read_to_string(path) {
        Ok(contents) if contents.trim().is_empty() => Ok(None),
        Ok(contents) => Ok(Some(toml::from_str(&contents)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(ConfigError::Io(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mc-cfg-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn loads_with_no_config_files() {
        let dir = temp_dir("empty");
        let config = ConfigLoader::new(&dir).load().unwrap();
        assert_eq!(config.provider, "anthropic");
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn project_overrides_defaults() {
        let dir = temp_dir("override");
        let mc = dir.join(".magic-code");
        fs::create_dir_all(&mc).unwrap();
        fs::write(
            mc.join("config.toml"),
            "[default]\nmodel = \"claude-opus-4-20250514\"\n",
        )
        .unwrap();
        let config = ConfigLoader::new(&dir).load().unwrap();
        assert_eq!(config.model, "claude-opus-4-20250514");
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn local_highest_priority() {
        let dir = temp_dir("priority");
        let mc = dir.join(".magic-code");
        fs::create_dir_all(&mc).unwrap();
        fs::write(mc.join("config.toml"), "[default]\nmodel = \"opus\"\n").unwrap();
        fs::write(
            mc.join("config.local.toml"),
            "[default]\nmodel = \"haiku\"\n",
        )
        .unwrap();
        let config = ConfigLoader::new(&dir).load().unwrap();
        assert_eq!(config.model, "haiku");
        fs::remove_dir_all(dir).ok();
    }
}
