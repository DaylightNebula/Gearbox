use std::{borrow::Cow, path::PathBuf, sync::Arc};

use ahash::AHashMap;
use anarchy::World;
use base64::{Engine, prelude::BASE64_STANDARD};
use crate::Graphics;
use crate::{AssetContent, MeshAsset, MeshAssetVault, SimpleTexturedMaterial, TextureType, TextureVault};
use magician_vgpu::{ImmutableBuffer, VirtualGpu, glam::*};
use gltf::Gltf;

use crate::skeletal::{SkeletalRenderableMesh, data::*, model::*};

const OCTET_STREAM: &str = "data:application/octet-stream;base64,";
const PNG_STREAM: &str = "data:image/png;base64,";

/// Load an already-parsed `Gltf` document into a `SkeletalMesh` plus its animations.
/// Mirrors `loader::fbx::load` so the two formats produce identical engine data.
pub fn load<'a>(
    gltf: &Gltf,
    world: &World,
    vgpu: &Graphics,
    mesh_vault: &MeshAssetVault,
    asset_file: &PathBuf,
    source_file: &PathBuf,
    extra_buffer: Option<Cow<'_, [u8]>>,
    hash: u64
) -> anyhow::Result<(SkeletalMesh, AHashMap<String, PreProcessAnimation>)> {
    let mut node_id_map: AHashMap<String, usize> = AHashMap::new();
    let mut nodes: Vec<ModelBone> = Vec::new();
    let mut meshes = AHashMap::new();

    let texture_vault = TextureVault::current(world, vgpu)?;

    // get filename and parent folder
    let mut out_folder_path = asset_file.clone();
    out_folder_path.pop();

    // load buffers
    let buffers = Arc::new(unpack_buffers(&gltf, source_file, extra_buffer));
    println!("Loaded {} buffer(s)", buffers.len());

    // load textures
    let textures = gltf
        .textures()
        .into_iter()
        .map(|texture| {
            let buffers = buffers.clone();
            unpack_texture(source_file, &buffers, &texture)
        })
        .collect::<AHashMap<_, _>>();
    println!("Loaded {} gltf textures!", textures.len());

    // load nodes
    for scene in gltf.scenes() {
        for node in scene.nodes() {
            // load mesh
            let node = unpack_node(vgpu, &mut meshes, &mut node_id_map, mesh_vault, &buffers, &node, &out_folder_path, hash, 0);
            nodes.push(node);
        }
    }

    if gltf.meshes().len() > 0 { println!("[WARN] Extra unrendered meshes {}", gltf.meshes().len()); }

    // load materials
    let material = gltf
        .materials()
        .next()
        .map(|material| unpack_material(textures, &material))
        .and_then(|material| material.albedo_texture)
        .and_then(|material| texture_vault.load(world, AssetContent::Binary(material.into_boxed_slice()), TextureType::PNG).ok())
        .map(|handle| SimpleTexturedMaterial::new(handle));

    // load animations
    let animations = gltf
        .animations()
        .into_iter()
        .map(|animation| unpack_animation(&buffers, &animation))
        .collect::<AHashMap<_, _>>();
    println!("Gltf animation count {}", gltf.animations().len());
    println!("Loaded {} animations!", animations.len());
    
    // load skin
    let skin = gltf.skins().next().map(|skin| {
        // load inverse bind positions
        let ibp = skin
            .reader(|buffer| Some(&buffers[buffer.index()]))
            .read_inverse_bind_matrices()
            .unwrap()
            .map(|mat| Mat4::from_cols_array_2d(&mat));

        // load joints
        let joints = skin.joints().map(|a| a.index() as u16);

        // zip ibp and joints into a single vector
        joints.zip(ibp).collect::<Vec<_>>()
    });

    Ok((
        SkeletalMesh { 
            bones: nodes, node_id_map, 
            skin, meshes, material
        },
        animations
    ))
}

