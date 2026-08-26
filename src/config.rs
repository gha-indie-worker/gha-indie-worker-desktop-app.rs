#![forbid(unsafe_code)]

#[derive(Clone, Debug)]
pub struct DesktopConfig {
    pub api_base: String,
}

impl DesktopConfig {
    pub fn from_env() -> Self {
        Self {
            api_base: std::env::var("GHA_INDIE_WORKER_API_BASE")
                .unwrap_or_else(|_| "http://127.0.0.1:8080".into()),
        }
    }
}

