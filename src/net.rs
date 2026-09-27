#![forbid(unsafe_code)]

use crate::{
    config::DesktopConfig,
    daemon::DaemonClient,
    state::{DesktopState, ServiceState},
};

pub fn probe(config: &DesktopConfig) -> DesktopState {
    let client = match DaemonClient::from_token_file(&config.daemon_base, &config.daemon_token_file) {
        Ok(client) => client,
        Err(error) => {
            return DesktopState {
                connected: false,
                endpoint: config.daemon_base.clone(),
                error: Some(error.to_string()),
                ..DesktopState::default()
            };
        }
    };

    match client.status() {
        Ok(status) => {
            return DesktopState {
                connected: true,
                endpoint: config.daemon_base.clone(),
                mode: Some(status.mode),
                services: status
                    .services
                    .into_iter()
                    .map(|service| ServiceState {
                        name: service.name,
                        running: service.running,
                        pid: service.pid,
                    })
                    .collect(),
                tunnel_running: status
                    .tunnel
                    .as_ref()
                    .map(|tunnel| tunnel.running)
                    .unwrap_or(false),
                keep_awake: status.keep_awake,
                error: None,
            };
        }
        Err(error) => {
            return DesktopState {
                connected: false,
                endpoint: config.daemon_base.clone(),
                error: Some(error.to_string()),
                ..DesktopState::default()
            };
        }
    }
}
