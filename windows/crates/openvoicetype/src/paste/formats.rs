//! Clipboard formats (the Windows side of `Paster.swift`'s pasteboard types): which ones a snapshot can copy and put
//! back, the registered names that keep our text out of clipboard history and cloud sync, and the "HTML Format"
//! envelope. Pure, so it's tested on every platform.

pub const CF_UNICODETEXT: u32 = 13;

/// Clipboard managers and the history (Win+V) skip data that carries this format.
pub const EXCLUDE_FROM_MONITORS: &str = "ExcludeClipboardContentFromMonitorProcessing";
/// DWORD 0: not kept in the clipboard history.
pub const IN_HISTORY: &str = "CanIncludeInClipboardHistory";
/// DWORD 0: not synced to other devices.
pub const UPLOAD_TO_CLOUD: &str = "CanUploadToCloudClipboard";
/// What browsers, Word and Outlook read rich text from.
pub const HTML: &str = "HTML Format";

/// The registered formats our clipboard item carries next to the text: name → data.
pub fn privacy_formats() -> [(&'static str, Vec<u8>); 3] {
    [
        (EXCLUDE_FROM_MONITORS, Vec::new()),
        (IN_HISTORY, 0u32.to_le_bytes().to_vec()),
        (UPLOAD_TO_CLOUD, 0u32.to_le_bytes().to_vec()),
    ]
}

/// Can a snapshot copy this format's data as plain bytes? Not the ones whose handle is a GDI object or something else
/// that isn't global memory (bitmaps, palettes, metafiles, owner-display, private and GDI-object ranges). Images
/// survive anyway as CF_DIB/CF_DIBV5, which are memory.
pub fn is_memory_format(format: u32) -> bool {
    const HANDLES: [u32; 8] = [
        2,    // CF_BITMAP
        3,    // CF_METAFILEPICT
        9,    // CF_PALETTE
        14,   // CF_ENHMETAFILE
        0x80, // CF_OWNERDISPLAY
        0x82, // CF_DSPBITMAP
        0x83, // CF_DSPMETAFILEPICT
        0x8E, // CF_DSPENHMETAFILE
    ];
    format != 0 && !HANDLES.contains(&format) && !(0x0200..=0x03FF).contains(&format)
}

/// UTF-16 with the terminating NUL, as CF_UNICODETEXT holds it. Line ends become CRLF, which every Windows app expects.
pub fn unicode_text(text: &str) -> Vec<u8> {
    let crlf = text.replace("\r\n", "\n").replace('\n', "\r\n");
    crlf.encode_utf16().chain(std::iter::once(0)).flat_map(u16::to_le_bytes).collect()
}

/// CF_HTML: the UTF-8 header with byte offsets of the HTML and of the fragment, then the HTML (with its NUL).
pub fn cf_html(fragment: &str) -> Vec<u8> {
    const HEADER: &str = "Version:0.9\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\nStartFragment:0000000000\r\nEndFragment:0000000000\r\n";
    let prefix = "<html><body>\r\n<!--StartFragment-->";
    let suffix = "<!--EndFragment-->\r\n</body></html>";
    let start_html = HEADER.len();
    let start_fragment = start_html + prefix.len();
    let end_fragment = start_fragment + fragment.len();
    let end_html = end_fragment + suffix.len();
    let header = format!(
        "Version:0.9\r\nStartHTML:{start_html:010}\r\nEndHTML:{end_html:010}\r\nStartFragment:{start_fragment:010}\r\nEndFragment:{end_fragment:010}\r\n"
    );
    let mut bytes = format!("{header}{prefix}{fragment}{suffix}").into_bytes();
    bytes.push(0);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_formats() {
        assert!(is_memory_format(CF_UNICODETEXT) && is_memory_format(8) && is_memory_format(17)); // text, DIB, DIBV5
        assert!(is_memory_format(0xC0FF)); // a registered format
        assert!(!is_memory_format(2) && !is_memory_format(14) && !is_memory_format(0x0250) && !is_memory_format(0));
    }

    #[test]
    fn text_is_utf16_with_crlf() {
        assert_eq!(unicode_text("a\nb"), vec![b'a', 0, b'\r', 0, b'\n', 0, b'b', 0, 0, 0]);
        assert_eq!(unicode_text("é"), vec![0xE9, 0, 0, 0]);
    }

    #[test]
    fn html_offsets_point_at_the_fragment() {
        let fragment = "<ul><li>caf\u{e9}</li></ul>";
        let bytes = cf_html(fragment);
        let text = String::from_utf8(bytes[..bytes.len() - 1].to_vec()).unwrap();
        let offset = |key: &str| -> usize {
            let at = text.find(key).unwrap() + key.len();
            text[at..at + 10].parse().unwrap()
        };
        assert_eq!(&text[offset("StartFragment:")..offset("EndFragment:")], fragment);
        assert!(text[offset("StartHTML:")..].starts_with("<html>"));
        assert_eq!(offset("EndHTML:"), text.len());
        assert_eq!(privacy_formats()[1].1, vec![0, 0, 0, 0]);
    }
}
