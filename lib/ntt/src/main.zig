const std = @import("std");
const ntt = @import("ntt");
const posix = @import("posix.zig");
const c = std.c;

const linux = std.os.linux;

// Extern C functions
extern "c" fn openpty(
    master: *c_int,
    slave: *c_int,
    name: ?[*]u8,
    termp: ?*const posix.termios,
    winsize: ?*const anyopaque,
) c_int;

// Terminal control constants for Linux
const IGNBRK: u32 = 0o000001;
const BRKINT: u32 = 0o000002;
const PARMRK: u32 = 0o000010;
const ISTRIP: u32 = 0o000040;
const INLCR: u32 = 0o000100;
const IGNCR: u32 = 0o000200;
const ICRNL: u32 = 0o000400;
const IXON: u32 = 0o002000;
const OPOST: u32 = 0o000001;
const ECHO: u32 = 0o000010;
const ECHONL: u32 = 0o000100;
const ICANON: u32 = 0o000002;
const ISIG: u32 = 0o000001;
const IEXTEN: u32 = 0o100000;
const CSIZE: u32 = 0o000060;
const PARENB: u32 = 0o000400;
const CS8: u32 = 0o000060;
const VMIN: usize = 6;
const VTIME: usize = 5;
const TIOCSCTTY: u32 = 0x540E;
const TIOCGWINSZ: u32 = 0x5413;
const TIOCSWINSZ: u32 = 0x5414;
const F_GETFL: c_int = 3;
const F_SETFL: c_int = 4;
const O_NONBLOCK: u32 = 0o4000;

// Socket address family
const AF_UNIX: u32 = 1;
const SOCK_STREAM: u32 = 1;

// Socket address structure for Unix domain sockets
const sockaddr_un = extern struct {
    sun_family: u16,
    sun_path: [108]u8,
};

// Window size structure
const winsize = extern struct {
    ws_row: u16,
    ws_col: u16,
    ws_xpixel: u16,
    ws_ypixel: u16,
};

// Terminal session structure
const TerminalSession = struct {
    master_fd: c_int,
    child_pid: posix.pid_t,
};

// Global variables for signal handler
var global_master_fd: c_int = -1;
var global_stdin_fd: c_int = -1;

// Pending write buffer for non-blocking PTY writes
var pending_write_buf: [64 * 1024]u8 = undefined;
var pending_write_start: usize = 0;
var pending_write_end: usize = 0;

fn setNonBlocking(fd: c_int) !void {
    const flags = linux.fcntl(fd, F_GETFL, @as(usize, 0));
    if (@as(i32, @bitCast(@as(u32, @truncate(flags)))) < 0) return error.FcntlError;
    const rc = linux.fcntl(fd, F_SETFL, flags | O_NONBLOCK);
    if (@as(i32, @bitCast(@as(u32, @truncate(rc)))) < 0) return error.FcntlError;
}

fn pendingLen() usize {
    return pending_write_end - pending_write_start;
}

fn enqueuePendingWrite(data: []const u8) void {
    // Compact buffer if needed
    if (pending_write_start > 0 and pending_write_end + data.len > pending_write_buf.len) {
        const len = pendingLen();
        std.mem.copyForwards(u8, pending_write_buf[0..len], pending_write_buf[pending_write_start..pending_write_end]);
        pending_write_start = 0;
        pending_write_end = len;
    }
    const copy_len = @min(data.len, pending_write_buf.len - pending_write_end);
    @memcpy(pending_write_buf[pending_write_end .. pending_write_end + copy_len], data[0..copy_len]);
    pending_write_end += copy_len;
    _ = data.len - copy_len; // remaining bytes (if any) are dropped
}

