//! Profiles, filesystem layout and `config.toml`.
//!
//! Two profiles never share files: `release` is the daily-driver install, `dev` is what
//! `cargo xtask run` launches while developing.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// macOS `sun_path` is 104 bytes including the trailing NUL.
const MAX_SOCKET_PATH: usize = 103;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    Release,
    Dev,
}

impl Profile {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "release" => Some(Self::Release),
            "dev" => Some(Self::Dev),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Release => "release",
            Self::Dev => "dev",
        }
    }

    /// Directory name used under `Application Support` and `Logs`.
    fn dir_name(self) -> &'static str {
        match self {
            Self::Release => "Navori",
            Self::Dev => "Navori Dev",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub data_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub backups_dir: PathBuf,
    pub config_file: PathBuf,
    pub db_file: PathBuf,
    pub socket: PathBuf,
}

impl Paths {
    /// Pure computation; creates nothing on disk.
    pub fn new(profile: Profile, home: &Path, uid: u32) -> Self {
        let data_dir = home
            .join("Library/Application Support")
            .join(profile.dir_name());
        let logs_dir = home.join("Library/Logs").join(profile.dir_name());

        let preferred = data_dir.join("run/navori.sock");
        let socket = if preferred.as_os_str().len() <= MAX_SOCKET_PATH {
            preferred
        } else {
            PathBuf::from(format!("/tmp/navori-{uid}/{}.sock", profile.as_str()))
        };

        Self {
            backups_dir: data_dir.join("backups"),
            config_file: data_dir.join("config.toml"),
            db_file: data_dir.join("navori.db"),
            socket,
            logs_dir,
            data_dir,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// One of `error`, `warn`, `info`, `debug`, `trace`.
    pub log_level: String,
    pub journal: JournalConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct JournalConfig {
    /// Events older than this are pruned. Must be ≥ 1.
    pub retention_days: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            log_level: "info".to_owned(),
            journal: JournalConfig::default(),
        }
    }
}

impl Default for JournalConfig {
    fn default() -> Self {
        Self { retention_days: 30 }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid config {path}: {message}")]
    Invalid { path: PathBuf, message: String },
}

const LOG_LEVELS: &[&str] = &["error", "warn", "info", "debug", "trace"];

impl Config {
    /// Loads `path`. A missing file yields defaults; unknown keys are ignored so older
    /// builds can read configs written by newer ones.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let raw = match std::fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(ConfigError::Io {
                    path: path.to_owned(),
                    source,
                });
            }
        };
        let invalid = |message: String| ConfigError::Invalid {
            path: path.to_owned(),
            message,
        };

        let cfg: Self = toml::from_str(&raw).map_err(|e| invalid(e.to_string()))?;
        if !LOG_LEVELS.contains(&cfg.log_level.as_str()) {
            return Err(invalid(format!(
                "log_level must be one of {}, got {:?}",
                LOG_LEVELS.join("/"),
                cfg.log_level
            )));
        }
        if cfg.journal.retention_days == 0 {
            return Err(invalid(
                "journal.retention_days must be at least 1".to_owned(),
            ));
        }
        Ok(cfg)
    }
}
