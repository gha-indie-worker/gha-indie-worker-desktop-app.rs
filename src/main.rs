#![forbid(unsafe_code)]

use anyhow::{Context as _, Result, anyhow, bail};
use serde_json::Value;
use slint::{SharedString, Timer, TimerMode};
use std::{
    env,
    io::Read,
    net::IpAddr,
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};

const DEFAULT_DAEMON_URL: &str = "http://127.0.0.1:8770";
const MAX_DAEMON_RESPONSE_BYTES: u64 = 1024 * 1024;
const MAX_TOKEN_FILE_BYTES: u64 = 16 * 1024;
const MAX_TOKEN_BYTES: usize = 4096;

slint::slint! {
    import { Button } from "std-widgets.slint";

    export component DesktopApp inherits Window {
        in property <string> status_text;
        callback refresh();

        title: "IndieBuild desktop";
        width: 760px;
        height: 500px;

        VerticalLayout {
            padding: 18px;
            spacing: 12px;
            Text { text: "IndieBuild local execution"; font-size: 24px; }
            Text { text: "IndieBuild owns the product/control experience; this UI talks only to the IndieBuild desktop daemon. The daemon delegates local execution to Scintilla."; wrap: word-wrap; }
            Button { text: "Refresh local status"; clicked => { root.refresh(); } }
            Rectangle {
                background: #202020;
                border-radius: 8px;
                Text {
                    text: root.status_text;
                    color: #eeeeee;
                    wrap: word-wrap;
                    x: 12px;
                    y: 12px;
                    width: parent.width - 24px;
                    height: parent.height - 24px;
                }
            }
        }
    }
}

fn main() -> Result<()> {
    let daemon_url =
        env::var("GIW_DESKTOP_DAEMON_URL").unwrap_or_else(|_| DEFAULT_DAEMON_URL.to_owned());
    let daemon_url = validate_daemon_url(&daemon_url)?;
    let token = read_token()?;
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let app = DesktopApp::new()?;
    let (tx, rx) = mpsc::channel::<String>();

    let refresh_client = client.clone();
    let refresh_url = daemon_url.clone();
    let refresh_token = token.clone();
    app.on_refresh(move || {
        let tx = tx.clone();
        let client = refresh_client.clone();
        let daemon_url = refresh_url.clone();
        let token = refresh_token.clone();
        std::thread::spawn(move || {
            let text = match fetch_status(&client, &daemon_url, &token) {
                Ok(value) => match serde_json::to_string_pretty(&value) {
                    Ok(value) => value,
                    Err(error) => format!("invalid status payload: {error}"),
                },
                Err(error) => format!("daemon unavailable: {error}"),
            };
            let _ = tx.send(text);
        });
    });

    let weak = app.as_weak();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(100), move || {
        while let Ok(text) = rx.try_recv() {
            if let Some(app) = weak.upgrade() {
                app.set_status_text(SharedString::from(text));
            }
        }
    });

    app.set_status_text(SharedString::from(
        "Press Refresh local status. Execution ownership stays in giw-desktop-daemon -> scintilla-desktop-daemon.",
    ));
    app.run()?;
    return Ok(());
}

fn parse_literal_loopback_host(host: &str) -> Result<IpAddr> {
    let normalized = host
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(host);
    let ip = normalized
        .parse::<IpAddr>()
        .context("GIW_DESKTOP_DAEMON_URL host must be a literal IP address")?;
    if !ip.is_loopback() {
        bail!("GIW_DESKTOP_DAEMON_URL must target a literal loopback address");
    }
    return Ok(ip);
}

