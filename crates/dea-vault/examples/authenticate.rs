//! Shows this OS's authentication dialog, for testing by hand:
//!
//! ```text
//! cargo run -p dea-vault --example authenticate -- "Send POST /v1/shipments to production using `prod_access_token`"
//! ```

use dea_vault::native_authenticator;

fn main() {
    let reason = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "Unlock the Vault".to_string());
    #[cfg(windows)]
    #[allow(unsafe_code)]
    // SAFETY: no arguments; returns null when there is no console window.
    let window = Some(dea_vault::WindowHandle(
        (unsafe { windows_sys::Win32::System::Console::GetConsoleWindow() }) as isize,
    ));
    #[cfg(not(windows))]
    let window = None;
    let auth = native_authenticator(window);
    println!("method: {:?}", auth.method());
    println!("{:?}", auth.authenticate(&reason));
}
