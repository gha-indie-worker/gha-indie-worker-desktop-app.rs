#![forbid(unsafe_code)]

use crate::config::DesktopConfig;
use crate::net;
use crate::ui;

pub struct DesktopApp {
    config: DesktopConfig,
}

impl DesktopApp {
    pub fn new(config: DesktopConfig) -> Self {
        Self { config }
    }

    pub fn run(&self) {
        let state = net::probe(&self.config.api_base);
        print!("{}", ui::render(&state));
    }
}

