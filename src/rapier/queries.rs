use std::any::Any;

use anarchy::{EntityID, Resource};
use getset::Getters;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use crate::glam::*;
use mutual::{Ref, SharedList};
use rapier3d::{na::{Isometry3, Point3, Vector3}, parry::query::{Ray, ShapeCastOptions}, prelude::QueryPipeline};

use crate::rapier::{Collider, PhysicsEngine};

/// A query that can be submitted to `PhysicsQueries` to a query.
/// 
/// Options:
///  - Ray:
///     - eye: The position of the ray
///     - dir: The direction of the ray
///     - max_dist: The maximum distance an intersection will be tested with.
///     - solid: When true, if started from the inside of an object, the ray will not "hit" that object.
///     - stop_at_first: When true, the query will stop at the first intersection.
///  - Shape
///     - shape: The shape to test intersections with.
///     - position: The position of the shape in world space.
///     - rotation: The rotation of the shape in world space.
///     - max_dist: The farthest away an intersection may be.
///     - stop_at_first: When true, the query will stop at the first intersection.
#[derive(Clone, PartialEq, Debug)]
pub enum PhysicsQuery {
    Ray { eye: Vec3, dir: Vec3, max_dist: f32, solid: bool, stop_at_first: bool },
    Shape { shape: Collider, position: Vec3, rotation: Quat, max_dist: f32, stop_at_first: bool }
}

/// The result of a physics query.
/// 
/// Contents:
/// - hit_position_normal: An option to a tuple that contains the position and normal of the intersection.
/// - entity_id: The ID of the entity hit.
#[derive(Getters, Clone, Copy, PartialEq)]
#[getset(get = "pub")]
pub struct PhysicsQueryResult {
    pub hit_position_normal: Option<(Vec3, Vec3)>,
    pub entity_id: EntityID
}

/// A pending query submitted via `PhysicsQueries::create_query`, awaiting
/// execution on the next `execute_queries` call. Holds the query's `id`, the
/// `PhysicsQuery` to run, and any caller-supplied `data` to return alongside
/// the results.
struct PhysicsQueryInput {
    id: String,
    query: PhysicsQuery,
    data: Option<Box<dyn Any>>
}

/// The completed `results` of a query, kept in `PhysicsQueries::results`
/// until consumed via `consume_query` or expired after 10 seconds based on
/// `created_at`. Retains the `data` supplied when the query was created.
struct PhysicsStoredResult {
    results: Vec<PhysicsQueryResult>,
    created_at: DateTime<Utc>,
    data: Ref<Option<Box<dyn Any>>>
}

/// Resource for querying the physics world without requiring all physics queries to be
/// run on the same systems executor as `PhysicsEngine`.  Instead of queries being run
/// immeidately when a query is submitted, a query is created with `create_query` or 
/// `create_query_with_data`, executed on the next physics tick and then the results can 
/// be consumed by `consume_query` after it is complete.
/// 
/// Heres an example:
/// ```rust,ignore
/// fn update(
///     input: Res<Inputs>,
///     queries: Res<PhysicsQueries>,
///     mut camera: Query<(&'static Transform, &'static CameraView)>
/// ) {
///     if input.is_pressed(Button::MouseLeft) {
///         let Some((_id, (camera_transform, camera))) = camera.next().unwrap() else { return };
/// 
///         let (eye, dir) = get_camera_ray( ... );
/// 
///         queries.create_query("place-block", PhysicsQuery::Ray { eye, dir, max_dist: 1000.0, solid: false, stop_at_first: true });
///     }
/// 
///     if let Some((results, _data)) = queries.consume_query("place-block") {
///         let Some(result) = results.into_iter().next() else { return };
///         let Some(position_normal) = result.hit_position_normal else { return };
/// 
///         log::info!("Physics hit {} {}", position_normal.0, position_normal.1);
///     }
/// }
/// ```
#[derive(Resource, Default)]
pub struct PhysicsQueries {
    new_queries: SharedList<PhysicsQueryInput>,
    results: DashMap<String, PhysicsStoredResult>
}

impl std::fmt::Debug for PhysicsQueries {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PhysicsQueries").field("results", &self.results.len()).finish_non_exhaustive()
    }
}

impl PhysicsQueries {
    /// Create a new query with no data.  This quey will be completed during the next
    /// physics tick.
    pub fn create_query(&self, id: impl Into<String>, query: PhysicsQuery) {
        let id = id.into();
        self.new_queries.push(PhysicsQueryInput { id, query, data: None });
    }
    
    /// Creates a new query with the given data.  This query will be completed during
    /// the next physics tick.
    pub fn create_query_with_data(&self, id: impl Into<String>, query: PhysicsQuery, data: Box<dyn Any>) {
        let id = id.into();
        self.new_queries.push(PhysicsQueryInput { id, query, data: Some(data) });
    }

    /// Consumes a query of the given ID.  If a query of the given ID has been completed, 
    /// it will be removed from the internal storage and returned.
    pub fn consume_query(&self, id: impl Into<String>) -> Option<(Vec<PhysicsQueryResult>, Ref<Option<Box<dyn Any>>>)> {
        let Some((_id, result)) = self.results.remove(&id.into()) else { return None };
        return Some((result.results, result.data));
    }

