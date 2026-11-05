use std::collections::BTreeMap;

use super::baseline::{Baseline, Machine};

#[derive(Debug, Clone, Copy)]
pub enum Kind {
    Gated { budget: f64 },
    BaselineOnly,
}

#[derive(Debug, Clone, Copy)]
pub struct MetricSpec {
    pub name: &'static str,
    pub unit: &'static str,
    pub kind: Kind,
    pub floor: f64,
}

pub const METRICS: &[MetricSpec] = &[
    MetricSpec {
        name: "cold_start_ms",
        unit: "ms",
        kind: Kind::Gated { budget: 200.0 },
        floor: 10.0,
    },
    MetricSpec {
        name: "menu_open_after_launch_ms",
        unit: "ms",
        kind: Kind::Gated { budget: 80.0 },
        floor: 5.0,
    },
    MetricSpec {
        name: "menu_reopen_ms",
        unit: "ms",
        kind: Kind::Gated { budget: 50.0 },
        floor: 5.0,
    },
    MetricSpec {
        name: "footprint_menu_open_mb",
        unit: "MB",
        kind: Kind::Gated { budget: 60.0 },
        floor: 1.0,
    },
    MetricSpec {
        name: "idle_cpu_pct",
        unit: "%",
        kind: Kind::Gated { budget: 0.0 },
        floor: 0.1,
    },
    MetricSpec {
        name: "idle_wakeups_per_s",
        unit: "/s",
        kind: Kind::Gated { budget: 1.0 },
        floor: 0.2,
    },
    MetricSpec {
        name: "idle_footprint_mb",
        unit: "MB",
        kind: Kind::Gated { budget: 30.0 },
        floor: 1.0,
    },
    MetricSpec {
        name: "app_size_mb",
        unit: "MB",
        kind: Kind::Gated { budget: 25.0 },
        floor: 0.5,
    },
    MetricSpec {
        name: "peak_footprint_mb",
        unit: "MB",
        kind: Kind::BaselineOnly,
        floor: 2.0,
    },
    MetricSpec {
        name: "idle_disk_writes_kb",
        unit: "KiB",
        kind: Kind::BaselineOnly,
        floor: 64.0,
    },
    MetricSpec {
        name: "menu_open_cpu_ms",
        unit: "ms",
        kind: Kind::BaselineOnly,
        floor: 5.0,
    },
    MetricSpec {
        name: "thread_count",
        unit: "threads",
        kind: Kind::BaselineOnly,
        floor: 2.0,
    },
    MetricSpec {
        name: "submenu_open_ms",
        unit: "ms",
        kind: Kind::Gated { budget: 16.0 },
        floor: 5.0,
    },
    MetricSpec {
        name: "submenu_intent_open_ms",
        unit: "ms",
        kind: Kind::Gated { budget: 80.0 },
        floor: 10.0,
    },
    MetricSpec {
        name: "snapshot_ms",
        unit: "ms",
        kind: Kind::Gated { budget: 10.0 },
        floor: 2.0,
    },
    MetricSpec {
        name: "port_scan_ms",
        unit: "ms",
        kind: Kind::Gated { budget: 50.0 },
        floor: 5.0,
    },
];

pub type Measurements = BTreeMap<String, f64>;

pub fn median(xs: &[f64]) -> Option<f64> {
    if xs.is_empty() {
        return None;
    }
    let mut sorted = xs.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        Some(sorted[mid])
    } else {
        Some((sorted[mid - 1] + sorted[mid]) / 2.0)
    }
}

#[derive(Debug)]
pub struct Verdict {
    pub lines: Vec<String>,
    pub warnings: Vec<String>,
    pub passed: bool,
}

