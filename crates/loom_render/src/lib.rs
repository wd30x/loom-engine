//! Renderer built directly on wgpu.
//!
//! Every frame is drawn into an offscreen color target. Window mode then
//! blits that target to the surface; headless mode reads it back to CPU
//! memory. The two paths share all drawing code. See `docs/architecture.md`.

mod buffers;
mod frame;
mod gpu;
mod mesh;
mod pipelines;
mod target;

use std::sync::mpsc;

use wgpu::util::DeviceExt;

pub use frame::{
    DirectionalLight, DrawItem, Frame, HudDraw, LineDraw, Projection, SpriteDraw, view_projection,
};
use frame::{GpuDraw, GpuGlobals, GpuSprite, HudVertex, LineVertex, hud_vertices, line_vertices};
pub use mesh::MeshKind;

use buffers::{
    DrawBuffer, GpuMesh, INITIAL_DRAW_CAPACITY, INITIAL_HUD_VERTICES, INITIAL_LINE_VERTICES,
    VertexBuffer,
};
use gpu::{EXIT, INIT, adapter_options, init_step, new_instance, step};
use target::{Offscreen, Presenter};

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
    #[error("texture size {width}x{height} must be non-zero and at most {max} pixels per side")]
    TextureSize { width: u32, height: u32, max: u32 },
    #[error("a {width}x{height} RGBA8 texture needs {expected} bytes, got {actual}")]
    TextureData {
        width: u32,
        height: u32,
        expected: usize,
        actual: usize,
    },
    #[error("sprite uses texture {0}, which this renderer did not create")]
    UnknownTexture(u32),
}

/// Handle of a texture created by [`Renderer::create_texture`]; only valid
/// with the renderer that created it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureId(u32);

