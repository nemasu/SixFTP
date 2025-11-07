use anyhow::Result;
use clap::Parser;
use unftp_sbe_fs::ServerExt;
use std::net::IpAddr;
use std::path::PathBuf;
use tracing::{info, error};

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
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Parse command line arguments
    let args = Args::parse();

    // Validate and parse passive port range
    let pasv_range = parse_pasv_range(&args.pasv_range)?;

    info!("Starting SixFTP server with passive port range: {} to {}", pasv_range.start(), pasv_range.end());

    // Validate directory exists
    if !args.directory.exists() {
        return Err(anyhow::anyhow!("Directory '{}' does not exist", args.directory.display()));
    }

    // Parse bind address
    let bind_addr: IpAddr = args.bind.parse()?;

    // Try to bind to all interfaces (IPv4 and IPv6)
    let successful_bindings = start_ftp_server(&args.directory, args.port, &bind_addr, &pasv_range).await?;

    // Display server information with successful bindings
    display_server_info(&successful_bindings, args.port, &pasv_range, &args.directory, &args.username, &args.password);

    // Wait for all servers to finish
    tokio::time::sleep(tokio::time::Duration::from_secs(u64::MAX)).await;

    Ok(())
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
    println!("SixFTP Server Started");
    println!("==========================");
    
    // Display successful listening addresses
    println!("📡 Successfully bound to:");
    
    for bind_addr in successful_bindings {
        if bind_addr.is_ipv6() {
            println!("   - ftp://{}:{}@[{}]:{}", username, password, bind_addr, port);
        } else {
            println!("   - ftp://{}:{}@{}:{}", username, password, bind_addr, port);
        }
    }
    
    // Show actual network addresses for clients to use
    if let Ok(network_ips) = get_network_ips() {
        if !network_ips.ipv4.is_empty() || !network_ips.ipv6.is_empty() {
            println!("\n🌐 Available network addresses:");
            
            // Show IPv4 addresses
            for ip in &network_ips.ipv4 {
                println!("   - ftp://{}:{}@{}:{}", username, password, ip, port);
            }
            
            // Show IPv6 addresses with temporary address detection
            for ip in &network_ips.ipv6 {
                let segments = ip.segments();
                let is_global = segments[0] >= 0x2000 && segments[0] <= 0x3FFF;
                let is_unique_local = segments[0] >= 0xFC00 && segments[0] <= 0xFDFF;
                
                if is_global {
                    if is_temporary_ipv6(ip) {
                        println!("   - ftp://{}:{}@[{}]:{} (temporary)", username, password, ip, port);
                    } else {
                        println!("   - ftp://{}:{}@[{}]:{} (public)", username, password, ip, port);
                    }
                } else if is_unique_local {
                    println!("   - ftp://{}:{}@[{}]:{} (private)", username, password, ip, port);
                } else {
                    println!("   - ftp://{}:{}@[{}]:{}", username, password, ip, port);
                }
            }
        }
    }
    
    println!("\n📁 Serving directory: {}", directory.display());
    println!("👤 Username: {}", username);
    println!("🔑 Password: {}", password);
    println!("🔒 Passive ports: {} to {}", pasv_range.start(), pasv_range.end());
    println!("ℹ️  Make sure to forward the main and passive port range in your firewall/router if needed.");
    println!("\n💡 Connect using any FTP client with the displayed addresses");
    println!("   Press Ctrl+C to stop the server\n");
}

struct NetworkIps {
    ipv4: Vec<std::net::Ipv4Addr>,
    ipv6: Vec<std::net::Ipv6Addr>,
}