fn flushPendingWrite(fd: c_int) void {
    while (pending_write_start < pending_write_end) {
        const to_write = pending_write_buf[pending_write_start..pending_write_end];
        const rc = linux.write(fd, to_write.ptr, to_write.len);
        const signed: isize = @bitCast(rc);
        if (signed < 0) {
            // EAGAIN/EWOULDBLOCK - can't write more right now
            return;
        }
        if (rc == 0) return;
        pending_write_start += @intCast(rc);
    }
    // Reset when fully drained
    pending_write_start = 0;
    pending_write_end = 0;
}

/// Non-blocking write to PTY. Writes as much as possible, enqueues the rest.
fn ptyWrite(fd: c_int, data: []const u8) void {
    // First flush any pending data
    if (pendingLen() > 0) {
        flushPendingWrite(fd);
        if (pendingLen() > 0) {
            // Still can't write, enqueue new data too
            enqueuePendingWrite(data);
            return;
        }
    }
    // Try to write directly
    var written: usize = 0;
    while (written < data.len) {
        const rc = linux.write(fd, data.ptr + written, data.len - written);
        const signed: isize = @bitCast(rc);
        if (signed < 0) {
            // EAGAIN - enqueue remainder
            enqueuePendingWrite(data[written..]);
            return;
        }
        if (rc == 0) {
            enqueuePendingWrite(data[written..]);
            return;
        }
        written += @intCast(rc);
    }
}

// Global variables for terminal management
var terminals: std.ArrayList(TerminalSession) = undefined;
var current_terminal_index: usize = 0;
var global_allocator: std.mem.Allocator = undefined;
var global_socket_path: [108]u8 = undefined;
var global_socket_path_len: usize = 0;

// PID file path
const PID_FILE_PATH = "/tmp/ntt.pid";

// Signal handler for window resize
fn handleSigwinch(_: posix.SIG) callconv(.c) void {
    if (global_master_fd < 0 or global_stdin_fd < 0) return;

    var ws: winsize = undefined;
    if (linux.ioctl(global_stdin_fd, TIOCGWINSZ, @intFromPtr(&ws)) == 0) {
        _ = linux.ioctl(global_master_fd, TIOCSWINSZ, @intFromPtr(&ws));
    }
}

// Create a new PTY terminal session
fn createTerminalSession(ws: *const winsize) !TerminalSession {
    var master_fd: c_int = undefined;
    var slave_fd: c_int = undefined;
    if (openpty(&master_fd, &slave_fd, null, null, @ptrCast(ws)) < 0) {
        return error.OpenptyError;
    }

    const pid = try posix.fork();
    if (pid == 0) {
        // Child: close master, create new session, set controlling terminal, then dup2
        posix.close(master_fd) catch posix.exit(1);

        _ = posix.setsid() catch posix.exit(1);
        _ = linux.ioctl(slave_fd, TIOCSCTTY, @as(usize, 0));

        posix.dup2(slave_fd, posix.STDIN_FILENO) catch posix.exit(1);
        posix.dup2(slave_fd, posix.STDOUT_FILENO) catch posix.exit(1);
        posix.dup2(slave_fd, posix.STDERR_FILENO) catch posix.exit(1);
        if (slave_fd > posix.STDERR_FILENO) {
            posix.close(slave_fd) catch {};
        }

        // Exec bash
        const argv = [_:null]?[*:0]const u8{ "bash", null };
        _ = posix.execveZ("/bin/bash", &argv, std.c.environ) catch {};
        posix.exit(1);
    }

    // Parent: Close slave, return session
    try posix.close(slave_fd);

    return TerminalSession{
        .master_fd = master_fd,
        .child_pid = pid,
    };
}

