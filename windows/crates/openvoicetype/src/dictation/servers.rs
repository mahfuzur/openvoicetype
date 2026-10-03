//! The model servers the app keeps loaded (follows the Mac app's use of `dictate.sh whisper-server|s1-server start`):
//! started in the background when recording starts, kept while they're used and healthy, stopped when idle. A start
//! can take 30 s (the first launch compiles GPU shaders), so only transcribing waits for it.

use super::worker::guarded;
use crate::applog;
use ovt_pipeline::servers::{Server, Spec};
use std::thread::JoinHandle;

pub(super) enum Slot {
    Ready(Option<Server>),
    Starting(JoinHandle<Option<Server>>),
}

impl Default for Slot {
    fn default() -> Self {
        Slot::Ready(None)
    }
}

impl Slot {
    /// Starts the server for `spec` in the background, or keeps the running one when it's healthy and has the same
    /// model. It ends up empty when its binary or model is missing (whisper-cli is then the fallback) or it fails.
    pub(super) fn prepare(&mut self, spec: Spec) {
        let previous = std::mem::take(self);
        *self = Slot::Starting(std::thread::spawn(move || {
            if let Some(server) = previous.wait() {
                if guarded(|| server.spec().model == spec.model && server.healthy()).unwrap_or(false) {
                    return Some(server);
                }
                guarded(|| server.stop());
            }
            start(spec)
        }));
    }

    /// Takes over a server started elsewhere (S1-mini started by the cleanup's fallback), replacing the current one.
    pub(super) fn adopt(&mut self, server: Server) {
        self.stop();
        *self = Slot::Ready(Some(server));
    }

    /// Waits for a start in progress.
    pub(super) fn ready(&mut self) {
        if let Slot::Starting(_) = self {
            *self = Slot::Ready(std::mem::take(self).wait());
        }
    }

    /// Stops it (waiting for a start in progress first).
    pub(super) fn stop(&mut self) {
        if let Some(server) = std::mem::take(self).wait() {
            guarded(|| server.stop());
        }
    }

    /// Stops it unless it's still starting (an idle check must never block).
    pub(super) fn stop_if_ready(&mut self) {
        if let Slot::Ready(_) = self {
            self.stop();
        }
    }

    fn wait(self) -> Option<Server> {
        match self {
            Slot::Ready(server) => server,
            Slot::Starting(handle) => handle.join().ok().flatten(),
        }
    }
}

pub(super) fn start(spec: Spec) -> Option<Server> {
    if !spec.binary.is_file() || !spec.model.is_file() {
        applog::write(&format!("SERVER {} or its model is missing", spec.binary.display()));
        return None;
    }
    let error_log = ovt_core::paths::log_dir().join("error.log");
    let binary = spec.binary.display().to_string();
    guarded(|| Server::start(spec, &error_log))?
        .map_err(|e| applog::write(&format!("SERVER {binary} didn't start: {e}")))
        .ok()
}
