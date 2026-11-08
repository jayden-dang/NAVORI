//! Needs you Destructive grant and Navori AI's refusal to answer.
#![allow(clippy::unwrap_used)]

use std::time::Duration;

use navori_agent::{Dispatcher, Toolset};
use navori_config::JournalSweep;
use navori_kernel::{Envelope, Kernel};
use navori_llm::ToolCall;
use navori_mod_needs_you::NeedsYou;
use navori_store::Store;
use serde_json::json;

#[tokio::test]
async fn navori_ai_cannot_answer_a_decision() {
    let store = Store::open_in_memory().unwrap();
    let kernel = Kernel::builder(store)
        .pause_journal_sweep()
        .register(NeedsYou::new(JournalSweep::Paused))
        .grant_destructive("needs_you")
        .start();

    for _ in 0..30 {
        tokio::task::yield_now().await;
    }

    let raised = kernel
        .dispatch(Envelope {
            module: "needs_you".into(),
            action: json!({
                "raise": {
                    "key": "k1",
                    "title": "Stay open",
                    "reason": "AI must not answer",
                    "primary": { "label": "Ok" },
                    "secondary": { "label": "Later" },
                }
            }),
        })
        .await
        .unwrap();
    let id = raised["id"].as_i64().unwrap();
    let revision = raised["revision"].as_u64().unwrap();

    let toolset = Toolset::from_specs(&kernel.actions());
    let outcome = Dispatcher::new(Duration::from_secs(5))
        .call(
            &kernel,
            &toolset,
            &ToolCall {
                id: "1".into(),
                name: "needs_you_answer".into(),
                arguments: json!({
                    "id": id,
                    "revision": revision,
                    "choice": "primary",
                })
                .to_string(),
            },
        )
        .await;

    assert!(outcome.is_error, "{outcome:?}");
    assert!(
        outcome.text.contains("not available") || outcome.text.contains("cannot run"),
        "{}",
        outcome.text
    );

    let state = kernel.state("needs_you").unwrap();
    let decisions = state["decisions"].as_array().unwrap();
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0]["state"], "open");

    kernel.shutdown().await;
}
