//! openOMSI on Android: the NativeActivity's `android_main`.
//!
//! A phone runs one program in one window, so the launcher and the game share both: the
//! launcher hands the game's command line over (`omsi_launcher_lib::launch` keeps it
//! instead of starting a process), the window goes to the game, and when the session ends
//! (Escape, the menu's Quit) the window comes back to the launcher, which is kept as it
//! was. The app's own data (settings, profiles, sessions) lives in its private folder
//! (`HOME`); the original game, the mods and the screenshots are on the shared storage in
//! `openOMSI/`, where a cable or a file manager reaches them.
//!
//! The Java side (`android/java/.../OmsiActivity.java`) only adds what NativeActivity
//! lacks: the full screen without the system bars, the screen kept on, asking for access
//! to the shared storage, and the vibration.

use super::*;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use winit::platform::android::activity::AndroidApp;
use winit::platform::android::EventLoopBuilderExtAndroid;

/// Where the app keeps what a person puts on the phone for it.
pub const SHARED: &str = "/storage/emulated/0/openOMSI";

#[no_mangle]
fn android_main(app: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("openOMSI"));
    // an error is also written where a person finds it without a computer:
    // openOMSI/crash.log (or Android/data/org.openomsi.game/files/crash.log)
    let crash_files: Vec<PathBuf> = [Some(PathBuf::from(SHARED)), app.external_data_path()].into_iter().flatten().map(|d| d.join("crash.log")).collect();
    std::panic::set_hook(Box::new(move |info| {
        let text = format!("the game stopped on an error (build {BUILD}): {info}\n{}", std::backtrace::Backtrace::force_capture());
        log::error!("{text}");
        for f in &crash_files {
            let _ = std::fs::write(f, &text);
        }
    }));
    log::info!("openOMSI {VERSION} for Android, build {BUILD}");
    // the Java activity (OmsiActivity) for the calls into it: ndk_context's context is the
    // Application, which has none of the activity's methods
    ACTIVITY.store(app.activity_as_ptr(), Ordering::Relaxed);
    // the app's own folder is the home of settings.cfg, launcher.json, the profiles
    if let Some(home) = app.internal_data_path() {
        std::env::set_var("HOME", &home);
    }
    // the content folder (mods, archives, screenshots): on the shared storage when the
    // app may write there, else in the app's own folder on it
    let shared = PathBuf::from(SHARED);
    let content = if std::fs::create_dir_all(&shared).is_ok() && is_writable(&shared) {
        shared
    } else {
        log::warn!("{SHARED} cannot be written (no access to the storage yet): the content folder is the app's own");
        app.external_data_path().unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()))
    };
    std::env::set_var("OMSI_CONTENT", &content);
    let _ = std::fs::write(content.join("README.txt"), README);
    // `openOMSI/env.txt`: the OMSI_* switches a computer takes from its environment, one
    // `NAME=value` a line (a phone has no environment to set; for looking into problems)
    if let Ok(t) = std::fs::read_to_string(content.join("env.txt")) {
        for line in t.lines() {
            if let Some((k, v)) = line.trim().split_once('=') {
                let k = k.trim();
                if k.starts_with("OMSI_") && !k.contains(char::is_whitespace) {
                    log::info!("env.txt: {k}");
                    std::env::set_var(k, v.trim());
                }
            }
        }
    }
    log::info!("home {:?}, content {}", std::env::var_os("HOME"), content.display());
    omsi_cfg::migrate_legacy_data_dir();
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1) % 1_000_000_000;
    omsi_script::set_session_seed(seed);

    let event_loop = match EventLoop::builder().with_android_app(app).build() {
        Ok(e) => e,
        Err(e) => {
            log::error!("no event loop: {e}");
            return;
        }
    };
    let mut shell = Shell { launcher: None, game: None, instance: None };
    if let Err(e) = event_loop.run_app(&mut shell) {
        log::error!("{e}");
    }
    lan_mods::clean_up();
    // (the activity ends with the program)
    std::process::exit(0);
}

