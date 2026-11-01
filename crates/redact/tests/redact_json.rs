use navori_redact::redact_json;
use serde_json::json;

#[test]
fn secrets_nested_anywhere_in_json_strings_are_redacted_and_counted() {
    let mut v = json!({
        "summary": "pushed with ghp_A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8",
        "nested": { "list": ["ok", "AKIAIOSFODNN7EXAMPLE"] },
        "count": 3,
    });
    let n = redact_json(&mut v);
    assert_eq!(n, 2);
    assert_eq!(v["summary"], "pushed with [REDACTED:github_token]");
    assert_eq!(v["nested"]["list"][1], "[REDACTED:aws_access_key]");
    assert_eq!(v["nested"]["list"][0], "ok");
    assert_eq!(v["count"], 3);
}

#[test]
fn values_under_secret_named_keys_are_redacted_even_if_they_look_harmless() {
    let mut v = json!({ "password": "hunter2", "api_key": "abc", "token": null, "name": "navori" });
    redact_json(&mut v);
    assert_eq!(v["password"], "[REDACTED:secret_field]");
    assert_eq!(v["api_key"], "[REDACTED:secret_field]");
    assert_eq!(v["token"], serde_json::Value::Null);
    assert_eq!(v["name"], "navori");
}
