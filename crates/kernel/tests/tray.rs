//! The icon state composed from Module lifecycle and the config notice.
#![allow(clippy::unwrap_used)] // helpers outside #[test] fns

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use navori_kernel::{
    ActionDoc, Kernel, Module, ModuleAction, ModuleCtx, ModuleInfo, ModuleState, NoConfig,
    RestartPolicy, Risk, TrayContribution, TrayGlyph, TrayState, compose_tray,
};
use navori_store::Store;
use serde::{Deserialize, Serialize};

fn entry(id: &str, state: ModuleState) -> (ModuleInfo, TrayContribution) {
    (
        ModuleInfo {
            id: id.to_owned(),
            state,
            restarts: 0,
        },
        TrayContribution::default(),
    )
}

fn failed(id: &str, error: &str) -> (ModuleInfo, TrayContribution) {
    entry(
        id,
        ModuleState::Failed {
            last_error: error.to_owned(),
        },
    )
}

fn restarting(id: &str) -> (ModuleInfo, TrayContribution) {
    entry(
        id,
        ModuleState::Restarting {
            attempt: 1,
            last_error: "boom".to_owned(),
        },
    )
}

fn tray(glyph: TrayGlyph, summary: &str) -> TrayState {
    TrayState {
        glyph,
        attention: false,
        summary: summary.to_owned(),
    }
}

const NOTICE: (&str, &str) = ("config.toml has bad values", "Check awake.minutes.");

#[test]
fn a_healthy_core_is_normal_and_says_navori() {
    let entries = [entry("system", ModuleState::Running)];
    assert_eq!(
        compose_tray(&entries, None),
        tray(TrayGlyph::Normal, "Navori")
    );
}

#[test]
fn a_failed_module_shows_the_problem_glyph_and_its_sentence() {
    let entries = [
        entry("system", ModuleState::Running),
        failed("awake", "no power"),
    ];
    assert_eq!(
        compose_tray(&entries, None),
        tray(TrayGlyph::Problem, "awake stopped: no power")
    );
}

#[test]
fn a_restarting_module_shows_the_problem_glyph_and_its_sentence() {
    let entries = [entry("system", ModuleState::Running), restarting("awake")];
    assert_eq!(
        compose_tray(&entries, None),
        tray(TrayGlyph::Problem, "awake is restarting")
    );
}

#[test]
fn the_first_failed_module_wins_over_a_restarting_one() {
    let entries = [
        restarting("alpha"),
        failed("beta", "one"),
        failed("gamma", "two"),
    ];
    assert_eq!(
        compose_tray(&entries, None),
        tray(TrayGlyph::Problem, "beta stopped: one")
    );
}

#[test]
fn a_failed_module_with_no_error_text_says_unknown_error() {
    let entries = [failed("awake", "")];
    assert_eq!(
        compose_tray(&entries, None),
        tray(TrayGlyph::Problem, "awake stopped: unknown error")
    );
}

#[test]
fn a_starting_or_stopped_module_stays_normal() {
    let entries = [
        entry("alpha", ModuleState::Starting),
        entry("beta", ModuleState::Stopped),
    ];
    assert_eq!(
        compose_tray(&entries, None),
        tray(TrayGlyph::Normal, "Navori")
    );
}

#[test]
fn a_config_notice_alone_stays_normal_and_joins_the_summary() {
    let entries = [entry("system", ModuleState::Running)];
    assert_eq!(
        compose_tray(&entries, Some(NOTICE)),
        tray(
            TrayGlyph::Normal,
            "Navori. config.toml has bad values. Check awake.minutes."
        )
    );
}

#[test]
fn a_failed_module_comes_before_the_config_notice() {
    let entries = [failed("awake", "no power")];
    assert_eq!(
        compose_tray(&entries, Some(NOTICE)),
        tray(
            TrayGlyph::Problem,
            "awake stopped: no power. config.toml has bad values. Check awake.minutes."
        )
    );
}

#[test]
fn a_notice_keeps_its_title_and_message_unchanged() {
    let entries = [entry("system", ModuleState::Running)];
    let state = compose_tray(&entries, Some(("Odd title!", "Odd, message?")));
    assert_eq!(state.summary, "Navori. Odd title!. Odd, message?");
}

/// A module action enum with one inert variant.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
enum NoAction {
    Nothing,
}

impl ModuleAction for NoAction {
    fn risk(&self) -> Risk {
        Risk::Read
    }
    fn catalog() -> Vec<ActionDoc> {
        vec![ActionDoc {
            name: "nothing",
            description: "Does nothing.",
            risk: Risk::Read,
        }]
    }
}

