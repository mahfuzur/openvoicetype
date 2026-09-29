//! Cleaned text with lists → simple HTML, offered next to the plain text on the clipboard (`text/html`), so rich-text
//! apps (LibreOffice, Thunderbird, Google Docs, Slack) paste real bulleted and numbered lists (follows `RichText.swift`).

use regex::Regex;
use std::sync::OnceLock;

fn patterns() -> &'static (Regex, Regex) {
    static PATTERNS: OnceLock<(Regex, Regex)> = OnceLock::new();
    PATTERNS.get_or_init(|| (Regex::new(r"^\s*[-*•]\s+(.*)$").unwrap(), Regex::new(r"^\s*\d+[.)]\s+(.*)$").unwrap()))
}

fn item<'a>(regex: &Regex, line: &'a str) -> Option<&'a str> {
    regex.captures(line).and_then(|c| c.get(1)).map(|m| m.as_str())
}

pub fn contains_list(text: &str) -> bool {
    let (bullet, numbered) = patterns();
    text.split('\n').any(|line| bullet.is_match(line) || numbered.is_match(line))
}

pub fn html(text: &str) -> String {
    let (bullet, numbered) = patterns();
    let mut html = String::new();
    let mut open_list: Option<&str> = None;
    let mut paragraph: Vec<&str> = Vec::new();

    fn flush(html: &mut String, paragraph: &mut Vec<&str>) {
        if !paragraph.is_empty() {
            html.push_str("<p>");
            html.push_str(&paragraph.iter().map(|l| escape(l)).collect::<Vec<_>>().join("<br>"));
            html.push_str("</p>");
            paragraph.clear();
        }
    }
    fn close(html: &mut String, open_list: &mut Option<&str>) {
        if let Some(tag) = open_list.take() {
            html.push_str(&format!("</{tag}>"));
        }
    }

    for line in text.split('\n') {
        if line.trim().is_empty() {
            flush(&mut html, &mut paragraph);
            close(&mut html, &mut open_list);
            continue;
        }
        let list = item(bullet, line).map(|i| ("ul", i)).or_else(|| item(numbered, line).map(|i| ("ol", i)));
        if let Some((tag, item)) = list {
            flush(&mut html, &mut paragraph);
            if open_list != Some(tag) {
                close(&mut html, &mut open_list);
                html.push_str(&format!("<{tag}>"));
                open_list = Some(tag);
            }
            html.push_str(&format!("<li>{}</li>", escape(item)));
        } else {
            close(&mut html, &mut open_list);
            paragraph.push(line);
        }
    }
    flush(&mut html, &mut paragraph);
    close(&mut html, &mut open_list);
    format!("<meta charset=\"utf-8\"><div style=\"font-family: sans-serif\">{html}</div>")
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_lists() {
        let text = "Groceries:\n- 1 kg bananas\n- milk & eggs\n\n1. First\n2) Second\nDone";
        assert!(contains_list(text));
        assert!(!contains_list("Just a sentence. And - not a list."));
        assert_eq!(
            html(text),
            "<meta charset=\"utf-8\"><div style=\"font-family: sans-serif\"><p>Groceries:</p><ul><li>1 kg bananas</li>\
             <li>milk &amp; eggs</li></ul><ol><li>First</li><li>Second</li></ol><p>Done</p></div>"
        );
    }
}
