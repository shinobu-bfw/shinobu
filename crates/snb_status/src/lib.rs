//! Runtime status snapshots for Shinobu bots.

use std::time::{Duration, Instant};

/// A point-in-time view of the running bot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BotStatus {
    /// How long the bot process has been running.
    pub uptime: Duration,
    /// Platform metadata for the current process.
    pub platform: PlatformStatus,
    /// Process memory usage, when the current platform supports it.
    pub memory: MemoryStatus,
    /// Number of plugins currently registered by the runtime.
    pub plugin_count: usize,
}

impl BotStatus {
    /// Build a status snapshot from runtime-owned values.
    #[must_use]
    pub fn collect(uptime: Duration, plugin_count: usize) -> Self {
        Self {
            uptime,
            platform: PlatformStatus::current(),
            memory: MemoryStatus::current(),
            plugin_count,
        }
    }
}

/// Platform metadata for the current process target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlatformStatus {
    pub os: &'static str,
    pub arch: &'static str,
    pub family: &'static str,
}

impl PlatformStatus {
    #[must_use]
    pub fn current() -> Self {
        Self {
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            family: std::env::consts::FAMILY,
        }
    }
}

/// Memory usage for the current process.
///
/// Values are optional because process memory APIs are platform-specific.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemoryStatus {
    /// Resident memory in bytes, also known as working set size.
    pub resident_bytes: Option<u64>,
    /// Virtual memory in bytes, when available.
    pub virtual_bytes: Option<u64>,
}

impl MemoryStatus {
    #[must_use]
    pub fn current() -> Self {
        platform::current_memory()
    }

    #[must_use]
    pub fn is_available(&self) -> bool {
        self.resident_bytes.is_some() || self.virtual_bytes.is_some()
    }
}

/// Monotonic clock used to derive bot uptime.
#[derive(Debug, Clone)]
pub struct UptimeClock {
    started_at: Instant,
}

impl UptimeClock {
    #[must_use]
    pub fn started_now() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }

    #[must_use]
    pub fn with_started_at(started_at: Instant) -> Self {
        Self { started_at }
    }

    #[must_use]
    pub fn started_at(&self) -> Instant {
        self.started_at
    }

    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    #[must_use]
    pub fn collect_status(&self, plugin_count: usize) -> BotStatus {
        BotStatus::collect(self.elapsed(), plugin_count)
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::MemoryStatus;

    pub fn current_memory() -> MemoryStatus {
        let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
            return MemoryStatus::default();
        };
        MemoryStatus {
            resident_bytes: parse_proc_status_bytes(&status, "VmRSS:"),
            virtual_bytes: parse_proc_status_bytes(&status, "VmSize:"),
        }
    }

    pub(super) fn parse_proc_status_bytes(status: &str, key: &str) -> Option<u64> {
        status.lines().find_map(|line| {
            let rest = line.strip_prefix(key)?;
            let mut parts = rest.split_whitespace();
            let value = parts.next()?.parse::<u64>().ok()?;
            match parts.next().unwrap_or("kB") {
                "kB" | "KB" => value.checked_mul(1024),
                "B" => Some(value),
                _ => None,
            }
        })
    }
}

#[cfg(windows)]
mod platform {
    use std::ffi::c_void;
    use std::mem::size_of;

    use super::MemoryStatus;

    type Handle = *mut c_void;

    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> Handle;
    }

    #[link(name = "psapi")]
    unsafe extern "system" {
        fn GetProcessMemoryInfo(
            process: Handle,
            counters: *mut ProcessMemoryCounters,
            size: u32,
        ) -> i32;
    }

    pub fn current_memory() -> MemoryStatus {
        let mut counters = ProcessMemoryCounters {
            cb: size_of::<ProcessMemoryCounters>() as u32,
            page_fault_count: 0,
            peak_working_set_size: 0,
            working_set_size: 0,
            quota_peak_paged_pool_usage: 0,
            quota_paged_pool_usage: 0,
            quota_peak_non_paged_pool_usage: 0,
            quota_non_paged_pool_usage: 0,
            pagefile_usage: 0,
            peak_pagefile_usage: 0,
        };
        let ok =
            unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) != 0 };
        if !ok {
            return MemoryStatus::default();
        }
        MemoryStatus {
            resident_bytes: Some(counters.working_set_size as u64),
            virtual_bytes: None,
        }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod platform {
    use super::MemoryStatus;

    pub fn current_memory() -> MemoryStatus {
        MemoryStatus::default()
    }
}

#[cfg(test)]
#[path = "../tests/unit/lib_tests.rs"]
mod lib_tests;
