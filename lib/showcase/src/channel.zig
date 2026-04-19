const std = @import("std");

// ════════════════════════════════════════════════════════════════════
// Channel
// ════════════════════════════════════════════════════════════════════

fn Channel(comptime T: type, comptime capacity: usize) type {
    return struct {
        const Self = @This();
        const is_unbuffered = capacity == 0;
        const buf_len = if (is_unbuffered) 1 else capacity;

        const SelectWaiter = struct {
            cond: *std.Io.Condition,
            mu: *std.Io.Mutex,
            notified: *bool,
            next: ?*SelectWaiter = null,
        };

        mu: std.Io.Mutex = .init,
        not_empty: std.Io.Condition = .init,
        not_full: std.Io.Condition = .init,

        buf: [buf_len]T = undefined,
        head: usize = 0,
        tail: usize = 0,
        count: usize = 0,

        // rendezvous state (unbuffered)
        rendezvous_item: ?T = null,
        rendezvous_taken: std.Io.Condition = .init,
        rendezvous_ready: std.Io.Condition = .init,
        sender_waiting: bool = false,

        // select support — waiters register here to be notified
        select_waiters: ?*SelectWaiter = null,

        closed: bool = false,

        // ── send ──────────────────────────────────────

        pub fn send(self: *Self, io: std.Io, item: T) !void {
            if (is_unbuffered) return self.sendRendezvous(io, item);
            return self.sendBuffered(io, item);
        }

        fn sendBuffered(self: *Self, io: std.Io, item: T) !void {
            self.mu.lockUncancelable(io);
            defer self.mu.unlock(io);
            while (self.count == capacity) {
                if (self.closed) return error.Closed;
                self.not_full.waitUncancelable(io, &self.mu);
            }
            if (self.closed) return error.Closed;
            self.pushItem(item);
            self.not_empty.signal(io);
            self.notifySelectWaiters(io);
        }

        fn sendRendezvous(self: *Self, io: std.Io, item: T) !void {
            self.mu.lockUncancelable(io);
            defer self.mu.unlock(io);
            while (self.sender_waiting) {
                if (self.closed) return error.Closed;
                self.not_full.waitUncancelable(io, &self.mu);
            }
            if (self.closed) return error.Closed;
            self.rendezvous_item = item;
            self.sender_waiting = true;
            self.rendezvous_ready.signal(io);
            self.not_empty.signal(io);
            self.notifySelectWaiters(io);
            while (self.sender_waiting) {
                if (self.closed) return error.Closed;
                self.rendezvous_taken.waitUncancelable(io, &self.mu);
            }
        }

        // ── recv ──────────────────────────────────────

        /// Blocking receive. Returns error.Closed when channel is closed
        /// AND drained (buffered) or just closed (unbuffered).
        pub fn recv(self: *Self, io: std.Io) !T {
            if (is_unbuffered) return self.recvRendezvous(io);
            return self.recvBuffered(io);
        }

        /// Blocking receive that also reports open/closed status.
        /// Returns .{ value, true } on success, .{ undefined, false } when
        /// the channel is closed and drained. Equivalent to `val, ok := <-ch` in Go.
        pub fn recvOk(self: *Self, io: std.Io) struct { value: T, ok: bool } {
            if (is_unbuffered) return self.recvOkRendezvous(io);
            return self.recvOkBuffered(io);
        }

        fn recvBuffered(self: *Self, io: std.Io) !T {
            self.mu.lockUncancelable(io);
            defer self.mu.unlock(io);
            while (self.count == 0) {
                if (self.closed) return error.Closed;
                self.not_empty.waitUncancelable(io, &self.mu);
            }
            return self.popItemAndNotify(io);
        }

        fn recvOkBuffered(self: *Self, io: std.Io) struct { value: T, ok: bool } {
            self.mu.lockUncancelable(io);
            defer self.mu.unlock(io);
            while (self.count == 0) {
                if (self.closed) {
                    return .{ .value = undefined, .ok = false };
                }
                self.not_empty.waitUncancelable(io, &self.mu);
            }
            return .{ .value = self.popItemAndNotify(io), .ok = true };
        }

        fn recvRendezvous(self: *Self, io: std.Io) !T {
            self.mu.lockUncancelable(io);
            defer self.mu.unlock(io);
            while (!self.sender_waiting) {
                if (self.closed) return error.Closed;
                self.rendezvous_ready.waitUncancelable(io, &self.mu);
            }
            return self.takeRendezvousAndNotify(io);
        }

        fn recvOkRendezvous(self: *Self, io: std.Io) struct { value: T, ok: bool } {
            self.mu.lockUncancelable(io);
            defer self.mu.unlock(io);
            while (!self.sender_waiting) {
                if (self.closed) {
                    return .{ .value = undefined, .ok = false };
                }
                self.rendezvous_ready.waitUncancelable(io, &self.mu);
            }
            return .{ .value = self.takeRendezvousAndNotify(io), .ok = true };
        }

        // ── non-blocking try variants ─────────────────

        pub fn trySend(self: *Self, io: std.Io, item: T) error{ WouldBlock, Closed }!void {
            self.mu.lockUncancelable(io);
            defer self.mu.unlock(io);
            if (self.closed) return error.Closed;
            if (is_unbuffered) {
                // unbuffered trySend: only works if a receiver is blocked
                // (not fully supported without deeper runtime hooks)
                return error.WouldBlock;
            }
            if (self.count == capacity) return error.WouldBlock;
            self.pushItem(item);
            self.not_empty.signal(io);
            self.notifySelectWaiters(io);
        }

        pub fn tryRecv(self: *Self, io: std.Io) error{ WouldBlock, Closed }!T {
            self.mu.lockUncancelable(io);
            defer self.mu.unlock(io);
            if (is_unbuffered) {
                if (!self.sender_waiting) {
                    if (self.closed) return error.Closed;
                    return error.WouldBlock;
                }
                return self.takeRendezvousAndNotify(io);
            }
            if (self.count == 0) {
                if (self.closed) return error.Closed;
                return error.WouldBlock;
            }
            return self.popItemAndNotify(io);
        }

        /// Non-blocking recv that also reports ok status.
        pub fn tryRecvOk(self: *Self, io: std.Io) error{WouldBlock}!struct { value: T, ok: bool } {
            self.mu.lockUncancelable(io);
            defer self.mu.unlock(io);
            if (is_unbuffered) {
                if (!self.sender_waiting) {
                    if (self.closed) return .{ .value = undefined, .ok = false };
                    return error.WouldBlock;
                }
                return .{ .value = self.takeRendezvousAndNotify(io), .ok = true };
            }
            if (self.count == 0) {
                if (self.closed) return .{ .value = undefined, .ok = false };
                return error.WouldBlock;
            }
            return .{ .value = self.popItemAndNotify(io), .ok = true };
        }

        // ── close ─────────────────────────────────────

        pub fn close(self: *Self, io: std.Io) void {
            self.mu.lockUncancelable(io);
            defer self.mu.unlock(io);
            self.closed = true;
            self.not_empty.broadcast(io);
            self.not_full.broadcast(io);
            self.rendezvous_ready.broadcast(io);
            self.rendezvous_taken.broadcast(io);
            self.notifySelectWaiters(io);
        }

        // ── len (for Go's len(ch)) ───────────────────

        pub fn len(self: *Self, io: std.Io) usize {
            self.mu.lockUncancelable(io);
            defer self.mu.unlock(io);
            if (is_unbuffered) {
                return if (self.sender_waiting) 1 else 0;
            }
            return self.count;
        }

        // ── iterator (range) ─────────────────────────

        pub const Iterator = struct {
            ch: *Self,
            io: std.Io,

            pub fn next(self: *Iterator) ?T {
                return self.ch.recv(self.io) catch null;
            }
        };

        pub fn iterate(self: *Self, io: std.Io) Iterator {
            return .{ .ch = self, .io = io };
        }

        // ── directional wrappers ─────────────────────

        pub const SendOnly = struct {
            ch: *Self,
            pub fn send(self: SendOnly, io: std.Io, item: T) !void {
                return self.ch.send(io, item);
            }
            pub fn trySend(self: SendOnly, io: std.Io, item: T) error{ WouldBlock, Closed }!void {
                return self.ch.trySend(io, item);
            }
            pub fn close(self: SendOnly, io: std.Io) void {
                self.ch.close(io);
            }
        };

        pub const RecvOnly = struct {
            ch: *Self,
            pub fn recv(self: RecvOnly, io: std.Io) !T {
                return self.ch.recv(io);
            }
            pub fn recvOk(self: RecvOnly, io: std.Io) struct { value: T, ok: bool } {
                return self.ch.recvOk(io);
            }
            pub fn tryRecv(self: RecvOnly, io: std.Io) error{ WouldBlock, Closed }!T {
                return self.ch.tryRecv(io);
            }
            pub fn iterate(self: RecvOnly, io: std.Io) Iterator {
                return self.ch.iterate(io);
            }
        };

        pub fn sendOnly(self: *Self) SendOnly {
            return .{ .ch = self };
        }

        pub fn recvOnly(self: *Self) RecvOnly {
            return .{ .ch = self };
        }

        // ── internal helpers ─────────────────────────

        fn pushItem(self: *Self, item: T) void {
            self.buf[self.tail] = item;
            self.tail = (self.tail + 1) % buf_len;
            self.count += 1;
        }

        fn popItemAndNotify(self: *Self, io: std.Io) T {
            const item = self.buf[self.head];
            self.head = (self.head + 1) % buf_len;
            self.count -= 1;
            self.not_full.signal(io);
            self.notifySelectWaiters(io);
            return item;
        }

        fn takeRendezvousAndNotify(self: *Self, io: std.Io) T {
            const item = self.rendezvous_item.?;
            self.rendezvous_item = null;
            self.sender_waiting = false;
            self.rendezvous_taken.signal(io);
            self.not_full.signal(io);
            self.notifySelectWaiters(io);
            return item;
        }

        // ── select waiter support ───────────────────────

        /// Called while holding self.mu — signals all registered select waiters.
        fn notifySelectWaiters(self: *Self, io: std.Io) void {
            var w = self.select_waiters;
            while (w) |waiter| {
                waiter.mu.lockUncancelable(io);
                waiter.notified.* = true;
                waiter.mu.unlock(io);
                waiter.cond.signal(io);
                w = waiter.next;
            }
        }

        /// Register a select waiter on this channel (locks self.mu).
        fn registerSelectWaiter(self: *Self, io: std.Io, waiter: *SelectWaiter) void {
            self.mu.lockUncancelable(io);
            waiter.next = self.select_waiters;
            self.select_waiters = waiter;
            self.mu.unlock(io);
        }

        /// Unregister a select waiter from this channel (locks self.mu).
        fn unregisterSelectWaiter(self: *Self, io: std.Io, waiter: *SelectWaiter) void {
            self.mu.lockUncancelable(io);
            if (self.select_waiters == waiter) {
                self.select_waiters = waiter.next;
            } else {
                var cur = self.select_waiters;
                while (cur) |c| {
                    if (c.next == waiter) {
                        c.next = waiter.next;
                        break;
                    }
                    cur = c.next;
                }
            }
            self.mu.unlock(io);
        }
    };
}

