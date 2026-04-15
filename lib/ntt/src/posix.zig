const std = @import("std");
const system = std.posix.system;

// Re-export types
pub const termios = system.termios;
pub const pid_t = system.pid_t;
pub const sockaddr = system.sockaddr;
pub const pollfd = system.pollfd;
pub const Sigaction = system.Sigaction;
pub const SIG = system.SIG;
pub const SA = system.SA;
pub const POLL = system.POLL;

// Re-export constants
pub const STDIN_FILENO = system.STDIN_FILENO;
pub const STDOUT_FILENO = system.STDOUT_FILENO;
pub const STDERR_FILENO = system.STDERR_FILENO;

pub fn close(fd: i32) !void {
    const rc = system.close(fd);
    if (rc < 0) {
        return error.CloseError;
    }
}

pub fn dup2(oldfd: i32, newfd: i32) !void {
    const rc = system.dup2(oldfd, newfd);
    if (rc < 0) {
        return error.Dup2Error;
    }
}

pub fn setsid() !pid_t {
    const rc = system.setsid();
    if (rc < 0) {
        return error.SetsidError;
    }
    return @intCast(rc);
}

pub fn execveZ(path: [*:0]const u8, argv: [*:null]const ?[*:0]const u8, envp: [*:null]?[*:0]u8) !void {
    const rc = system.execve(path, argv, envp);
    if (rc < 0) {
        return error.ExecveError;
    }
}

pub fn fork() !i32 {
    const rc: i32 = @bitCast(system.fork());
    if (rc < 0) return error.ForkError;
    return rc;
}

pub fn socket(domain: u32, socket_type: u32, protocol: u32) !i32 {
    const rc = system.socket(domain, socket_type, protocol);
    if (rc < 0) {
        return error.SocketError;
    }
    return @intCast(rc);
}

pub fn bind(sockfd: i32, addr: *const sockaddr, addrlen: u32) !void {
    const rc = system.bind(sockfd, addr, addrlen);
    if (rc < 0) {
        return error.BindError;
    }
}

pub fn listen(sockfd: i32, backlog: i32) !void {
    const rc = system.listen(sockfd, @intCast(backlog));
    if (rc < 0) {
        return error.ListenError;
    }
}

pub fn connect(sockfd: i32, addr: *const sockaddr, addrlen: u32) !void {
    const rc = system.connect(sockfd, addr, addrlen);
    if (rc < 0) {
        return error.ConnectError;
    }
}

pub fn write(fd: i32, buf: []const u8) !void {
    var total: usize = 0;
    while (total < buf.len) {
        const rc = system.write(fd, buf.ptr + total, buf.len - total);
        if (rc < 0) {
            return error.WriteError;
        }
        total += @intCast(rc);
    }
}

pub fn read(fd: i32, buf: []u8) !usize {
    const rc = system.read(fd, buf.ptr, buf.len);
    if (rc < 0) {
        return error.ReadError;
    }
    return @intCast(rc);
}

pub fn poll(fds: []pollfd, timeout: i32) !usize {
    const rc = system.poll(fds.ptr, fds.len, timeout);
    if (rc < 0) {
        return error.PollError;
    }
    return @intCast(rc);
}

pub const tcgetattr = std.posix.tcgetattr;

pub const tcsetattr = std.posix.tcsetattr;

pub const sigaction = std.posix.sigaction;

pub fn waitpid(pid: pid_t, options: i32) !pid_t {
    var status: i32 = 0;
    const rc = system.waitpid(pid, &status, options);
    if (rc < 0) {
        return error.WaitpidError;
    }
    return @intCast(rc);
}

pub fn exit(status: i32) noreturn {
    system.exit(status);
    unreachable;
}

pub fn unlink(path: [*:0]const u8) !void {
    const rc = system.unlink(path);
    if (rc < 0) {
        return error.UnlinkError;
    }
}
