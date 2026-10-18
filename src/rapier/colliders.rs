use log::warn;
use crate::glam::*;
use rapier3d::{math::{Isometry, UnitVector}, na::{OPoint, Vector3}, prelude::{ColliderBuilder, SharedShape}};

/// Engine-agnostic description of a collision shape, converted to a rapier
/// `ColliderBuilder`/`SharedShape` via `into_rapier_collider` and
/// `into_rapier_shared_shape`.
///
/// Variants:
/// - `Sphere`: A ball of the given `radius`.
/// - `Capsule`: A capsule aligned to the Y axis, with the given `radius` and
///   `half_height` of its cylindrical section.
/// - `Cuboid`: A box with the given half-extents (`half_width`,
///   `half_height`, `half_depth`).
/// - `Cylinder`: A cylinder aligned to the Y axis, with the given `radius`
///   and `half_height`.
/// - `Cone`: A cone aligned to the Y axis, with the given `radius` and
///   `half_height`.
/// - `Compound`: Multiple child colliders, each offset by a local
///   translation and rotation.
/// - `Voxels`: A voxelized shape made of integer grid `points`, each cell
///   sized `size`.
/// - `Mesh`: A convex mesh built from `vertices` and triangle `indices`.
/// - `Halfspace`: An infinite plane with the given surface `normal`.
/// - `Empty`: No collision shape; conversion produces `None`.
#[derive(Clone, Debug, PartialEq)]
pub enum Collider {
    Sphere { radius: f32 },
    Capsule { radius: f32, half_height: f32 },
    Cuboid { half_width: f32, half_height: f32, half_depth: f32 },
    Cylinder { radius: f32, half_height: f32 },
    Cone { radius: f32, half_height: f32 },
    Compound { children: Box<[(Vec3, Quat, Collider)]> },
    Voxels { size: Vec3, points: Box<[[i32; 3]]> },
    Mesh { vertices: Box<[Vec3]>, indices: Box<[[u32; 3]]> },
    Halfspace { normal: Vec3 },
    Empty
}

/// Error produced while converting a `Collider` to its rapier equivalent.
///
/// Variants:
/// - `FailedToBuildMesh`: rapier rejected the vertices/indices given to
///   `Collider::Mesh` (e.g. degenerate or non-convex geometry).
#[derive(Debug)]
pub enum ColliderError {
    FailedToBuildMesh
}

impl Collider {
    /// Converts this collider to a rapier collider.
    pub fn into_rapier_collider(&self) -> Result<Option<rapier3d::prelude::ColliderBuilder>, ColliderError> {
        Ok(match self {
            Collider::Sphere { radius } => Some(ColliderBuilder::ball(*radius)),
            Collider::Capsule { radius, half_height } => Some(ColliderBuilder::capsule_y(*half_height, *radius)),
            Collider::Cuboid { half_width, half_height, half_depth } => Some(ColliderBuilder::cuboid(*half_width, *half_height, *half_depth)),
            Collider::Cylinder { radius, half_height } => Some(ColliderBuilder::cylinder(*half_height, *radius)),
            Collider::Cone { radius, half_height } => Some(ColliderBuilder::cone(*half_height, *radius)),

            Collider::Halfspace { normal } => {
                let normal = Vector3::new(normal.x, normal.y, normal.z);
                Some(ColliderBuilder::halfspace(UnitVector::new_normalize(normal)))
            },

            Collider::Compound { children } => {
                let shapes = children.iter()
                    .filter_map(|(translation, rotation, collider)| {
                        let axisangle = rotation.to_axis_angle();
                        let shape = collider.into_rapier_shared_shape();
                        let Ok(shape) = shape else {
                            warn!("Failed to get mesh: {:?}", shape.err().unwrap());
                            return None
                        };
                        let Some(shape) = shape else { return None };
                        let translation = Vector3::new(translation.x, translation.y, translation.z);
                        let axisangle = axisangle.0 * axisangle.1;
                        let axisangle = Vector3::new(axisangle.x, axisangle.y, axisangle.z);
                        Some((Isometry::new(translation, axisangle), shape))
                    })
                    .collect::<Vec<_>>();
                Some(ColliderBuilder::compound(shapes))
            },

            Collider::Voxels { size, points } => {
                let points = points.iter()
                    .map(|point| OPoint::from_slice(point))
                    .collect::<Vec<_>>();
                let size = Vector3::new(size.x, size.y, size.z);
                Some(ColliderBuilder::voxels(size, &points))
            },

            Collider::Mesh { vertices, indices } => {
                let points = vertices.iter()
                    .map(|point| OPoint::from_slice(&[point.x, point.y, point.z]))
                    .collect::<Vec<_>>();
                let Some(builder) = ColliderBuilder::convex_mesh(points, &indices) else { return Err(ColliderError::FailedToBuildMesh) };
                Some(builder)
            },

            Collider::Empty => None
        })
    }

    /// Converts this collider to a rapier shared shape.
    pub fn into_rapier_shared_shape(&self) -> Result<Option<SharedShape>, ColliderError> {
        Ok(match self {
            Collider::Sphere { radius } => Some(SharedShape::ball(*radius)),
            Collider::Capsule { radius, half_height } => Some(SharedShape::capsule_y(*half_height, *radius)),
            Collider::Cuboid { half_width, half_height, half_depth } => Some(SharedShape::cuboid(*half_width, *half_height, *half_depth)),
            Collider::Cylinder { radius, half_height } => Some(SharedShape::cylinder(*half_height, *radius)),
            Collider::Cone { radius, half_height } => Some(SharedShape::cone(*half_height, *radius)),
            Collider::Halfspace { normal } => {
                let normal = Vector3::new(normal.x, normal.y, normal.z);
                Some(SharedShape::halfspace(UnitVector::new_normalize(normal)))
            },

            Collider::Compound { children } => {
                let shapes = children.iter()
                    .filter_map(|(translation, rotation, collider)| {
                        let axisangle = rotation.to_axis_angle();
                        let shape = collider.into_rapier_shared_shape();
                        let Ok(shape) = shape else {
                            warn!("Failed to get mesh: {:?}", shape.err().unwrap());
                            return None
                        };
                        let Some(shape) = shape else { return None };
                        let translation = Vector3::new(translation.x, translation.y, translation.z);
                        let axisangle = axisangle.0 * axisangle.1;
                        let axisangle = Vector3::new(axisangle.x, axisangle.y, axisangle.z);
                        Some((Isometry::new(translation, axisangle), shape))
                    })
                    .collect::<Vec<_>>();
                Some(SharedShape::compound(shapes))
            },

            Collider::Voxels { size, points } => {
                let points = points.iter()
                    .map(|point| OPoint::from_slice(point))
                    .collect::<Vec<_>>();
                let size = Vector3::new(size.x, size.y, size.z);
                Some(SharedShape::voxels(size, &points))
            },

            Collider::Mesh { vertices, indices } => {
                let points = vertices.iter()
                    .map(|point| OPoint::from_slice(&[point.x, point.y, point.z]))
                    .collect::<Vec<_>>();
                let Some(builder) = SharedShape::convex_mesh(points, &indices) else { return Err(ColliderError::FailedToBuildMesh) };
                Some(builder)
            },

            Collider::Empty => None
        })
    }
}
