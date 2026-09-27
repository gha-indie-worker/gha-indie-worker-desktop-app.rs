# gha-indie-worker-desktop-app.rs

Native Rust desktop application for IndieBuild. No webviews, React, or JSX.

## Desktop control plane

This application is a peer client of `giw-desktop-daemon`, alongside `giw-desktop-cli` and the
Flutter application. It does not supervise worker processes itself and it does not shell out to the
CLI for normal control operations.

```text
gha-indie-worker-desktop-app.rs --+
                                 |
giw-desktop-cli -----------------+--> giw-desktop-daemon --> giw-desktop-infra desired state
                                 |
gha-indie-worker-flutter --------+
```

The app:

- reads the local daemon token from `~/.giw/desktop/token` by default;
- refuses non-loopback daemon control endpoints;
- checks daemon protocol compatibility;
- reports managed service, tunnel, and keep-awake state;
- exposes typed actions for reconcile, process start/stop/restart, tunnel start/stop, and keep-awake;
- keeps the hosted `https://indiebuild.dev` API endpoint separate from the local daemon endpoint.

Environment overrides:

- `GHA_INDIE_WORKER_API_BASE` — hosted product API (default `https://indiebuild.dev`).
- `GIW_DESKTOP_URL` — loopback desktop daemon URL (default `http://127.0.0.1:18440`).
- `GIW_DESKTOP_TOKEN_FILE` — local daemon token file.

`src/ui.rs` remains a native UI boundary. A richer Rust-native toolkit can replace the current simple renderer
without changing the daemon/client trust model.
