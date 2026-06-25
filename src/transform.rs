use anarchy::macros::{Component, Getters, GettersMut, Setters};
use magician_vgpu::glam::{Mat4, Quat, Vec3};

#[derive(Component, Copy, Clone, Debug, Getters, GettersMut, Setters)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3
}

impl Transform {
    pub fn identity() -> Self {
        Self { translation: Vec3::ZERO, rotation: Quat::IDENTITY, scale: Vec3::ONE }
    }

    pub fn new(translation: Vec3, rotation: Quat, scale: Vec3) -> Self {
        Self { translation, rotation, scale }
    }

    pub fn as_matix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }

    pub fn with_translation(&self, translation: Vec3) -> Self {
        Self { translation, rotation: self.rotation, scale: self.scale }
    }

    pub fn with_rotation(&self, rotation: Quat) -> Self {
        Self { translation: self.translation, rotation, scale: self.scale }
    }

    pub fn with_scale(&self, scale: Vec3) -> Self {
        Self { translation: self.translation, rotation: self.rotation, scale }
    }

    pub fn translate_by(&mut self, translation: Vec3) {
        self.translation += translation;
    }

    pub fn rotate_by(&mut self, rotate: Quat) {
        self.rotation = rotate * self.rotation;
    }

    pub fn scale_by(&mut self, scale: Vec3) {
        self.scale *= scale;
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::identity()
    }
}