// ════════════════════════════════════════════════════════════════════
// Select
// ════════════════════════════════════════════════════════════════════
//
// Go's select is a language primitive. We emulate it with a builder
// pattern that collects cases, then executes. Supports:
//   - recv cases           (case msg := <-ch)
//   - recv-ok cases        (case msg, ok := <-ch)
//   - send cases           (case ch <- val)
//   - nil / disabled cases
//   - default (non-blocking)
//   - timeout
//   - random pick when multiple ready (fairness)
//

fn Select(comptime T: type, comptime capacity: usize, comptime max_cases: usize) type {
    const Ch = Channel(T, capacity);

    return struct {
        const Self = @This();

        const CaseKind = enum { recv, recv_ok, send, disabled };

        const Case = struct {
            kind: CaseKind,
            ch: ?*Ch, // null = disabled (nil channel)
            send_val: ?T, // only for send cases
            tag: usize,
        };

        const RecvOkResult = struct { value: T, ok: bool };

        const Result = union(enum) {
            recv: struct { tag: usize, value: T },
            recv_ok: struct { tag: usize, value: T, ok: bool },
            sent: struct { tag: usize },
            default: void,
            timeout: void,
            closed: void,
        };

        cases: [max_cases]Case = undefined,
        n: usize = 0,

        // ── builder ──────────────────────────────────

        /// Add a recv case. Equivalent to: case val := <-ch
        pub fn addRecv(self: *Self, ch: ?*Ch, tag: usize) void {
            self.cases[self.n] = .{
                .kind = if (ch != null) .recv else .disabled,
                .ch = ch,
                .send_val = null,
                .tag = tag,
            };
            self.n += 1;
        }

        /// Add a recv-ok case. Equivalent to: case val, ok := <-ch
        pub fn addRecvOk(self: *Self, ch: ?*Ch, tag: usize) void {
            self.cases[self.n] = .{
                .kind = if (ch != null) .recv_ok else .disabled,
                .ch = ch,
                .send_val = null,
                .tag = tag,
            };
            self.n += 1;
        }

        /// Add a send case. Equivalent to: case ch <- val
        pub fn addSend(self: *Self, ch: ?*Ch, val: T, tag: usize) void {
            self.cases[self.n] = .{
                .kind = if (ch != null) .send else .disabled,
                .ch = ch,
                .send_val = val,
                .tag = tag,
            };
            self.n += 1;
        }

        // ── execute ──────────────────────────────────

        /// Blocking select. Waits until one case fires.
        pub fn run(self: *Self, io: std.Io) Result {
            return self.runInner(io, false, null);
        }

        /// Non-blocking select (with default). Returns .default if nothing ready.
        pub fn runWithDefault(self: *Self, io: std.Io) Result {
            return self.runInner(io, true, null);
        }

        /// Select with timeout. Returns .timeout if deadline passes.
        pub fn runWithTimeout(self: *Self, io: std.Io, timeout_ch: ?*Channel(void, 0)) Result {
            return self.runInner(io, false, timeout_ch);
        }

        fn runInner(
            self: *Self,
            io: std.Io,
            has_default: bool,
            timeout_ch: ?*Channel(void, 0),
        ) Result {
            const cases = self.cases[0..self.n];

            // Phase 1: fast path — try all in randomized order
            if (self.tryOnce(io, cases, timeout_ch)) |result| return result;

            // If default, return immediately
            if (has_default) return .default;

            // Phase 2: slow path — register on ALL channels, then wait
            var shared_mu: std.Io.Mutex = .init;
            var shared_cond: std.Io.Condition = .init;
            var notified: bool = false;

            // Create waiter nodes and register on each channel
            var waiter_nodes: [max_cases]Ch.SelectWaiter = undefined;
            for (cases, 0..) |c, i| {
                waiter_nodes[i] = .{
                    .cond = &shared_cond,
                    .mu = &shared_mu,
                    .notified = &notified,
                    .next = null,
                };
                if (c.kind != .disabled) {
                    if (c.ch) |ch| {
                        ch.registerSelectWaiter(io, &waiter_nodes[i]);
                    }
                }
            }

            // Ensure we deregister from all channels on exit
            defer {
                for (cases, 0..) |c, i| {
                    if (c.kind != .disabled) {
                        if (c.ch) |ch| {
                            ch.unregisterSelectWaiter(io, &waiter_nodes[i]);
                        }
                    }
                }
            }

            while (true) {
                if (self.tryOnce(io, cases, timeout_ch)) |result| return result;

                // Check if all non-disabled cases are closed
                var all_closed = true;
                var any_enabled = false;
                for (cases) |c| {
                    if (c.kind == .disabled) continue;
                    any_enabled = true;
                    if (c.ch) |ch| {
                        if (!ch.closed) {
                            all_closed = false;
                            break;
                        }
                    }
                }
                if (!any_enabled or all_closed) return .closed;

                // Wait until any channel signals us
                shared_mu.lockUncancelable(io);
                if (!notified) {
                    shared_cond.waitUncancelable(io, &shared_mu);
                }
                notified = false;
                shared_mu.unlock(io);
            }
        }

        /// Try each case once in shuffled order for fairness.
        /// Returns non-null if a case fired.
        fn tryOnce(
            self: *Self,
            io: std.Io,
            cases: []const Case,
            timeout_ch: ?*Channel(void, 0),
        ) ?Result {
            _ = self;

            // Build shuffled index array
            var indices: [max_cases]usize = undefined;
            for (0..cases.len) |i| indices[i] = i;
            shuffle(indices[0..cases.len], io);

            // Check timeout channel first
            if (timeout_ch) |tch| {
                if (tch.tryRecv(io)) |_| {
                    return .timeout;
                } else |_| {}
            }

            for (indices[0..cases.len]) |idx| {
                const c = cases[idx];
                switch (c.kind) {
                    .disabled => continue,
                    .recv => {
                        if (c.ch) |ch| {
                            if (ch.tryRecv(io)) |val| {
                                return .{ .recv = .{ .tag = c.tag, .value = val } };
                            } else |_| {}
                        }
                    },
                    .recv_ok => {
                        if (c.ch) |ch| {
                            if (ch.tryRecvOk(io)) |res| {
                                return .{ .recv_ok = .{
                                    .tag = c.tag,
                                    .value = res.value,
                                    .ok = res.ok,
                                } };
                            } else |_| {} // WouldBlock
                        }
                    },
                    .send => {
                        if (c.ch) |ch| {
                            if (c.send_val) |val| {
                                ch.trySend(io, val) catch |err| switch (err) {
                                    error.WouldBlock => continue,
                                    error.Closed => continue,
                                };
                                return .{ .sent = .{ .tag = c.tag } };
                            }
                        }
                    },
                }
            }
            return null;
        }

        /// Fisher-Yates shuffle using a simple xorshift seeded from
        /// the clock so we get Go-like random fairness.
        fn shuffle(slice: []usize, io: std.Io) void {
            if (slice.len <= 1) return;
            // Seed from nanosecond timestamp — good enough for fairness
            var seed: u64 = @bitCast(@as(i64, @truncate(std.Io.Timestamp.now(io, .boot).nanoseconds)));
            var i: usize = slice.len - 1;
            while (i > 0) : (i -= 1) {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                const j = seed % (i + 1);
                const tmp = slice[i];
                slice[i] = slice[j];
                slice[j] = tmp;
            }
        }
    };
}

