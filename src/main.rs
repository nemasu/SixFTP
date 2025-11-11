// Hide console window on Windows for GUI mode only
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use anyhow::Result;
use clap::Parser;
use unftp_sbe_fs::ServerExt;
use std::net::IpAddr;
use std::path::PathBuf;
use tracing::{info, error};
use std::env;

#[cfg(windows)]
use windows::Win32::System::Console::{AllocConsole, AttachConsole, ATTACH_PARENT_PROCESS};

mod gui;
mod network_info;



/// A simple portable FTP server
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Directory to serve via FTP
    #[arg(short, long, default_value = ".")]
    directory: PathBuf,

    /// FTP username
    #[arg(short, long, default_value = "user")]
    username: String,

    /// FTP password
    #[arg(long, default_value = "password")]
    password: String,

    /// Main FTP port
    #[arg(short, long, default_value = "9000")]
    port: u16,

    /// Passive port range (format: start-end)
    #[arg(long, default_value = "30000-30100")]
    pasv_range: String,

    /// Bind address
    #[arg(short, long, default_value = "0.0.0.0")]
    bind: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Check if any command line arguments were provided
    let args: Vec<String> = env::args().collect();

    // If command-line arguments are provided (CLI mode), allocate/attach a console on Windows
    #[cfg(windows)]
    if args.len() > 1 {
        unsafe {
            // Try to attach to parent process console first (if launched from cmd/powershell)
            if AttachConsole(ATTACH_PARENT_PROCESS).is_err() {
                // If no parent console, allocate a new one
                let _ = AllocConsole();
            }
        }
    }

    // Initialize logging with custom configuration
    // If RUST_LOG=debug is set, show all logs
    // Otherwise, show our app logs and libunftp logs at INFO level
    use tracing_subscriber::{EnvFilter, fmt};

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| {
            // Default filter: show our app's logs and libunftp logs at INFO level and above,
            // suppress other library logs unless they are ERROR
            EnvFilter::new("sixftp=info,libunftp=info,error")
        });

    fmt()
        .with_env_filter(filter)
        .init();

    // If no arguments provided (only program name), launch GUI
    // If any arguments are present (including -h for help), use CLI
    if args.len() == 1 {
        return run_gui_mode().await;
    }

    // Run CLI mode
    run_cli_mode().await
}

async fn start_ftp_server(directory: &PathBuf, port: u16, bind_addr: &IpAddr, pasv_range: &std::ops::RangeInclusive<u16>) -> Result<Vec<IpAddr>> {
    let mut successful_bindings = Vec::new();
    let mut tasks = Vec::new();

    // If bind address is unspecified (0.0.0.0 or ::), bind to both IPv4 and IPv6
    if bind_addr.is_unspecified() {
        // Try IPv4
        let ipv4_bind = "0.0.0.0".parse::<IpAddr>().unwrap();
        let bind_string = format!("{}:{}", ipv4_bind, port);

        let server = libunftp::Server::with_fs(directory.clone())
            .passive_ports(pasv_range.clone())
            .passive_host(libunftp::options::PassiveHost::FromConnection)
            .greeting("Welcome to QuickFTP Server")
            .build()
            .unwrap();
        
        let task = tokio::spawn(async move {
            match server.listen(bind_string).await {
                Ok(_) => {
                    info!("FTP server stopped gracefully on IPv4");
                    Some(ipv4_bind)
                }
                Err(e) => {
                    error!("Failed to bind to IPv4 {}: {}", ipv4_bind, e);
                    None
                }
            }
        });
        tasks.push((task, ipv4_bind));

        // Try IPv6
        let ipv6_bind = "::".parse::<IpAddr>().unwrap();
        let bind_string = format!("[{}]:{}", ipv6_bind, port);

        let server = libunftp::Server::with_fs(directory.clone())
            .passive_ports(pasv_range.clone())
            .passive_host(libunftp::options::PassiveHost::FromConnection)
            .greeting("Welcome to SixFTP Server")
            .build()
            .unwrap();
        
        let task = tokio::spawn(async move {
            match server.listen(bind_string).await {
                Ok(_) => {
                    info!("FTP server stopped gracefully on IPv6");
                    Some(ipv6_bind)
                }
                Err(e) => {
                    error!("Failed to bind to IPv6 {}: {}", ipv6_bind, e);
                    None
                }
            }
        });
        tasks.push((task, ipv6_bind));
    } else {
        // Use the specified bind address
        let bind_string = if bind_addr.is_ipv6() {
            format!("[{}]:{}", bind_addr, port)
        } else {
            format!("{}:{}", bind_addr, port)
        };

        let server = libunftp::Server::with_fs(directory.clone())
            .passive_ports(pasv_range.clone())
            .passive_host(libunftp::options::PassiveHost::FromConnection)
            .greeting("Welcome to SixFTP Server")
            .build()
            .unwrap();
        
        let bind_addr_clone = *bind_addr;
        
        let task = tokio::spawn(async move {
            match server.listen(bind_string).await {
                Ok(_) => {
                    info!("FTP server stopped gracefully on {}", bind_addr_clone);
                    Some(bind_addr_clone)
                }
                Err(e) => {
                    error!("Failed to bind to {}: {}", bind_addr_clone, e);
                    None
                }
            }
        });
        tasks.push((task, *bind_addr));
    }

    // Wait a bit for bindings to succeed or fail
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Check which bindings succeeded
    for (task, addr) in tasks {
        if !task.is_finished() {
            // If the task is still running, the binding succeeded
            successful_bindings.push(addr);
            info!("Successfully bound to {}:{}", addr, port);
        }
    }

    if successful_bindings.is_empty() {
        return Err(anyhow::anyhow!("Failed to bind to any address on port {}", port));
    }

    info!("FTP server started successfully on {} address(es)", successful_bindings.len());

    Ok(successful_bindings)
}

