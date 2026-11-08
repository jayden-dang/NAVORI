//! A Module raises through its dispatcher; answering runs its Envelope.
#![allow(clippy::unwrap_used)]

mod common;

use common::harness;
use navori_kernel::Envelope;
use serde_json::json;

#[tokio::test(start_paused = true)]
async fn a_module_raises_through_its_dispatcher_and_its_envelope_runs_on_answer() {
    let h = harness();
    h.idle().await;

    let reply = h
        .kernel
        .dispatch(Envelope {
            module: "producer".into(),
            action: json!({
                "raise": {
                    "key": "from-producer",
                    "title": "Producer raised",
                    "reason": "via dispatcher",
                    "primary_label": "Poke",
                    "primary_envelope": { "module": "target", "action": "poke" },
                    "secondary_label": "Later",
                }
            }),
        })
        .await
        .unwrap();

    let id = reply["id"].as_i64().unwrap();
    let revision = reply["revision"].as_u64().unwrap() as u32;
    assert_eq!(h.state()["decisions"][0]["key"], "from-producer");

    h.answer(id, revision, "primary").await.unwrap();
    h.settle().await;

    assert_eq!(h.target_state()["seen"], json!(["poke"]));
    assert!(h.state()["decisions"].as_array().unwrap().is_empty());

    h.shutdown().await;
}
