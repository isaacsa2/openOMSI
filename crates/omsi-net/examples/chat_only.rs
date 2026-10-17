//! A chat-only openOMSI client, using the normal LAN transport without a map, vehicle or GPU.
//!
//! cargo run -p omsi-net --example chat_only -- 127.0.0.1:27015 --name Isaac
//! cargo run -p omsi-net --example chat_only -- https://server.example --name Isaac --message "Hello!"
//!
//! This is an ordinary player connection, NOT an administrative bypass or a verified staff identity.

use omsi_net::{LanEvent, LanSession, Pose, WorldInfo, MAX_CHAT, MAX_NAME};
use std::collections::VecDeque;
use std::io::{self, BufRead};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

const TICK: Duration = Duration::from_millis(50);

struct Args {
    target: String,
    name: String,
    message: Option<String>,
}

fn usage() -> &'static str {
    "Usage: cargo run -p omsi-net --example chat_only -- <server> [--name NAME] [--message TEXT]\n\
     Server: IP:UDP_PORT, OMSI session code, http(s):// gateway, or official alias\n\
     Interactive commands: :help, :quit\n\
     Connects as a regular chat-only player; it does not grant administrator rights."
}

fn valid_field(value: &str, limit: usize) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= limit
        && !value.chars().any(|c| c.is_control() || c == '|')
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Option<Args>, String> {
    let Some(target) = args.next() else { return Err(usage().into()) };
    if matches!(target.as_str(), "--help" | "-h") {
        return Ok(None);
    }
    if target.starts_with("--") {
        return Err(usage().into());
    }
    let mut name = "Chat Console".to_string();
    let mut message = None;
    while let Some(option) = args.next() {
        match option.as_str() {
            "--name" => name = args.next().ok_or("--name requires a value")?,
            "--message" if message.is_none() => {
                message = Some(args.next().ok_or("--message requires a value")?);
            }
            "--help" | "-h" => return Ok(None),
            _ => return Err(format!("unknown or repeated option: {option}\n{}", usage())),
        }
    }
    if !valid_field(&name, MAX_NAME) {
        return Err(format!("name must be 1 to {MAX_NAME} characters, without controls or |"));
    }
    if message.as_ref().is_some_and(|m| !valid_field(m, MAX_CHAT)) {
        return Err(format!("message must be 1 to {MAX_CHAT} characters, without controls or |"));
    }
    Ok(Some(Args { target, name, message }))
}

fn stdin_lines() -> Receiver<String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                return;
            }
        }
        // Closing stdin exits the interactive client cleanly.
        let _ = tx.send(":quit".into());
    });
    rx
}

fn print_event(event: LanEvent) {
    match event {
        LanEvent::Chat { name, text, .. } => println!("<{name}> {text}"),
        LanEvent::Notice(text) => println!("* {text}"),
    }
}

fn connect(target: &str, name: &str) -> Result<(LanSession, Option<omsi_net::ws::WsClient>), String> {
    let resolved = omsi_net::official::resolve_target(target)?;
    let ws = if let Some(url) = omsi_net::ws::ws_url(&resolved) {
        Some(omsi_net::ws::WsClient::connect(&url)?)
    } else {
        None
    };
    // The WebSocket bridge exposes local UDP to the regular LAN transport.
    // Keep the bridge alive until the session closes.
    let address = match ws.as_ref() {
        Some(client) => client.local.to_string(),
        None => resolved,
    };
    let session = LanSession::join(&address, name, WorldInfo::default(), Duration::from_secs(5))?;
    Ok((session, ws))
}

fn run_chat(mut lan: LanSession, ws: Option<omsi_net::ws::WsClient>, once: Option<String>) -> Result<(), String> {
    let interactive = once.is_none();
    let input = interactive.then(stdin_lines);
    let mut pending = VecDeque::new();
    if let Some(msg) = once {
        pending.push_back(msg);
    }
    let pose = Pose::default(); // No bus, person, position claim or world simulation.
    let mut connected = false;
    let mut last = Instant::now();
    let mut delivered_at = None;
    let mut reconnects = ws.as_ref().map(|c| c.reconnects()).unwrap_or(0);

    loop {
        let dt = last.elapsed().as_secs_f32().min(0.25);
        last = Instant::now();
        if let Some(bridge) = ws.as_ref() {
            let count = bridge.reconnects();
            if count != reconnects {
                reconnects = count;
                lan.rehello();
            }
        }
        lan.tick(dt, &pose);
        for event in lan.take_events() {
            print_event(event);
        }
        if let Some(why) = lan.turned_away.as_ref().or(lan.rejected.as_ref()) {
            return Err(why.clone());
        }
        if lan.connected && !connected {
            connected = true;
            println!("Connected as {} (player {}). No map or bus loaded.", lan.my_name, lan.my_id);
        } else if !lan.connected && connected {
            connected = false;
            println!("* Connection lost; waiting for the normal session to reconnect.");
        }

        if let Some(rx) = input.as_ref() {
            while let Ok(line) = rx.try_recv() {
                match line.trim() {
                    ":quit" => return Ok(()),
                    ":help" => println!("Type a chat line or :quit. Messages are limited to {MAX_CHAT} characters."),
                    "" => {}
                    _ if !valid_field(&line, MAX_CHAT) => {
                        eprintln!("Chat lines must be 1 to {MAX_CHAT} characters, without controls or |");
                    }
                    _ if pending.len() < 8 => pending.push_back(line),
                    _ => eprintln!("Chat queue full (8 lines); slow down."),
                }
            }
        }

        if lan.connected {
            if let Some(line) = pending.front() {
                match lan.say(line) {
                    Ok(()) => {
                        pending.pop_front();
                        if !interactive {
                            delivered_at = Some(Instant::now());
                        }
                    }
                    // The shared protocol allows a small burst, then one line/second.
                    Err(e) if e.starts_with("not so fast") => {}
                    Err(e) => {
                        eprintln!("Message not sent: {e}");
                        pending.pop_front();
                    }
                }
            }
        }
        // UDP has no chat acknowledgement. Give a one-shot message a short transport window
        // before disconnecting; this cannot prove that the host displayed the message.
        if delivered_at.is_some_and(|at: Instant| at.elapsed() >= Duration::from_secs(2)) {
            return Ok(());
        }
        std::thread::sleep(TICK);
    }
}

fn main() {
    let result = (|| {
        let Some(args) = parse_args(std::env::args().skip(1))? else {
            println!("{}", usage());
            return Ok(());
        };
        let (lan, bridge) = connect(&args.target, &args.name)?;
        println!("Joining {} as {} (chat only).", args.target, args.name);
        if args.message.is_none() {
            println!("Type a message, or :quit to leave.");
        }
        run_chat(lan, bridge, args.message)
    })();
    if let Err(e) = result {
        eprintln!("chat_only: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_public_chat_only_and_one_shot() {
        let a = parse_args(["127.0.0.1:27015", "--name", "Isaac", "--message", "Hi"]
            .into_iter().map(str::to_string)).unwrap().unwrap();
        assert_eq!(a.target, "127.0.0.1:27015");
        assert_eq!(a.name, "Isaac");
        assert_eq!(a.message.as_deref(), Some("Hi"));
    }

    #[test]
    fn rejects_protocol_separator_and_oversized_chat() {
        assert!(parse_args(["server", "--name", "Admin|Fake"].into_iter().map(str::to_string)).is_err());
        assert!(parse_args(["server", "--message", &"x".repeat(MAX_CHAT + 1)]
            .into_iter().map(str::to_string)).is_err());
    }
}
