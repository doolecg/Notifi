//! Port manager (spec/apps/port-manager.md): listening TCP sockets and bound UDP sockets with their owners.
//! Windows reads the IP helper tables; Linux reads /proc/net.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use serde::Serialize;
use sysinfo::{Pid, Users};
use tauri::AppHandle;

use super::process;

#[derive(Debug, Clone, PartialEq)]
pub struct Raw {
    pub proto: &'static str,
    pub addr: IpAddr,
    pub port: u16,
    pub pid: Option<u32>,
    pub uid: Option<u32>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Socket {
    pub proto: &'static str,
    pub address: String,
    pub port: u16,
    pub pid: Option<u32>,
    pub process: String,
    pub user: String,
    /// Not loopback-only: other machines may be able to reach it.
    pub exposed: bool,
    /// Bound to every interface (0.0.0.0 or ::).
    pub all_interfaces: bool,
    pub start_time: u64,
    pub protected: Option<&'static str>,
}

fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn v4(b: &[u8], at: usize) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(b[at], b[at + 1], b[at + 2], b[at + 3]))
}

fn v6(b: &[u8], at: usize) -> IpAddr {
    let mut octets = [0u8; 16];
    octets.copy_from_slice(&b[at..at + 16]);
    IpAddr::V6(Ipv6Addr::from(octets))
}

/// The port DWORD of an IP helper row holds the port in network order in its low 16 bits.
fn port(b: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([b[at], b[at + 1]])
}

fn rows(buf: &[u8], size: usize, mut each: impl FnMut(&[u8])) {
    if buf.len() < 4 {
        return;
    }
    let count = le32(buf, 0) as usize;
    for i in 0..count {
        let start = 4 + i * size;
        match buf.get(start..start + size) {
            Some(row) => each(row),
            None => break,
        }
    }
}

/// MIB_TCPTABLE_OWNER_PID, listener table.
pub fn decode_tcp4(buf: &[u8]) -> Vec<Raw> {
    let mut out = Vec::new();
    rows(buf, 24, |r| out.push(Raw { proto: "TCP", addr: v4(r, 4), port: port(r, 8), pid: Some(le32(r, 20)), uid: None }));
    out
}

/// MIB_TCP6TABLE_OWNER_PID, listener table.
pub fn decode_tcp6(buf: &[u8]) -> Vec<Raw> {
    let mut out = Vec::new();
    rows(buf, 56, |r| out.push(Raw { proto: "TCP", addr: v6(r, 0), port: port(r, 20), pid: Some(le32(r, 52)), uid: None }));
    out
}

pub fn decode_udp4(buf: &[u8]) -> Vec<Raw> {
    let mut out = Vec::new();
    rows(buf, 12, |r| out.push(Raw { proto: "UDP", addr: v4(r, 0), port: port(r, 4), pid: Some(le32(r, 8)), uid: None }));
    out
}

pub fn decode_udp6(buf: &[u8]) -> Vec<Raw> {
    let mut out = Vec::new();
    rows(buf, 28, |r| out.push(Raw { proto: "UDP", addr: v6(r, 0), port: port(r, 20), pid: Some(le32(r, 24)), uid: None }));
    out
}

#[cfg(any(unix, test))]
fn hex_addr(hex: &str) -> Option<IpAddr> {
    match hex.len() {
        8 => Some(IpAddr::V4(Ipv4Addr::from(u32::from_str_radix(hex, 16).ok()?.swap_bytes()))),
        32 => {
            let mut octets = [0u8; 16];
            for (i, chunk) in octets.chunks_mut(4).enumerate() {
                let word = u32::from_str_radix(&hex[i * 8..i * 8 + 8], 16).ok()?;
                chunk.copy_from_slice(&word.to_le_bytes());
            }
            Some(IpAddr::V6(Ipv6Addr::from(octets)))
        }
        _ => None,
    }
}

#[cfg(any(unix, test))]
/// One of /proc/net/{tcp,tcp6,udp,udp6}. Returns each socket with its inode.
pub fn parse_proc_net(text: &str, proto: &'static str) -> Vec<(Raw, u64)> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() < 10 {
                return None;
            }
            let (host, port_hex) = f[1].rsplit_once(':')?;
            let (_, remote_port) = f[2].rsplit_once(':')?;
            let listening = if proto == "TCP" { f[3] == "0A" } else { remote_port == "0000" };
            if !listening {
                return None;
            }
            let inode: u64 = f[9].parse().ok()?;
            Some((Raw { proto, addr: hex_addr(host)?, port: u16::from_str_radix(port_hex, 16).ok()?, pid: None, uid: f[7].parse().ok() }, inode))
        })
        .collect()
}