#[derive(Debug, Clone, Default, Serialize)]
struct Unit;

/// A module with no tray override.
#[derive(Debug, Default)]
struct Quitter {
    crash: bool,
}

impl Module for Quitter {
    const ID: &'static str = "quitter";
    type Action = NoAction;
    type Config = NoConfig;
    type State = Unit;

    fn initial_state(&self) -> Unit {
        Unit
    }

    async fn run(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
        if self.crash {
            anyhow::bail!("refusing to start");
        }
        while let Some(req) = ctx.recv().await {
            req.respond::<()>(Ok(()));
        }
        Ok(())
    }
}

fn kernel_with(module: Quitter) -> Kernel {
    Kernel::builder(Store::open_in_memory().unwrap())
        .restart_policy(RestartPolicy {
            max_restarts: 2,
            base_delay: Duration::from_millis(5),
            max_delay: Duration::from_millis(20),
        })
        .register(module)
        .start()
}

async fn wait_until(kernel: &Kernel, pred: impl Fn(&ModuleState) -> bool) {
    for _ in 0..400 {
        if kernel
            .modules()
            .iter()
            .any(|m| m.id == "quitter" && pred(&m.state))
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("condition not reached; modules: {:?}", kernel.modules());
}

#[tokio::test]
async fn a_module_that_gives_up_shows_the_problem_glyph() {
    let kernel = kernel_with(Quitter { crash: true });
    wait_until(&kernel, |s| matches!(s, ModuleState::Failed { .. })).await;
    let state = kernel.tray(None);
    assert_eq!(state.glyph, TrayGlyph::Problem);
    assert_eq!(state.summary, "quitter stopped: refusing to start");
    kernel.shutdown().await;
}

#[tokio::test]
async fn a_module_with_no_tray_builds_and_runs_unchanged() {
    let kernel = kernel_with(Quitter::default());
    wait_until(&kernel, |s| *s == ModuleState::Running).await;
    assert_eq!(kernel.tray(None), tray(TrayGlyph::Normal, "Navori"));
    kernel.shutdown().await;
}

fn with_contribution(
    id: &str,
    state: ModuleState,
    contribution: TrayContribution,
) -> (ModuleInfo, TrayContribution) {
    let (info, _) = entry(id, state);
    (info, contribution)
}

fn glyph(glyph: TrayGlyph, sentence: &str) -> TrayContribution {
    TrayContribution {
        glyph: Some((glyph, sentence.to_owned())),
        attention: None,
    }
}

fn waiting(clause: &str) -> TrayContribution {
    TrayContribution {
        glyph: None,
        attention: Some(clause.to_owned()),
    }
}

fn running(id: &str, contribution: TrayContribution) -> (ModuleInfo, TrayContribution) {
    with_contribution(id, ModuleState::Running, contribution)
}

#[test]
fn problem_beats_lease_beats_ai_running_beats_normal() {
    let ai = running("ai", glyph(TrayGlyph::AiRunning, "Navori AI is working"));
    let lease = running("lease", glyph(TrayGlyph::Lease, "Mac kept awake"));
    let problem = running("pr", glyph(TrayGlyph::Problem, "pr cannot reach GitHub"));

    let state = compose_tray(std::slice::from_ref(&ai), None);
    assert_eq!(state.glyph, TrayGlyph::AiRunning);
    assert_eq!(state.summary, "Navori AI is working");

    let state = compose_tray(&[ai.clone(), lease.clone()], None);
    assert_eq!(state.glyph, TrayGlyph::Lease);
    assert_eq!(state.summary, "Mac kept awake");

    let state = compose_tray(&[ai, lease, problem], None);
    assert_eq!(state.glyph, TrayGlyph::Problem);
    assert_eq!(state.summary, "pr cannot reach GitHub");
}

#[test]
fn a_stopped_or_failed_module_is_ignored_even_with_a_lease_contribution() {
    let lease = glyph(TrayGlyph::Lease, "Mac kept awake");
    let stopped = with_contribution("awake", ModuleState::Stopped, lease.clone());
    assert_eq!(
        compose_tray(&[stopped], None),
        tray(TrayGlyph::Normal, "Navori")
    );

    let failed = with_contribution(
        "awake",
        ModuleState::Failed {
            last_error: "boom".to_owned(),
        },
        lease,
    );
    assert_eq!(
        compose_tray(&[failed], None),
        tray(TrayGlyph::Problem, "awake stopped: boom")
    );
}

#[test]
fn a_failed_module_comes_before_a_problem_a_module_reports() {
    let entries = [
        running("pr", glyph(TrayGlyph::Problem, "pr cannot reach GitHub")),
        failed("awake", "no power"),
    ];
    assert_eq!(
        compose_tray(&entries, None).summary,
        "awake stopped: no power. pr cannot reach GitHub"
    );
}

#[test]
fn attention_turns_on_and_keeps_the_glyph() {
    let entries = [
        running("lease", glyph(TrayGlyph::Lease, "Mac kept awake")),
        running("pr", waiting("1 Decision waits")),
    ];
    let state = compose_tray(&entries, None);
    assert_eq!(state.glyph, TrayGlyph::Lease);
    assert!(state.attention);
    assert_eq!(state.summary, "Mac kept awake. 1 Decision waits");
}

#[test]
fn a_problem_and_attention_both_stay_in_the_state() {
    let entries = [
        failed("awake", "no power"),
        running("pr", waiting("1 Decision waits")),
    ];
    let state = compose_tray(&entries, None);
    assert_eq!(state.glyph, TrayGlyph::Problem);
    assert!(state.attention);
}

#[test]
fn the_summary_is_glyph_sentence_then_decision_then_config() {
    let entries = [
        running("lease", glyph(TrayGlyph::Lease, "Mac kept awake")),
        running("pr", waiting("1 Decision waits")),
    ];
    assert_eq!(
        compose_tray(&entries, Some(NOTICE)).summary,
        "Mac kept awake. 1 Decision waits. config.toml has bad values. Check awake.minutes."
    );
}

#[test]
fn a_sentence_ending_in_a_period_joins_without_a_double_period() {
    let entries = [
        running("lease", glyph(TrayGlyph::Lease, "Mac kept awake.")),
        running("pr", waiting("1 Decision waits.")),
    ];
    assert_eq!(
        compose_tray(&entries, None).summary,
        "Mac kept awake. 1 Decision waits."
    );
}

/// Defines a Module whose published State is its tray contribution.
macro_rules! signal_module {
    ($name:ident, $id:literal, $contribution:expr) => {
        #[derive(Debug, Default)]
        struct $name;

        impl Module for $name {
            const ID: &'static str = $id;
            type Action = NoAction;
            type Config = NoConfig;
            type State = Unit;

            fn initial_state(&self) -> Unit {
                Unit
            }

            fn tray(_state: &Unit) -> TrayContribution {
                $contribution
            }

            async fn run(&mut self, ctx: &mut ModuleCtx<Self>) -> anyhow::Result<()> {
                while let Some(req) = ctx.recv().await {
                    req.respond::<()>(Ok(()));
                }
                Ok(())
            }
        }
    };
}

signal_module!(
    Lamp,
    "lamp",
    glyph(TrayGlyph::Lease, "Lamp keeps the Mac awake")
);

static PANIC_CALLS: AtomicUsize = AtomicUsize::new(0);

signal_module!(Bomb, "bomb", {
    PANIC_CALLS.fetch_add(1, Ordering::SeqCst);
    panic!("tray blew up")
});

async fn wait_all_running(kernel: &Kernel) {
    for _ in 0..400 {
        let modules = kernel.modules();
        if modules.iter().all(|m| m.state == ModuleState::Running) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("modules not running: {:?}", kernel.modules());
}

#[tokio::test]
async fn a_module_adds_a_glyph_with_no_edit_to_the_composer() {
    let kernel = Kernel::builder(Store::open_in_memory().unwrap())
        .register(Lamp)
        .start();
    wait_all_running(&kernel).await;
    assert_eq!(
        kernel.tray(None),
        tray(TrayGlyph::Lease, "Lamp keeps the Mac awake")
    );
    kernel.shutdown().await;
}

#[tokio::test]
async fn a_panicking_tray_reports_nothing_and_is_called_only_once() {
    let kernel = Kernel::builder(Store::open_in_memory().unwrap())
        .register(Lamp)
        .register(Bomb)
        .start();
    wait_all_running(&kernel).await;
    for _ in 0..3 {
        assert_eq!(
            kernel.tray(None),
            tray(TrayGlyph::Lease, "Lamp keeps the Mac awake")
        );
    }
    assert_eq!(PANIC_CALLS.load(Ordering::SeqCst), 1);
    kernel.shutdown().await;
}
