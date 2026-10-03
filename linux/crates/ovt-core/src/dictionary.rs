//! `dictionary.txt`, shared with the CLI and `dictate.sh` (follows `DictionaryFile.swift`): one term per line (Whisper and
//! the cleanup spell it exactly like this), or `heard => wanted` replacements applied after cleanup. `#` lines are
//! comments, kept at the top when the file is written back.

use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Replacement {
    pub heard: String,
    pub wanted: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dictionary {
    /// The file's own comment lines.
    pub header: String,
    pub terms: Vec<String>,
    pub replacements: Vec<Replacement>,
}

pub const TEMPLATE: &str = "# OpenVoiceType dictionary (also edited in Settings → Dictionary)
#
# One name or term per line: Whisper and Claude will spell it exactly like this.
#   Claude Code
# Replacements, applied after cleanup: heard => wanted
#   cloud code => Claude Code";

impl Default for Dictionary {
    fn default() -> Self {
        Dictionary { header: TEMPLATE.into(), terms: Vec::new(), replacements: Vec::new() }
    }
}

impl Dictionary {
    pub fn parse(text: &str) -> Self {
        let mut comments = Vec::new();
        let mut dictionary = Dictionary::default();
        for raw in text.lines() {
            let line = raw.trim();
            if line.starts_with('#') {
                comments.push(raw);
            } else if let Some((heard, wanted)) = line.split_once("=>") {
                let (heard, wanted) = (heard.trim(), wanted.trim());
                if !heard.is_empty() && !wanted.is_empty() {
                    dictionary.replacements.push(Replacement { heard: heard.into(), wanted: wanted.into() });
                }
            } else if !line.is_empty() {
                dictionary.terms.push(line.into());
            }
        }
        if !comments.is_empty() {
            dictionary.header = comments.join("\n");
        }
        dictionary
    }

    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path).map(|text| Self::parse(&text)).unwrap_or_default()
    }

    pub fn serialize(&self) -> String {
        let mut lines = vec![self.header.clone(), String::new()];
        lines.extend(self.terms.iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()));
        let rules: Vec<_> =
            self.replacements.iter().filter(|r| !r.heard.trim().is_empty() && !r.wanted.trim().is_empty()).collect();
        if !rules.is_empty() {
            lines.push(String::new());
        }
        lines.extend(rules.iter().map(|r| format!("{} => {}", r.heard.trim(), r.wanted.trim())));
        lines.join("\n") + "\n"
    }

    /// Writes the file atomically (a temporary file, then a rename).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let temporary = path.with_extension("txt.tmp");
        std::fs::write(&temporary, self.serialize())?;
        std::fs::rename(temporary, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let text = "# my words\nClaude Code\n  Kubernetes  \n\ncloud code => Claude Code\nbad =>\n";
        let dictionary = Dictionary::parse(text);
        assert_eq!(dictionary.header, "# my words");
        assert_eq!(dictionary.terms, vec!["Claude Code", "Kubernetes"]);
        assert_eq!(
            dictionary.replacements,
            vec![Replacement { heard: "cloud code".into(), wanted: "Claude Code".into() }]
        );
        assert_eq!(dictionary.serialize(), "# my words\n\nClaude Code\nKubernetes\n\ncloud code => Claude Code\n");
        assert_eq!(Dictionary::parse(&dictionary.serialize()), dictionary);
        assert_eq!(Dictionary::parse("").header, TEMPLATE);
    }
}
