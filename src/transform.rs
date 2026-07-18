use anarchy::macros::{Component, Getters, GettersMut, Setters};
use magician_vgpu::glam::{Mat4, Quat, Vec3};

/// A [`Component`](anarchy::Component) giving an entity a position, rotation, and scale in world space.
///
/// Read by [`Camera`](crate::Camera) (view matrix) and mesh types like
/// [`BasicMesh`](crate::BasicMesh) (instance matrix) each render frame.
#[derive(Component, Copy, Clone, Debug, Getters, GettersMut, Setters)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3
}

impl Transform {
    /// Creates a `Transform` at the origin with no rotation and unit scale.
    pub fn identity() -> Self {
        Self { translation: Vec3::ZERO, rotation: Quat::IDENTITY, scale: Vec3::ONE }
    }

    /// Creates a new `Transform` from its parts.
    pub fn new(translation: Vec3, rotation: Quat, scale: Vec3) -> Self {
        Self { translation, rotation, scale }
    }

    /// Computes the world-space model matrix for this transform.
    pub fn as_matix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }

    /// Returns a copy of this transform with `translation` replaced.
    pub fn with_translation(&self, translation: Vec3) -> Self {
        Self { translation, rotation: self.rotation, scale: self.scale }
    }

    /// Returns a copy of this transform with `rotation` replaced.
    pub fn with_rotation(&self, rotation: Quat) -> Self {
        Self { translation: self.translation, rotation, scale: self.scale }
    }

    /// Returns a copy of this transform with `scale` replaced.
    pub fn with_scale(&self, scale: Vec3) -> Self {
        Self { translation: self.translation, rotation: self.rotation, scale }
    }

    /// Offsets `translation` by `translation` in place.
    pub fn translate_by(&mut self, translation: Vec3) {
        self.translation += translation;
    }

    /// Applies `rotate` on top of the current rotation in place.
    pub fn rotate_by(&mut self, rotate: Quat) {
        self.rotation = rotate * self.rotation;
    }

    /// Multiplies the current scale by `scale` in place.
    pub fn scale_by(&mut self, scale: Vec3) {
        self.scale *= scale;
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::identity()
    }
}
