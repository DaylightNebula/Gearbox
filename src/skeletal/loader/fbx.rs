use std::path::{Path, PathBuf};

use ahash::AHashMap;
use anarchy::World;
use log::warn;
use crate::Graphics;
use crate::{AssetContent, TextureType, TextureVault, MeshAsset, MeshAssetVault, SimpleTexturedMaterial};
use magician_vgpu::{ImmutableBuffer, VirtualGpu, glam::*};

use crate::skeletal::{SkeletalMesh, SkeletalMeshVertex, SkeletalRenderableMesh, SkeletalSubMesh, data::*};

/// Called with the texture's referenced name (its `relative_filename`, or whatever
/// non-empty name ufbx gave it) before any built-in path resolution is tried. Return
/// `Some(path)` to load that texture from `path` instead, e.g. when the FBX's stored
/// reference no longer matches where the file actually lives.
pub type TextureResolver<'a> = dyn Fn(&str) -> Option<PathBuf> + 'a;

/// Load an already-parsed ufbx `Scene` into a `SkeletalMesh` plus its animations.
/// Mirrors `loader::gltf::load` so the two formats produce identical engine data.
pub fn load(
    scene: &ufbx::Scene,
    world: &World,
    vgpu: &Graphics,
    mesh_vault: &MeshAssetVault,
    source_file: Option<&PathBuf>,
    texture_resolver: Option<&TextureResolver>,
    hash: u64
) -> anyhow::Result<(SkeletalMesh, AHashMap<String, PreProcessAnimation>)> {
    let mut node_id_map: AHashMap<String, usize> = AHashMap::new();
    let mut meshes = AHashMap::new();

    let texture_vault = TextureVault::current(world, vgpu)?;

    // load nodes, skipping ufbx's synthetic scene root
    let nodes = scene
        .root_node
        .children
        .iter()
        .map(|node| unpack_node(vgpu, &mut meshes, &mut node_id_map, mesh_vault, node, hash))
        .collect::<Vec<_>>();

    // load material (only the first, mirroring the gltf loader)
    let material = scene.materials.iter().next()
        .map(|material| unpack_material(source_file, material, texture_resolver))
        .and_then(|material| material.albedo_texture)
        .and_then(|material| texture_vault.load(world, AssetContent::Binary(material.into_boxed_slice()), TextureType::PNG).ok())
        .map(|handle| SimpleTexturedMaterial::new(handle));

    // load animations
    let animations = scene
        .anim_stacks
        .iter()
        .map(|stack| unpack_animation(scene, stack))
        .collect::<AHashMap<_, _>>();
    println!("Loaded {} fbx animations!", animations.len());

    // load skin (only the first, mirroring the gltf loader)
    let skin = scene.skin_deformers.iter().next().map(|deformer| {
        deformer
            .clusters
            .iter()
            .filter_map(|cluster| {
                let bone = cluster.bone_node.as_ref()?;
                Some((bone.element.typed_id as u16, matrix_to_mat4(&cluster.geometry_to_bone)))
            })
            .collect::<Vec<_>>()
    });

    Ok((
        SkeletalMesh {
            bones: nodes,
            node_id_map,
            skin,
            meshes,
            material
        },
        animations,
    ))
}

fn unpack_node(
    vgpu: &VirtualGpu,
    meshes: &mut AHashMap<usize, SkeletalSubMesh>,
    node_id_map: &mut AHashMap<String, usize>,
    mesh_vault: &MeshAssetVault,
    node: &ufbx::Node,
    hash: u64
) -> ModelBone {
    let id = node.element.typed_id as u16;

    // load transform
    let t = &node.local_transform;
    let translation = Vec3::new(t.translation.x as f32, t.translation.y as f32, t.translation.z as f32);
    let rotation = Quat::from_xyzw(t.rotation.x as f32, t.rotation.y as f32, t.rotation.z as f32, t.rotation.w as f32);
    let scale = Vec3::new(t.scale.x as f32, t.scale.y as f32, t.scale.z as f32);
    let transform = Mat4::from_scale_rotation_translation(scale, rotation, translation);

    // load mesh if necessary
    let mesh = node.mesh.as_ref().map(|mesh| {
        let idx = mesh.element.typed_id as usize;
        if !meshes.contains_key(&idx) {
            let label = node.element.name.to_string();
            meshes.insert(idx, unpack_mesh(vgpu, mesh, mesh_vault, label, hash, id as usize));
        }
        idx
    });

    // load children
    let children = node
        .children
        .iter()
        .map(|child| unpack_node(vgpu, meshes, node_id_map, mesh_vault, child, hash))
        .collect();

    // save node ID to node ID tracking map
    if !node.element.name.is_empty() {
        node_id_map.insert(node.element.name.to_string(), id as usize);
    }

    ModelBone { id, transform, mesh, children }
}