// ════════════════════════════════════════════════════════════════════
// Timer / Ticker helpers (Go's time.After, time.NewTicker)
// ════════════════════════════════════════════════════════════════════

/// One-shot timer that sends to a channel after a delay.
/// Equivalent to Go's time.After().
fn Timer(comptime T: type) type {
    return struct {
        ch: Channel(T, 1) = .{},
        thread: ?std.Thread = null,

        const SelfTimer = @This();

        fn start(self: *SelfTimer, io: std.Io, ns: u64, val: T) !void {
            const Ctx = struct { s: *SelfTimer, io: std.Io, ns: u64, val: T };
            self.thread = try std.Thread.spawn(.{}, struct {
                fn run(ctx: Ctx) void {
                    ctx.io.sleep(
                        std.Io.Duration.fromNanoseconds(ctx.ns),
                        .boot,
                    ) catch {};
                    ctx.s.ch.send(ctx.io, ctx.val) catch {};
                }
            }.run, .{Ctx{ .s = self, .io = io, .ns = ns, .val = val }});
        }

        fn join(self: *SelfTimer) void {
            if (self.thread) |t| t.join();
        }
    };
}

/// Repeating ticker that sends to a channel at fixed intervals.
/// Equivalent to Go's time.NewTicker().
fn Ticker(comptime T: type) type {
    return struct {
        ch: Channel(T, 1) = .{},
        stop_flag: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),
        thread: ?std.Thread = null,

        const SelfTicker = @This();

        fn start(self: *SelfTicker, io: std.Io, interval_ns: u64, val: T) !void {
            const Ctx = struct { s: *SelfTicker, io: std.Io, ns: u64, val: T };
            self.thread = try std.Thread.spawn(.{}, struct {
                fn run(ctx: Ctx) void {
                    while (!ctx.s.stop_flag.load(.acquire)) {
                        ctx.io.sleep(
                            std.Io.Duration.fromNanoseconds(ctx.ns),
                            .boot,
                        ) catch {};
                        if (ctx.s.stop_flag.load(.acquire)) break;
                        ctx.s.ch.send(ctx.io, ctx.val) catch {};
                    }
                }
            }.run, .{Ctx{ .s = self, .io = io, .ns = interval_ns, .val = val }});
        }

        fn stop(self: *SelfTicker) void {
            self.stop_flag.store(true, .release);
            if (self.thread) |t| t.join();
        }
    };
}

