// ============================================================================
// ZIG LANGUAGE FEATURE SHOWCASE
// A comprehensive single-file tour of every major Zig feature
// ============================================================================

const std = @import("std");
const builtin = @import("builtin");
const mem = std.mem;
const math = std.math;
const testing = std.testing;
const Allocator = std.mem.Allocator;

const channel = @import("channel.zig");

// ============================================================================
// 1. PRIMITIVE TYPES & LITERALS
// ============================================================================

const primitive_examples = struct {
    // Integer types (arbitrary bit-width)
    const a_u8: u8 = 255;
    const a_i8: i8 = -128;
    const a_u16: u16 = 65535;
    const a_i32: i32 = -2_147_483_648; // underscores for readability
    const a_u64: u64 = 18_446_744_073_709_551_615;
    const a_i128: i128 = 0;
    const a_usize: usize = 0;
    const a_isize: isize = -1;

    // Arbitrary bit-width integers
    const a_u3: u3 = 7; // 3-bit unsigned
    const a_i5: i5 = -16; // 5-bit signed

    // Floating point
    const a_f16: f16 = 1.5;
    const a_f32: f32 = 3.14;
    const a_f64: f64 = 2.718281828459045;
    const a_f128: f128 = 0.0;

    // Boolean
    const a_bool: bool = true;

    // Comptime types
    const a_comptime_int: comptime_int = 100_000_000_000_000_000_000;
    const a_comptime_float: comptime_float = 3.14159265358979323846;

    // Type itself is a type
    const a_type: type = u32;

    // Void, noreturn, undefined
    const a_void: void = {};

    // Integer literal bases
    const decimal = 98_222;
    const hex = 0xff;
    const octal = 0o77;
    const binary = 0b1111_0000;

    // Character literals
    const newline: u8 = '\n';
    const tab: u8 = '\t';
    const letter_a: u8 = 'A';
    const unicode_char: u21 = '⚡';

    // Null & undefined
    const opt_null: ?u32 = null;
    const sentinel: [*:0]const u8 = "hello";
};

// ============================================================================
// 2. STRING LITERALS & MULTILINE STRINGS
// ============================================================================

const string_examples = struct {
    // String literals are *const [N:0]u8 coerced to []const u8
    const hello: []const u8 = "Hello, World!";

    // Escape sequences
    const escapes = "tab:\there\nnewline\x41\u{1F600}";

    // Multiline string literal (each line is trimmed of leading \\)
    const multiline =
        \\This is a multiline
        \\string literal in Zig.
        \\No escape processing here.
    ;

    fn demo() void {
        // String concatenation at comptime
        const combined = "Hello" ++ ", " ++ "World!";
        _ = combined;

        // Repetition
        const repeated = "ha" ** 3; // "hahaha"
        _ = repeated;
    }
};

// ============================================================================
// 3. ARRAYS & SLICES
// ============================================================================

const array_examples = struct {
    // Fixed-size arrays
    const arr: [5]u32 = .{ 1, 2, 3, 4, 5 };

    // Sentinel-terminated array
    const sentinel_arr: [3:0]u8 = .{ 'a', 'b', 'c' };

    // Array initialization with default
    const zeroed: [10]u8 = .{0} ** 10;

    // Init with index
    const indexed = blk: {
        var a: [10]u32 = undefined;
        for (&a, 0..) |*elem, i| {
            elem.* = @intCast(i * 2);
        }
        break :blk a;
    };

    fn demo() void {
        // Slices
        const full_slice: []const u32 = &arr;
        _ = full_slice;

        const sub_slice = arr[1..3]; // [2, 3]
        _ = sub_slice;

        // Open-ended slice
        const from_2 = arr[2..];
        _ = from_2;

        // Array concatenation (comptime)
        const a1 = [_]u32{ 1, 2 };
        const a2 = [_]u32{ 3, 4 };
        const concat = a1 ++ a2;
        _ = concat;

        // Array multiplication (comptime)
        const repeated = [_]u8{0xAB} ** 4;
        _ = repeated;
    }
};

// ============================================================================
// 4. STRUCTS
// ============================================================================

// Regular struct
const Point = struct {
    x: f64,
    y: f64,
    z: f64 = 0.0, // default value

    const Self = @This();

    // Method
    fn distance(self: Self, other: Self) f64 {
        const dx = self.x - other.x;
        const dy = self.y - other.y;
        const dz = self.z - other.z;
        return @sqrt(dx * dx + dy * dy + dz * dz);
    }

    // "Static" function (no self)
    fn origin() Self {
        return .{ .x = 0, .y = 0, .z = 0 };
    }

    // Mutable self
    fn translate(self: *Self, dx: f64, dy: f64) void {
        self.x += dx;
        self.y += dy;
    }

    // Format for std.fmt
    pub fn format(
        self: Self,
        comptime fmt: []const u8,
        options: std.fmt.FormatOptions,
        writer: anytype,
    ) !void {
        _ = fmt;
        _ = options;
        try writer.print("Point({d:.2}, {d:.2}, {d:.2})", .{ self.x, self.y, self.z });
    }
};

// Packed struct (exact memory layout, no padding)
const PackedFlags = packed struct {
    flag_a: bool,
    flag_b: bool,
    flag_c: bool,
    _padding: u5 = 0,
};

// Extern struct (C-compatible layout)
const CPoint = extern struct {
    x: c_int,
    y: c_int,
};

// Tuple (anonymous struct with numbered fields)
fn tuple_example() void {
    const tuple: struct { u32, []const u8, f64 } = .{ 42, "hello", 3.14 };
    _ = tuple[0]; // 42
    _ = tuple[1]; // "hello"
}

// ============================================================================
// 5. ENUMS
// ============================================================================

