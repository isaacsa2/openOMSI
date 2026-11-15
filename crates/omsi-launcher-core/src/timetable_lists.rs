use super::*;

// ---------------------------------------------------------------------------------------
// timetable: lines, tours, trips and the roadbook

#[derive(Serialize, Clone, Debug)]
pub struct StopInfo {
    pub name: String,
    pub arr: f64,
    pub dep: f64,
}

/// One trip of a tour, as OMSI's timetable dialog lists it: when it leaves, from where to
/// where, on which line, and the trip's (route's) own name.
#[derive(Serialize, Clone, Debug)]
pub struct TripInfo {
    /// The trip file's name ("4 Liman-ZS"): the name the map gives the route.
    pub name: String,
    /// Its place in the tour, 1 = the first (what `--trip` takes, as does its departure).
    pub index: usize,
    /// The line its displays show (a depot run has none).
    pub line: String,
    /// First and last stop.
    pub from: String,
    pub terminus: String,
    pub departure: f64,
    pub arrival: f64,
    pub stops: Vec<StopInfo>,
    pub km: f64,
}

#[derive(Serialize, Clone, Debug)]
pub struct TourInfo {
    /// Selection identity within a line; the displayed number may repeat on other days.
    pub id: String,
    pub number: String,
    pub ai_group: String,
    pub first: f64,
    pub last: f64,
    /// The days it runs, in words ("Mon-Fri", "Sat", "daily", ...), from its validity mask.
    pub days: String,
    /// It runs on the date asked for (the game's timetable has only these tours that day).
    pub runs: bool,
    /// The first date from the one asked for on which it runs (`YYYY-MM-DD`): the game's
    /// timetable dialog lists only the tours of the chosen day, so a
    /// tour of another day is picked by moving the date to it.
    pub next_run: Option<String>,
    pub trips: Vec<TripInfo>,
}

#[derive(Serialize, Clone, Debug)]
pub struct LineInfo {
    pub name: String,
    pub user_allowed: bool,
    /// The line's ends: the first and the last stop of its longest trip (the trips' own
    /// termini where no trip has two stops).
    pub termini: Vec<String>,
    pub tours: Vec<TourInfo>,
}

impl LineInfo {
    /// Older saved choices contain just the tour number. Prefer today's variant when
    /// upgrading those choices, while exact IDs keep an explicit other-day selection.
    pub fn tour(&self, selection: &str) -> Option<&TourInfo> {
        self.tours
            .iter()
            .find(|t| t.id == selection)
            .or_else(|| self.tours.iter().find(|t| t.number == selection && t.runs))
            .or_else(|| self.tours.iter().find(|t| t.number == selection))
    }
}

/// The date the game starts on without `--date` (its clock's default, day 150 of 1989), and
/// the launcher's own default date.
pub const DEFAULT_DATE: &str = "1989-05-30";

/// A trip as a map picture needs it: the map's own road pieces the trip drives, in order,
/// and its stops.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TripPath {
    /// The road pieces it drives, in order (the shared piece of two links appears once).
    pub route: Vec<RoadPiece>,
    /// The stop objects the trip calls at, in order.
    pub stops: Vec<i64>,
}

/// One spline of the map a trip drives over: its id in its tile, which `[path]` of it the
/// trip uses, and the tile's place in `global.cfg`'s `[map]` list. It is the game's own
/// `LaneKey` (`schedule::steps_of` builds the same three fields), so the launcher's map
/// picture can find the very lane the game would drive the trip on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RoadPiece {
    pub tile_x: i32,
    pub tile_y: i32,
    pub spline: i64,
    /// The `[path]` of that spline's `.sli` the trip drives (the game's own lane).
    pub path: u16,
}

