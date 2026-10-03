//! The model servers (`srv_*` in `dictate.sh`): whisper-server on `WHISPER_PORT` (8179) and llama-server with S1-mini
//! on `S1_PORT` (8178), as child processes of the app (the script's pid files, watchdogs and signatures aren't needed:
//! the app lives as long as the servers). Started when recording starts, kept while used, stopped when idle.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

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

impl Spec {
    /// `srv_launch`'s arguments.
    pub fn args(&self) -> Vec<OsString> {
        let port = self.port.to_string();
        let mut args: Vec<OsString> = vec!["-m".into(), self.model.clone().into()];
        args.extend(["--host", "127.0.0.1", "--port", &port].map(OsString::from));
        match self.kind {
            Kind::S1 => args.extend(
                [
                    "--jinja",
                    "--chat-template-kwargs",
                    r#"{"enable_thinking":false}"#,
                    "--temp",
                    "0",
                    "-c",
                    "4096",
                    "-np",
                    "1",
                ]
                .map(OsString::from),
            ),
            Kind::Whisper => args.extend(["-nt", "-sns", "-l", &self.language].map(OsString::from)),
        }
        args
    }
}

/// A running server; ended (and waited for) when dropped.
pub struct Server {
    child: Child,
    spec: Spec,
    last_used: Instant,
}

/// How long `start` waits for `/health` (the first launch compiles GPU shaders).
const START_TIMEOUT: Duration = Duration::from_secs(30);

impl Server {
    /// Launches it and waits until `/health` answers (up to 30 s: the first launch compiles GPU shaders). Its output
    /// goes to error.log.
    pub fn start(spec: Spec, error_log: &std::path::Path) -> std::io::Result<Server> {
        if let Some(dir) = error_log.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let log = std::fs::OpenOptions::new().create(true).append(true).open(error_log)?;
        let mut command = Command::new(&spec.binary);
        command.args(spec.args()).stdin(Stdio::null()).stdout(log.try_clone()?).stderr(log);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let child = command.spawn()?;
        // From here on, an early return drops the server, which ends the process.
        let mut server = Server { child, spec, last_used: Instant::now() };
        let deadline = Instant::now() + START_TIMEOUT;
        loop {
            if let Some(status) = server.child.try_wait()? {
                return Err(std::io::Error::other(format!("{} exited ({status})", server.spec.binary.display())));
            }
            if server.healthy() {
                server.last_used = Instant::now();
                return Ok(server);
            }
            if Instant::now() >= deadline {
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "did not become ready in 30 s"));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    pub fn spec(&self) -> &Spec {
        &self.spec
    }

    /// `srv_healthy`.
    pub fn healthy(&self) -> bool {
        healthy(self.spec.port)
    }

    /// True while the process runs.
    pub fn running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Marks it used now (the script's `.used` file, for the idle timer).
    pub fn touch(&mut self) {
        self.last_used = Instant::now();
    }

    /// How long since it was started or last touched.
    pub fn idle(&self) -> Duration {
        self.last_used.elapsed()
    }

    /// Ends it (and waits for it).
    pub fn stop(self) {
        drop(self)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `srv_healthy`: `GET /health` on 127.0.0.1:`port` answers with "ok" within about a second (whisper-server and
/// llama-server answer `{"status":"ok"}` once the model is loaded; llama-server says 503 "Loading model" before).
pub fn healthy(port: u16) -> bool {
    let timeout = Duration::from_secs(1);
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let Ok(mut stream) = TcpStream::connect_timeout(&address, timeout) else { return false };
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));
    let request = format!("GET /health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut response = Vec::new();
    let _ = stream.take(64 * 1024).read_to_end(&mut response);
    String::from_utf8_lossy(&response).contains("\"ok\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn spec(kind: Kind) -> Spec {
        Spec { kind, binary: "server".into(), model: "/m/model.bin".into(), port: 8179, language: "en".into() }
    }

    #[test]
    fn launch_arguments_follow_srv_launch() {
        let args = |kind| spec(kind).args().into_iter().map(|a| a.into_string().unwrap()).collect::<Vec<_>>().join(" ");
        assert_eq!(args(Kind::Whisper), "-m /m/model.bin --host 127.0.0.1 --port 8179 -nt -sns -l en");
        assert_eq!(
            args(Kind::S1),
            r#"-m /m/model.bin --host 127.0.0.1 --port 8179 --jinja --chat-template-kwargs {"enable_thinking":false} --temp 0 -c 4096 -np 1"#
        );
    }

    #[test]
    fn health_check() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let answer = std::thread::spawn(move || {
            for body in [r#"{"status":"ok"}"#, r#"{"error":{"code":503,"message":"Loading model"}}"#] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0u8; 1024];
                let _ = stream.read(&mut request);
                let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}", body.len());
            }
        });
        assert!(healthy(port));
        assert!(!healthy(port));
        answer.join().unwrap();
        // Nothing listening any more.
        assert!(!healthy(port));
    }

    #[test]
    fn a_server_that_exits_is_an_error() {
        let dir = std::env::temp_dir().join(format!("ovt-servers-{}", std::process::id()));
        let missing = Spec { binary: dir.join("no-such-server"), ..spec(Kind::Whisper) };
        assert!(Server::start(missing, &dir.join("error.log")).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
