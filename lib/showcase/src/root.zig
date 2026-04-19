//! By convention, root.zig is the root source file when making a package.
const std = @import("std");

pub const manifest = @cImport({
    @cInclude("main.h");
});

pub const cpp = @cImport({
    @cInclude("main.hh");
});
