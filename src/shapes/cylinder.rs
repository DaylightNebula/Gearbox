use crate::{glam::*, Shape, ShapeMeshData};

/// A cylinder [`Shape`] centered on `offset`, with capped top and bottom.
/// `segments` controls the number of sides used around the circumference
/// (default 16 if `None`).
pub struct Cylinder {
    pub offset: Vec3,
    pub rotation: Quat,
    pub radius: f32,
    pub height: f32,
    pub segments: Option<u32>
}

impl Cylinder {
    /// Creates a new `Cylinder` shape.
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

impl Shape for Cylinder {
    fn as_mesh_data(&self) -> anyhow::Result<(Vec3, Quat, crate::ShapeMeshData)> {
        let radius = self.radius;
        let height = self.height;
        let segments = self.segments.unwrap_or(16);
        
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        
        let hh = height / 2.0; // half height
        
        // Generate vertices for top and bottom circles
        for i in 0..=segments {
            let angle = (i as f32 / segments as f32) * 2.0 * std::f32::consts::PI;
            let x = angle.cos() * radius;
            let z = angle.sin() * radius;
            let u = i as f32 / segments as f32;
            
            // Side vertices - top
            vertices.push(shaders::basic_vertex::VertexInput {
                position: Vec3::new(x, hh, z).into(),
                uvs: Vec2::new(u, 0.0).into(),
                normals: Vec3::new(x / radius, 0.0, z / radius).normalize().into(),
            });
            
            // Side vertices - bottom
            vertices.push(shaders::basic_vertex::VertexInput {
                position: Vec3::new(x, -hh, z).into(),
                uvs: Vec2::new(u, 1.0).into(),
                normals: Vec3::new(x / radius, 0.0, z / radius).normalize().into()
            });
        }
        
        // Generate indices for cylinder sides
        for i in 0..segments {
            let base = i * 2;
            
            // Two triangles per segment
            indices.push(base as u32);
            indices.push((base + 2) as u32);
            indices.push((base + 1) as u32);
            
            indices.push((base + 1) as u32);
            indices.push((base + 2) as u32);
            indices.push((base + 3) as u32);
        }
        
        // Top cap center
        let top_center_idx = vertices.len() as u32;
        vertices.push(shaders::basic_vertex::VertexInput {
            position: Vec3::new(0.0, hh, 0.0).into(),
            uvs: Vec2::new(0.5, 0.5).into(),
            normals: Vec3::new(0.0, 1.0, 0.0).into()
        });
        
        // Top cap rim vertices
        for i in 0..=segments {
            let angle = (i as f32 / segments as f32) * 2.0 * std::f32::consts::PI;
            let x = angle.cos() * radius;
            let z = angle.sin() * radius * -1.0;
            
            vertices.push(shaders::basic_vertex::VertexInput {
                position: Vec3::new(x, hh, z).into(),
                uvs: Vec2::new(x / radius * 0.5 + 0.5, z / radius * 0.5 + 0.5).into(),
                normals: Vec3::new(0.0, 1.0, 0.0).into()
            });
        }
        
        // Top cap indices
        for i in 0..segments {
            indices.push(top_center_idx);
            indices.push(top_center_idx + 1 + i as u32);
            indices.push(top_center_idx + 2 + i as u32);
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
            let z = angle.sin() * radius * -1.0;
            
            vertices.push(shaders::basic_vertex::VertexInput {
                position: Vec3::new(x, -hh, z).into(),
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
        
        Ok((
            self.offset,
            self.rotation,
            ShapeMeshData { vertices, indices }
        ))
    }
}
