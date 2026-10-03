//! `golden/text.json`, made by `golden/make-text.sh` from `dictate.sh`'s own functions: `text.rs` must give the
//! script's output for every case.

use chrono::{TimeZone, Utc};
use ovt_pipeline::config::{Config, Job};
use ovt_pipeline::text::{self, Dictionary};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The case's config: the script's variables (`vars`) through `Config::from_vars`, and its files written to `dir`.
fn config(case: &Value, dir: &Path) -> Config {
    let mut vars: HashMap<String, String> = HashMap::new();
    for (name, value) in case["vars"].as_object().into_iter().flatten() {
        let name = match name.as_str() {
            "APP_NAME" => "VTT_APP",
            other => other,
        };
        vars.insert(name.into(), value.as_str().expect("string").into());
    }
    let prompts = case["prompts_dir"].as_str().unwrap_or("prompts");
    vars.insert("VTT_PROMPTS_DIR".into(), repo().join(prompts).to_string_lossy().into());
    let prompt_file = dir.join("prompt.txt");
    if let Some(text) = case["prompt_file"].as_str() {
        std::fs::write(&prompt_file, text).unwrap();
    }
    vars.insert("PROMPT_FILE".into(), prompt_file.to_string_lossy().into());
    let command_file = dir.join("command.json");
    match &case["command"] {
        Value::Null => {}
        Value::String(raw) => std::fs::write(&command_file, raw).unwrap(),
        object => std::fs::write(&command_file, object.to_string()).unwrap(),
    }
    if !case["command"].is_null() || case["command_missing"] == json!(true) {
        vars.insert("VTT_COMMAND_FILE".into(), command_file.to_string_lossy().into());
    }
    let mut config = Config::from_vars(|name| vars.get(name).cloned());
    if vars.get("JOB").map(String::as_str) == Some("command") {
        config.job = Job::Command;
    }
    config
}

fn run(case: &Value, dir: &Path) -> Value {
    std::fs::create_dir_all(dir).unwrap();
    let config = config(case, dir);
    let dictionary = Dictionary::parse(case["dictionary"].as_str().unwrap_or(""));
    let arg = |i: usize| case["args"][i].as_str().expect("argument").to_string();
    let text = match case["fn"].as_str().unwrap() {
        "load_dictionary" => {
            let replacements: Vec<_> =
                dictionary.replacements.iter().map(|(from, to)| format!("{from}\t{to}")).collect();
            return json!({ "terms": dictionary.terms, "replacements": replacements });
        }
        "is_hallucination" => return json!(text::is_hallucination(&arg(0))),
        "meaning_guard" => return json!(text::meaning_guard(&arg(0), &arg(1))),
        "trim" => text::trim(&arg(0)).to_string(),
        "word_count" => text::word_count(&arg(0)).to_string(),
        "vocabulary" => text::vocabulary(&config, &dictionary),
        "whisper_prompt" => text::whisper_prompt(&config, &dictionary),
        "clean_transcript" => text::clean_transcript(&arg(0)),
        "system_prompt" => text::system_prompt(&config),
        "user_message" => text::user_message(&config, &dictionary, &arg(0)),
        "command_message" => text::command_message(&config, &dictionary, &arg(0)),
        "command_source" => text::command_source(&config),
        "post_process" => text::post_process(&config, &dictionary, &arg(0)),
        "s1_control_line" => text::s1_control_line(&config.mode).to_string(),
        // Not today, like every case (make-text.sh checks).
        "format_resets" => text::format_resets_at(&arg(0), Utc.with_ymd_and_hms(2099, 6, 15, 12, 0, 0).unwrap()),
        "log_safe" => text::log_safe(&arg(0), config.log_text),
        other => panic!("unknown function {other}"),
    };
    json!(text)
}

#[test]
fn text_functions_match_dictate_sh() {
    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("golden/text.json");
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(golden).unwrap()).unwrap();
    let dir = std::env::temp_dir().join(format!("ovt-golden-text-{}", std::process::id()));
    let mut failures = Vec::new();
    for (i, case) in cases.iter().enumerate() {
        let got = run(case, &dir.join(i.to_string()));
        if got != case["expected"] {
            failures.push(format!("{}:\n  script: {}\n  rust:   {}", case["name"], case["expected"], got));
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(failures.is_empty(), "{} of {} cases differ:\n{}", failures.len(), cases.len(), failures.join("\n"));
    assert!(cases.len() >= 120, "only {} cases", cases.len());
}
