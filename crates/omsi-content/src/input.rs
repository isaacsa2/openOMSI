//! `Inputs/keyboard.cfg`, `Inputs/gamectrler.cfg`, `Inputs/*.kyb` (unit `mc_input`).

use omsi_cfg::CfgFile;
use std::path::Path;

/// The low bit in a `keyboard.cfg` entry is OMSI's "duration" flag: the action
/// remains active for as long as the key is held.  It is not a keyboard modifier.
pub const KEY_FLAG_DURATION: i32 = 1;
pub const KEY_MOD_SHIFT: i32 = 2;
pub const KEY_MOD_CTRL: i32 = 4;
pub const KEY_MOD_ALT: i32 = 8;

/// The actual modifier-key part of the flags stored in `keyboard.cfg`.
pub fn key_modifiers(flags: i32) -> i32 {
    flags & !KEY_FLAG_DURATION
}

/// Encode the modifier keys in the representation used by `keyboard.cfg`.
pub fn key_modifier_flags(shift: bool, ctrl: bool, alt: bool) -> i32 {
    (shift as i32) * KEY_MOD_SHIFT
        | (ctrl as i32) * KEY_MOD_CTRL
        | (alt as i32) * KEY_MOD_ALT
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct KeyBinding {
    pub action: String,
    /// DirectInput scan code
    pub scan_code: i32,
    /// OMSI key flags: 1 = duration, 2 = Shift, 4 = Ctrl, 8 = Alt.
    pub modifier: i32,
}

impl KeyBinding {
    /// Whether this binding is the given physical key chord.  Duration is an action
    /// property and therefore deliberately does not participate in chord matching.
    pub fn matches(&self, scan_code: i32, modifiers: i32) -> bool {
        self.scan_code == scan_code && key_modifiers(self.modifier) == modifiers
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct KeyboardCfg {
    pub game: Vec<KeyBinding>,
    pub vehicles: Vec<KeyBinding>,
}

impl KeyboardCfg {
    pub fn load(path: &Path) -> Result<KeyboardCfg, omsi_cfg::CfgError> {
        let f = CfgFile::read(path)?;
        let mut k = KeyboardCfg::default();
        let mut section = 0;
        let mut r = f.reader();
        while let Some(kw) = r.next_keyword() {
            match kw.as_str() {
                "game" => section = 0,
                "vehicles" => section = 1,
                "entry" => {
                    let b = KeyBinding {
                        action: r.str().to_string(),
                        scan_code: r.i32(),
                        modifier: r.i32(),
                    };
                    if section == 0 {
                        k.game.push(b);
                    } else {
                        k.vehicles.push(b);
                    }
                }
                _ => {}
            }
        }
        Ok(k)
    }

    /// The keys the game adds to the file's (not written back by `save`).
    pub fn with_game_defaults(mut self) -> Self {
        // The IBIS's next stop with its announcement (`IBIS_vor`) has no key in OMSI's own
        // file: only the mouse on the IBIS reached it. Give it Q (scan code 16) only where
        // that physical chord is free. A duration binding (the stock microphone/announcement
        // action, for example) is still physically unmodified and therefore occupies Q.
        let q_taken = self
            .game
            .iter()
            .chain(self.vehicles.iter())
            .any(|b| b.scan_code == 16 && key_modifiers(b.modifier) == 0);
        if !q_taken && !self.vehicles.iter().any(|b| b.action.eq_ignore_ascii_case("IBIS_vor")) {
            self.vehicles.push(KeyBinding { action: "IBIS_vor".into(), scan_code: 16, modifier: 0 });
        }
        self
    }

    /// Write a `keyboard.cfg` the game (and this same loader) can read back: one
    /// `[game]`/`[vehicles]` section, each entry as `action / scan code / modifier`, blank
    /// lines between entries as the original ships it (some tools that read the file split
    /// on the blank line rather than the keyword).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let mut t = String::new();
        for (section, list) in [("game", &self.game), ("vehicles", &self.vehicles)] {
            t.push_str(&format!("[{section}]\r\n"));
            for b in list {
                t.push_str(&format!(
                    "\r\n[entry]\r\n{}\r\n{}\r\n{}\r\n",
                    b.action, b.scan_code, b.modifier
                ));
            }
            t.push_str("\r\n");
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, t)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct GameController {
    pub name: String,
    pub index: i32,
    /// 16 values: per logical axis (steer, throttle, brake, clutch, combined …) the device
    /// axis number and inversion flag.
    pub axes: Vec<i32>,
    pub buttons: Vec<(String, i32)>,
    pub ff_scale: (f32, f32),
}

pub fn load_game_controllers(path: &Path) -> Result<Vec<GameController>, omsi_cfg::CfgError> {
    let f = CfgFile::read(path)?;
    let mut out: Vec<GameController> = Vec::new();
    let mut r = f.reader();
    while let Some(kw) = r.next_keyword() {
        match kw.as_str() {
            "ctrl" => out.push(GameController {
                name: r.str().to_string(),
                index: r.i32(),
                ..Default::default()
            }),
            "axis" => {
                let v = (0..16).map(|_| r.i32()).collect();
                if let Some(c) = out.last_mut() {
                    c.axes = v;
                }
            }
            "buttons" => {
                let n = r.usize();
                let mut b = Vec::with_capacity(n);
                for _ in 0..n {
                    let a = r.str().to_string();
                    let i = r.i32();
                    b.push((a, i));
                }
                if let Some(c) = out.last_mut() {
                    c.buttons = b;
                }
            }
            "ffscale" => {
                let a = r.f32();
                let b = r.f32();
                if let Some(c) = out.last_mut() {
                    c.ff_scale = (a, b);
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

/// `.kyb`: `scancode<TAB>name` lines.
pub fn load_key_names(path: &Path) -> Result<Vec<(i32, String)>, omsi_cfg::CfgError> {
    let f = CfgFile::read(path)?;
    Ok(f.lines
        .iter()
        .filter_map(|l| {
            let (a, b) = l.split_once('\t')?;
            Some((omsi_cfg::parse_i32(a), b.trim().to_string()))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_cfg_round_trips_through_save() {
        let k = KeyboardCfg {
            game: vec![KeyBinding {
                action: "exit".into(),
                scan_code: 1,
                modifier: 4,
            }],
            vehicles: vec![
                KeyBinding {
                    action: "kw_blinker_links".into(),
                    scan_code: 44,
                    modifier: 0,
                },
                KeyBinding {
                    action: "kw_m_enginestart".into(),
                    scan_code: 50,
                    modifier: 1,
                },
            ],
        };
        let path =
            std::env::temp_dir().join(format!("omsi-keyboard-cfg-test-{}.cfg", std::process::id()));
        k.save(&path).unwrap();
        let back = KeyboardCfg::load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(back, k);
    }

    #[test]
    fn duration_is_not_a_keyboard_modifier() {
        assert_eq!(key_modifiers(KEY_FLAG_DURATION), 0);
        assert_eq!(key_modifiers(KEY_FLAG_DURATION | KEY_MOD_SHIFT), KEY_MOD_SHIFT);
        assert_eq!(key_modifier_flags(true, true, false), KEY_MOD_SHIFT | KEY_MOD_CTRL);

        let throttle = KeyBinding {
            action: "throttle".into(),
            scan_code: 17,
            modifier: KEY_FLAG_DURATION,
        };
        assert!(throttle.matches(17, 0));
        assert!(!throttle.matches(17, KEY_MOD_SHIFT));

        let shifted = KeyBinding {
            action: "wiper_interval".into(),
            scan_code: 17,
            modifier: KEY_FLAG_DURATION | KEY_MOD_SHIFT,
        };
        assert!(shifted.matches(17, KEY_MOD_SHIFT));
        assert!(!shifted.matches(17, 0));
    }
}
