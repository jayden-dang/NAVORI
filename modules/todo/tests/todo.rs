//! Adding a To-do and keeping it across a restart.
#![allow(clippy::unwrap_used)]

mod common;

use common::{harness, today};
use navori_kernel::{DispatchError, ModuleAction};
use navori_mod_todo::TodoAction;

#[tokio::test(start_paused = true)]
async fn adding_a_todo_shows_it_at_once() {
    let h = harness();
    h.idle().await;

    let reply = h.add("Buy milk", Some(today())).await.unwrap();
    let id = reply["id"].as_i64().unwrap();
    assert!(id > 0, "{reply}");

    let state = h.state();
    assert_eq!(state["today"], today());
    let open = state["open"].as_array().unwrap();
    assert_eq!(open.len(), 1, "{state}");
    assert_eq!(open[0]["id"], id);
    assert_eq!(open[0]["title"], "Buy milk");
    assert_eq!(open[0]["due_on"], today());
    assert_eq!(open[0]["status"], "today");

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn a_blank_title_adds_nothing() {
    let h = harness();
    h.idle().await;

    for title in ["", "   ", "\t"] {
        let err = h.add(title, None).await.expect_err(title);
        assert!(matches!(err, DispatchError::Action(_)), "{title}: {err:?}");
        assert!(
            h.state()["open"].as_array().unwrap().is_empty(),
            "{title} left a To-do"
        );
    }

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn the_title_passes_the_shield() {
    let h = harness();
    h.idle().await;
    let secret = concat!("sk-", "proj-", "Ab12Cd34Ef56Gh78Ij90Kl12Mn34Op56Qr78");

    h.add(&format!("leak {secret} here"), None).await.unwrap();

    let state = h.state();
    let title = state["open"][0]["title"].as_str().unwrap();
    assert!(!title.contains(secret), "{title}");
    assert!(title.contains("[REDACTED:sk_api_key]"), "{title}");

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn todos_survive_a_restart() {
    let mut h = harness();
    h.idle().await;

    let reply = h.add("Keep me", Some(today())).await.unwrap();
    let id = reply["id"].clone();

    h.reopen().await;

    let open = h.state()["open"].as_array().unwrap().clone();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0]["id"], id);
    assert_eq!(open[0]["title"], "Keep me");
    assert_eq!(open[0]["due_on"], today());
    assert_eq!(open[0]["status"], "today");

    h.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn todos_never_turn_on_attention() {
    let h = harness();
    h.idle().await;

    assert!(!h.kernel.tray(None).attention);

    h.add("Quiet", Some(today())).await.unwrap();
    assert!(!h.kernel.tray(None).attention);

    h.shutdown().await;
}

#[test]
fn add_is_listed_as_write() {
    let docs = TodoAction::catalog();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].name, "add");
    assert_eq!(docs[0].risk, navori_kernel::Risk::Write);
}