const Color = enum(u8) {
    red = 0,
    green = 1,
    blue = 2,
    _, // non-exhaustive: allows other values

    fn isWarm(self: Color) bool {
        return self == .red;
    }
};

const Direction = enum {
    north,
    south,
    east,
    west,

    fn opposite(self: Direction) Direction {
        return switch (self) {
            .north => .south,
            .south => .north,
            .east => .west,
            .west => .east,
        };
    }
};

// ============================================================================
// 6. UNIONS (Tagged and Untagged)
// ============================================================================

// Tagged union (algebraic data type / sum type)
const Token = union(enum) {
    number: f64,
    string: []const u8,
    keyword: Keyword,
    eof,

    const Keyword = enum { @"if", @"else", @"while", @"return" };

    fn isEof(self: Token) bool {
        return self == .eof;
    }
};

// Extern union (C-compatible)
const CValue = extern union {
    int_val: c_int,
    float_val: f32,
};

fn union_demo() void {
    const tok = Token{ .number = 42.0 };
    switch (tok) {
        .number => |n| {
            _ = n;
        },
        .string => |s| {
            _ = s;
        },
        .keyword => |k| {
            _ = k;
        },
        .eof => {},
    }
}

// ============================================================================
// 7. OPTIONALS
// ============================================================================

fn optional_demo() void {
    var maybe: ?u32 = 42;

    // Unwrap with orelse
    const val = maybe orelse 0;
    _ = val;

    // if-unwrap
    if (maybe) |value| {
        _ = value;
    }

    // .? operator (unwrap or unreachable)
    const definite = maybe.?;
    _ = definite;

    // Optional pointer
    var x: u32 = 5;
    const ptr: ?*u32 = &x;
    if (ptr) |p| {
        p.* = 10;
    }

    // while with optionals
    maybe = null;
    var iter = IteratorExample{ .index = 0 };
    while (iter.next()) |item| {
        _ = item;
    }
}

const IteratorExample = struct {
    index: usize,

    fn next(self: *IteratorExample) ?u32 {
        if (self.index >= 5) return null;
        self.index += 1;
        return @intCast(self.index);
    }
};

// ============================================================================
// 8. ERROR HANDLING
// ============================================================================

// Error sets
const FileError = error{
    NotFound,
    AccessDenied,
    DiskFull,
};

const NetworkError = error{
    Timeout,
    ConnectionRefused,
};

// Merged error sets
const AppError = FileError || NetworkError;

// Error union return type
fn divide(a: f64, b: f64) error{DivisionByZero}!f64 {
    if (b == 0.0) return error.DivisionByZero;
    return a / b;
}

fn error_handling_demo() void {
    // try: propagate errors
    const result = try_wrapper() catch 0.0;
    _ = result;

    // catch with payload
    const r2 = divide(10, 0) catch |err| blk: {
        _ = err;
        break :blk -1.0;
    };
    _ = r2;

    // errdefer
    _ = errdefer_example() catch {};
}

fn try_wrapper() !f64 {
    // try is sugar for `catch |err| return err`
    const result = try divide(10, 3);
    return result * 2;
}

fn errdefer_example() !void {
    var resource: u32 = 42;
    // errdefer runs only when function returns an error
    errdefer {
        resource = 0; // cleanup on error
    }
    // defer always runs
    defer {
        std.debug.print("Resource value at end: {d}\n", .{resource});
    }
}

// ============================================================================
// 9. CONTROL FLOW
// ============================================================================

fn control_flow_demo() void {
    // --- if / else if / else ---
    const x: i32 = 5;
    if (x > 0) {
        // positive
    } else if (x < 0) {
        // negative
    } else {
        // zero
    }

    // if as expression
    const abs_x: i32 = if (x < 0) -x else x;
    _ = abs_x;

    // --- while ---
    var i: u32 = 0;
    while (i < 10) : (i += 1) {
        if (i == 5) continue;
        if (i == 8) break;
    }

    // while as expression with else
    const found = while_find(5);
    _ = found;

    // --- for ---
    const items = [_]u32{ 10, 20, 30, 40, 50 };
    for (items, 0..) |item, idx| {
        _ = item;
        _ = idx;
    }

    // Multi-object for
    const a = [_]u8{ 1, 2, 3 };
    const b = [_]u8{ 10, 20, 30 };
    for (a, b) |x_val, y_val| {
        _ = x_val + y_val;
    }

    // --- switch ---
    const color: Color = .red;
    switch (color) {
        .red => {},
        .green, .blue => {},
        _ => {}, // non-exhaustive catch-all
    }

    // switch as expression with ranges
    const grade: u8 = 85;
    const letter: []const u8 = switch (grade) {
        0...59 => "F",
        60...69 => "D",
        70...79 => "C",
        80...89 => "B",
        90...100 => "A",
        else => "?",
    };
    _ = letter;
}

fn while_find(target: u32) ?u32 {
    var i: u32 = 0;
    return while (i < 100) : (i += 1) {
        if (i == target) break i;
    } else null;
}

// ============================================================================
// 10. LABELED BLOCKS & LOOPS
// ============================================================================

fn labeled_demo() !void {
    // Labeled block as expression
    const result = blk: {
        const a: u32 = 10;
        const b: u32 = 20;
        break :blk a + b;
    };
    _ = result;

    // Labeled loops
    outer: for (0..10) |i| {
        for (0..10) |j| {
            if (i + j > 12) break :outer;
            std.debug.print("i: {d}, j: {d}\n", .{ i, j });
        }
    }
}

// ============================================================================
// 11. FUNCTIONS & CALLING CONVENTIONS
// ============================================================================

// Basic function
fn add(a: u32, b: u32) u32 {
    return a + b;
}

