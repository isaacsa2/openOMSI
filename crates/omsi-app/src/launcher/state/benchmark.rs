use super::{core, State};

impl State {
    pub fn benchmark(&mut self) {
        if !self.save_pending_settings() {
            return;
        }
        let root = std::path::Path::new(&self.config.root);
        if !omsi_cfg::missing_original_essentials(root).is_empty() {
            self.set_status("The benchmark needs the original OMSI 2: choose its folder under Setup first.", true);
            return;
        }
        let required = [
            "maps/Grundorf/global.cfg",
            "Vehicles/MAN_SD200/MAN_SD80.bus",
        ];
        let missing: Vec<_> = required
            .iter()
            .filter(|rel| !root.join(rel).is_file())
            .copied()
            .collect();
        if !missing.is_empty() {
            self.set_status(
                format!("Benchmark cannot start: stock OMSI 2 content is missing: {}", missing.join(", ")),
                true,
            );
            return;
        }
        self.set_status("Starting stock Grundorf performance benchmark…", false);
        self.queued_launch = Some(core::Duty {
            benchmark: true,
            map: "maps/Grundorf/global.cfg".into(),
            bus: "Vehicles/MAN_SD200/MAN_SD80.bus".into(),
            time: "09:00".into(),
            date: Some("1989-05-30".into()),
            traffic: Some(20),
            passengers: Some(true),
            schedule: Some(true),
            autostart: Some(true),
            ..Default::default()
        });
    }
}
