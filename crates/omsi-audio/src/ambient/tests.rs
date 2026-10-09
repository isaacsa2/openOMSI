use super::*;

const RATE: u32 = 48_000;

/// `secs` seconds of the ambience with `p`, stereo interleaved, after `settle` seconds.
fn run(p: AmbientParams, settle: f32, secs: f32) -> Vec<f32> {
    let mut a = Ambient::new(RATE);
    a.set_params(p);
    let mut out = vec![0.0f32; 960];
    let settle_blocks = (settle * RATE as f32 / 480.0) as usize;
    for _ in 0..settle_blocks {
        out.iter_mut().for_each(|x| *x = 0.0);
        a.render(&mut out, 2, 1.0);
    }
    let blocks = (secs * RATE as f32 / 480.0) as usize;
    let mut all = Vec::with_capacity(blocks * 960);
    for _ in 0..blocks {
        out.iter_mut().for_each(|x| *x = 0.0);
        a.render(&mut out, 2, 1.0);
        all.extend_from_slice(&out);
    }
    all
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt() as f32
}

fn storm() -> AmbientParams {
    let mut p = AmbientParams { enabled: true, wind_10m: 19.0, roughness: 0.8, foliage: 1.0, rain_mm_h: 12.0, wetness: 1.0, convective: true, urban: 1.0, traffic: 1.0, ..Default::default() };
    p.wheels[0] = WheelInput { speed: 14.0, surface: Surface::Cobble, water_mm: 1.0, puddle_mm: 30.0, load: 1.0, gain_l: 1.0, gain_r: 1.0, ..Default::default() };
    p
}

#[test]
fn every_part_stays_finite_and_bounded() {
    // the loudest corner of every parameter: no NaN, no denormal blow-up, nothing near
    // full scale (the master limiter is not there to catch this layer)
    for surface in Surface::ALL {
        for inside in [false, true] {
            let mut p = storm();
            p.inside = inside;
            p.roof_rain = true;
            p.air_speed = 25.0;
            p.bus_speed = 25.0;
            for w in p.wheels.iter_mut() {
                *w = WheelInput { speed: 25.0, surface, water_mm: 2.0, puddle_mm: 60.0, load: 2.0, gain_l: 1.0, gain_r: 1.0, ..Default::default() };
            }
            let x = run(p, 0.5, 2.0);
            let peak = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(x.iter().all(|v| v.is_finite()), "{surface:?} inside {inside}");
            assert!(peak < 0.95, "{surface:?} inside {inside}: peak {peak}");
        }
    }
}

#[test]
fn calm_and_disabled_are_silent() {
    let p = AmbientParams { enabled: false, ..storm() };
    assert!(rms(&run(p, 0.5, 1.0)) == 0.0);
    // a calm, dry night with no wheel turning, nobody in town: next to nothing
    let calm = AmbientParams { enabled: true, wind_10m: 0.0, urban: 0.0, traffic: 0.0, sun_elevation: -30.0, day_of_year: 20.0, temperature: -2.0, ..Default::default() };
    assert!(rms(&run(calm, 0.5, 1.0)) < 1.0e-4);
}

#[test]
fn the_bodywork_muffles_the_outside() {
    let outside = AmbientParams { enabled: true, wind_10m: 12.0, foliage: 1.0, urban: 0.5, ..Default::default() };
    let closed = AmbientParams { inside: true, open: 0.0, ..outside };
    let open = AmbientParams { inside: true, open: 1.0, ..outside };
    let (o, c, d) = (rms(&run(outside, 2.0, 3.0)), rms(&run(closed, 2.0, 3.0)), rms(&run(open, 2.0, 3.0)));
    assert!(c < o * 0.3, "closed {c} vs outside {o}");
    assert!(d > c * 2.0 && d <= o * 1.05, "open {d} closed {c} outside {o}");
}

