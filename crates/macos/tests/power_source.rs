use std::io::ErrorKind;

use navori_macos::fake::FakePowerSource;
use navori_macos::{MacPowerSource, PowerKind, PowerSource};

#[test]
fn an_empty_watch_would_block() {
    let source = FakePowerSource::new(PowerKind::Adapter);
    let watch = source.watch().expect("watch");
    let err = watch.read_tokens().expect_err("nothing to read");
    assert_eq!(err.kind(), ErrorKind::WouldBlock);
}

#[test]
fn a_fired_watch_reads_tokens_then_would_block() {
    let source = FakePowerSource::new(PowerKind::Adapter);
    let watch = source.watch().expect("watch");
    source.fire();
    source.fire();
    assert_eq!(watch.read_tokens().expect("two changes"), 2);
    let err = watch.read_tokens().expect_err("drained");
    assert_eq!(err.kind(), ErrorKind::WouldBlock);
}

#[test]
fn a_closed_watch_reads_zero() {
    let source = FakePowerSource::new(PowerKind::Adapter);
    let watch = source.watch().expect("watch");
    source.kill();
    assert_eq!(watch.read_tokens().expect("closed"), 0);
}

#[test]
fn the_mac_power_source_names_a_kind() {
    assert!(MacPowerSource.current().is_ok());
}