impl TextureId {
    /// A handle by index, for tests of code that builds frames without a
    /// GPU. [`Renderer::render`] rejects one its renderer did not create.
    #[doc(hidden)]
    pub fn from_index(index: u32) -> Self {
        Self(index)
    }
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

/// The renderer. Draws a [`Frame`] (lit meshes seen from a camera) into an
/// offscreen target, and optionally presents it to a surface.
pub struct Renderer {
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    globals_buffer: wgpu::Buffer,
    draws: DrawBuffer,
    sprite_pipeline: wgpu::RenderPipeline,
    texture_layout: wgpu::BindGroupLayout,
    sprites: DrawBuffer,
    line_pipeline: wgpu::RenderPipeline,
    /// Binds only the globals, for the line pipeline.
    line_bind_group: wgpu::BindGroup,
    lines: VertexBuffer,
    hud_solid_pipeline: wgpu::RenderPipeline,
    hud_textured_pipeline: wgpu::RenderPipeline,
    hud: VertexBuffer,
    /// Bind groups of the textures, each binding one as group 1 of the
    /// sprite pipeline; indexed by [`TextureId`], never removed.
    textures: Vec<wgpu::BindGroup>,
    /// Every mesh uploaded so far, found by `==`: the built-in ones, then
    /// each capsule shape at its first use. Shapes that are no longer drawn
    /// (e.g. after a hot reload changed a capsule) stay; there are only a few.
    meshes: Vec<(MeshKind, GpuMesh)>,
    /// Requested size; `(0, 0)` while minimized.
    size: (u32, u32),
    /// `None` while the size is zero.
    offscreen: Option<Offscreen>,
    presenter: Option<Presenter>,
}

impl Renderer {
    /// Creates a renderer without any surface. Frames can only be read back.
    pub fn new_headless(width: u32, height: u32) -> Result<Self, RenderError> {
        let instance = init_step("create_instance", new_instance);
        let adapter = init_step("request_adapter", || {
            pollster::block_on(instance.request_adapter(&adapter_options(None)))
        })?;
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
        let instance = init_step("create_instance", new_instance);
        let surface = init_step("create_surface", || instance.create_surface(target))?;
        let adapter = init_step("request_adapter", || {
            pollster::block_on(instance.request_adapter(&adapter_options(Some(&surface))))
        })?;
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
        let (device, queue) = init_step("request_device", || {
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("loom"),
                required_limits: limits,
                ..Default::default()
            }))
        })?;
        log::debug!("{INIT}: pipeline and resources ...");
        let resources_start = std::time::Instant::now();

        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/mesh.wgsl"));
        let globals_buffer = pipelines::globals_buffer(&device);
        let bind_group_layout = pipelines::mesh_bind_group_layout(&device);
        let draws = DrawBuffer::new(
            &device,
            &bind_group_layout,
            &globals_buffer,
            "mesh",
            std::mem::size_of::<GpuDraw>(),
            INITIAL_DRAW_CAPACITY,
        );
        let pipeline = pipelines::mesh_pipeline(&device, &shader, &bind_group_layout);

        // Sprites share group 0's layout with meshes (globals, then per-draw
        // storage read by the vertex stage); group 1 is the texture.
        let sprite_shader = device.create_shader_module(wgpu::include_wgsl!("shaders/sprite.wgsl"));
        let texture_layout = pipelines::texture_layout(&device);
        let sprites = DrawBuffer::new(
            &device,
            &bind_group_layout,
            &globals_buffer,
            "sprite",
            std::mem::size_of::<GpuSprite>(),
            INITIAL_DRAW_CAPACITY,
        );
        let sprite_pipeline = pipelines::sprite_pipeline(
            &device,
            &sprite_shader,
            &bind_group_layout,
            &texture_layout,
        );

        // Debug lines: world-space vertices, the globals' view-projection,
        // no depth test and no depth writes, drawn last.
        let line_shader = device.create_shader_module(wgpu::include_wgsl!("shaders/line.wgsl"));
        let (line_bind_group, line_pipeline) =
            pipelines::line_pipeline(&device, &line_shader, &globals_buffer);
        let lines = VertexBuffer::new(
            &device,
            "debug lines",
            std::mem::size_of::<LineVertex>(),
            INITIAL_LINE_VERTICES,
        );

        // HUD: NDC vertices, no bindings, alpha blending, no depth test and
        // no depth writes, drawn between the sprites and the debug lines.
        let hud_shader = device.create_shader_module(wgpu::include_wgsl!("shaders/hud.wgsl"));
        let (hud_solid_pipeline, hud_textured_pipeline) =
            pipelines::hud_pipelines(&device, &hud_shader, &texture_layout);
        let hud = VertexBuffer::new(
            &device,
            "hud",
            std::mem::size_of::<HudVertex>(),
            INITIAL_HUD_VERTICES,
        );

        let meshes = MeshKind::BUILT_IN
            .iter()
            .map(|&kind| (kind, GpuMesh::new(&device, kind)))
            .collect();

        log::debug!(
            "{INIT}: pipeline and resources took {} ms",
            resources_start.elapsed().as_millis()
        );
        Ok(Self {
            adapter,
            device,
            queue,
            pipeline,
            bind_group_layout,
            globals_buffer,
            draws,
            sprite_pipeline,
            texture_layout,
            sprites,
            line_pipeline,
            line_bind_group,
            lines,
            hud_solid_pipeline,
            hud_textured_pipeline,
            hud,
            textures: Vec::new(),
            meshes,
            size: (0, 0),
            offscreen: None,
            presenter: None,
        })
    }

    pub fn adapter_summary(&self) -> AdapterSummary {
        AdapterSummary::from_info(&self.adapter.get_info())
    }

    /// Destroys the renderer one part at a time, logging each step (debug)
    /// like renderer creation does: the surface, then every resource, then
    /// the queue, the device and the adapter. Resources hold the device, so
    /// the device step is where it is really destroyed.
    pub fn shutdown(self) {
        let Self {
            adapter,
            device,
            queue,
            pipeline,
            bind_group_layout,
            globals_buffer,
            draws,
            sprite_pipeline,
            texture_layout,
            sprites,
            line_pipeline,
            line_bind_group,
            lines,
            hud_solid_pipeline,
            hud_textured_pipeline,
            hud,
            textures,
            meshes,
            size: _,
            offscreen,
            presenter,
        } = self;
        step(EXIT, "presenter", || drop(presenter));
        step(EXIT, "resources", || {
            drop((
                offscreen,
                draws,
                sprites,
                (lines, line_bind_group, line_pipeline),
                (hud, hud_solid_pipeline, hud_textured_pipeline),
                textures,
                meshes,
                pipeline,
                sprite_pipeline,
                bind_group_layout,
                texture_layout,
                globals_buffer,
            ));
        });
        step(EXIT, "queue", || drop(queue));
        step(EXIT, "device", || drop(device));
        step(EXIT, "adapter", || drop(adapter));
    }

    /// Uploads a `width` x `height` texture from tightly packed, sRGB-encoded
    /// RGBA8 rows (top row first), for [`SpriteDraw`]s.
    pub fn create_texture(
        &mut self,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<TextureId, RenderError> {
        let max = self.device.limits().max_texture_dimension_2d;
        if width == 0 || height == 0 || width > max || height > max {
            return Err(RenderError::TextureSize { width, height, max });
        }
        let expected = width as usize * height as usize * 4;
        if rgba.len() != expected {
            return Err(RenderError::TextureData {
                width,
                height,
                expected,
                actual: rgba.len(),
            });
        }
        let texture = self.device.create_texture_with_data(
            &self.queue,
            &wgpu::TextureDescriptor {
                label: Some("sprite texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            rgba,
        );
        let view = texture.create_view(&Default::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sprite texture"),
            layout: &self.texture_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            }],
        });
        let id = u32::try_from(self.textures.len()).expect("far fewer than 2^32 textures");
        self.textures.push(bind_group);
        Ok(TextureId(id))
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

    /// Draws one frame, and presents it if a surface is attached. The
    /// projection's aspect ratio follows the current render size. Returns
    /// `false` if the frame was skipped because the size is zero.
    pub fn render(&mut self, frame: &Frame<'_>) -> Result<bool, RenderError> {
        if self.offscreen.is_none() {
            return Ok(false);
        }
        let hud_textures = frame.hud.iter().filter_map(|item| match item {
            HudDraw::Image { texture, .. } => Some(*texture),
            HudDraw::Rect { .. } | HudDraw::Disc { .. } => None,
        });
        for texture in frame.sprites.iter().map(|s| s.texture).chain(hud_textures) {
            let TextureId(id) = texture;
            if self.textures.get(id as usize).is_none() {
                return Err(RenderError::UnknownTexture(id));
            }
        }
        self.draws.reserve(
            &self.device,
            &self.bind_group_layout,
            &self.globals_buffer,
            frame.draws.len(),
        );
        self.sprites.reserve(
            &self.device,
            &self.bind_group_layout,
            &self.globals_buffer,
            frame.sprites.len(),
        );
        for item in frame.draws {
            if !self.meshes.iter().any(|(kind, _)| *kind == item.mesh) {
                log::debug!("uploading mesh {:?}", item.mesh);
                self.meshes
                    .push((item.mesh, GpuMesh::new(&self.device, item.mesh)));
            }
        }
        let Some(offscreen) = &self.offscreen else {
            return Ok(false);
        };
        let aspect = offscreen.width as f32 / offscreen.height as f32;
        let globals = GpuGlobals::new(frame, aspect);
        self.queue
            .write_buffer(&self.globals_buffer, 0, bytemuck::bytes_of(&globals));
        let draws: Vec<GpuDraw> = frame.draws.iter().map(GpuDraw::new).collect();
        if !draws.is_empty() {
            self.queue
                .write_buffer(&self.draws.buffer, 0, bytemuck::cast_slice(&draws));
        }
        let sprites: Vec<GpuSprite> = frame.sprites.iter().map(GpuSprite::new).collect();
        if !sprites.is_empty() {
            self.queue
                .write_buffer(&self.sprites.buffer, 0, bytemuck::cast_slice(&sprites));
        }
        let (hud_vertices, hud_runs) = hud_vertices(frame.hud, offscreen.width, offscreen.height);
        if !hud_vertices.is_empty() {
            self.hud.reserve(&self.device, hud_vertices.len());
            self.queue
                .write_buffer(&self.hud.buffer, 0, bytemuck::cast_slice(&hud_vertices));
        }
        let line_vertices = line_vertices(frame.lines);
        if !line_vertices.is_empty() {
            self.lines.reserve(&self.device, line_vertices.len());
            self.queue
                .write_buffer(&self.lines.buffer, 0, bytemuck::cast_slice(&line_vertices));
        }

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
            pass.set_bind_group(0, &self.draws.bind_group, &[]);
            let mut bound = None;
            for (i, item) in frame.draws.iter().enumerate() {
                let slot = self
                    .meshes
                    .iter()
                    .position(|(kind, _)| *kind == item.mesh)
                    .expect("every mesh of this frame was uploaded above");
                let mesh = &self.meshes[slot].1;
                if bound != Some(slot) {
                    pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                    pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint16);
                    bound = Some(slot);
                }
                // The instance index selects this item's per-draw data.
                let instance = u32::try_from(i).expect("draw count fits in u32");
                pass.draw_indexed(0..mesh.index_count, 0, instance..instance + 1);
            }
            if !frame.sprites.is_empty() {
                pass.set_pipeline(&self.sprite_pipeline);
                pass.set_bind_group(0, &self.sprites.bind_group, &[]);
                // One draw per run of consecutive sprites with the same
                // texture; the instance index selects each one's data.
                let mut start = 0;
                for run in frame.sprites.chunk_by(|a, b| a.texture == b.texture) {
                    let TextureId(id) = run[0].texture;
                    pass.set_bind_group(1, &self.textures[id as usize], &[]);
                    let end = start + u32::try_from(run.len()).expect("sprite count fits in u32");
                    pass.draw(0..6, start..end);
                    start = end;
                }
            }
            if !hud_vertices.is_empty() {
                pass.set_vertex_buffer(0, self.hud.buffer.slice(..));
                // Run by run, in order: the list's order is kept.
                for (texture, range) in hud_runs {
                    match texture {
                        None => pass.set_pipeline(&self.hud_solid_pipeline),
                        Some(TextureId(id)) => {
                            pass.set_pipeline(&self.hud_textured_pipeline);
                            pass.set_bind_group(0, &self.textures[id as usize], &[]);
                        }
                    }
                    pass.draw(range, 0..1);
                }
            }
            if !line_vertices.is_empty() {
                pass.set_pipeline(&self.line_pipeline);
                pass.set_bind_group(0, &self.line_bind_group, &[]);
                pass.set_vertex_buffer(0, self.lines.buffer.slice(..));
                let count =
                    u32::try_from(line_vertices.len()).expect("line vertex count fits in u32");
                pass.draw(0..count, 0..1);
            }
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

#[cfg(test)]
#[path = "hud_gpu_tests.rs"]
mod hud_gpu_tests;