pub fn judge(m: &Measurements, baseline: Option<&Baseline>, machine: &Machine) -> Verdict {
    let mut warnings = Vec::new();
    let baseline = baseline_for_regression(baseline, machine, &mut warnings);
    let mut lines = Vec::with_capacity(METRICS.len());
    let mut passed = true;
    for spec in METRICS {
        match spec.kind {
            Kind::BaselineOnly => match m.get(spec.name) {
                None => {
                    lines.push(missing_metric(spec.name));
                    passed = false;
                }
                Some(&value) => {
                    let mut line = format!("{} {value} {} baseline-only", spec.name, spec.unit);
                    note_regression(spec, value, baseline, &mut line, &mut passed);
                    lines.push(line);
                }
            },
            Kind::Gated { budget } => match m.get(spec.name) {
                None => {
                    lines.push(missing_metric(spec.name));
                    passed = false;
                }
                Some(&value) => {
                    let comparable = if spec.name == "idle_cpu_pct" {
                        (value * 10.0).round() / 10.0
                    } else {
                        value
                    };
                    if comparable > budget {
                        passed = false;
                    }
                    let mut line = format!("{} {value} {} budget {budget}", spec.name, spec.unit);
                    if spec.name == "footprint_menu_open_mb"
                        && let Some(&modules) = m.get("modules_running")
                    {
                        line.push_str(&format!(" modules: {modules}"));
                    }
                    note_regression(spec, value, baseline, &mut line, &mut passed);
                    lines.push(line);
                }
            },
        }
    }
    Verdict {
        lines,
        warnings,
        passed,
    }
}

fn baseline_for_regression<'a>(
    baseline: Option<&'a Baseline>,
    machine: &Machine,
    warnings: &mut Vec<String>,
) -> Option<&'a Baseline> {
    let Some(baseline) = baseline else {
        warnings.push("regression check skipped: no baseline".to_owned());
        return None;
    };
    let mut reasons = Vec::new();
    if baseline.machine.model != machine.model {
        reasons.push(format!(
            "baseline machine is {}, this Mac is {}",
            baseline.machine.model, machine.model
        ));
    }
    if baseline.machine.macos != machine.macos {
        reasons.push(format!(
            "baseline macOS is {}, this Mac is {}",
            baseline.machine.macos, machine.macos
        ));
    }
    if reasons.is_empty() {
        Some(baseline)
    } else {
        warnings.push(format!("regression check skipped: {}", reasons.join("; ")));
        None
    }
}

fn note_regression(
    spec: &MetricSpec,
    value: f64,
    baseline: Option<&Baseline>,
    line: &mut String,
    passed: &mut bool,
) {
    let Some(baseline) = baseline else {
        return;
    };
    let Some(&(median, _)) = baseline.metrics.get(spec.name) else {
        *passed = false;
        line.push_str(" missing from baseline");
        return;
    };
    // Floor comes from the metric table, not the recorded file, so a stale file cannot loosen it.
    let threshold = median + f64::max(median * 0.10, spec.floor);
    if value > threshold {
        *passed = false;
        line.push_str(&format!(" threshold {threshold}"));
    }
}