// ════════════════════════════════════════════════════════════════════
// Examples — mirror of every Go example
// ════════════════════════════════════════════════════════════════════

const Buf1 = Channel([]const u8, 1);
const Unbuf = Channel([]const u8, 0);
const IntBuf1 = Channel(i32, 1);
const IntUnbuf = Channel(i32, 0);

fn sleep(io: std.Io, ns: u64) void {
    io.sleep(std.Io.Duration.fromNanoseconds(ns), .boot) catch {};
}

const ms = 1_000_000;

// ── 1. Basic select ──────────────────────────────────────────────

fn example1_basic(io: std.Io) !void {
    std.debug.print("--- 1. Basic Select ---\n", .{});

    const Ctx = struct { ch: *Unbuf, io: std.Io };

    var ch1 = Unbuf{};
    var ch2 = Unbuf{};

    _ = try std.Thread.spawn(.{}, struct {
        fn f(ctx: Ctx) void {
            sleep(ctx.io, 50 * ms);
            ctx.ch.send(ctx.io, "one") catch {};
        }
    }.f, .{Ctx{ .ch = &ch1, .io = io }});

    _ = try std.Thread.spawn(.{}, struct {
        fn f(ctx: Ctx) void {
            sleep(ctx.io, 30 * ms);
            ctx.ch.send(ctx.io, "two") catch {};
        }
    }.f, .{Ctx{ .ch = &ch2, .io = io }});

    var sel = Select([]const u8, 0, 4){};
    sel.addRecv(&ch1, 0);
    sel.addRecv(&ch2, 1);

    switch (sel.run(io)) {
        .recv => |r| std.debug.print("Received: {s}\n", .{r.value}),
        else => {},
    }
    std.debug.print("\n", .{});
    sleep(io, 60 * ms); // let other goroutine finish
}

