use std::net::TcpListener;
use std::time::Duration;

use anyhow::{Context, Result};

pub(crate) fn bind() -> Result<TcpListener> {
    TcpListener::bind("127.0.0.1:0").context("bind a loopback listener")
}

pub(crate) fn listen(secs: u64) -> Result<()> {
    let listener = bind()?;
    std::thread::sleep(Duration::from_secs(secs));
    drop(listener);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::net::TcpStream;

    use super::bind;

    #[test]
    fn the_listener_binds_loopback_only_and_accepts_connections() {
        let listener = bind().unwrap();
        let addr = listener.local_addr().unwrap();
        assert!(addr.ip().is_loopback());
        assert_ne!(addr.port(), 0);
        TcpStream::connect(addr).unwrap();
    }
}
