//! The icon state composed from Module lifecycle and the config notice.
#![allow(clippy::unwrap_used)] // helpers outside #[test] fns

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
