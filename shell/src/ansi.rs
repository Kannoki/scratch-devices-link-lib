//! Raw SGR escape sequences mirroring the npm `ansi-string` constants used in
//! the original JS streaming logs. These are streamed verbatim to the client in
//! `uploadStdout` notifications.

pub const CLEAR: &str = "\u{1b}[0m";
pub const RED: &str = "\u{1b}[31m";
pub const GREEN_DARK: &str = "\u{1b}[32m";
pub const YELLOW_DARK: &str = "\u{1b}[33m";

/// Remove ANSI escape sequences from `input`.
///
/// The daemon colours its streaming log with SGR sequences, but the same text
/// is reused for user-facing error messages (JSON-RPC `uploadError`
/// notifications, Windows toasts, and the error report sent to the Google
/// Sheet). Those consumers render the text verbatim, so the raw escape bytes
/// surface as a stray `[31m` prefix.
///
/// The scanner handles CSI sequences (`ESC [ ... final-byte`), which covers
/// every SGR colour code used here, plus OSC sequences (`ESC ] ... BEL` or
/// `ESC ] ... ESC \`) so a hyperlink or title escape cannot leak either.
/// A hand-rolled scanner is used deliberately to avoid adding a regex
/// dependency to the shell crate.
pub fn strip_ansi(input: &str) -> String {
    if !input.contains('\u{1b}') {
        return input.to_string();
    }
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != 0x1b {
            // Copy the whole UTF-8 sequence starting at `i` verbatim.
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i] & 0xc0) == 0x80 {
                i += 1;
            }
            out.push_str(&input[start..i]);
            continue;
        }
        // ESC - decide which sequence family follows.
        match bytes.get(i + 1) {
            None => {
                // Lone ESC at end of input: drop it.
                i += 1;
            }
            Some(b'[') => {
                // CSI: ESC [ parameter bytes intermediate bytes final byte.
                let mut j = i + 2;
                while j < bytes.len() {
                    let byte = bytes[j];
                    // Final bytes are in the range 0x40..=0x7e.
                    if (0x40..=0x7e).contains(&byte) {
                        j += 1;
                        break;
                    }
                    j += 1;
                }
                i = j.min(bytes.len());
            }
            Some(b']') => {
                // OSC: terminated by BEL or ST (ESC \).
                let mut j = i + 2;
                while j < bytes.len() {
                    if bytes[j] == 0x07 {
                        j += 1;
                        break;
                    }
                    if bytes[j] == 0x1b && bytes.get(j + 1) == Some(&b'\\') {
                        j += 2;
                        break;
                    }
                    j += 1;
                }
                i = j.min(bytes.len());
            }
            Some(_) => {
                // Two-character escape (e.g. ESC c) or a charset selector.
                i += 2;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_sgr_colour_codes() {
        assert_eq!(strip_ansi("\u{1b}[31mUnknown error"), "Unknown error");
        assert_eq!(
            strip_ansi("\u{1b}[31mBuild failed\u{1b}[0m"),
            "Build failed"
        );
        assert_eq!(
            strip_ansi("\u{1b}[1;31mred\u{1b}[0m plain"),
            "red plain"
        );
    }

    #[test]
    fn leaves_plain_text_untouched() {
        assert_eq!(strip_ansi("Build failed"), "Build failed");
        assert_eq!(strip_ansi(""), "");
        assert_eq!(strip_ansi("a [31m b"), "a [31m b");
    }

    #[test]
    fn strips_osc_and_two_char_escapes() {
        assert_eq!(strip_ansi("\u{1b}]8;;http://x\u{7}link\u{1b}]8;;\u{7}"), "link");
        assert_eq!(strip_ansi("\u{1b}creset"), "reset");
    }

    #[test]
    fn preserves_multibyte_text() {
        assert_eq!(strip_ansi("\u{1b}[31mLỗi nạp\u{1b}[0m"), "Lỗi nạp");
    }

    #[test]
    fn handles_truncated_escape_at_end_of_input() {
        assert_eq!(strip_ansi("text\u{1b}"), "text");
        assert_eq!(strip_ansi("text\u{1b}["), "text");
        assert_eq!(strip_ansi("text\u{1b}[31"), "text");
    }
}
