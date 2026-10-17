//! Minimal protocol-7 client exclusively for the terminal chat example.
//! Based on openOMSI's original v7 HELLO, WELCOME, INFO, CHAT and empty STATE
//! layouts (openOMSI-org/openOMSI at c1e52786d5d3).
//! Do not use this as a general replacement for current-game LanSession.

use super::{stdin_lines, valid_field, TICK};
use omsi_net::{self, JoinTarget, Pose, MAX_CHAT};
use std::collections::VecDeque;
use std::io::ErrorKind;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

const VERSION: u8 = 7;
const JOIN_TIMEOUT: Duration = Duration::from_secs(20);
const HOST_TIMEOUT: Duration = Duration::from_secs(20);
const CHAT_DELAY: Duration = Duration::from_millis(1100);

fn hello(name: &str, session: Option<u64>, nonce: u64) -> String {
    let session = session.map(omsi_net::session_hex).unwrap_or_else(|| "-".into());
    // HELLO|protocol|session|name|bus|map|date|time|weather|season|nonce
    format!("HELLO|{VERSION}|{session}|{name}|||0.00|||{nonce:016X}")
}

fn info(id: u32, name: &str) -> String {
    // INFO|id|name|bus|paint|line|destination|length|width|offset|table|tour|texts|figure
    format!("INFO|{id}|{name}|||||0.00|0.00|0.00|00000000|||")
}

fn heartbeat(id: u32, seq: u16, elapsed_ms: u32) -> Vec<u8> {
    // v7 and current wire.rs have the same encoding for a vehicleless pose.
    omsi_net::wire::encode_state(&Pose { id, sent_ms: elapsed_ms, ..Default::default() }, VERSION, seq)
}

fn welcome_id(fields: &[&str], expected_session: Option<u64>) -> Option<u32> {
    if fields.first().copied() != Some("WELCOME") || fields.get(1).copied() != Some("7") {
        return None;
    }
    let id = fields.get(2)?.parse::<u32>().ok()?;
    if !(2..=u16::MAX as u32).contains(&id) {
        return None;
    }
    if let Some(expected) = expected_session {
        if u64::from_str_radix(fields.get(3)?, 16).ok()? != expected {
            return None;
        }
    }
    Some(id)
}

fn targets(target: &str) -> Result<(Vec<SocketAddr>, Option<u64>, Option<omsi_net::ws::WsClient>), String> {
    let resolved = omsi_net::official::resolve_target(target)?;
    if let Some(url) = omsi_net::ws::ws_url(&resolved) {
        let ws = omsi_net::ws::WsClient::connect(&url)?;
        return Ok((vec![ws.local], None, Some(ws)));
    }
    match omsi_net::parse_join(&resolved)? {
        JoinTarget::Discover => Err("protocol 7 needs a server address, session code or WebSocket URL".into()),
        JoinTarget::Direct { addrs, session, protocol } => {
            if protocol.is_some_and(|p| p != VERSION) {
                return Err(format!("session code belongs to protocol {}, not 7", protocol.unwrap_or(0)));
            }
            if addrs.is_empty() {
                return Err("no addresses were provided".into());
            }
            Ok((addrs, session, None))
        }
    }
}

fn send(socket: &UdpSocket, destination: SocketAddr, text: &str) -> Result<(), String> {
    socket.send_to(text.as_bytes(), destination).map_err(|e| e.to_string())?;
    Ok(())
}

