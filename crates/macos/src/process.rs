//! Process list and per-pid reads.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use crate::error::{PlatformError, ReadError, StartTime};

/// One `kinfo_proc` record, in bytes, measured on this OS.
const RECORD_LEN: usize = 648;
const OFF_TV_SEC: usize = 0;
const OFF_TV_USEC: usize = 8;
const OFF_P_STAT: usize = 36;
const OFF_P_PID: usize = 40;
const OFF_P_RUID: usize = 392;
const OFF_E_PPID: usize = 560;
/// `SZOMB` in `<sys/proc.h>`: waiting for the parent to collect the exit status.
const ZOMBIE: u8 = 5;
const LIST_TRIES: usize = 3;

/// One process, as listed. Paths and arguments are separate reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcEntry {
    pub pid: i32,
    pub ppid: i32,
    pub ruid: u32,
    pub start: StartTime,
    pub zombie: bool,
}

/// List processes and read one process's executable, arguments, working directory, and CPU time.
pub trait ProcessSource: Send + Sync {
    /// Real uid of the calling process.
    fn current_uid(&self) -> u32;

    /// Every process the kernel lists, including other users.
    fn list(&self) -> Result<Vec<ProcEntry>, PlatformError>;

    /// One process, using the same record layout as [`ProcessSource::list`].
    fn entry(&self, pid: i32) -> Result<ProcEntry, ReadError>;

    /// Executable path.
    fn exe(&self, pid: i32) -> Result<PathBuf, ReadError>;

    /// Argument vector, without the environment that follows it in the kernel buffer.
    fn argv(&self, pid: i32) -> Result<Vec<String>, ReadError>;

    /// Working directory.
    fn cwd(&self, pid: i32) -> Result<PathBuf, ReadError>;

    /// User plus system CPU time accumulated by this process, not by its children.
    fn cpu_time(&self, pid: i32) -> Result<Duration, ReadError>;
}

/// macOS [`ProcessSource`]. [`MacProcesses::new`] refuses a `kinfo_proc` layout it does not recognize.
#[derive(Debug)]
pub struct MacProcesses {
    _private: (),
}

impl MacProcesses {
    /// Check that pid 1 is a 648-byte record with parent 0 and real uid 0.
    pub fn new() -> Result<Self, PlatformError> {
        let launchd = read_entry(1).map_err(|_| PlatformError::Layout { len: 0 })?;
        if launchd.ppid != 0 || launchd.ruid != 0 {
            return Err(PlatformError::Layout { len: RECORD_LEN });
        }
        Ok(Self { _private: () })
    }
}

impl ProcessSource for MacProcesses {
    fn current_uid(&self) -> u32 {
        current_uid()
    }

    fn list(&self) -> Result<Vec<ProcEntry>, PlatformError> {
        list_records()
    }

    fn entry(&self, pid: i32) -> Result<ProcEntry, ReadError> {
        read_entry(pid)
    }

    fn exe(&self, pid: i32) -> Result<PathBuf, ReadError> {
        read_exe(pid)
    }

    fn argv(&self, _pid: i32) -> Result<Vec<String>, ReadError> {
        Err(ReadError::Malformed)
    }

    fn cwd(&self, pid: i32) -> Result<PathBuf, ReadError> {
        read_cwd(pid)
    }

    fn cpu_time(&self, pid: i32) -> Result<Duration, ReadError> {
        read_cpu_time(pid)
    }
}

fn current_uid() -> u32 {
    unsafe {
        // SAFETY: getuid returns the caller's real uid and takes no pointer.
        libc::getuid()
    }
}