// Create Unix socket for receiving commands
fn createCommandSocket(pid: posix.pid_t) !c_int {
    const sock_fd = try posix.socket(AF_UNIX, SOCK_STREAM, 0);
    errdefer posix.close(sock_fd) catch {};

    var addr = std.mem.zeroes(sockaddr_un);
    addr.sun_family = AF_UNIX;

    // Create socket path: /tmp/ntt-<pid>.sock
    const path = try std.fmt.bufPrintZ(&global_socket_path, "/tmp/ntt-{d}.sock", .{pid});
    global_socket_path_len = path.len;
    @memcpy(addr.sun_path[0..path.len], path);

    // Remove old socket file if it exists
    posix.unlink(path) catch {};

    // Bind the socket
    const addr_ptr: *const posix.sockaddr = @ptrCast(&addr);
    try posix.bind(sock_fd, addr_ptr, @sizeOf(sockaddr_un));

    // Listen for connections
    try posix.listen(sock_fd, 5);

    // std.debug.print("Command socket created at: {s}\n", .{path});

    return sock_fd;
}

// Handle command received from client
fn handleCommand(cmd: []const u8, ws: *const winsize) !void {
    // Parse command (format: "command args...")
    var iter = std.mem.splitScalar(u8, cmd, ' ');
    const command = iter.first();

    if (std.mem.eql(u8, command, "new")) {
        // Create a new terminal session
        const new_session = try createTerminalSession(ws);
        try setNonBlocking(new_session.master_fd);
        try terminals.append(global_allocator, new_session);
        // Immediately switch to the new terminal
        current_terminal_index = terminals.items.len - 1;
        global_master_fd = new_session.master_fd;
    } else if (std.mem.eql(u8, command, "next")) {
        // Switch to next terminal in round-robin fashion
        if (terminals.items.len > 0) {
            current_terminal_index = (current_terminal_index + 1) % terminals.items.len;
            global_master_fd = terminals.items[current_terminal_index].master_fd;
        }
    }
}

// Write PID file with server's PID
fn writePidFile(io: std.Io, pid: posix.pid_t) !void {
    const file = try std.Io.Dir.cwd().createFile(io, PID_FILE_PATH, .{});
    defer file.close(io);
    var buf: [32]u8 = undefined;
    const pid_str = try std.fmt.bufPrint(&buf, "{d}", .{pid});
    try file.writePositionalAll(io, pid_str, 0);
}

// Remove PID file
fn removePidFile(io: std.Io) !void {
    try std.Io.Dir.cwd().deleteFile(io, PID_FILE_PATH);
}

pub fn pidExistsLinux(io: std.Io, pid: linux.pid_t) bool {
    if (pid == 0) return false; // Invalid PID

    var path_buf: [32]u8 = undefined;
    const path = std.fmt.bufPrint(&path_buf, "/proc/{d}/stat", .{pid}) catch return false;

    std.Io.Dir.cwd().access(io, path, .{}) catch return false;
    return true;
}

// Read PID from PID file and return socket name, returns null if no valid server
fn findServerSocket(io: std.Io, allocator: std.mem.Allocator) !?[]u8 {
    // Read PID from file
    const file = std.Io.Dir.cwd().openFile(io, PID_FILE_PATH, .{}) catch return null;
    defer file.close(io);

    var buf: [32]u8 = undefined;
    const bytes_read = file.readPositionalAll(io, &buf, 0) catch return null;
    if (bytes_read == 0) return null;

    const pid_str = std.mem.trimEnd(u8, buf[0..bytes_read], &[_]u8{ '\n', '\r', ' ', '\t' });
    const pid = std.fmt.parseInt(posix.pid_t, pid_str, 10) catch return null;

    // Check if process exists by sending signal 0 (doesn't actually send a signal, just checks)

    if (!pidExistsLinux(io, pid)) {
        // Process doesn't exist, clean up stale PID file
        try removePidFile(io);
        return null;
    }

    // Build socket name and path
    var socket_name_buf: [64]u8 = undefined;
    const socket_name = try std.fmt.bufPrint(&socket_name_buf, "ntt-{d}.sock", .{pid});

    // Check if socket file exists (verifies server is likely running)
    var socket_path_buf: [128]u8 = undefined;
    const socket_path = std.fmt.bufPrint(&socket_path_buf, "/tmp/{s}", .{socket_name}) catch return null;
    std.Io.Dir.cwd().access(io, socket_path, .{}) catch {
        // Socket doesn't exist, clean up stale PID file
        try removePidFile(io);
        return null;
    };

    return try allocator.dupe(u8, socket_name);
}

