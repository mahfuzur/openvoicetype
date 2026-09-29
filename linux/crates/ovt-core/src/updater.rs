//! The daily release check (follows `Updater.swift`): one anonymous request to GitHub's public API, and a notice when a
//! newer version exists. Fetching is the app's job; this is the version logic.

pub const REPOSITORY: &str = "mahfuzur/openvoicetype";

pub fn latest_release_api() -> String {
    format!("https://api.github.com/repos/{REPOSITORY}/releases/latest")
}

/// "v0.7.0" → "0.7.0".
pub fn version_from_tag(tag: &str) -> &str {
    tag.strip_prefix('v').unwrap_or(tag)
}

/// Compares dotted version numbers ("0.10.0" is newer than "0.9.1").
pub fn is_newer(candidate: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.split('.')
            .map(|part| part.chars().take_while(char::is_ascii_digit).collect::<String>().parse().unwrap_or(0))
            .collect()
    };
    let (a, b) = (parse(candidate), parse(current));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("0.10.0", "0.9.1"));
        assert!(is_newer("1.0", "0.99.99"));
        assert!(!is_newer("0.5.0", "0.5"));
        assert!(!is_newer("0.4.9", "0.5.0"));
        assert!(is_newer("0.6.0-beta1", "0.5.9"));
        assert_eq!(version_from_tag("v0.7.0"), "0.7.0");
    }
}