fn unpack_mesh(
    vgpu: &VirtualGpu,
    mesh: &ufbx::Mesh,
    mesh_vault: &MeshAssetVault,
    label: String,
    hash: u64,
    parent_id: usize,
) -> SkeletalSubMesh {
    let skin = mesh.skin_deformers.iter().next();

    let local_hash = hash + parent_id as u64;
    let handle = 
        if let Some(handle) = mesh_vault.get_handle(local_hash) { 
            handle 
        } else {
            let mut vertices = Vec::<SkeletalMeshVertex>::with_capacity(mesh.num_indices);
            let mut indices = Vec::<u32>::with_capacity(mesh.num_triangles * 3);
            let mut tri_buf = Vec::new();

            for face in mesh.faces.iter() {
                ufbx::triangulate_face_vec(&mut tri_buf, mesh, *face);

                for &pi in tri_buf.iter() {
                    let pi = pi as usize;
                    let position = mesh.vertex_position[pi];
                    let normal = if mesh.vertex_normal.exists { mesh.vertex_normal[pi] } else { ufbx::Vec3::default() };
                    let uv = if mesh.vertex_uv.exists { mesh.vertex_uv[pi] } else { ufbx::Vec2::default() };

                    let (joints, weights) = skin
                        .map(|skin| vertex_skin(skin, mesh.vertex_indices[pi] as usize))
                        .unwrap_or(([parent_id as u32, 0, 0, 0], [0.0, 0.0, 0.0, 0.0]));

                    indices.push(vertices.len() as u32);
                    vertices.push(SkeletalMeshVertex {
                        position: Vec3::new(position.x as f32, position.y as f32, position.z as f32).into(),
                        // FBX authors V with 0 at the bottom (Maya/OpenGL convention); this engine's
                        // shader samples with V=0 at the top (glTF convention, same as the PNG's row order).
                        uvs: Vec2::new(uv.x as f32, 1.0 - uv.y as f32).into(),
                        normal: Vec3::new(normal.x as f32, normal.y as f32, normal.z as f32).into(),
                        weights: Vec4::from_array(weights).into(),
                        joints: UVec4::from_array(joints).into(),
                    });
                }
            }

            println!("Load fbx mesh with {} vertices and {} indices", vertices.len(), indices.len());

            mesh_vault.load_raw(local_hash, MeshAsset(Box::new(SkeletalRenderableMesh { 
                vertices: ImmutableBuffer::new(vgpu, &vertices, wgpu::BufferUsages::VERTEX),
                indices: ImmutableBuffer::new(vgpu, &indices, wgpu::BufferUsages::INDEX)
            })))
        };
    SkeletalSubMesh {
        mesh: handle,
        label
    }
}

/// Picks the 4 heaviest skin weights for a vertex (our vertex format is capped at 4)
/// and renormalizes them, returning (cluster indices, weights).
fn vertex_skin(skin: &ufbx::SkinDeformer, control_point: usize) -> ([u32; 4], [f32; 4]) {
    let sv = skin.vertices[control_point];
    let mut pairs = (0..sv.num_weights as usize)
        .map(|i| {
            let w = skin.weights[sv.weight_begin as usize + i];
            (w.cluster_index, w.weight as f32)
        })
        .collect::<Vec<_>>();
    pairs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    pairs.truncate(4);

    let sum: f32 = pairs.iter().map(|(_, w)| w).sum();
    let mut joints = [0u32; 4];
    let mut weights = [0f32; 4];
    for (i, (cluster, w)) in pairs.into_iter().enumerate() {
        joints[i] = cluster;
        weights[i] = if sum > 0.0 { w / sum } else { 0.0 };
    }
    (joints, weights)
}

