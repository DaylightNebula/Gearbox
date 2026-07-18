use std::f32::consts::PI;

use crate::{Shape, ShapeMeshData, glam::*};

/// A sphere [`Shape`] centered on `offset`, built by subdividing an icosahedron.
/// `mesh_subdivisions` controls mesh detail (default 3 if `None`); values above
/// 5 or 6 get expensive.
pub struct Sphere {
    pub offset: Vec3,
    pub rotation: Quat,
    pub radius: f32,
    pub mesh_subdivisions: Option<u32>
}

impl Sphere {
    /// Creates a new `Sphere` shape.
    pub fn new(
        offset: Vec3, 
        rotation: Quat, 
        radius: f32, 
        mesh_subdivisions: Option<u32>
    ) -> Self {
        Self { 
            offset, rotation, 
            radius, mesh_subdivisions 
        }
    }
}

impl Shape for Sphere {
    fn as_mesh_data(&self) -> anyhow::Result<(Vec3, Quat, crate::ShapeMeshData)> {
        let radius = self.radius;
        let subdivisions = self.mesh_subdivisions.unwrap_or(3);

        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        
        // Golden ratio
        let t = (1.0 + 5.0_f32.sqrt()) / 2.0;
        
        // Create initial icosahedron vertices
        let initial_vertices = vec![
            Vec3::new(-1.0,  t,  0.0).normalize() * radius,
            Vec3::new( 1.0,  t,  0.0).normalize() * radius,
            Vec3::new(-1.0, -t,  0.0).normalize() * radius,
            Vec3::new( 1.0, -t,  0.0).normalize() * radius,
            Vec3::new( 0.0, -1.0,  t).normalize() * radius,
            Vec3::new( 0.0,  1.0,  t).normalize() * radius,
            Vec3::new( 0.0, -1.0, -t).normalize() * radius,
            Vec3::new( 0.0,  1.0, -t).normalize() * radius,
            Vec3::new( t,  0.0, -1.0).normalize() * radius,
            Vec3::new( t,  0.0,  1.0).normalize() * radius,
            Vec3::new(-t,  0.0, -1.0).normalize() * radius,
            Vec3::new(-t,  0.0,  1.0).normalize() * radius,
        ];
        
        // Initial icosahedron faces
        let mut triangles = vec![
            [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11],
            [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
            [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9],
            [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
        ];
        
        let mut positions = initial_vertices;
        
        // Subdivide
        for _idx in 0..subdivisions {
            let mut new_triangles = Vec::new();
            let mut midpoint_cache = std::collections::HashMap::new();
            
            for tri in &triangles {
                let v0 = tri[0];
                let v1 = tri[1];
                let v2 = tri[2];
                
                let a = get_midpoint(v0, v1, &mut positions, &mut midpoint_cache, radius);
                let b = get_midpoint(v1, v2, &mut positions, &mut midpoint_cache, radius);
                let c = get_midpoint(v2, v0, &mut positions, &mut midpoint_cache, radius);
                
                new_triangles.push([v0, a, c]);
                new_triangles.push([v1, b, a]);
                new_triangles.push([v2, c, b]);
                new_triangles.push([a, b, c]);
            }
            
            triangles = new_triangles;
        }
        
        // Convert positions to vertices with normals and UVs
        for pos in &positions {
            let normal = pos.normalize();
            
            // Spherical UV mapping
            let u = 0.5 + (normal.z.atan2(normal.x) / (2.0 * PI));
            let v = 0.5 - (normal.y.asin() / PI);
            
            vertices.push(shaders::basic_vertex::VertexInput {
                position: (*pos).into(),
                uvs: Vec2::new(u, v).into(),
                normals: normal.into()
            });
        }
        
        // Convert triangles to indices
        for tri in triangles {
            indices.push(tri[0] as u32);
            indices.push(tri[1] as u32);
            indices.push(tri[2] as u32);
        }
        
        Ok((
            self.offset,
            self.rotation,
            ShapeMeshData { vertices, indices }
        ))
    }
}

/// Utility for getting midpoints while building spheres.
fn get_midpoint(
    v1: usize,
    v2: usize,
    positions: &mut Vec<Vec3>,
    cache: &mut std::collections::HashMap<(usize, usize), usize>,
    radius: f32,
) -> usize {
    let key = if v1 < v2 { (v1, v2) } else { (v2, v1) };
    
    if let Some(&index) = cache.get(&key) {
        return index;
    }
    
    let p1 = positions[v1];
    let p2 = positions[v2];
    let middle = ((p1 + p2) * 0.5).normalize() * radius;
    
    let index = positions.len();
    positions.push(middle);
    cache.insert(key, index);
    
    index
}
