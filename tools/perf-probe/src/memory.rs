//! Memory charged to one process, as the operating system's own monitor
//! shows it. Summing resident set sizes would count shared system libraries
//! (WebKit, the C runtime, UI frameworks) once per process and overstate
//! an app with several webview processes by a factor of two or more.
//!
//! - macOS: physical footprint, the "Memory" column of Activity Monitor.
//! - Windows: private working set, the "Memory" column of Task Manager.
//! - Linux: proportional set size (PSS), shared pages split between users.

/// Bytes charged to process `pid`, or `None` when it cannot be read, for
/// example because the process has exited.
pub fn process_memory_bytes(pid: u32) -> Option<u64> {
    imp::process_memory_bytes(pid)
}

#[cfg(target_os = "macos")]
mod imp {
    pub fn process_memory_bytes(pid: u32) -> Option<u64> {
        let pid = libc::c_int::try_from(pid).ok()?;
        let mut info: libc::rusage_info_v2 = unsafe { std::mem::zeroed() };
        // SAFETY: `info` is a valid, writable rusage_info_v2, which is what
        // the RUSAGE_INFO_V2 flavor writes.
        let rc = unsafe {
            libc::proc_pid_rusage(
                pid,
                libc::RUSAGE_INFO_V2,
                (&mut info as *mut libc::rusage_info_v2).cast(),
            )
        };
        (rc == 0).then_some(info.ri_phys_footprint)
    }
}

#[cfg(target_os = "linux")]
mod imp {
    pub fn process_memory_bytes(pid: u32) -> Option<u64> {
        let text = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")).ok()?;
        text.lines().find_map(|line| {
            let kb = line.strip_prefix("Pss:")?.trim().strip_suffix("kB")?;
            kb.trim().parse::<u64>().ok().map(|kb| kb * 1024)
        })
    }
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX2,
    };
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    pub fn process_memory_bytes(pid: u32) -> Option<u64> {
        // SAFETY: plain Win32 calls; the handle is closed before returning
        // and the counters struct is sized by `cb`.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return None;
            }
            let mut counters: PROCESS_MEMORY_COUNTERS_EX2 = std::mem::zeroed();
            counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32;
            let ok = K32GetProcessMemoryInfo(
                handle,
                (&mut counters as *mut PROCESS_MEMORY_COUNTERS_EX2)
                    .cast::<PROCESS_MEMORY_COUNTERS>(),
                counters.cb,
            );
            CloseHandle(handle);
            if ok == 0 {
                return None;
            }
            // Windows before build 20348 leaves the private working set at
            // zero; private commit is the closest figure there.
            let bytes = if counters.PrivateWorkingSetSize > 0 {
                counters.PrivateWorkingSetSize
            } else {
                counters.PrivateUsage
            };
            Some(bytes as u64)
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
mod imp {
    pub fn process_memory_bytes(_pid: u32) -> Option<u64> {
        None
    }
}