// ── 2. Non-blocking with default ─────────────────────────────────

fn example2_default(io: std.Io) !void {
    std.debug.print("--- 2. Non-Blocking with Default ---\n", .{});

    var ch = IntUnbuf{};

    var sel = Select(i32, 0, 4){};
    sel.addRecv(&ch, 0);

    switch (sel.runWithDefault(io)) {
        .recv => |r| std.debug.print("Got: {d}\n", .{r.value}),
        .default => std.debug.print("No value ready, moving on\n", .{}),
        else => {},
    }
    std.debug.print("\n", .{});
}

// ── 3. Timeout ───────────────────────────────────────────────────

fn example3_timeout(io: std.Io) !void {
    std.debug.print("--- 3. Timeout ---\n", .{});

    const Ctx = struct { ch: *Buf1, io: std.Io };
    var ch = Buf1{};

    _ = try std.Thread.spawn(.{}, struct {
        fn f(ctx: Ctx) void {
            sleep(ctx.io, 200 * ms);
            ctx.ch.send(ctx.io, "slow result") catch {};
        }
    }.f, .{Ctx{ .ch = &ch, .io = io }});

    // Use a timer channel for timeout
    var timer = Timer([]const u8){};
    try timer.start(io, 100 * ms, "timeout");

    var sel = Select([]const u8, 1, 4){};
    sel.addRecv(&ch, 0);
    sel.addRecv(&timer.ch, 1);

    switch (sel.run(io)) {
        .recv => |r| {
            if (r.tag == 0) {
                std.debug.print("Got: {s}\n", .{r.value});
            } else {
                std.debug.print("Timed out!\n", .{});
            }
        },
        else => {},
    }
    std.debug.print("\n", .{});
    timer.join();
}

// ── 4. Random pick when both ready (fairness) ────────────────────

fn example4_multipleChannels(io: std.Io) !void {
    std.debug.print("--- 4. Random Pick When Both Ready ---\n", .{});

    var ch1 = Buf1{};
    var ch2 = Buf1{};

    var count_alpha: usize = 0;
    var count_beta: usize = 0;

    for (0..100) |_| {
        // refill
        if (ch1.len(io) == 0) ch1.send(io, "alpha") catch {};
        if (ch2.len(io) == 0) ch2.send(io, "beta") catch {};

        var sel = Select([]const u8, 1, 4){};
        sel.addRecv(&ch1, 0);
        sel.addRecv(&ch2, 1);

        switch (sel.run(io)) {
            .recv => |r| {
                if (r.tag == 0) count_alpha += 1 else count_beta += 1;
            },
            else => {},
        }
    }
    std.debug.print("Distribution over 100 picks: alpha={d} beta={d}\n", .{ count_alpha, count_beta });
    std.debug.print("\n", .{});
}

// ── 5. Nil channel (disable a case) ──────────────────────────────

fn example5_nilChannel(io: std.Io) !void {
    std.debug.print("--- 5. Nil Channel (Disable a Case) ---\n", .{});

    var ch1 = Buf1{};
    try ch1.send(io, "only option");

    var sel = Select([]const u8, 1, 4){};
    sel.addRecv(&ch1, 0);
    sel.addRecv(null, 1); // nil channel — disabled

    switch (sel.run(io)) {
        .recv => |r| {
            if (r.tag == 0) {
                std.debug.print("ch1: {s}\n", .{r.value});
            } else {
                std.debug.print("ch2: {s}\n", .{r.value}); // never reached
            }
        },
        else => {},
    }
    std.debug.print("\n", .{});
}

// ── 6. Select in a loop until close ──────────────────────────────

