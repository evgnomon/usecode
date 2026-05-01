//! jsonc.zig — strip comments and trailing commas from JSONC, producing
//! valid JSON per RFC 8259.
//!
//! Port of the Go reference implementation. The output is always the same
//! length as the input and preserves line breaks at matching offsets, so
//! downstream parsers report messages and errors at the correct positions.
//!
//! Build:   zig build-exe jsonc.zig -O ReleaseFast
//! Use:     ./jsonc input.jsonc          (writes to stdout)
//!          ./jsonc < input.jsonc        (reads from stdin)
//!          ./jsonc input.jsonc -o out.json
//! Tests:   zig test jsonc.zig
//!

const std = @import("std");

const cwd = std.Io.Dir.cwd();

/// Convert JSONC bytes in `src` to JSON bytes, allocating a new buffer.
/// Caller owns the returned slice.
pub fn toJSON(allocator: std.mem.Allocator, src: []const u8) ![]u8 {
    const dst = try allocator.alloc(u8, src.len);
    errdefer allocator.free(dst);
    const n = toJSONInto(dst, src);
    // Output length always equals input length, but assert for safety.
    std.debug.assert(n == src.len);
    return dst;
}

/// Convert JSONC bytes in `src` to JSON bytes in `buf` (which may alias
/// `src`). Returns the number of bytes written, which always equals
/// `src.len`. `buf.len` must be at least `src.len`.
pub fn toJSONInto(buf: []u8, src: []const u8) usize {
    std.debug.assert(buf.len >= src.len);

    var di: usize = 0; // destination index
    var i: usize = 0;

    while (i < src.len) : (i += 1) {
        const c = src[i];

        // ── Comment handling ────────────────────────────────────────────
        if (c == '/' and i + 1 < src.len) {
            const next = src[i + 1];

            if (next == '/') {
                // Line comment: replace with spaces up to (but not including)
                // the newline, which is preserved verbatim.
                buf[di] = ' ';
                buf[di + 1] = ' ';
                di += 2;
                i += 2;
                while (i < src.len) : (i += 1) {
                    const cc = src[i];
                    if (cc == '\n') {
                        buf[di] = '\n';
                        di += 1;
                        break;
                    } else if (cc == '\t' or cc == '\r') {
                        buf[di] = cc;
                        di += 1;
                    } else {
                        buf[di] = ' ';
                        di += 1;
                    }
                }
                continue;
            }

            if (next == '*') {
                // Block comment: replace with spaces, preserving \n \r \t.
                // If unterminated, restore the leading "/*" so the output is
                // recognizably broken (matching the Go reference's behavior).
                buf[di] = ' ';
                buf[di + 1] = ' ';
                const start_di = di; // remember where "/*" went, for restoration
                di += 2;
                i += 2;
                var closed = false;
                while (i + 1 < src.len) : (i += 1) {
                    const cc = src[i];
                    if (cc == '*' and src[i + 1] == '/') {
                        buf[di] = ' ';
                        buf[di + 1] = ' ';
                        di += 2;
                        i += 1; // consume the '/'
                        closed = true;
                        break;
                    } else if (cc == '\n' or cc == '\t' or cc == '\r') {
                        buf[di] = cc;
                        di += 1;
                    } else {
                        buf[di] = ' ';
                        di += 1;
                    }
                }
                if (!closed) {
                    // Pad whatever bytes are left and restore "/*" marker.
                    while (i < src.len) : (i += 1) {
                        buf[di] = ' ';
                        di += 1;
                    }
                    buf[start_di] = '/';
                    buf[start_di + 1] = '*';
                }
                continue;
            }
        }

        // ── Default: copy the byte ──────────────────────────────────────
        buf[di] = c;
        di += 1;

        // ── String literal: copy verbatim through closing quote ─────────
        if (c == '"') {
            i += 1;
            while (i < src.len) : (i += 1) {
                const sc = src[i];
                buf[di] = sc;
                di += 1;
                if (sc == '"') {
                    // Closing quote only if the preceding run of backslashes
                    // is even-length (i.e. the quote isn't escaped).
                    var j: usize = i;
                    while (j > 0 and src[j - 1] == '\\') : (j -= 1) {}
                    const backslashes = i - j;
                    if (backslashes % 2 == 0) break;
                }
            }
        }
        // ── Trailing-comma removal before } or ] ────────────────────────
        else if (c == '}' or c == ']') {
            // Walk back through whitespace; if we find a comma, blank it.
            var j: isize = @as(isize, @intCast(di)) - 2;
            while (j >= 0) : (j -= 1) {
                const bj = buf[@intCast(j)];
                if (bj <= ' ') continue;
                if (bj == ',') buf[@intCast(j)] = ' ';
                break;
            }
        }
    }

    return di;
}