fn get_network_ips() -> Result<NetworkIps> {
    let mut ipv4_ips = Vec::new();
    let mut ipv6_ips = Vec::new();
    
    // Add localhost addresses
    ipv4_ips.push(std::net::Ipv4Addr::new(127, 0, 0, 1));
    ipv6_ips.push(std::net::Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1));
    
    // Try to get network interface IPs
    if let Ok(interfaces) = local_ip_address::list_afinet_netifas() {
        for (_, ip) in interfaces {
            match ip {
                std::net::IpAddr::V4(ipv4) => {
                    // Skip loopback and link-local addresses for public display
                    if !ipv4.is_loopback() && !ipv4.is_link_local() {
                        ipv4_ips.push(ipv4);
                    }
                }
                std::net::IpAddr::V6(ipv6) => {
                    // For IPv6, we want to show:
                    // - Global unicast addresses (public IPv6) - starts with 2000::/3
                    // - Unique local addresses (private IPv6) - starts with fc00::/7
                    // Skip link-local (fe80::/10) and loopback
                    if !ipv6.is_loopback() && !ipv6.is_unspecified() {
                        let segments = ipv6.segments();
                        // Check for global unicast (2000::/3)
                        let is_global = segments[0] >= 0x2000 && segments[0] <= 0x3FFF;
                        // Check for unique local (fc00::/7) 
                        let is_unique_local = segments[0] >= 0xFC00 && segments[0] <= 0xFDFF;
                        // Check for link-local (fe80::/10)
                        let is_link_local = segments[0] >= 0xFE80 && segments[0] <= 0xFEBF;
                        
                        if (is_global || is_unique_local) && !is_link_local {
                            ipv6_ips.push(ipv6);
                        }
                    }
                }
            }
        }
    }
    
    // If no IPv6 addresses found, try to get them from system interfaces
    if ipv6_ips.len() <= 1 { // Only localhost
        if let Ok(interfaces) = get_ipv6_interfaces() {
            for ipv6 in interfaces {
                if !ipv6_ips.contains(&ipv6) {
                    ipv6_ips.push(ipv6);
                }
            }
        }
    }
    
    Ok(NetworkIps {
        ipv4: ipv4_ips,
        ipv6: ipv6_ips,
    })
}

fn get_ipv6_interfaces() -> Result<Vec<std::net::Ipv6Addr>> {
    use std::net::UdpSocket;
    
    let mut ipv6_addresses = Vec::new();
    
    // Try to create a UDP socket to detect available IPv6 interfaces
    if let Ok(socket) = UdpSocket::bind("[::]:0") {
        // Get the local address of the socket
        if let Ok(local_addr) = socket.local_addr() {
            if let std::net::IpAddr::V6(ipv6) = local_addr.ip() {
                if !ipv6.is_loopback() && !ipv6.is_unspecified() {
                    ipv6_addresses.push(ipv6);
                }
            }
        }
    }
    
    // Also try to get IPv6 addresses from network interfaces
    if let Ok(interfaces) = local_ip_address::list_afinet_netifas() {
        for (_, ip) in interfaces {
            if let std::net::IpAddr::V6(ipv6) = ip {
                // Include global unicast (public) and unique local (private) IPv6 addresses
                let segments = ipv6.segments();
                let is_global = segments[0] >= 0x2000 && segments[0] <= 0x3FFF;
                let is_unique_local = segments[0] >= 0xFC00 && segments[0] <= 0xFDFF;
                let is_link_local = segments[0] >= 0xFE80 && segments[0] <= 0xFEBF;
                
                if (is_global || is_unique_local) && 
                   !ipv6.is_loopback() && !ipv6.is_unspecified() && !is_link_local {
                    if !ipv6_addresses.contains(&ipv6) {
                        ipv6_addresses.push(ipv6);
                    }
                }
            }
        }
    }
    
    Ok(ipv6_addresses)
}

/// Check if an IPv6 address is a temporary address (privacy extension)
/// Temporary addresses have the universal/local bit (bit 6) set to 1
/// This indicates they were generated by privacy extensions rather than from MAC addresses
fn is_temporary_ipv6(ipv6: &std::net::Ipv6Addr) -> bool {
    let segments = ipv6.segments();
    
    // For IPv6 addresses, the interface identifier is the last 64 bits
    // The universal/local bit is bit 6 (counting from 0) in the interface identifier
    // In the last segment (segments[7]), this is bit 6 of the 16-bit value
    
    // Check if this is a global unicast address (starts with 2000::/3)
    let is_global_unicast = segments[0] >= 0x2000 && segments[0] <= 0x3FFF;
    
    if !is_global_unicast {
        return false;
    }
    
    // Extract the interface identifier (last 64 bits)
    let interface_id = ((segments[4] as u64) << 48) |
                      ((segments[5] as u64) << 32) |
                      ((segments[6] as u64) << 16) |
                      (segments[7] as u64);
    
    // The universal/local bit is bit 6 (counting from 0) in the interface identifier
    // This corresponds to position 70 in the full 128-bit IPv6 address
    let universal_local_bit = (interface_id >> 57) & 0x1;
    
    // Temporary addresses have the universal/local bit set to 1
    universal_local_bit == 1
}