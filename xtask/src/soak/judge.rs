use std::time::Duration;

use super::plan::Tier;

const MIB: f64 = 1024.0 * 1024.0;
const MIN_READINGS: usize = 9;
const FOOTPRINT_FLOOR_MIB: f64 = 1.0;
const THREADS_FLOOR: f64 = 2.0;
const FDS_FLOOR: f64 = 4.0;
const RELATIVE_RISE: f64 = 0.10;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Reading {
    pub at: Duration,
    pub footprint_bytes: u64,
    pub threads: u32,
    pub fds: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Side {
    Core,
    Menu,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Hang {
    pub side: Side,
    pub at: Duration,
    pub waited: Duration,
    pub load_avg: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RunFacts {
    pub settle: Duration,
    pub readings: Vec<Reading>,
    pub hangs: Vec<Hang>,
    pub exited_at: Option<Duration>,
    pub elapsed: Duration,
    pub load_errors: u32,
    pub load_timeouts: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Report {
    lines: Vec<String>,
    passed: bool,
}

impl Report {
    pub(crate) fn lines(&self) -> Vec<String> {
        self.lines.clone()
    }

    pub(crate) fn passed(&self) -> bool {
        self.passed
    }

    pub(crate) fn exit_ok(&self, tier: &Tier) -> bool {
        match tier {
            Tier::Short => self.passed,
            Tier::Long { .. } => true,
        }
    }
}

fn median(xs: &[f64]) -> f64 {
    let mut sorted = xs.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[mid]
    } else {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    }
}

fn slope_per_hour(points: &[(f64, f64)]) -> f64 {
    let n = points.len() as f64;
    let mean_x = points.iter().map(|p| p.0).sum::<f64>() / n;
    let mean_y = points.iter().map(|p| p.1).sum::<f64>() / n;
    let num: f64 = points.iter().map(|p| (p.0 - mean_x) * (p.1 - mean_y)).sum();
    let den: f64 = points.iter().map(|p| (p.0 - mean_x).powi(2)).sum();
    if den == 0.0 { 0.0 } else { num / den * 3600.0 }
}

struct Signal {
    name: &'static str,
    floor: f64,
    one_decimal: bool,
}

fn clock(d: Duration) -> String {
    let s = d.as_secs();
    format!("{:02}:{:02}", s / 60, s % 60)
}

fn signed(tenths_or_whole: i64, one_decimal: bool) -> String {
    let sign = if tenths_or_whole < 0 { '-' } else { '+' };
    let abs = tenths_or_whole.unsigned_abs();
    if one_decimal {
        format!("{sign}{}.{}", abs / 10, abs % 10)
    } else {
        format!("{sign}{abs}")
    }
}

fn plain(x: f64, one_decimal: bool) -> String {
    if one_decimal || x.fract() != 0.0 {
        format!("{x:.1}")
    } else {
        format!("{x}")
    }
}

fn signal_line(signal: &Signal, at: &[f64], values: &[f64]) -> (String, bool) {
    let base = values.len() / 3;
    let first = median(&values[..base]);
    let middle = median(&values[base..values.len() - base]);
    let last = median(&values[values.len() - base..]);
    let leak = middle >= first && last - first > (first * RELATIVE_RISE).max(signal.floor);
    let points: Vec<(f64, f64)> = at.iter().copied().zip(values.iter().copied()).collect();
    let slope = slope_per_hour(&points);
    let trend = if signal.one_decimal {
        signed((slope * 10.0).round() as i64, true)
    } else {
        signed(slope.round() as i64, false)
    };
    let verdict = if leak { "LEAK" } else { "ok" };
    let line = format!(
        "soak {} start {} end {} trend {trend}/h {verdict}",
        signal.name,
        plain(first, signal.one_decimal),
        plain(last, signal.one_decimal),
    );
    (line, leak)
}

pub(crate) fn judge(run: &RunFacts) -> Report {
    let mut lines = Vec::new();
    let mut passed = true;
    let settled: Vec<&Reading> = run.readings.iter().filter(|r| r.at >= run.settle).collect();
    if settled.len() < MIN_READINGS {
        lines.push(format!("soak too few samples ({})", settled.len()));
        passed = false;
    } else {
        let at: Vec<f64> = settled.iter().map(|r| r.at.as_secs_f64()).collect();
        let series: [(Signal, Vec<f64>); 3] = [
            (
                Signal {
                    name: "footprint",
                    floor: FOOTPRINT_FLOOR_MIB,

                    one_decimal: true,
                },
                settled
                    .iter()
                    .map(|r| r.footprint_bytes as f64 / MIB)
                    .collect(),
            ),
            (
                Signal {
                    name: "threads",
                    floor: THREADS_FLOOR,

                    one_decimal: false,
                },
                settled.iter().map(|r| f64::from(r.threads)).collect(),
            ),
            (
                Signal {
                    name: "fds",
                    floor: FDS_FLOOR,

                    one_decimal: false,
                },
                settled.iter().map(|r| f64::from(r.fds)).collect(),
            ),
        ];
        for (signal, values) in &series {
            let (line, leak) = signal_line(signal, &at, values);
            lines.push(line);
            passed &= !leak;
        }
    }
    for hang in &run.hangs {
        let side = match hang.side {
            Side::Core => "core",
            Side::Menu => "menu",
        };
        lines.push(format!(
            "soak hang {side} at +{} waited {} load {:.2}",
            clock(hang.at),
            hang.waited.as_millis(),
            hang.load_avg
        ));
        passed = false;
    }
    if let Some(at) = run.exited_at {
        lines.push(format!("soak app exited at +{}", clock(at)));
        passed = false;
    }
    lines.push(format!(
        "soak load errors {} timeouts {}",
        run.load_errors, run.load_timeouts
    ));
    lines.push(format!("soak elapsed {}", clock(run.elapsed)));
    lines.push(format!(
        "soak verdict {}",
        if passed { "pass" } else { "fail" }
    ));
    Report { lines, passed }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soak::plan::Tier;
    use std::time::Duration;

    const MIB: u64 = 1024 * 1024;

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    fn readings(series: &[(f64, u32, u32)]) -> Vec<Reading> {
        series
            .iter()
            .enumerate()
            .map(|(i, &(mib, threads, fds))| Reading {
                at: secs(60 + 10 * i as u64),
                footprint_bytes: (mib * MIB as f64) as u64,
                threads,
                fds,
            })
            .collect()
    }

    fn run(readings: Vec<Reading>) -> RunFacts {
        RunFacts {
            settle: secs(60),
            readings,
            hangs: vec![],
            exited_at: None,
            elapsed: secs(180),
            load_errors: 0,
            load_timeouts: 0,
        }
    }

    fn fds(values: &[u32]) -> RunFacts {
        run(readings(
            &values.iter().map(|&f| (30.0, 9, f)).collect::<Vec<_>>(),
        ))
    }

    fn threads(values: &[u32]) -> RunFacts {
        run(readings(
            &values.iter().map(|&t| (30.0, t, 40)).collect::<Vec<_>>(),
        ))
    }

    fn footprint(thirds: [f64; 3]) -> RunFacts {
        let series: Vec<_> = thirds
            .iter()
            .flat_map(|&m| std::iter::repeat_n((m, 9, 40), 4))
            .collect();
        run(readings(&series))
    }

    fn lines(run: &RunFacts) -> Vec<String> {
        judge(run).lines()
    }

    fn has(lines: &[String], want: &str) -> bool {
        lines.iter().any(|l| l == want)
    }

    #[test]
    fn flat_signals_pass_and_print_start_end_and_trend() {
        let r = run(readings(&[(30.0, 9, 40); 12]));
        let l = lines(&r);
        assert!(
            has(&l, "soak footprint start 30.0 end 30.0 trend +0.0/h ok"),
            "{l:?}"
        );
        assert!(has(&l, "soak threads start 9 end 9 trend +0/h ok"), "{l:?}");
        assert!(has(&l, "soak fds start 40 end 40 trend +0/h ok"), "{l:?}");
        assert!(has(&l, "soak verdict pass"), "{l:?}");
        assert!(judge(&r).passed());
    }

    #[test]
    fn one_more_fd_every_reading_is_a_leak_named_with_its_slope() {
        let values: Vec<u32> = (40..52).collect();
        let r = fds(&values);
        let l = lines(&r);
        assert!(
            has(&l, "soak fds start 41.5 end 49.5 trend +360/h LEAK"),
            "{l:?}"
        );
        assert_eq!(l.last().unwrap(), "soak verdict fail");
        assert!(!judge(&r).passed());
    }

    #[test]
    fn a_rise_within_the_floor_is_not_a_leak() {
        let l = lines(&threads(&[9, 9, 10, 10, 10, 10, 10, 10, 11, 11, 11, 11]));
        assert!(
            l.iter()
                .any(|x| x.starts_with("soak threads") && x.ends_with(" ok")),
            "{l:?}"
        );
        assert!(has(&l, "soak verdict pass"), "{l:?}");
    }

    #[test]
    fn a_rise_that_falls_back_in_the_middle_is_not_a_leak() {
        let mut v = vec![40; 4];
        v.extend([30; 4]);
        v.extend([50; 4]);
        let l = lines(&fds(&v));
        assert!(
            l.iter()
                .any(|x| x.starts_with("soak fds") && x.ends_with(" ok")),
            "{l:?}"
        );
    }

    #[test]
    fn ten_percent_of_a_large_footprint_outweighs_its_floor() {
        let l = lines(&footprint([40.0, 41.5, 43.0]));
        assert!(
            l.iter()
                .any(|x| x.starts_with("soak footprint") && x.ends_with(" ok")),
            "{l:?}"
        );
        let l = lines(&footprint([40.0, 42.0, 45.0]));
        assert!(
            l.iter()
                .any(|x| x.starts_with("soak footprint") && x.ends_with(" LEAK")),
            "{l:?}"
        );
    }

    #[test]
    fn fewer_than_nine_readings_fail_without_judging() {
        let r = run(readings(&[(30.0, 9, 40); 8]));
        let l = lines(&r);
        assert_eq!(l[0], "soak too few samples (8)");
        assert_eq!(l.last().unwrap(), "soak verdict fail");
        assert!(
            !l.iter()
                .any(|x| x.starts_with("soak footprint") || x.starts_with("soak fds"))
        );
        assert!(!judge(&r).passed());
    }

    #[test]
    fn a_core_hang_fails_and_says_when_and_how_long() {
        let mut r = run(readings(&[(30.0, 9, 40); 12]));
        r.hangs.push(Hang {
            side: Side::Core,
            at: secs(754),
            waited: Duration::from_millis(2000),
            load_avg: 3.2,
        });
        let l = lines(&r);
        let i = l
            .iter()
            .position(|x| x == "soak hang core at +12:34 waited 2000 load 3.20");
        assert!(i.is_some(), "{l:?}");
        assert_eq!(l.last().unwrap(), "soak verdict fail");
    }

    #[test]
    fn a_menu_hang_names_the_menu_side() {
        let mut r = run(readings(&[(30.0, 9, 40); 12]));
        r.hangs.push(Hang {
            side: Side::Menu,
            at: secs(61),
            waited: Duration::from_millis(2500),
            load_avg: 0.5,
        });
        assert!(has(
            &lines(&r),
            "soak hang menu at +01:01 waited 2500 load 0.50"
        ));
    }

    #[test]
    fn an_app_that_exited_fails() {
        let mut r = run(readings(&[(30.0, 9, 40); 12]));
        r.exited_at = Some(secs(300));
        let l = lines(&r);
        assert!(has(&l, "soak app exited at +05:00"), "{l:?}");
        assert_eq!(l.last().unwrap(), "soak verdict fail");
    }

    #[test]
    fn load_errors_and_timeouts_are_printed_but_do_not_fail() {
        let mut r = run(readings(&[(30.0, 9, 40); 12]));
        r.load_errors = 3;
        r.load_timeouts = 1;
        let l = lines(&r);
        assert!(has(&l, "soak load errors 3 timeouts 1"), "{l:?}");
        assert!(judge(&r).passed());
    }

    #[test]
    fn a_pass_prints_the_elapsed_time() {
        let mut r = run(readings(&[(30.0, 9, 40); 12]));
        r.elapsed = secs(1265);
        let l = lines(&r);
        let n = l.len();
        assert_eq!(l[n - 2], "soak elapsed 21:05");
        assert_eq!(l[n - 1], "soak verdict pass");
    }

    #[test]
    fn a_fail_still_prints_the_elapsed_time() {
        let mut r = run(readings(&[(30.0, 9, 40); 12]));
        r.elapsed = secs(1265);
        r.hangs.push(Hang {
            side: Side::Core,
            at: secs(754),
            waited: secs(2),
            load_avg: 3.2,
        });
        let l = lines(&r);
        let n = l.len();
        assert_eq!(l[n - 2], "soak elapsed 21:05");
        assert_eq!(l[n - 1], "soak verdict fail");
    }

    #[test]
    fn the_long_tier_exits_ok_even_on_a_leak() {
        let values: Vec<u32> = (40..52).collect();
        let report = judge(&fds(&values));
        assert!(!report.passed());
        assert!(report.exit_ok(&Tier::Long { load: secs(3600) }));
        assert!(!report.exit_ok(&Tier::Short));
        let ok = judge(&run(readings(&[(30.0, 9, 40); 12])));
        assert!(ok.exit_ok(&Tier::Short));
    }
}
