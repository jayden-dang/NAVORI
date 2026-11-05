#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::atomic::Ordering;
use std::time::Duration;

use navori_agent::{Dispatcher, Toolset};
use navori_context::ToolCall;

fn call(name: &str, arguments: &str) -> ToolCall {
    ToolCall {
        id: "c1".to_owned(),
        name: name.to_owned(),
        arguments: arguments.to_owned(),
    }
}

#[tokio::test]
async fn a_read_action_is_dispatched_and_its_reply_returned() {
    let (kernel, received) = common::kernel_with_probe();
    let toolset = Toolset::from_specs(&kernel.actions());

    let outcome = Dispatcher::new(Duration::from_secs(10))
        .call(&kernel, &toolset, &call("probe_look", "{}"))
        .await;

    assert_eq!(outcome.text, r#"{"seen":"room"}"#);
    assert!(!outcome.is_error);
    assert_eq!(received.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_write_action_is_refused_without_reaching_the_module() {
    let (kernel, received) = common::kernel_with_probe();
    let toolset = Toolset::from_specs(&kernel.actions());

    let outcome = Dispatcher::new(Duration::from_secs(10))
        .call(&kernel, &toolset, &call("probe_change", "{}"))
        .await;

    assert_eq!(
        outcome.text,
        "Error: Navori AI cannot run probe_change yet: it changes things and needs your approval, which is not available."
    );
    assert!(outcome.is_error);
    assert_eq!(received.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn an_unknown_name_is_refused() {
    let (kernel, received) = common::kernel_with_probe();
    let toolset = Toolset::from_specs(&kernel.actions());

    let outcome = Dispatcher::new(Duration::from_secs(10))
        .call(&kernel, &toolset, &call("probe_gone", "{}"))
        .await;

    assert_eq!(outcome.text, "Error: no Action named probe_gone.");
    assert!(outcome.is_error);
    assert_eq!(received.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn arguments_that_are_not_json_are_refused() {
    let (kernel, received) = common::kernel_with_probe();
    let toolset = Toolset::from_specs(&kernel.actions());

    let outcome = Dispatcher::new(Duration::from_secs(10))
        .call(&kernel, &toolset, &call("probe_look_at", "{n:"))
        .await;

    assert!(
        outcome
            .text
            .starts_with("Error: the arguments are not valid JSON: "),
        "{}",
        outcome.text
    );
    assert!(outcome.text.ends_with('.'));
    assert!(outcome.is_error);
    assert_eq!(received.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn arguments_that_do_not_fit_reach_no_module() {
    let (kernel, received) = common::kernel_with_probe();
    let toolset = Toolset::from_specs(&kernel.actions());

    let outcome = Dispatcher::new(Duration::from_secs(10))
        .call(&kernel, &toolset, &call("probe_look_at", r#"{"n":"x"}"#))
        .await;

    assert!(
        outcome
            .text
            .starts_with("Error: the arguments do not fit probe_look_at"),
        "{}",
        outcome.text
    );
    assert!(outcome.is_error);
    assert_eq!(received.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_unit_action_is_sent_as_its_bare_name() {
    for arguments in ["", "null", "{}"] {
        let (kernel, received) = common::kernel_with_probe();
        let toolset = Toolset::from_specs(&kernel.actions());

        let outcome = Dispatcher::new(Duration::from_secs(10))
            .call(&kernel, &toolset, &call("probe_look", arguments))
            .await;

        assert_eq!(
            outcome.text, r#"{"seen":"room"}"#,
            "arguments {arguments:?}"
        );
        assert!(!outcome.is_error);
        assert_eq!(received.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn a_unit_action_given_arguments_reaches_no_module() {
    let (kernel, received) = common::kernel_with_probe();
    let toolset = Toolset::from_specs(&kernel.actions());

    let outcome = Dispatcher::new(Duration::from_secs(10))
        .call(&kernel, &toolset, &call("probe_look", r#"{"x":1}"#))
        .await;

    assert!(
        outcome
            .text
            .starts_with("Error: the arguments do not fit probe_look"),
        "{}",
        outcome.text
    );
    assert!(outcome.is_error);
    assert_eq!(received.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn an_action_slower_than_the_limit_times_out() {
    let (kernel, _received) = common::kernel_with_probe();
    let toolset = Toolset::from_specs(&kernel.actions());

    let outcome = Dispatcher::new(Duration::from_millis(200))
        .call(&kernel, &toolset, &call("probe_slow", "{}"))
        .await;

    assert_eq!(
        outcome.text,
        "Error: probe_slow did not answer within 0.2 s."
    );
    assert!(outcome.is_error);
}