fn list_records() -> Result<Vec<ProcEntry>, PlatformError> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROC, libc::KERN_PROC_ALL];
    for _ in 0..LIST_TRIES {
        let probe = sysctl(&mut mib, None).map_err(|errno| PlatformError::Sysctl { errno })?;
        let cap = probe.saturating_add(probe / 8);
        let mut buf = vec![0u8; cap];
        match sysctl(&mut mib, Some(&mut buf)) {
            Ok(len) => {
                if !len.is_multiple_of(RECORD_LEN) {
                    return Err(PlatformError::Layout { len });
                }
                buf.truncate(len);
                return buf
                    .chunks_exact(RECORD_LEN)
                    .map(parse_record)
                    .collect::<Option<Vec<_>>>()
                    .ok_or(PlatformError::Layout { len });
            }
            Err(errno) if errno == libc::ENOMEM => {}
            Err(errno) => return Err(PlatformError::Sysctl { errno }),
        }
    }
    Err(PlatformError::Sysctl {
        errno: libc::ENOMEM,
    })
}

fn read_entry(pid: i32) -> Result<ProcEntry, ReadError> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROC, libc::KERN_PROC_PID, pid];
    let mut buf = vec![0u8; RECORD_LEN];
    match sysctl(&mut mib, Some(&mut buf)) {
        Ok(0) => Err(ReadError::Gone),
        Ok(len) if len < RECORD_LEN || !len.is_multiple_of(RECORD_LEN) => Err(ReadError::Malformed),
        Ok(_) => parse_record(&buf[..RECORD_LEN]).ok_or(ReadError::Malformed),
        Err(errno) => Err(read_error(errno)),
    }
}

fn parse_record(rec: &[u8]) -> Option<ProcEntry> {
    let sec = read_i64(rec, OFF_TV_SEC)?;
    let usec = read_i32(rec, OFF_TV_USEC)?;
    let stat = *rec.get(OFF_P_STAT)?;
    let pid = read_i32(rec, OFF_P_PID)?;
    let ruid = read_u32(rec, OFF_P_RUID)?;
    let ppid = read_i32(rec, OFF_E_PPID)?;
    let start = start_time(sec, usec)?;
    Some(ProcEntry {
        pid,
        ppid,
        ruid,
        start,
        zombie: stat == ZOMBIE,
    })
}

fn start_time(sec: i64, usec: i32) -> Option<StartTime> {
    let sec = u64::try_from(sec).ok()?;
    let usec = u64::try_from(usec).ok()?;
    Some(StartTime(sec.checked_mul(1_000_000)?.checked_add(usec)?))
}

fn read_i64(rec: &[u8], at: usize) -> Option<i64> {
    let bytes: [u8; 8] = rec.get(at..at + 8)?.try_into().ok()?;
    Some(i64::from_ne_bytes(bytes))
}

fn read_i32(rec: &[u8], at: usize) -> Option<i32> {
    let bytes: [u8; 4] = rec.get(at..at + 4)?.try_into().ok()?;
    Some(i32::from_ne_bytes(bytes))
}

fn read_u32(rec: &[u8], at: usize) -> Option<u32> {
    let bytes: [u8; 4] = rec.get(at..at + 4)?.try_into().ok()?;
    Some(u32::from_ne_bytes(bytes))
}

fn read_exe(pid: i32) -> Result<PathBuf, ReadError> {
    let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let wrote = unsafe {
        // SAFETY: buf is writable for PROC_PIDPATHINFO_MAXSIZE bytes. proc_pidpath writes a
        // NUL-terminated path and returns its length, or 0 and sets errno.
        libc::proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32)
    };
    if wrote <= 0 {
        return Err(read_error(last_errno()));
    }
    let wrote = usize::try_from(wrote).map_err(|_| ReadError::Malformed)?;
    path_from_bytes(&buf[..wrote.min(buf.len())])
}

fn read_cwd(pid: i32) -> Result<PathBuf, ReadError> {
    let mut info = unsafe {
        // SAFETY: proc_vnodepathinfo is a C struct of integers and byte arrays; zero is a valid value.
        std::mem::zeroed::<libc::proc_vnodepathinfo>()
    };
    let size = std::mem::size_of::<libc::proc_vnodepathinfo>() as i32;
    let wrote = unsafe {
        // SAFETY: info is writable for `size` bytes. PROC_PIDVNODEPATHINFO fills one proc_vnodepathinfo.
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDVNODEPATHINFO,
            0,
            (&raw mut info).cast(),
            size,
        )
    };
    if wrote <= 0 {
        return Err(read_error(last_errno()));
    }
    if wrote != size {
        return Err(ReadError::Malformed);
    }
    path_from_bytes(&flatten_vip_path(&info.pvi_cdir.vip_path))
}

