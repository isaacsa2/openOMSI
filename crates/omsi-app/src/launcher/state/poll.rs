//! A poll of the launcher core taken in: the installs that started and ended, the games
//! running and the ones that ended (on an error, or sent away by their server), and a
//! changed installation read again.

use super::*;

impl State {
    pub(super) fn polled(&mut self, p: core::Poll) {
        for s in &p.started {
            core::log_to_file(&format!("inbox: installing {s}"));
        }
        if !p.started.is_empty() {
            self.set_status(format!("Installing from the Mods folder: {}", p.started.iter().map(|x| x.rsplit('/').next().unwrap_or(x)).collect::<Vec<_>>().join(", ")), false);
        }
        let mut installed = false;
        for j in &p.jobs {
            let was = self.jobs.iter().find(|x| x.id == j.id).map(|x| x.finished.is_some()).unwrap_or(false);
            if j.finished.is_some() && !was && self.stamp.is_some() {
                if j.state == "done" {
                    installed = true;
                    self.set_status(j.message.clone(), false);
                } else if j.state == "failed" {
                    self.set_status(format!("{}: {}", j.name, j.message), true);
                }
            }
        }
        self.jobs = p.jobs;
        // a game that was running and is not any more: did it end on an error?
        for old in self.instances.iter().filter(|i| i.running) {
            let still = p.instances.iter().any(|n| n.pid == old.pid && n.running);
            if !still && !self.stopping.contains(&old.pid) {
                if let Some(why) = disconnect_of(std::path::Path::new(&old.log)) {
                    core::log_to_file(&format!("game {} was sent away by its server: {why}", old.pid));
                    self.disconnected = Some(why);
                } else if let Some(c) = crash_of(std::path::Path::new(&old.log)) {
                    core::log_to_file(&format!("game {} ended on an error: {}", old.pid, c.0));
                    self.crash = Some(c);
                }
            }
        }
        self.instances = p.instances;
        // the game started from here is in the list: whether it runs is known
        // (one that ended at once left the launcher blank until the 15 s were out)
        if game_listed(self.launched_pid, &self.instances) {
            self.launch_hold = None;
            self.launched_pid = None;
        }
        for i in &self.instances {
            if !i.running {
                self.stopping.remove(&i.pid);
            }
        }
        // (while the lists are read the stamp moves by itself - the cache gains
        // the folders each bus depends on - and every poll started the whole
        // reading over on top of the one going: a big installation never
        // finished. A change then is taken up when the reading is done.)
        let changed = self.stamp.as_ref().map(|s| *s != p.stamp).unwrap_or(false);
        self.stamp = if self.loading_content { None } else { Some(p.stamp) };
        if changed || installed {
            if self.loading_content {
                self.reload_content = true;
            } else {
                self.load_content();
            }
            self.load_mods();
        }
        for pid in self.open_logs.clone() {
            self.log_tail(pid);
        }
    }
}