fn example6_loop(io: std.Io) !void {
    std.debug.print("--- 6. Select in a Loop ---\n", .{});

    const Ctx = struct { ch: *IntUnbuf, io: std.Io };
    var ch = IntUnbuf{};

    const t = try std.Thread.spawn(.{}, struct {
        fn f(ctx: Ctx) void {
            for (1..4) |i| {
                ctx.ch.send(ctx.io, @intCast(i)) catch return;
                sleep(ctx.io, 30 * ms);
            }
            ctx.ch.close(ctx.io);
        }
    }.f, .{Ctx{ .ch = &ch, .io = io }});

    while (true) {
        var sel = Select(i32, 0, 4){};
        sel.addRecvOk(&ch, 0);

        switch (sel.run(io)) {
            .recv_ok => |r| {
                if (!r.ok) {
                    std.debug.print("Channel closed, exiting loop\n", .{});
                    break;
                }
                std.debug.print("  Got: {d}\n", .{r.value});
            },
            .closed => break,
            else => {},
        }
    }
    t.join();
    std.debug.print("\n", .{});
}

// ── 7. Done / cancel pattern ─────────────────────────────────────

fn example7_done(io: std.Io) !void {
    std.debug.print("--- 7. Done / Cancel Pattern ---\n", .{});

    const VoidCh = Channel(void, 0);
    const Ctx = struct { done: *VoidCh, results: *IntUnbuf, io: std.Io };
    var done = VoidCh{};
    var results = IntUnbuf{};

    _ = try std.Thread.spawn(.{}, struct {
        fn f(ctx: Ctx) void {
            var i: i32 = 0;
            while (true) {
                // Try to send result, but also check done.
                // We implement this as: try send, if blocked check done.
                if (ctx.done.tryRecv(ctx.io)) |_| {
                    std.debug.print("  Worker stopped\n", .{});
                    return;
                } else |_| {}

                ctx.results.send(ctx.io, i) catch return;
                i += 1;
            }
        }
    }.f, .{Ctx{ .done = &done, .results = &results, .io = io }});

    // Take 3 values then cancel
    for (0..3) |_| {
        const val = results.recv(io) catch break;
        std.debug.print("  Received: {d}\n", .{val});
    }
    done.close(io); // signal stop
    sleep(io, 20 * ms);
    std.debug.print("\n", .{});
}

// ── 8. Fan-in (merge channels) ───────────────────────────────────

fn example8_fanIn(io: std.Io) !void {
    std.debug.print("--- 8. Fan-In (Merge Channels) ---\n", .{});

    const Ctx = struct { ch: *Unbuf, io: std.Io };
    var ch_a = Unbuf{};
    var ch_b = Unbuf{};

    _ = try std.Thread.spawn(.{}, struct {
        fn f(ctx: Ctx) void {
            const items = [_][]const u8{ "A-0", "A-1" };
            for (items) |item| {
                sleep(ctx.io, 30 * ms);
                ctx.ch.send(ctx.io, item) catch return;
            }
        }
    }.f, .{Ctx{ .ch = &ch_a, .io = io }});

    _ = try std.Thread.spawn(.{}, struct {
        fn f(ctx: Ctx) void {
            const items = [_][]const u8{ "B-0", "B-1" };
            for (items) |item| {
                sleep(ctx.io, 50 * ms);
                ctx.ch.send(ctx.io, item) catch return;
            }
        }
    }.f, .{Ctx{ .ch = &ch_b, .io = io }});

    for (0..4) |_| {
        var sel = Select([]const u8, 0, 4){};
        sel.addRecv(&ch_a, 0);
        sel.addRecv(&ch_b, 1);

        switch (sel.run(io)) {
            .recv => |r| {
                if (r.tag == 0) {
                    std.debug.print("  From A: {s}\n", .{r.value});
                } else {
                    std.debug.print("  From B: {s}\n", .{r.value});
                }
            },
            else => {},
        }
    }
    std.debug.print("\n", .{});
}

// ── 9. Ticker + quit ─────────────────────────────────────────────

fn example9_ticker(io: std.Io) !void {
    std.debug.print("--- 9. Ticker + Quit ---\n", .{});

    const TickVal = []const u8;
    var ticker = Ticker(TickVal){};
    try ticker.start(io, 40 * ms, "tick");

    var quit_timer = Timer(TickVal){};
    try quit_timer.start(io, 150 * ms, "quit");

    var count: usize = 0;
    while (true) {
        var sel = Select(TickVal, 1, 4){};
        sel.addRecv(&ticker.ch, 0);
        sel.addRecv(&quit_timer.ch, 1);

        switch (sel.run(io)) {
            .recv => |r| {
                if (r.tag == 0) {
                    count += 1;
                    std.debug.print("  Tick #{d}\n", .{count});
                } else {
                    std.debug.print("  Quit signal received\n", .{});
                    ticker.stop();
                    quit_timer.join();
                    break;
                }
            },
            else => break,
        }
    }
    std.debug.print("\n", .{});
}

// ── 10. Select on send ──────────────────────────────────────────

fn example10_selectSend(io: std.Io) !void {
    std.debug.print("--- 10. Select on Send ---\n", .{});

    var fast = Buf1{};
    var slow = Buf1{};

    // fast has buffer space, slow does not
    {
        var sel = Select([]const u8, 1, 4){};
        sel.addSend(&fast, "data", 0);
        sel.addSend(&slow, "data", 1);

        switch (sel.runWithDefault(io)) {
            .sent => |r| {
                if (r.tag == 0) {
                    std.debug.print("Sent to fast channel\n", .{});
                } else {
                    std.debug.print("Sent to slow channel\n", .{});
                }
            },
            .default => std.debug.print("Neither channel ready\n", .{}),
            else => {},
        }

        const val = fast.recv(io) catch "???";
        std.debug.print("fast received: {s}\n", .{val});
    }

    // Random send when both buffered channels ready
    {
        var a = IntBuf1{};
        var b = IntBuf1{};

        var sel = Select(i32, 1, 4){};
        sel.addSend(&a, 42, 0);
        sel.addSend(&b, 42, 1);

        switch (sel.run(io)) {
            .sent => |r| {
                if (r.tag == 0) {
                    const v = a.recv(io) catch 0;
                    std.debug.print("Sent {d} to channel a\n", .{v});
                } else {
                    const v = b.recv(io) catch 0;
                    std.debug.print("Sent {d} to channel b\n", .{v});
                }
            },
            else => {},
        }
    }
    std.debug.print("\n", .{});
}

