use std::{error::Error, fmt::Display};

use crate::{BasicMesh, glam::*};

pub mod capsule;
pub mod cone;
pub mod cube;
pub mod cylinder;
pub mod sphere;

pub use capsule::*;
pub use cone::*;
pub use cube::*;
pub use cylinder::*;
use magician_vgpu::VirtualGpu;
pub use sphere::*;

/// Standard trait to define a shape.
pub trait Shape {
    /// Convert this shape to `ShapeMeshData` paired with offset position and rotation information.
    fn as_mesh_data(&self) -> anyhow::Result<(Vec3, Quat, ShapeMeshData)>;
}

/// Data structure intermediary for storing vertex and index information for building mesh.
pub struct ShapeMeshData {
    pub vertices: Vec<shaders::basic_vertex::VertexInput>,
    pub indices: Vec<u32>
}

/// Standard builder for building shapes, including compound shapes.
#[derive(Default)]
pub struct ShapeBuilder {
    shapes: Vec<Box<dyn Shape>>
}

/// Some various errors that may be thrown by `ShapeBuilder`.
#[derive(Debug, Clone, Copy)]
pub enum ShapeBuilderError {
    ColliderBuildError(&'static str)
}

impl Display for ShapeBuilderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShapeBuilderError::ColliderBuildError(error) => write!(f, "Collider Build Error: {}", error),
        }
    }
}

impl Error for ShapeBuilderError {}

impl ShapeBuilder {
    /// Create a new `ShapeBuilder`.
    pub fn new() -> Self { Self::default() }

    /// Add a cube to this builder.
    pub fn cube(
        &mut self, 
        offset: Vec3, 
        rotation: Quat, 
        width: f32, 
        height: f32, 
        depth: f32
    ) -> &mut Self {
        let shape = Cube { offset, rotation, width, height, depth };
        self.shapes.push(Box::new(shape));
        return self;
    }

    /// Add a new sphere to this builder.
    pub fn sphere(
        &mut self,
        offset: Vec3,
        rotation: Quat,
        radius: f32
    ) -> &mut Self {
        let shape = Sphere { offset, rotation, radius, mesh_subdivisions: None };
        self.shapes.push(Box::new(shape));
        return self;
    }

    /// Add a new sphere to this builder.
    /// The extra mesh_subdivisions field tells the sphere how many subdivisions
    /// to use when generating a mesh from this shape.
    /// WARN: Subdivision counts above 5 or 6, gets really expensive.
    pub fn sphere_subdivided(
        &mut self,
        offset: Vec3,
        rotation: Quat,
        radius: f32,
        mesh_subdivisions: u32
    ) -> &mut Self {
        let shape = Sphere { offset, rotation, radius, mesh_subdivisions: Some(mesh_subdivisions) };
        self.shapes.push(Box::new(shape));
        return self;
    }

    /// Adds a new capsule to this builder.
    /// If converted to mesh, the default segments and rings values will be used (16 and 16 respectively).
    pub fn capsule(
        &mut self,
        offset: Vec3,
        rotation: Quat,
        radius: f32,
        height: f32
    ) -> &mut Self {
        let shape = Capsule { offset, rotation, radius, height, draw_instructions: None };
        self.shapes.push(Box::new(shape));
        return self;
    }

    /// Adds a new capsule to this builder with extra draw instructions to define
    /// how many segments and rings the mesh should have when converted to mesh.
    pub fn capsule_with_draw_instructions(
        &mut self,
        offset: Vec3,
        rotation: Quat,
        radius: f32,
        height: f32,
        draw_instructions: CapsuleDrawInstructions
    ) -> &mut Self {
        let shape = Capsule { offset, rotation, radius, height, draw_instructions: Some(draw_instructions) };
        self.shapes.push(Box::new(shape));
        return self;
    }

    /// Adds a new cylinder shape to this builder.
    /// No segment argument is provided so a default of 16 will be used.
    pub fn cylinder(
        &mut self,
        offset: Vec3,
        rotation: Quat,
        radius: f32,
        height: f32
    ) -> &mut Self {
        let shape = Cylinder { offset, rotation, radius, height, segments: None };
        self.shapes.push(Box::new(shape));
        return self;
    }

    /// Adds a new cylinder shape to this builder.
    /// If converted to a mesh, the segment count provided will be used.
    pub fn cylinder_segmented(
        &mut self,
        offset: Vec3,
        rotation: Quat,
        radius: f32,
        height: f32,
        segments: u32
    ) -> &mut Self {
        let shape = Cylinder { offset, rotation, radius, height, segments: Some(segments) };
        self.shapes.push(Box::new(shape));
        return self;
    }

    /// Adds a new cone shape to this builder.
    /// No segment argument is provided so a default of 16 will be used.
    pub fn cone(
        &mut self,
        offset: Vec3,
        rotation: Quat,
        radius: f32,
        height: f32
    ) -> &mut Self {
        let shape = Cone { offset, rotation, radius, height, segments: None };
        self.shapes.push(Box::new(shape));
        return self;
    }

    /// Adds a new cone shape to this builder.
    /// If converted to a mesh, the segment count provided will be used.
    pub fn cone_segmented(
        &mut self,
        offset: Vec3,
        rotation: Quat,
        radius: f32,
        height: f32,
        segments: u32
    ) -> &mut Self {
        let shape = Cone { offset, rotation, radius, height, segments: Some(segments) };
        self.shapes.push(Box::new(shape));
        return self;
    }

    /// Builds this `ShapeBuilder` into a single `MeshAsset`, may return various build errors.
    pub fn build_mesh(&self, vgpu: &VirtualGpu) -> anyhow::Result<BasicMesh> {
        if self.shapes.is_empty() {
            return Ok(BasicMesh::new(vgpu, &[], &[]))
        }

        // convert all shapes to vertices and indices, counting each as we go
        let mut num_vertices = 0;
        let mut num_indices = 0;
        let mesh_data = self.shapes.iter()
            .map(|shape| {
                let mesh_data = shape.as_mesh_data()?;
                num_vertices += mesh_data.2.vertices.len();
                num_indices += mesh_data.2.indices.len();
                Ok(mesh_data)
            })
            .collect::<anyhow::Result<Vec<(Vec3, Quat, ShapeMeshData)>>>()?;

        // create final storage for vertices and indices
        let mut vertices = Vec::with_capacity(num_vertices);
        let mut indices = Vec::with_capacity(num_indices);
        mesh_data.iter().for_each(|(offset, rotation, mesh_data)| {
            // create transformation matrix
            let t_mat = Mat4::from_translation(*offset);
            let r_mat = Mat4::from_quat(*rotation);
            let s_mat = Mat4::from_scale(Vec3::ONE);
            let transform = t_mat * r_mat * s_mat;
            
            // transform and save vertices
            mesh_data.vertices.iter().for_each(|vertex| {
                vertices.push(shaders::basic_vertex::VertexInput {
                    position: transform.transform_point3(vertex.position.into()).into(),
                    uvs: vertex.uvs,
                    normals: vertex.normals
                });
            });

            // save indices
            indices.extend_from_slice(&mesh_data.indices);
        });

        // compile result
        // Ok(MeshAsset::from_raw(
        //     vertices, 
        //     indices, 
        //     num_vertices as u32, 
        //     num_indices as u32, 
        //     shaders::basic_vertex::VertexInput::contents()
        // ))
        Ok(BasicMesh::new(vgpu, &vertices, &indices))
    }
}