/// The route of trip `trip` of the map `map` on `date`: its road pieces and its stops.
/// The trip's own track (`.ttr`) when it names one - trains, ferries, and the type-1 trips
/// of mod maps - else the station links between its stops, as the game walks them.
pub fn trip_path(map: &str, date: &str, trip: &str) -> Result<TripPath> {
    let map_dir = resolve_content(map)?
        .parent()
        .map(|p| p.to_path_buf())
        .context("map folder")?;
    let date = if date.trim().is_empty() {
        DEFAULT_DATE
    } else {
        date.trim()
    };
    let code = omsi_map::date_code(date)
        .with_context(|| i18n::trf("'{date}' is not a date (YYYY-MM-DD)", &[("date", &date)]))?;
    let chrono = omsi_map::active_chrono_dirs(&map_dir, code);
    let off = omsi_map::chrono_deactivated_lines(&chrono);
    let data = omsi_timetable::TimetableData::load_with_chrono(&map_dir, &chrono, &off);
    let mut out = TripPath::default();
    let Some(t) = data.trips.iter().find(|x| x.is_named(trip.trim())) else {
        return Ok(out);
    };
    // a type-1 trip's stations are its own `[station]` records (all of Novi Sad)
    out.stops = if t.stations.is_empty() {
        t.stations_legacy
            .iter()
            .filter_map(|r| r.first()?.trim().parse::<i64>().ok())
            .collect()
    } else {
        t.stations.clone()
    };
    // the tile of a path step is the index of its entry in `[map]`, as the game reads it
    let tiles = omsi_map::GlobalCfg::load(&omsi_cfg::oi::global_path(&map_dir))
        .map(|g| g.raw_tiles)
        .unwrap_or_default();
    let at = |tile_index: f64| tiles.get(tile_index as usize).copied();
    let track_name = if t.display_name.trim().is_empty() {
        t.name.trim()
    } else {
        t.display_name.trim()
    };
    if let Some(track) = data.tracks.iter().find(|x| {
        x.path
            .file_stem()
            .map(|s| s.to_string_lossy().eq_ignore_ascii_case(track_name))
            .unwrap_or(false)
    }) {
        for e in &track.entries {
            if e.values.len() < 5 {
                continue;
            }
            if let Some(tile) = at(e.values[2]) {
                out.route.push(RoadPiece {
                    tile_x: tile.0,
                    tile_y: tile.1,
                    spline: e.values[0] as i64,
                    path: e.values[1] as u16,
                });
            }
        }
        return Ok(out);
    }
    out.route = link_route(&data, &out.stops, &at);
    Ok(out)
}

/// The road pieces a run of stops drives by the map's station links (consecutive links
/// repeat the piece they share).
fn link_route(
    data: &omsi_timetable::TimetableData,
    stops: &[i64],
    at: &dyn Fn(f64) -> Option<(i32, i32)>,
) -> Vec<RoadPiece> {
    let mut route: Vec<RoadPiece> = Vec::new();
    for w in stops.windows(2) {
        let Some(link) = data
            .stn_links
            .iter()
            .find(|l| l.from_id == w[0] && l.to_id == w[1])
        else {
            continue;
        };
        for e in &link.entries {
            if e.values.len() < 4 {
                continue;
            }
            let Some(tile) = at(e.values[2]) else {
                continue;
            };
            let p = RoadPiece {
                tile_x: tile.0,
                tile_y: tile.1,
                spline: e.values[0] as i64,
                path: e.values[1] as u16,
            };
            if route.last() != Some(&p) {
                route.push(p);
            }
        }
    }
    route
}

/// Active Chrono directories for `map_dir` when `date` parses.
pub fn chrono_dirs_for_date(map_dir: &Path, date: &str) -> Vec<PathBuf> {
    omsi_map::date_code(date.trim())
        .map(|c| omsi_map::active_chrono_dirs(map_dir, c))
        .unwrap_or_default()
}

/// Timetable data for `map_dir`, layering active Chrono events when `date` parses.
pub fn timetable_for_date(map_dir: &Path, date: &str) -> omsi_timetable::TimetableData {
    let chrono = chrono_dirs_for_date(map_dir, date);
    let off = omsi_map::chrono_deactivated_lines(&chrono);
    omsi_timetable::TimetableData::load_with_chrono(map_dir, &chrono, &off)
}

/// A run of stops - a trip being made in the timetable editor - as a map picture needs it:
/// the road pieces the map's station links drive between them.
pub fn chain_path(map: &str, date: &str, stops: &[i64]) -> Result<TripPath> {
    let map_dir = resolve_content(map)?
        .parent()
        .map(|p| p.to_path_buf())
        .context("map folder")?;
    let data = timetable_for_date(&map_dir, date);
    let tiles = omsi_map::GlobalCfg::load(&map_dir.join("global.cfg"))
        .map(|g| g.raw_tiles)
        .unwrap_or_default();
    let at = |tile_index: f64| tiles.get(tile_index as usize).copied();
    Ok(TripPath {
        route: link_route(&data, stops, &at),
        stops: stops.to_vec(),
    })
}

/// The lines of a map's timetable on `date` (`YYYY-MM-DD`, the game's default when empty):
/// the chrono folders active that day add their lines and take theirs off, as the game does
/// - Spandau's 1991 timetable change replaces line "5 & 5N" and sixteen others.
pub fn list_lines(map: &str, date: &str) -> Result<Vec<LineInfo>> {
    let map_dir = resolve_content(map)?
        .parent()
        .map(|p| p.to_path_buf())
        .context("map folder")?;
    lines_on(&map_dir, date)
}