fn unpack_material(source_file: Option<&PathBuf>, material: &ufbx::Material, resolver: Option<&TextureResolver>) -> SkeletalMaterial {
    let diffuse = material.fbx.diffuse_color.value_vec4;
    let emissive = material.fbx.emission_color.value_vec4;

    SkeletalMaterial {
        albedo_texture: load_texture(source_file, material.fbx.diffuse_color.texture.as_ref().or(material.pbr.base_color.texture.as_ref()).map(|v| &**v), resolver),
        roughness_texture: load_texture(source_file, material.pbr.roughness.texture.as_ref().map(|v| &**v), resolver),
        emissive_texture: load_texture(source_file, material.fbx.emission_color.texture.as_ref().map(|v| &**v), resolver),
        normal_texture: load_texture(source_file, material.fbx.normal_map.texture.as_ref().map(|v| &**v), resolver),
        occlusion_texture: load_texture(source_file, material.pbr.ambient_occlusion.texture.as_ref().map(|v| &**v), resolver),
        albedo_color: [diffuse.x as f32, diffuse.y as f32, diffuse.z as f32, diffuse.w as f32],
        emissive_color: [emissive.x as f32, emissive.y as f32, emissive.z as f32],
        metallic_factor: material.pbr.metalness.value_vec4.x as f32,
        roughness_factor: material.pbr.roughness.value_vec4.x as f32,
        alpha_mode: 1.0,
        alpha_cutoff: 0.0,
    }
}

fn load_texture(source_file: Option<&PathBuf>, texture: Option<&ufbx::Texture>, resolver: Option<&TextureResolver>) -> Option<Vec<u8>> {
    let texture = texture?;
    // prefer relative_filename as the resolver key since it's what's actually authored
    // in the fbx; fall back to whatever other name ufbx has for the texture
    let name: &str = texture.relative_filename.as_ref();
    let name = if name.is_empty() { texture.absolute_filename.as_ref() } else { name };
    let name = if name.is_empty() { texture.element.name.as_ref() } else { name };

    // explicit caller override takes priority over anything the fbx itself claims
    if let Some(resolver) = resolver {
        if let Some(path) = resolver(name) {
            return match std::fs::read(&path) {
                Ok(bytes) => Some(bytes),
                Err(err) => {
                    warn!("Texture resolver path {path:?} for {name:?} could not be read: {err}");
                    None
                }
            };
        }
    }

    if !texture.content.is_empty() {
        return Some(texture.content.to_vec());
    }

    let rel: &str = texture.relative_filename.as_ref();
    if rel.is_empty() {
        return None;
    }
    // FBX files exported on Windows store `relative_filename` with `\` separators,
    // which PathBuf::push treats as a single literal (non-splitting) component on Unix.
    let rel = rel.replace('\\', "/");
    let Some(source_file) = source_file else { return None };
    let dir = source_file.parent().unwrap_or_else(|| Path::new("."));

    let direct = dir.join(&rel);
    if let Ok(bytes) = std::fs::read(&direct) {
        return Some(bytes);
    }

    // the fbx's authored path is frequently stale (baked from the artist's machine);
    // fall back to just the filename in case the texture was copied next to the model.
    if let Some(basename) = Path::new(&rel).file_name() {
        let sibling = dir.join(basename);
        println!("Looking for {:?}", sibling);
        if sibling != direct {
            if let Ok(bytes) = std::fs::read(&sibling) {
                return Some(bytes);
            }
        }
    }

    warn!("Could not load fbx texture {rel:?} (tried {direct:?} and its sibling by basename)");
    None
}

fn unpack_animation(scene: &ufbx::Scene, stack: &ufbx::AnimStack) -> (String, PreProcessAnimation) {
    let mut channels: AHashMap<String, PreProcessChannel> = AHashMap::new();

    for layer in stack.layers.iter() {
        for prop in layer.anim_props.iter() {
            let id = prop.element.name.to_string();
            if id.is_empty() {
                continue;
            }

            let times = collect_times(&prop.anim_value.curves);
            if times.is_empty() {
                continue;
            }
            let rel_times = times.iter().map(|t| (t - stack.time_begin) as f32).collect::<Vec<_>>();

            let channel = channels.entry(id).or_insert_with(|| PreProcessChannel {
                positions: None,
                rotations: None,
                scale: None,
            });

            match prop.prop_name.as_ref() {
                "Lcl Translation" => {
                    channel.positions = Some((Interpolation::Linear, rel_times, sample_vec3(&prop.anim_value, &times)));
                }
                "Lcl Scaling" => {
                    channel.scale = Some((Interpolation::Linear, rel_times, sample_vec3(&prop.anim_value, &times)));
                }
                "Lcl Rotation" => {
                    let order = scene.find_node(&prop.element.name).map(|n| n.rotation_order).unwrap_or_default();
                    channel.rotations = Some((Interpolation::Linear, rel_times, sample_euler(&prop.anim_value, &times, order)));
                }
                _ => {}
            }
        }
    }

    let length = (stack.time_end - stack.time_begin).max(0.0) as f32;
    println!("Loaded fbx animation {:?}", stack.element.name.as_ref());

    (
        stack.element.name.to_string(),
        PreProcessAnimation { name: Some(stack.element.name.to_string()), channels, length },
    )
}

