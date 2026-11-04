//! Listening TCP sockets: a real read-back of this process, and the scripted fake.

use std::net::{IpAddr, TcpListener, TcpStream};

use navori_macos::fake::{FakeProcess, FakeProcesses};
use navori_macos::{Listener, MacProcesses, ProcessSource, ReadError};

#[test]
#[ignore = "reads this process's own sockets through the kernel"]
fn a_loopback_listener_in_this_process_is_read_back() -> Result<(), Box<dyn std::error::Error>> {
    let v4 = TcpListener::bind("127.0.0.1:0")?;
    let v6 = TcpListener::bind("[::1]:0")?;
    let _client = TcpStream::connect(v4.local_addr()?)?;
    let mut want = vec![
        Listener {
            addr: v4.local_addr()?.ip(),
            port: v4.local_addr()?.port(),
        },
        Listener {
            addr: v6.local_addr()?.ip(),
            port: v6.local_addr()?.port(),
        },
    ];
    want.sort();

    let mut got = MacProcesses::new()?.listeners(std::process::id() as i32)?;
    got.sort();

    assert_eq!(got, want);
    Ok(())
}

#[test]
#[ignore = "reads this process's own name through the kernel"]
fn the_short_name_of_this_process_is_read() -> Result<(), Box<dyn std::error::Error>> {
    let name = MacProcesses::new()?.name(std::process::id() as i32)?;
    assert!(!name.is_empty());
    Ok(())
}

#[test]
fn the_fake_returns_the_listeners_it_was_given_and_records_the_read() {
    let fake = FakeProcesses::new(501);
    fake.add(
        FakeProcess::new(10, 1, 501, 1_000, "/usr/bin/node")
            .listen("127.0.0.1", 3000)
            .listen("::", 5432),
    );

    assert_eq!(
        fake.listeners(10),
        Ok(vec![
            Listener {
                addr: IpAddr::from([127, 0, 0, 1]),
                port: 3000
            },
            Listener {
                addr: IpAddr::from([0u16; 8]),
                port: 5432
            },
        ])
    );
    assert_eq!(fake.reads(), vec![(10, "fds")]);
}

#[test]
fn the_fake_reports_a_refused_socket_read_and_a_missing_process() {
    let fake = FakeProcesses::new(501);
    fake.add(FakeProcess::new(10, 1, 501, 1_000, "/bin/sh").fds_err(ReadError::Denied));

    assert_eq!(fake.listeners(10), Err(ReadError::Denied));
    assert_eq!(fake.listeners(99), Err(ReadError::Gone));
    assert_eq!(fake.name(99), Err(ReadError::Gone));
}
