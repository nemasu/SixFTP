use anyhow::Result;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

#[derive(Debug, Clone)]
pub struct NetworkIps {
    pub ipv4: Vec<Ipv4Addr>,
    pub ipv6: Vec<Ipv6Addr>,
}

pub struct ServerInfo {
    pub successful_bindings: Vec<IpAddr>,
    pub port: u16,
    pub pasv_range: std::ops::RangeInclusive<u16>,
    pub directory: std::path::PathBuf,
    pub username: String,
    pub password: String,
}

impl ServerInfo {
    pub fn format_display_info(&self) -> String {
        let mut info = String::new();

        info.push_str("SixFTP Server Started\n");
        info.push_str("==========================\n\n");

        // Show actual network addresses for clients to use
        if let Ok(network_ips) = get_network_ips() {
            if !network_ips.ipv4.is_empty() || !network_ips.ipv6.is_empty() {
                info.push_str("Available network addresses:\n");

                // Show IPv4 addresses
                for ip in &network_ips.ipv4 {
                    info.push_str(&format!(
                        "   - ftp://{}:{}@{}:{}\n",
                        self.username, self.password, ip, self.port
                    ));
                }

                // Show IPv6 addresses with temporary address detection
                for ip in &network_ips.ipv6 {
                    let segments = ip.segments();
                    let is_global = segments[0] >= 0x2000 && segments[0] <= 0x3FFF;
                    let is_unique_local = segments[0] >= 0xFC00 && segments[0] <= 0xFDFF;

                    if is_global {
                        if is_temporary_ipv6(ip) {
                            info.push_str(&format!(
                                "   - ftp://{}:{}@[{}]:{} (temporary)\n",
                                self.username, self.password, ip, self.port
                            ));
                        } else {
                            info.push_str(&format!(
                                "   - ftp://{}:{}@[{}]:{} (public)\n",
                                self.username, self.password, ip, self.port
                            ));
                        }
                    } else if is_unique_local {
                        info.push_str(&format!(
                            "   - ftp://{}:{}@[{}]:{} (private)\n",
                            self.username, self.password, ip, self.port
                        ));
                    } else {
                        info.push_str(&format!(
                            "   - ftp://{}:{}@[{}]:{}\n",
                            self.username, self.password, ip, self.port
                        ));
                    }
                }
            }
        }

        // Display successful listening addresses
        info.push_str("\nSuccessfully bound to:\n");

        for bind_addr in &self.successful_bindings {
            if bind_addr.is_ipv6() {
                info.push_str(&format!(
                    "   - ftp://{}:{}@[{}]:{}\n",
                    self.username, self.password, bind_addr, self.port
                ));
            } else {
                info.push_str(&format!(
                    "   - ftp://{}:{}@{}:{}\n",
                    self.username, self.password, bind_addr, self.port
                ));
            }
        }

        info.push_str(&format!(
            "\nServing directory: {}\n",
            self.directory.display()
        ));
        info.push_str(&format!("Username: {}\n", self.username));
        info.push_str(&format!("Password: {}\n", self.password));
        info.push_str(&format!(
            "Passive ports: {} to {}\n",
            self.pasv_range.start(),
            self.pasv_range.end()
        ));
        info.push_str("Make sure to forward the main and passive port range in your firewall/router if needed.\n");
        info.push_str("\nConnect using any FTP client with the displayed addresses\n");

        info
    }
}

pub fn get_network_ips() -> Result<NetworkIps> {
    let mut ipv4_ips = Vec::new();
    let mut ipv6_ips = Vec::new();

    // Add localhost addresses
    ipv4_ips.push(Ipv4Addr::new(127, 0, 0, 1));
    ipv6_ips.push(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1));

    // Try to get network interface IPs
    if let Ok(interfaces) = local_ip_address::list_afinet_netifas() {
        for (_, ip) in interfaces {
            match ip {
                IpAddr::V4(ipv4) => {
                    // Skip loopback and link-local addresses for public display
                    if !ipv4.is_loopback() && !ipv4.is_link_local() {
                        ipv4_ips.push(ipv4);
                    }
                }
                IpAddr::V6(ipv6) => {
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
    if ipv6_ips.len() <= 1 {
        // Only localhost
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

fn get_ipv6_interfaces() -> Result<Vec<Ipv6Addr>> {
    use std::net::UdpSocket;

    let mut ipv6_addresses = Vec::new();

    // Try to create a UDP socket to detect available IPv6 interfaces
    if let Ok(socket) = UdpSocket::bind("[::]:0") {
        // Get the local address of the socket
        if let Ok(local_addr) = socket.local_addr() {
            if let IpAddr::V6(ipv6) = local_addr.ip() {
                if !ipv6.is_loopback() && !ipv6.is_unspecified() {
                    ipv6_addresses.push(ipv6);
                }
            }
        }
    }

    // Also try to get IPv6 addresses from network interfaces
    if let Ok(interfaces) = local_ip_address::list_afinet_netifas() {
        for (_, ip) in interfaces {
            if let IpAddr::V6(ipv6) = ip {
                // Include global unicast (public) and unique local (private) IPv6 addresses
                let segments = ipv6.segments();
                let is_global = segments[0] >= 0x2000 && segments[0] <= 0x3FFF;
                let is_unique_local = segments[0] >= 0xFC00 && segments[0] <= 0xFDFF;
                let is_link_local = segments[0] >= 0xFE80 && segments[0] <= 0xFEBF;

                if (is_global || is_unique_local)
                    && !ipv6.is_loopback()
                    && !ipv6.is_unspecified()
                    && !is_link_local
                {
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
fn is_temporary_ipv6(ipv6: &Ipv6Addr) -> bool {
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
    let interface_id = ((segments[4] as u64) << 48)
        | ((segments[5] as u64) << 32)
        | ((segments[6] as u64) << 16)
        | (segments[7] as u64);

    // The universal/local bit is bit 6 (counting from 0) in the interface identifier
    // This corresponds to position 70 in the full 128-bit IPv6 address
    let universal_local_bit = (interface_id >> 57) & 0x1;

    // Temporary addresses have the universal/local bit set to 1
    universal_local_bit == 1
}
