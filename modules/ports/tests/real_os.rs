use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use navori_macos::{MacProcesses, MacSignals, ProcessSource, Signals};
use navori_mod_ports::{
    DockerStatus, FreshRead, Guard, HarnessMemo, Outcome, Placement, PortEntry, PortKind, Resolved,
    Scan, Scanner, Selector, Target, resolve,
};

#[test]
#[ignore = "prints the median for the 50 ms budget; cargo xtask perf runs it on an idle machine"]
fn port_scan_cost_for_the_perf_gate() -> Result<(), String> {
    let mac = MacProcesses::new().map_err(|err| err.to_string())?;
    let source: Arc<dyn ProcessSource> = Arc::new(mac);
    let scanner = Scanner::new(source, PathBuf::from("/var/empty"));
    let mut memo = HarnessMemo::new();
    let mut samples: Vec<(Duration, usize)> = Vec::with_capacity(5);
    for _ in 0..5 {
        let started = Instant::now();
        let (result, kept) = scanner.scan(memo);
        let elapsed = started.elapsed();
        memo = kept;
        let scan = result.map_err(|err| err.to_string())?;
        samples.push((elapsed, scan.entries.len()));
    }
    samples.sort_by_key(|(elapsed, _)| *elapsed);
    let (median, listeners) = samples[2];
    println!("median {} µs; listeners {listeners}", median.as_micros());
    Ok(())
}

fn listening(pid: i32, start_us: u64) -> PortEntry {
    PortEntry {
        port: 49_999,
        addresses: vec![Ipv4Addr::LOCALHOST.into()],
        pid,
        start_us,
        name: "probe".to_owned(),
        exe: None,
        place: Placement::default(),
        harness: None,
        kind: PortKind::DevServer,
        container: None,
    }
}

#[test]
#[ignore = "reads the real process table and group calls; sends no signal"]
fn the_real_guard_is_complete_and_protects_this_process_and_its_parent() -> Result<(), String> {
    let source: Arc<dyn ProcessSource> =
        Arc::new(MacProcesses::new().map_err(|err| err.to_string())?);
    let signals = Arc::new(MacSignals::new());
    let own = signals.own_pid();
    let own_entry = source.entry(own).map_err(|err| format!("{err:?}"))?;
    let parent = own_entry.ppid;
    let parent_entry = source.entry(parent).map_err(|err| format!("{err:?}"))?;
    let guard = Guard::new(
        Arc::clone(&signals) as Arc<dyn Signals>,
        Arc::clone(&source),
        None,
    );
    let fresh = FreshRead {
        scan: Scan::default(),
        entries: vec![
            listening(own, own_entry.start.0),
            listening(parent, parent_entry.start.0),
        ],
        docker: DockerStatus::Read,
    };
    let targets = [
        Target::Process {
            pid: own,
            start_us: own_entry.start.0,
        },
        Target::Process {
            pid: parent,
            start_us: parent_entry.start.0,
        },
    ];

    let got = resolve(&fresh, &guard, &Selector::Project(None), &targets);

    for (verdict, what) in got.iter().zip(["this process", "its parent"]) {
        match verdict {
            Resolved::Refused(Outcome::Protected, _) => {}
            other => return Err(format!("{what} was not protected: {other:?}")),
        }
    }
    Ok(())
}
