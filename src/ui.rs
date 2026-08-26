#![forbid(unsafe_code)]

use crate::state::DesktopState;

/// Native UI surface. Not a webview and not React.
pub fn render(state: &DesktopState) -> String {
    format!(
        "GHA Indie Worker desktop\nendpoint={}\nconnected={}\n",
        state.endpoint,
        state.connected
    )
}

