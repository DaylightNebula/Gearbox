use crate::{glam::*, Shape, ShapeMeshData};

pub struct Cone {
    pub offset: Vec3,
    pub rotation: Quat,
    pub radius: f32,
    pub height: f32,
    pub segments: Option<u32>
}

impl Cone {
    pub fn new(
        offset: Vec3,
        rotation: Quat,
        radius: f32,
        height: f32,
        segments: Option<u32>
    ) -> Self {
        Self { offset, rotation, radius, height, segments }
    }
}

impl Shape for Cone {
    fn as_mesh_data(&self) -> anyhow::Result<(Vec3, Quat, crate::ShapeMeshData)> {
        let radius = self.radius;
        let height = self.height;
        let segments = self.segments.unwrap_or(16);
        
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        
        let hh = height / 2.0; // half height
        
        // Apex vertex (at top, duplicated for each segment for proper normals)
        for i in 0..=segments {
            let angle = (i as f32 / segments as f32) * 2.0 * std::f32::consts::PI;
            let next_angle = ((i + 1) as f32 / segments as f32) * 2.0 * std::f32::consts::PI;
            
            let x = angle.cos() * radius;
            let z = angle.sin() * radius;
            let next_x = next_angle.cos() * radius;
            let next_z = next_angle.sin() * radius;
            
            // Calculate normal for this segment (cross product of two edges)
            let edge1 = Vec3::new(x, -height, z);
            let edge2 = Vec3::new(next_x, -height, next_z);
            let normal = edge1.cross(edge2).normalize();
            
            let u = i as f32 / segments as f32;
            
            // Apex
            vertices.push(shaders::basic_vertex::VertexInput {
                position: Vec3::new(0.0, hh, 0.0).into(),
                uvs: Vec2::new(u, 0.0).into(),
                normals: normal.into(),
            });
            
            // Base
            vertices.push(shaders::basic_vertex::VertexInput {
                position: Vec3::new(-x, -hh, z).into(),
                uvs: Vec2::new(u, 1.0).into(),
                normals: normal.into(),
            });
        }
        
        // Generate indices for cone sides
        for i in 0..segments {
            let base = i * 2 + 1;
            
            indices.push(base as u32);
            indices.push((base + 2) as u32);
            indices.push((base + 1) as u32);
        }
        
        // Bottom cap center
        let bottom_center_idx = vertices.len() as u32;
        vertices.push(shaders::basic_vertex::VertexInput {
            position: Vec3::new(0.0, -hh, 0.0).into(),
            uvs: Vec2::new(0.5, 0.5).into(),
            normals: Vec3::new(0.0, -1.0, 0.0).into()
        });
        
        // Bottom cap rim vertices
        for i in 0..=segments {
            let angle = (i as f32 / segments as f32) * 2.0 * std::f32::consts::PI;
            let x = angle.cos() * radius;
            let z = angle.sin() * radius;
            
            vertices.push(shaders::basic_vertex::VertexInput {
                position: Vec3::new(-x, -hh, z).into(),
                uvs: Vec2::new(x / radius * 0.5 + 0.5, z / radius * 0.5 + 0.5).into(),
                normals: Vec3::new(0.0, -1.0, 0.0).into()
            });
        }
        
        // Bottom cap indices (reversed winding)
        for i in 0..segments {
            indices.push(bottom_center_idx);
            indices.push(bottom_center_idx + 2 + i as u32);
            indices.push(bottom_center_idx + 1 + i as u32);
        }
        
        Ok((self.offset, self.rotation, ShapeMeshData { vertices, indices }))
    }
}