#[test]
fn stronger_wind_is_louder() {
    let at = |u: f32| rms(&run(AmbientParams { enabled: true, wind_10m: u, urban: 0.0, foliage: 0.0, ..Default::default() }, 3.0, 6.0));
    let (breeze, gale) = (at(4.0), at(16.0));
    assert!(gale > 8.0 * breeze, "{breeze} {gale}");
}

#[test]
fn switching_off_fades_without_a_click() {
    let mut a = Ambient::new(RATE);
    a.set_params(AmbientParams { enabled: true, wind_10m: 15.0, foliage: 1.0, ..Default::default() });
    let mut out = vec![0.0f32; 960];
    for _ in 0..200 {
        out.iter_mut().for_each(|x| *x = 0.0);
        a.render(&mut out, 2, 1.0);
    }
    let before = rms(&out);
    a.set_params(AmbientParams { enabled: false, wind_10m: 15.0, foliage: 1.0, ..Default::default() });
    out.iter_mut().for_each(|x| *x = 0.0);
    a.render(&mut out, 2, 1.0);
    // the first block after the switch still carries most of the sound (a fade, not a cut)
    assert!(rms(&out) > before * 0.5, "{} {before}", rms(&out));
    for _ in 0..100 {
        out.iter_mut().for_each(|x| *x = 0.0);
        a.render(&mut out, 2, 1.0);
    }
    assert_eq!(rms(&out), 0.0);
}

/// The audio thread's budget: the full storm with four wheels must take a small part of
/// real time (it ran at some 2 % of one core when written; the bound leaves room for a
/// slow debug build on a busy test machine).
#[test]
fn the_ambience_is_cheap() {
    let mut p = storm();
    p.roof_rain = true;
    p.inside = true;
    let first = p.wheels[0];
    p.wheels = [first; MAX_WHEELS];
    let mut a = Ambient::new(RATE);
    a.set_params(p);
    let mut out = vec![0.0f32; 1024];
    let t = std::time::Instant::now();
    let secs = 4.0;
    let blocks = (secs * RATE as f32 / 512.0) as usize;
    for _ in 0..blocks {
        out.iter_mut().for_each(|x| *x = 0.0);
        a.render(&mut out, 2, 1.0);
    }
    let share = t.elapsed().as_secs_f32() / secs;
    println!("ambience, storm with four wheels: {:.2} % of one core (48 kHz stereo)", share * 100.0);
    let limit = if cfg!(debug_assertions) { 0.6 } else { 0.05 };
    assert!(share < limit, "{:.1} % of real time", share * 100.0);
}

#[test]
fn surface_ids_of_omsi() {
    assert_eq!(Surface::from_omsi(0), Surface::Asphalt);
    assert_eq!(Surface::from_omsi(2), Surface::Cobble);
    assert_eq!(Surface::from_omsi(5), Surface::Gravel);
    assert_eq!(Surface::from_omsi(8), Surface::Snow);
    assert_eq!(Surface::from_omsi(200), Surface::Asphalt);
    for s in Surface::ALL {
        assert_eq!(Surface::from_omsi(s as u8), s);
    }
}

