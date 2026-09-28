#![forbid(unsafe_code)]

use anyhow::{Context as _, Result, anyhow};
use serde_json::Value;
use slint::SharedString;
use std::{env, path::PathBuf, time::Duration};

slint::slint! {
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
            Text { text: "IndieBuild owns the product/control experience; local execution is delegated to the Scintilla desktop daemon."; wrap: word-wrap; }
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
    let app = DesktopApp::new()?;
    let daemon_url = env::var("GIW_DESKTOP_DAEMON_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8756".to_owned());
    let token = read_token()?;
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?;

    let weak = app.as_weak();
    app.on_refresh(move || {
        let text = match fetch_status(&client, &daemon_url, &token) {
            Ok(value) => serde_json::to_string_pretty(&value).unwrap_or_else(|error| error.to_string()),
            Err(error) => format!("daemon unavailable: {error}"),
        };
        if let Some(app) = weak.upgrade() {
            app.set_status_text(SharedString::from(text));
        }
    });

    app.set_status_text(SharedString::from("Press Refresh local status."));
    app.run()?;
    return Ok(());
}

fn fetch_status(client: &reqwest::blocking::Client, daemon_url: &str, token: &str) -> Result<Value> {
    let response = client
        .get(format!("{}/v1/status", daemon_url.trim_end_matches('/')))
        .bearer_auth(token)
        .send()?;
    let status = response.status();
    if !status.is_success() {
        return Err(anyhow!("daemon returned {status}"));
    }
    let value = response.json::<Value>().context("daemon returned invalid JSON")?;
    return Ok(value);
}

fn read_token() -> Result<String> {
    let path = if let Some(path) = env::var_os("GIW_DESKTOP_TOKEN_FILE") {
        PathBuf::from(path)
    } else {
        home_dir()?.join(".indiebuild/daemon/token")
    };
    let token = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read daemon token at {}", path.display()))?;
    return Ok(token.trim().to_owned());
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
