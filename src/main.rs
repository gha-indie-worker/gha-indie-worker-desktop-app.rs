#![deny(unsafe_code)]

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
        width: 900px;
        height: 640px;
        background: #07111f;

        VerticalLayout {
            padding: 28px;
            spacing: 18px;

            HorizontalLayout {
                spacing: 18px;
                VerticalLayout {
                    spacing: 5px;
                    Text {
                        text: "IndieBuild local execution";
                        color: #eef6ff;
                        font-size: 30px;
                        font-weight: 700;
                    }
                    Text {
                        text: "Read-only visibility into the canonical desktop execution chain";
                        color: #a8b8c8;
                        font-size: 15px;
                    }
                }
                Rectangle { horizontal-stretch: 1; }
                Button {
                    text: "Refresh status";
                    clicked => { root.refresh(); }
                }
            }

            Rectangle { height: 1px; background: #24364b; }

            HorizontalLayout {
                spacing: 14px;
                Rectangle {
                    height: 104px;
                    background: #0d1929;
                    border-radius: 12px;
                    border-width: 1px;
                    border-color: #24364b;
                    VerticalLayout {
                        padding: 16px;
                        spacing: 6px;
                        Text { text: "Product owner"; color: #a8b8c8; font-size: 13px; }
                        Text { text: "IndieBuild"; color: #eef6ff; font-size: 18px; font-weight: 600; }
                    }
                }
                Rectangle {
                    height: 104px;
                    background: #0d1929;
                    border-radius: 12px;
                    border-width: 1px;
                    border-color: #24364b;
                    VerticalLayout {
                        padding: 16px;
                        spacing: 6px;
                        Text { text: "Execution owner"; color: #a8b8c8; font-size: 13px; }
                        Text { text: "GIW daemon → Scintilla daemon"; color: #eef6ff; font-size: 18px; font-weight: 600; wrap: word-wrap; }
                    }
                }
                Rectangle {
                    height: 104px;
                    background: #0d1929;
                    border-radius: 12px;
                    border-width: 1px;
                    border-color: #24364b;
                    VerticalLayout {
                        padding: 16px;
                        spacing: 6px;
                        Text { text: "Control mode"; color: #a8b8c8; font-size: 13px; }
                        Text { text: "Read-only status"; color: #eef6ff; font-size: 18px; font-weight: 600; }
                    }
                }
            }

            Text {
                text: "Local runtime status";
                color: #eef6ff;
                font-size: 19px;
                font-weight: 600;
            }

            Rectangle {
                vertical-stretch: 1;
                background: #0d1929;
                border-radius: 12px;
                border-width: 1px;
                border-color: #24364b;
                Text {
                    text: root.status_text;
                    color: #dce9f7;
                    wrap: word-wrap;
                    x: 18px;
                    y: 18px;
                    width: parent.width - 36px;
                    height: parent.height - 36px;
                }
            }

            Text {
                text: "Safety: credentials and raw daemon diagnostics are never rendered. This client reads canonical /v1/status only; lifecycle and execution remain daemon-owned.";
                color: #70869f;
                font-size: 12px;
                wrap: word-wrap;
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
                Ok(value) => render_status_summary(&value),
                Err(_) => {
                    "Local status is unavailable. Start the IndieBuild desktop daemon and try again."
                        .to_owned()
                }
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
        "Press Refresh status. Execution ownership stays in giw-desktop-daemon → scintilla-desktop-daemon.",
    ));
    app.run()?;
    Ok(())
}

fn render_status_summary(value: &Value) -> String {
    let Some(object) = value.as_object() else {
        return "Daemon online · valid status received.".to_owned();
    };

    let mut fields = Vec::new();
    for key in [
        "product",
        "execution_backend",
        "isolation",
        "worker_reuse",
        "in_flight_dispatches",
        "max_in_flight_dispatches",
        "uptime_ms",
    ] {
        let Some(value) = object.get(key) else {
            continue;
        };
        let rendered = match value {
            Value::String(value) => value.clone(),
            Value::Bool(value) => value.to_string(),
            Value::Number(value) => value.to_string(),
            _ => continue,
        };
        fields.push(format!("{key}={}", truncate_for_ui(&rendered, 96)));
    }

    if let Some(scintilla) = object.get("scintilla").and_then(Value::as_object) {
        for key in ["status", "connected", "healthy", "version"] {
            let Some(value) = scintilla.get(key) else {
                continue;
            };
            let rendered = match value {
                Value::String(value) => value.clone(),
                Value::Bool(value) => value.to_string(),
                Value::Number(value) => value.to_string(),
                _ => continue,
            };
            fields.push(format!(
                "scintilla.{key}={}",
                truncate_for_ui(&rendered, 96)
            ));
        }
    }

    if fields.is_empty() {
        "Daemon online · valid status received.".to_owned()
    } else {
        format!("Daemon online · {}", fields.join(" · "))
    }
}

fn truncate_for_ui(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let prefix = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
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
    Ok(ip)
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
    Ok(url.as_str().trim_end_matches('/').to_owned())
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
    serde_json::from_slice::<Value>(&body).context("daemon returned invalid JSON")
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
    Ok(token.to_owned())
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
    Ok(())
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
    Err(anyhow!("cannot determine user home directory"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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

    #[test]
    fn status_summary_is_human_readable_and_bounded() {
        let status = json!({
            "product": "gha-indie-worker",
            "execution_backend": "scintilla",
            "isolation": "worker",
            "in_flight_dispatches": 2,
            "max_in_flight_dispatches": 8,
            "scintilla": {"healthy": true}
        });
        let rendered = render_status_summary(&status);
        assert!(rendered.contains("product=gha-indie-worker"));
        assert!(rendered.contains("execution_backend=scintilla"));
        assert!(rendered.contains("scintilla.healthy=true"));
        assert!(!rendered.contains('{'));
    }

    #[test]
    fn truncation_is_unicode_safe() {
        assert_eq!(truncate_for_ui("abcdef", 6), "abcdef");
        assert_eq!(truncate_for_ui("abcdefg", 6), "abcdef…");
        assert_eq!(truncate_for_ui("ééé", 2), "éé…");
    }
}
