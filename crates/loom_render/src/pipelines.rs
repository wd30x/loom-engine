//! The render pipelines and the layouts they bind, built once by
//! `Renderer::with_adapter` in this order: the globals, the meshes, the
//! skinned meshes, the sprites, the debug lines, the HUD.

use crate::frame::{GpuGlobals, HudVertex, LineVertex};
use crate::{COLOR_FORMAT, DEPTH_FORMAT, mesh};

/// A read-only storage buffer read by the vertex stage, at `binding`.
fn vertex_storage(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// The uniform buffer of the globals every pipeline reads.
pub(crate) fn globals_buffer(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("globals"),
        size: std::mem::size_of::<GpuGlobals>() as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// Group 0 of the meshes and the sprites: the globals, then the per-draw
/// storage read by the vertex stage.
pub(crate) fn mesh_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("mesh"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            vertex_storage(1),
        ],
    })
}

/// The pipeline of the lit meshes.
pub(crate) fn mesh_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    lit_pipeline(
        device,
        "mesh",
        shader,
        bind_group_layout,
        mesh::Vertex::layout(),
    )
}

/// Group 0 of the skinned meshes: the globals, the per-draw storage and
/// the joint matrices, both read by the vertex stage.
pub(crate) fn skinned_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("skinned"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            vertex_storage(1),
            vertex_storage(2),
        ],
    })
}

/// The pipeline of the lit skinned meshes: the lit meshes' state with the
/// skinned vertex layout.
pub(crate) fn skinned_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    lit_pipeline(
        device,
        "skinned mesh",
        shader,
        bind_group_layout,
        mesh::SkinnedVertex::layout(),
    )
}

/// A lit, back-face culled, depth-tested pipeline of `shader` with one
/// vertex buffer of `vertex_layout`.
fn lit_pipeline(
    device: &wgpu::Device,
    label: &str,
    shader: &wgpu::ShaderModule,
    bind_group_layout: &wgpu::BindGroupLayout,
    vertex_layout: wgpu::VertexBufferLayout<'static>,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[Some(bind_group_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[Some(vertex_layout)],
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
            module: shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(COLOR_FORMAT.into())],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Group 1 of the sprites, and group 0 of the HUD's text and images: a
/// sprite sheet texture.
pub(crate) fn texture_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("sprite texture"),
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
    })
}

/// The pipeline of the sprites.
pub(crate) fn sprite_pipeline(
    device: &wgpu::Device,
    sprite_shader: &wgpu::ShaderModule,
    bind_group_layout: &wgpu::BindGroupLayout,
    texture_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let sprite_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("sprite"),
        bind_group_layouts: &[Some(bind_group_layout), Some(texture_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("sprite"),
        layout: Some(&sprite_layout),
        vertex: wgpu::VertexState {
            module: sprite_shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState {
            front_face: wgpu::FrontFace::Ccw,
            // No culling: a sprite stays visible whichever way its node is
            // turned (e.g. a negative scale to mirror it), rather than
            // depending on the winding surviving the model matrix.
            cull_mode: None,
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
            module: sprite_shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(COLOR_FORMAT.into())],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// The bind group and the pipeline of the debug lines.
pub(crate) fn line_pipeline(
    device: &wgpu::Device,
    line_shader: &wgpu::ShaderModule,
    globals_buffer: &wgpu::Buffer,
) -> (wgpu::BindGroup, wgpu::RenderPipeline) {
    let line_bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("debug lines"),
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
    let line_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("debug lines"),
        layout: &line_bind_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: globals_buffer.as_entire_binding(),
        }],
    });
    let line_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("debug lines"),
        bind_group_layouts: &[Some(&line_bind_layout)],
        immediate_size: 0,
    });
    let line_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("debug lines"),
        layout: Some(&line_layout),
        vertex: wgpu::VertexState {
            module: line_shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[Some(LineVertex::layout())],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::LineList,
            cull_mode: None,
            ..Default::default()
        },
        // Same pass as the meshes, so the same depth format, but the
        // depth buffer is neither tested nor written.
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: line_shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(COLOR_FORMAT.into())],
        }),
        multiview_mask: None,
        cache: None,
    });
    (line_bind_group, line_pipeline)
}

/// The HUD's pipelines: the solid one, then the textured one.
pub(crate) fn hud_pipelines(
    device: &wgpu::Device,
    hud_shader: &wgpu::ShaderModule,
    texture_layout: &wgpu::BindGroupLayout,
) -> (wgpu::RenderPipeline, wgpu::RenderPipeline) {
    // Text and images bind a sprite texture as group 0.
    let hud_pipeline = |label: &str, layouts: &[Option<&wgpu::BindGroupLayout>], fs: &str| {
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(label),
            bind_group_layouts: layouts,
            immediate_size: 0,
        });
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: hud_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(HudVertex::layout())],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: hud_shader,
                entry_point: Some(fs),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let hud_solid_pipeline = hud_pipeline("hud", &[], "fs_solid");
    let hud_textured_pipeline =
        hud_pipeline("hud textured", &[Some(texture_layout)], "fs_textured");
    (hud_solid_pipeline, hud_textured_pipeline)
}
