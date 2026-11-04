#![allow(clippy::unwrap_used, clippy::expect_used)]

use navori_protocol::rpc::{code, method};
use navori_protocol::{AskReply, AskRequest};

#[test]
fn core_ask_has_its_method_name_and_four_distinct_codes_in_the_free_range() {
    assert_eq!(method::ASK, "core.ask");
    assert_eq!(code::ASK_NO_KEY, -32004);
    assert_eq!(code::ASK_KEYCHAIN, -32005);
    assert_eq!(code::ASK_BLOCKED, -32006);
    assert_eq!(code::ASK_PROVIDER, -32007);

    let new = [
        code::ASK_NO_KEY,
        code::ASK_KEYCHAIN,
        code::ASK_BLOCKED,
        code::ASK_PROVIDER,
    ];
    let existing = [
        code::PARSE_ERROR,
        code::INVALID_REQUEST,
        code::METHOD_NOT_FOUND,
        code::INVALID_PARAMS,
        code::INTERNAL,
        code::UNKNOWN_MODULE,
        code::ACTION_FAILED,
        code::MODULE_UNAVAILABLE,
    ];
    for (i, c) in new.iter().enumerate() {
        assert!(*c < -32003 && *c >= -32099, "{c} outside the free range");
        assert!(!existing.contains(c), "{c} collides with an existing code");
        assert!(!new[i + 1..].contains(c), "{c} is duplicated");
    }
}

#[test]
fn ask_params_and_reply_round_trip_through_json() {
    let request = AskRequest {
        model: "m".to_owned(),
        text: "hi".to_owned(),
    };
    assert_eq!(
        serde_json::to_string(&request).unwrap(),
        r#"{"model":"m","text":"hi"}"#
    );

    let reply: AskReply = serde_json::from_str(r#"{"reply":"ok"}"#).unwrap();
    assert_eq!(
        reply,
        AskReply {
            reply: "ok".to_owned()
        }
    );
}