// ── 11. Mixed send + recv in one select ──────────────────────────
//
// Go equivalent:
//
//   select {
//   case msg := <-inbox:
//       process(msg)
//   case outbox <- result:
//       fmt.Println("forwarded")
//   case err := <-errors:
//       handleErr(err)
//   case <-done:
//       return
//   }
//
// This is a pipeline stage: read from inbox, compute, try to push
// to outbox, while also watching for errors and a done signal.

const Msg = union(enum) {
    data: []const u8,
    err: []const u8,
    done: void,
    result: []const u8,
};

const MsgCh = Channel(Msg, 4);
const MsgUnbuf = Channel(Msg, 0);

fn example11_mixedSendRecv(io: std.Io) !void {
    std.debug.print("--- 11. Mixed Send + Recv in One Select ---\n", .{});

    var inbox = MsgCh{};
    var outbox = MsgCh{};
    var errors = MsgCh{};
    var done = MsgCh{};

    const ProducerCtx = struct { inbox: *MsgCh, errors: *MsgCh, done: *MsgCh, io: std.Io };
    // Producer: sends work items then signals done
    _ = try std.Thread.spawn(.{}, struct {
        fn f(ctx: ProducerCtx) void {
            const items = [_][]const u8{ "task-1", "task-2", "task-3" };
            for (items) |item| {
                sleep(ctx.io, 20 * ms);
                ctx.inbox.send(ctx.io, .{ .data = item }) catch return;
            }
            // send one error mid-stream
            sleep(ctx.io, 10 * ms);
            ctx.errors.send(ctx.io, .{ .err = "disk full" }) catch {};

            // one more task then done
            sleep(ctx.io, 10 * ms);
            ctx.inbox.send(ctx.io, .{ .data = "task-4" }) catch {};
            sleep(ctx.io, 20 * ms);
            ctx.done.send(ctx.io, .done) catch {};
        }
    }.f, .{ProducerCtx{ .inbox = &inbox, .errors = &errors, .done = &done, .io = io }});

    const ConsumerCtx = struct { outbox: *MsgCh, io: std.Io };
    // Consumer on the other end of outbox
    const consumer_t = try std.Thread.spawn(.{}, struct {
        fn f(ctx: ConsumerCtx) void {
            while (true) {
                const msg = ctx.outbox.recv(ctx.io) catch break;
                switch (msg) {
                    .result => |r| std.debug.print("  [consumer] got result: {s}\n", .{r}),
                    else => {},
                }
            }
        }
    }.f, .{ConsumerCtx{ .outbox = &outbox, .io = io }});

    // Pipeline stage: mixed send + recv select loop
    var pending_result: ?Msg = null;
    var running = true;

    while (running) {
        var sel = Select(Msg, 4, 8){};

        // recv cases — always active
        sel.addRecv(&inbox, 0); // case msg := <-inbox
        sel.addRecv(&errors, 1); // case err := <-errors
        sel.addRecv(&done, 2); // case <-done

        // send case — only active when we have something to forward
        if (pending_result != null) {
            sel.addSend(&outbox, pending_result.?, 3); // case outbox <- result
        }

        switch (sel.run(io)) {
            .recv => |r| {
                switch (r.tag) {
                    0 => {
                        // received from inbox — process and queue result
                        switch (r.value) {
                            .data => |d| {
                                std.debug.print("  [stage] processing: {s}\n", .{d});
                                pending_result = .{ .result = d };
                            },
                            else => {},
                        }
                    },
                    1 => {
                        // received error
                        switch (r.value) {
                            .err => |e| std.debug.print("  [stage] error: {s}\n", .{e}),
                            else => {},
                        }
                    },
                    2 => {
                        // done signal
                        std.debug.print("  [stage] done signal received\n", .{});
                        running = false;
                    },
                    else => {},
                }
            },
            .sent => |r| {
                if (r.tag == 3) {
                    std.debug.print("  [stage] forwarded result to outbox\n", .{});
                    pending_result = null;
                }
            },
            .closed => {
                running = false;
            },
            else => {},
        }
    }

    // Drain any last pending result
    if (pending_result) |pr| {
        outbox.send(io, pr) catch {};
        std.debug.print("  [stage] forwarded final result\n", .{});
    }

    outbox.close(io);
    consumer_t.join();
    std.debug.print("\n", .{});
}

// ── 12. Work-stealing / load-balancing with mixed select ─────────
//
// Go equivalent:
//
//   select {
//   case job := <-jobs:
//       result := process(job)
//       select {
//       case fast <- result:
//       case slow <- result:
//       }
//   case <-done:
//       return
//   }
//
// Worker receives jobs and sends results to whichever output
// channel has capacity first — simple load balancing.