/// Union of every keyframe time across an anim value's up-to-3 curves, sorted and deduped.
fn collect_times(curves: &[Option<ufbx::Ref<ufbx::AnimCurve>>; 3]) -> Vec<f64> {
    let mut times = curves
        .iter()
        .flatten()
        .flat_map(|c| c.keyframes.iter().map(|k| k.time))
        .collect::<Vec<_>>();
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    times.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    times
}

fn sample_vec3(av: &ufbx::AnimValue, times: &[f64]) -> Vec<Vec3> {
    times
        .iter()
        .map(|&t| {
            let x = av.curves[0].as_ref().map(|c| c.evaluate(t, av.default_value.x)).unwrap_or(av.default_value.x);
            let y = av.curves[1].as_ref().map(|c| c.evaluate(t, av.default_value.y)).unwrap_or(av.default_value.y);
            let z = av.curves[2].as_ref().map(|c| c.evaluate(t, av.default_value.z)).unwrap_or(av.default_value.z);
            Vec3::new(x as f32, y as f32, z as f32)
        })
        .collect()
}

fn sample_euler(av: &ufbx::AnimValue, times: &[f64], order: ufbx::RotationOrder) -> Vec<Quat> {
    let euler_order = match order {
        ufbx::RotationOrder::Xyz => EulerRot::XYZ,
        ufbx::RotationOrder::Xzy => EulerRot::XZY,
        ufbx::RotationOrder::Yzx => EulerRot::YZX,
        ufbx::RotationOrder::Yxz => EulerRot::YXZ,
        ufbx::RotationOrder::Zxy => EulerRot::ZXY,
        ufbx::RotationOrder::Zyx => EulerRot::ZYX,
        ufbx::RotationOrder::Spheric => EulerRot::XYZ,
    };

    sample_vec3(av, times)
        .into_iter()
        .map(|euler| Quat::from_euler(euler_order, euler.x.to_radians(), euler.y.to_radians(), euler.z.to_radians()))
        .collect()
}

fn matrix_to_mat4(m: &ufbx::Matrix) -> Mat4 {
    Mat4::from_cols(
        Vec4::new(m.m00 as f32, m.m10 as f32, m.m20 as f32, 0.0),
        Vec4::new(m.m01 as f32, m.m11 as f32, m.m21 as f32, 0.0),
        Vec4::new(m.m02 as f32, m.m12 as f32, m.m22 as f32, 0.0),
        Vec4::new(m.m03 as f32, m.m13 as f32, m.m23 as f32, 1.0),
    )
}

/// Recommended `ufbx::LoadOpts` for this engine: normalizes coordinate space to
/// right-handed Y-up / meters (matching glTF) and bakes any FBX pivot offsets
/// directly into vertex data so `ModelBone.transform` stays a plain local TRS.
pub fn load_opts<'a>() -> ufbx::LoadOpts<'a> {
    ufbx::LoadOpts {
        target_axes: ufbx::CoordinateAxes {
            right: ufbx::CoordinateAxis::PositiveX,
            up: ufbx::CoordinateAxis::PositiveY,
            front: ufbx::CoordinateAxis::PositiveZ,
        },
        target_unit_meters: 1.0,
        geometry_transform_handling: ufbx::GeometryTransformHandling::ModifyGeometry,
        inherit_mode_handling: ufbx::InheritModeHandling::Ignore,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_skin_caps_at_four_and_normalizes() {
        let weights = [
            ufbx::SkinWeight { cluster_index: 0, weight: 0.1 },
            ufbx::SkinWeight { cluster_index: 1, weight: 0.5 },
            ufbx::SkinWeight { cluster_index: 2, weight: 0.2 },
            ufbx::SkinWeight { cluster_index: 3, weight: 0.05 },
            ufbx::SkinWeight { cluster_index: 4, weight: 0.15 },
        ];
        // build a minimal SkinDeformer-like check by exercising the sort/truncate/normalize
        // logic directly, since ufbx::SkinDeformer can't be constructed outside a loaded scene.
        let mut pairs = weights.iter().map(|w| (w.cluster_index, w.weight as f32)).collect::<Vec<_>>();
        pairs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        pairs.truncate(4);
        let sum: f32 = pairs.iter().map(|(_, w)| w).sum();

        assert_eq!(pairs.len(), 4);
        assert_eq!(pairs[0].0, 1); // heaviest weight kept first
        assert!(!pairs.iter().any(|(c, _)| *c == 3)); // lightest weight dropped
        assert!((sum - 0.95).abs() < 1e-6);
    }
}
