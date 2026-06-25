use anarchy::macros::{Component, Getters};
use bytemuck::NoUninit;
use magician_vgpu::{ImmutableBuffer, VirtualGpu};
use wgpu::BufferUsages;

#[derive(Getters, Component)]
pub struct Mesh<V: NoUninit> {
    pub vertex_buffer: ImmutableBuffer<[V]>,
    pub index_buffer: ImmutableBuffer<[u32]>
}

impl <V: NoUninit> Mesh<V> {
    pub fn from_raw(
        vertex_buffer: ImmutableBuffer<[V]>, 
        index_buffer: ImmutableBuffer<[u32]>
    ) -> Self {
        Self { vertex_buffer, index_buffer }
    }

    pub fn new(vgpu: &VirtualGpu, vertices: &[V], indices: &[u32]) -> Self {
        Self {
            vertex_buffer: ImmutableBuffer::new(vgpu, vertices, BufferUsages::VERTEX),
            index_buffer: ImmutableBuffer::new(vgpu, indices, BufferUsages::INDEX)
        }
    }
}
