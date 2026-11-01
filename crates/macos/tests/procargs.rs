//! Argument-buffer parser, including buffers the kernel never returns.

#![allow(clippy::unwrap_used)] // helpers outside #[test] fns

use navori_macos::{ReadError, parse_procargs};

/// `argc`, the executable path, four padding NULs, then each argument and env var.
fn buf(argc: i32, exec: &[u8], args: &[&[u8]], env: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&argc.to_ne_bytes());
    out.extend_from_slice(exec);
    out.extend_from_slice(&[0, 0, 0, 0]);
    for arg in args {
        out.extend_from_slice(arg);
        out.push(0);
    }
    for var in env {
        out.extend_from_slice(var);
        out.push(0);
    }
    out
}

/// Marsaglia xorshift64. The seed is never zero, so the sequence does not collapse.
fn xorshift64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

#[test]
fn the_arguments_are_returned_and_the_environment_is_not() {
    let raw = buf(
        2,
        b"/bin/zsh",
        &[b"zsh", b"-l"],
        &[b"HOME=/Users/x", b"PATH=/bin"],
    );
    assert_eq!(parse_procargs(&raw).unwrap(), vec!["zsh", "-l"]);
}

#[test]
fn a_non_utf8_argument_is_kept_lossily() {
    let raw = buf(2, b"/usr/bin/node", &[b"node", b"\xff/x.js"], &[]);
    assert_eq!(parse_procargs(&raw).unwrap(), vec!["node", "\u{fffd}/x.js"]);
}

#[test]
fn a_buffer_with_fewer_strings_than_argc_is_malformed() {
    let raw = buf(3, b"/bin/sh", &[b"sh", b"-c"], &[]);
    assert_eq!(parse_procargs(&raw).unwrap_err(), ReadError::Malformed);
}

#[test]
fn an_unterminated_argument_is_malformed() {
    let mut raw = buf(2, b"/bin/sh", &[b"sh", b"-c"], &[]);
    raw.pop().unwrap();
    assert_eq!(parse_procargs(&raw).unwrap_err(), ReadError::Malformed);
}

#[test]
fn an_empty_or_argc_only_buffer_is_malformed() {
    assert_eq!(parse_procargs(&[]).unwrap_err(), ReadError::Malformed);
    assert_eq!(
        parse_procargs(&2i32.to_ne_bytes()).unwrap_err(),
        ReadError::Malformed
    );
}

#[test]
fn random_and_truncated_buffers_never_panic() {
    let mut state = 0x9E37_79B9_7F4A_7C15;
    for _ in 0..10_000 {
        let len = usize::try_from(xorshift64(&mut state) % 513).unwrap();
        let mut raw = vec![0u8; len];
        for chunk in raw.chunks_mut(8) {
            let word = xorshift64(&mut state).to_ne_bytes();
            chunk.copy_from_slice(&word[..chunk.len()]);
        }
        let parsed = parse_procargs(&raw);
        assert!(parsed.is_ok() || parsed.is_err());
    }

    let valid = buf(
        2,
        b"/bin/zsh",
        &[b"zsh", b"-l"],
        &[b"HOME=/Users/x", b"PATH=/bin"],
    );
    for len in 0..=valid.len() {
        let parsed = parse_procargs(valid.get(..len).unwrap());
        assert!(parsed.is_ok() || parsed.is_err());
    }
}
