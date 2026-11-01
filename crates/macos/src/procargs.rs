//! Argument buffer for one process, without its environment.

use std::sync::{Mutex, OnceLock};

use crate::error::ReadError;

/// Split a `KERN_PROCARGS2` buffer into its `argc` argument strings.
///
/// The executable path and the environment that follows the arguments are not returned.
/// A byte that is not UTF-8 becomes U+FFFD. The buffer is malformed when `argc` strings
/// are not all there, each with its trailing NUL.
pub fn parse_procargs(buf: &[u8]) -> Result<Vec<String>, ReadError> {
    let argc = read_i32(buf, 0).ok_or(ReadError::Malformed)?;
    let argc = usize::try_from(argc).map_err(|_| ReadError::Malformed)?;
    let (mut index, _) = cstr_at(buf, 4).ok_or(ReadError::Malformed)?;
    index = skip_nuls(buf, index).ok_or(ReadError::Malformed)?;
    let mut args = Vec::new();
    for _ in 0..argc {
        let (next, bytes) = cstr_at(buf, index).ok_or(ReadError::Malformed)?;
        args.push(String::from_utf8_lossy(bytes).into_owned());
        index = next;
    }
    Ok(args)
}

/// Read the argument vector for `pid`.
///
/// `entry_failed` runs only after `EINVAL`. That is what `KERN_PROCARGS2` returns both
/// when the pid is gone and when the caller may not inspect it.
pub(crate) fn read_argv(
    pid: i32,
    entry_failed: impl FnOnce() -> bool,
) -> Result<Vec<String>, ReadError> {
    let max = argmax().map_err(crate::process::read_error)?;
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
    let probed = match crate::process::sysctl(&mut mib, None) {
        Ok(len) if len > 0 && len <= max => len,
        Ok(_) => return Err(ReadError::Malformed),
        Err(errno) => return Err(procargs_error(errno, entry_failed)),
    };
    let buf = match copy_procargs(&mut mib, probed) {
        Ok(buf) => buf,
        Err(errno) => return Err(procargs_error(errno, entry_failed)),
    };
    parse_procargs(&buf)
}

fn read_i32(buf: &[u8], at: usize) -> Option<i32> {
    let end = at.checked_add(4)?;
    let bytes: [u8; 4] = buf.get(at..end)?.try_into().ok()?;
    Some(i32::from_ne_bytes(bytes))
}

/// The bytes of one NUL-terminated string, and the index just past its NUL.
fn cstr_at(buf: &[u8], index: usize) -> Option<(usize, &[u8])> {
    let rest = buf.get(index..)?;
    let end = rest.iter().position(|byte| *byte == 0)?;
    let bytes = rest.get(..end)?;
    let next = index.checked_add(end)?.checked_add(1)?;
    Some((next, bytes))
}

fn skip_nuls(buf: &[u8], mut index: usize) -> Option<usize> {
    while buf.get(index).copied() == Some(0) {
        index = index.checked_add(1)?;
    }
    Some(index)
}

fn argmax() -> Result<usize, i32> {
    static ARGMAX: OnceLock<usize> = OnceLock::new();
    if let Some(max) = ARGMAX.get() {
        return Ok(*max);
    }
    let mut mib = [libc::CTL_KERN, libc::KERN_ARGMAX];
    let mut raw = [0u8; 4];
    let len = crate::process::sysctl(&mut mib, Some(&mut raw))?;
    if len != raw.len() {
        return Err(libc::EINVAL);
    }
    let max = usize::try_from(i32::from_ne_bytes(raw)).map_err(|_| libc::EINVAL)?;
    if max == 0 {
        return Err(libc::EINVAL);
    }
    Ok(*ARGMAX.get_or_init(|| max))
}

/// One scratch buffer, grown to the largest block read. The kernel's ceiling is `KERN_ARGMAX`.
static SCRATCH: Mutex<Vec<u8>> = Mutex::new(Vec::new());

fn copy_procargs(mib: &mut [i32], probed: usize) -> Result<Vec<u8>, i32> {
    let mut scratch = SCRATCH.lock().unwrap_or_else(|poisoned| {
        let mut guard = poisoned.into_inner();
        // A panic may have unlocked this buffer before it was cleared.
        wipe(&mut guard);
        guard
    });
    read_procargs(&mut scratch, mib, probed)
}

/// Zeroes the argument scratch, including spare capacity, when the read returns or panics.
struct ClearOnDrop<'a>(&'a mut Vec<u8>);

impl Drop for ClearOnDrop<'_> {
    fn drop(&mut self) {
        wipe(self.0);
    }
}

fn read_procargs(scratch: &mut Vec<u8>, mib: &mut [i32], probed: usize) -> Result<Vec<u8>, i32> {
    let clear = ClearOnDrop(scratch);
    if clear.0.len() < probed {
        clear.0.resize(probed, 0);
    }
    let Some(dest) = clear.0.get_mut(..probed) else {
        return Ok(Vec::new());
    };
    let len = crate::process::sysctl(mib, Some(dest))?;
    // A reported length past the buffer we offered is not a readable block.
    if len > probed {
        return Ok(Vec::new());
    }
    Ok(clear.0.get(..len).unwrap_or_default().to_vec())
}

/// `len` stays at capacity so the spare bytes are initialized zeros, not leftover arguments.
fn wipe(buf: &mut Vec<u8>) {
    let cap = buf.capacity();
    if buf.len() < cap {
        buf.resize(cap, 0);
    }
    buf.fill(0);
}

fn procargs_error(errno: i32, entry_failed: impl FnOnce() -> bool) -> ReadError {
    if errno != libc::EINVAL {
        return crate::process::read_error(errno);
    }
    if entry_failed() {
        ReadError::Gone
    } else {
        ReadError::Denied
    }
}

#[cfg(test)]
mod tests {
    use super::SCRATCH;

    fn scratch_kept_a_byte() -> bool {
        let scratch = SCRATCH
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        scratch.capacity() == 0
            || scratch.len() != scratch.capacity()
            || scratch.iter().any(|byte| *byte != 0)
    }

    #[test]
    fn the_argument_scratch_does_not_keep_a_previous_environment_byte() {
        let pid = std::process::id() as i32;
        let read = super::read_argv(pid, || false);
        assert!(read.is_ok(), "argument read failed");
        assert!(
            !scratch_kept_a_byte(),
            "argument scratch kept a previous byte"
        );

        let poisoned = std::panic::catch_unwind(|| {
            let mut scratch = SCRATCH
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            scratch.clear();
            scratch.resize(32, 0xA5);
            panic!("poison");
        });
        assert!(poisoned.is_err(), "argument scratch was not poisoned");

        let read = super::read_argv(pid, || false);
        assert!(read.is_ok(), "argument read failed");
        assert!(
            !scratch_kept_a_byte(),
            "argument scratch kept a previous byte"
        );
    }
}
