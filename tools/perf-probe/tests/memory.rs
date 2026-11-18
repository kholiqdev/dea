use perf_probe::process_memory_bytes;

#[test]
fn own_process_memory_is_measured_the_way_the_os_shows_it() {
    let bytes = process_memory_bytes(std::process::id()).expect("supported on this OS");
    // A test binary uses at least some memory and far less than 4 GiB.
    assert!(bytes > 100 * 1024, "{bytes}");
    assert!(bytes < 4 * 1024 * 1024 * 1024, "{bytes}");
}

#[test]
fn memory_of_a_missing_process_is_unknown() {
    // Process ids this large are never handed out.
    assert_eq!(process_memory_bytes(u32::MAX - 7), None);
}