pub(super) fn run(target: &str, name: &str, once: Option<String>) -> Result<(), String> {
    let (addresses, requested_session, ws) = targets(target)?;
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| format!("UDP socket: {e}"))?;
    socket.set_nonblocking(true).map_err(|e| e.to_string())?;
    let start = Instant::now();
    let nonce = omsi_net::random_session_id() ^ ((std::process::id() as u64) << 48);
    let greeting = hello(name, requested_session, nonce);
    let input = once.is_none().then(stdin_lines);
    let single = input.is_none();
    let mut queue = VecDeque::<String>::new();
    if let Some(text) = once {
        queue.push_back(text);
    }
    let mut server = None::<SocketAddr>;
    let mut player_id = 0u32;
    let mut last_hello = None::<Instant>;
    let mut last_host = start;
    let mut last_info = None::<Instant>;
    let mut last_heartbeat = None::<Instant>;
    let mut last_chat = None::<Instant>;
    let mut one_shot_sent = None::<Instant>;
    let mut sequence = 0u16;
    let ws_reconnects = ws.as_ref().map(|w| w.reconnects()).unwrap_or(0);
    let mut buffer = [0u8; 1400];

    println!("Joining {target} as {name} (protocol 7, chat only).");
    if !single {
        println!("Type a message, or :quit to leave.");
    }

    let result = 'session: loop {
        if ws.as_ref().is_some_and(|w| w.reconnects() != ws_reconnects) {
            // A returning v7 peer needs an admission rebind proof. Require a fresh
            // session rather than bypassing the original challenge.
            break 'session Err("WebSocket reconnected; restart chat_only for a fresh protocol-7 session".into());
        }
        if server.is_none() && start.elapsed() >= JOIN_TIMEOUT {
            break 'session Err("no protocol-7 WELCOME received within 20 seconds".into());
        }
        if server.is_some() && last_host.elapsed() >= HOST_TIMEOUT {
            break 'session Err("server stopped responding".into());
        }

        if server.is_none() && last_hello.is_none_or(|when| when.elapsed() >= Duration::from_secs(1)) {
            for &address in &addresses {
                if let Err(error) = send(&socket, address, &greeting) {
                    eprintln!("HELLO to {address}: {error}");
                }
            }
            last_hello = Some(Instant::now());
        }

        loop {
            let (len, from) = match socket.recv_from(&mut buffer) {
                Ok(v) => v,
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if matches!(e.kind(), ErrorKind::ConnectionReset | ErrorKind::ConnectionRefused) => break,
                Err(e) => break 'session Err(e.to_string()),
            };
            if !server.map_or(addresses.contains(&from), |s| s == from) || len == 0 {
                continue;
            }
            // This client reads only text chat/control frames, not gameplay frames.
            let Ok(line) = std::str::from_utf8(&buffer[..len]) else {
                continue;
            };
            let parts: Vec<&str> = line.split('|').collect();
            match parts[0] {
                "WELCOME" if server.is_none() => {
                    if let Some(id) = welcome_id(&parts, requested_session) {
                        server = Some(from);
                        player_id = id;
                        last_host = Instant::now();
                        println!("* Connected as {name} (player {id}). No map or bus loaded.");
                    }
                }
                "WELCOME" if server == Some(from) => {
                    if welcome_id(&parts, requested_session) == Some(player_id) {
                        last_host = Instant::now();
                    }
                }
                "REJECT" | "KICK" => {
                    let reason = if parts[0] == "REJECT" {
                        parts.get(2).copied().unwrap_or("connection rejected")
                    } else {
                        parts.get(1).copied().unwrap_or("removed from server")
                    };
                    break 'session Err(format!("host refused admission: {reason}"));
                }
                "SAY" if server.is_some() => {
                    last_host = Instant::now();
                    if let (Some(author), Some(text)) = (parts.get(2), parts.get(3)) {
                        println!("<{author}> {text}");
                    }
                }
                "NOTE" if server.is_some() => {
                    last_host = Instant::now();
                    println!("* {}", parts.get(1).copied().unwrap_or(""));
                }
                "CLOCK" if server.is_some() => last_host = Instant::now(),
                "BYE" if server.is_some() && parts.get(1).copied() == Some("1") => {
                    break 'session Err("host closed the session".into());
                }
                _ => {}
            }
        }

        if let Some(input) = input.as_ref() {
            while let Ok(line) = input.try_recv() {
                match line.trim() {
                    ":quit" => break 'session Ok(()),
                    ":help" => println!("Enter a message or :quit; {MAX_CHAT} characters maximum."),
                    "" => {}
                    _ if !valid_field(&line, MAX_CHAT) => eprintln!("Invalid chat line (maximum {MAX_CHAT} characters; no controls or '|')."),
                    _ if queue.len() < 8 => queue.push_back(line),
                    _ => eprintln!("Chat queue full (8 lines); slow down."),
                }
            }
        }

        if let Some(address) = server {
            if last_info.is_none_or(|when| when.elapsed() >= Duration::from_secs(2)) {
                if let Err(e) = send(&socket, address, &info(player_id, name)) {
                    break 'session Err(e);
                }
                last_info = Some(Instant::now());
            }
            if last_heartbeat.is_none_or(|when| when.elapsed() >= Duration::from_secs(1)) {
                sequence = sequence.wrapping_add(1);
                let frame = heartbeat(player_id, sequence, (start.elapsed().as_millis() as u32).max(1));
                if let Err(e) = socket.send_to(&frame, address) {
                    break 'session Err(e.to_string());
                }
                last_heartbeat = Some(Instant::now());
            }
            if let Some(message) = queue.front() {
                if last_chat.is_none_or(|when| when.elapsed() >= CHAT_DELAY) {
                    if let Err(e) = send(&socket, address, &format!("CHAT|{player_id}|{message}")) {
                        break 'session Err(e);
                    }
                    println!("<{name}> {message}");
                    queue.pop_front();
                    last_chat = Some(Instant::now());
                    if single {
                        one_shot_sent = Some(Instant::now());
                    }
                }
            }
        }

        // The original protocol uses UDP; a successful send does not prove delivery.
        if one_shot_sent.is_some_and(|when| when.elapsed() >= Duration::from_secs(2)) {
            break 'session Ok(());
        }
        std::thread::sleep(TICK);
    };

    if let Some(address) = server {
        let _ = send(&socket, address, &format!("BYE|{player_id}"));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handshake_matches_legacy_v7_fields() {
        let greeting = hello("DEV_TEST_001", Some(0xabc), 0x1234);
        let words: Vec<_> = greeting.split('|').collect();
        assert_eq!(words.len(), 11);
        assert_eq!(words[1], "7");
        assert_eq!(words[2], "000000000ABC");
        assert_eq!(words[3], "DEV_TEST_001");
        assert_eq!(words[7], "0.00");
        assert_eq!(words[10], "0000000000001234");
    }

    #[test]
    fn legacy_info_is_vehicleless() {
        let message = info(42, "Dev");
        let fields: Vec<_> = message.split('|').collect();
        assert_eq!(fields.len(), 14);
        assert_eq!(fields[3], "");
        assert_eq!(fields[7], "0.00");
    }

    #[test]
    fn old_heartbeat_decodes_as_no_vehicle() {
        let bytes = heartbeat(42, 2, 1000);
        let (id, seq, pose) = omsi_net::wire::decode_state(&bytes, 7).unwrap();
        assert_eq!((id, seq, pose.sent_ms), (42, 2, 1000));
        assert_eq!(pose.flags & omsi_net::FLAG_VEHICLE, 0);
    }

    #[test]
    fn welcome_must_use_protocol_seven_and_its_session() {
        assert_eq!(welcome_id(&["WELCOME", "7", "9", "000000000ABC"], Some(0xabc)), Some(9));
        assert_eq!(welcome_id(&["WELCOME", "14", "9", "000000000ABC"], Some(0xabc)), None);
        assert_eq!(welcome_id(&["WELCOME", "7", "9", "000000000ABB"], Some(0xabc)), None);
    }
}
