use std::collections::BTreeMap;

use super::baseline::{Baseline, Machine};

#[derive(Debug, Clone, Copy)]
pub enum Kind {
    Gated { budget: f64 },
    BaselineOnly,
    Skipped { reason: &'static str },
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
        kind: Kind::Gated { budget: 50.0 },
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
        unit: "KB",
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
        kind: Kind::Skipped {
            reason: "no submenu until the A5 menu",
        },
        floor: 0.0,
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
    // Budgets do not consult the baseline or the machine.
    let _ = (baseline, machine);

    let mut lines = Vec::with_capacity(METRICS.len());
    let mut passed = true;
    for spec in METRICS {
        match spec.kind {
            Kind::Skipped { reason } => {
                lines.push(format!("{} skipped: {reason}", spec.name));
            }
            Kind::BaselineOnly => match m.get(spec.name) {
                None => {
                    lines.push(missing_metric(spec.name));
                    passed = false;
                }
                Some(&value) => {
                    lines.push(format!("{} {value} {} baseline-only", spec.name, spec.unit));
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
                    lines.push(line);
                }
            },
        }
    }
    Verdict {
        lines,
        warnings: Vec::new(),
        passed,
    }
}

fn missing_metric(name: &str) -> String {
    format!("no line for an enabled metric: {name}")
}

#[cfg(test)]
mod tests {
    use super::super::baseline::Machine;
    use super::{Measurements, judge, median};

    fn machine() -> Machine {
        Machine {
            model: "test-model".to_owned(),
            macos: "15.0".to_owned(),
        }
    }

    fn within_budget() -> Measurements {
        Measurements::from([
            ("cold_start_ms".to_owned(), 200.0),
            ("menu_open_after_launch_ms".to_owned(), 50.0),
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
        assert!(verdict.warnings.is_empty(), "{:?}", verdict.warnings);
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
    fn skipped_metric_prints_reason_and_does_not_fail() {
        let verdict = judge(&within_budget(), None, &machine());
        assert!(
            verdict
                .lines
                .iter()
                .any(|line| line.contains("skipped: no submenu until the A5 menu")),
            "{:?}",
            verdict.lines
        );
        assert!(verdict.passed, "{:?}", verdict.lines);
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
}
