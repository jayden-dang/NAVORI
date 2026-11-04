#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

fn rust_files_in(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files = Vec::new();
    for path in entries.map(|entry| entry.unwrap().path()) {
        if path.is_dir() {
            files.extend(rust_files_in(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    files
}

#[test]
fn every_default_test_uses_fakes() {
    let llm = Path::new(env!("CARGO_MANIFEST_DIR"));
    let live = llm.join("tests/live_deepseek.rs");
    let this = llm.join("tests/default_tests_use_fakes.rs");
    let mut sources: Vec<PathBuf> = rust_files_in(&llm.join("tests"))
        .into_iter()
        .filter(|path| *path != live && *path != this)
        .collect();
    sources.extend(rust_files_in(&llm.join("../context/tests")));
    sources.push(llm.join("../../crates/ffi/tests/ask.rs"));

    let forbidden = [
        concat!("Keychain", "Store::open"),
        concat!("Command::new(", "\"pass\")"),
    ];
    for path in sources {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        for pattern in forbidden {
            assert!(
                !text.contains(pattern),
                "{} reaches a real boundary with `{pattern}`",
                path.display()
            );
        }
    }

    let live_text = fs::read_to_string(&live).expect("the live test file exists");
    assert!(
        live_text.contains("#[ignore"),
        "the live test must be marked ignored"
    );
}
