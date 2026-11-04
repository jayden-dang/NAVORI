#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::sync::Arc;

use navori_macos::fake::{FakeProcess, FakeProcesses};
use navori_mod_ports::{HarnessMemo, PortKind, Scanner, kind};

#[test]
fn each_port_gets_the_first_kind_that_applies() {
    let rows: [(Option<&str>, bool, PortKind); 12] = [
        (
            Some("/Applications/Docker.app/Contents/MacOS/com.docker.backend"),
            false,
            PortKind::Docker,
        ),
        (
            Some("/Applications/Docker.app/Contents/MacOS/com.docker.backend"),
            true,
            PortKind::Docker,
        ),
        (
            Some("/opt/homebrew/bin/postgres"),
            false,
            PortKind::Database,
        ),
        (Some("/opt/homebrew/bin/postgres"), true, PortKind::Database),
        (
            Some("/System/Library/CoreServices/ControlCenter.app/Contents/MacOS/ControlCenter"),
            false,
            PortKind::System,
        ),
        (Some("/usr/libexec/rapportd"), false, PortKind::System),
        (Some("/usr/sbin/sshd"), false, PortKind::System),
        (Some("/usr/libexec/rapportd"), true, PortKind::DevServer),
        (Some("/usr/bin/ruby"), false, PortKind::Other),
        (Some("/usr/local/bin/caddy"), false, PortKind::Other),
        (
            Some("/System/Volumes/Data/Users/u/bin/y"),
            false,
            PortKind::Other,
        ),
        (None, true, PortKind::DevServer),
    ];
    for (exe, placed, expected) in rows {
        assert_eq!(
            kind(exe.map(Path::new), placed),
            expected,
            "{exe:?} {placed}"
        );
    }
    assert_eq!(kind(None, false), PortKind::Other);
}

#[test]
fn each_database_name_is_a_database() {
    for name in [
        "mysqld",
        "mariadbd",
        "redis-server",
        "valkey-server",
        "mongod",
    ] {
        let exe = format!("/opt/homebrew/bin/{name}");
        assert_eq!(
            kind(Some(Path::new(&exe)), false),
            PortKind::Database,
            "{name}"
        );
    }
}

#[test]
fn system_ports_are_listed_like_any_other() {
    let fake = Arc::new(FakeProcesses::new(501));
    fake.add(FakeProcess::new(100, 1, 501, 10_100, "/usr/sbin/sshd").listen("0.0.0.0", 22));
    fake.add(FakeProcess::new(101, 1, 501, 10_101, "/usr/bin/ruby").listen("127.0.0.1", 4000));
    let scanner = Scanner::new(fake, std::env::temp_dir());

    let (scan, _) = scanner.scan(HarnessMemo::new());
    let scan = scan.unwrap();

    let listed: Vec<(u16, PortKind)> = scan.entries.iter().map(|e| (e.port, e.kind)).collect();
    assert_eq!(
        listed,
        vec![(22, PortKind::System), (4000, PortKind::Other)]
    );
}
