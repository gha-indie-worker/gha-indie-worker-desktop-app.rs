#![forbid(unsafe_code)]

#[derive(Clone, Debug, Default)]
pub struct DesktopState {
    pub connected: bool,
    pub endpoint: String,
    pub mode: Option<String>,
    pub services: Vec<ServiceState>,
    pub tunnel_running: bool,
    pub keep_awake: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ServiceState {
    pub name: String,
    pub running: bool,
    pub pid: Option<u32>,
}
