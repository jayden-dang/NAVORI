#![allow(clippy::unwrap_used)]

mod common;

use common::Rig;
use navori_kernel::{Module, TrayContribution, TrayGlyph};
use navori_mod_awake::{Awake, AwakeState, LeaseState};
use serde_json::json;

const SYSTEM_PROBLEM: &str = "macOS refused the request. Navori keeps trying.";

fn lease(kind: &str) -> LeaseState {
    LeaseState {
        key: "manual".to_owned(),
        kind: kind.to_owned(),
        harness: None,
        label: None,
        pid: None,
        stopped: false,
        ends_at_ms: None,
        title: None,
    }
}

fn state(leases: Vec<LeaseState>, system_held: bool, problem: Option<&str>) -> AwakeState {
    AwakeState {
        leases,
        system_held,
        display_held: false,
        since_ms: None,
        problem: problem.map(str::to_owned),
        tracking: true,
    }
}

fn tray(state: &AwakeState) -> TrayContribution {
    Awake::tray(state)
}

#[test]
fn no_lease_reports_nothing() {
    let contribution = tray(&state(Vec::new(), false, None));

    assert_eq!(contribution, TrayContribution::default());
}

#[test]
fn a_held_lease_reports_the_lease_glyph() {
    let contribution = tray(&state(vec![lease("duration")], true, None));

    assert_eq!(
        contribution.glyph,
        Some((TrayGlyph::Lease, "Keeping this Mac awake".to_owned()))
    );
    assert_eq!(contribution.attention, None);
}

#[test]
fn a_display_lease_counts_as_a_lease() {
    let contribution = tray(&state(vec![lease("display")], true, None));

    assert_eq!(
        contribution.glyph,
        Some((TrayGlyph::Lease, "Keeping this Mac awake".to_owned()))
    );
}

#[test]
fn a_refused_lease_reports_the_problem_glyph_and_says_it_cannot_hold() {
    let contribution = tray(&state(vec![lease("duration")], false, Some(SYSTEM_PROBLEM)));

    assert_eq!(
        contribution.glyph,
        Some((
            TrayGlyph::Problem,
            format!("Can't hold this Mac awake. {SYSTEM_PROBLEM}")
        ))
    );
    assert_eq!(contribution.attention, None);
}

#[test]
fn other_problems_do_not_report_the_problem_glyph() {
    let texts = [
        "macOS refused to keep the display on. Navori keeps trying.",
        "Releasing a power assertion failed.",
        "Awake could not save a lease. Navori retries in 60 seconds.",
        "Awake could not read the platform.",
        "Awake could not watch for exits.",
    ];
    for text in texts {
        let contribution = tray(&state(vec![lease("duration")], true, Some(text)));

        assert_eq!(
            contribution.glyph,
            Some((TrayGlyph::Lease, "Keeping this Mac awake".to_owned())),
            "{text}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_refusal_through_the_kernel_reports_no_assertion_and_the_problem_glyph() {
    let rig = Rig::new();
    rig.power.refuse(4);

    rig.dispatch(json!("hold_until_off")).await.unwrap();

    assert!(rig.power.held().is_empty());
    let tray = rig.kernel.tray(None);
    assert_eq!(tray.glyph, TrayGlyph::Problem);
    assert_eq!(
        tray.summary,
        format!("Can't hold this Mac awake. {SYSTEM_PROBLEM}")
    );
    rig.shutdown().await;
}