/// `OMSI_AMBIENT_LEVELS=1 cargo test -p omsi-audio ambient_levels -- --nocapture`: the
/// level of every part in typical situations (the calibration table).
#[test]
fn ambient_levels() {
    if !omsi_cfg::flags::OMSI_AMBIENT_LEVELS.is_set() {
        return;
    }
    let dbfs = |x: f32| 20.0 * x.max(1.0e-9).log10();
    let wheel = |s: Surface, v: f32| WheelInput { speed: v, surface: s, load: 1.0, gain_l: 1.0, gain_r: 1.0, ..Default::default() };
    let mut cases: Vec<(String, AmbientParams)> = Vec::new();
    for s in Surface::ALL {
        let mut p = AmbientParams { enabled: true, urban: 0.0, ..Default::default() };
        p.wheels[0] = wheel(s, 13.9);
        cases.push((format!("{} 50 km/h", s.name()), p));
    }
    let mut p = AmbientParams { enabled: true, urban: 0.0, wetness: 1.0, ..Default::default() };
    p.wheels[0] = WheelInput { water_mm: 1.0, ..wheel(Surface::Asphalt, 13.9) };
    cases.push(("wet asphalt 50".into(), p));
    let mut p = AmbientParams { enabled: true, urban: 0.0, ..Default::default() };
    p.wheels[0] = WheelInput { puddle_mm: 30.0, ..wheel(Surface::Asphalt, 8.0) };
    cases.push(("puddle 30 km/h".into(), p));
    for u in [3.0, 8.0, 19.0] {
        cases.push((format!("wind {u} town"), AmbientParams { enabled: true, wind_10m: u, roughness: 1.0, urban: 1.0, traffic: 0.0, hour: 3.0, ..Default::default() }));
        cases.push((format!("wind {u} field+trees"), AmbientParams { enabled: true, wind_10m: u, roughness: 0.1, urban: 0.0, foliage: 1.0, ..Default::default() }));
    }
    for r in [1.0, 4.0, 12.0] {
        cases.push((format!("rain {r} outside"), AmbientParams { enabled: true, rain_mm_h: r, urban: 0.0, wetness: 1.0, ..Default::default() }));
        cases.push((format!("rain {r} inside roof"), AmbientParams { enabled: true, rain_mm_h: r, urban: 0.0, inside: true, roof_rain: true, bus_speed: 10.0, ..Default::default() }));
    }
    cases.push(("game storm".into(), AmbientParams { enabled: true, wind_10m: 18.9, roughness: 0.65, foliage: 0.21, urban: 0.52, rain_mm_h: 2.4, wetness: 1.0, sun_elevation: 57.0, hour: 12.0, temperature: 12.0, ..Default::default() }));
    cases.push(("city noon".into(), AmbientParams { enabled: true, urban: 1.0, hour: 12.0, ..Default::default() }));
    cases.push(("city 3 am".into(), AmbientParams { enabled: true, urban: 1.0, hour: 3.0, ..Default::default() }));
    cases.push(("crickets".into(), AmbientParams { enabled: true, urban: 0.0, foliage: 1.0, sun_elevation: -20.0, day_of_year: 205.0, temperature: 21.0, hour: 23.0, ..Default::default() }));
    cases.push(("thunder storm".into(), AmbientParams { enabled: true, urban: 0.0, rain_mm_h: 12.0, convective: true, ..Default::default() }));
    cases.push(("inside flow 50".into(), AmbientParams { enabled: true, urban: 0.0, inside: true, air_speed: 13.9, ..Default::default() }));
    for (name, p) in cases {
        let mut a = Ambient::new(RATE);
        a.set_params(p);
        let mut out = vec![0.0f32; 960];
        let (mut peak, mut sum, mut n) = (0.0f32, 0.0f64, 0usize);
        let mut parts = [0.0f32; PARTS.len()];
        for k in 0..(120 * 100) {
            out.iter_mut().for_each(|x| *x = 0.0);
            a.render(&mut out, 2, 1.0);
            if k > 300 {
                peak = out.iter().fold(peak, |m, v| m.max(v.abs()));
                sum += out.iter().map(|v| (*v as f64).powi(2)).sum::<f64>();
                n += out.len();
                for (q, l) in parts.iter_mut().zip(a.levels()) {
                    *q = q.max(l);
                }
            }
        }
        let rms = (sum / n as f64).sqrt() as f32;
        let detail: Vec<String> = PARTS.iter().zip(parts).filter(|(_, l)| *l > 1.0e-5).map(|(k, l)| format!("{k} {:.0}", dbfs(l))).collect();
        println!("{name:24} rms {:6.1} dBFS  peak {:6.1} dBFS   ({})", dbfs(rms), dbfs(peak), detail.join(", "));
    }
}
