#![forbid(unsafe_code)]

use std::fmt::Write;

use crate::state::DesktopState;

/// Native UI model rendering. This repository must remain free of React, JSX, webviews,
/// and browser-shell process control. A richer native toolkit can bind the same state/actions.
pub fn render(state: &DesktopState) -> String {
    let mut output = String::new();
    let _ = writeln!(output, "IndieBuild desktop");
    let _ = writeln!(output, "daemon={}", state.endpoint);
    let _ = writeln!(output, "connected={}", state.connected);

    if let Some(mode) = &state.mode {
        let _ = writeln!(output, "mode={mode}");
    }

    let _ = writeln!(output, "tunnel_running={}", state.tunnel_running);
    let _ = writeln!(output, "keep_awake={}", state.keep_awake);

    for service in &state.services {
        let pid = service
            .pid
            .map(|pid| pid.to_string())
            .unwrap_or_else(|| "-".to_string());
        let _ = writeln!(
            output,
            "service={} running={} pid={}",
            service.name, service.running, pid
        );
    }

    if let Some(error) = &state.error {
        let _ = writeln!(output, "error={error}");
    }

    return output;
}
