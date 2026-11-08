//! Raise a Decision through needs_you.raise.
#![allow(clippy::unwrap_used)]

mod common;

use common::{ORIGIN_MS, harness, raise_body};
use navori_kernel::DispatchError;
use navori_mod_needs_you::stored;
use serde_json::{Value, json};

#[tokio::test(start_paused = true)]
async fn a_new_key_adds_an_open_decision() {
    let h = harness();
    h.idle().await;

    let reply = h
        .raise(raise_body("k1", "Review this", "because", "Open", "Later"))
        .await
        .unwrap();
    assert_eq!(reply["revision"], 1);
    assert!(reply["id"].as_i64().unwrap() > 0);

    let state = h.state();
    let decisions = state["decisions"].as_array().unwrap();
    assert_eq!(decisions.len(), 1);
    let d = &decisions[0];
    assert_eq!(d["key"], "k1");
    assert_eq!(d["title"], "Review this");
    assert_eq!(d["reason"], "because");
    assert_eq!(d["revision"], 1);
    assert_eq!(d["raised_at_ms"], ORIGIN_MS);
    assert_eq!(d["state"], "open");
    assert_eq!(d["primary"]["label"], "Open");
    assert_eq!(d["secondary"]["label"], "Later");

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_repeat_key_replaces_and_bumps_revision() {
    let h = harness();
    h.idle().await;

    let first = h
        .raise(raise_body("k1", "First", "r1", "A", "B"))
        .await
        .unwrap();
    let second = h
        .raise(raise_body("k1", "Second", "r2", "Yes", "No"))
        .await
        .unwrap();

    assert_eq!(first["id"], second["id"]);
    assert_eq!(first["revision"], 1);
    assert_eq!(second["revision"], 2);

    let decisions = h.state()["decisions"].as_array().unwrap().clone();
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0]["title"], "Second");
    assert_eq!(decisions[0]["reason"], "r2");
    assert_eq!(decisions[0]["revision"], 2);
    assert_eq!(decisions[0]["primary"]["label"], "Yes");
    assert_eq!(decisions[0]["secondary"]["label"], "No");
    assert_eq!(decisions[0]["raised_at_ms"], ORIGIN_MS);

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn each_choice_records_its_catalog_risk() {
    let h = harness();
    h.idle().await;

    h.raise(json!({
        "raise": {
            "key": "k1",
            "title": "Act",
            "reason": "run poke",
            "primary": {
                "label": "Poke",
                "envelope": { "module": "target", "action": "poke" }
            },
            "secondary": { "label": "Skip" }
        }
    }))
    .await
    .unwrap();

    let d = &h.state()["decisions"][0];
    assert_eq!(d["primary"]["risk"], "write");
    assert!(d["secondary"]["risk"].is_null());

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn refused_raises_add_nothing() {
    let cases: Vec<(&str, Value)> = vec![
        (
            "empty key",
            raise_body("", "Title", "reason", "Ok", "Later"),
        ),
        (
            "blank title",
            raise_body("k1", "   ", "reason", "Ok", "Later"),
        ),
        (
            "blank label",
            raise_body("k1", "Title", "reason", "Ok", "  "),
        ),
        (
            "unknown module",
            json!({
                "raise": {
                    "key": "k1",
                    "title": "Title",
                    "reason": "reason",
                    "primary": {
                        "label": "Go",
                        "envelope": { "module": "nope", "action": "poke" }
                    },
                    "secondary": { "label": "Later" }
                }
            }),
        ),
        (
            "unknown action",
            json!({
                "raise": {
                    "key": "k1",
                    "title": "Title",
                    "reason": "reason",
                    "primary": {
                        "label": "Go",
                        "envelope": { "module": "target", "action": "missing" }
                    },
                    "secondary": { "label": "Later" }
                }
            }),
        ),
        (
            "needs_you action",
            json!({
                "raise": {
                    "key": "k1",
                    "title": "Title",
                    "reason": "reason",
                    "primary": {
                        "label": "Go",
                        "envelope": {
                            "module": "needs_you",
                            "action": {
                                "raise": {
                                    "key": "inner",
                                    "title": "t",
                                    "reason": "r",
                                    "primary": { "label": "a" },
                                    "secondary": { "label": "b" }
                                }
                            }
                        }
                    },
                    "secondary": { "label": "Later" }
                }
            }),
        ),
        (
            "token field",
            json!({
                "raise": {
                    "key": "k1",
                    "title": "Title",
                    "reason": "reason",
                    "primary": {
                        "label": "Go",
                        "envelope": {
                            "module": "target",
                            "action": { "poke": { "token": "secret" } }
                        }
                    },
                    "secondary": { "label": "Later" }
                }
            }),
        ),
        (
            "past expires_at_ms",
            json!({
                "raise": {
                    "key": "k1",
                    "title": "Title",
                    "reason": "reason",
                    "expires_at_ms": ORIGIN_MS,
                    "primary": { "label": "Ok" },
                    "secondary": { "label": "Later" }
                }
            }),
        ),
    ];

    for (name, body) in cases {
        let h = harness();
        h.idle().await;
        let err = h.raise(body).await.expect_err(name);
        assert!(matches!(err, DispatchError::Action(_)), "{name}: {err:?}");
        assert!(
            h.state()["decisions"].as_array().unwrap().is_empty(),
            "{name} left a Decision"
        );
        h.shutdown().await;
    }
}

#[tokio::test(start_paused = true)]
async fn title_reason_and_labels_pass_the_shield() {
    let h = harness();
    h.idle().await;
    let secret = concat!("sk-", "proj-", "Ab12Cd34Ef56Gh78Ij90Kl12Mn34Op56Qr78");

    h.raise(raise_body(
        "k1",
        "Title",
        &format!("leak {secret} here"),
        "Ok",
        "Later",
    ))
    .await
    .unwrap();

    let state = h.state();
    let reason = state["decisions"][0]["reason"].as_str().unwrap();
    assert!(!reason.contains(secret));
    assert!(reason.contains("[REDACTED:sk_api_key]"));

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn open_decisions_survive_a_restart() {
    let mut h = harness();
    h.idle().await;

    let reply = h
        .raise(raise_body("k1", "Keep me", "across restart", "Ok", "Later"))
        .await
        .unwrap();
    let id = reply["id"].clone();

    h.reopen().await;

    let decisions = h.state()["decisions"].as_array().unwrap().clone();
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0]["id"], id);
    assert_eq!(decisions[0]["key"], "k1");
    assert_eq!(decisions[0]["title"], "Keep me");
    assert_eq!(decisions[0]["reason"], "across restart");
    assert_eq!(decisions[0]["primary"]["label"], "Ok");
    assert_eq!(decisions[0]["secondary"]["label"], "Later");

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_raise_while_answering_is_refused() {
    let h = harness();
    h.idle().await;

    let reply = h
        .raise(raise_body("k1", "Busy", "answering", "Ok", "Later"))
        .await
        .unwrap();
    let id = reply["id"].as_i64().unwrap();
    stored::mark_answering(&h.store, id, "primary").unwrap();

    let err = h
        .raise(raise_body("k1", "Again", "nope", "Ok", "Later"))
        .await
        .expect_err("raise while answering");
    assert!(matches!(err, DispatchError::Action(_)), "{err:?}");

    // State still reflects the pre-reload answering row until a successful raise reloads;
    // the refused raise must not replace the stored row.
    let open = stored::find_open_key(&h.store, "k1").unwrap().unwrap();
    assert_eq!(open.id, id);
    assert_eq!(open.kind, stored::OpenKind::Answering);
    assert_eq!(open.revision, 1);

    h.shutdown().await;
}
