use anarchy::{AsAny, EntityID, World, WorldImpl};
use anyhow::Context;
use getset::Getters;
use crate::{AssetVault, Handle, Mesh, MeshAssetVault, Transform, glam::Mat4};
use magician_vgpu::{BindGroupProvider, BindableObject, Buffer, DrawSettings, ImmutableBuffer, MutableBuffer, Pipeline, PipelineBuilder, ShaderSource, ShaderType, SinglePass, VirtualGpu, WritableBuffer};
use mutual::{CowData, DashSet};
use crate::shaders::skeletal::{AnimationInfo, AnimationInfoInput};
use wgpu::ShaderStages;

use crate::skeletal::{AnimationVault, Animator, MAX_SKELETAL_BONES, ModelBone, SkeletalAnimationBuffers, SkeletalMesh, SkeletalMeshVault, SkeletalMeshVertex, instance_buffer_layout, vertex_buffer_layout};

/// The GPU vertex/index buffers for a single sub-mesh of a `SkeletalMesh`.
/// Its `create_pipeline` panics because it is only ever drawn through
/// `SkeletalMeshHandle::draw`, which builds the real pipeline and binds the
/// per-frame animation data before delegating here.
#[derive(AsAny)]
pub struct SkeletalRenderableMesh {
    pub vertices: ImmutableBuffer<[SkeletalMeshVertex]>,
    pub indices: ImmutableBuffer<[u32]>,
}

impl Mesh for SkeletalRenderableMesh {
    fn create_pipeline<'a>(
        &self,
        _vgpu: &VirtualGpu,
        _world: &World
    ) -> anyhow::Result<PipelineBuilder<'a>>
    {
        panic!("Building pipeline from skeletal sub mesh")
    }

    fn draw(
        &self,
        _vgpu: &VirtualGpu,
        pass: &mut SinglePass,
        _world: &World,
        _entity: EntityID
    ) -> anyhow::Result<()> {
        pass.draw(
            &self.vertices,
            &self.indices,
            DrawSettings::default()
        );
        Ok(())
    }
}

/// The `Mesh` component put on an entity to render a `SkeletalMesh`. Wraps a
/// vault `handle` plus per-entity GPU state lazily built on first draw: the
/// `instance_buffer` (this entity's transform), `animation_buffers` (this
/// frame's bone/node matrices), and `invisible_bones`, a set of bone labels
/// whose sub-mesh should be skipped when drawing (see `recr_bone`). Cloning
/// a handle keeps the same underlying mesh but starts with fresh (null) GPU
/// state, since that state is per-entity, not shared.
#[derive(AsAny, Getters)]
#[getset(get = "pub")]
pub struct SkeletalMeshHandle {
    pub(crate) handle: Handle<SkeletalMesh>,
    pub(crate) instance_buffer: CowData<MutableBuffer<[Mat4]>>,
    pub(crate) animation_buffers: CowData<SkeletalAnimationBuffers>,
    pub(crate) invisible_bones: DashSet<String>
}

impl SkeletalMeshHandle {
    pub fn new(handle: Handle<SkeletalMesh>) -> Self {
        Self {
            handle,
            instance_buffer: CowData::null(),
            animation_buffers: CowData::null(),
            invisible_bones: DashSet::new()
        }
    }
}

impl std::fmt::Debug for SkeletalMeshHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SkeletalMeshHandle")
            .field("hash", &self.handle.inner().0)
            .field("invisible_bones", &self.invisible_bones.len())
            .finish_non_exhaustive()
    }
}

impl Clone for SkeletalMeshHandle {
    fn clone(&self) -> Self {
        Self {
            handle: self.handle.clone(),
            instance_buffer: CowData::null(),
            animation_buffers: CowData::null(),
            invisible_bones: DashSet::new()
        }
    }
}