#[cfg(windows)]
fn read_table(fetch: impl Fn(*mut core::ffi::c_void, &mut u32) -> u32) -> Vec<u8> {
    let mut size = 0u32;
    let mut buf: Vec<u8> = Vec::new();
    for _ in 0..6 {
        let ptr = if buf.is_empty() { std::ptr::null_mut() } else { buf.as_mut_ptr().cast() };
        match fetch(ptr, &mut size) {
            0 => return buf,
            122 => buf = vec![0u8; size as usize + 512],
            _ => return Vec::new(),
        }
    }
    Vec::new()
}

#[cfg(windows)]
fn collect(udp: bool) -> Vec<Raw> {
    use windows::Win32::NetworkManagement::IpHelper::{GetExtendedTcpTable, GetExtendedUdpTable, TCP_TABLE_OWNER_PID_LISTENER, UDP_TABLE_OWNER_PID};
    const AF_INET: u32 = 2;
    const AF_INET6: u32 = 23;
    let mut out = Vec::new();
    for family in [AF_INET, AF_INET6] {
        let buf = if udp {
            read_table(|p, size| unsafe { GetExtendedUdpTable(Some(p).filter(|p| !p.is_null()), size, false, family, UDP_TABLE_OWNER_PID, 0) })
        } else {
            read_table(|p, size| unsafe { GetExtendedTcpTable(Some(p).filter(|p| !p.is_null()), size, false, family, TCP_TABLE_OWNER_PID_LISTENER, 0) })
        };
        out.extend(match (udp, family) {
            (false, AF_INET) => decode_tcp4(&buf),
            (false, _) => decode_tcp6(&buf),
            (true, AF_INET) => decode_udp4(&buf),
            (true, _) => decode_udp6(&buf),
        });
    }
    out
}

#[cfg(unix)]
fn socket_owners() -> std::collections::HashMap<u64, u32> {
    let mut map = std::collections::HashMap::new();
    let Ok(procs) = std::fs::read_dir("/proc") else { return map };
    for entry in procs.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
        let Ok(fds) = std::fs::read_dir(entry.path().join("fd")) else { continue };
        for fd in fds.flatten() {
            if let Ok(link) = std::fs::read_link(fd.path()) {
                if let Some(inode) = link.to_string_lossy().strip_prefix("socket:[").and_then(|s| s.strip_suffix(']')).and_then(|s| s.parse().ok()) {
                    map.insert(inode, pid);
                }
            }
        }
    }
    map
}

#[cfg(unix)]
fn collect_all() -> Vec<Raw> {
    let owners = socket_owners();
    let mut out = Vec::new();
    for (file, proto) in [("tcp", "TCP"), ("tcp6", "TCP"), ("udp", "UDP"), ("udp6", "UDP")] {
        let Ok(text) = std::fs::read_to_string(format!("/proc/net/{file}")) else { continue };
        for (mut raw, inode) in parse_proc_net(&text, proto) {
            raw.pid = owners.get(&inode).copied();
            out.push(raw);
        }
    }
    out
}

#[cfg(windows)]
fn collect_all() -> Vec<Raw> {
    let mut all = collect(false);
    all.extend(collect(true));
    all
}

#[cfg(unix)]
fn passwd_name(uid: u32) -> String {
    std::fs::read_to_string("/etc/passwd")
        .ok()
        .and_then(|t| t.lines().find_map(|l| {
            let f: Vec<&str> = l.split(':').collect();
            (f.get(2).and_then(|u| u.parse::<u32>().ok()) == Some(uid)).then(|| f[0].to_string())
        }))
        .unwrap_or_else(|| uid.to_string())
}

fn build(raws: Vec<Raw>, include_udp: bool, all_only: bool) -> Vec<Socket> {
    let sys = process::snapshot();
    let users = Users::new_with_refreshed_list();
    let mut out: Vec<Socket> = raws
        .into_iter()
        .filter(|r| include_udp || r.proto == "TCP")
        .map(|r| {
            let proc = r.pid.filter(|p| *p != 0).and_then(|p| sys.process(Pid::from_u32(p)));
            let name = proc.map(|p| p.name().to_string_lossy().into_owned()).unwrap_or_default();
            #[cfg(unix)]
            let user = r.uid.map(passwd_name).unwrap_or_default();
            #[cfg(not(unix))]
            let user = proc.map(|p| process::user_of(&users, p)).unwrap_or_default();
            let protected = proc.and_then(|p| process::protected_reason(&name, p.pid().as_u32(), p.parent().map(Pid::as_u32)));
            Socket {
                proto: r.proto,
                address: r.addr.to_string(),
                port: r.port,
                pid: proc.map(|p| p.pid().as_u32()),
                process: name,
                user,
                exposed: !r.addr.is_loopback(),
                all_interfaces: r.addr.is_unspecified(),
                start_time: proc.map(|p| p.start_time()).unwrap_or(0),
                protected,
            }
        })
        .filter(|s| !all_only || s.all_interfaces)
        .collect();
    out.sort_by(|a, b| a.port.cmp(&b.port).then(a.proto.cmp(b.proto)).then(a.address.cmp(&b.address)));
    out.dedup_by(|a, b| a.port == b.port && a.proto == b.proto && a.address == b.address && a.pid == b.pid);
    let _ = &users;
    out
}

