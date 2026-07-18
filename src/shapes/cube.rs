use crate::{glam::*, Shape, ShapeMeshData};

/// A box [`Shape`] centered on `offset`, spanning `width` x `height` x `depth`.
pub struct Cube {
    pub offset: Vec3,
    pub rotation: Quat,
    pub width: f32,
    pub height: f32,
    pub depth: f32
}

impl Cube {
    /// Creates a new `Cube` shape.
    pub fn new(offset: Vec3, rotation: Quat, width: f32, height: f32, depth: f32) -> Self {
        Self { offset, rotation, width, height, depth }
    }
}

impl Shape for Cube {
    fn as_mesh_data(&self) -> anyhow::Result<(Vec3, Quat, crate::ShapeMeshData)> {
        let hw = self.width / 2.0;  // half width
        let hh = self.height / 2.0; // half height
        let hd = self.depth / 2.0;  // half depth

        let vertices = vec![
            // Front face (z+)
            shaders::basic_vertex::VertexInput { position: Vec3::new(-hw, -hh,  hd).into(), uvs: Vec2::new(0.0, 1.0).into(), normals: Vec3::new(0.0, 0.0, 1.0).into() },
            shaders::basic_vertex::VertexInput { position: Vec3::new( hw, -hh,  hd).into(), uvs: Vec2::new(1.0, 1.0).into(), normals: Vec3::new(0.0, 0.0, 1.0).into() },
            shaders::basic_vertex::VertexInput { position: Vec3::new( hw,  hh,  hd).into(), uvs: Vec2::new(1.0, 0.0).into(), normals: Vec3::new(0.0, 0.0, 1.0).into() },
            shaders::basic_vertex::VertexInput { position: Vec3::new(-hw,  hh,  hd).into(), uvs: Vec2::new(0.0, 0.0).into(), normals: Vec3::new(0.0, 0.0, 1.0).into() },
            
            // Back face (z-)
            shaders::basic_vertex::VertexInput { position: Vec3::new( hw, -hh, -hd).into(), uvs: Vec2::new(0.0, 1.0).into(), normals: Vec3::new(0.0, 0.0, -1.0).into() },
            shaders::basic_vertex::VertexInput { position: Vec3::new(-hw, -hh, -hd).into(), uvs: Vec2::new(1.0, 1.0).into(), normals: Vec3::new(0.0, 0.0, -1.0).into() },
            shaders::basic_vertex::VertexInput { position: Vec3::new(-hw,  hh, -hd).into(), uvs: Vec2::new(1.0, 0.0).into(), normals: Vec3::new(0.0, 0.0, -1.0).into() },
            shaders::basic_vertex::VertexInput { position: Vec3::new( hw,  hh, -hd).into(), uvs: Vec2::new(0.0, 0.0).into(), normals: Vec3::new(0.0, 0.0, -1.0).into() },
            
            // Right face (x+)
            shaders::basic_vertex::VertexInput { position: Vec3::new( hw, -hh,  hd).into(), uvs: Vec2::new(0.0, 1.0).into(), normals: Vec3::new(1.0, 0.0, 0.0).into() },
            shaders::basic_vertex::VertexInput { position: Vec3::new( hw, -hh, -hd).into(), uvs: Vec2::new(1.0, 1.0).into(), normals: Vec3::new(1.0, 0.0, 0.0).into() },
            shaders::basic_vertex::VertexInput { position: Vec3::new( hw,  hh, -hd).into(), uvs: Vec2::new(1.0, 0.0).into(), normals: Vec3::new(1.0, 0.0, 0.0).into() },
            shaders::basic_vertex::VertexInput { position: Vec3::new( hw,  hh,  hd).into(), uvs: Vec2::new(0.0, 0.0).into(), normals: Vec3::new(1.0, 0.0, 0.0).into() },
            
            // Left face (x-)
            shaders::basic_vertex::VertexInput { position: Vec3::new(-hw, -hh, -hd).into(), uvs: Vec2::new(0.0, 1.0).into(), normals: Vec3::new(-1.0, 0.0, 0.0).into() },
            shaders::basic_vertex::VertexInput { position: Vec3::new(-hw, -hh,  hd).into(), uvs: Vec2::new(1.0, 1.0).into(), normals: Vec3::new(-1.0, 0.0, 0.0).into() },
            shaders::basic_vertex::VertexInput { position: Vec3::new(-hw,  hh,  hd).into(), uvs: Vec2::new(1.0, 0.0).into(), normals: Vec3::new(-1.0, 0.0, 0.0).into() },
            shaders::basic_vertex::VertexInput { position: Vec3::new(-hw,  hh, -hd).into(), uvs: Vec2::new(0.0, 0.0).into(), normals: Vec3::new(-1.0, 0.0, 0.0).into() },
            
            // Top face (y+)
            shaders::basic_vertex::VertexInput { position: Vec3::new(-hw,  hh,  hd).into(), uvs: Vec2::new(0.0, 1.0).into(), normals: Vec3::new(0.0, 1.0, 0.0).into() },
            shaders::basic_vertex::VertexInput { position: Vec3::new( hw,  hh,  hd).into(), uvs: Vec2::new(1.0, 1.0).into(), normals: Vec3::new(0.0, 1.0, 0.0).into() },
            shaders::basic_vertex::VertexInput { position: Vec3::new( hw,  hh, -hd).into(), uvs: Vec2::new(1.0, 0.0).into(), normals: Vec3::new(0.0, 1.0, 0.0).into() },
            shaders::basic_vertex::VertexInput { position: Vec3::new(-hw,  hh, -hd).into(), uvs: Vec2::new(0.0, 0.0).into(), normals: Vec3::new(0.0, 1.0, 0.0).into() },
            
            // Bottom face (y-)
            shaders::basic_vertex::VertexInput { position: Vec3::new(-hw, -hh, -hd).into(), uvs: Vec2::new(0.0, 1.0).into(), normals: Vec3::new(0.0, -1.0, 0.0).into() },
            shaders::basic_vertex::VertexInput { position: Vec3::new( hw, -hh, -hd).into(), uvs: Vec2::new(1.0, 1.0).into(), normals: Vec3::new(0.0, -1.0, 0.0).into() },
            shaders::basic_vertex::VertexInput { position: Vec3::new( hw, -hh,  hd).into(), uvs: Vec2::new(1.0, 0.0).into(), normals: Vec3::new(0.0, -1.0, 0.0).into() },
            shaders::basic_vertex::VertexInput { position: Vec3::new(-hw, -hh,  hd).into(), uvs: Vec2::new(0.0, 0.0).into(), normals: Vec3::new(0.0, -1.0, 0.0).into() },
        ];

        let indices = vec![
            0, 1, 2,  0, 2, 3,
            4, 5, 6,  4, 6, 7,
            8, 9, 10,  8, 10, 11,
            12, 13, 14,  12, 14, 15,
            16, 17, 18,  16, 18, 19,
            20, 21, 22,  20, 22, 23,
        ];

        Ok((self.offset, self.rotation, ShapeMeshData { vertices, indices }))
    }
}