impl Mesh for SkeletalMeshHandle {
    fn create_pipeline<'a>(
        &self,
        vgpu: &VirtualGpu,
        _world: &World
    ) -> anyhow::Result<PipelineBuilder<'a>> {
        Ok(
            Pipeline::builder("Skeletal Vertex Shader")
                .source(
                    ShaderType::Vertex,
                    ShaderSource {
                        source: crate::shaders::skeletal::SHADER_skeletal_vertex_main.into(),
                        main_function: "skeletal_vertex_main".into()
                    }
                )
                .vertex(vertex_buffer_layout())
                .vertex(instance_buffer_layout())
                .layout_raw::<crate::shaders::common::CameraInput>(2, crate::shaders::common::CameraInput::layout(vgpu, ShaderStages::VERTEX_FRAGMENT))
                .layout_raw::<crate::shaders::skeletal::AnimationInfoInput>(3, crate::shaders::skeletal::AnimationInfoInput::layout(vgpu, ShaderStages::VERTEX_FRAGMENT))
        )
    }

    fn draw(
        &self,
        vgpu: &VirtualGpu,
        pass: &mut SinglePass,
        world: &World,
        entity: EntityID
    ) -> anyhow::Result<()> {
        let skvault = world.resource::<SkeletalMeshVault>()
            .context("Could not find skeletal mesh vault")?;

        let mesh = skvault.get(self)
            .context("Failed to get mesh")?;

        let mesh_vault = world.resource::<MeshAssetVault>()
            .context("Missing mesh vault")?;
        let anim_vault = world.resource::<AnimationVault>()
            .context("Missing animation vault")?;

        // extract transform and animator components
        let (transform, mut animator) = world.get::<(&Transform, &mut Animator)>(entity)?
            .context("SkeletalMesh requires Transform and Animator companion components")?;

        // create instance matrix to draw
        let instances = [
            Mat4::from_scale_rotation_translation(
                transform.scale,
                transform.rotation,
                transform.translation
            )
        ];

        // create or update instance buffer
        if self.instance_buffer.is_null() {
            self.instance_buffer.set(MutableBuffer::new(
                vgpu,
                &instances,
                wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST
            ));
        } else {
            self.instance_buffer.get_ref().write(vgpu, &instances)
                .expect("Failed to update instance buffer");
        }

        // setup instances buffer (same for all bones, animations are handled through bones_bindable)
        pass.bind_instances(&*self.instance_buffer.get_ref());

        // execute animators animation queue before rendering
        animator.exec_anim_queue(&anim_vault, &mesh);

        // render each bone
        for bone in &mesh.bones {
            // get node transforms from the animator
            let node_transforms = animator.animate(bone);

            // generate skin matrices (world * inverse bind), used by actual skinned vertices
            let bones = if let (Some(skin), Some(nodes)) = (mesh.skin.as_ref(), node_transforms.as_ref()) {
                let mut bones = skin.iter()
                    .map(|(idx, ibp)| nodes[*idx as usize] * ibp)
                    .map(|a| a.into())
                    .collect::<Vec<magician_vgpu::rust::Mat4>>();
                bones.resize(MAX_SKELETAL_BONES, Mat4::IDENTITY.into());
                bones
            } else {
                vec![Mat4::IDENTITY.into(); MAX_SKELETAL_BONES]
            };

            // generate plain per-node world matrices, used by rigid (non-skinned)
            // attachments (hats, weapons, shields, etc) parented to a bone
            let node_mats = if let Some(nodes) = node_transforms.as_ref() {
                let mut node_mats = nodes.iter()
                    .map(|m| (*m).into())
                    .collect::<Vec<magician_vgpu::rust::Mat4>>();
                node_mats.resize(MAX_SKELETAL_BONES, Mat4::IDENTITY.into());
                node_mats
            } else {
                vec![Mat4::IDENTITY.into(); MAX_SKELETAL_BONES]
            };

            let info = AnimationInfo {
                bones: bones.as_slice().try_into().unwrap(),
                nodes: node_mats.as_slice().try_into().unwrap()
            };

            if self.animation_buffers.is_null() {
                let buffer = MutableBuffer::new(
                    vgpu, &info,
                    wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST
                );
                let bindable = BindableObject
                    ::<AnimationInfoInput>
                    ::from_inputs(vgpu, &buffer.buffer());
                self.animation_buffers.set(SkeletalAnimationBuffers { buffer, bindable });
            } else {
                self.animation_buffers.get_ref().buffer.write(vgpu, &info)
                    .expect("Failed to update animation buffers");
            }

            pass.bind_raw(3, &self.animation_buffers.get_ref().bindable.bind_group());


            recr_bone(
                vgpu,
                &mesh_vault,
                pass,
                world,
                entity,
                &self,
                &mesh,
                bone
            )?;
        }

        Ok(())
    }
}

/// Recursively draws `bone`'s sub-mesh (if any and not in `handle`'s
/// `invisible_bones`) then recurses into its children.
fn recr_bone(
    vgpu: &VirtualGpu,
    vault: &MeshAssetVault,
    pass: &mut SinglePass,
    world: &World,
    entity: EntityID,
    handle: &SkeletalMeshHandle,
    mesh: &SkeletalMesh,
    bone: &ModelBone,
) -> anyhow::Result<()> {
    // attempt to find bone mesh to draw
    let bone_mesh = bone.mesh.map(|a| mesh.meshes.get(&a)).flatten();
    if let Some(bone_mesh) = bone_mesh {
        if !handle.invisible_bones().contains(&bone_mesh.label) {
            // draw bone specific mesh
            if let Some(bone_mesh) = vault.get(&bone_mesh.mesh) {
                bone_mesh.draw(vgpu, pass, world, entity)?;
            }
        }
    }

    // draw children bones
    for child in &bone.children {
        recr_bone(vgpu, vault, pass, world, entity, handle, mesh, child)?;
    }

    Ok(())
}
