use std::net::TcpListener;
use std::time::Duration;

use anyhow::{Context, Result};

pub(crate) fn listen(secs: u64) -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").context("bind a loopback listener")?;
    std::thread::sleep(Duration::from_secs(secs));
    drop(listener);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::process::Command;
    use std::time::Duration;

    use super::listen;

    #[test]
    fn listening_holds_a_loopback_only_tcp_listener_for_the_duration() {
        let held = std::thread::spawn(|| listen(2));
        std::thread::sleep(Duration::from_millis(500));
        let out = Command::new("/usr/sbin/lsof")
            .args(["-nP", "-a", "-iTCP", "-sTCP:LISTEN", "-Fn", "-p"])
            .arg(std::process::id().to_string())
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        assert!(text.contains("n127.0.0.1:"), "{text}");
        assert!(!text.contains("n*:"), "{text}");
        held.join().unwrap().unwrap();
    }
}
