//! `ovt`: the Rust pipeline with `dictate.sh`'s app contract, so `scripts/test-dictate.sh` and `evals/run.py` run against
//! both (`DICTATE=… scripts/test-dictate.sh`, `evals/run.py --bin …`).
//!
//!   ovt refine       clean up the transcript on stdin; exit 3 = Whisper's text, 4 = S1-mini replaced the online engine,
//!                    5 = the meaning guard used Whisper's text; details in $VTT_RESULT_FILE
//!   ovt command      Command Mode: the instruction on stdin, the plan in $VTT_COMMAND_FILE; exit 3 = nothing to paste
//!   ovt transcribe <wav>   Whisper's text (empty = no speech): the server on $WHISPER_PORT if it answers, else whisper-cli
//!   ovt file <wav>   transcribe + refine, with timings (no paste)
//!   ovt selftest     the same on synthesized speech (the Windows voice, `say` on macOS, espeak-ng + sox on Linux)

use ovt_pipeline::config::{Config, Job};
use ovt_pipeline::refine::{Report, Session};
use ovt_pipeline::whisper;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("help");
    let code = match command {
        "refine" => cleanup(Job::Cleanup),
        "command" => cleanup(Job::Command),
        "transcribe" => match args.get(1) {
            Some(wav) => transcribe(Path::new(wav)),
            None => usage("ovt transcribe <wav>"),
        },
        "file" => match args.get(1) {
            Some(wav) => file(Path::new(wav)),
            None => usage("ovt file <wav>"),
        },
        "selftest" => selftest(),
        "-h" | "--help" | "help" => {
            println!("{}", HELP.trim());
            0
        }
        other => {
            eprintln!("Unknown command: {other}");
            2
        }
    };
    ExitCode::from(code)
}

const HELP: &str = "
ovt refine | command | transcribe <wav> | file <wav> | selftest
The OpenVoiceType pipeline with dictate.sh's app contract (VTT_* variables, exit codes, VTT_RESULT_FILE).";

fn usage(text: &str) -> u8 {
    eprintln!("usage: {text}");
    2
}

/// `cmd_refine` / `cmd_command`: Claude is pre-started before stdin is read (the app writes the transcript when recording
/// stops). Empty input ends it without work. Like the script's `$(cat)`, trailing newlines are dropped.
fn cleanup(job: Job) -> u8 {
    let config = Config { job, ..Config::from_env() };
    let result_file = std::env::var_os("VTT_RESULT_FILE").filter(|f| !f.is_empty()).map(PathBuf::from);
    let session = Session::start(config);
    let mut input = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut input);
    let raw = String::from_utf8_lossy(&input);
    let raw = raw.trim_end_matches('\n');
    let empty = match job {
        Job::Cleanup => raw.is_empty(),
        Job::Command => raw.trim().is_empty(),
    };
    if empty {
        drop(session);
        return 0;
    }
    let report = session.finish(raw);
    if let Some(file) = result_file {
        let _ = std::fs::write(file, report.result_json());
    }
    print!("{}", report.result);
    report.exit_code() as u8
}

/// The Whisper settings the script reads: `WHISPER_PORT`, `VTT_WHISPER_MODEL`/`WHISPER_MODEL`, and whisper-cli from
/// `VTT_BIN_DIR` or `PATH`.
struct WhisperSetup {
    port: u16,
    cli: PathBuf,
    model: PathBuf,
}

impl WhisperSetup {
    fn from_env() -> Self {
        let var = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
        let port = var("WHISPER_PORT").and_then(|p| p.parse().ok()).unwrap_or(8179);
        let model = var("VTT_WHISPER_MODEL").or_else(|| var("WHISPER_MODEL")).map(PathBuf::from).unwrap_or_else(|| {
            let dir = ovt_core::paths::whisper_dir();
            ["ggml-large-v3-turbo-q5_0.bin", "ggml-large-v3-turbo.bin"]
                .iter()
                .map(|name| dir.join(name))
                .find(|path| path.is_file())
                .unwrap_or_else(|| dir.join("ggml-large-v3-turbo-q5_0.bin"))
        });
        let exe = if cfg!(windows) { "whisper-cli.exe" } else { "whisper-cli" };
        let cli = var("VTT_BIN_DIR")
            .map(|dir| PathBuf::from(dir).join(exe))
            .filter(|path| path.is_file())
            .unwrap_or_else(|| PathBuf::from(exe));
        WhisperSetup { port, cli, model }
    }
}

