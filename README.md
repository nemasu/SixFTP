# SixFTP - IPv6-Ready Portable FTP Server

A simple, portable FTP server written in Rust with full IPv6 support using the libunftp library.

## Features

- **Dual Interface Modes** - GUI & CLI mode
- **IPv6 & IPv4 dual-stack support** - perfect for IPoE/IPv6-only networks
- **Portable** - single executable with no dependencies
- **Configurable** - directory, username, password, ports, and bind address
- **Network interface detection** - automatically displays all available IP addresses
- **Passive mode support** - configurable passive port range

## Usage

### Dual Mode Operation

SixFTP automatically switches between GUI and CLI modes based on command line arguments:

- **GUI Mode** (no arguments): Launches a graphical interface
- **CLI Mode** (with arguments): Runs as a command-line server

### GUI Mode

```bash
# Launch the GUI interface (no arguments needed)
sixftp
```

The GUI provides:
- Visual configuration
- Easy start/stop controls
- Copy-paste friendly connection information

### CLI Mode

```bash
# Serve current directory with default settings (CLI mode)
sixftp -d .

# Serve a specific directory
sixftp -d /path/to/directory

# Custom username and password
sixftp -u admin --password secret

# Custom port
sixftp -p 21212

# Custom passive port range
sixftp --pasv-range 40000-40010

# Bind to specific address
sixftp -b 127.0.0.1
```

### Command Line Options

```
-d, --directory <DIRECTORY>    Directory to serve via FTP [default: .]
-u, --username <USERNAME>      FTP username [default: user]
    --password <PASSWORD>      FTP password [default: password]
-p, --port <PORT>              Main FTP port [default: 9000]
    --pasv-range <PASV_RANGE>  Passive port range (format: start-end) [default: 30000-30100]
-b, --bind <BIND>              Bind address [default: 0.0.0.0]
-h, --help                     Print help
-V, --version                  Print version
```

### Example: Full Custom Configuration (CLI Mode)

```bash
sixftp \
  -d /home/user/shared \
  -u admin \
  --password mypassword \
  -p 21212 \
  --pasv-range 40000-40020 \
  -b 0.0.0.0
```

## Building

### Prerequisites

- Rust 1.70.0 or higher
- Cargo

### Build Steps

```bash
# Clone the repository
git clone https://github.com/nemasu/SixFTP
cd SixFTP

# Build in debug mode
cargo build

# Build in release mode
cargo build --release

# The executable will be in target/debug/sixftp or target/release/sixftp
```

## Connecting to the Server

Once the server is running, you can connect using any FTP client:

### Using Command Line FTP Client

```bash
# Connect to localhost
ftp 127.0.0.1 21212

# Or connect to your local IP
ftp 192.168.1.100 21212
```

### Using GUI FTP Clients

- **FileZilla**: Use "Quickconnect" with the displayed IP and port (supports IPv6)
- **WinSCP**: Use FTP protocol with the displayed IP and port
- **Any other FTP client**: Use the connection information displayed when the server starts

### GUI Features

When running in GUI mode, you can:

1. **Configure Server Settings**: Set directory, credentials, ports, and bind address
2. **Start/Stop Server**: Control server operation with visual buttons
3. **View Connection Info**: See all connection details for easy copy-paste
4. **Status**: See server status and binding information

### Default Credentials

- **Username**: `user`
- **Password**: `password`

## Network Configuration

### Firewall Considerations

You may need to allow the application through the firewall:

1. Open Windows Defender Firewall
2. Click "Allow an app or feature through Windows Defender Firewall"
3. Add the SixFTP executable and allow it for both private and public networks

### Port Forwarding (For External Access)

If you want to access the server from outside your local network:

1. **Main FTP Port**: Forward the port specified with `-p` (default: 2121)
2. **Passive Ports**: Forward the entire range specified with `--pasv-range` (default: 30000-30010)

## Troubleshooting

### Common Issues

1. **"io error" when starting**
   - Try using a higher port number (above 1024)
   - Run as administrator if you need to use privileged ports
   - Check if the port is already in use

2. **Can't connect from other devices**
   - Try binding on IP used to access internet
   - Check firewall settings
   - Verify network connectivity

3. **Passive mode not working**
   - Ensure the passive port range is properly forwarded on your router
   - Try a different passive port range

### Logging

The server outputs structured logs with different levels:
- **INFO**: Server startup and normal operations
- **ERROR**: Error conditions
- **DEBUG**: Detailed debugging information (enable with `RUST_LOG=debug`)

## License

This project is licensed under the MIT License.

## Dependencies

- [libunftp](https://crates.io/crates/libunftp) - FTP server library
- [unftp-sbe-fs](https://crates.io/crates/unftp-sbe-fs) - Filesystem storage backend
- [tokio](https://crates.io/crates/tokio) - Async runtime
- [clap](https://crates.io/crates/clap) - Command line argument parsing
- [tracing](https://crates.io/crates/tracing) - Structured logging