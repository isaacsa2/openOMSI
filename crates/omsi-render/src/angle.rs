//! ANGLE/D3D11 instance creation shared by the game and the Windows regression test.

#[cfg(any(windows, test))]
const SERIAL_COMPILER_FEATURES: [&str; 3] = [
    "compileJobIsThreadSafe",
    "linkJobIsThreadSafe",
    "alwaysRunLinkSubJobsThreaded",
];

#[cfg(any(windows, test))]
fn serial_compiler_overrides(existing: &str) -> String {
    let mut features: Vec<&str> = existing.split(':').filter(|f| !f.is_empty()).collect();
    for feature in SERIAL_COMPILER_FEATURES {
        if !features.contains(&feature) {
            features.push(feature);
        }
    }
    features.join(":")
}

/// Create an ANGLE instance, keeping shader translation and linking on the calling
/// thread. The packaged ANGLE reproduced a native worker stack overflow on the first
/// scene pipeline in Windows/WARP CI. Its frontend feature overrides select its existing
/// synchronous pools without changing shader contents or rendered effects.
///
/// Call during startup, before any ANGLE display is initialized: ANGLE caches these
/// overrides. Renderer pipeline creation then uses `compile`'s managed stack, rather
/// than relying on the stack size chosen by ANGLE or the host's entry thread.
pub fn instance(mut descriptor: wgpu::InstanceDescriptor) -> wgpu::Instance {
    #[cfg(windows)]
    {
        let existing = std::env::var("ANGLE_FEATURE_OVERRIDES_DISABLED").unwrap_or_default();
        std::env::set_var(
            "ANGLE_FEATURE_OVERRIDES_DISABLED",
            serial_compiler_overrides(&existing),
        );
        log::info!("ANGLE: native shader worker jobs disabled; renderer compilation uses a managed 32 MB stack");
    }
    descriptor.backends = wgpu::Backends::GL;
    descriptor.backend_options.gl.platform = wgpu::GlPlatform::Angle;
    wgpu::Instance::new(descriptor)
}

/// A single worker for the entire pipeline build, not one thread per pipeline. With
/// ANGLE's native jobs disabled, translation and D3D compilation inherit this stack.
/// Reserving stack address space does not commit all 32 MB at thread creation.
pub(crate) fn compile<F, T>(operation: F) -> T
where
    F: FnOnce() -> T + Send,
    T: Send,
{
    #[cfg(windows)]
    {
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .name("ANGLE shader compiler".into())
                .stack_size(32 * 1024 * 1024)
                .spawn_scoped(scope, operation)
                .expect("ANGLE shader compiler thread")
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
        })
    }
    #[cfg(not(windows))]
    operation()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_compiler_overrides_use_angles_colon_separator() {
        assert_eq!(
            serial_compiler_overrides(""),
            "compileJobIsThreadSafe:linkJobIsThreadSafe:alwaysRunLinkSubJobsThreaded"
        );
    }

    #[test]
    fn serial_compiler_overrides_preserve_user_features_and_do_not_duplicate() {
        let original = "otherWorkaround:compileJobIsThreadSafe";
        let combined = serial_compiler_overrides(original);
        assert!(combined.starts_with(original));
        assert_eq!(combined.matches("compileJobIsThreadSafe").count(), 1);
        assert_eq!(serial_compiler_overrides(&combined), combined);
    }
}
