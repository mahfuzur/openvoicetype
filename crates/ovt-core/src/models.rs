//! The models the app can download (follows `ModelManager.swift`). URLs are pinned to a Hugging Face commit, so the
//! SHA-256 always matches; `scripts/install.sh` pins the same files. Keep all three in sync.

use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Whisper,
    S1Mini,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelFile {
    pub file_name: &'static str,
    pub title: &'static str,
    pub detail: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
    pub kind: Kind,
}

const WHISPER_REPO: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/";
const S1_REPO: &str =
    "https://huggingface.co/superwhisper/s1-mini-GGUF/resolve/34add00a48a2e5d24e5a4ee5405a99620a3a240c/";

pub const DEFAULT_WHISPER: ModelFile = ModelFile {
    file_name: "ggml-large-v3-turbo-q5_0.bin",
    title: "Compressed (recommended)",
    detail: "large-v3-turbo, compressed. Same accuracy in our tests, less than half the memory.",
    bytes: 574_041_195,
    sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
    kind: Kind::Whisper,
};

pub const FULL_WHISPER: ModelFile = ModelFile {
    file_name: "ggml-large-v3-turbo.bin",
    title: "Full",
    detail: "large-v3-turbo at full precision. A bigger download and about 1.7 GB of memory while loaded.",
    bytes: 1_624_555_275,
    sha256: "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69",
    kind: Kind::Whisper,
};

pub const FAST_WHISPER: ModelFile = ModelFile {
    file_name: "ggml-base.en.bin",
    title: "Fast",
    detail: "base.en: small and quick, but noticeably less accurate. For older or low-memory computers.",
    bytes: 147_964_211,
    sha256: "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002",
    kind: Kind::Whisper,
};

pub const S1_MINI: ModelFile = ModelFile {
    file_name: "s1-mini-q4_k_m.gguf",
    title: "S1-mini by Superwhisper",
    detail: "Cleans up text on this computer, with no internet. English only.",
    bytes: 484_219_808,
    sha256: "3b41ebe2502cbd03e811d5d16b022f5ab551eda58d62597d152f89535003c634",
    kind: Kind::S1Mini,
};

pub const WHISPER: [ModelFile; 3] = [DEFAULT_WHISPER, FULL_WHISPER, FAST_WHISPER];

impl ModelFile {
    pub fn url(&self) -> String {
        match self.kind {
            Kind::Whisper => format!("{WHISPER_REPO}{}", self.file_name),
            Kind::S1Mini => format!("{S1_REPO}{}", self.file_name),
        }
    }

    pub fn directory(&self) -> PathBuf {
        match self.kind {
            Kind::Whisper => crate::paths::whisper_dir(),
            Kind::S1Mini => crate::paths::s1_dir(),
        }
    }

    pub fn path(&self) -> PathBuf {
        self.directory().join(self.file_name)
    }

    /// A symlink to a copy elsewhere counts as installed (follows it, like the macOS app).
    pub fn is_installed(&self) -> bool {
        self.path().exists()
    }
}

pub fn whisper_named(file_name: &str) -> Option<ModelFile> {
    WHISPER.into_iter().find(|m| m.file_name == file_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_matches_install_sh() {
        let install = include_str!("../../../scripts/install.sh");
        for model in [DEFAULT_WHISPER, FULL_WHISPER, S1_MINI] {
            assert!(install.contains(model.sha256), "{} checksum differs from install.sh", model.file_name);
        }
        assert!(
            install.contains(WHISPER_REPO.trim_end_matches('/')) && install.contains(S1_REPO.trim_end_matches('/'))
        );
        let swift = include_str!("../../../app/Sources/VoiceToText/ModelManager.swift");
        for model in WHISPER.iter().chain([&S1_MINI]) {
            assert!(swift.contains(model.sha256), "{} checksum differs from ModelManager.swift", model.file_name);
        }
        assert_eq!(whisper_named("ggml-base.en.bin"), Some(FAST_WHISPER));
    }
}
