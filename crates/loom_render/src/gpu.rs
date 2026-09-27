//! Creating the wgpu instance and choosing the adapter, and the logged
//! steps of renderer creation and shutdown.

/// The backends used unless `WGPU_BACKEND` overrides them. On Windows only
/// Dx12: with Vulkan, exiting after a window run sometimes hung for about
/// 2 minutes or died with exit code 2173 on this project's machine, while
/// Dx12 never did in the same measurements (`docs/architecture.md` section
/// 3). Elsewhere, wgpu's default.
fn default_backends() -> wgpu::Backends {
    if cfg!(windows) {
        wgpu::Backends::DX12
    } else {
        wgpu::Backends::default()
    }
}

pub(crate) fn new_instance() -> wgpu::Instance {
    // `with_env` lets WGPU_BACKEND (and the flag and backend-option
    // variables) override the defaults, e.g. `WGPU_BACKEND=vulkan` to debug.
    wgpu::Instance::new(
        wgpu::InstanceDescriptor {
            backends: default_backends(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        }
        .with_env(),
    )
}

/// Log prefix of the renderer creation steps.
pub(crate) const INIT: &str = "renderer init";
/// Log prefix of the renderer shutdown steps.
pub(crate) const EXIT: &str = "renderer exit";

/// Runs one step of renderer creation or shutdown (`phase`), logging (at
/// debug level) before it starts and how long it took. If a step hangs, the
/// last line of the log names it.
pub(crate) fn step<T>(phase: &str, name: &str, f: impl FnOnce() -> T) -> T {
    log::debug!("{phase}: {name} ...");
    let start = std::time::Instant::now();
    let out = f();
    log::debug!("{phase}: {name} took {} ms", start.elapsed().as_millis());
    out
}

/// [`step`] of renderer creation.
pub(crate) fn init_step<T>(name: &str, f: impl FnOnce() -> T) -> T {
    step(INIT, name, f)
}

pub(crate) fn adapter_options<'a>(
    surface: Option<&'a wgpu::Surface<'a>>,
) -> wgpu::RequestAdapterOptions<'a, 'a> {
    wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: surface,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    #[test]
    fn default_backend_on_windows_is_dx12_only() {
        assert_eq!(super::default_backends(), wgpu::Backends::DX12);
    }
}
