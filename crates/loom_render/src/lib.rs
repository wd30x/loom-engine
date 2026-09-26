//! Renderer built directly on wgpu.
//!
//! Every frame is drawn into an offscreen color target. Window mode then
//! blits that target to the surface; headless mode reads it back to CPU
//! memory. The two paths share all drawing code. See `docs/architecture.md`.

mod camera;
mod cube;

use std::sync::mpsc;

use glam::Mat4;
use wgpu::util::DeviceExt;

pub use camera::Camera;

/// Format of the offscreen color target. Readback returns its bytes as-is,
/// i.e. sRGB-encoded RGBA8.
pub const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Clear color in linear space.
const CLEAR_COLOR: wgpu::Color = wgpu::Color {
    r: 0.02,
    g: 0.02,
    b: 0.03,
    a: 1.0,
};

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("no suitable GPU adapter: {0}")]
    Adapter(#[from] wgpu::RequestAdapterError),
    #[error("failed to create GPU device: {0}")]
    Device(#[from] wgpu::RequestDeviceError),
    #[error("failed to create surface: {0}")]
    CreateSurface(#[from] wgpu::CreateSurfaceError),
    #[error("the surface is not supported by the selected GPU adapter")]
    SurfaceUnsupported,
    #[error("render size {width}x{height} exceeds the GPU limit of {max} pixels per side")]
    SizeTooLarge { width: u32, height: u32, max: u32 },
    #[error("cannot read back pixels: the render target has zero size")]
    ZeroSize,
    #[error("waiting for the GPU failed: {0}")]
    Poll(#[from] wgpu::PollError),
    #[error("failed to map the readback buffer: {0}")]
    Map(#[from] wgpu::BufferAsyncError),
    #[error("failed to access the mapped readback buffer: {0}")]
    MapRange(#[from] wgpu::MapRangeError),
    #[error("the readback buffer map callback never ran")]
    MapCallbackLost,
}

/// GPU adapter description, for `loom info` and run reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterSummary {
    pub name: String,
    /// wgpu backend name, e.g. `Dx12`, `Vulkan`.
    pub backend: String,
    pub driver: String,
    /// e.g. `DiscreteGpu`, `IntegratedGpu`.
    pub device_type: String,
}

impl AdapterSummary {
    fn from_info(info: &wgpu::AdapterInfo) -> Self {
        Self {
            name: info.name.clone(),
            backend: format!("{:?}", info.backend),
            driver: info.driver.clone(),
            device_type: format!("{:?}", info.device_type),
        }
    }
}

/// Picks the adapter a headless renderer would use, without creating a device.
pub fn probe_adapter() -> Result<AdapterSummary, RenderError> {
    let instance = new_instance();
    let adapter = pollster::block_on(instance.request_adapter(&adapter_options(None)))?;
    Ok(AdapterSummary::from_info(&adapter.get_info()))
}

fn new_instance() -> wgpu::Instance {
    // `_from_env` lets WGPU_BACKEND etc. override the defaults for debugging.
    wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env())
}

fn adapter_options<'a>(
    surface: Option<&'a wgpu::Surface<'a>>,
) -> wgpu::RequestAdapterOptions<'a, 'a> {
    wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: surface,
        ..Default::default()
    }
}

/// Offscreen color + depth target that every frame is drawn into.
struct Offscreen {
    width: u32,
    height: u32,
    color: wgpu::Texture,
    color_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
}

impl Offscreen {
    fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
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
struct Presenter {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    /// sRGB view format used when rendering into the surface texture.
    view_format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    /// Binds the current offscreen color view; rebuilt on resize.
    bind_group: Option<wgpu::BindGroup>,
}

impl Presenter {
    fn new(
        device: &wgpu::Device,
        adapter: &wgpu::Adapter,
        surface: wgpu::Surface<'static>,
    ) -> Result<Self, RenderError> {
        let caps = surface.get_capabilities(adapter);
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
        })
    }

    fn resize(&mut self, device: &wgpu::Device, offscreen: &Offscreen) {
        self.config.width = offscreen.width;
        self.config.height = offscreen.height;
        self.surface.configure(device, &self.config);
        self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blit"),
            layout: &self.bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&offscreen.color_view),
            }],
        }));
    }

    /// Acquires the next surface texture. Returns `None` (after
    /// reconfiguring when needed) if this frame cannot be presented.
    fn acquire(&mut self, device: &wgpu::Device) -> Option<(wgpu::SurfaceTexture, bool)> {
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

/// The renderer. Draws a single mesh (the cube) with a caller-supplied model
/// matrix, into an offscreen target, and optionally presents it to a surface.
pub struct Renderer {
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    camera: Camera,
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    /// Requested size; `(0, 0)` while minimized.
    size: (u32, u32),
    /// `None` while the size is zero.
    offscreen: Option<Offscreen>,
    presenter: Option<Presenter>,
}

impl Renderer {
    /// Creates a renderer without any surface. Frames can only be read back.
    pub fn new_headless(width: u32, height: u32) -> Result<Self, RenderError> {
        let instance = new_instance();
        let adapter = pollster::block_on(instance.request_adapter(&adapter_options(None)))?;
        let mut renderer = Self::with_adapter(adapter)?;
        renderer.resize(width, height)?;
        Ok(renderer)
    }

    /// Creates a renderer that presents to `target` (e.g. an `Arc` of a
    /// window) with the given initial size in physical pixels.
    pub fn new_with_surface(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
    ) -> Result<Self, RenderError> {
        let instance = new_instance();
        let surface = instance.create_surface(target)?;
        let adapter =
            pollster::block_on(instance.request_adapter(&adapter_options(Some(&surface))))?;
        let mut renderer = Self::with_adapter(adapter)?;
        renderer.presenter = Some(Presenter::new(
            &renderer.device,
            &renderer.adapter,
            surface,
        )?);
        renderer.resize(width, height)?;
        Ok(renderer)
    }

    fn with_adapter(adapter: wgpu::Adapter) -> Result<Self, RenderError> {
        let info = adapter.get_info();
        log::info!(
            "GPU adapter: {} ({:?}, driver {:?})",
            info.name,
            info.backend,
            info.driver
        );
        let limits = wgpu::Limits {
            max_texture_dimension_2d: adapter.limits().max_texture_dimension_2d,
            ..Default::default()
        };
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("loom"),
                required_limits: limits,
                ..Default::default()
            }))?;

        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/mesh.wgsl"));
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mesh uniforms"),
            size: std::mem::size_of::<Mat4>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mesh"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mesh"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mesh"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(cube::Vertex::layout())],
            },
            primitive: wgpu::PrimitiveState {
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(COLOR_FORMAT.into())],
            }),
            multiview_mask: None,
            cache: None,
        });

        let (vertices, indices) = cube::cube_mesh();
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cube indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Ok(Self {
            adapter,
            device,
            queue,
            camera: Camera::default(),
            pipeline,
            uniform_buffer,
            uniform_bind_group,
            vertex_buffer,
            index_buffer,
            index_count: u32::try_from(indices.len()).expect("cube has 36 indices"),
            size: (0, 0),
            offscreen: None,
            presenter: None,
        })
    }

    pub fn adapter_summary(&self) -> AdapterSummary {
        AdapterSummary::from_info(&self.adapter.get_info())
    }

    /// Current render size in pixels; `(0, 0)` while minimized.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Resizes the offscreen target (and surface, if any). A zero width or
    /// height is valid and means "minimized": rendering is skipped until the
    /// next non-zero resize.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), RenderError> {
        if width == 0 || height == 0 {
            self.size = (0, 0);
            self.offscreen = None;
            if let Some(p) = &mut self.presenter {
                p.bind_group = None;
            }
            return Ok(());
        }
        let max = self.device.limits().max_texture_dimension_2d;
        if width > max || height > max {
            return Err(RenderError::SizeTooLarge { width, height, max });
        }
        if self
            .offscreen
            .as_ref()
            .is_some_and(|o| (o.width, o.height) == (width, height))
        {
            return Ok(());
        }
        let offscreen = Offscreen::new(&self.device, width, height);
        if let Some(p) = &mut self.presenter {
            p.resize(&self.device, &offscreen);
        }
        self.size = (width, height);
        self.offscreen = Some(offscreen);
        Ok(())
    }

    /// Draws one frame with the given model matrix, and presents it if a
    /// surface is attached. Returns `false` if the frame was skipped because
    /// the size is zero.
    pub fn render(&mut self, model: Mat4) -> Result<bool, RenderError> {
        let Some(offscreen) = &self.offscreen else {
            return Ok(false);
        };
        let aspect = offscreen.width as f32 / offscreen.height as f32;
        let mvp = self.camera.view_projection(aspect) * model;
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&mvp));

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mesh"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &offscreen.color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(CLEAR_COLOR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &offscreen.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..self.index_count, 0, 0..1);
        }

        let mut frame = None;
        if let Some(presenter) = &mut self.presenter
            && let Some(bind_group) = presenter.bind_group.clone()
            && let Some((texture, suboptimal)) = presenter.acquire(&self.device)
        {
            let view = texture.texture.create_view(&wgpu::TextureViewDescriptor {
                format: Some(presenter.view_format),
                ..Default::default()
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("blit"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&presenter.pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
            frame = Some((texture, suboptimal));
        }

        self.queue.submit([encoder.finish()]);
        if let Some((texture, suboptimal)) = frame {
            self.queue.present(texture);
            if suboptimal && let Some(presenter) = &mut self.presenter {
                presenter.surface.configure(&self.device, &presenter.config);
            }
        }
        Ok(true)
    }

    /// Reads back the most recently rendered frame as tightly packed,
    /// sRGB-encoded RGBA8 rows (`width * height * 4` bytes, top row first).
    pub fn read_pixels(&self) -> Result<Vec<u8>, RenderError> {
        let offscreen = self.offscreen.as_ref().ok_or(RenderError::ZeroSize)?;
        let (width, height) = (offscreen.width, offscreen.height);
        let row_bytes = width * 4;
        let padded_row_bytes = row_bytes.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;

        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(padded_row_bytes) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("readback"),
            });
        encoder.copy_texture_to_buffer(
            offscreen.color.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row_bytes),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);

        let slice = buffer.slice(..);
        let (tx, rx) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            // The receiver only disappears if read_pixels already returned.
            let _ = tx.send(result);
        });
        self.device.poll(wgpu::PollType::wait_indefinitely())?;
        rx.recv().map_err(|_| RenderError::MapCallbackLost)??;

        let mut pixels = Vec::with_capacity((row_bytes * height) as usize);
        {
            let mapped = slice.get_mapped_range()?;
            for row in mapped.chunks_exact(padded_row_bytes as usize) {
                pixels.extend_from_slice(&row[..row_bytes as usize]);
            }
        }
        buffer.unmap();
        Ok(pixels)
    }
}
