#![forbid(unsafe_code)]

use std::{fs, path::Path};

use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::{json, Value};

const SUPPORTED_PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize)]
pub struct ProcessStatus {
    pub name: String,
    pub running: bool,
    pub pid: Option<u32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TunnelStatus {
    pub name: String,
    pub hostname: Option<String>,
    pub service_url: String,
    pub running: bool,
    pub pid: Option<u32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DaemonStatus {
    pub protocol_version: u32,
    pub mode: String,
    pub bind: String,
    pub manifest_version: u32,
    pub services: Vec<ProcessStatus>,
    pub tunnel: Option<TunnelStatus>,
    pub keep_awake: bool,
}

#[derive(Debug)]
pub enum DaemonClientError {
    InvalidEndpoint(String),
    Token(String),
    Http(String),
    Protocol(String),
}

impl std::fmt::Display for DaemonClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidEndpoint(message)
            | Self::Token(message)
            | Self::Http(message)
            | Self::Protocol(message) => {
                return write!(f, "{message}");
            }
        }
    }
}

impl std::error::Error for DaemonClientError {}

pub struct DaemonClient {
    base_url: String,
    token: String,
    http: Client,
}

impl DaemonClient {
    pub fn from_token_file(base_url: &str, token_file: &Path) -> Result<Self, DaemonClientError> {
        validate_loopback_url(base_url)?;
        let raw = fs::read_to_string(token_file).map_err(|error| {
            DaemonClientError::Token(format!(
                "failed to read GIW desktop daemon token at {}: {error}",
                token_file.display()
            ))
        })?;
        let token = raw.trim();
        if token.len() < 32 || token.chars().any(char::is_whitespace) {
            return Err(DaemonClientError::Token(format!(
                "GIW desktop daemon token at {} is malformed",
                token_file.display()
            )));
        }

        return Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            http: Client::new(),
        });
    }

    pub fn status(&self) -> Result<DaemonStatus, DaemonClientError> {
        let value = self.get_json("/v1/status")?;
        let status: DaemonStatus = serde_json::from_value(value).map_err(|error| {
            DaemonClientError::Protocol(format!("invalid GIW daemon status payload: {error}"))
        })?;
        ensure_protocol(status.protocol_version)?;
        return Ok(status);
    }

    pub fn reconcile(&self) -> Result<Value, DaemonClientError> {
        return self.post_json("/v1/reconcile", None);
    }

    pub fn start_process(&self, name: &str) -> Result<Value, DaemonClientError> {
        validate_process_name(name)?;
        return self.post_json(&format!("/v1/processes/{name}/start"), None);
    }

    pub fn stop_process(&self, name: &str) -> Result<Value, DaemonClientError> {
        validate_process_name(name)?;
        return self.post_json(&format!("/v1/processes/{name}/stop"), None);
    }

    pub fn restart_process(&self, name: &str) -> Result<Value, DaemonClientError> {
        validate_process_name(name)?;
        return self.post_json(&format!("/v1/processes/{name}/restart"), None);
    }

    pub fn start_tunnel(&self) -> Result<Value, DaemonClientError> {
        return self.post_json("/v1/tunnel/start", None);
    }

    pub fn stop_tunnel(&self) -> Result<Value, DaemonClientError> {
        return self.post_json("/v1/tunnel/stop", None);
    }

    pub fn set_keep_awake(&self, enabled: bool) -> Result<Value, DaemonClientError> {
        return self.post_json("/v1/power/keep-awake", Some(json!({"enabled": enabled})));
    }

    pub fn apply_update(&self) -> Result<Value, DaemonClientError> {
        return self.post_json("/v1/updates/apply", None);
    }

    fn get_json(&self, path: &str) -> Result<Value, DaemonClientError> {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .http
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .map_err(|error| {
                DaemonClientError::Http(format!("daemon GET {url} failed: {error}"))
            })?;
        return parse_response(response, &url);
    }

    fn post_json(&self, path: &str, body: Option<Value>) -> Result<Value, DaemonClientError> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = self.http.post(&url).bearer_auth(&self.token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().map_err(|error| {
            DaemonClientError::Http(format!("daemon POST {url} failed: {error}"))
        })?;
        return parse_response(response, &url);
    }
}

fn parse_response(
    response: reqwest::blocking::Response,
    url: &str,
) -> Result<Value, DaemonClientError> {
    let status = response.status();
    let text = response
        .text()
        .map_err(|error| DaemonClientError::Http(format!("failed reading {url}: {error}")))?;
    let value: Value = serde_json::from_str(&text).map_err(|error| {
        DaemonClientError::Protocol(format!("daemon returned non-JSON from {url}: {error}"))
    })?;

    if !status.is_success() {
        return Err(DaemonClientError::Http(format!(
            "daemon request to {url} failed with {status}: {value}"
        )));
    }

    if let Some(version) = value.get("protocol_version").and_then(Value::as_u64) {
        ensure_protocol(version as u32)?;
    }
    return Ok(value);
}

fn validate_loopback_url(base_url: &str) -> Result<(), DaemonClientError> {
    if base_url.starts_with("http://127.0.0.1:")
        || base_url.starts_with("http://localhost:")
        || base_url.starts_with("http://[::1]:")
    {
        return Ok(());
    }

    return Err(DaemonClientError::InvalidEndpoint(format!(
        "GIW desktop daemon endpoint must be loopback HTTP; got {base_url:?}"
    )));
}

fn validate_process_name(name: &str) -> Result<(), DaemonClientError> {
    if name.trim().is_empty() || name.contains('/') {
        return Err(DaemonClientError::Protocol(
            "daemon process name must be a non-empty manifest name without '/'".to_string(),
        ));
    }
    return Ok(());
}

fn ensure_protocol(version: u32) -> Result<(), DaemonClientError> {
    if version > SUPPORTED_PROTOCOL_VERSION {
        return Err(DaemonClientError::Protocol(format!(
            "GIW daemon protocol {version} is newer than this desktop app supports ({SUPPORTED_PROTOCOL_VERSION})"
        )));
    }
    return Ok(());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_remote_control_endpoints() {
        assert!(validate_loopback_url("http://127.0.0.1:18440").is_ok());
        assert!(validate_loopback_url("https://indiebuild.dev").is_err());
    }

    #[test]
    fn rejects_path_like_process_names() {
        assert!(validate_process_name("build-server").is_ok());
        assert!(validate_process_name("../../bin/sh").is_err());
    }
}
