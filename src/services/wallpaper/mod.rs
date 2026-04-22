// SPDX-FileCopyrightText: 2026 SOV710
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! `wallpaper` service — wallpaper source/view state plus renderer supervision.
//!
//! The daemon owns:
//! - wallpaper directory discovery
//! - source and per-output view selection
//! - process supervision for the dedicated wallpaper renderer
//!
//! The renderer is isolated in a separate process (`qsov-wallpaper-renderer`) so the
//! main shell no longer carries wallpaper video decode / render load.

mod actions;
mod config;
mod model;
mod renderer;
mod scan;

use serde_json::Value;
use tokio::sync::{mpsc, watch};
use tracing::info;

use crate::bus::{ServiceHandle, ServiceRequest};
use crate::config::Config;
use crate::wallpaper_contract::{WALLPAPER_LAYER_NAMESPACE, WALLPAPER_TOPIC};

use self::actions::WallpaperAction;
use self::config::WallpaperCfg;
use self::model::WallpaperState;
use self::renderer::supervise_renderer;

/// Spawn the `wallpaper` service and return its [`ServiceHandle`].
pub fn spawn(cfg: &Config) -> ServiceHandle {
    let cfg = WallpaperCfg::from_config(cfg);
    let mut state = WallpaperState::new(&cfg);
    state.rescan();

    let (state_tx, state_rx) = watch::channel(state.snapshot());
    let (request_tx, request_rx) = mpsc::channel(16);
    let (renderer_tx, renderer_rx) = watch::channel(model::RendererRuntime::starting());

    tokio::spawn(supervise_renderer(cfg.clone(), renderer_tx));
    tokio::spawn(run(request_rx, state_tx, renderer_rx, state));

    ServiceHandle {
        request_tx,
        state_rx,
        events_tx: None,
    }
}

async fn run(
    mut request_rx: mpsc::Receiver<ServiceRequest>,
    state_tx: watch::Sender<Value>,
    mut renderer_rx: watch::Receiver<model::RendererRuntime>,
    mut state: WallpaperState,
) {
    info!(path = %state.directory().display(), "wallpaper service started");
    info!(
        topic = WALLPAPER_TOPIC,
        renderer_layer_namespace = WALLPAPER_LAYER_NAMESPACE,
        "wallpaper runtime contract loaded"
    );

    state.set_renderer(renderer_rx.borrow().clone());
    state_tx.send_replace(state.snapshot());

    loop {
        tokio::select! {
            maybe_req = request_rx.recv() => {
                let Some(req) = maybe_req else {
                    break;
                };

                let result = WallpaperAction::parse(&req.action, &req.payload)
                    .and_then(|action| action.apply(&mut state));

                state_tx.send_replace(state.snapshot());
                req.reply.send(result).ok();
            }
            changed = renderer_rx.changed() => {
                if changed.is_err() {
                    break;
                }
                state.set_renderer(renderer_rx.borrow_and_update().clone());
                state_tx.send_replace(state.snapshot());
            }
        }
    }

    info!("wallpaper service stopped");
}