// Inline function
inline fn fast_double(x: u32) u32 {
    return x << 1;
}

// Noinline function
noinline fn slow_path(x: u32) u32 {
    return x * x;
}

// Export for C ABI
export fn exported_function(x: c_int) c_int {
    return x + 1;
}

// Extern function declaration (C interop)
extern "c" fn abs(x: c_int) c_int;

// Variadic function (only for extern C compat)
// extern fn printf(fmt: [*:0]const u8, ...) c_int;

// Function returning function (closures are not in Zig, but fn pointers work)
fn get_operation(comptime op: u8) fn (u32, u32) u32 {
    return switch (op) {
        '+' => struct {
            fn f(a: u32, b: u32) u32 {
                return a + b;
            }
        }.f,
        '*' => struct {
            fn f(a: u32, b: u32) u32 {
                return a * b;
            }
        }.f,
        else => unreachable,
    };
}

// ============================================================================
// 12. POINTERS
// ============================================================================

fn pointer_demo() void {
    // Single-item pointer
    var x: u32 = 42;
    const ptr: *u32 = &x;
    ptr.* = 100;

    // Const pointer
    const cptr: *const u32 = &x;
    _ = cptr.*;

    // Many-item pointer (unknown length)
    const arr = [_]u32{ 1, 2, 3, 4, 5 };
    const many: [*]const u32 = &arr;
    _ = many[2];

    // Sentinel-terminated pointer
    const str: [*:0]const u8 = "hello";
    _ = str;

    // Pointer arithmetic (only on [*] pointers)
    const next = many + 1;
    _ = next[0]; // 2

    // Volatile pointer (for MMIO)
    var vol: u32 = 0;
    const vptr: *volatile u32 = &vol;
    vptr.* = 1;
    _ = vptr.*;

    // Align cast
    const aligned: *align(16) u32 = @alignCast(@as(*u32, &x));
    _ = aligned;

    // Optional pointer (null-pointer optimization: same size as regular pointer)
    var opt_ptr: ?*u32 = &x;
    opt_ptr = null;
}

// ============================================================================
// 13. GENERICS (comptime parameters)
// ============================================================================

// Generic data structure
fn Stack(comptime T: type) type {
    return struct {
        items: [256]T = undefined,
        count: usize = 0,

        const Self = @This();

        fn push(self: *Self, value: T) !void {
            if (self.count >= 256) return error.StackOverflow;
            self.items[self.count] = value;
            self.count += 1;
        }

        fn pop(self: *Self) ?T {
            if (self.count == 0) return null;
            self.count -= 1;
            return self.items[self.count];
        }

        fn peek(self: *const Self) ?T {
            if (self.count == 0) return null;
            return self.items[self.count - 1];
        }
    };
}

// Generic function
fn max_generic(comptime T: type, a: T, b: T) T {
    return if (a > b) a else b;
}

// Generic with trait-like constraint
fn stringify(value: anytype) []const u8 {
    const T = @TypeOf(value);
    if (@hasDecl(T, "toString")) {
        return value.toString();
    }
    return "unknown";
}

// ============================================================================
// 14. COMPTIME (Compile-Time Evaluation)
// ============================================================================

// Comptime variable
const comptime_result = blk: {
    var sum: u64 = 0;
    for (1..101) |i| {
        sum += i;
    }
    break :blk sum; // 5050
};

// Compile-time type reflection & generation
fn Matrix(comptime T: type, comptime rows: usize, comptime cols: usize) type {
    return struct {
        data: [rows][cols]T,

        const Self = @This();
        const Rows = rows;
        const Cols = cols;

        fn zero() Self {
            return .{ .data = .{.{0} ** cols} ** rows };
        }

        fn identity() Self {
            comptime {
                if (rows != cols) @compileError("Identity only for square matrices");
            }
            var result = zero();
            inline for (0..rows) |i| {
                result.data[i][i] = 1;
            }
            return result;
        }
    };
}

// Compile-time string processing
fn comptimeUpperCase(comptime input: []const u8) *const [input.len]u8 {
    return comptime blk: {
        var result: [input.len]u8 = undefined;
        for (input, 0..) |c, i| {
            result[i] = if (c >= 'a' and c <= 'z') c - 32 else c;
        }
        const final = result;
        break :blk &final;
    };
}

// Compile-time fibonacci
fn comptimeFib(comptime n: u32) u64 {
    if (n <= 1) return n;
    return comptimeFib(n - 1) + comptimeFib(n - 2);
}

// comptime assert
comptime {
    if (comptimeFib(10) != 55) @compileError("Fibonacci is broken!");
    if (!mem.eql(u8, comptimeUpperCase("hello"), "HELLO")) @compileError("UpperCase broken!");
}

// ============================================================================
// 15. INLINE FOR / INLINE WHILE / INLINE SWITCH
// ============================================================================

fn inline_demo() void {
    // inline for — unrolled at compile time
    const fields = .{ "name", "age", "score" };
    inline for (fields) |field| {
        _ = field;
    }

    // inline switch — all branches exist at compile time
    const val: u8 = 3;
    inline for (0..8) |bit| {
        if (val & (1 << bit) != 0) {
            // bit is comptime-known here
        }
    }
}

// ============================================================================
// 16. BUILTINS (@functions)
// ============================================================================

