use super::*;

    use super::*;

    #[test]
    fn bounded_due_window_matches_full_scan_at_boundaries_and_after_clock_changes() {
        let times = [-86_400.0, -100.0, 0.0, 30.0, 60.0, 60.0, 90.0, 1_200.0, 86_400.0];
        let departures: Vec<Departure> = times.iter().map(|&time| Departure {
            time,
            trip: 0,
            profile: 0,
            line: String::new(),
            ai_group: String::new(),
            tour: String::new(),
            mask: 0,
            spawned: false,
            yesterday: false,
        }).collect();
        // Include the loading/restart behavior: a trip already underway from
        // before the due window must still be considered.
        for (tod, window) in [(60.0, 0.0), (60.0, 2.0), (60.0, 60.0), (60.0, 1_200.0),
                              (90.0, 20.0), (-50.0, 2.0), (86_400.0, 1_200.0)] {
            let loading = window > 60.0;
            let eligible = |i: usize| {
                let d = &departures[i];
                d.time <= tod && (d.time > tod - window
                    || (loading && d.time + 3_600.0 > tod))
            };
            let old: Vec<usize> = (0..departures.len()).filter(|&i| eligible(i)).collect();
            let new: Vec<usize> = due_departure_range(&departures, tod, window, loading)
                .filter(|&i| eligible(i))
                .collect();
            assert_eq!(new, old, "at time {tod} with window {window}");
        }
    }
