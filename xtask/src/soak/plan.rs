use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Tier {
    Short,
    Long { load: Duration },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LoadStep {
    CommandLease,
    RenewLease,
    ReleaseAll,
    OpenListener {
        hold: Duration,
    },
    HoldStore {
        renew_after: Duration,
        probe_after: Duration,
        commit_after: Duration,
    },
    ProbeCore,
    Sample,
    ProbeMenu,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Step {
    pub at: Duration,
    pub load: LoadStep,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Plan {
    pub settle: Duration,
    pub steps: Vec<Step>,
}

#[derive(Debug)]
struct Rates {
    lease: u64,
    release_all: u64,
    release_all_phase: u64,
    listener: u64,
    store: u64,
    sample: u64,
}

const SETTLE_SECS: u64 = 60;
const SHORT_LOAD_SECS: u64 = 1200;
const LISTENER_HOLD_SECS: u64 = 45;
const STORE_PHASE_SECS: u64 = 1;
const CORE_PROBE_SECS: u64 = 30;

const SHORT: Rates = Rates {
    lease: 5,
    release_all: 60,
    release_all_phase: 30,
    listener: 20,
    store: 120,
    sample: 10,
};

const LONG: Rates = Rates {
    lease: 25,
    release_all: 300,
    release_all_phase: 150,
    listener: 100,
    store: 600,
    sample: 60,
};

pub(crate) fn plan(tier: &Tier) -> Plan {
    let (rates, load) = match tier {
        Tier::Short => (&SHORT, Duration::from_secs(SHORT_LOAD_SECS)),
        Tier::Long { load } => (&LONG, *load),
    };
    let settle = Duration::from_secs(SETTLE_SECS);
    let end = settle + load;
    let mut steps = Vec::new();
    let mut series = |phase: u64, period: u64, events: &[LoadStep]| {
        let mut at = settle + Duration::from_secs(phase);
        while at < end {
            for load in events {
                steps.push(Step {
                    at,
                    load: load.clone(),
                });
            }
            at += Duration::from_secs(period);
        }
    };
    series(
        0,
        rates.lease,
        &[LoadStep::CommandLease, LoadStep::RenewLease],
    );
    series(
        rates.release_all_phase,
        rates.release_all,
        &[LoadStep::ReleaseAll],
    );
    series(
        0,
        rates.listener,
        &[LoadStep::OpenListener {
            hold: Duration::from_secs(LISTENER_HOLD_SECS),
        }],
    );
    series(
        STORE_PHASE_SECS,
        rates.store,
        &[LoadStep::HoldStore {
            renew_after: Duration::from_millis(500),
            probe_after: Duration::from_secs(1),
            commit_after: Duration::from_secs(4),
        }],
    );
    series(0, CORE_PROBE_SECS, &[LoadStep::ProbeCore]);
    series(0, rates.sample, &[LoadStep::Sample]);
    steps.push(Step {
        at: end,
        load: LoadStep::ProbeMenu,
    });
    steps.sort_by_key(|s| s.at);
    Plan { settle, steps }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    fn short() -> Plan {
        plan(&Tier::Short)
    }

    fn long() -> Plan {
        plan(&Tier::Long { load: secs(7200) })
    }

    fn ats(plan: &Plan, pick: impl Fn(&LoadStep) -> bool) -> Vec<Duration> {
        plan.steps
            .iter()
            .filter(|s| pick(&s.load))
            .map(|s| s.at)
            .collect()
    }

    fn gaps(at: &[Duration]) -> Vec<Duration> {
        at.windows(2).map(|w| w[1] - w[0]).collect()
    }

    #[test]
    fn the_short_tier_settles_for_a_minute_then_loads_for_twenty() {
        let p = short();
        assert_eq!(p.settle, secs(60));
        assert!(p.steps.iter().all(|s| s.at >= secs(60)));
    }

    #[test]
    fn the_short_tier_takes_a_command_lease_every_five_seconds() {
        let at = ats(&short(), |e| matches!(e, LoadStep::CommandLease));
        assert_eq!(at.len(), 240);
        assert!(gaps(&at).iter().all(|g| *g == secs(5)));
    }

    #[test]
    fn the_short_tier_renews_the_lease_on_every_command_lease_tick() {
        let p = short();
        let lease = ats(&p, |e| matches!(e, LoadStep::CommandLease));
        let renew = ats(&p, |e| matches!(e, LoadStep::RenewLease));
        assert_eq!(lease, renew);
    }

    #[test]
    fn the_short_tier_releases_all_every_minute_at_half_past() {
        let at = ats(&short(), |e| matches!(e, LoadStep::ReleaseAll));
        assert_eq!(at.len(), 20);
        let offsets: Vec<Duration> = at.iter().map(|a| *a - secs(60)).collect();
        assert_eq!(&offsets[..3], &[secs(30), secs(90), secs(150)]);
        assert!(gaps(&at).iter().all(|g| *g == secs(60)));
    }

    #[test]
    fn the_short_tier_opens_a_listener_every_twenty_seconds_for_forty_five() {
        let p = short();
        let at = ats(&p, |e| matches!(e, LoadStep::OpenListener { .. }));
        assert_eq!(at.len(), 60);
        assert!(gaps(&at).iter().all(|g| *g == secs(20)));
        assert!(p.steps.iter().all(|s| match s.load {
            LoadStep::OpenListener { hold } => hold == secs(45),
            _ => true,
        }));
    }

    #[test]
    fn the_short_tier_holds_the_store_every_two_minutes_one_second_after_a_lease_tick() {
        let p = short();
        let at = ats(&p, |e| matches!(e, LoadStep::HoldStore { .. }));
        let expected: Vec<Duration> = (0..10).map(|k| secs(60 + 120 * k + 1)).collect();
        assert_eq!(at, expected);
        assert!(p.steps.iter().all(|s| match s.load {
            LoadStep::HoldStore {
                renew_after,
                probe_after,
                commit_after,
            } => {
                renew_after == Duration::from_millis(500)
                    && probe_after == secs(1)
                    && commit_after == secs(4)
            }
            _ => true,
        }));
    }

    #[test]
    fn every_store_hold_renews_before_its_probe() {
        for p in [short(), long()] {
            let mut holds = 0;
            for s in &p.steps {
                if let LoadStep::HoldStore {
                    renew_after,
                    probe_after,
                    commit_after,
                } = s.load
                {
                    holds += 1;
                    assert!(renew_after < probe_after);
                    assert!(probe_after < commit_after);
                }
            }
            assert!(holds > 0);
        }
    }

    #[test]
    fn the_core_is_probed_every_thirty_seconds() {
        let at = ats(&short(), |e| matches!(e, LoadStep::ProbeCore));
        assert_eq!(at.len(), 40);
        assert_eq!(at[0], secs(60));
        assert!(gaps(&at).iter().all(|g| *g == secs(30)));
    }

    #[test]
    fn the_short_tier_samples_every_ten_seconds() {
        let at = ats(&short(), |e| matches!(e, LoadStep::Sample));
        assert_eq!(at.len(), 120);
        assert!(gaps(&at).iter().all(|g| *g == secs(10)));
    }

    #[test]
    fn the_menu_is_probed_once_and_only_after_the_last_sample() {
        for p in [short(), long()] {
            let menu = ats(&p, |e| matches!(e, LoadStep::ProbeMenu));
            let last_sample = *ats(&p, |e| matches!(e, LoadStep::Sample)).last().unwrap();
            assert_eq!(menu.len(), 1);
            assert!(menu[0] > last_sample);
        }
    }

    #[test]
    fn steps_are_sorted_by_time() {
        for p in [short(), long()] {
            assert!(p.steps.windows(2).all(|w| w[0].at <= w[1].at));
        }
    }

    #[test]
    fn the_long_tier_runs_the_same_steps_at_its_own_rates() {
        let p = long();
        assert_eq!(p.settle, secs(60));
        assert!(
            p.steps
                .iter()
                .any(|s| s.at == secs(7260) && s.load == LoadStep::ProbeMenu)
        );
        let lease = ats(&p, |e| matches!(e, LoadStep::CommandLease));
        assert!(gaps(&lease).iter().all(|g| *g == secs(25)));
        let release = ats(&p, |e| matches!(e, LoadStep::ReleaseAll));
        assert_eq!(release[0], secs(60 + 150));
        assert!(gaps(&release).iter().all(|g| *g == secs(300)));
        let listener = ats(&p, |e| matches!(e, LoadStep::OpenListener { .. }));
        assert!(gaps(&listener).iter().all(|g| *g == secs(100)));
        let store = ats(&p, |e| matches!(e, LoadStep::HoldStore { .. }));
        assert_eq!(store[0], secs(61));
        assert!(gaps(&store).iter().all(|g| *g == secs(600)));
        let sample = ats(&p, |e| matches!(e, LoadStep::Sample));
        assert_eq!(sample.len(), 120);
        assert!(gaps(&sample).iter().all(|g| *g == secs(60)));
        let core = ats(&p, |e| matches!(e, LoadStep::ProbeCore));
        assert!(gaps(&core).iter().all(|g| *g == secs(30)));
    }
}
