use std::f32::consts::PI;

use crate::{glam::*, Shape, ShapeMeshData};

/// A capsule (cylinder capped with two hemispheres) [`Shape`] centered on `offset`.
/// `height` is the total end-to-end height including both hemispherical caps.
/// `draw_instructions` overrides the default mesh resolution (16 segments, 16 rings)
/// when set.
pub struct Capsule {
    pub offset: Vec3,
    pub rotation: Quat,
    pub radius: f32,
    pub height: f32,
    pub draw_instructions: Option<CapsuleDrawInstructions>
}

/// Mesh resolution for a [`Capsule`]: `segments` around the circumference and
/// `rings` per hemispherical cap.
pub struct CapsuleDrawInstructions {
    pub segments: u32,
    pub rings: u32
}

impl Capsule {
    /// Creates a new capsule.
    pub fn new(
        offset: Vec3,
        rotation: Quat,
        radius: f32,
        height: f32,
        draw_instructions: Option<CapsuleDrawInstructions>
    ) -> Self {
        Self { offset, rotation, radius, height, draw_instructions }
    }
}

impl Shape for Capsule {
    fn as_mesh_data(&self) -> anyhow::Result<(Vec3, Quat, crate::ShapeMeshData)> {
        let radius = self.radius;
        let height = self.height;
        let segments = self.draw_instructions
            .as_ref()
            .map(|a| a.segments)
            .unwrap_or(16);
        let rings = self.draw_instructions
            .as_ref()
            .map(|a| a.rings)
            .unwrap_or(16);
        
        let mut vertices = Vec::new();
        let mut indices = Vec::<u32>::new();
        
        let cylinder_height = height - 2.0 * radius;
        let half_cylinder = cylinder_height / 2.0;
        
        // Top hemisphere
        for ring in 0 ..= rings {
            let phi = (ring as f32 / rings as f32) * (PI / 2.0);
            let y = radius * phi.cos() + half_cylinder;
            let ring_radius = radius * phi.sin();
            
            for seg in 0..=segments {
                let theta = (seg as f32 / segments as f32) * 2.0 * PI;
                let x = ring_radius * theta.cos();
                let z = ring_radius * theta.sin();
                
                let normals = Vec3::new(x, radius * phi.cos(), z).normalize();
                let u = seg as f32 / segments as f32;
                let v = 1.0 - (ring as f32 / rings as f32) * 0.5;
                
                vertices.push(shaders::basic_vertex::VertexInput {
                    position: Vec3::new(x, y, z).into(),
                    uvs: Vec2::new(u, v).into(),
                    normals: normals.into()
                });
            }
        }
        
        // Cylinder body
        let cylinder_rings = 2;
        for ring in 0..=cylinder_rings {
            let y = half_cylinder - (ring as f32 / cylinder_rings as f32) * cylinder_height;
            
            for seg in 0..=segments {
                let theta = (seg as f32 / segments as f32) * 2.0 * PI;
                let x = radius * theta.cos();
                let z = radius * theta.sin();
                
                let normal = Vec3::new(x, 0.0, z).normalize();
                let u = seg as f32 / segments as f32;
                let v = 0.5 - (ring as f32 / cylinder_rings as f32) * 0.25;
                
                vertices.push(shaders::basic_vertex::VertexInput {
                    position: Vec3::new(x, y, z).into(),
                    uvs: Vec2::new(u, v).into(),
                    normals: normal.into(),
                });
            }
        }
        
        // Bottom hemisphere
        for ring in 0..=rings {
            let phi = (ring as f32 / rings as f32) * (PI / 2.0);
            let y = -radius * phi.cos() - half_cylinder;
            let ring_radius = radius * phi.sin();
            
            for seg in 0..=segments {
                let theta = (seg as f32 / segments as f32) * 2.0 * PI;
                let x = ring_radius * theta.cos();
                let z = ring_radius * theta.sin() * -1.0;
                
                let normal = Vec3::new(x, -radius * phi.cos(), z).normalize();
                let u = seg as f32 / segments as f32;
                let v = 0.25 - (ring as f32 / rings as f32) * 0.25;
                
                vertices.push(shaders::basic_vertex::VertexInput {
                    position: Vec3::new(x, y, z).into(),
                    uvs: Vec2::new(u, v).into(),
                    normals: normal.into(),
                });
            }
        }
        
        // Generate indices
        let segs_plus_one = (segments + 1) as u32;
        
        // Top hemisphere indices
        for ring in 0..rings {
            for seg in 0..segments {
                let current = ring * segs_plus_one + seg;
                let next = current + segs_plus_one;
                
                indices.push(current as u32);
                indices.push(next as u32);
                indices.push(current as u32 + 1);
                
                indices.push(current as u32 + 1);
                indices.push(next as u32);
                indices.push(next as u32 + 1);
            }
        }
        
        // Cylinder indices
        let cylinder_start = (rings + 1) * segs_plus_one;
        for ring in 0..cylinder_rings {
            for seg in 0..segments {
                let current = cylinder_start + ring * segs_plus_one + seg;
                let next = current + segs_plus_one;
                
                indices.push(current as u32);
                indices.push(next as u32);
                indices.push(current as u32 + 1);
                
                indices.push(current as u32 + 1);
                indices.push(next as u32);
                indices.push(next as u32 + 1);
            }
        }
        
        // Bottom hemisphere indices
        let bottom_start = cylinder_start + (cylinder_rings + 1) * segs_plus_one;
        for ring in 0..rings {
            for seg in 0..segments {
                let current = bottom_start + ring * segs_plus_one + seg;
                let next = current + segs_plus_one;
                
                indices.push(current as u32);
                indices.push(next as u32);
                indices.push(current as u32 + 1);
                
                indices.push(current as u32 + 1);
                indices.push(next as u32);
                indices.push(next as u32 + 1);
            }
        }

        Ok((
            self.offset,
            self.rotation, 
            ShapeMeshData { vertices, indices }
        ))
    }
}
