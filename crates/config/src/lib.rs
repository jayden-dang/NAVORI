//! Profiles, filesystem layout and `config.toml`.
//!
//! Two profiles never share files: `release` is the daily-driver install, `dev` is what
//! `cargo xtask run` launches while developing.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

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

const LOG_LEVELS: &[&str] = &["error", "warn", "info", "debug", "trace"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JournalSweep {
    KeepDays(u32),
    Paused,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigReport {
    Clean,
    BadValues(Vec<String>),
    WholeFile(WholeFileReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WholeFileReason {
    Unreadable,
    NotToml { position: Option<(usize, usize)> },
}

pub struct ConfigLoad {
    pub config: Config,
    pub sweep: JournalSweep,
    pub sections: Map<String, Value>,
    pub report: ConfigReport,
}

impl std::fmt::Debug for ConfigLoad {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let section_keys: Vec<&String> = self.sections.keys().collect();
        f.debug_struct("ConfigLoad")
            .field("config", &self.config)
            .field("sweep", &self.sweep)
            .field("section_count", &self.sections.len())
            .field("section_keys", &section_keys)
            .field("report", &self.report)
            .finish()
    }
}

impl Config {
    /// Loads `path`. A missing file yields defaults. One bad value falls back on its own.
    /// Other top-level keys are returned as JSON and are not an error.
    pub fn load(path: &Path) -> ConfigLoad {
        let raw = match std::fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return clean_defaults(),
            Err(_) => return whole_file(WholeFileReason::Unreadable),
        };

        let table = match toml::from_str::<toml::Table>(&raw) {
            Ok(table) => table,
            Err(error) => {
                let position = error.span().and_then(|span| line_column(&raw, span.start));
                return whole_file(WholeFileReason::NotToml { position });
            }
        };

        let mut config = Config::default();
        let mut bad = Vec::new();
        if let Some(value) = table.get("log_level") {
            apply_log_level(value, &mut config, &mut bad);
        }
        let sweep = match table.get("journal") {
            Some(value) => apply_journal(value, &mut config, &mut bad),
            None => JournalSweep::KeepDays(config.journal.retention_days),
        };

        ConfigLoad {
            config,
            sweep,
            sections: other_sections(&table),
            report: if bad.is_empty() {
                ConfigReport::Clean
            } else {
                ConfigReport::BadValues(bad)
            },
        }
    }
}

fn clean_defaults() -> ConfigLoad {
    let config = Config::default();
    let sweep = JournalSweep::KeepDays(config.journal.retention_days);
    ConfigLoad {
        config,
        sweep,
        sections: Map::new(),
        report: ConfigReport::Clean,
    }
}

fn whole_file(reason: WholeFileReason) -> ConfigLoad {
    ConfigLoad {
        config: Config::default(),
        sweep: JournalSweep::Paused,
        sections: Map::new(),
        report: ConfigReport::WholeFile(reason),
    }
}

fn apply_log_level(value: &toml::Value, config: &mut Config, bad: &mut Vec<String>) {
    let Ok(level) = value.clone().try_into::<String>() else {
        bad.push("log_level".to_owned());
        return;
    };
    if LOG_LEVELS.contains(&level.as_str()) {
        config.log_level = level;
    } else {
        bad.push("log_level".to_owned());
    }
}

fn apply_journal(value: &toml::Value, config: &mut Config, bad: &mut Vec<String>) -> JournalSweep {
    let toml::Value::Table(table) = value else {
        bad.push("journal".to_owned());
        return JournalSweep::Paused;
    };
    let Some(raw_days) = table.get("retention_days") else {
        return JournalSweep::KeepDays(config.journal.retention_days);
    };
    match raw_days.clone().try_into::<u32>() {
        Ok(days) if days >= 1 => {
            config.journal.retention_days = days;
            JournalSweep::KeepDays(days)
        }
        _ => {
            bad.push("journal.retention_days".to_owned());
            JournalSweep::Paused
        }
    }
}

fn other_sections(table: &toml::Table) -> Map<String, Value> {
    let mut sections = Map::new();
    for (key, value) in table {
        if key == "log_level" || key == "journal" {
            continue;
        }
        let json = serde_json::to_value(value).unwrap_or(Value::Null);
        sections.insert(key.clone(), json);
    }
    sections
}

fn line_column(text: &str, start: usize) -> Option<(usize, usize)> {
    if start > text.len() || !text.is_char_boundary(start) {
        return None;
    }
    let prefix = &text[..start];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = match prefix.rfind('\n') {
        Some(newline) => prefix[newline + 1..].chars().count() + 1,
        None => prefix.chars().count() + 1,
    };
    Some((line, column))
}