// Send a command to the running server
fn sendCommand(socket_name: []const u8, cmd: []const u8) !void {
    // Create socket path
    var path_buf: [256]u8 = undefined;
    const socket_path = try std.fmt.bufPrintZ(&path_buf, "/tmp/{s}", .{socket_name});

    // Create socket and connect
    const sock_fd = try posix.socket(AF_UNIX, SOCK_STREAM, 0);
    defer posix.close(sock_fd) catch {};

    var addr = std.mem.zeroes(sockaddr_un);
    addr.sun_family = AF_UNIX;
    @memcpy(addr.sun_path[0..socket_path.len], socket_path);

    const addr_ptr: *const posix.sockaddr = @ptrCast(&addr);
    try posix.connect(sock_fd, addr_ptr, @sizeOf(sockaddr_un));

    // Send command
    _ = try posix.write(sock_fd, cmd);
}

// Client mode: send command to running server
fn runClient(io: std.Io, allocator: std.mem.Allocator, args: []const [:0]const u8) !void {
    // Find the master process by looking for socket files
    const found_socket = try findServerSocket(io, allocator);

    if (found_socket == null) {
        std.debug.print("Error: No ntt server process found. Please start ntt first.\n", .{});
        return error.NoMasterProcess;
    }
    defer allocator.free(found_socket.?);

    // Build command string from arguments
    var cmd_buf: [1024]u8 = undefined;
    var cmd_len: usize = 0;

    for (args[1..]) |arg| {
        if (cmd_len > 0) {
            cmd_buf[cmd_len] = ' ';
            cmd_len += 1;
        }
        @memcpy(cmd_buf[cmd_len .. cmd_len + arg.len], arg);
        cmd_len += arg.len;
    }

    try sendCommand(found_socket.?, cmd_buf[0..cmd_len]);
}

// Close a terminal and switch to the next one
fn closeTerminal(index: usize) !void {
    if (index >= terminals.items.len) return;

    const terminal = terminals.items[index];
    try posix.close(terminal.master_fd);
    _ = try posix.waitpid(terminal.child_pid, 0);

    // Remove the terminal from the list
    _ = terminals.orderedRemove(index);

    // If no terminals left, we'll exit
    if (terminals.items.len == 0) return;

    // Adjust current index after removal
    if (current_terminal_index >= terminals.items.len) {
        current_terminal_index = 0;
    } else if (current_terminal_index > index) {
        // The current terminal index needs to be adjusted down since we removed a terminal before it
        current_terminal_index -= 1;
    }
    // If current_terminal_index == index, we stay at the same index (which now points to the next terminal)

    global_master_fd = terminals.items[current_terminal_index].master_fd;
}