fn example12_loadBalance(io: std.Io) !void {
    std.debug.print("--- 12. Load-Balancing with Mixed Select ---\n", .{});

    const WorkCh = Channel(i32, 4);
    const JobsCtx = struct { jobs: *WorkCh, io: std.Io };
    const WorkerCtx = struct { ch: *WorkCh, io: std.Io };
    var jobs = WorkCh{};
    var fast_out = WorkCh{};
    var slow_out = WorkCh{};

    // Send jobs
    _ = try std.Thread.spawn(.{}, struct {
        fn f(ctx: JobsCtx) void {
            for (0..6) |i| {
                ctx.jobs.send(ctx.io, @intCast(i)) catch return;
            }
            ctx.jobs.close(ctx.io);
        }
    }.f, .{JobsCtx{ .jobs = &jobs, .io = io }});

    // Fast consumer (drains quickly)
    const fast_t = try std.Thread.spawn(.{}, struct {
        fn f(ctx: WorkerCtx) void {
            var it = ctx.ch.iterate(ctx.io);
            while (it.next()) |val| {
                std.debug.print("  [fast] processed: {d}\n", .{val});
                sleep(ctx.io, 10 * ms);
            }
        }
    }.f, .{WorkerCtx{ .ch = &fast_out, .io = io }});

    // Slow consumer (drains slowly, so its buffer fills up)
    const slow_t = try std.Thread.spawn(.{}, struct {
        fn f(ctx: WorkerCtx) void {
            var it = ctx.ch.iterate(ctx.io);
            while (it.next()) |val| {
                std.debug.print("  [slow] processed: {d}\n", .{val});
                sleep(ctx.io, 80 * ms);
            }
        }
    }.f, .{WorkerCtx{ .ch = &slow_out, .io = io }});

    // Worker: recv from jobs, send to whichever output is ready
    while (true) {
        // Phase 1: get a job
        const job = jobs.recv(io) catch break;
        const result = job * 10; // "process"

        // Phase 2: send result to whichever output has capacity
        var sel = Select(i32, 4, 4){};
        sel.addSend(&fast_out, result, 0);
        sel.addSend(&slow_out, result, 1);

        switch (sel.run(io)) {
            .sent => |r| {
                const dest = if (r.tag == 0) "fast" else "slow";
                std.debug.print("  [worker] sent {d} -> {s}\n", .{ result, dest });
            },
            else => {},
        }
    }

    fast_out.close(io);
    slow_out.close(io);
    fast_t.join();
    slow_t.join();
    std.debug.print("\n", .{});
}

// ── 13. Bidirectional ping-pong with mixed select ────────────────
//
// Two goroutines each select on both send and recv simultaneously
// on the same two channels, playing ping-pong.

fn example13_pingPong(io: std.Io) !void {
    std.debug.print("--- 13. Bidirectional Ping-Pong ---\n", .{});

    const PingCh = Channel(i32, 1);
    const PingCtx = struct { send_ch: *PingCh, recv_ch: *PingCh, io: std.Io };
    var left_to_right = PingCh{};
    var right_to_left = PingCh{};

    // Seed: left sends first
    try left_to_right.send(io, 0);

    // Left player
    const left_t = try std.Thread.spawn(.{}, struct {
        fn f(ctx: PingCtx) void {
            for (0..5) |_| {
                // recv from right
                const val = ctx.recv_ch.recv(ctx.io) catch return;
                std.debug.print("  [left]  got {d}, sending {d}\n", .{ val, val + 1 });
                sleep(ctx.io, 20 * ms);

                // send to right — could also use select here to mix with timeout
                ctx.send_ch.send(ctx.io, val + 1) catch return;
            }
        }
    }.f, .{PingCtx{ .send_ch = &left_to_right, .recv_ch = &right_to_left, .io = io }});

    // Right player — uses mixed select to recv and conditionally send
    const right_t = try std.Thread.spawn(.{}, struct {
        fn f(ctx: PingCtx) void {
            for (0..5) |_| {
                // mixed select: try to recv from left
                var sel = Select(i32, 1, 4){};
                sel.addRecv(ctx.recv_ch, 0);

                switch (sel.run(ctx.io)) {
                    .recv => |r| {
                        std.debug.print("  [right] got {d}, sending {d}\n", .{ r.value, r.value + 1 });
                        sleep(ctx.io, 20 * ms);

                        // send back — use select with timeout to avoid deadlock
                        var send_sel = Select(i32, 1, 4){};
                        send_sel.addSend(ctx.send_ch, r.value + 1, 0);

                        switch (send_sel.run(ctx.io)) {
                            .sent => {},
                            else => return,
                        }
                    },
                    else => return,
                }
            }
        }
    }.f, .{PingCtx{ .send_ch = &right_to_left, .recv_ch = &left_to_right, .io = io }});

    left_t.join();
    right_t.join();
    std.debug.print("\n", .{});
}

// ════════════════════════════════════════════════════════════════════
// Main
// ════════════════════════════════════════════════════════════════════

pub fn channel_main(io: std.Io) !void {
    std.debug.print("=== Zig Select Statement Showcase ===\n\n", .{});

    try example1_basic(io);
    try example2_default(io);
    try example3_timeout(io);
    try example4_multipleChannels(io);
    try example5_nilChannel(io);
    try example6_loop(io);
    try example7_done(io);
    try example8_fanIn(io);
    try example9_ticker(io);
    try example10_selectSend(io);
    try example11_mixedSendRecv(io);
    try example12_loadBalance(io);
    try example13_pingPong(io);
}
