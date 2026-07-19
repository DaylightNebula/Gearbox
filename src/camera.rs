use std::{any::TypeId, collections::hash_map::Entry};

use ahash::AHashMap;
use anarchy::{anyhow, macros::{Component, Getters, GettersMut, Setters}};
use magician_vgpu::{BindableObject, Buffer, MutableBuffer, StaticTexture, VirtualGpu, WritableBuffer, glam::{Mat4, UVec2, Vec3}, rust};
use wgpu::BufferUsages;

use crate::Transform;

const OPENGL_TO_WGPU_MATRIX: Mat4 = Mat4::from_cols_array_2d(&[
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 0.5, 0.0],
    [0.0, 0.0, 0.5, 1.0]
]);

/// A [`Component`](anarchy::Component) that turns an entity into a render camera.
///
/// Paired with a [`Transform`] on the same entity to determine view position; the
/// GPU-side view/projection buffer and any framebuffers are lazily created on the
/// first internal update and stored in `buffers`. `gearbox`'s main render pass
/// (see [`crate::update_cameras`]) uses the first camera found in the world each frame.
#[derive(Component, Getters, GettersMut, Setters)]
pub struct Camera {
    fovy_radians: f32,
    znear: f32,
    zfar: f32,
    buffers: Option<CameraBuffers>
}

/// The lazily-initialized GPU resources backing a [`Camera`]: its view/projection
/// uniform buffer, the bindable object exposing that buffer to shaders, and any
/// framebuffers (e.g. depth) requested via [`Camera::get_or_compute_framebuffer`].
pub struct CameraBuffers {
    buffer: MutableBuffer<[shaders::common::Camera]>,
    bindable: BindableObject<shaders::common::CameraInput>,
    framebuffers: AHashMap<FrameBufferKey, StaticTexture>
}

/// A key identifying one of a [`Camera`]'s auxiliary framebuffers (e.g. depth), as
/// used by [`Camera::get_framebuffer`], [`Camera::set_framebuffer`], and
/// [`Camera::get_or_compute_framebuffer`]. `Str` and `TypeId` variants let callers
/// key framebuffers by their own identifiers without needing to extend this enum.
#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum FrameBufferKey {
    #[default]
    Depth,
    U32(u32),
    TypeId(TypeId),
    Str(&'static str)
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            fovy_radians: 0.78,
            znear: 0.1,
            zfar: 10000.0,
            buffers: None
        }
    }
}

impl Camera {
    /// Create new camera.
    pub fn new(fovy_radians: f32, znear: f32, zfar: f32) -> Self {
        Self {
            fovy_radians,
            znear, zfar,
            buffers: None
        }
    }

    /// Get bindable object of this camera.
    pub fn bindable(&self) -> Option<&BindableObject<shaders::common::CameraInput>> {
        self.buffers.as_ref().map(|a| &a.bindable)
    }

    /// Returns true if the internal buffers have been setup
    pub fn is_buffers_initialize(&self) -> bool {
        self.buffers.is_some()
    }

    /// Get a framebuffer assigned to a certain key in this `Camera`.
    pub fn get_framebuffer(&self, key: FrameBufferKey) -> Option<&StaticTexture> {
        self.buffers.as_ref()
            .map(|a| a.framebuffers.get(&key))
            .flatten()
    }

    /// Assign a framebuffer to this `Camera`.
    pub fn set_framebuffer(&mut self, key: FrameBufferKey, framebuffer: StaticTexture) {
        if let Some(buffers) = self.buffers.as_mut() {
            buffers.framebuffers.insert(key, framebuffer);
        }
    }

    /// Gets or creates a frame buffer with the given key.  If the frame buffer is created,
    /// the given format and usage will be created using `StaticTexture::framebuffer`. None
    /// will be returned if the internal buffers of this `Camera` have not been initialized.
    pub fn get_or_compute_framebuffer(
        &mut self, 
        vgpu: &VirtualGpu,
        key: FrameBufferKey,
        format: wgpu::TextureFormat,
        usage: wgpu::TextureUsages,
        dimensions: UVec2
    ) -> Option<&StaticTexture> {
        if let Some(buffers) = self.buffers.as_mut() {
            // let buffer = buffers.framebuffers.get(&key);
            // let is_buffer_valid = buffer.map(|buffer| 
            //         buffer.texture.width() == dimensions.x && 
            //         buffer.texture.height() == dimensions.y
            //     ).unwrap_or(false);

            // if is_buffer_valid { return buffer }

            // let buffer = StaticTexture::framebuffer(vgpu, format, usage);
            // buffers.framebuffers.insert(key, buffer);
            // return buffers.framebuffers.get(&key);

            let mut entry = buffers.framebuffers.entry(key);

            if let Entry::Occupied(entry) = &mut entry {
                let tex = &entry.get().texture;
                if tex.width() != dimensions.x || tex.height() != dimensions.y {
                    entry.insert(StaticTexture::framebuffer(vgpu, format, usage));
                }
            }

            Some(entry.or_insert_with(|| StaticTexture::framebuffer(vgpu, format, usage)))
        } else {
            return None;
        }
    }

    /// Update the internal buffers for this camera
    pub(crate) fn update(&mut self, vgpu: &VirtualGpu, transform: &Transform) -> anyhow::Result<()> {
        // generate view and projection matricies
        #[allow(deprecated)]
        let vp_mat = OPENGL_TO_WGPU_MATRIX * Mat4::perspective_rh(
            0.785398, 
            vgpu.config().width as f32 / vgpu.config().height as f32, 
            0.1, 10000.0
        ) * Mat4::look_at_rh(
            transform.translation.into(), 
            Vec3::new(0.0, 0.0, 0.0).into(), 
            Vec3::new(0.0, 1.0, 0.0).into()
        );

        // build new camera shader info object
        let camera = shaders::common::Camera { 
            view_pos: rust::Vec4::from_vec3_w(transform.translation.into(), 0.0), 
            view_proj: vp_mat.into() 
        };

        // create or update camera buffers
        if let Some(buffers) = &self.buffers {
            buffers.buffer.write(vgpu, &[camera])?;
        } else {
            let buffer = MutableBuffer::<[shaders::common::Camera]>::new(vgpu, &[camera], BufferUsages::UNIFORM | BufferUsages::COPY_DST);
            let bindable = BindableObject::<shaders::common::CameraInput>::from_inputs(vgpu, buffer.buffer());
            self.buffers = Some(CameraBuffers { buffer, bindable, framebuffers: AHashMap::default() })
        };

        Ok(())
    }
}
