use std::fs;

use navori_config::{Config, ConfigError};

#[test]
fn missing_file_yields_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = Config::load(&dir.path().join("config.toml")).unwrap();
    assert_eq!(cfg.log_level, "info");
    assert_eq!(cfg.journal.retention_days, 30);
}

#[test]
fn partial_file_overrides_only_what_it_sets() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "log_level = \"debug\"\n").unwrap();
    let cfg = Config::load(&path).unwrap();
    assert_eq!(cfg.log_level, "debug");
    assert_eq!(cfg.journal.retention_days, 30);
}

#[test]
fn unknown_keys_are_tolerated_for_forward_compatibility() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(
        &path,
        "future_feature = true\n[journal]\nretention_days = 7\n",
    )
    .unwrap();
    assert_eq!(Config::load(&path).unwrap().journal.retention_days, 7);
}

#[test]
fn malformed_toml_is_rejected_with_the_file_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "log_level = \n").unwrap();
    let err = Config::load(&path).unwrap_err();
    assert!(matches!(err, ConfigError::Invalid { .. }));
    assert!(err.to_string().contains("config.toml"), "{err}");
}

#[test]
fn out_of_range_values_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "[journal]\nretention_days = 0\n").unwrap();
    let err = Config::load(&path).unwrap_err();
    assert!(err.to_string().contains("retention_days"), "{err}");

    fs::write(&path, "log_level = \"loud\"\n").unwrap();
    let err = Config::load(&path).unwrap_err();
    assert!(err.to_string().contains("log_level"), "{err}");
}
