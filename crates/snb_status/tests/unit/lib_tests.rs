use std::time::Duration;

use super::{BotStatus, MemoryStatus, PlatformStatus, UptimeClock};

#[cfg(target_os = "linux")]
#[test]
fn parses_proc_status_kib_fields() {
    let status = "Name:\ttest\nVmSize:\t  123 kB\nVmRSS:\t  45 kB\n";

    assert_eq!(
        super::platform::parse_proc_status_bytes(status, "VmSize:"),
        Some(125_952)
    );
    assert_eq!(
        super::platform::parse_proc_status_bytes(status, "VmRSS:"),
        Some(46_080)
    );
}

#[test]
fn platform_status_uses_current_target_consts() {
    let platform = PlatformStatus::current();

    assert_eq!(platform.os, std::env::consts::OS);
    assert_eq!(platform.arch, std::env::consts::ARCH);
    assert_eq!(platform.family, std::env::consts::FAMILY);
}

#[test]
fn bot_status_preserves_runtime_values() {
    let status = BotStatus::collect(Duration::from_secs(42), 7);

    assert_eq!(status.uptime, Duration::from_secs(42));
    assert_eq!(status.plugin_count, 7);
    assert_eq!(status.platform, PlatformStatus::current());
}

#[test]
fn memory_status_current_is_safe_to_collect() {
    let memory = MemoryStatus::current();

    if let Some(resident_bytes) = memory.resident_bytes {
        assert!(resident_bytes > 0);
    }
    if let Some(virtual_bytes) = memory.virtual_bytes {
        assert!(virtual_bytes > 0);
    }
}

#[test]
fn uptime_clock_reports_elapsed_time() {
    let clock = UptimeClock::with_started_at(std::time::Instant::now() - Duration::from_secs(5));

    assert!(clock.elapsed() >= Duration::from_secs(5));
    assert_eq!(clock.collect_status(3).plugin_count, 3);
}