fn builtin_showcase() void {
    // Type conversions
    const x: u32 = 42;
    const y: u64 = @intCast(x);
    _ = y;
    const z: f64 = @floatFromInt(x);
    // _ = z;
    const w: u32 = @intFromFloat(z);
    _ = w;

    // Bitwise
    const bits: u8 = 0b1010_0101;
    const reversed = @bitReverse(bits);
    _ = reversed;
    const leading = @clz(bits);
    _ = leading;
    const trailing = @ctz(bits);
    _ = trailing;
    const popcnt = @popCount(bits);
    _ = popcnt;

    // Overflow arithmetic
    const ov = @addWithOverflow(@as(u8, 200), @as(u8, 100));
    const result = ov[0]; // wrapped result
    const overflowed = ov[1]; // 1 if overflow, 0 if not
    _ = result;
    _ = overflowed;

    // Size and alignment
    const size = @sizeOf(Point);
    _ = size;
    const alignment = @alignOf(Point);
    _ = alignment;

    // Type info (powerful reflection)
    const info = @typeInfo(Color);
    _ = info;

    // @typeName
    const name = @typeName(Point);
    _ = name; // "Point" (or full path)

    // @as - explicit type coercion
    const explicit: u16 = @as(u16, 256);
    _ = explicit;

    // @ptrFromInt / @intFromPtr
    const addr: usize = 0xDEAD_BEEF;
    const fake_ptr: *const u8 = @ptrFromInt(addr);
    const back: usize = @intFromPtr(fake_ptr);
    _ = back;

    // @memset / @memcpy
    var buf: [100]u8 = undefined;
    @memset(&buf, 0);

    // @min / @max
    const minimum = @min(x, 100);
    _ = minimum;

    // @src - source location
    const loc = @src();
    _ = loc.file;
    _ = loc.line;

    // @compileLog - prints at compile time (for debugging)
    // @compileLog("debug value:", comptime_result);
}

// ============================================================================
// 17. TYPE REFLECTION (@typeInfo)
// ============================================================================

fn printStructFields(comptime T: type) void {
    const info = @typeInfo(T);
    switch (info) {
        .@"struct" => |s| {
            inline for (s.fields) |field| {
                _ = field.name;
                _ = field.type;
            }
        },
        else => @compileError("Expected a struct type"),
    }
}

// Runtime type coercion check
fn isOptional(comptime T: type) bool {
    return @typeInfo(T) == .optional;
}

comptime {
    if (!isOptional(?u32)) @compileError("Should be optional");
    if (isOptional(u32)) @compileError("Should not be optional");
}

// ============================================================================
// 18. MEMORY MANAGEMENT & ALLOCATORS
// ============================================================================

fn allocator_demo() !void {
    // Page allocator (asks OS directly)
    const page_alloc = std.heap.page_allocator;

    // General purpose allocator (debug-friendly)
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    // Arena allocator (bulk free)
    var arena = std.heap.ArenaAllocator.init(page_alloc);
    defer arena.deinit();
    const arena_alloc = arena.allocator();

    // Fixed buffer allocator (no heap)
    var buffer: [1024]u8 = undefined;
    var fba = std.heap.FixedBufferAllocator.init(&buffer);
    const fb_alloc = fba.allocator();

    // Allocate single item
    const ptr = try allocator.create(Point);
    defer allocator.destroy(ptr);
    ptr.* = Point{ .x = 1, .y = 2 };

    // Allocate slice
    const slice = try allocator.alloc(u8, 100);
    defer allocator.free(slice);

    // ArrayList (dynamic array)
    var list = std.ArrayList(u32).init(allocator);
    defer list.deinit();
    try list.append(42);
    try list.appendSlice(&.{ 1, 2, 3 });

    // HashMap
    var map = std.StringHashMap(u32).init(allocator);
    defer map.deinit();
    try map.put("hello", 42);
    const lookup = map.get("hello"); // ?u32
    _ = lookup;

    // Use arena (no individual frees needed)
    const arena_data = try arena_alloc.alloc(u8, 512);
    _ = arena_data;

    _ = fb_alloc;
}

// ============================================================================
// 19. ASYNC / SUSPEND-RESUME (stage1 feature, disabled in stage2 as of 0.13)
//     Included here for completeness as a comment.
// ============================================================================

// NOTE: Async frames were a core Zig feature but are currently disabled in the
// self-hosted compiler (stage2). They may return in a future version.
//
// fn asyncExample() void {
//     var frame = async fetchData();
//     // ... do other work ...
//     const result = await frame;
// }

// ============================================================================
// 20. ERROR RETURN TRACES & STACK TRACES
// ============================================================================

fn show_error_trace() !void {
    // Zig automatically provides error return traces in debug mode
    return error.DemoError;
}

// ============================================================================
// 21. SENTINEL-TERMINATED TYPES
// ============================================================================

fn sentinel_demo() void {
    // Null-terminated string (C compat)
    const c_str: [:0]const u8 = "Hello C";
    _ = c_str;

    // Sentinel-terminated array
    const arr: [3:0]u8 = .{ 1, 2, 3 };
    // arr[3] == 0 (the sentinel)
    _ = arr;

    // Sentinel-terminated slice
    const slice: [:0]const u8 = "test";
    _ = slice;
}

// ============================================================================
// 22. VECTORS (SIMD)
// ============================================================================

fn simd_demo() void {
    // @Vector for SIMD operations
    const Vec4f = @Vector(4, f32);

    const a: Vec4f = .{ 1.0, 2.0, 3.0, 4.0 };
    const b: Vec4f = .{ 5.0, 6.0, 7.0, 8.0 };

    // Element-wise operations (compiled to SIMD instructions)
    const sum = a + b;
    const product = a * b;
    const diff = a - b;
    _ = sum;
    _ = product;
    _ = diff;

    // Splat (broadcast scalar)
    const scalar: Vec4f = @splat(2.0);
    const doubled = a * scalar;
    _ = doubled;

    // Reduce
    const total = @reduce(.Add, a); // 10.0
    _ = total;
    const max_val = @reduce(.Max, a); // 4.0
    _ = max_val;

    // Shuffle
    const shuffled = @shuffle(f32, a, b, [4]i32{ 0, 5, 2, 7 });
    _ = shuffled;
}

