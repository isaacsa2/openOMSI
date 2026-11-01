//! Short join codes: `123-456-789` for a session code (`OMSI-...`), from the code service
//! (`services/serverlist`, `/v1/codes`). A game hosting a session registers its session code
//! there (`Registration`) and shows the nine digits; a player types them and the game asks the
//! service for the session code behind them (`official::resolve_target`). The session code
//! itself always works: a game that cannot reach the service shows and takes it as before,
//! and the service knows nothing more than the session code a host would hand out anyway.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

/// The code service (the server list's Worker; `OMSI_CODE_SERVICE` names another, `https://`,
/// or `off`: no short codes, the session code is all that is shown and taken).
pub const SERVICE: &str = "https://serverlist.openomsi.workers.dev";

/// `OMSI_CODE_SERVICE=off`: nothing is sent to the code service.
pub fn disabled() -> bool {
    omsi_cfg::flags::OMSI_CODE_SERVICE.var().is_some_and(|s| s.trim().eq_ignore_ascii_case("off"))
}
/// How often a host tells the service it still hosts (a code lives three minutes after).
const REFRESH: Duration = Duration::from_secs(60);

fn service() -> String {
    match omsi_cfg::flags::OMSI_CODE_SERVICE.var().map(|s| s.trim().trim_end_matches('/')) {
        Some(s) if s.starts_with("https://") && s.len() > 8 && !s.contains(char::is_whitespace) => s.to_string(),
        _ => SERVICE.to_string(),
    }
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(8)).user_agent("openOMSI").build()
}

/// The nine digits of a short code however it was typed (`123-456-789`, `123 456 789`,
/// `123456789`); `None` for anything else - a port, an address, a session code. A short
/// code never starts with 0.
pub fn parse(text: &str) -> Option<String> {
    let t = text.trim();
    if !t.bytes().all(|b| b.is_ascii_digit() || b == b'-' || b == b' ') {
        return None;
    }
    let d: String = t.chars().filter(char::is_ascii_digit).collect();
    (d.len() == 9 && !d.starts_with('0')).then_some(d)
}

/// `123-456-789`.
pub fn format(digits: &str) -> String {
    match parse(digits) {
        Some(d) => format!("{}-{}-{}", &d[..3], &d[3..6], &d[6..]),
        None => digits.to_string(),
    }
}

/// A JSON request to the service and its JSON answer (`Null` when it has none).
fn post(agent: &ureq::Agent, url: &str, body: serde_json::Value) -> Result<serde_json::Value, ureq::Error> {
    let r = agent.post(url).set("Content-Type", "application/json").send_string(&body.to_string())?;
    Ok(r.into_string().ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default())
}

fn field<'a>(v: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(|x| x.as_str())
}

/// The session code a short code leads to now.
pub fn resolve(text: &str) -> Result<String, String> {
    let d = parse(text).ok_or("not a short code (nine digits)")?;
    if disabled() {
        return Err("short codes are turned off here (OMSI_CODE_SERVICE=off): ask the host for the long code (OMSI-...)".into());
    }
    let r = agent().get(&format!("{}/v1/codes/{d}", service())).call();
    let body: serde_json::Value = match r {
        Ok(r) => r.into_string().ok().and_then(|t| serde_json::from_str(&t).ok()).ok_or("the code service answered oddly")?,
        Err(ureq::Error::Status(404, _)) => return Err(format!("no session has the code {} now (it ends a few minutes after the host stops)", format(&d))),
        Err(ureq::Error::Status(429, _)) => return Err("too many codes tried: wait a minute".into()),
        Err(e) => return Err(format!("the code service could not be reached ({e}); ask the host for the long code (OMSI-...)")),
    };
    let code = field(&body, "code").ok_or("the code service answered oddly")?;
    if !crate::looks_like_code(code) {
        return Err("the code service answered oddly".into());
    }
    Ok(code.to_string())
}

/// A host's short code, kept while this lives: registered and refreshed on a thread of its
/// own (nothing waits on the network), released when dropped. `short` is `None` until the
/// service has answered, and while it cannot be reached.
pub struct Registration {
    short: Arc<Mutex<Option<String>>>,
    code: Arc<Mutex<String>>,
    stop: Arc<AtomicBool>,
}

