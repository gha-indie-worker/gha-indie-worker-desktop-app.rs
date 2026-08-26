#![forbid(unsafe_code)]

use crate::state::DesktopState;

pub fn probe(endpoint: &str) -> DesktopState {
    DesktopState {
        connected: false,
        endpoint: endpoint.to_string(),
    }
}

