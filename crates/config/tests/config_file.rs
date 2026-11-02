use std::fs;

use navori_config::{Config, ConfigReport, JournalSweep, WholeFileReason};

#[test]
fn missing_file_yields_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let loaded = Config::load(&dir.path().join("config.toml"));
    assert_eq!(loaded.config.log_level, "info");
    assert_eq!(loaded.config.journal.retention_days, 30);
    assert_eq!(loaded.sweep, JournalSweep::KeepDays(30));
    assert_eq!(loaded.report, ConfigReport::Clean);
}

#[test]
fn partial_file_overrides_only_what_it_sets() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "log_level = \"debug\"\n").unwrap();
    let loaded = Config::load(&path);
    assert_eq!(loaded.config.log_level, "debug");
    assert_eq!(loaded.config.journal.retention_days, 30);
    assert_eq!(loaded.sweep, JournalSweep::KeepDays(30));
    assert_eq!(loaded.report, ConfigReport::Clean);
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
    let loaded = Config::load(&path);
    assert_eq!(loaded.config.log_level, "info");
    assert_eq!(loaded.config.journal.retention_days, 7);
    assert_eq!(loaded.sweep, JournalSweep::KeepDays(7));
    assert_eq!(loaded.report, ConfigReport::Clean);
}

#[test]
fn a_bad_value_falls_back_alone() {
    let cases = [
        (
            "log_level = \"loud\"\n[journal]\nretention_days = 365\n",
            "info",
            365,
            JournalSweep::KeepDays(365),
            "log_level",
        ),
        (
            "log_level = 42\n[journal]\nretention_days = 365\n",
            "info",
            365,
            JournalSweep::KeepDays(365),
            "log_level",
        ),
        (
            "log_level = \"debug\"\n[journal]\nretention_days = 0\n",
            "debug",
            30,
            JournalSweep::Paused,
            "journal.retention_days",
        ),
        (
            "log_level = \"debug\"\n[journal]\nretention_days = \"365\"\n",
            "debug",
            30,
            JournalSweep::Paused,
            "journal.retention_days",
        ),
        (
            "log_level = \"debug\"\njournal = 3\n",
            "debug",
            30,
            JournalSweep::Paused,
            "journal",
        ),
    ];

    for (body, log_level, retention_days, sweep, key) in cases {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, body).unwrap();
        let loaded = Config::load(&path);
        assert_eq!(loaded.config.log_level, log_level, "{body}");
        assert_eq!(
            loaded.config.journal.retention_days, retention_days,
            "{body}"
        );
        assert_eq!(loaded.sweep, sweep, "{body}");
        assert_eq!(
            loaded.report,
            ConfigReport::BadValues(vec![key.to_owned()]),
            "{body}"
        );
    }
}

#[test]
fn invalid_toml_falls_back_as_a_whole_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "log_level = \n").unwrap();
    let loaded = Config::load(&path);
    assert_eq!(loaded.config, Config::default());
    assert_eq!(loaded.sweep, JournalSweep::Paused);
    assert!(loaded.sections.is_empty());
    assert!(
        matches!(
            loaded.report,
            ConfigReport::WholeFile(WholeFileReason::NotToml {
                position: Some((1, _))
            })
        ),
        "{:?}",
        loaded.report
    );
}

#[test]
fn an_unreadable_file_falls_back_as_a_whole_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::create_dir(&path).unwrap();
    let loaded = Config::load(&path);
    assert_eq!(loaded.config, Config::default());
    assert_eq!(loaded.sweep, JournalSweep::Paused);
    assert!(matches!(
        loaded.report,
        ConfigReport::WholeFile(WholeFileReason::Unreadable)
    ));

    fs::remove_dir(&path).unwrap();
    fs::write(&path, [0xff, 0xfe, 0xfd]).unwrap();
    let loaded = Config::load(&path);
    assert_eq!(loaded.config, Config::default());
    assert_eq!(loaded.sweep, JournalSweep::Paused);
    assert!(matches!(
        loaded.report,
        ConfigReport::WholeFile(WholeFileReason::Unreadable)
    ));
}

#[test]
fn a_journal_table_without_the_key_counts_as_absent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(
        &path,
        "log_level = \"debug\"\n[journal]\n# retention_days = 365\n",
    )
    .unwrap();
    let loaded = Config::load(&path);
    assert_eq!(loaded.config.log_level, "debug");
    assert_eq!(loaded.config.journal.retention_days, 30);
    assert_eq!(loaded.sweep, JournalSweep::KeepDays(30));
    assert_eq!(loaded.report, ConfigReport::Clean);
}

#[test]
fn module_sections_reach_the_map_and_core_keys_do_not() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(
        &path,
        "log_level = \"debug\"\nfuture_feature = true\n[journal]\nretention_days = 7\n[notes]\ntitle = \"hello\"\n",
    )
    .unwrap();
    let loaded = Config::load(&path);
    assert_eq!(loaded.config.log_level, "debug");
    assert_eq!(loaded.config.journal.retention_days, 7);
    assert_eq!(loaded.sweep, JournalSweep::KeepDays(7));
    assert_eq!(loaded.report, ConfigReport::Clean);
    assert!(!loaded.sections.contains_key("log_level"));
    assert!(!loaded.sections.contains_key("journal"));
    assert_eq!(
        loaded
            .sections
            .get("future_feature")
            .and_then(|value| value.as_bool()),
        Some(true)
    );
    let notes = loaded
        .sections
        .get("notes")
        .and_then(|value| value.as_object())
        .expect("notes section");
    assert_eq!(
        notes.get("title").and_then(|value| value.as_str()),
        Some("hello")
    );
}