fn unpack_animation<'a>(
    buffers: &Vec<Vec<u8>>,
    animation: &gltf::Animation
) -> (String, PreProcessAnimation) {
    let mut channels = AHashMap::new();
    let mut length = 0.0_f32;

    // load all channels of the animation
    for ch in animation.channels() {
        let id = ch.target().node().name().expect("Animated nodes must be named").to_string();

        // get channel
        let mut channel = if channels.contains_key(&id) {
            channels.remove(&id).unwrap()
        } else {
            PreProcessChannel {
                positions: None,
                rotations: None,
                scale: None,
            }
        };

        // interpolation
        let interpolation = match ch.sampler().interpolation() {
            gltf::animation::Interpolation::Linear => Interpolation::Linear,
            gltf::animation::Interpolation::Step => Interpolation::Step,
            gltf::animation::Interpolation::CubicSpline => Interpolation::Wave,
        };

        // setup reader
        let reader = ch.reader(|buffer| Some(&buffers[buffer.index()]));

        // read timestamps
        let Some(timestamps) = reader.read_inputs() else {
            continue;
        };
        let timestamps = timestamps.collect::<Vec<_>>();

        if let Some(latest_timestamp) = timestamps.iter().max_by(|a, b| a.partial_cmp(b).unwrap()) {
            length = length.max(*latest_timestamp);
        }

        // update the channel based on the keyframes
        if let Some(outputs) = reader.read_outputs() {
            match outputs {
                gltf::animation::util::ReadOutputs::Translations(translations) => {
                    let translations = translations.map(|vec| vec.into());
                    channel.positions =
                        Some((interpolation, timestamps, translations.collect::<Vec<_>>()))
                }
                gltf::animation::util::ReadOutputs::Rotations(rotations) => {
                    let rotations = rotations
                        .into_f32()
                        .map(|rotation| Quat::from_array(rotation));
                    channel.rotations =
                        Some((interpolation, timestamps, rotations.collect::<Vec<_>>()));
                }
                gltf::animation::util::ReadOutputs::Scales(scales) => {
                    let scales = scales.map(|vec| vec.into());
                    channel.scale = Some((interpolation, timestamps, scales.collect::<Vec<_>>()));
                }
                gltf::animation::util::ReadOutputs::MorphTargetWeights(_) => {
                    println!("[WARN] Morph targets not supported!");
                }
            }
        }

        // save channel
        channels.insert(id, channel);
    }

    println!("Loaded animation {:?}", animation.name());

    (
        animation
            .name()
            .map(|a| a.to_string())
            .unwrap_or("".to_string()),
        PreProcessAnimation {
            name: animation.name().map(|a| a.into()),
            channels,
            length
        },
    )
}

fn unpack_node<'a>(
    vgpu: &VirtualGpu,
    meshes: &mut AHashMap<usize, SkeletalSubMesh>,
    node_id_map: &mut AHashMap<String, usize>,
    mesh_vault: &MeshAssetVault,
    buffers: &Vec<Vec<u8>>,
    node: &gltf::Node<'a>,
    out_folder_path: &PathBuf,
    hash: u64,
    depth: usize,
) -> ModelBone {
    // load transform
    let (position, rotation, scale) = node.transform().decomposed();
    let translation: Vec3 = position.into();
    let rotation: Quat = Quat::from_array(rotation);
    let scale: Vec3 = scale.into();
    let transform = Mat4::from_scale_rotation_translation(scale, rotation, translation);

    // load mesh if necessary
    if let Some(mesh) = node.mesh() {
        let (idx, asset) = unpack_mesh(vgpu, mesh_vault, buffers, &mesh, node.name().map(|a| a.to_string()), hash, node.index());
        meshes.insert(idx, asset);
    }

    // load children
    let children = node
        .children()
        .into_iter()
        .map(|child| unpack_node(vgpu, meshes, node_id_map, mesh_vault, buffers, &child, out_folder_path, hash, depth + 1))
        .collect();

    // save node ID to node ID tracking map
    if let Some(id) = node.name() {
        node_id_map.insert(id.to_string(), node.index());
    }

    // compile final model node
    ModelBone {
        id: node.index() as u16,
        transform,
        mesh: node
            .mesh()
            .map(|mesh| mesh.index()),
        children
    }
}

