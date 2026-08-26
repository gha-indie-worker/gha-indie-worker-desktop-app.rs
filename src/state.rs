#![forbid(unsafe_code)]

#[derive(Clone, Debug, Default)]
pub struct DesktopState {
    pub connected: bool,
    pub endpoint: String,
}

