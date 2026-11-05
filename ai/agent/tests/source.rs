#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;

const HEADER_PREFIX: &str = "// Ported from dsh (MIT, Copyright (c) 2026 DeepSeek): ";
const HEADER_SUFFIX: &str = ". See ai/agent/LICENSE-dsh.";

fn crate_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn the_dsh_licence_text_keeps_its_copyright_line() {
    let text = fs::read_to_string(crate_dir().join("LICENSE-dsh")).unwrap();
    assert!(text.contains("Copyright (c) 2026 DeepSeek"));
    assert!(text.contains("Permission is hereby granted, free of charge"));
}

#[test]
fn every_ported_file_opens_with_the_port_header() {
    let mut offenders = Vec::new();
    for entry in fs::read_dir(crate_dir().join("src")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        let first = text.lines().next().unwrap_or_default();
        let mentions_dsh = text.contains("dsh");
        let well_formed = first.starts_with(HEADER_PREFIX) && first.ends_with(HEADER_SUFFIX);
        if mentions_dsh && !well_formed {
            offenders.push(path.display().to_string());
        }
    }
    assert!(offenders.is_empty(), "missing port header: {offenders:?}");
}

#[test]
fn only_the_dispatcher_calls_kernel_dispatch() {
    let mut offenders = Vec::new();
    for entry in fs::read_dir(crate_dir().join("src")).unwrap() {
        let path = entry.unwrap().path();
        if path.file_name().is_some_and(|n| n == "dispatch.rs") {
            continue;
        }
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        if fs::read_to_string(&path).unwrap().contains(".dispatch(") {
            offenders.push(path.display().to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "dispatch outside the dispatcher: {offenders:?}"
    );
}
