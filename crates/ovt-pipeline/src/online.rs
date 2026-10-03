//! `is_offline`: no network, or a 1 s TCP connect to the engine's host fails (2 s with DNS). Skipped behind a proxy.

use crate::config::Config;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

/// True when the online engine can't be reached. `host`/`port` default to `ONLINE_CHECK_HOST`:443.
///
/// `is_offline` without `has_default_route` (there's no portable check): a missing route fails the connect at once.
pub fn is_offline(config: &Config, host: Option<(&str, u16)>) -> bool {
    if config.force_offline {
        return true;
    }
    let proxy = ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"]
        .iter()
        .any(|name| std::env::var_os(name).is_some_and(|v| !v.is_empty()));
    if !config.online_check || proxy {
        return false;
    }
    let (host, port) = host.unwrap_or((&config.online_check_host, 443));
    !tcp_reachable(host, port)
}

/// `tcp_reachable`: a TCP connect within 1 s, 2 s in all with DNS (resolved on a thread: std's lookup has no timeout).
pub fn tcp_reachable(host: &str, port: u16) -> bool {
    let start = Instant::now();
    let total = Duration::from_secs(2);
    let (send, receive) = std::sync::mpsc::channel();
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    let target = if bare.contains(':') { format!("[{bare}]:{port}") } else { format!("{bare}:{port}") };
    std::thread::spawn(move || {
        let _ = send.send(target.to_socket_addrs().map(|a| a.collect::<Vec<SocketAddr>>()));
    });
    let Ok(Ok(addresses)) = receive.recv_timeout(total) else { return false };
    for address in addresses {
        let left = total.saturating_sub(start.elapsed());
        if left.is_zero() {
            return false;
        }
        if TcpStream::connect_timeout(&address, left.min(Duration::from_secs(1))).is_ok() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_a_tcp_connect() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let config = Config::default();
        assert!(!is_offline(&config, Some(("127.0.0.1", port))));
        assert!(!is_offline(&config, Some(("localhost", port))));
        drop(listener);
        assert!(is_offline(&config, Some(("127.0.0.1", port))));
        assert!(is_offline(&Config { force_offline: true, ..Config::default() }, Some(("127.0.0.1", port))));
        assert!(!is_offline(&Config { online_check: false, ..Config::default() }, Some(("127.0.0.1", port))));
        assert!(is_offline(&config, Some(("no-such-host.invalid", 443))));
    }
}
