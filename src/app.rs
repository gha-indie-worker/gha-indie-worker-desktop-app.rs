#![forbid(unsafe_code)]

use serde_json::Value;

use crate::config::DesktopConfig;
use crate::daemon::{DaemonClient, DaemonClientError};
use crate::net;
use crate::ui;

pub struct DesktopApp {
    config: DesktopConfig,
}

impl DesktopApp {
    pub fn new(config: DesktopConfig) -> Self {
        return Self { config };
    }

    pub fn run(&self) {
        let state = net::probe(&self.config);
        print!("{}", ui::render(&state));
    }

    pub fn reconcile(&self) -> Result<Value, DaemonClientError> {
        return self.daemon_client()?.reconcile();
    }

    pub fn set_keep_awake(&self, enabled: bool) -> Result<Value, DaemonClientError> {
        return self.daemon_client()?.set_keep_awake(enabled);
    }

    pub fn start_tunnel(&self) -> Result<Value, DaemonClientError> {
        return self.daemon_client()?.start_tunnel();
    }

    pub fn stop_tunnel(&self) -> Result<Value, DaemonClientError> {
        return self.daemon_client()?.stop_tunnel();
    }

    pub fn start_process(&self, name: &str) -> Result<Value, DaemonClientError> {
        return self.daemon_client()?.start_process(name);
    }

    pub fn stop_process(&self, name: &str) -> Result<Value, DaemonClientError> {
        return self.daemon_client()?.stop_process(name);
    }

    pub fn restart_process(&self, name: &str) -> Result<Value, DaemonClientError> {
        return self.daemon_client()?.restart_process(name);
    }

    fn daemon_client(&self) -> Result<DaemonClient, DaemonClientError> {
        return DaemonClient::from_token_file(
            &self.config.daemon_base,
            &self.config.daemon_token_file,
        );
    }
}