// ============================================================================
// 23. DEFER & ERRDEFER
// ============================================================================

fn defer_demo(allocator: Allocator) ![]u8 {
    const buf = try allocator.alloc(u8, 100);
    // errdefer frees ONLY if function returns an error
    errdefer allocator.free(buf);

    // defer runs in reverse order when scope exits
    var log_count: u32 = 0;
    defer log_count += 1; // runs second
    defer log_count += 2; // runs first

    // Defers in loops
    for (0..5) |_| {
        defer log_count += 1; // runs each iteration
    }

    return buf;
}

// ============================================================================
// 24. TESTING
// ============================================================================

test "basic addition" {
    const result = add(2, 3);
    try testing.expectEqual(@as(u32, 5), result);
}

test "point distance" {
    const a = Point{ .x = 0, .y = 0 };
    const b = Point{ .x = 3, .y = 4 };
    try testing.expectApproxEqAbs(a.distance(b), 5.0, 0.001);
}

test "generic stack" {
    var stack = Stack(u32){};
    try stack.push(10);
    try stack.push(20);
    try testing.expectEqual(@as(?u32, 20), stack.pop());
    try testing.expectEqual(@as(?u32, 10), stack.pop());
    try testing.expectEqual(@as(?u32, null), stack.pop());
}

test "error handling" {
    const result = divide(10, 2) catch unreachable;
    try testing.expectApproxEqAbs(result, 5.0, 0.001);

    const err_result = divide(10, 0);
    try testing.expectError(error.DivisionByZero, err_result);
}

test "optional unwrap" {
    const val: ?u32 = 42;
    try testing.expectEqual(@as(u32, 42), val.?);

    const empty: ?u32 = null;
    try testing.expect(empty == null);
}

test "slicing" {
    const arr = [_]u32{ 0, 1, 2, 3, 4, 5 };
    const slice = arr[2..5];
    try testing.expectEqualSlices(u32, &.{ 2, 3, 4 }, slice);
}

test "comptime fibonacci" {
    try testing.expectEqual(@as(u64, 55), comptimeFib(10));
    try testing.expectEqual(@as(u64, 6765), comptimeFib(20));
}

test "matrix identity" {
    const Mat3 = Matrix(f64, 3, 3);
    const eye = Mat3.identity();
    try testing.expectApproxEqAbs(eye.data[0][0], 1.0, 0.001);
    try testing.expectApproxEqAbs(eye.data[1][1], 1.0, 0.001);
    try testing.expectApproxEqAbs(eye.data[2][2], 1.0, 0.001);
    try testing.expectApproxEqAbs(eye.data[0][1], 0.0, 0.001);
}

// ============================================================================
// 25. C INTEROP
// ============================================================================

const c_interop = struct {
    // Translate C types
    const CStruct = extern struct {
        value: c_int,
        name: [*:0]const u8,
    };

    // Use C allocator
    fn use_c_allocator() !void {
        const alloc = std.heap.c_allocator;
        const data = try alloc.alloc(u8, 256);
        defer alloc.free(data);
    }

    // @cImport for including C headers
    // const c = @cImport({
    //     @cInclude("stdio.h");
    //     @cInclude("stdlib.h");
    // });
};

// ============================================================================
// 26. OPAQUE TYPES
// ============================================================================

const Handle = opaque {};
fn useHandle(h: *Handle) void {
    _ = h;
}

// ============================================================================
// 27. usingnamespace (namespace mixin)
// ============================================================================

const MixinExample = struct {
    // Brings all declarations from another namespace into this one
    fn mixedInFunctionPriv() u32 {
        return 42;
    }

    const mixedInFunction = if (true) mixedInFunctionPriv else unreachable;
};

// ============================================================================
// 28. NOSUSPEND, NORETURN, UNREACHABLE
// ============================================================================

fn noreturn_demo() noreturn {
    @panic("This function never returns");
}

fn unreachable_demo(x: u2) u32 {
    return switch (x) {
        0 => 10,
        1 => 20,
        2 => 30,
        3 => 40,
    };
    // all cases covered, no `else` needed
}

// ============================================================================
// 29. COMPILE-TIME INTERFACES ("Duck Typing")
// ============================================================================

fn Writer(comptime Context: type) type {
    return struct {
        context: Context,
        writeFn: *const fn (Context, []const u8) anyerror!usize,

        const Self = @This();

        fn write(self: Self, data: []const u8) !usize {
            return self.writeFn(self.context, data);
        }
    };
}

// anytype parameters: compile-time duck typing
fn serialize(writer: anytype, value: anytype) !void {
    const T = @TypeOf(value);
    switch (@typeInfo(T)) {
        .int => try writer.print("{d}", .{value}),
        .float => try writer.print("{d:.4}", .{value}),
        .pointer => |p| {
            if (p.size == .Slice and p.child == u8) {
                try writer.print("\"{s}\"", .{value});
            }
        },
        .bool => try writer.print("{}", .{value}),
        else => try writer.print("({s})", .{@typeName(T)}),
    }
}

// ============================================================================
// 30. ATOMIC OPERATIONS & THREADING
// ============================================================================

const Atomic = std.atomic.Value;

const AtomicCounter = struct {
    count: Atomic(u64),

    fn init() AtomicCounter {
        return .{ .count = Atomic(u64).init(0) };
    }

    fn increment(self: *AtomicCounter) void {
        _ = self.count.fetchAdd(1, .seq_cst);
    }

    fn load(self: *const AtomicCounter) u64 {
        return self.count.load(.seq_cst);
    }
};

