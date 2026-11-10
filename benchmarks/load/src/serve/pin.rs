//! Pins the calling thread to a set of CPUs; a no-op where the OS has no affinity API.

use std::{io, thread};

/// Number of CPU ids a Linux `cpu_set_t` holds; higher ids cannot be pinned to.
pub const MAX_CPUS: usize = 1024;

/// Pins the calling thread to `cpus`.
#[cfg(target_os = "linux")]
pub fn current_thread(cpus: &[usize]) -> io::Result<()> {
    // SAFETY: `set` is a zeroed, valid cpu_set_t; every id is below `MAX_CPUS`, which equals
    // `CPU_SETSIZE`, so CPU_SET only writes inside it, and sched_setaffinity reads
    // `size_of::<cpu_set_t>()` bytes from it.
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();

        for &cpu in cpus {
            assert!(cpu < MAX_CPUS, "CPU {cpu} is out of range");
            libc::CPU_SET(cpu, &mut set);
        }

        if libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set) != 0 {
            return Err(io::Error::last_os_error());
        }
    }

    Ok(())
}

/// Pins the calling thread to `cpus`.
#[cfg(not(target_os = "linux"))]
pub fn current_thread(_: &[usize]) -> io::Result<()> {
    Ok(())
}

/// Pins the calling thread to `cpus`, panicking on failure: threads cannot report it, and an
/// unpinned run must not pass as a pinned one.
pub fn require(cpus: &[usize]) {
    if let Err(error) = current_thread(cpus) {
        panic!("cannot pin thread to CPUs {cpus:?}: {error}");
    }
}

/// Checks up front that `cpus` can be pinned to, on a throwaway thread so the caller's own
/// affinity (inherited by the threads it spawns) stays untouched.
pub fn check(cpus: &[usize]) -> io::Result<()> {
    thread::scope(|scope| {
        scope
            .spawn(|| current_thread(cpus))
            .join()
            .unwrap_or_else(|_| Err(io::Error::other("pinning check thread panicked")))
    })
}
