use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use navori_macos::{MacProcesses, ProcessSource};
use navori_mod_ports::{HarnessMemo, Scanner};

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
