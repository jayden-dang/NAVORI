//! Listening TCP sockets: a real read-back of this process.

use std::net::{TcpListener, TcpStream};

use navori_macos::{Listener, MacProcesses, ProcessSource};

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
    let binary = std::env::current_exe()?;
    let binary = binary
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    assert!(
        !name.is_empty() && binary.starts_with(&name),
        "{name} {binary}"
    );
    Ok(())
}
