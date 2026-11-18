//! Prints every screen lock and sleep event, for testing by hand. Lock the
//! screen or put the device to sleep, then stop with Ctrl+C:
//!
//! ```text
//! cargo run -p dea-vault --example os_events
//! ```

use dea_vault::watch_os_events;

fn main() {
    let _watch = watch_os_events(|event| println!("{event:?}")).expect("watching starts");
    println!("Watching for screen lock and sleep. Ctrl+C to stop.");
    loop {
        // macOS may deliver through the main run loop, so keep it running.
        #[cfg(target_os = "macos")]
        objc2_foundation::NSRunLoop::mainRunLoop()
            .runUntilDate(&objc2_foundation::NSDate::dateWithTimeIntervalSinceNow(1.0));
        #[cfg(not(target_os = "macos"))]
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