fn missing_metric(name: &str) -> String {
    format!("no line for an enabled metric: {name}")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::super::baseline::{Baseline, Machine};
    use super::{Kind, METRICS, Measurements, judge, median};

    fn machine() -> Machine {
        Machine {
            model: "test-model".to_owned(),
            macos: "15.0".to_owned(),
        }
    }

    fn within_budget() -> Measurements {
        Measurements::from([
            ("cold_start_ms".to_owned(), 200.0),
            ("menu_open_after_launch_ms".to_owned(), 80.0),
            ("menu_reopen_ms".to_owned(), 50.0),
            ("footprint_menu_open_mb".to_owned(), 60.0),
            ("idle_cpu_pct".to_owned(), 0.0),
            ("idle_wakeups_per_s".to_owned(), 1.0),
            ("idle_footprint_mb".to_owned(), 30.0),
            ("app_size_mb".to_owned(), 25.0),
            ("peak_footprint_mb".to_owned(), 2.0),
            ("idle_disk_writes_kb".to_owned(), 64.0),
            ("menu_open_cpu_ms".to_owned(), 5.0),
            ("thread_count".to_owned(), 2.0),
            ("submenu_open_ms".to_owned(), 16.0),
            ("submenu_intent_open_ms".to_owned(), 80.0),
            ("snapshot_ms".to_owned(), 10.0),
            ("port_scan_ms".to_owned(), 50.0),
        ])
    }

    #[test]
    fn median_of_odd_and_even_counts() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(&[4.0, 1.0, 3.0, 2.0]), Some(2.5));
        assert_eq!(median(&[]), None);
    }

    #[test]
    fn all_within_budget_passes() {
        let verdict = judge(&within_budget(), None, &machine());
        assert!(verdict.passed, "{:?}", verdict.lines);
        assert_eq!(
            verdict.warnings,
            vec!["regression check skipped: no baseline".to_owned()]
        );
        let names = [
            "cold_start_ms",
            "menu_open_after_launch_ms",
            "menu_reopen_ms",
            "footprint_menu_open_mb",
            "idle_cpu_pct",
            "idle_wakeups_per_s",
            "idle_footprint_mb",
            "app_size_mb",
            "peak_footprint_mb",
            "idle_disk_writes_kb",
            "menu_open_cpu_ms",
            "thread_count",
            "submenu_open_ms",
            "submenu_intent_open_ms",
            "snapshot_ms",
            "port_scan_ms",
        ];
        assert_eq!(verdict.lines.len(), names.len(), "{:?}", verdict.lines);
        for (line, name) in verdict.lines.iter().zip(names) {
            assert!(line.contains(name), "{line} missing {name}");
        }
    }

    #[test]
    fn over_budget_fails_naming_metric_value_and_budget() {
        let mut measurements = within_budget();
        measurements.insert("cold_start_ms".to_owned(), 201.0);
        let verdict = judge(&measurements, None, &machine());
        assert!(
            verdict.lines.iter().any(|line| {
                line.contains("cold_start_ms") && line.contains("201") && line.contains("200")
            }),
            "{:?}",
            verdict.lines
        );
        assert!(!verdict.passed, "{:?}", verdict.lines);
    }

    #[test]
    fn idle_cpu_rounds_to_one_decimal() {
        let mut under = within_budget();
        under.insert("idle_cpu_pct".to_owned(), 0.04);
        let under = judge(&under, None, &machine());
        assert!(under.passed, "{:?}", under.lines);

        let mut over = within_budget();
        over.insert("idle_cpu_pct".to_owned(), 0.05);
        let over = judge(&over, None, &machine());
        assert!(!over.passed, "{:?}", over.lines);
        assert!(
            over.lines
                .iter()
                .any(|line| line.contains("idle_cpu_pct") && line.contains("0.05")),
            "{:?}",
            over.lines
        );
    }

    #[test]
    fn no_metric_reports_itself_as_skipped() {
        let verdict = judge(&within_budget(), None, &machine());
        assert!(
            verdict.lines.iter().all(|line| !line.contains("skipped")),
            "{:?}",
            verdict.lines
        );
        assert!(verdict.passed, "{:?}", verdict.lines);
    }

    #[test]
    fn an_intent_open_at_the_old_150_ms_timer_fails_the_run() {
        // The re-armed 150 ms timer plus about 8 ms of open cost.
        let mut measurements = within_budget();
        measurements.insert("submenu_intent_open_ms".to_owned(), 158.0);
        let verdict = judge(&measurements, None, &machine());
        assert!(!verdict.passed, "{:?}", verdict.lines);
        assert!(
            verdict.lines.iter().any(|line| {
                line.contains("submenu_intent_open_ms") && line.contains("158") && line.contains("80")
            }),
            "{:?}",
            verdict.lines
        );
        measurements.insert("submenu_intent_open_ms".to_owned(), 58.0);
        assert!(judge(&measurements, None, &machine()).passed);
    }

    #[test]
    fn a_submenu_open_over_sixteen_ms_fails_the_run() {
        let mut measurements = within_budget();
        measurements.insert("submenu_open_ms".to_owned(), 17.0);
        let verdict = judge(&measurements, None, &machine());
        assert!(
            verdict.lines.iter().any(|line| {
                line.contains("submenu_open_ms") && line.contains("17") && line.contains("16")
            }),
            "{:?}",
            verdict.lines
        );
        assert!(!verdict.passed, "{:?}", verdict.lines);
    }

    #[test]
    fn a_missing_submenu_measurement_fails_the_run() {
        let mut measurements = within_budget();
        measurements.remove("submenu_open_ms");
        let verdict = judge(&measurements, None, &machine());
        assert!(
            verdict
                .lines
                .iter()
                .any(|line| line.contains("no line for an enabled metric: submenu_open_ms")),
            "{:?}",
            verdict.lines
        );
        assert!(!verdict.passed, "{:?}", verdict.lines);
    }

    #[test]
    fn missing_enabled_metric_fails() {
        let mut measurements = within_budget();
        measurements.remove("menu_reopen_ms");
        let verdict = judge(&measurements, None, &machine());
        assert!(
            verdict
                .lines
                .iter()
                .any(|line| line.contains("no line for an enabled metric: menu_reopen_ms")),
            "{:?}",
            verdict.lines
        );
        assert!(!verdict.passed, "{:?}", verdict.lines);
    }

    #[test]
    fn baseline_only_metric_is_labelled_and_never_budget_fails() {
        let mut measurements = within_budget();
        for name in [
            "peak_footprint_mb",
            "idle_disk_writes_kb",
            "menu_open_cpu_ms",
            "thread_count",
        ] {
            measurements.insert(name.to_owned(), 1_000_000.0);
        }
        let verdict = judge(&measurements, None, &machine());
        for name in [
            "peak_footprint_mb",
            "idle_disk_writes_kb",
            "menu_open_cpu_ms",
            "thread_count",
        ] {
            assert!(
                verdict
                    .lines
                    .iter()
                    .any(|line| line.contains(name) && line.contains("baseline-only")),
                "{name} in {:?}",
                verdict.lines
            );
        }
        assert!(verdict.passed, "{:?}", verdict.lines);
    }

    #[test]
    fn menu_open_footprint_line_shows_running_modules() {
        let mut measurements = within_budget();
        measurements.insert("modules_running".to_owned(), 1.0);
        let verdict = judge(&measurements, None, &machine());
        assert!(
            verdict.lines.iter().any(|line| {
                line.contains("footprint_menu_open_mb") && line.contains("modules: 1")
            }),
            "{:?}",
            verdict.lines
        );
    }

    fn baseline_on(
        model: &str,
        macos: &str,
        metric: &str,
        median_value: f64,
        floor: f64,
    ) -> Baseline {
        let measured = within_budget();
        let mut metrics = BTreeMap::new();
        for spec in METRICS {
            let value = measured[spec.name];
            metrics.insert(spec.name.to_owned(), (value, spec.floor));
        }
        metrics.insert(metric.to_owned(), (median_value, floor));
        Baseline {
            machine: Machine {
                model: model.to_owned(),
                macos: macos.to_owned(),
            },
            revision: "rev".to_owned(),
            recorded: "2026-11-01".to_owned(),
            metrics,
        }
    }

    fn baseline_with(metric: &str, median_value: f64, floor: f64) -> Baseline {
        baseline_on("test-model", "15.0", metric, median_value, floor)
    }

    #[test]
    fn regression_beyond_ten_percent_fails() {
        let baseline = baseline_with("cold_start_ms", 100.0, 10.0);
        let mut at_limit = within_budget();
        at_limit.insert("cold_start_ms".to_owned(), 110.0);
        let at_limit = judge(&at_limit, Some(&baseline), &machine());
        assert!(at_limit.passed, "{:?}", at_limit.lines);
        assert!(at_limit.warnings.is_empty(), "{:?}", at_limit.warnings);

        let mut over = within_budget();
        over.insert("cold_start_ms".to_owned(), 111.0);
        let over = judge(&over, Some(&baseline), &machine());
        assert!(!over.passed, "{:?}", over.lines);
        assert!(
            over.lines.iter().any(|line| {
                line.contains("cold_start_ms")
                    && line.contains("111")
                    && line.contains("threshold 110")
            }),
            "{:?}",
            over.lines
        );
    }

    #[test]
    fn matching_baseline_missing_an_enabled_metric_fails() {
        let mut baseline = baseline_with("cold_start_ms", 100.0, 10.0);
        baseline.metrics.remove("menu_reopen_ms");
        let verdict = judge(&within_budget(), Some(&baseline), &machine());
        assert!(!verdict.passed, "{:?}", verdict.lines);
        assert!(
            verdict.lines.iter().any(|line| {
                line.contains("menu_reopen_ms") && line.contains("missing from baseline")
            }),
            "{:?}",
            verdict.lines
        );
    }

    #[test]
    fn metrics_match_the_performance_standard() {
        let standard = include_str!("../../../docs/standards/performance.md");
        let expected: &[(&str, f64, Option<f64>, &str)] = &[
            ("cold_start_ms", 10.0, Some(200.0), "≤ 200 ms"),
            ("menu_open_after_launch_ms", 5.0, Some(80.0), "≤ 80 ms"),
            ("menu_reopen_ms", 5.0, Some(50.0), "≤ 50 ms"),
            ("footprint_menu_open_mb", 1.0, Some(60.0), "≤ 60 MB"),
            ("idle_cpu_pct", 0.1, Some(0.0), "0%"),
            ("idle_wakeups_per_s", 0.2, Some(1.0), "≤ 1 / s"),
            ("idle_footprint_mb", 1.0, Some(30.0), "≤ 30 MB"),
            ("app_size_mb", 0.5, Some(25.0), "≤ 25 MB"),
            ("peak_footprint_mb", 2.0, None, "2 MB `peak_footprint_mb`"),
            (
                "idle_disk_writes_kb",
                64.0,
                None,
                "64 KiB `idle_disk_writes_kb`",
            ),
            (
                "menu_open_cpu_ms",
                5.0,
                None,
                "5 ms `menu_open_after_launch_ms`",
            ),
            ("thread_count", 2.0, None, "2 threads `thread_count`"),
            ("submenu_open_ms", 5.0, Some(16.0), "≤ 16 ms"),
            (
                "submenu_intent_open_ms",
                10.0,
                Some(80.0),
                "Submenu open from pointer entry | ≤ 80 ms",
            ),
            ("snapshot_ms", 2.0, Some(10.0), "≤ 10 ms"),
            ("port_scan_ms", 5.0, Some(50.0), "≤ 50 ms"),
        ];
        assert_eq!(METRICS.len(), expected.len());
        for (spec, (name, floor, budget, phrase)) in METRICS.iter().zip(expected) {
            assert_eq!(spec.name, *name);
            assert_eq!(spec.floor, *floor);
            match (spec.kind, budget) {
                (Kind::Gated { budget: actual }, Some(want)) => assert_eq!(actual, *want),
                (Kind::BaselineOnly, None) => {}
                (kind, want) => panic!("{name} kind {kind:?} budget {want:?}"),
            }
            assert!(
                standard.contains(phrase),
                "{name} phrase {phrase:?} missing from the standard"
            );
        }
    }

    #[test]
    fn the_standard_names_the_port_scan_floor() {
        let standard = include_str!("../../../docs/standards/performance.md");
        let floors = standard
            .lines()
            .find(|line| line.starts_with("- Floors:"))
            .unwrap();
        assert!(floors.contains("`port_scan_ms`"), "{floors}");
    }

    #[test]
    fn floor_wins_for_small_values() {
        // The stored floor is 100, not the 5 ms spec floor. A judge that
        // reads the file floor would allow 25.1.
        let baseline = baseline_with("menu_open_cpu_ms", 20.0, 100.0);
        let mut at_limit = within_budget();
        at_limit.insert("menu_open_cpu_ms".to_owned(), 25.0);
        let at_limit = judge(&at_limit, Some(&baseline), &machine());
        assert!(at_limit.passed, "{:?}", at_limit.lines);

        let mut over = within_budget();
        over.insert("menu_open_cpu_ms".to_owned(), 25.1);
        let over = judge(&over, Some(&baseline), &machine());
        assert!(!over.passed, "{:?}", over.lines);
        assert!(
            over.lines.iter().any(|line| {
                line.contains("menu_open_cpu_ms")
                    && line.contains("25.1")
                    && line.contains("threshold 25")
            }),
            "{:?}",
            over.lines
        );
    }

    #[test]
    fn zero_baseline_uses_floor() {
        let baseline = baseline_with("idle_wakeups_per_s", 0.0, 0.2);
        let mut at_limit = within_budget();
        at_limit.insert("idle_wakeups_per_s".to_owned(), 0.2);
        let at_limit = judge(&at_limit, Some(&baseline), &machine());
        assert!(at_limit.passed, "{:?}", at_limit.lines);

        let mut over = within_budget();
        over.insert("idle_wakeups_per_s".to_owned(), 0.3);
        let over = judge(&over, Some(&baseline), &machine());
        assert!(!over.passed, "{:?}", over.lines);
        assert!(
            over.lines.iter().any(|line| {
                line.contains("idle_wakeups_per_s")
                    && line.contains("0.3")
                    && line.contains("threshold 0.2")
            }),
            "{:?}",
            over.lines
        );
    }

    #[test]
    fn other_machine_or_macos_checks_budget_only_with_warning() {
        let mut measurements = within_budget();
        measurements.insert("cold_start_ms".to_owned(), 111.0);
        let other_model = baseline_on("OtherMac", "15.0", "cold_start_ms", 100.0, 10.0);
        let verdict = judge(&measurements, Some(&other_model), &machine());
        assert!(verdict.passed, "{:?}", verdict.lines);
        assert_eq!(
            verdict.warnings,
            vec![
                "regression check skipped: baseline machine is OtherMac, this Mac is test-model"
                    .to_owned()
            ]
        );

        let other_os = baseline_on("test-model", "14.0", "cold_start_ms", 100.0, 10.0);
        let verdict = judge(&measurements, Some(&other_os), &machine());
        assert!(verdict.passed, "{:?}", verdict.lines);
        assert_eq!(
            verdict.warnings,
            vec!["regression check skipped: baseline macOS is 14.0, this Mac is 15.0".to_owned()]
        );

        let both = baseline_on("OtherMac", "14.0", "cold_start_ms", 100.0, 10.0);
        let verdict = judge(&measurements, Some(&both), &machine());
        assert!(verdict.passed, "{:?}", verdict.lines);
        assert_eq!(
            verdict.warnings,
            vec![
                "regression check skipped: baseline machine is OtherMac, this Mac is test-model; baseline macOS is 14.0, this Mac is 15.0"
                    .to_owned()
            ]
        );

        measurements.insert("cold_start_ms".to_owned(), 201.0);
        let verdict = judge(&measurements, Some(&other_model), &machine());
        assert!(!verdict.passed, "{:?}", verdict.lines);
        assert!(
            verdict.lines.iter().any(|line| {
                line.contains("cold_start_ms") && line.contains("201") && line.contains("200")
            }),
            "{:?}",
            verdict.lines
        );
    }

    #[test]
    fn missing_baseline_warns_and_checks_budget_only() {
        let verdict = judge(&within_budget(), None, &machine());
        assert!(verdict.passed, "{:?}", verdict.lines);
        assert_eq!(
            verdict.warnings,
            vec!["regression check skipped: no baseline".to_owned()]
        );

        let mut over = within_budget();
        over.insert("cold_start_ms".to_owned(), 201.0);
        let over = judge(&over, None, &machine());
        assert!(!over.passed, "{:?}", over.lines);
        assert_eq!(
            over.warnings,
            vec!["regression check skipped: no baseline".to_owned()]
        );
    }
}