/// `cmd_transcribe`: the text, or nothing for no speech.
fn transcribe(wav: &Path) -> u8 {
    let config = Config::from_env();
    let setup = WhisperSetup::from_env();
    match whisper::transcribe(&config, wav, setup.port, &setup.cli, &setup.model) {
        Ok(transcript) => {
            print!("{}", transcript.text);
            0
        }
        Err(error) => {
            eprintln!("Transcription failed: {error}");
            1
        }
    }
}

/// `run_file`: transcribe and clean up a WAV, printing the raw and cleaned text and the timings.
fn file(wav: &Path) -> u8 {
    if !wav.is_file() {
        eprintln!("No such file: {}", wav.display());
        return 1;
    }
    let started = Instant::now();
    let config = Config::from_env();
    let setup = WhisperSetup::from_env();
    // Claude starts while Whisper runs (process_wav).
    let session = Session::start(config.clone());
    let transcript = match whisper::transcribe(&config, wav, setup.port, &setup.cli, &setup.model) {
        Ok(transcript) if !transcript.text.is_empty() => transcript,
        Ok(_) => {
            eprintln!("(no speech)");
            return 1;
        }
        Err(error) => {
            eprintln!("Transcription failed: {error}");
            return 1;
        }
    };
    let report = session.finish(&transcript.text);
    println!("raw:     {}", transcript.text);
    println!("cleaned: {}", report.result);
    println!(
        "audio={:.3}s whisper={}ms {} total={}ms",
        transcript.seconds,
        transcript.whisper_ms,
        timings(&report, &config),
        started.elapsed().as_millis()
    );
    if !report.guard.is_empty() {
        println!("guard:   dropped {}, used the raw text (rejected: {})", report.guard, report.rejected);
    }
    0
}

/// `cleanup_timings`.
fn timings(report: &Report, config: &Config) -> String {
    let mut line = format!("claude={}ms s1={}ms", report.claude_ms, report.s1_ms);
    if config.online_engine() == "openai" {
        line += &format!(" openai={}ms", report.openai_ms);
    }
    line += &format!(" refine={}", report.status);
    if !report.error.is_empty() {
        line += &format!(" error={}", report.error);
    }
    line
}

/// `selftest`: the script's sentence, synthesized, through the whole pipeline.
fn selftest() -> u8 {
    const TEXT: &str = "Um, so, like, we need to uh deploy the kubernetes cluster to a w s, and then, you know, update the docker image in git hub.";
    let dir = ovt_core::paths::state_dir();
    let _ = std::fs::create_dir_all(&dir);
    let wav = dir.join("selftest.wav");
    if let Err(error) = synthesize(TEXT, &wav) {
        eprintln!("selftest can't synthesize speech: {error}");
        return 1;
    }
    let code = file(&wav);
    let _ = std::fs::remove_file(&wav);
    code
}

/// Speech as a 16 kHz 16-bit mono WAV.
fn synthesize(text: &str, wav: &Path) -> std::io::Result<()> {
    use std::process::Command;
    let ok = |status: std::process::ExitStatus| {
        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(format!("exit {status}")))
        }
    };
    if cfg!(windows) {
        // The Windows voice (System.Speech), written straight in Whisper's format.
        let script = "Add-Type -AssemblyName System.Speech; \
            $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; \
            $f = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo 16000, ([System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen), ([System.Speech.AudioFormat.AudioChannel]::Mono); \
            $s.SetOutputToWaveFile($env:VTT_TTS_WAV, $f); $s.Speak($env:VTT_TTS_TEXT); $s.Dispose()";
        ok(Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("VTT_TTS_WAV", wav)
            .env("VTT_TTS_TEXT", text)
            .status()?)
    } else if cfg!(target_os = "macos") {
        ok(Command::new("say").arg("--data-format=LEI16@16000").arg("-o").arg(wav).arg(text).status()?)
    } else {
        let speech = wav.with_extension("tts.wav");
        ok(Command::new("espeak-ng").arg("-w").arg(&speech).arg(text).status()?)?;
        let status = Command::new("sox").arg(&speech).args(["-r", "16000", "-c", "1", "-b", "16"]).arg(wav).status();
        let _ = std::fs::remove_file(&speech);
        ok(status?)
    }
}