impl Registration {
    pub fn start(code: String) -> Registration {
        let r = Registration { short: Arc::default(), code: Arc::new(Mutex::new(code)), stop: Arc::default() };
        if disabled() {
            return r;
        }
        let (short, code, stop) = (r.short.clone(), r.code.clone(), r.stop.clone());
        let spawned = std::thread::Builder::new().name("short code".into()).spawn(move || keep(&short, &code, &stop));
        if let Err(e) = spawned {
            log::warn!("short code: {e}");
        }
        r
    }

    /// `123-456-789`, once the service has given it.
    pub fn short(&self) -> Option<String> {
        self.short.lock().ok()?.as_deref().map(format)
    }

    /// The session code changed (a new address): the short code leads there from the next
    /// refresh on.
    pub fn update(&self, code: &str) {
        if let Ok(mut c) = self.code.lock() {
            if *c != code {
                *c = code.to_string();
            }
        }
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// The registration's thread: register, refresh every minute (registering anew when the
/// service forgot the code), release on stop.
fn keep(short: &Mutex<Option<String>>, code: &Mutex<String>, stop: &AtomicBool) {
    let agent = agent();
    let base = service();
    let mut held: Option<(String, String)> = None;
    let mut next = Instant::now();
    let mut told = false;
    while !stop.load(Ordering::Relaxed) {
        if Instant::now() >= next {
            let current = code.lock().map(|c| c.clone()).unwrap_or_default();
            let refreshed = held.as_ref().map(|(s, token)| {
                post(&agent, &format!("{base}/v1/codes/refresh"), serde_json::json!({ "short": s, "token": token, "code": current }))
            });
            let ok = match refreshed {
                Some(Ok(_)) => true,
                // (forgotten - the service restarted, or the code expired while this
                // computer slept): a new code
                Some(Err(ureq::Error::Status(404 | 403, _))) | None => {
                    match post(&agent, &format!("{base}/v1/codes"), serde_json::json!({ "code": current })) {
                        Ok(b) => match (field(&b, "short").and_then(parse), field(&b, "token")) {
                            (Some(s), Some(t)) => {
                                log::info!("short code: {} for {current}", format(&s));
                                held = Some((s.clone(), t.to_string()));
                                if let Ok(mut sh) = short.lock() {
                                    *sh = Some(s);
                                }
                                true
                            }
                            _ => false,
                        },
                        Err(e) => {
                            if !told {
                                log::info!("short code: the code service could not be reached ({e}); the session code works as it is");
                                told = true;
                            }
                            false
                        }
                    }
                }
                Some(Err(e)) => {
                    log::info!("short code: refresh failed ({e})");
                    false
                }
            };
            if !ok && held.is_none() {
                if let Ok(mut sh) = short.lock() {
                    *sh = None;
                }
            }
            next = Instant::now() + REFRESH;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    if let Some((s, token)) = held {
        let _ = post(&agent, &format!("{base}/v1/codes/release"), serde_json::json!({ "short": s, "token": token }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_codes_are_nine_digits_however_typed() {
        for typed in ["123-456-789", " 123 456 789 ", "123456789", "123-456 789"] {
            assert_eq!(parse(typed).as_deref(), Some("123456789"), "{typed}");
        }
        for not in ["", "27015", "12345678", "1234567890", "012-345-678", "123-456-78a", "OMSI-ABCD-EFGH", "1.2.3.4", "123:456:789"] {
            assert_eq!(parse(not), None, "{not}");
        }
        assert_eq!(format("123456789"), "123-456-789");
        assert_eq!(format("x"), "x");
    }

    #[test]
    fn a_short_code_is_not_taken_for_a_port_or_a_session_code() {
        assert!(!crate::looks_like_code("123-456-789"));
        assert!(crate::describe_join("123-456-789").unwrap().contains("short code"));
        assert!(crate::describe_join("27015").unwrap().contains("port 27015"));
    }
}
