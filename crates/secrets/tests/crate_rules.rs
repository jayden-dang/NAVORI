#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::fs;
use std::path::{Path, PathBuf};

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workspace_root() -> PathBuf {
    crate_dir().join("../..")
}

fn read(path: &Path) -> toml::Table {
    fs::read_to_string(path)
        .expect("read")
        .parse()
        .expect("toml")
}

/// Every `*dependencies*` table of a manifest, including `[target.*.dependencies]`.
fn dependency_tables(manifest: &toml::Table) -> Vec<(String, &toml::Table)> {
    let mut found = Vec::new();
    for (key, value) in manifest {
        if key.ends_with("dependencies")
            && let Some(table) = value.as_table()
        {
            found.push((key.clone(), table));
        }
        if key == "target" {
            for (cfg, body) in value.as_table().into_iter().flatten() {
                for (inner, table) in body.as_table().into_iter().flatten() {
                    if inner.ends_with("dependencies")
                        && let Some(table) = table.as_table()
                    {
                        found.push((format!("target.{cfg}.{inner}"), table));
                    }
                }
            }
        }
    }
    found
}

#[test]
fn the_crate_depends_on_no_other_navori_crate_and_no_database_crate() {
    let manifest = read(&crate_dir().join("Cargo.toml"));
    for (table_name, table) in dependency_tables(&manifest) {
        for name in table.keys() {
            assert!(
                name != "rusqlite" && name != "navori-store",
                "{table_name} names {name}"
            );
            let own_dev_entry = table_name == "dev-dependencies" && name == "navori-secrets";
            assert!(
                !name.starts_with("navori-") || own_dev_entry,
                "{table_name} names {name}"
            );
        }
    }
}

#[test]
fn the_apple_store_is_the_regular_keychain_only() {
    let manifest = read(&crate_dir().join("Cargo.toml"));
    let own = manifest["dependencies"]["apple-native-keyring-store"]
        .as_table()
        .expect("table");
    assert_eq!(
        own.get("workspace").and_then(toml::Value::as_bool),
        Some(true)
    );
    assert!(
        own.get("features").is_none(),
        "the crate must not add features of its own"
    );
    let root = read(&workspace_root().join("Cargo.toml"));
    let shared = root["workspace"]["dependencies"]["apple-native-keyring-store"]
        .as_table()
        .expect("table");
    let features: Vec<&str> = shared["features"]
        .as_array()
        .expect("features")
        .iter()
        .filter_map(toml::Value::as_str)
        .collect();
    assert_eq!(features, ["keychain"]);
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if matches!(
            entry.file_name().to_str(),
            Some("target" | "build" | ".git" | ".worktrees" | ".claude" | "node_modules")
        ) {
            continue;
        }
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn only_the_manual_run_opens_the_real_keychain_and_it_is_ignored_by_default() {
    let needle = concat!("Keychain", "Store::open(");
    let manual = crate_dir()
        .join("tests/keychain_real.rs")
        .canonicalize()
        .expect("manual run exists");
    let mut files = Vec::new();
    rust_files(&workspace_root(), &mut files);
    assert!(
        !files.is_empty(),
        "the scan found no Rust files; the workspace root is wrong"
    );
    for file in files {
        let text = fs::read_to_string(&file).expect("read");
        if file.canonicalize().ok().as_deref() == Some(manual.as_path()) {
            assert!(
                text.lines()
                    .any(|line| line.trim_start().starts_with("#[ignore")),
                "the manual run must stay #[ignore]d"
            );
        } else if file.components().any(|part| part.as_os_str() == "tests") {
            assert!(
                !text.contains(needle),
                "{} opens the real Keychain",
                file.display()
            );
        }
    }
}