fn thread_demo() !void {
    var counter = AtomicCounter.init();

    const thread = try std.Thread.spawn(.{}, struct {
        fn run(ctr: *AtomicCounter) void {
            for (0..1000) |_| {
                ctr.increment();
            }
        }
    }.run, .{&counter});

    // Do work in main thread too
    for (0..1000) |_| {
        counter.increment();
    }

    thread.join();
    // counter.load() == 2000
}

// ============================================================================
// 31. COMPTIME TYPE CONSTRUCTION (Metaprogramming)
// ============================================================================

// Build a struct type at compile time
fn FieldPair(comptime K: type, comptime V: type) type {
    return struct {
        key: K,
        value: V,
    };
}

// Create an enum from a list of names
fn MakeEnum(comptime names: []const []const u8) type {
    const initTags = std.math.IntFittingRange(0, names.len - 1);
    return @Enum(initTags, .exhaustive, names, &std.simd.iota(initTags, names.len));
}

const Fruit = MakeEnum(&.{ "apple", "banana", "cherry" });

comptime {
    const f: Fruit = .apple;
    if (@intFromEnum(f) != 0) @compileError("enum generation broken");
}

// ============================================================================
// 32. IO & FILE OPERATIONS
// ============================================================================

fn io_demo() !void {
    // stdout / stderr
    const stdout = std.io.getStdOut().writer();
    const stderr = std.io.getStdErr().writer();

    try stdout.print("Hello, {s}! The answer is {d}.\n", .{ "World", 42 });
    try stderr.print("Warning: something happened\n", .{});

    // Buffered writer
    var buf_writer = std.io.bufferedWriter(stdout);
    const writer = buf_writer.writer();
    try writer.print("Buffered output\n", .{});
    try buf_writer.flush();

    // File operations
    const cwd = std.fs.cwd();

    // Write file
    const file = try cwd.createFile("test_output.txt", .{});
    defer file.close();
    try file.writeAll("Hello from Zig!\n");

    // Read file
    const contents = try cwd.readFileAlloc(std.heap.page_allocator, "test_output.txt", 1024 * 1024);
    defer std.heap.page_allocator.free(contents);
}

// ============================================================================
// 33. FORMATTING (std.fmt)
// ============================================================================

fn format_demo() !void {
    var buf: [256]u8 = undefined;

    // Format into buffer
    const str = try std.fmt.bufPrint(&buf, "x={d}, hex=0x{x}, float={d:.3}", .{ 42, 255, 3.14159 });
    _ = str;

    // Allocating print
    const alloc = std.heap.page_allocator;
    const allocated = try std.fmt.allocPrint(alloc, "Dynamically formatted: {s}", .{"hello"});
    defer alloc.free(allocated);

    // Format specifiers:
    // {d} - decimal integer
    // {x} - hex
    // {o} - octal
    // {b} - binary
    // {s} - string
    // {c} - character
    // {e} - scientific float
    // {d:.N} - float with N decimal places
    // {any} - any type (uses format method or default)
    // {*} - pointer
}

// ============================================================================
// 34. ITERATORS & RANGES
// ============================================================================

const FibIterator = struct {
    a: u64 = 0,
    b: u64 = 1,
    limit: u64,

    fn next(self: *FibIterator) ?u64 {
        if (self.a > self.limit) return null;
        defer {
            const temp = self.a + self.b;
            self.a = self.b;
            self.b = temp;
        }
        return self.a;
    }
};

fn iterator_demo() void {
    // Range-based for
    for (0..10) |i| {
        _ = i;
    }

    // Custom iterator
    var fib = FibIterator{ .limit = 100 };
    while (fib.next()) |value| {
        _ = value;
    }
}

// ============================================================================
// 35. ALIGNMENT & PACKED DATA
// ============================================================================

fn alignment_demo() void {
    // Specify alignment
    var aligned_data: [64]u8 align(64) = undefined;
    _ = &aligned_data;

    // Check alignment
    const natural = @alignOf(u64); // typically 8
    _ = natural;

    // Packed struct (bit-level layout)
    const Pixel = packed struct {
        r: u5,
        g: u6,
        b: u5,
    };
    const p = Pixel{ .r = 31, .g = 63, .b = 31 };
    const raw: u16 = @bitCast(p);
    _ = raw;
}

// ============================================================================
// 36. CLOSURES VIA STRUCT + POINTER
// ============================================================================

// Zig doesn't have closures, but you can emulate them:
fn makeAdder(x: u32) struct {
    x: u32,

    fn call(self: @This(), y: u32) u32 {
        return self.x + y;
    }
} {
    return .{ .x = x };
}

fn closure_demo() void {
    const add5 = makeAdder(5);
    const result = add5.call(3); // 8
    _ = result;
}

// ============================================================================
// 37. COMPILE LOG & COMPILE ERROR
// ============================================================================

fn comptime_assertions() void {
    comptime {
        // @compileError("message") stops compilation
        // @compileLog(value) prints at compile time

        const x: u32 = 42;
        if (x != 42) @compileError("impossible");
    }
}

// Static assert at file scope
comptime {
    if (@sizeOf(usize) < 4) @compileError("Need at least 32-bit platform");
}

// // ============================================================================
// // 38. ASSEMBLY (inline asm)
// // ============================================================================

// fn inline_asm_demo() u64 {
//     if (builtin.cpu.arch == .x86_64) {
//         var low: u32 = undefined;
//         var high: u32 = undefined;
//         asm volatile ("rdtsc"
//             : [low] "={eax}" (low),
//               [high] "={edx}" (high),
//             :
//             :
//         );
//         return (@as(u64, high) << 32) | low;
//     }
//     return 0;
// }

