//! The models the app can download (follows `ModelManager.swift`). URLs are pinned to a Hugging Face commit, so the
//! SHA-256 always matches; `scripts/install.sh` pins the same files. Keep all three in sync.

use std::path::{Path, PathBuf};

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

/// Moves the models from the folders used up to v0.5 (`~/.local/share/whisper` and `s1-mini`) into the app's own
/// folder, like `Migration.moveModels` in the macOS app. The app runs it at startup, before it checks which models are
/// installed. Returns the moved files.
pub fn move_old_models() -> Vec<PathBuf> {
    move_models(&crate::paths::data_home(), &crate::paths::models_dir())
}

/// Only the catalog's files: anything else in the old folders isn't ours. The old and new folders have the same names.
fn move_models(old_root: &Path, new_root: &Path) -> Vec<PathBuf> {
    let mut moved = Vec::new();
    for model in WHISPER.iter().chain([&S1_MINI]) {
        let kind = model.directory().file_name().map(PathBuf::from).unwrap_or_default();
        let old = old_root.join(&kind).join(model.file_name);
        let new = new_root.join(&kind).join(model.file_name);
        // symlink_metadata doesn't follow links: a link to another app's copy moves as the link.
        if old.symlink_metadata().is_err() || new.symlink_metadata().is_ok() {
            continue;
        }
        // A relative link would point somewhere else after the move: link to the file it resolves to instead.
        let relative = std::fs::read_link(&old).is_ok_and(|link| link.is_relative());
        let result = std::fs::create_dir_all(new_root.join(&kind)).and_then(|()| match std::fs::canonicalize(&old) {
            Ok(target) if relative => {
                std::os::unix::fs::symlink(target, &new).and_then(|()| std::fs::remove_file(&old))
            }
            _ => std::fs::rename(&old, &new),
        });
        if result.is_ok() {
            moved.push(new);
        }
    }
    for kind in ["whisper", "s1-mini"] {
        let _ = std::fs::remove_dir(old_root.join(kind)); // only if nothing else is in it
    }
    moved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_matches_install_sh() {
        let install = include_str!("../../../../scripts/install.sh");
        for model in [DEFAULT_WHISPER, FULL_WHISPER, S1_MINI] {
            assert!(install.contains(model.sha256), "{} checksum differs from install.sh", model.file_name);
        }
        assert!(
            install.contains(WHISPER_REPO.trim_end_matches('/')) && install.contains(S1_REPO.trim_end_matches('/'))
        );
        let swift = include_str!("../../../../app/Sources/VoiceToText/ModelManager.swift");
        for model in WHISPER.iter().chain([&S1_MINI]) {
            assert!(swift.contains(model.sha256), "{} checksum differs from ModelManager.swift", model.file_name);
        }
        assert_eq!(whisper_named("ggml-base.en.bin"), Some(FAST_WHISPER));
    }

    #[test]
    fn moves_old_models() {
        let scratch = std::env::temp_dir().join(format!("ovt-models-{}", std::process::id()));
        let (old_root, new_root) = (scratch.join("share"), scratch.join("models"));
        std::fs::create_dir_all(old_root.join("whisper")).unwrap();
        std::fs::create_dir_all(old_root.join("s1-mini")).unwrap();
        std::fs::write(old_root.join("whisper").join(DEFAULT_WHISPER.file_name), "model").unwrap();
        std::fs::write(old_root.join("whisper/ggml-tiny.bin"), "other").unwrap();
        std::fs::write(scratch.join("elsewhere.gguf"), "s1").unwrap();
        std::os::unix::fs::symlink("../../elsewhere.gguf", old_root.join("s1-mini").join(S1_MINI.file_name)).unwrap();

        assert_eq!(move_models(&old_root, &new_root).len(), 2);
        let read = |path: PathBuf| std::fs::read_to_string(path).unwrap();
        assert_eq!(read(new_root.join("whisper").join(DEFAULT_WHISPER.file_name)), "model");
        assert_eq!(read(new_root.join("s1-mini").join(S1_MINI.file_name)), "s1");
        // Someone else's file stays, so its folder does too; the emptied one is removed.
        assert!(old_root.join("whisper/ggml-tiny.bin").exists() && !old_root.join("s1-mini").exists());
        assert!(move_models(&old_root, &new_root).is_empty());
        std::fs::remove_dir_all(scratch).unwrap();
    }
}
