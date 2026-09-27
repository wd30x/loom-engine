//! The GPU buffers the renderer fills every frame, which grow as needed,
//! and the built-in meshes' buffers.

use wgpu::util::DeviceExt;

use crate::mesh::{self, MeshKind};

/// Per-draw data for every draw item (or sprite) of a frame, read by the
/// shader by instance index, bound together with the globals. Grows as needed.
pub(crate) struct DrawBuffer {
    pub(crate) buffer: wgpu::Buffer,
    label: &'static str,
    /// Bytes per item.
    stride: usize,
    capacity: usize,
    pub(crate) bind_group: wgpu::BindGroup,
}

impl DrawBuffer {
    /// A buffer for `capacity` items of `stride` bytes, bound with `globals`.
    pub(crate) fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        globals: &wgpu::Buffer,
        label: &'static str,
        stride: usize,
        capacity: usize,
    ) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: (capacity * stride) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buffer.as_entire_binding(),
                },
            ],
        });
        Self {
            buffer,
            label,
            stride,
            capacity,
            bind_group,
        }
    }

    /// Replaces the buffer with a larger one if `len` items do not fit.
    pub(crate) fn reserve(
        &mut self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        globals: &wgpu::Buffer,
        len: usize,
    ) {
        if len > self.capacity {
            let capacity = len.next_power_of_two();
            *self = Self::new(device, layout, globals, self.label, self.stride, capacity);
        }
    }
}

/// The vertex buffer of the debug lines or of the HUD. Grows as needed,
/// never shrinks.
pub(crate) struct VertexBuffer {
    pub(crate) buffer: wgpu::Buffer,
    label: &'static str,
    /// Bytes per vertex.
    stride: usize,
    /// In vertices.
    capacity: usize,
}

impl VertexBuffer {
    pub(crate) fn new(
        device: &wgpu::Device,
        label: &'static str,
        stride: usize,
        capacity: usize,
    ) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: (capacity * stride) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            buffer,
            label,
            stride,
            capacity,
        }
    }

    /// Replaces the buffer with a larger one if `len` vertices do not fit.
    pub(crate) fn reserve(&mut self, device: &wgpu::Device, len: usize) {
        if len > self.capacity {
            *self = Self::new(device, self.label, self.stride, len.next_power_of_two());
        }
    }
}

/// Draw items the per-draw buffer holds before it first has to grow.
pub(crate) const INITIAL_DRAW_CAPACITY: usize = 64;
/// Debug line vertices the first line buffer holds (two per line).
pub(crate) const INITIAL_LINE_VERTICES: usize = 1024;
/// HUD vertices the first HUD buffer holds (six per item).
pub(crate) const INITIAL_HUD_VERTICES: usize = 96;

/// Vertex and index buffers of one mesh.
pub(crate) struct GpuMesh {
    pub(crate) vertices: wgpu::Buffer,
    pub(crate) indices: wgpu::Buffer,
    pub(crate) index_count: u32,
}

impl GpuMesh {
    pub(crate) fn new(device: &wgpu::Device, kind: MeshKind) -> Self {
        let (vertices, indices) = mesh::mesh(kind);
        Self {
            vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mesh vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mesh indices"),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
            index_count: u32::try_from(indices.len()).expect("meshes have few indices"),
        }
    }
}
