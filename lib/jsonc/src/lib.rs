//! jsonc — strip comments and trailing commas from JSONC, producing valid
//! JSON per RFC 8259.
//!
//! The output is always the same length as the input and preserves line
//! breaks at matching offsets, so downstream parsers report messages and
//! errors at the correct positions.

/// Convert JSONC bytes in `src` to JSON bytes, allocating a new buffer.
pub fn to_json(src: &[u8]) -> Vec<u8> {
    let mut out = src.to_vec();
    let n = to_json_in_place(&mut out);
    // Output length always equals input length, but assert for safety.
    debug_assert_eq!(n, src.len());
    out.truncate(n);
    out
}

/// Convert JSONC bytes in `src` to JSON bytes in `dst`. Returns the number
/// of bytes written, which always equals `src.len()`. `dst.len()` must be at
/// least `src.len()`.
pub fn to_json_into(dst: &mut [u8], src: &[u8]) -> usize {
    assert!(dst.len() >= src.len());
    dst[..src.len()].copy_from_slice(src);
    to_json_in_place(&mut dst[..src.len()])
}

/// Convert JSONC bytes to JSON bytes in place. Returns the number of bytes
/// written, which always equals `buf.len()`.
///
/// Every branch below writes exactly as many bytes as it consumes, so the
/// write cursor never overtakes the read cursor and the rewrite is safe to
/// perform on a single buffer.
pub fn to_json_in_place(buf: &mut [u8]) -> usize {
    let len = buf.len();
    let mut di = 0usize; // destination index
    let mut i = 0usize;

    while i < len {
        let c = buf[i];

        // ── Comment handling ────────────────────────────────────────────
        if c == b'/' && i + 1 < len {
            let next = buf[i + 1];

            if next == b'/' {
                // Line comment: replace with spaces up to (but not including)
                // the newline, which is preserved verbatim.
                buf[di] = b' ';
                buf[di + 1] = b' ';
                di += 2;
                i += 2;
                while i < len {
                    let cc = buf[i];
                    if cc == b'\n' {
                        buf[di] = b'\n';
                        di += 1;
                        break;
                    } else if cc == b'\t' || cc == b'\r' {
                        buf[di] = cc;
                        di += 1;
                    } else {
                        buf[di] = b' ';
                        di += 1;
                    }
                    i += 1;
                }
                i += 1;
                continue;
            }

            if next == b'*' {
                // Block comment: replace with spaces, preserving \n \r \t.
                // If unterminated, restore the leading "/*" so the output is
                // recognizably broken (matching the Go reference's behavior).
                buf[di] = b' ';
                buf[di + 1] = b' ';
                let start_di = di; // remember where "/*" went, for restoration
                di += 2;
                i += 2;
                let mut closed = false;
                while i + 1 < len {
                    let cc = buf[i];
                    if cc == b'*' && buf[i + 1] == b'/' {
                        buf[di] = b' ';
                        buf[di + 1] = b' ';
                        di += 2;
                        i += 1; // consume the '/'
                        closed = true;
                        break;
                    } else if cc == b'\n' || cc == b'\t' || cc == b'\r' {
                        buf[di] = cc;
                        di += 1;
                    } else {
                        buf[di] = b' ';
                        di += 1;
                    }
                    i += 1;
                }
                if !closed {
                    // Pad whatever bytes are left and restore "/*" marker.
                    while i < len {
                        buf[di] = b' ';
                        di += 1;
                        i += 1;
                    }
                    buf[start_di] = b'/';
                    buf[start_di + 1] = b'*';
                }
                i += 1;
                continue;
            }
        }

        // ── Default: copy the byte ──────────────────────────────────────
        buf[di] = c;
        di += 1;

        // ── String literal: copy verbatim through closing quote ─────────
        if c == b'"' {
            i += 1;
            while i < len {
                let sc = buf[i];
                buf[di] = sc;
                di += 1;
                if sc == b'"' {
                    // Closing quote only if the preceding run of backslashes
                    // is even-length (i.e. the quote isn't escaped).
                    let mut j = i;
                    while j > 0 && buf[j - 1] == b'\\' {
                        j -= 1;
                    }
                    if (i - j).is_multiple_of(2) {
                        break;
                    }
                }
                i += 1;
            }
        }
        // ── Trailing-comma removal before } or ] ────────────────────────
        else if c == b'}' || c == b']' {
            // Walk back through whitespace; if we find a comma, blank it.
            let mut j = di as isize - 2;
            while j >= 0 {
                let bj = buf[j as usize];
                if bj <= b' ' {
                    j -= 1;
                    continue;
                }
                if bj == b',' {
                    buf[j as usize] = b' ';
                }
                break;
            }
        }

        i += 1;
    }

    di
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn convert(src: &str) -> String {
        String::from_utf8(to_json(src.as_bytes())).unwrap()
    }

    #[test]
    fn no_comments_passes_through_unchanged() {
        let src = r#"{"a":1,"b":[1,2,3]}"#;
        assert_eq!(convert(src), src);
    }

    #[test]
    fn line_comment_is_blanked_newline_preserved() {
        let src = "{\n  // comment here\n  \"a\": 1\n}";
        // The 17 chars between the two leading newlines ("  // comment here")
        // become 17 spaces; the trailing \n is preserved.
        let want = "{\n                 \n  \"a\": 1\n}";
        let out = convert(src);
        assert_eq!(out, want);
        assert_eq!(out.len(), src.len());
    }

    #[test]
    fn block_comment_is_blanked_line_breaks_preserved() {
        let src = "{/* multi\nline\ncomment */\"a\":1}";
        let out = convert(src);
        assert_eq!(out.len(), src.len());
        // Newlines must be at the same offsets.
        for (s, o) in src.bytes().zip(out.bytes()) {
            if s == b'\n' {
                assert_eq!(o, b'\n');
            }
        }
        // The "/*...*/" region should now be all whitespace; the JSON content
        // should remain intact.
        assert!(out.contains(r#""a":1}"#));
    }

    #[test]
    fn trailing_comma_before_brace_is_removed() {
        assert_eq!(convert(r#"{"a":1,}"#), r#"{"a":1 }"#);
    }

    #[test]
    fn trailing_comma_before_bracket_is_removed() {
        assert_eq!(convert("[1, 2, 3, ]"), "[1, 2, 3  ]");
    }

    #[test]
    fn comment_like_content_inside_string_is_untouched() {
        let src = r#"{"url":"http://x/y","note":"// not a comment"}"#;
        assert_eq!(convert(src), src);
    }

    #[test]
    fn escaped_quote_does_not_end_string() {
        let src = r#"{"a":"he said \"hi\""}"#;
        assert_eq!(convert(src), src);
    }

    #[test]
    fn escaped_backslash_before_quote_does_end_string() {
        // "a":"x\\" — the \\ is an escaped backslash, so the next " closes.
        let src = r#"{"a":"x\\","b":2}"#;
        assert_eq!(convert(src), src);
    }

    #[test]
    fn unterminated_block_comment_keeps_marker() {
        let src = "{/* oops";
        let out = convert(src);
        assert_eq!(out.len(), src.len());
        assert_eq!(out, "{/*     ");
    }

    #[test]
    fn in_place_conversion_produces_same_result_as_allocating() {
        let original = "{\n  // hi\n  \"a\":1,\n}";
        let mut buf = original.as_bytes().to_vec();
        let expected = to_json(original.as_bytes());

        let n = to_json_in_place(&mut buf);
        assert_eq!(n, original.len());
        assert_eq!(&buf[..n], &expected[..]);
    }

    #[test]
    fn to_json_into_writes_into_larger_buffer() {
        let src = b"{\"a\":1,}";
        let mut dst = vec![0u8; src.len() + 4];
        let n = to_json_into(&mut dst, src);
        assert_eq!(n, src.len());
        assert_eq!(&dst[..n], b"{\"a\":1 }");
    }

    #[test]
    fn output_length_equals_input_length_for_mixed_content() {
        let src = concat!(
            "{\n",
            "  // a line comment\n",
            "  \"name\": \"value\", /* trailing block */\n",
            "  \"list\": [1, 2, 3,],\n",
            "  /* another\n     comment */\n",
            "  \"x\": \"// inside string\",\n",
            "}"
        );
        assert_eq!(convert(src).len(), src.len());
    }
}