pub(super) fn lines_on(map_dir: &Path, date: &str) -> Result<Vec<LineInfo>> {
    let date = if date.trim().is_empty() {
        DEFAULT_DATE
    } else {
        date.trim()
    };
    let code = omsi_map::date_code(date)
        .with_context(|| i18n::trf("'{date}' is not a date (YYYY-MM-DD)", &[("date", &date)]))?;
    let chrono = omsi_map::active_chrono_dirs(map_dir, code);
    let off = omsi_map::chrono_deactivated_lines(&chrono);
    let data = omsi_timetable::TimetableData::load_with_chrono(map_dir, &chrono, &off);
    // which tours run that day: the tour's mask as the game reads it (bits 0-6 Monday to
    // Sunday, 7 a public holiday, 8 school days, 9 school holidays: the original, #1883)
    let calendar = omsi_map::Calendar::load(&map_dir.join("Holidays.txt")).unwrap_or_default();
    let day_bit = if calendar.is_holiday(code) {
        1 << 7
    } else {
        1 << weekday(code)
    };
    let school_bit = if calendar.in_holiday_range(code) {
        1 << 9
    } else {
        1 << 8
    };
    let mut out = Vec::new();
    for l in &data.lines {
        let mut termini: Vec<String> = Vec::new();
        // the line's two ends: the first and last stop of its longest trip (the termini of
        // every short working and depot run, listed one after another, read as all its stops)
        let mut ends: Option<(usize, String, String)> = None;
        let mut tours = Vec::new();
        for (index, t) in l.tours.iter().enumerate() {
            let mask = t.extra.trim().parse::<i32>().unwrap_or(1023);
            let runs_on = |c: i32| {
                let day = if calendar.is_holiday(c) {
                    1 << 7
                } else {
                    1 << weekday(c)
                };
                let school = if calendar.in_holiday_range(c) {
                    1 << 9
                } else {
                    1 << 8
                };
                mask & day != 0 && mask & school != 0
            };
            // (and yesterday's night tour: its trips past 24:00 run today, #1576)
            let night = t.trips.iter().any(|tt| tt.departure >= 24.0 * 60.0);
            let runs = (mask & day_bit != 0 && mask & school_bit != 0)
                || (night && runs_on(day_before(code)));
            let next_run = (0..400)
                .map(|k| add_days(code, k))
                .find(|c| runs_on(*c))
                .map(|c| format!("{:04}-{:02}-{:02}", c / 10000, c / 100 % 100, c % 100));
            let mut trips = Vec::new();
            for tt in &t.trips {
                let Some(trip) = data.trips.iter().find(|x| x.is_named(&tt.trip)) else {
                    continue;
                };
                let departure = tt.departure as f64 * 60.0;
                let duration = trip
                    .profiles
                    .get(tt.profile.max(0) as usize)
                    .or(trip.profiles.first())
                    .map(|p| p.factor as f64 * 60.0)
                    .filter(|d| *d > 0.0)
                    .unwrap_or(600.0);
                // its stations: [station_typ2] objects, or the [station] records of a type-1
                // trip (all of Novi Sad), which carry their stop's name themselves
                let legacy: Vec<(i64, String)> = trip
                    .stations_legacy
                    .iter()
                    .filter_map(|r| {
                        Some((
                            r.first()?.trim().parse::<i64>().ok()?,
                            r.get(2).map(|n| n.trim().to_string()).unwrap_or_default(),
                        ))
                    })
                    .collect();
                let stations: Vec<i64> = if trip.stations.is_empty() {
                    legacy.iter().map(|x| x.0).collect()
                } else {
                    trip.stations.clone()
                };
                let mut lens = Vec::new();
                for w in stations.windows(2) {
                    lens.push(
                        data.stn_links
                            .iter()
                            .find(|k| k.from_id == w[0] && k.to_id == w[1])
                            .map(|k| k.length.max(1.0))
                            .unwrap_or(500.0),
                    );
                }
                let total: f64 = lens.iter().sum::<f64>().max(1.0);
                let mut acc = 0.0;
                let mut stops = Vec::new();
                for (i, id) in stations.iter().enumerate() {
                    let t_at = departure + duration * acc / total;
                    let name = data
                        .bus_stops
                        .iter()
                        .find(|b| b.object_id == *id)
                        .map(|b| b.name.trim().to_string())
                        .or_else(|| {
                            legacy
                                .iter()
                                .find(|x| x.0 == *id)
                                .map(|x| x.1.clone())
                                .filter(|n| !n.is_empty())
                        })
                        .unwrap_or_else(|| i18n::trf("stop {id}", &[("id", &id)]));
                    stops.push(StopInfo {
                        name,
                        arr: t_at,
                        dep: if i == 0 { departure } else { t_at },
                    });
                    if i < lens.len() {
                        acc += lens[i];
                    }
                }
                if !trip.terminus.trim().is_empty()
                    && !termini.iter().any(|x| x == trip.terminus.trim())
                {
                    termini.push(trip.terminus.trim().to_string());
                }
                let from = stops.first().map(|s| s.name.clone()).unwrap_or_default();
                if stops.len() >= 2 && ends.as_ref().is_none_or(|e| stops.len() > e.0) {
                    ends = Some((
                        stops.len(),
                        from.clone(),
                        stops.last().map(|s| s.name.clone()).unwrap_or_default(),
                    ));
                }
                let index = trips.len() + 1;
                trips.push(TripInfo {
                    name: trip.name.clone(),
                    index,
                    line: trip.line.trim().to_string(),
                    from,
                    terminus: trip.terminus.trim().to_string(),
                    departure,
                    arrival: departure + duration,
                    stops,
                    km: total / 1000.0,
                });
            }
            // (a tour with no trip the timetable knows cannot be driven: not offered)
            if trips.is_empty() {
                continue;
            }
            let first = trips.first().map(|t| t.departure).unwrap_or(0.0);
            let last = trips.last().map(|t| t.arrival).unwrap_or(0.0);
            let id = serde_json::to_string(&(index, &t.number, &t.ai_group, &t.extra))?;
            tours.push(TourInfo {
                id,
                number: t.number.clone(),
                ai_group: t.ai_group.clone(),
                first,
                last,
                days: days_of(mask),
                runs,
                next_run,
                trips,
            });
        }
        tours.sort_by(|a, b| a.first.total_cmp(&b.first));
        if let Some((_, first, last)) = ends {
            termini = if first == last {
                vec![first]
            } else {
                vec![first, last]
            };
        }
        out.push(LineInfo {
            name: l.name.clone(),
            user_allowed: l.user_allowed,
            termini,
            tours,
        });
    }
    out.sort_by(|a, b| natural_key(&a.name).cmp(&natural_key(&b.name)));
    Ok(out)
}


