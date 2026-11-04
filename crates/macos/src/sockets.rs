//! Listening TCP sockets and the short name of one process.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::error::ReadError;
use crate::process::{last_errno, read_error};

/// `PROC_PIDFDSOCKETINFO` in `<sys/proc_info.h>`.
const PROC_PIDFDSOCKETINFO: libc::c_int = 3;
/// `SOCKINFO_TCP` in `<sys/proc_info.h>`.
const SOCKINFO_TCP: i32 = 2;
/// `TSI_S_LISTEN` in `<sys/proc_info.h>`.
const TSI_S_LISTEN: i32 = 1;
const INI_IPV4: u8 = 0x1;
const INI_IPV6: u8 = 0x2;

/// `sizeof(struct socket_fdinfo)`, measured on this OS.
const SOCKET_FDINFO_LEN: usize = 792;
const OFF_SOI_KIND: usize = 256;
const OFF_LPORT: usize = 268;
const OFF_VFLAG: usize = 288;
const OFF_LADDR: usize = 312;
const OFF_TCP_STATE: usize = 344;
/// An IPv4 address sits in the last four bytes of the 16-byte `in4in6_addr`.
const V4_IN_ADDR: usize = 12;

const FD_TRIES: usize = 2;
const NAME_LEN: usize = 32;

/// One TCP socket in the LISTEN state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Listener {
    pub addr: IpAddr,
    pub port: u16,
}

pub(crate) fn read_listeners(pid: i32) -> Result<Vec<Listener>, ReadError> {
    let fds = list_fds(pid)?;
    let mut out = Vec::new();
    let mut buf = [0u8; SOCKET_FDINFO_LEN];
    for fd in fds
        .iter()
        .filter(|fd| fd.proc_fdtype == libc::PROX_FDTYPE_SOCKET as u32)
    {
        let wrote = unsafe {
            // SAFETY: buf is writable for SOCKET_FDINFO_LEN bytes. PROC_PIDFDSOCKETINFO fills at most
            // that many bytes of one socket_fdinfo and returns the count, or <= 0 and sets errno.
            libc::proc_pidfdinfo(
                pid,
                fd.proc_fd,
                PROC_PIDFDSOCKETINFO,
                buf.as_mut_ptr().cast(),
                SOCKET_FDINFO_LEN as libc::c_int,
            )
        };
        if wrote <= 0 {
            match last_errno() {
                libc::EBADF => continue,
                errno => return Err(read_error(errno)),
            }
        }
        if usize::try_from(wrote) != Ok(SOCKET_FDINFO_LEN) {
            return Err(ReadError::Malformed);
        }
        if let Some(listener) = parse_listener(&buf) {
            out.push(listener);
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn list_fds(pid: i32) -> Result<Vec<libc::proc_fdinfo>, ReadError> {
    let item = std::mem::size_of::<libc::proc_fdinfo>();
    for _ in 0..FD_TRIES {
        let probe = unsafe {
            // SAFETY: a null buffer with size 0 asks PROC_PIDLISTFDS only for the byte count.
            libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0)
        };
        if probe <= 0 {
            return Err(read_error(last_errno()));
        }
        let probe = usize::try_from(probe).map_err(|_| ReadError::Malformed)?;
        let cap = probe / item + probe / item / 8 + 1;
        let mut fds = vec![
            libc::proc_fdinfo {
                proc_fd: 0,
                proc_fdtype: 0
            };
            cap
        ];
        let size = cap * item;
        let wrote = unsafe {
            // SAFETY: fds is writable for `size` bytes, a whole number of proc_fdinfo records.
            // PROC_PIDLISTFDS writes whole records and returns the byte count, or <= 0 and sets errno.
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDLISTFDS,
                0,
                fds.as_mut_ptr().cast(),
                size as libc::c_int,
            )
        };
        if wrote <= 0 {
            return Err(read_error(last_errno()));
        }
        let wrote = usize::try_from(wrote).map_err(|_| ReadError::Malformed)?;
        if wrote < size {
            fds.truncate(wrote / item);
            return Ok(fds);
        }
    }
    Err(ReadError::Malformed)
}

fn parse_listener(buf: &[u8]) -> Option<Listener> {
    if buf.len() != SOCKET_FDINFO_LEN {
        return None;
    }
    let kind = i32::from_ne_bytes(buf.get(OFF_SOI_KIND..OFF_SOI_KIND + 4)?.try_into().ok()?);
    let state = i32::from_ne_bytes(buf.get(OFF_TCP_STATE..OFF_TCP_STATE + 4)?.try_into().ok()?);
    if kind != SOCKINFO_TCP || state != TSI_S_LISTEN {
        return None;
    }
    let port = u16::from_be_bytes(buf.get(OFF_LPORT..OFF_LPORT + 2)?.try_into().ok()?);
    let vflag = *buf.get(OFF_VFLAG)?;
    let addr = if vflag & INI_IPV6 != 0 {
        let bytes: [u8; 16] = buf.get(OFF_LADDR..OFF_LADDR + 16)?.try_into().ok()?;
        IpAddr::V6(Ipv6Addr::from(bytes))
    } else if vflag & INI_IPV4 != 0 {
        let at = OFF_LADDR + V4_IN_ADDR;
        let bytes: [u8; 4] = buf.get(at..at + 4)?.try_into().ok()?;
        IpAddr::V4(Ipv4Addr::from(bytes))
    } else {
        return None;
    };
    Some(Listener { addr, port })
}

pub(crate) fn read_name(pid: i32) -> Result<String, ReadError> {
    let mut buf = [0u8; NAME_LEN];
    let wrote = unsafe {
        // SAFETY: buf is writable for NAME_LEN bytes. proc_name writes a NUL-terminated name of at
        // most that size and returns its length, or 0 and sets errno.
        libc::proc_name(pid, buf.as_mut_ptr().cast(), NAME_LEN as u32)
    };
    if wrote <= 0 {
        return Err(read_error(last_errno()));
    }
    let wrote = usize::try_from(wrote).map_err(|_| ReadError::Malformed)?;
    let name = &buf[..wrote.min(NAME_LEN)];
    let end = name
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(name.len());
    if end == 0 {
        return Err(ReadError::Malformed);
    }
    Ok(String::from_utf8_lossy(&name[..end]).into_owned())
}