#[tauri::command]
pub async fn apps_ports(app: AppHandle) -> Result<Vec<Socket>, String> {
    let udp = super::flag(&app, "portManager", "showUdp", true);
    let all_only = super::flag(&app, "portManager", "allInterfacesOnly", false);
    tauri::async_runtime::spawn_blocking(move || build(collect_all(), udp, all_only)).await.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dword_port(p: u16) -> [u8; 4] {
        let be = p.to_be_bytes();
        [be[0], be[1], 0, 0]
    }

    #[test]
    fn decodes_ipv4_tcp_rows() {
        let mut buf = 2u32.to_le_bytes().to_vec();
        for (addr, port, pid) in [([0u8, 0, 0, 0], 8080u16, 4321u32), ([127, 0, 0, 1], 5432, 99)] {
            buf.extend(2u32.to_le_bytes());
            buf.extend(addr);
            buf.extend(dword_port(port));
            buf.extend([0u8; 8]);
            buf.extend(pid.to_le_bytes());
        }
        let raws = decode_tcp4(&buf);
        assert_eq!(raws.len(), 2);
        assert_eq!((raws[0].addr, raws[0].port, raws[0].pid), (IpAddr::V4(Ipv4Addr::UNSPECIFIED), 8080, Some(4321)));
        assert_eq!((raws[1].addr, raws[1].port), (IpAddr::V4(Ipv4Addr::LOCALHOST), 5432));
    }

    #[test]
    fn decodes_ipv6_tcp_and_udp_rows() {
        let mut tcp = 1u32.to_le_bytes().to_vec();
        tcp.extend(Ipv6Addr::LOCALHOST.octets());
        tcp.extend(0u32.to_le_bytes());
        tcp.extend(dword_port(3000));
        tcp.extend([0u8; 28]);
        tcp.extend(7u32.to_le_bytes());
        let raws = decode_tcp6(&tcp);
        assert_eq!((raws[0].addr, raws[0].port, raws[0].pid), (IpAddr::V6(Ipv6Addr::LOCALHOST), 3000, Some(7)));

        let mut udp = 1u32.to_le_bytes().to_vec();
        udp.extend([0u8; 4]);
        udp.extend(dword_port(5353));
        udp.extend(88u32.to_le_bytes());
        let raws = decode_udp4(&udp);
        assert_eq!((raws[0].proto, raws[0].port, raws[0].pid), ("UDP", 5353, Some(88)));
    }

    #[test]
    fn short_or_empty_tables_are_safe() {
        assert!(decode_tcp4(&[]).is_empty());
        let mut buf = 5u32.to_le_bytes().to_vec();
        buf.extend([0u8; 30]);
        assert_eq!(decode_tcp4(&buf).len(), 1);
    }

    #[test]
    fn parses_proc_net_listeners() {
        let tcp = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
   0: 0100007F:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 12345 1 0000000000000000 100 0 0 10 0\n\
   1: 00000000:0016 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 222 1 0000000000000000 100 0 0 10 0\n\
   2: 0100007F:D2F0 0100007F:1F90 01 00000000:00000000 00:00000000 00000000  1000        0 333 1 0000000000000000 100 0 0 10 0\n";
        let rows = parse_proc_net(tcp, "TCP");
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].0.addr, rows[0].0.port, rows[0].0.uid, rows[0].1), (IpAddr::V4(Ipv4Addr::LOCALHOST), 8080, Some(1000), 12345));
        assert!(rows[1].0.addr.is_unspecified());
        let tcp6 = "  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
   0: 00000000000000000000000001000000:0050 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 4444 1 0\n";
        let rows = parse_proc_net(tcp6, "TCP");
        assert_eq!((rows[0].0.addr, rows[0].0.port), (IpAddr::V6(Ipv6Addr::LOCALHOST), 80));
    }

    #[test]
    fn udp_keeps_unconnected_sockets_only() {
        let udp = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode ref pointer drops\n\
  10: 00000000:14E9 00000000:0000 07 00000000:00000000 00:00000000 00000000   101        0 5555 2 0000000000000000 0\n\
  11: 0100007F:0044 0100007F:0035 01 00000000:00000000 00:00000000 00000000   101        0 6666 2 0000000000000000 0\n";
        let rows = parse_proc_net(udp, "UDP");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0.port, 5353);
    }
}