fn natural_key(s: &str) -> (u64, String) {
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    (digits.parse().unwrap_or(u64::MAX), s.to_ascii_lowercase())
}

/// What the driver types into the IBIS for a line, from the bus's depot file: the line
/// code, and for each destination the route code and the terminus code.
#[derive(Serialize, Clone, Debug)]
pub struct IbisInfo {
    pub hof: String,
    pub line_code: String,
    pub routes: Vec<IbisRoute>,
    pub trip_route: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct IbisRoute {
    pub code: String,
    pub route: String,
    pub name: String,
    pub terminus_code: i32,
    pub terminus: String,
}

pub fn ibis_info(
    bus: &str,
    hof_name: &str,
    line: &str,
    terminus: &str,
    stops: &[String],
) -> Result<IbisInfo> {
    let bus_path = resolve_content(bus)?;
    let dir = bus_path.parent().context("bus folder")?;
    // the bus's own depot file of that name (every content root's copy of its folder), else
    // the one another bus brings (the game borrows it the same way), else the bus's first
    let hof = omsi_vehicle::hof::depot_in(dir, hof_name)
        .or_else(|| omsi_vehicle::hof::depot_anywhere(hof_name))
        .or_else(|| {
            omsi_vehicle::hof::depot_files(dir)
                .iter()
                .find_map(|f| omsi_vehicle::Hof::load(f).ok())
        })
        .with_context(|| i18n::tr("no depot file next to the bus"))?;
    let hof = &hof;
    let line_digits: String = line.chars().take_while(|c| c.is_ascii_digit()).collect();
    let mut routes = Vec::new();
    for t in &hof.info_trips {
        let matches = t.line.trim().eq_ignore_ascii_case(line.trim())
            || (!line_digits.is_empty()
                && t.code
                    .trim_start_matches('0')
                    .starts_with(line_digits.trim_start_matches('0'))
                && t.code.len() >= line_digits.len());
        if !matches {
            continue;
        }
        let code = omsi_cfg::parse_i32(&t.route);
        let terminus = hof
            .termini
            .iter()
            .find(|x| x.code == code)
            .and_then(|x| x.strings.first().cloned())
            .unwrap_or_default();
        routes.push(IbisRoute {
            code: t.code.clone(),
            route: t.route.clone(),
            name: t.name.clone(),
            terminus_code: code,
            terminus,
        });
    }
    let stops: Vec<&str> = stops.iter().map(String::as_str).collect();
    let trip_route = (!terminus.trim().is_empty())
        .then(|| omsi_vehicle::hof::trip_terminus(hof, line, terminus, &stops))
        .flatten()
        .and_then(|(_, r)| r)
        .map(|r| hof.info_trips[r].code.clone());
    Ok(IbisInfo {
        hof: hof.name.clone(),
        line_code: line_digits,
        routes,
        trip_route,
    })
}
