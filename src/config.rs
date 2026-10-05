use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_host")]
    pub ollama_host: String,
    #[serde(default)]
    pub lms_path: String,
    /// 0 means total VRAM minus 2048 MiB.
    #[serde(default)]
    pub vram_budget_mib: u64,
    #[serde(default = "default_gpu_poll")]
    pub gpu_poll_ms: u64,
    #[serde(default = "default_model_poll")]
    pub model_poll_ms: u64,
}

fn default_host() -> String {
    "http://127.0.0.1:11434".into()
}

fn default_gpu_poll() -> u64 {
    1000
}

fn default_model_poll() -> u64 {
    2000
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ollama_host: default_host(),
            lms_path: String::new(),
            vram_budget_mib: 0,
            gpu_poll_ms: default_gpu_poll(),
            model_poll_ms: default_model_poll(),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        toml::from_str(&text).unwrap_or_default()
    }

    pub fn lms_bin(&self) -> PathBuf {
        if !self.lms_path.trim().is_empty() {
            return PathBuf::from(self.lms_path.trim());
        }
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/"))
            .join(".lmstudio/bin/lms")
    }

    pub fn host(&self) -> String {
        let host = self.ollama_host.trim();
        if host.is_empty() {
            return default_host();
        }
        if host.starts_with("http://") || host.starts_with("https://") {
            host.trim_end_matches('/').to_string()
        } else {
            format!("http://{}", host.trim_end_matches('/'))
        }
    }

    pub fn budget_mib(&self, total_mib: u64) -> u64 {
        if self.vram_budget_mib > 0 {
            self.vram_budget_mib.min(total_mib)
        } else {
            total_mib.saturating_sub(2048)
        }
    }
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("vramtui")
        .join("config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_budget_leaves_two_gib() {
        let cfg = Config::default();
        assert_eq!(cfg.budget_mib(16303), 16303 - 2048);
        let mut cfg = Config::default();
        cfg.vram_budget_mib = 12000;
        assert_eq!(cfg.budget_mib(16303), 12000);
    }

    #[test]
    fn host_gains_a_scheme() {
        let mut cfg = Config::default();
        cfg.ollama_host = "127.0.0.1:11434".into();
        assert_eq!(cfg.host(), "http://127.0.0.1:11434");
    }
}
