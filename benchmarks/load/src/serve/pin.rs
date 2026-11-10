//! Pins the calling thread to a set of CPUs; a no-op where the OS has no affinity API.

/// Pins the calling thread to `cpus`.
#[cfg(target_os = "linux")]
pub fn current_thread(cpus: &[usize]) {
    // SAFETY: `set` is a zeroed, valid cpu_set_t; CPU_SET only writes inside it and
    // sched_setaffinity reads `size_of::<cpu_set_t>()` bytes from it.
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();

        for &cpu in cpus {
            libc::CPU_SET(cpu, &mut set);
        }

        // Pinning is best effort: an unpinned run is still valid, only noisier.
        let _ = libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set);
    }
}

/// Pins the calling thread to `cpus`.
#[cfg(not(target_os = "linux"))]
pub fn current_thread(_: &[usize]) {}