    /// The general update function to complete active queries.
    pub fn execute_queries(&self, engine: &PhysicsEngine, query_pipeline: &QueryPipeline) {
        for query in self.new_queries.drain() {
            // unpack
            let id = query.id.clone();
            let query_instruction = (&*query).query.clone();
            let data = Ref::new(query, |query| &query.downcast_ref::<PhysicsQueryInput>().unwrap().data);

            // match to query pipeline
            let results = match query_instruction {
                PhysicsQuery::Ray { 
                    eye, dir, max_dist, 
                    solid, stop_at_first 
                } => {
                    // create
                    let ray = Ray::new(
                        Point3::new(eye.x, eye.y, eye.z), 
                        Vector3::new(dir.x, dir.y, dir.z)
                    );

                    // run single intersection or results
                    if stop_at_first {
                        Some(
                            query_pipeline.cast_ray_and_get_normal(&ray, max_dist, solid)
                                .into_iter()
                                .filter_map(|(handle, intersection)| {
                                    let Some(physics_id) = engine.collider_map.get_by_right(&handle) else { return None };
                                    let Some(entity_id) = engine.entity_id(*physics_id) else { return None };
                                    let hit_point = ray.point_at(intersection.time_of_impact);
                                    let hit_normal = intersection.normal;

                                    Some(PhysicsQueryResult { 
                                        hit_position_normal: Some((
                                            Vec3::new(hit_point.x, hit_point.y, hit_point.z),
                                            Vec3::new(hit_normal.x, hit_normal.y, hit_normal.z)
                                        )),
                                        entity_id
                                    })
                                })
                                .collect::<Vec<_>>()
                        )
                    } else {
                        Some(
                            query_pipeline.intersect_ray(ray, max_dist, solid)
                                .into_iter()
                                .filter_map(|(handle, _, intersection)| {
                                    let Some(physics_id) = engine.collider_map.get_by_right(&handle) else { return None };
                                    let Some(entity_id) = engine.entity_id(*physics_id) else { return None };
                                    let hit_point = ray.point_at(intersection.time_of_impact);
                                    let hit_normal = intersection.normal;
                                    Some(PhysicsQueryResult { 
                                        hit_position_normal: Some((
                                            Vec3::new(hit_point.x, hit_point.y, hit_point.z),
                                            Vec3::new(hit_normal.x, hit_normal.y, hit_normal.z)
                                        )),
                                        entity_id
                                    })
                                })
                                .collect::<Vec<_>>()
                        )
                    }
                },
                PhysicsQuery::Shape { 
                    shape, 
                    position, rotation, 
                    max_dist, stop_at_first
                } => {
                    let axisangle = rotation.to_axis_angle();
                    let position = Isometry3::new(
                        Vector3::new(position.x, position.y, position.z), 
                        Vector3::new(axisangle.0.x, axisangle.0.y, axisangle.0.z)
                    );

                    let options = ShapeCastOptions {
                        max_time_of_impact: max_dist,
                        target_distance: 0.0,
                        stop_at_penetration: false,
                        compute_impact_geometry_on_penetration: false
                    };

                    if let Some(shape) = shape.into_rapier_collider().ok().flatten() {
                        let shape = shape.build();
                        let shape = shape.shape();

                        // run single intersection or results
                        if stop_at_first {
                            Some(
                                query_pipeline.cast_shape(&position, &Vector3::new(0.0, 0.0, 0.0), shape, options)
                                    .into_iter()
                                    .filter_map(|(handle, intersection)| {
                                        let Some(physics_id) = engine.collider_map.get_by_right(&handle) else { return None };
                                        let Some(entity_id) = engine.entity_id(*physics_id) else { return None };

                                        let hit_point = intersection.witness1;
                                        let hit_point = Vec3::new(hit_point.x, hit_point.y, hit_point.z);

                                        let hit_normal = intersection.normal1;
                                        let hit_normal = Vec3::new(hit_normal.x, hit_normal.y, hit_normal.z);

                                        Some(PhysicsQueryResult { 
                                            hit_position_normal: Some((
                                                Vec3::new(hit_point.x, hit_point.y, hit_point.z),
                                                Vec3::new(hit_normal.x, hit_normal.y, hit_normal.z)
                                            )),
                                            entity_id
                                        })
                                    })
                                    .collect::<Vec<_>>()
                            )
                        } else {
                            Some(
                                query_pipeline.intersect_shape(position, shape)
                                    .into_iter()
                                    .filter_map(|(handle, _)| {
                                        let Some(physics_id) = engine.collider_map.get_by_right(&handle) else { return None };
                                        let Some(entity_id) = engine.entity_id(*physics_id) else { return None };
                                        Some(PhysicsQueryResult { 
                                            hit_position_normal: None,
                                            entity_id
                                        })
                                    })
                                    .collect::<Vec<_>>()
                            )
                        }
                    } else { None }
                }
            };

            if let Some(results) = results {
                let result = PhysicsStoredResult { results, data, created_at: Utc::now() };
                self.results.insert(id, result);
            }
        }

        // only keep results less than 10s old
        let now = Utc::now();
        self.results.retain(|_, result| now.signed_duration_since(result.created_at).as_seconds_f32() < 10.0);
    }
}
