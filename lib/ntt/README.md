# NTT Transparent Terminal
> Terminal is not an emulation anymore, everything else is!

```
███╗   ██╗████████╗████████╗
████╗  ██║╚══██╔══╝╚══██╔══╝
██╔██╗ ██║   ██║      ██║
██║╚██╗██║   ██║      ██║
██║ ╚████║   ██║      ██║
╚═╝  ╚═══╝   ╚═╝      ╚═╝
```

> A blazingly fast, modern terminal switcher built with Zig - Trash tmux and put your terminal full-screen on top of everything else!

[![GitHub release](https://img.shields.io/github/release/evgnomon/ntt.svg)](

[![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)]()
[![License](https://img.shields.io/badge/license-MIT-blue.svg)]()
[![Zig Version](https://img.shields.io/badge/zig-0.13.0-orange.svg)](https://ziglang.org/)

## Why NTT?

NTT trashes tmux. Tmux is bloat, it's time for a fresh approach. **NTT** (NTT Transparent Terminal or NTT trashes tmux) reimagines what a terminal multiplexer should be:

- **🚀 Performance First** - Written in Zig for maximum speed and minimal resource usage
- **🎨 Modern UX** - A short list of commands and that is all, call it everywhere!
- **🔄 Drop-in Compatible** - No-keybindings, just commands, zero wrong key pressing.
- **🪶 Zero Dependencies** - Single binary, no runtime dependencies.
- **🔍 Transparent** - Crystal clear session management and debugging without any sign of multiplexer.

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/evgnomon/ntt.git
cd ntt

# Build with Zig
zig build --release=fast

# Install to your PATH
sudo cp zig-out/bin/ntt /usr/local/bin/
```

### Binary Releases

Download the latest release for your platform from the [releases page](https://github.com/evgnomon/ntt/releases).

## Quick Start

```bash
# Start a new session
ntt # or CTRL+n after first run

# Switch to the next terminal round-robin
ntt next # or CTRL+b
```

## Key Bindings
No keybindings by design.

### Building

```bash
# Debug build
zig build

# Run tests
zig build test

# Run the binary
zig build run
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

This project is licensed under the HGL License

## Acknowledgments

- Built with [Zig](https://ziglang.org/) - a modern systems programming language
- Thanks to all contributors and early adopters

