//! The model servers (`srv_*` in `dictate.sh`): whisper-server on `WHISPER_PORT` (8179) and llama-server with S1-mini
//! on `S1_PORT` (8178), as child processes of the app (the script's pid files, watchdogs and signatures aren't needed:
//! the app lives as long as the servers). Started when recording starts, kept while used, stopped when idle.

use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Whisper,
    S1,
}

/// What to run: the server binary and the model, with `srv_launch`'s arguments.
#[derive(Clone, Debug)]
pub struct Spec {
    pub kind: Kind,
    pub binary: PathBuf,
    pub model: PathBuf,
    pub port: u16,
    /// Whisper's language (`LANGUAGE`, default "en").
    pub language: String,
}

pub struct Server {
    // private: the child, the spec, last used
}

impl Server {
    /// Launches it and waits until `/health` answers (up to 30 s: the first launch compiles GPU shaders). Its output
    /// goes to error.log.
    pub fn start(spec: Spec, error_log: &std::path::Path) -> std::io::Result<Server> {
        todo!("srv_launch + srv_start's wait")
    }

    pub fn spec(&self) -> &Spec {
        todo!()
    }

    /// `srv_healthy`.
    pub fn healthy(&self) -> bool {
        todo!("srv_healthy")
    }

    /// Ends it (and waits for it).
    pub fn stop(self) {
        todo!("srv_stop")
    }
}