fn stdlib_demo() !void {
    const alloc = std.heap.page_allocator;

    // --- std.ArrayList ---
    var list = std.ArrayList(i32).init(alloc);
    defer list.deinit();
    try list.append(1);
    try list.appendSlice(&.{ 2, 3, 4 });

    // --- std.HashMap ---
    var map = std.AutoHashMap(u32, []const u8).init(alloc);
    defer map.deinit();
    try map.put(1, "one");
    try map.put(2, "two");

    // --- std.BoundedArray ---
    var bounded = try std.BoundedArray(u8, 64).init(0);
    try bounded.appendSlice("hello");

    // --- std.PriorityQueue ---
    var pq = std.PriorityQueue(i32, void, struct {
        fn f(_: void, a: i32, b: i32) std.math.Order {
            return std.math.order(a, b);
        }
    }.f).init(alloc, {});
    defer pq.deinit();
    try pq.add(3);
    try pq.add(1);
    try pq.add(2);

    // --- std.sort ---
    var data = [_]u32{ 5, 3, 1, 4, 2 };
    std.mem.sort(u32, &data, {}, std.sort.asc(u32));

    // --- std.rand ---
    var prng = std.Random.DefaultPrng.init(12345);
    const random = prng.random();
    _ = random.int(u32);
    _ = random.float(f64);
    _ = random.intRangeAtMost(u32, 1, 100);

    // --- std.json ---
    const json_str = "{\"name\":\"Zig\",\"version\":14}";
    const parsed = try std.json.parseFromSlice(
        struct { name: []const u8, version: u32 },
        alloc,
        json_str,
        .{},
    );
    defer parsed.deinit();

    // --- std.time ---
    const timestamp = std.time.milliTimestamp();
    _ = timestamp;

    // --- std.process ---
    // const result = try std.process.Child.run(.{
    //     .allocator = alloc,
    //     .argv = &.{ "echo", "hello" },
    // });

    // --- std.math ---
    _ = math.sqrt(2.0);
    _ = math.pow(f64, 2.0, 10.0);
    _ = math.log2(8.0);
    _ = @min(@as(u32, 3), @as(u32, 5));
}

// ============================================================================
// 41. MUTEX & CONDITION VARIABLES
// ============================================================================

const ThreadSafeQueue = struct {
    mutex: std.Thread.Mutex = .{},
    items: [256]u32 = undefined,
    count: usize = 0,

    fn push(self: *ThreadSafeQueue, val: u32) void {
        self.mutex.lock();
        defer self.mutex.unlock();
        if (self.count < 256) {
            self.items[self.count] = val;
            self.count += 1;
        }
    }

    fn pop(self: *ThreadSafeQueue) ?u32 {
        self.mutex.lock();
        defer self.mutex.unlock();
        if (self.count == 0) return null;
        self.count -= 1;
        return self.items[self.count];
    }
};

// ============================================================================
// 42. SAFETY FEATURES
// ============================================================================

fn safety_demo() void {
    // Integer overflow detection (checked in Debug/ReleaseSafe)
    // var x: u8 = 255;
    // x += 1; // panic in debug: integer overflow

    // Null pointer dereference detection
    // const ptr: ?*u32 = null;
    // _ = ptr.?; // panic: attempt to unwrap null

    // Out-of-bounds detection
    // const arr = [_]u32{1, 2, 3};
    // _ = arr[5]; // panic: index out of bounds

    // Unreachable detection
    // unreachable; // panic in debug

    // Use @intCast for safe narrowing
    const big: u64 = 42;
    const small: u8 = @intCast(big); // panics if big > 255
    _ = small;
}

// ============================================================================
// 43. COMPTIME INTERFACES / TRAIT PATTERN
// ============================================================================

fn Comparable(comptime T: type) type {
    // Verify that T has the required methods at comptime
    comptime {
        if (!@hasDecl(T, "lessThan")) {
            @compileError(@typeName(T) ++ " must implement lessThan");
        }
    }
    return struct {
        fn sort(items: []T) void {
            // Simple bubble sort using the type's lessThan
            for (0..items.len) |_| {
                for (0..items.len - 1) |j| {
                    if (items[j + 1].lessThan(items[j])) {
                        const tmp = items[j];
                        items[j] = items[j + 1];
                        items[j + 1] = tmp;
                    }
                }
            }
        }
    };
}

const Temperature = struct {
    kelvin: f64,

    fn lessThan(self: Temperature, other: Temperature) bool {
        return self.kelvin < other.kelvin;
    }
};

// This works because Temperature has .lessThan
const TemperatureSorter = Comparable(Temperature);

// ============================================================================
// 44. @embedFile
// ============================================================================

// Embed a file as a byte array at compile time
// const embedded_data = @embedFile("data/config.json");
// This is extremely useful for embedding assets, shaders, etc.

// ============================================================================
// 45. SATURATING & WRAPPING ARITHMETIC
// ============================================================================

fn special_arithmetic() void {
    // Saturating arithmetic (clamps at min/max)
    const a: u8 = 200;
    const b: u8 = 100;
    const sat_add = a +| b; // 255 (saturated)
    _ = sat_add;
    const sat_sub: u8 = 10 -| 20; // 0 (saturated)
    _ = sat_sub;

    // Wrapping arithmetic (modular)
    const wrap_add = a +% b; // wraps around
    _ = wrap_add;
    const wrap_sub: u8 = @as(u8, 0) -% @as(u8, 1); // 255
    _ = wrap_sub;

    // Wrapping shift
    const shifted = @as(u8, 1) <<| 9; // saturates
    _ = shifted;
}

// ============================================================================
// 46. NOSUSPEND, COMPTIME KNOWN, @fieldParentPtr
// ============================================================================

const Node = struct {
    data: u32,
    next: ?*Node = null,

    // Intrusive linked list pattern using @fieldParentPtr
    const ListNode = struct {
        prev: ?*ListNode = null,
        next: ?*ListNode = null,
    };
};