// ─── CLI ────────────────────────────────────────────────────────────────

pub fn main(init: std.process.Init) !void {
    const allocator = init.gpa;
    const io = init.io;

    // Read Input
    var stdin = std.Io.File.stdin();
    var sr = stdin.reader(io, &.{});
    var src = try sr.interface.allocRemaining(allocator, .unlimited);
    defer allocator.free(src);

    // Convert in place — saves an allocation.
    const out_len = toJSONInto(src, src);
    const out = src[0..out_len];
    var stdout = std.Io.File.stdout();
    var sw = stdout.writer(io, &.{});
    sw.interface.writeAll(out) catch |err| {
        std.debug.print("error writing output: {t}\n", .{err});
        std.process.exit(1);
    };
}

// ─── Tests ──────────────────────────────────────────────────────────────

const testing = std.testing;

fn convert(allocator: std.mem.Allocator, src: []const u8) ![]u8 {
    return toJSON(allocator, src);
}

test "no comments passes through unchanged" {
    const src = "{\"a\":1,\"b\":[1,2,3]}";
    const out = try convert(testing.allocator, src);
    defer testing.allocator.free(out);
    try testing.expectEqualStrings(src, out);
}

test "line comment is blanked, newline preserved" {
    const src = "{\n  // comment here\n  \"a\": 1\n}";
    // The 17 chars between the two leading newlines ("  // comment here")
    // become 17 spaces; the trailing \n is preserved.
    const want = "{\n                 \n  \"a\": 1\n}";
    const out = try convert(testing.allocator, src);
    defer testing.allocator.free(out);
    try testing.expectEqualStrings(want, out);
    try testing.expectEqual(src.len, out.len);
}

test "block comment is blanked, line breaks preserved" {
    const src = "{/* multi\nline\ncomment */\"a\":1}";
    const out = try convert(testing.allocator, src);
    defer testing.allocator.free(out);
    try testing.expectEqual(src.len, out.len);
    // Newlines must be at the same offsets.
    for (src, out, 0..) |s, o, k| {
        if (s == '\n') try testing.expectEqual(@as(u8, '\n'), o) else _ = k;
    }
    // The "/*...*/ " region should now be all whitespace; the JSON content
    // should remain intact.
    try testing.expect(std.mem.indexOf(u8, out, "\"a\":1}") != null);
}

test "trailing comma before brace is removed" {
    const src = "{\"a\":1,}";
    const want = "{\"a\":1 }";
    const out = try convert(testing.allocator, src);
    defer testing.allocator.free(out);
    try testing.expectEqualStrings(want, out);
}

test "trailing comma before bracket is removed" {
    const src = "[1, 2, 3, ]";
    const want = "[1, 2, 3  ]";
    const out = try convert(testing.allocator, src);
    defer testing.allocator.free(out);
    try testing.expectEqualStrings(want, out);
}

test "comment-like content inside string is untouched" {
    const src = "{\"url\":\"http://x/y\",\"note\":\"// not a comment\"}";
    const out = try convert(testing.allocator, src);
    defer testing.allocator.free(out);
    try testing.expectEqualStrings(src, out);
}

test "escaped quote does not end string" {
    const src = "{\"a\":\"he said \\\"hi\\\"\"}";
    const out = try convert(testing.allocator, src);
    defer testing.allocator.free(out);
    try testing.expectEqualStrings(src, out);
}

test "escaped backslash before quote does end string" {
    // "a":"x\\" — the \\ is an escaped backslash, so the next " closes.
    const src = "{\"a\":\"x\\\\\",\"b\":2}";
    const out = try convert(testing.allocator, src);
    defer testing.allocator.free(out);
    try testing.expectEqualStrings(src, out);
}

test "in-place conversion produces same result as allocating" {
    const original = "{\n  // hi\n  \"a\":1,\n}";
    var buf = try testing.allocator.dupe(u8, original);
    defer testing.allocator.free(buf);

    const expected = try convert(testing.allocator, original);
    defer testing.allocator.free(expected);

    const n = toJSONInto(buf, buf);
    try testing.expectEqual(original.len, n);
    try testing.expectEqualStrings(expected, buf[0..n]);
}

test "output length equals input length for mixed content" {
    const src =
        "{\n" ++
        "  // a line comment\n" ++
        "  \"name\": \"value\", /* trailing block */\n" ++
        "  \"list\": [1, 2, 3,],\n" ++
        "  /* another\n     comment */\n" ++
        "  \"x\": \"// inside string\",\n" ++
        "}";
    const out = try convert(testing.allocator, src);
    defer testing.allocator.free(out);
    try testing.expectEqual(src.len, out.len);
}
