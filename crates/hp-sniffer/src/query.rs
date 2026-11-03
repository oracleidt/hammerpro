use std::net::{ToSocketAddrs, UdpSocket};
use std::time::Duration;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerInfo {
    pub name: String,
    pub map: String,
    pub folder: String,
    pub game: String,
    pub players: u8,
    pub max_players: u8,
    pub vac: bool,
    pub version: String,
}

const REQUEST: &[u8] = b"\xff\xff\xff\xffTSource Engine Query\0";

fn read_cstr(data: &[u8], at: &mut usize) -> Option<String> {
    let rest = data.get(*at..)?;
    let end = rest.iter().position(|b| *b == 0)?;
    let s = String::from_utf8_lossy(&rest[..end]).into_owned();
    *at += end + 1;
    Some(s)
}

pub fn parse_info(packet: &[u8]) -> Option<ServerInfo> {
    if packet.len() < 6 || packet[..4] != [0xff; 4] || packet[4] != b'I' {
        return None;
    }
    let mut at = 6;
    let name = read_cstr(packet, &mut at)?;
    let map = read_cstr(packet, &mut at)?;
    let folder = read_cstr(packet, &mut at)?;
    let game = read_cstr(packet, &mut at)?;
    at += 2;
    let players = *packet.get(at)?;
    let max_players = *packet.get(at + 1)?;
    at += 6;
    let vac = *packet.get(at)? == 1;
    at += 1;
    let version = read_cstr(packet, &mut at).unwrap_or_default();
    Some(ServerInfo { name, map, folder, game, players, max_players, vac, version })
}

pub fn is_challenge(packet: &[u8]) -> Option<[u8; 4]> {
    if packet.len() >= 9 && packet[..4] == [0xff; 4] && packet[4] == b'A' {
        return Some([packet[5], packet[6], packet[7], packet[8]]);
    }
    None
}

pub fn query(host: &str, port: u16, timeout: Duration) -> Result<ServerInfo, String> {
    let addr = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("cannot resolve {host}: {e}"))?
        .find(|a| a.is_ipv4())
        .ok_or_else(|| format!("no IPv4 address for {host}"))?;
    let sock = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    sock.set_read_timeout(Some(timeout)).map_err(|e| e.to_string())?;
    sock.connect(addr).map_err(|e| e.to_string())?;
    sock.send(REQUEST).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 4096];
    for _ in 0..3 {
        let n = sock.recv(&mut buf).map_err(|e| format!("the server did not answer: {e}"))?;
        let packet = &buf[..n];
        if let Some(challenge) = is_challenge(packet) {
            let mut again = REQUEST.to_vec();
            again.extend_from_slice(&challenge);
            sock.send(&again).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(info) = parse_info(packet) {
            return Ok(info);
        }
        return Err("the server answered with something that is not a Source query reply".to_owned());
    }
    Err("the server kept asking for a challenge".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        let mut p = vec![0xff, 0xff, 0xff, 0xff, b'I', 17];
        for s in ["My Server", "rp_rockford_v2b", "garrysmod", "DarkRP"] {
            p.extend_from_slice(s.as_bytes());
            p.push(0);
        }
        p.extend_from_slice(&[0xa0, 0x0f]);
        p.extend_from_slice(&[43, 128, 0]);
        p.extend_from_slice(&[b'd', b'l', 0, 1]);
        p.extend_from_slice(b"2026.04.29\0");
        p
    }

    #[test]
    fn a_reply_is_read() {
        let info = parse_info(&sample()).expect("info");
        assert_eq!(info.name, "My Server");
        assert_eq!(info.map, "rp_rockford_v2b");
        assert_eq!(info.game, "DarkRP");
        assert_eq!(info.players, 43);
        assert_eq!(info.max_players, 128);
        assert!(info.vac);
        assert_eq!(info.version, "2026.04.29");
    }

    #[test]
    fn junk_is_refused() {
        assert!(parse_info(b"").is_none());
        assert!(parse_info(&[0xff, 0xff, 0xff, 0xff, b'I']).is_none());
        let mut p = sample();
        p.truncate(12);
        assert!(parse_info(&p).is_none());
        assert!(parse_info(b"hello world, not a packet").is_none());
    }

    #[test]
    fn challenges_are_recognised() {
        let c = [0xff, 0xff, 0xff, 0xff, b'A', 1, 2, 3, 4];
        assert_eq!(is_challenge(&c), Some([1, 2, 3, 4]));
        assert_eq!(is_challenge(&sample()), None);
    }
}