fn unpack_mesh<'mesh>(
    vgpu: &VirtualGpu,
    mesh_vault: &MeshAssetVault,
    buffers: &Vec<Vec<u8>>,
    mesh: &gltf::Mesh<'mesh>,
    label: Option<String>,
    hash: u64,
    parent_id: usize
) -> (usize, SkeletalSubMesh) {
    // create mesh
    let local_hash = hash + parent_id as u64;
    let handle = 
        if let Some(handle) = mesh_vault.get_handle(local_hash) { 
            handle 
        } else {
            let mut final_vertices = Vec::new();
            let mut final_indices = Vec::new();

            // load primitives
            mesh.primitives().into_iter().for_each(|primitive| {
                let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

                // read primitive
                let mut positions = reader
                    .read_positions()
                    .expect("Expecting positions in gltf.");
                let mut normals = reader.read_normals().expect("Expecting normals in gltf.");
                let mut tex_coords = reader
                    .read_tex_coords(0)
                    .expect("Expecting TexCoords(0) in gltf.")
                    .into_f32();
                let indices = reader.read_indices().expect("Expecting indices in gltf.");

                // read weights
                let weights = reader.read_weights(0);
                let mut weights = if weights.is_some() {
                    weights.unwrap().into_f32().collect::<Vec<_>>()
                } else {
                    vec![[0.0, 0.0, 0.0, 0.0]; positions.len()]
                }
                .into_iter();

                // read joints
                let joints = reader.read_joints(0);
                let mut joints = if joints.is_some() {
                    joints.unwrap().into_u16().collect::<Vec<_>>()
                } else {
                    vec![[parent_id as u16, 0, 0, 0]; positions.len()]
                }
                .into_iter();

                // load vertices
                let mut vertices = Vec::<SkeletalMeshVertex>::with_capacity(positions.len());
                while let Some(vertex) = positions.next() {
                    let normal = normals.next().unwrap();
                    let tex_coord = tex_coords.next().unwrap();
                    let weight = weights.next().unwrap();
                    let joints = joints.next().unwrap().map(|a| a as u32);

                    vertices.push(SkeletalMeshVertex {
                        position: Vec3::from_array(vertex).into(),
                        uvs: Vec2::from_array(tex_coord).into(),
                        normal: Vec3::from_array(normal).into(),
                        weights: Vec4::from_array(weight).into(),
                        joints: UVec4::from_array(joints).into()
                    });
                }

                // load indices
                let indices_offset = final_indices.len() as u32;
                let indices = indices
                    .into_u32()
                    .map(|a| a as u32 + indices_offset)
                    .collect::<Vec<_>>();

                // create mesh
                println!("Load gltf mesh with {} vertices and {} indices", vertices.len(), indices.len());
                final_vertices.extend(vertices);
                final_indices.extend(indices);
            });

            mesh_vault.load_raw(local_hash, MeshAsset(Box::new(SkeletalRenderableMesh { 
                vertices: ImmutableBuffer::new(vgpu, &final_vertices, wgpu::BufferUsages::VERTEX),
                indices: ImmutableBuffer::new(vgpu, &final_indices, wgpu::BufferUsages::INDEX)
            })))
        };
    let m_idx = mesh.index();
    let mesh = SkeletalSubMesh {
        mesh: handle,
        label: label.unwrap_or_else(|| format!("#unnamed_{}", mesh.index()))
    };

    (m_idx, mesh)
}