fn parse_pasv_range(range_str: &str) -> Result<std::ops::RangeInclusive<u16>> {
    let parts: Vec<&str> = range_str.split('-').collect();
    if parts.len() != 2 {
        return Err(anyhow::anyhow!("Invalid passive port range format. Use 'start-end' (e.g., 30000-30010)"));
    }

    let start: u16 = parts[0].parse()?;
    let end: u16 = parts[1].parse()?;

    if start > end {
        return Err(anyhow::anyhow!("Passive port range start must be less than or equal to end"));
    }

    if end - start > 100 {
        return Err(anyhow::anyhow!("Passive port range too large. Maximum 100 ports allowed"));
    }

    Ok(start..=end)
}

fn display_server_info(successful_bindings: &[IpAddr], port: u16, pasv_range: &std::ops::RangeInclusive<u16>, directory: &PathBuf, username: &str, password: &str) {
    let server_info = network_info::ServerInfo {
        successful_bindings: successful_bindings.to_vec(),
        port,
        pasv_range: pasv_range.clone(),
        directory: directory.clone(),
        username: username.to_string(),
        password: password.to_string(),
    };
    
    println!("{}", server_info.format_display_info());
    println!("   Press Ctrl+C to stop the server\n");
}



/// Run the GUI version of the FTP server
async fn run_gui_mode() -> Result<()> {
    info!("Starting SixFTP GUI mode");
    
    // Run the GUI application
    if let Err(e) = gui::run_gui() {
        error!("GUI error: {}", e);
        return Err(anyhow::anyhow!("GUI failed to start: {}", e));
    }
    
    Ok(())
}

/// Run the CLI version of the FTP server
async fn run_cli_mode() -> Result<()> {
    info!("Starting SixFTP CLI mode");
    
    // Parse command line arguments for CLI mode
    let args = Args::parse();

    // Validate and parse passive port range
    let pasv_range = parse_pasv_range(&args.pasv_range)?;

    info!("Starting SixFTP server with passive port range: {} to {}", pasv_range.start(), pasv_range.end());

    // Validate directory exists
    if !args.directory.exists() {
        return Err(anyhow::anyhow!("Directory '{}' does not exist", args.directory.display()));
    }

    // Parse bind address (strip brackets from IPv6 addresses if present)
    let bind_address_cleaned = args.bind.trim()
        .trim_start_matches('[')
        .trim_end_matches(']');
    let bind_addr: IpAddr = bind_address_cleaned.parse()?;

    // Try to bind to all interfaces (IPv4 and IPv6)
    let successful_bindings = start_ftp_server(&args.directory, args.port, &bind_addr, &pasv_range).await?;

    // Display server information with successful bindings
    display_server_info(&successful_bindings, args.port, &pasv_range, &args.directory, &args.username, &args.password);

    // Wait for all servers to finish
    tokio::time::sleep(tokio::time::Duration::from_secs(u64::MAX)).await;

    Ok(())
}