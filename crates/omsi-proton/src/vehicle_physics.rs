//! Authored rigid-bus data translated to the native vehicle/physics interface.
use crate::{
    ini,
    settings::{number, position, value},
    vehicle::Inspection,
    Ini,
};
use anyhow::{ensure, Context, Result};

pub(crate) struct Axle {
    pub left: [f32; 3],
    pub right: [f32; 3],
    pub radius: f32,
    pub spring: f32,
    pub damper: f32,
    pub driven: bool,
}
pub(crate) struct Physics {
    pub axles: Vec<Axle>,
    pub engine: Ini,
    pub max_steer: f32,
    pub idle: f32,
    pub peak: f32,
    pub torque: f32,
    pub max_rpm: f32,
    pub ratios: Vec<f32>,
    pub reverse: f32,
    pub differential: f32,
    pub shift_up: f32,
    pub shift_down: f32,
    pub brake_torque: f32,
    pub parking_torque: f32,
}
impl Physics {
    pub fn read(info: &Inspection) -> Result<Self> {
        ensure!(
            (1000.0..=100_000.0).contains(&info.mass_kg),
            "vehicle mass must be 1000..100000 kg"
        );
        ensure!(
            ["4x2", "6x2", "6x4"].contains(&info.template.as_str()),
            "unsupported rigid vehicle template {}; articulated parts need their own conversion",
            info.template
        );
        let component = |role: &str| {
            info.components
                .iter()
                .find(|c| c.role == role && c.present)
                .context(format!("missing required {role}"))
        };
        let wheels = ini(&component("wheels1")?.file)?;
        let engine = ini(&component("engine1auto")
            .or_else(|_| component("engine1manual"))?
            .file)?;
        let mut axles = Vec::new();
        for i in 1..=4 {
            let (left, right) = (format!("leftwheel{i}"), format!("rightwheel{i}"));
            if !wheels.contains_key(&left) && !wheels.contains_key(&right) {
                break;
            }
            ensure!(
                wheels.contains_key(&left) && wheels.contains_key(&right),
                "axle {i} needs both wheel definitions"
            );
            let (l, r) = (position(&wheels, &left)?, position(&wheels, &right)?);
            let radius = bounded(&wheels, &left, "radius", 0.5, 0.15, 1.5)?;
            ensure!(
                l[0] < 0. && r[0] > 0. && (l[1] - r[1]).abs() < 0.1 && (l[2] - r[2]).abs() < 0.1,
                "invalid wheel pair on axle {i}"
            );
            ensure!(
                (radius - bounded(&wheels, &right, "radius", radius, 0.15, 1.5)?).abs() < 0.01,
                "different wheel radii on axle {i}"
            );
            axles.push(Axle {
                left: l,
                right: r,
                radius,
                spring: bounded(&wheels, &left, "springrate", 150000., 10000., 2000000.)? / 1000.,
                damper: bounded(&wheels, &left, "damperrate", 12000., 100., 200000.)? / 1000.,
                driven: false,
            });
        }
        let count = (info.template.as_bytes()[0] - b'0') as usize / 2;
        ensure!(
            axles.len() == count && axles.len() >= 2,
            "template and axle count differ"
        );
        axles.sort_by(|a, b| b.left[1].total_cmp(&a.left[1]));
        ensure!(
            axles[0].left[1] - axles.last().unwrap().left[1] > 1.,
            "invalid wheelbase"
        );
        axles[1].driven = true;
        if info.template.ends_with('4') {
            axles[2].driven = true;
        }
        let mut out = Self {
            axles,
            max_steer: bounded(&engine, "steering", "max_steer_angle", 40., 10., 60.)?,
            idle: bounded(&engine, "engine", "idle_rpm", 600., 200., 1500.)?,
            peak: bounded(&engine, "engine", "peak_rpm", 1200., 400., 4000.)?,
            torque: bounded(&engine, "engine", "peak_rpm_torque", 900., 50., 10000.)?,
            max_rpm: bounded(&engine, "engine", "max_rpm", 2500., 1000., 6000.)?,
            ratios: vec![],
            reverse: bounded(
                &engine,
                "automatic_gearbox",
                "reverse_ratio",
                -4.,
                -20.,
                -0.1,
            )?,
            differential: bounded(&engine, "differential", "gearratio", 5., 1., 20.)?,
            shift_up: bounded(
                &engine,
                "automatic_gearbox",
                "gear_up_rpm",
                1800.,
                500.,
                6000.,
            )?,
            shift_down: bounded(
                &engine,
                "automatic_gearbox",
                "gear_down_rpm",
                1000.,
                200.,
                4000.,
            )?,
            brake_torque: bounded(&engine, "brakes", "max_brake_torque", 18000., 100., 100000.)?,
            parking_torque: bounded(&engine, "brakes", "handbrake_torque", 10000., 100., 100000.)?,
            engine,
        };
        let gearbox = if out.engine.contains_key("automatic_gearbox") {
            "automatic_gearbox"
        } else {
            "manual_gearbox"
        };
        let n = number(&out.engine, gearbox, "num_forward_ratios", 6.)?;
        ensure!(
            n.fract() == 0. && (1.0..=16.0).contains(&n),
            "invalid gearbox ratio count"
        );
        for i in 1..=n as usize {
            let key = format!("fwd_ratio_{i}");
            ensure!(
                value(&out.engine, gearbox, &key).is_some(),
                "missing gearbox {key}"
            );
            out.ratios
                .push(bounded(&out.engine, gearbox, &key, 1., 0.1, 20.)?);
        }
        ensure!(
            out.idle < out.peak && out.peak < out.max_rpm && out.shift_down < out.shift_up,
            "invalid engine/shift RPM ordering"
        );
        Ok(out)
    }
    pub fn definition(&self, cfg: &Ini, info: &Inspection) -> Result<String> {
        let center = position(cfg, "collision1")?;
        let size = [
            number(cfg, "collision1", "sizex", 0.)?,
            number(cfg, "collision1", "sizez", 0.)?,
            number(cfg, "collision1", "sizey", 0.)?,
        ];
        ensure!(
            size.iter().all(|v| (0.5..=30.).contains(v)) && center.iter().all(|v| v.abs() < 30.),
            "missing or invalid collision1 box"
        );
        let cog = position(cfg, "poscenterofmass1")?;
        ensure!(
            cog.iter().all(|v| v.abs() < 30.),
            "invalid vehicle centre of mass"
        );
        let rear = self.axles.last().unwrap().left[1];
        let inv = self.max_steer.to_radians().tan() / (self.axles[0].left[1] - rear);
        let mass = info.mass_kg / 1000.;
        // Uniform-box approximation in tonne m², about the authored centre of mass.
        // OMSI orders these as pitch, yaw, roll (its original y-up frame).
        // Omitting this leaves the native rigid body with a near-zero inertia.
        let offset = std::array::from_fn::<_, 3, _>(|i| center[i] - cog[i]);
        let inertia = |a: usize, b: usize| {
            mass * ((size[a] * size[a] + size[b] * size[b]) / 12.
                + offset[a] * offset[a]
                + offset[b] * offset[b])
        };
        let mut moi = [inertia(1, 2), inertia(0, 1), inertia(0, 2)];
        // The native reader treats a pitch inertia >= 5000 as kg m².
        if moi[0] >= 5000. {
            moi.iter_mut().for_each(|v| *v *= 1000.);
        }
        let mut s=format!("[friendlyname]\nProton Bus\n{}\nDefault\n[ai_veh_type]\n2\n[mass]\n{mass}\n[cog]\n{}\n{}\n{}\n[schwerpunkt]\n{}\n[boundingbox]\n{}\n{}\n{}\n{}\n{}\n{}\n[rollwiderstand]\n{}\n[rot_pnt_long]\n{rear}\n[inv_min_turnradius]\n{inv}\n[model]\nModel/model.cfgoi\n[paths]\nModel/paths.cfgoi\n[passengercabin]\nModel/cabin.cfgoi\n[varnamelist]\n1\nScript/variables.cfgoi\n[stringvarnamelist]\n1\nScript/strings.cfgoi\n[script]\n1\nScript/main.cfgoi\n",info.name,cog[0],cog[1],cog[2],cog[2],size[0],size[1],size[2],center[0],center[1],center[2],info.mass_kg*0.009*9.81);
        s.push_str(&format!(
            "[momentofintertia]\n{}\n{}\n{}\n",
            moi[0], moi[1], moi[2]
        ));
        s.push_str(&format!(
            "[set_camera_outside_center]\n{}\n{}\n{}\n",
            center[0], center[1], center[2]
        ));
        for a in &self.axles {
            s.push_str(&format!("[newachse]\nachse_long\n{}\nachse_maxwidth\n{}\nachse_minwidth\n{}\nachse_raddurchmesser\n{}\nachse_feder\n{}\nachse_maxforce\n150\nachse_daempfer\n{}\nachse_antrieb\n{}\n",a.left[1],a.right[0]-a.left[0],(a.right[0]-a.left[0])*0.75,a.radius*2.,a.spring,a.damper,a.driven as u8));
        }
        camera(&mut s, "add_camera_driver", info.driver_camera, 65., 0., 0.);
        for (section, _) in cfg.iter().filter(|(n, _)| n.starts_with("pospassengercam")) {
            camera(
                &mut s,
                "add_camera_pax",
                position(cfg, section)?,
                65.,
                0.,
                0.,
            );
        }
        for (section, yaw) in [
            ("leftmirror", 180.),
            ("rightmirror", 180.),
            ("middlemirror", 180.),
            ("reversecamera", 0.),
        ] {
            if cfg.contains_key(section) {
                camera(
                    &mut s,
                    if section == "reversecamera" {
                        "add_camera_reflexion_static"
                    } else {
                        "add_camera_reflexion"
                    },
                    position(cfg, section)?,
                    60.,
                    yaw + number(cfg, section, "roty", 0.)?,
                    number(cfg, section, "rotx", 0.)?,
                );
            }
        }
        Ok(s)
    }
}
fn camera(s: &mut String, key: &str, p: [f32; 3], fov: f32, yaw: f32, pitch: f32) {
    s.push_str(&format!(
        "[{key}]\n{}\n{}\n{}\n0\n{fov}\n{yaw}\n{pitch}\n",
        p[0], p[1], p[2]
    ));
}
pub(crate) fn bounded(c: &Ini, s: &str, k: &str, d: f32, lo: f32, hi: f32) -> Result<f32> {
    let v = number(c, s, k, d)?;
    ensure!((lo..=hi).contains(&v), "invalid vehicle {s}.{k}: {v}");
    Ok(v)
}