fn unpack_material(textures: AHashMap<usize, Vec<u8>>, material: &gltf::Material) -> SkeletalMaterial {
    let pbr = material.pbr_metallic_roughness();
    SkeletalMaterial {
        albedo_texture: pbr.base_color_texture().map(|texture| textures.get(&texture.as_ref().index()).cloned().unwrap()),
        roughness_texture: pbr.metallic_roughness_texture().map(|texture| textures.get(&texture.as_ref().index()).cloned().unwrap()),
        emissive_texture: material.emissive_texture().map(|texture| textures.get(&texture.as_ref().index()).cloned().unwrap()),
        normal_texture: material.normal_texture().map(|texture| textures.get(&texture.as_ref().index()).cloned().unwrap()),
        occlusion_texture: material.occlusion_texture().map(|texture| textures.get(&texture.as_ref().index()).cloned().unwrap()),
        albedo_color: pbr.base_color_factor().into(),
        emissive_color: material.emissive_factor().into(),
        metallic_factor: pbr.metallic_factor().into(),
        roughness_factor: pbr.roughness_factor().into(),
        alpha_mode: match material.alpha_mode() {
            gltf::material::AlphaMode::Opaque => 1.0,
            gltf::material::AlphaMode::Mask => 2.0,
            gltf::material::AlphaMode::Blend => 3.0,
        },
        alpha_cutoff: material.alpha_cutoff().unwrap_or(0.0),
    }
}

fn unpack_texture(
    source_file: &PathBuf,
    buffers: &Vec<Vec<u8>>,
    texture: &gltf::Texture,
) -> (usize, Vec<u8>) {
    match texture.source().source() {
            gltf::image::Source::View { view, .. } => {
                let start = view.offset();
                let end = view.offset() + view.length();
                let buffer = buffers[view.buffer().index()][start..end].to_vec();
                println!("Loading texture with {} bytes", buffer.len());
                let t_idx = texture.index();
                (t_idx, buffer)
            }
            gltf::image::Source::Uri { uri, .. } => {
                if uri.starts_with(PNG_STREAM) {
                    let bytes = BASE64_STANDARD.decode(uri.split_at(PNG_STREAM.len()).1);
                    let bytes = bytes.expect("Could not load octet stream");
                    let t_idx = texture.index();
                    (t_idx, bytes)
                } else if uri.starts_with("http") {
                    todo!("Load textures from URLs");
                } else {
                    let mut file = source_file.clone();
                    file.pop();
                    file.push(uri);
                    let bytes = std::fs::read(&file)
                        .expect(format!("Failed to load texture file {file:?}").as_str());
                    let t_idx = texture.index();
                    (t_idx, bytes)
                }
            }
        }
}

fn unpack_buffers(
    gltf: &Gltf, 
    source_file: &PathBuf,
    mut extra_buffer: Option<Cow<'_, [u8]>>
) -> Vec<Vec<u8>> {
    let mut buffer_data = Vec::new();

    for buffer in gltf.buffers() {
        println!("Attempting to load buffer {} {} {:?}", buffer.index(), buffer.length(), buffer.source());

        match buffer.source() {
            gltf::buffer::Source::Uri(uri) => {
                if uri.starts_with(OCTET_STREAM) {
                    let bytes = BASE64_STANDARD.decode(uri.split_at(OCTET_STREAM.len()).1);
                    let bytes = bytes.expect("Could not load octet stream");
                    buffer_data.push(bytes);
                } else if uri.starts_with("http") {
                    todo!("Load from URLs");
                } else {
                    let mut bin_path = source_file.clone();
                    bin_path.pop();
                    bin_path.push(uri);
                    let contents = std::fs::read(&bin_path)
                        .expect(format!("Failed to load file at {bin_path:?}").as_str());
                    buffer_data.push(contents);
                }
            }
            gltf::buffer::Source::Bin => {
                println!("Blob {:?} {:?}", gltf.blob.is_some(), extra_buffer.is_some());
                if let Some(blob) = gltf.blob.as_deref() {
                    buffer_data.push(blob.into());
                } else if let Some(blob) = extra_buffer.take() {
                    buffer_data.push(blob.into());
                }
            }
        }
    }

    return buffer_data;
}
