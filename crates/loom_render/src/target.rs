//! The render targets: the offscreen target every frame is drawn into, and
//! the window surface it is blitted onto.

use crate::gpu::init_step;
use crate::{COLOR_FORMAT, DEPTH_FORMAT, RenderError};

/// Offscreen color + depth target that every frame is drawn into.
pub(crate) struct Offscreen {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) color: wgpu::Texture,
    pub(crate) color_view: wgpu::TextureView,
    pub(crate) depth_view: wgpu::TextureView,
}

impl Offscreen {
    pub(crate) fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let color = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen color"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COLOR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        Self {
            width,
            height,
            color_view: color.create_view(&Default::default()),
            depth_view: depth.create_view(&Default::default()),
            color,
        }
    }
}

/// Window output: the surface plus the blit pass that copies the offscreen
/// target onto it.
pub(crate) struct Presenter {
    pub(crate) surface: wgpu::Surface<'static>,
    pub(crate) config: wgpu::SurfaceConfiguration,
    /// sRGB view format used when rendering into the surface texture.
    pub(crate) view_format: wgpu::TextureFormat,
    pub(crate) pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    /// Binds the current offscreen color view; rebuilt on resize.
    pub(crate) bind_group: Option<wgpu::BindGroup>,
    /// `config` changed since the surface was last configured. Configuring
    /// is deferred to the next acquire: on Vulkan, destroying a swapchain
    /// that was configured but never presented kills the process (exit code
    /// 2173) when the window closes before the first frame.
    needs_configure: bool,
}

impl Presenter {
    pub(crate) fn new(
        device: &wgpu::Device,
        adapter: &wgpu::Adapter,
        surface: wgpu::Surface<'static>,
    ) -> Result<Self, RenderError> {
        let caps = init_step("get_capabilities", || surface.get_capabilities(adapter));
        let format = caps
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| caps.formats.first().copied())
            .ok_or(RenderError::SurfaceUnsupported)?;
        let alpha_mode = caps
            .alpha_modes
            .first()
            .copied()
            .ok_or(RenderError::SurfaceUnsupported)?;
        // Always present through an sRGB view so the blit re-encodes the
        // linear values it reads from the offscreen target.
        let view_format = format.add_srgb_suffix();
        let view_formats = if view_format == format {
            vec![]
        } else {
            vec![view_format]
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: 0,
            height: 0,
            desired_maximum_frame_latency: 2,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats,
        };
        log::info!("surface format {format:?} (view {view_format:?}), alpha {alpha_mode:?}");

        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/blit.wgsl"));
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blit"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(view_format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });

        Ok(Self {
            surface,
            config,
            view_format,
            pipeline,
            bind_group_layout,
            bind_group: None,
            needs_configure: false,
        })
    }

    pub(crate) fn resize(&mut self, device: &wgpu::Device, offscreen: &Offscreen) {
        self.config.width = offscreen.width;
        self.config.height = offscreen.height;
        self.needs_configure = true;
        self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blit"),
            layout: &self.bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&offscreen.color_view),
            }],
        }));
    }

    /// Configures the surface if needed, then acquires the next surface
    /// texture. Returns `None` (after reconfiguring when needed) if this
    /// frame cannot be presented.
    pub(crate) fn acquire(
        &mut self,
        device: &wgpu::Device,
    ) -> Option<(wgpu::SurfaceTexture, bool)> {
        // Only reached with a bind group, i.e. after a non-zero resize, so
        // the surface is never configured with a zero size.
        if self.needs_configure {
            self.surface.configure(device, &self.config);
            self.needs_configure = false;
        }
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => Some((t, false)),
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => Some((t, true)),
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                log::debug!("surface outdated or lost; reconfiguring");
                self.surface.configure(device, &self.config);
                None
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => None,
            wgpu::CurrentSurfaceTexture::Validation => {
                log::warn!("surface validation error; skipping present");
                None
            }
        }
    }
}