fn flatten_vip_path(chunks: &[[libc::c_char; 32]; 32]) -> [u8; 1024] {
    let mut bytes = [0u8; 1024];
    for (index, chunk) in chunks.iter().enumerate() {
        for (offset, byte) in chunk.iter().enumerate() {
            bytes[index * 32 + offset] = byte.cast_unsigned();
        }
    }
    bytes
}

fn path_from_bytes(buf: &[u8]) -> Result<PathBuf, ReadError> {
    let end = buf.iter().position(|byte| *byte == 0).unwrap_or(buf.len());
    if end == 0 {
        return Err(ReadError::Malformed);
    }
    Ok(PathBuf::from(OsStr::from_bytes(&buf[..end])))
}

fn read_cpu_time(pid: i32) -> Result<Duration, ReadError> {
    let mut info = unsafe {
        // SAFETY: rusage_info_v2 is a C struct of integers and a byte array; zero is a valid value.
        std::mem::zeroed::<libc::rusage_info_v2>()
    };
    let rc = unsafe {
        // SAFETY: info is writable storage for RUSAGE_INFO_V2. The cast matches the C buffer pointer.
        libc::proc_pid_rusage(pid, libc::RUSAGE_INFO_V2, (&raw mut info).cast())
    };
    if rc != 0 {
        return Err(read_error(last_errno()));
    }
    let (numer, denom) = timebase()?;
    let ticks = u128::from(info.ri_user_time) + u128::from(info.ri_system_time);
    let nanos = ticks * u128::from(numer) / u128::from(denom);
    let nanos = u64::try_from(nanos).unwrap_or(u64::MAX);
    Ok(Duration::from_nanos(nanos))
}

fn timebase() -> Result<(u32, u32), ReadError> {
    static TIMEBASE: OnceLock<(u32, u32)> = OnceLock::new();
    if let Some(base) = TIMEBASE.get() {
        return Ok(*base);
    }
    let base = mach_timebase()?;
    Ok(*TIMEBASE.get_or_init(|| base))
}

#[allow(deprecated)] // libc 0.2.191 deprecates mach_timebase_info; this is its only call.
fn mach_timebase() -> Result<(u32, u32), ReadError> {
    let mut info = libc::mach_timebase_info { numer: 0, denom: 0 };
    let rc = unsafe {
        // SAFETY: info is a live local. mach_timebase_info writes the timebase and does not retain it.
        libc::mach_timebase_info(&raw mut info)
    };
    if rc != 0 || info.denom == 0 {
        return Err(ReadError::Malformed);
    }
    Ok((info.numer, info.denom))
}

fn sysctl(mib: &mut [i32], dest: Option<&mut [u8]>) -> Result<usize, i32> {
    let mut len = dest.as_ref().map_or(0, |buf| buf.len());
    let ptr = dest
        .map(|buf| buf.as_mut_ptr().cast::<libc::c_void>())
        .unwrap_or(std::ptr::null_mut());
    let rc = unsafe {
        // SAFETY: mib is a live MIB of namelen ints. ptr is null for a size probe, or writable
        // for `len` bytes. sysctl writes at most `len` bytes and stores the byte count in `len`.
        // newp is null because this call only reads.
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as libc::c_uint,
            ptr,
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc == 0 { Ok(len) } else { Err(last_errno()) }
}

fn read_error(errno: i32) -> ReadError {
    match errno {
        libc::ESRCH => ReadError::Gone,
        libc::EPERM => ReadError::Denied,
        _ => ReadError::Malformed,
    }
}

fn last_errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}