fn field_parent_demo() void {
    const Container = struct {
        value: u32,
        hook: Node.ListNode = .{},
    };
    var c = Container{ .value = 42, .hook = .{} };
    const hook_ptr = &c.hook;
    // Get back to Container from hook pointer
    const container: *Container = @fieldParentPtr(hook_ptr, "hook");
    _ = container.value; // 42
}

// ============================================================================
// 47. COMPILE-TIME GENERATED LOOKUP TABLES
// ============================================================================

const crc32_table = blk: {
    @setEvalBranchQuota(10000);
    var table: [256]u32 = undefined;
    for (0..256) |i| {
        var crc: u32 = @intCast(i);
        for (0..8) |_| {
            if (crc & 1 != 0) {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
        table[i] = crc;
    }
    break :blk table;
};

fn crc32(data: []const u8) u32 {
    var crc: u32 = 0xFFFFFFFF;
    for (data) |byte| {
        const index = (crc ^ byte) & 0xFF;
        crc = (crc >> 8) ^ crc32_table[index];
    }
    return crc ^ 0xFFFFFFFF;
}

test "crc32 lookup table" {
    const result = crc32("Hello");
    try testing.expect(result != 0);
}

// ============================================================================
// MAIN
// ============================================================================

pub fn main(init: std.process.Init) !void {
    var stdout_buffer: [4096]u8 = undefined;
    var stdout_writer = std.Io.File.stdout().writer(init.io, &stdout_buffer);
    const stdout = &stdout_writer.interface;

    try stdout.print(
        \\
        \\╔══════════════════════════════════════════════╗
        \\║       ZIG LANGUAGE FEATURE SHOWCASE          ║
        \\╠══════════════════════════════════════════════╣
        \\
    , .{});
    // 1. Types
    try stdout.print("║ 1.  Primitive types & literals              ║\n", .{});
    try stdout.print("║     u8={d}, i32 min={d}        ║\n", .{ primitive_examples.a_u8, primitive_examples.a_i32 });
    // 2. Strings
    try stdout.print("║ 2.  String: {s}                 ║\n", .{string_examples.hello});
    // 3. Struct method
    const p1 = Point{ .x = 3, .y = 4 };
    const p2 = Point.origin();
    try stdout.print("║ 3.  Point distance: {d:.2}                   ║\n", .{p1.distance(p2)});
    // 4. Comptime
    try stdout.print("║ 4.  Comptime sum(1..100) = {d}              ║\n", .{comptime_result});
    try stdout.print("║ 5.  Comptime fib(10) = {d}                  ║\n", .{comptimeFib(10)});
    try stdout.print("║ 6.  Comptime upper: {s}                ║\n", .{comptimeUpperCase("hello")});
    // 5. Generics
    var stack = Stack(u32){};
    try stack.push(10);
    try stack.push(20);
    try stack.push(30);
    try stdout.print("║ 7.  Stack pop: {?d}                         ║\n", .{stack.pop()});
    // 6. Error handling
    const div_result = divide(22, 7) catch 0;
    try stdout.print("║ 8.  22/7 = {d:.6}                      ║\n", .{div_result});
    // 7. Generic max
    try stdout.print("║ 9.  max(42, 17) = {d}                       ║\n", .{max_generic(u32, 42, 17)});
    // 8. Function from function
    const op = get_operation('+');
    try stdout.print("║ 10. op(3,4) = {d}                            ║\n", .{op(3, 4)});
    // 9. SIMD
    const v: @Vector(4, f32) = .{ 1, 2, 3, 4 };
    const sum = @reduce(.Add, v);
    try stdout.print("║ 11. SIMD reduce sum = {d:.1}                ║\n", .{sum});
    // 10. Matrix
    const Mat3 = Matrix(f64, 3, 3);
    const eye = Mat3.identity();
    try stdout.print("║ 12. Matrix[1][1] = {d:.0} (identity)          ║\n", .{eye.data[1][1]});
    // 11. CRC
    try stdout.print("║ 13. CRC32(\"Hello\") = 0x{X:0>8}          ║\n", .{crc32("Hello")});
    // 12. Enum
    try stdout.print("║ 14. North opposite = {s}                ║\n", .{@tagName(Direction.north.opposite())});
    // 13. Closure emulation
    const add5 = makeAdder(5);
    try stdout.print("║ 15. Closure add5(3) = {d}                    ║\n", .{add5.call(3)});
    // 14. Special arithmetic
    const sat: u8 = @as(u8, 200) +| @as(u8, 100);
    try stdout.print("║ 16. Saturating 200+|100 = {d}                ║\n", .{sat});
    // 15. Comptime enum
    try stdout.print("║ 17. Comptime enum Fruit: {s}            ║\n", .{@tagName(Fruit.banana)});
    // 16. Arch info
    try stdout.print("║ 18. Target arch: {s}                   ║\n", .{@tagName(builtin.cpu.arch)});
    // 17. Atomic counter
    var counter = AtomicCounter.init();
    counter.increment();
    counter.increment();
    try stdout.print("║ 19. Atomic counter: {d}                       ║\n", .{counter.load()});
    // Allocator demo
    try stdout.print("║ 20. Allocators, JSON, sorting...   [OK]     ║\n", .{});
    try stdout.print(
        \\╠══════════════════════════════════════════════╣
        \\║  47 features showcased. Run tests with:      ║
        \\║    zig test zig_showcase.zig                  ║
        \\╚══════════════════════════════════════════════╝
        \\
    , .{});

    try stdout.flush();

    // Call the C++ showcase via the extern "C" bridge
    const manifest = @import("manifest");
    _ = manifest.cpp.cpp_main();

    try channel.channel_main(init.io);
}
