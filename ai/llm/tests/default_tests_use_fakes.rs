#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

fn rust_files_in(dir: &Path) -> Vec<PathBuf> {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("cannot read {}: {:?}", dir.display(), error.kind()));
    let mut files = Vec::new();
    for entry in entries {
        let path = entry
            .unwrap_or_else(|error| {
                panic!(
                    "cannot read an entry of {}: {:?}",
                    dir.display(),
                    error.kind()
                )
            })
            .path();
        if path.is_dir() {
            files.extend(rust_files_in(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    files
}

fn squeezed(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

#[test]
fn every_default_test_uses_fakes() {
    let llm = Path::new(env!("CARGO_MANIFEST_DIR"));
    let live = llm.join("tests/live_deepseek.rs");
    let this = llm.join("tests/default_tests_use_fakes.rs");
    let ask = llm.join("../../crates/ffi/tests/ask.rs");
    let llm_sources: Vec<PathBuf> = rust_files_in(&llm.join("tests"))
        .into_iter()
        .filter(|path| *path != live && *path != this)
        .collect();
    let context_sources = rust_files_in(&llm.join("../context/tests"));
    assert!(
        !llm_sources.is_empty(),
        "no ai/llm default tests were found"
    );
    assert!(
        !context_sources.is_empty(),
        "no ai/context tests were found"
    );
    assert!(ask.is_file(), "{} is missing", ask.display());

    let mut sources = llm_sources;
    sources.extend(context_sources);
    sources.push(ask);

    let agent_tests = llm.join("../agent/tests");
    if agent_tests.is_dir() {
        sources.extend(rust_files_in(&agent_tests));
    }
    let ffi_agent = llm.join("../../crates/ffi/tests/agent.rs");
    if ffi_agent.is_file() {
        sources.push(ffi_agent);
    }
    let live_agent = llm.join("../../crates/ffi/tests/live_agent.rs");
    sources.retain(|path| *path != live_agent);

    let forbidden = [
        concat!("Keychain", "Store::open"),
        concat!("Command::new(", "\"pass"),
        concat!("proc", "ess::"),
        concat!("Command", "as"),
    ];
    for path in &sources {
        let text =
            squeezed(&fs::read_to_string(path).unwrap_or_else(|error| {
                panic!("cannot read {}: {:?}", path.display(), error.kind())
            }));
        for pattern in forbidden {
            assert!(
                !text.contains(pattern),
                "{} reaches a real boundary with `{pattern}`",
                path.display()
            );
        }
    }

    let live_raw = fs::read_to_string(&live).expect("the live test file exists");
    assert!(
        live_raw.contains("#[ignore"),
        "the live test must be marked ignored"
    );
    let live_text = squeezed(&live_raw);
    let decode = concat!("from_", "utf8(");
    for (start, _) in live_text.match_indices(decode) {
        let statement = live_text[start..].split(';').next().unwrap();
        for sink in [concat!(".ex", "pect("), concat!(".un", "wrap(")] {
            assert!(
                !statement.contains(sink),
                "the live test formats the decode error of the pass output with `{sink}`"
            );
        }
    }
}