fn validate_daemon_url(raw: &str) -> Result<String> {
    let url = reqwest::Url::parse(raw).context("GIW_DESKTOP_DAEMON_URL is not a valid URL")?;
    if url.scheme() != "http" {
        bail!("GIW_DESKTOP_DAEMON_URL must use http://");
    }
    if !url.username().is_empty() || url.password().is_some() {
        bail!("GIW_DESKTOP_DAEMON_URL must not contain credentials");
    }
    if url.query().is_some() || url.fragment().is_some() {
        bail!("GIW_DESKTOP_DAEMON_URL must not contain a query or fragment");
    }
    if url.path() != "/" && !url.path().is_empty() {
        bail!("GIW_DESKTOP_DAEMON_URL must not contain a base path");
    }
    if url.port().is_none() {
        bail!("GIW_DESKTOP_DAEMON_URL must include an explicit port");
    }
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("GIW_DESKTOP_DAEMON_URL must include a host"))?;
    let _ = parse_literal_loopback_host(host)?;
    return Ok(url.as_str().trim_end_matches('/').to_owned());
}

fn fetch_status(
    client: &reqwest::blocking::Client,
    daemon_url: &str,
    token: &str,
) -> Result<Value> {
    let response = client
        .get(format!("{daemon_url}/v1/status"))
        .bearer_auth(token)
        .send()?;
    let status = response.status();
    if !status.is_success() {
        return Err(anyhow!("daemon returned {status}"));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_DAEMON_RESPONSE_BYTES)
    {
        bail!("daemon response exceeds {MAX_DAEMON_RESPONSE_BYTES} bytes");
    }
    let mut body = Vec::new();
    response
        .take(MAX_DAEMON_RESPONSE_BYTES + 1)
        .read_to_end(&mut body)
        .context("failed to read daemon response")?;
    if body.len() as u64 > MAX_DAEMON_RESPONSE_BYTES {
        bail!("daemon response exceeds {MAX_DAEMON_RESPONSE_BYTES} bytes");
    }
    return serde_json::from_slice::<Value>(&body).context("daemon returned invalid JSON");
}

fn read_token() -> Result<String> {
    let path = if let Some(path) = env::var_os("GIW_DESKTOP_TOKEN_FILE") {
        PathBuf::from(path)
    } else {
        home_dir()?.join(".indiebuild/daemon/token")
    };
    validate_token_file(&path)?;
    let token = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read daemon token at {}", path.display()))?;
    let token = token.trim();
    if token.len() < 32 || token.len() > MAX_TOKEN_BYTES || token.chars().any(char::is_whitespace) {
        bail!("daemon token is malformed");
    }
    return Ok(token.to_owned());
}

fn validate_token_file(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)
        .with_context(|| format!("cannot inspect daemon token at {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        bail!("daemon token must be a regular non-symlink file");
    }
    if metadata.len() == 0 || metadata.len() > MAX_TOKEN_FILE_BYTES {
        bail!("daemon token file has an invalid size");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            bail!("daemon token file must not be accessible by group or others");
        }
    }
    return Ok(());
}

fn home_dir() -> Result<PathBuf> {
    if let Some(home) = env::var_os("HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(home));
    }
    if let Some(profile) = env::var_os("USERPROFILE").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(profile));
    }
    let drive = env::var_os("HOMEDRIVE").filter(|value| !value.is_empty());
    let path = env::var_os("HOMEPATH").filter(|value| !value.is_empty());
    if let (Some(drive), Some(path)) = (drive, path) {
        let mut value = PathBuf::from(drive);
        value.push(path);
        return Ok(value);
    }
    return Err(anyhow!("cannot determine user home directory"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_url_requires_literal_loopback_http() {
        assert!(validate_daemon_url("http://127.0.0.1:8770").is_ok());
        assert!(validate_daemon_url("http://[::1]:8770").is_ok());
        assert!(validate_daemon_url("http://localhost:8770").is_err());
        assert!(validate_daemon_url("https://127.0.0.1:8770").is_err());
        assert!(validate_daemon_url("http://127.0.0.1:8770/base").is_err());
        assert!(validate_daemon_url("http://user:pass@127.0.0.1:8770").is_err());
        assert!(validate_daemon_url("http://192.0.2.1:8770").is_err());
    }

    #[test]
    fn default_daemon_port_matches_product_daemon() {
        assert_eq!(DEFAULT_DAEMON_URL, "http://127.0.0.1:8770");
    }
}
