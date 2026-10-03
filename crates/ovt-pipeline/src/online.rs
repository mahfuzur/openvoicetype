//! `is_offline`: no network, or a 1 s TCP connect to the engine's host fails (2 s with DNS). Skipped behind a proxy.

use crate::config::Config;

/// True when the online engine can't be reached. `host`/`port` default to `ONLINE_CHECK_HOST`:443.
pub fn is_offline(config: &Config, host: Option<(&str, u16)>) -> bool {
    todo!("is_offline")
}