// Server mode: run the terminal multiplexer
fn runServer(io: std.Io, allocator: std.mem.Allocator) !void {
    global_allocator = allocator;

    // Initialize terminals array
    terminals = .{};
    defer {
        // Clean up all terminals
        for (terminals.items) |terminal| {
            posix.close(terminal.master_fd) catch {};
            _ = posix.waitpid(terminal.child_pid, 0) catch {};
        }
        terminals.deinit(allocator);
    }

    // Step 1: Save original terminal attributes and enter raw mode
    const stdin_fd = posix.STDIN_FILENO;
    const stdout_fd = posix.STDOUT_FILENO;

    const orig_termios = try posix.tcgetattr(stdin_fd);
    defer posix.tcsetattr(stdin_fd, .FLUSH, orig_termios) catch {}; // Restore on exit

    var raw_termios = orig_termios;

    // Work with raw integer values for the flags
    var iflag_raw = @as(u32, @bitCast(raw_termios.iflag));
    iflag_raw &= ~(IGNBRK | BRKINT | PARMRK | ISTRIP | INLCR | IGNCR | ICRNL | IXON);
    raw_termios.iflag = @bitCast(iflag_raw);

    var oflag_raw = @as(u32, @bitCast(raw_termios.oflag));
    oflag_raw &= ~OPOST;
    raw_termios.oflag = @bitCast(oflag_raw);

    var lflag_raw = @as(u32, @bitCast(raw_termios.lflag));
    lflag_raw &= ~(ECHO | ECHONL | ICANON | ISIG | IEXTEN);
    raw_termios.lflag = @bitCast(lflag_raw);

    var cflag_raw = @as(u32, @bitCast(raw_termios.cflag));
    cflag_raw &= ~(CSIZE | PARENB);
    cflag_raw |= CS8;
    raw_termios.cflag = @bitCast(cflag_raw);

    raw_termios.cc[VMIN] = 1; // Read at least 1 byte
    raw_termios.cc[VTIME] = 0; // No timeout
    try posix.tcsetattr(stdin_fd, .FLUSH, raw_termios);

    // Step 2: Get current terminal window size
    var ws: winsize = undefined;
    if (linux.ioctl(stdin_fd, TIOCGWINSZ, @intFromPtr(&ws)) != 0) {
        // If we can't get size, use reasonable defaults
        ws.ws_row = 24;
        ws.ws_col = 80;
        ws.ws_xpixel = 0;
        ws.ws_ypixel = 0;
    }

    // Step 3: Create first terminal session
    const first_session = try createTerminalSession(&ws);
    try setNonBlocking(first_session.master_fd);
    try terminals.append(allocator, first_session);
    current_terminal_index = 0;

    // Use our own PID (the ntt server process) for socket and PID file
    const server_pid = linux.getpid();

    // Create command socket for IPC
    const sock_fd = try createCommandSocket(@intCast(server_pid));
    defer posix.close(sock_fd) catch {};
    defer posix.unlink(global_socket_path[0..global_socket_path_len :0]) catch {};

    // Write PID file so clients can find us
    try writePidFile(io, @intCast(server_pid));
    defer removePidFile(io) catch {};

    // Step 5: Set up SIGWINCH handler to update PTY size on terminal resize
    const sa = posix.Sigaction{
        .handler = .{ .handler = handleSigwinch },
        .mask = [_]c_ulong{0} ** 16,
        .flags = posix.SA.RESTART,
    };
    posix.sigaction(posix.SIG.WINCH, &sa, null);

    // Store master_fd and stdin_fd in globals for signal handler
    global_master_fd = terminals.items[current_terminal_index].master_fd;
    global_stdin_fd = stdin_fd;

    // Step 6: I/O loop with polling to avoid blocking
    var buf: [1024]u8 = undefined;
    var drain_buf: [4096]u8 = undefined;

    // Dynamic pollfds: [0]=stdin, [1]=sock_fd, [2..2+N]=terminal master_fds
    const MAX_TERMINALS = 64;
    var pollfds: [2 + MAX_TERMINALS]posix.pollfd = undefined;
    pollfds[0] = .{ .fd = stdin_fd, .events = posix.POLL.IN, .revents = 0 };
    pollfds[1] = .{ .fd = sock_fd, .events = posix.POLL.IN, .revents = 0 };

    var client_fd: c_int = -1; // Currently connected client

    while (true) {
        global_master_fd = terminals.items[current_terminal_index].master_fd;

        // Build pollfds for all terminals so inactive ones get drained
        const num_terms = terminals.items.len;
        for (terminals.items, 0..) |term, idx| {
            pollfds[2 + idx] = .{
                .fd = term.master_fd,
                .events = posix.POLL.IN | (if (idx == current_terminal_index and pendingLen() > 0) posix.POLL.OUT else @as(i16, 0)),
                .revents = 0,
            };
        }

        // Poll stdin, socket, and ALL terminal PTYs
        _ = posix.poll(pollfds[0 .. 2 + num_terms], -1) catch break;

        // Flush pending writes if active PTY is writable
        if (pollfds[2 + current_terminal_index].revents & posix.POLL.OUT != 0) {
            flushPendingWrite(terminals.items[current_terminal_index].master_fd);
        }

        // Check if stdin has data (user input)
        if (pollfds[0].revents & posix.POLL.IN != 0) {
            if (posix.read(stdin_fd, &buf)) |n| {
                if (n == 0) break;

                // Forward input to PTY, only intercepting ntt control keys.
                // Escape sequences are passed through as-is — the child
                // application (vim, bash, etc.) handles them.
                // Regular bytes are batched and written in bulk to avoid
                // PTY buffer deadlocks when pasting large text.
                var i: usize = 0;
                while (i < n) : (i += 1) {
                    const byte = buf[i];

                    if (byte == 0x02) {
                        // Ctrl+B detected - switch to next terminal
                        if (terminals.items.len > 1) {
                            current_terminal_index = (current_terminal_index + 1) % terminals.items.len;
                            global_master_fd = terminals.items[current_terminal_index].master_fd;
                            // Print a visual indicator that we switched
                            _ = posix.write(stdout_fd, "\x1b[2J\x1b[H") catch {}; // Clear screen and move to top
                            _ = posix.write(stdout_fd, "\x1b[7m") catch {}; // Reverse video
                            var status_buf: [64]u8 = undefined;
                            const status = std.fmt.bufPrint(&status_buf, " Terminal {}/{} ", .{ current_terminal_index + 1, terminals.items.len }) catch "";
                            _ = posix.write(stdout_fd, status) catch {};
                            _ = posix.write(stdout_fd, "\x1b[0m\r\n") catch {}; // Reset formatting
                        } else {
                            // Only one terminal, just show status
                            _ = posix.write(stdout_fd, "\x1b[7m Terminal 1/1 \x1b[0m\r\n") catch {};
                        }
                    } else if (byte == 0x0e) {
                        // Ctrl+N detected - create new terminal session
                        const new_session = createTerminalSession(&ws) catch |err| {
                            var err_buf: [128]u8 = undefined;
                            const err_msg = std.fmt.bufPrint(&err_buf, "\r\nFailed to create terminal: {}\r\n", .{err}) catch "Failed to create terminal\r\n";
                            _ = posix.write(stdout_fd, err_msg) catch {};
                            continue;
                        };
                        setNonBlocking(new_session.master_fd) catch {};
                        terminals.append(allocator, new_session) catch |err| {
                            try posix.close(new_session.master_fd);
                            _ = try posix.waitpid(new_session.child_pid, 0);
                            var err_buf: [128]u8 = undefined;
                            const err_msg = std.fmt.bufPrint(&err_buf, "\r\nFailed to add terminal: {}\r\n", .{err}) catch "Failed to add terminal\r\n";
                            _ = posix.write(stdout_fd, err_msg) catch {};
                            continue;
                        };
                        // Switch to the new terminal
                        current_terminal_index = terminals.items.len - 1;
                        global_master_fd = new_session.master_fd;
                        // Print a visual indicator
                        _ = posix.write(stdout_fd, "\x1b[2J\x1b[H") catch {}; // Clear screen and move to top
                        _ = posix.write(stdout_fd, "\x1b[7m") catch {}; // Reverse video
                        var status_buf: [64]u8 = undefined;
                        const status = std.fmt.bufPrint(&status_buf, " New Terminal {}/{} ", .{ current_terminal_index + 1, terminals.items.len }) catch "";
                        _ = posix.write(stdout_fd, status) catch {};
                        _ = posix.write(stdout_fd, "\x1b[0m\r\n") catch {}; // Reset formatting
                    } else {
                        // Regular bytes - collect a contiguous run and write in bulk
                        const start = i;
                        while (i + 1 < n and buf[i + 1] != 0x02 and buf[i + 1] != 0x0e) : (i += 1) {}
                        const chunk = buf[start .. i + 1];
                        ptyWrite(terminals.items[current_terminal_index].master_fd, chunk);
                    }
                }
            } else |_| break;
        }

        // Handle output from ALL terminal PTYs
        {
            var term_idx: usize = 0;
            while (term_idx < terminals.items.len) {
                const poll_idx = 2 + term_idx;

                // Check for hangup first
                if (pollfds[poll_idx].revents & posix.POLL.HUP != 0) {
                    closeTerminal(term_idx) catch {};
                    if (terminals.items.len == 0) break;
                    continue; // don't increment - next terminal shifted into this slot
                }

                if (pollfds[poll_idx].revents & posix.POLL.IN != 0) {
                    const master_fd_cur = terminals.items[term_idx].master_fd;
                    const rc = linux.read(master_fd_cur, &buf, buf.len);
                    const signed: isize = @bitCast(rc);
                    if (signed < 0) {
                        const err_code: u32 = @truncate(@as(usize, @bitCast(-signed)));
                        if (err_code != 11) { // 11 = EAGAIN
                            closeTerminal(term_idx) catch {};
                            if (terminals.items.len == 0) break;
                            continue;
                        }
                    } else if (rc == 0) {
                        closeTerminal(term_idx) catch {};
                        if (terminals.items.len == 0) break;
                        continue;
                    } else if (term_idx == current_terminal_index) {
                        // Active terminal: display output
                        const n: usize = @intCast(rc);
                        _ = posix.write(stdout_fd, buf[0..n]) catch break;
                    }
                    // Inactive terminals: output is read and discarded to prevent
                    // the kernel PTY buffer from filling up and blocking the child.
                }

                term_idx += 1;
            }
            if (terminals.items.len == 0) break;

            // Drain any remaining buffered output from inactive terminals
            // so they don't block between poll cycles
            for (terminals.items, 0..) |term, idx| {
                if (idx == current_terminal_index) continue;
                while (true) {
                    const rc = linux.read(term.master_fd, &drain_buf, drain_buf.len);
                    const signed: isize = @bitCast(rc);
                    if (signed <= 0) break; // EAGAIN or EOF
                }
            }
        }

        // Check if command socket has incoming connection
        if (pollfds[1].revents & posix.POLL.IN != 0) {
            // Accept new connection (only one at a time for simplicity)
            if (client_fd >= 0) {
                try posix.close(client_fd);
            }
            const accept_result = linux.accept4(sock_fd, null, null, 0);
            client_fd = if (accept_result >= 0) @intCast(accept_result) else -1;

            if (client_fd >= 0) {
                // Read command from client
                if (posix.read(client_fd, &buf)) |n| {
                    if (n > 0) {
                        // Trim any trailing newline/whitespace
                        var cmd_len = n;
                        while (cmd_len > 0 and (buf[cmd_len - 1] == '\n' or buf[cmd_len - 1] == '\r')) {
                            cmd_len -= 1;
                        }
                        handleCommand(buf[0..cmd_len], &ws) catch {};
                    }
                } else |_| {}

                try posix.close(client_fd);
                client_fd = -1;
            }
        }

        // Check for stdin hangup
        if (pollfds[0].revents & posix.POLL.HUP != 0) {
            break;
        }
    }

    // Cleanup handled by defer
}

pub fn main(init: std.process.Init) !void {
    // Initialize allocator
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    // Get command line arguments
    const args = try init.minimal.args.toSlice(allocator);
    defer allocator.free(args);

    if (args.len == 1) {
        // No arguments - check if server is already running
        if (try findServerSocket(init.io, allocator)) |socket_name| {
            // Server is running, send "new" command (equivalent to "ntt new")
            defer allocator.free(socket_name);
            try sendCommand(socket_name, "new");
        } else {
            // No server running - start as server
            try runServer(init.io, allocator);
        }
    } else {
        // Has arguments - run as client
        try runClient(init.io, allocator, args);
    }
}
