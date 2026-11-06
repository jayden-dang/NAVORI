#![allow(clippy::unwrap_used)]

use navori_protocol::Harness;

#[test]
fn each_harness_has_one_id_and_one_display_name() {
    assert_eq!(Harness::ALL.map(Harness::id), ["claude", "codex", "grok"]);
    assert_eq!(
        Harness::ALL.map(Harness::display),
        ["Claude Code", "Codex", "Grok Build"]
    );
}

#[test]
fn the_id_round_trips_through_json() {
    let json = serde_json::to_value(Harness::ClaudeCode).unwrap();
    assert_eq!(json, serde_json::json!("claude"));
    assert_eq!(
        serde_json::from_value::<Harness>(json).unwrap(),
        Harness::ClaudeCode
    );
}

#[test]
fn parse_accepts_the_id_the_display_name_and_the_old_spellings() {
    for text in ["claude", "claude_code", "Claude Code", "ClaudeCode"] {
        assert_eq!(Harness::parse(text), Some(Harness::ClaudeCode), "{text}");
    }
    for text in ["codex", "Codex"] {
        assert_eq!(Harness::parse(text), Some(Harness::Codex), "{text}");
    }
    for text in ["grok", "Grok", "Grok Build"] {
        assert_eq!(Harness::parse(text), Some(Harness::Grok), "{text}");
    }
    assert_eq!(Harness::parse("unknown"), None);
}