fn is_writable(dir: &Path) -> bool {
    let probe = dir.join(".openomsi-write-test");
    let ok = std::fs::write(&probe, b"x").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

const README: &str = "openOMSI\n\
\n\
Put a complete copy of OMSI 2 (the folder with Omsi.exe, maps and Vehicles in it) here as\n\
\"OMSI 2\", e.g. openOMSI/OMSI 2, and choose it in the launcher under Setup.\n\
Mods: copy them into openOMSI/Mods (they are installed when the launcher opens), or install\n\
a folder or a .zip from the launcher's Mods page. Screenshots are written to OMSI 2/Screenshots.\n";

/// The launcher, or the game in the launcher's window.
struct Shell {
    launcher: Option<Box<launcher::Launcher>>,
    game: Option<Box<App>>,
    instance: Option<()>,
}

impl Shell {
    fn launcher(&mut self) -> &mut launcher::Launcher {
        if self.launcher.is_none() {
            // the original installation and the content roots, as a bare start finds them
            let args = Args::parse_from(["openomsi"]);
            if let Err(e) = prepare(args, true) {
                log::error!("{e:#}");
            }
            launcher_statics();
            self.instance = Some(());
            self.launcher = Some(Box::new(launcher::Launcher::new(graphics_instance_for_launcher())));
        }
        self.launcher.as_mut().unwrap()
    }

    /// After every event: a game the launcher asked for starts, a game that ended gives
    /// the window back.
    fn switch(&mut self, event_loop: &ActiveEventLoop) {
        if self.game.is_some() {
            if !crate::platform::take_leave() {
                return;
            }
            let mut game = self.game.take().unwrap();
            game.exiting(event_loop);
            let window = game.window.take();
            drop(game);
            lan_mods::clean_up();
            log::info!("session ended: back to the launcher");
            let l = self.launcher();
            if let Some(w) = window {
                l.adopt_window(w);
            }
            l.resumed(event_loop);
            return;
        }
        let Some(line) = omsi_launcher_lib::take_in_process_launch() else { return };
        log::info!("starting the game: {}", line.join(" "));
        let argv: Vec<String> = std::iter::once("openomsi".to_string()).chain(line).collect();
        let args = match Args::try_parse_from(&argv) {
            Ok(a) => a,
            Err(e) => {
                log::error!("the launcher's command line: {e}");
                return;
            }
        };
        let game = prepare(args, false).and_then(|p| match p {
            Some((args, server)) => make_app(args, server),
            None => Ok(None),
        });
        let mut app = match game {
            Ok(Some(app)) => app,
            Ok(None) => return,
            Err(e) => {
                log::error!("the game could not start: {e:#}");
                return;
            }
        };
        let window = self.launcher().release_window();
        app.create_window(event_loop, window);
        self.game = Some(Box::new(app));
    }
}

impl ApplicationHandler for Shell {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        match self.game.as_mut() {
            Some(g) => g.resumed(event_loop),
            None => self.launcher().resumed(event_loop),
        }
        self.switch(event_loop);
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        match self.game.as_mut() {
            Some(g) => g.suspended(event_loop),
            None => self.launcher().suspended(event_loop),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        match self.game.as_mut() {
            Some(g) => g.window_event(event_loop, id, event),
            None => self.launcher().window_event(event_loop, id, event),
        }
        self.switch(event_loop);
    }

    fn device_event(&mut self, event_loop: &ActiveEventLoop, id: winit::event::DeviceId, event: DeviceEvent) {
        if let Some(g) = self.game.as_mut() {
            g.device_event(event_loop, id, event);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        match self.game.as_mut() {
            Some(g) => {
                event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
                g.about_to_wait(event_loop)
            }
            None => self.launcher().about_to_wait(event_loop),
        }
        self.switch(event_loop);
    }

    fn memory_warning(&mut self, _event_loop: &ActiveEventLoop) {
        log::warn!("the system is short of memory");
        crate::memory::release_free_memory();
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(g) = self.game.as_mut() {
            g.exiting(event_loop);
        }
    }
}

// --- the phone's tilt as a steering wheel ----------------------------------------------

static TILT_ON: AtomicBool = AtomicBool::new(false);
/// The latest turn (-1 .. 1) as f32 bits, and whether the sensor gave one yet.
static TILT: AtomicU32 = AtomicU32::new(0);
static TILT_SEEN: AtomicBool = AtomicBool::new(false);
static TILT_THREAD: std::sync::Once = std::sync::Once::new();

pub(crate) fn tilt() -> Option<f32> {
    (TILT_ON.load(Ordering::Relaxed) && TILT_SEEN.load(Ordering::Relaxed)).then(|| f32::from_bits(TILT.load(Ordering::Relaxed)))
}

pub(crate) fn set_tilt(on: bool) {
    TILT_ON.store(on, Ordering::Relaxed);
    if on {
        TILT_THREAD.call_once(|| {
            let _ = std::thread::Builder::new().name("tilt".into()).spawn(tilt_thread);
        });
    }
}

#[repr(C)]
struct SensorEvent {
    version: i32,
    sensor: i32,
    kind: i32,
    reserved0: i32,
    timestamp: i64,
    data: [f32; 16],
    flags: u32,
    reserved1: [i32; 3],
}

#[link(name = "android")]
extern "C" {
    fn ASensorManager_getInstanceForPackage(package: *const std::ffi::c_char) -> *mut std::ffi::c_void;
    fn ASensorManager_getDefaultSensor(manager: *mut std::ffi::c_void, kind: i32) -> *const std::ffi::c_void;
    fn ASensorManager_createEventQueue(manager: *mut std::ffi::c_void, looper: *mut std::ffi::c_void, ident: i32, callback: *const std::ffi::c_void, data: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    fn ASensorEventQueue_enableSensor(queue: *mut std::ffi::c_void, sensor: *const std::ffi::c_void) -> i32;
    fn ASensorEventQueue_disableSensor(queue: *mut std::ffi::c_void, sensor: *const std::ffi::c_void) -> i32;
    fn ASensorEventQueue_setEventRate(queue: *mut std::ffi::c_void, sensor: *const std::ffi::c_void, usec: i32) -> i32;
    fn ASensorEventQueue_getEvents(queue: *mut std::ffi::c_void, events: *mut SensorEvent, count: usize) -> isize;
    fn ALooper_prepare(opts: i32) -> *mut std::ffi::c_void;
    fn ALooper_pollOnce(timeout_ms: i32, fd: *mut i32, events: *mut i32, data: *mut *mut std::ffi::c_void) -> i32;
}

/// Reads the accelerometer while tilt steering is on (a looper of its own, so the
/// activity's does not see the sensor's events).
fn tilt_thread() {
    const ACCELEROMETER: i32 = 1;
    // SAFETY: the NDK's sensor API used as documented, all on this one thread; the queue and
    // the sensor live as long as the thread (the program)
    unsafe {
        let looper = ALooper_prepare(0);
        let manager = ASensorManager_getInstanceForPackage(c"org.openomsi.game".as_ptr());
        if manager.is_null() {
            log::warn!("tilt steering: no sensor manager");
            return;
        }
        let sensor = ASensorManager_getDefaultSensor(manager, ACCELEROMETER);
        if sensor.is_null() {
            log::warn!("tilt steering: this device has no accelerometer");
            return;
        }
        let queue = ASensorManager_createEventQueue(manager, looper, 3, std::ptr::null(), std::ptr::null_mut());
        if queue.is_null() {
            log::warn!("tilt steering: no sensor queue");
            return;
        }
        let mut enabled = false;
        let mut smooth = 0.0f32;
        let mut events: Vec<SensorEvent> = (0..16).map(|_| std::mem::zeroed()).collect();
        loop {
            let on = TILT_ON.load(Ordering::Relaxed);
            if on != enabled {
                if on {
                    ASensorEventQueue_enableSensor(queue, sensor);
                    ASensorEventQueue_setEventRate(queue, sensor, 16_000);
                } else {
                    ASensorEventQueue_disableSensor(queue, sensor);
                    TILT_SEEN.store(false, Ordering::Relaxed);
                }
                enabled = on;
            }
            if !on {
                std::thread::sleep(std::time::Duration::from_millis(200));
                continue;
            }
            ALooper_pollOnce(100, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
            loop {
                let n = ASensorEventQueue_getEvents(queue, events.as_mut_ptr(), events.len());
                if n <= 0 {
                    break;
                }
                for e in &events[..n as usize] {
                    // held across (landscape either way round): gravity lies along the
                    // device's x axis, and turning it like a wheel moves it into y
                    let (ax, ay) = (e.data[0], e.data[1]);
                    let angle = (ay * ax.signum()).atan2(ax.abs()).to_degrees();
                    let dead = 2.5;
                    let a = if angle.abs() < dead { 0.0 } else { angle - dead * angle.signum() };
                    let turn = (a / 45.0).clamp(-1.0, 1.0);
                    smooth += (turn - smooth) * 0.35;
                    TILT.store(smooth.to_bits(), Ordering::Relaxed);
                    TILT_SEEN.store(true, Ordering::Relaxed);
                }
            }
        }
    }
}

// --- the Java side ---------------------------------------------------------------------

/// Call a `void name(int)` method of the activity (OmsiActivity.java).
/// The NativeActivity (`OmsiActivity`) instance, set in `android_main`.
static ACTIVITY: std::sync::atomic::AtomicPtr<std::ffi::c_void> = std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

/// The activity as a JNI object (the context ndk_context gives when it was not set).
fn activity_ptr() -> *mut std::ffi::c_void {
    let a = ACTIVITY.load(Ordering::Relaxed);
    if a.is_null() {
        ndk_context::android_context().context()
    } else {
        a
    }
}

fn call_activity_int(name: &str, arg: i32) -> Option<()> {
    let ctx = ndk_context::android_context();
    // SAFETY: the VM ndk_context was given and the activity android-activity holds live as
    // long as the program
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    let activity = unsafe { jni::objects::JObject::from_raw(activity_ptr().cast()) };
    let r = env.call_method(&activity, name, "(I)V", &[jni::objects::JValue::Int(arg)]);
    if r.is_err() {
        let _ = env.exception_clear();
    }
    // (the activity is not ours to delete: it was lent)
    std::mem::forget(activity);
    Some(())
}

pub(crate) fn vibrate(ms: u32) {
    static OFF: AtomicBool = AtomicBool::new(false);
    if OFF.load(Ordering::Relaxed) {
        return;
    }
    if call_activity_int("vibrate", ms as i32).is_none() {
        OFF.store(true, Ordering::Relaxed);
    }
}

/// Run `f` with the Java environment and the activity (None when Java is out of reach or
/// the call threw).
fn with_activity<R>(f: impl FnOnce(&mut jni::JNIEnv, &jni::objects::JObject) -> jni::errors::Result<R>) -> Option<R> {
    let ctx = ndk_context::android_context();
    // SAFETY: as in `call_activity_int`
    let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    let activity = unsafe { jni::objects::JObject::from_raw(activity_ptr().cast()) };
    let r = f(&mut env, &activity);
    if let Err(e) = &r {
        log::warn!("Java call failed: {e}");
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
    std::mem::forget(activity);
    r.ok()
}

/// Hand the downloaded APK to the system's package installer (`OmsiActivity.installApk`):
/// the system asks the player, `install_status` tells what they answered.
pub(crate) fn install_apk(path: &std::path::Path) -> anyhow::Result<()> {
    let p = path.to_string_lossy().to_string();
    with_activity(|env, activity| {
        let s = env.new_string(&p)?;
        env.call_method(activity, "installApk", "(Ljava/lang/String;)V", &[(&s).into()])?;
        Ok(())
    })
    .ok_or_else(|| anyhow::anyhow!("the system's package installer could not be reached"))
}

/// The package installer's answer so far: 0 nothing yet, 1 asking the player, 2 installed,
/// 3 cancelled, 4 failed (with the system's message), 5 waiting for "Install unknown apps",
/// 6 that permission refused.
pub(crate) fn install_status() -> Option<(i32, String)> {
    with_activity(|env, activity| {
        let code = env.call_method(activity, "getInstallStatus", "()I", &[])?.i()?;
        let msg = env.call_method(activity, "getInstallMessage", "()Ljava/lang/String;", &[])?.l()?;
        let msg: String = if msg.is_null() { String::new() } else { env.get_string(&jni::objects::JString::from(msg))?.into() };
        Ok((code, msg))
    })
}

/// A web page in the phone's browser.
pub(crate) fn open_url(url: &str) {
    let u = url.to_string();
    let _ = with_activity(|env, activity| {
        let s = env.new_string(&u)?;
        env.call_method(activity, "openUrl", "(Ljava/lang/String;)V", &[(&s).into()])?;
        Ok(())
    });
}
