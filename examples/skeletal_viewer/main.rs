use std::path::PathBuf;

use anarchy::{Query, Res, ResMut, Resource, World};
use gearbox::{App, EguiCtx, EguiPlugin, egui::egui, spawn};
use gearbox::{AssetContent, AssetVault, LoadableAssetVault, MaterialRef, MeshRef, glam::*};
use gearbox::{Camera, GearboxRenderPlugin, Transform};
use gearbox::skeletal::anim::Animator;
use gearbox::skeletal::{SkeletalMeshHandle, SkeletalMeshLoadType, SkeletalMeshPlugin, SkeletalMeshVault};

#[derive(Debug, Resource)]
pub struct ViewerData {
    pub anim_mesh_handle_storage: Vec<SkeletalMeshHandle>,
    pub loop_animations: bool
}

fn main() -> gearbox::anyhow::Result<()> {
    App::new()
        .add_plugin(GearboxRenderPlugin)
        .add_plugin(SkeletalMeshPlugin)
        .add_plugin(EguiPlugin)
        .on_render_startup(setup)
        .on_render_update(update)
        .add_resource(ViewerData { anim_mesh_handle_storage: vec![], loop_animations: true })
        .run()
}

fn setup(
    world: World,
    meshes: Res<SkeletalMeshVault>
) {
    spawn(&world, vec![
        Box::new(Transform::new(Vec3::new(0.0, 10.0, 30.0), Quat::from_rotation_x(-0.5), Vec3::ONE)),
        Box::new(Camera::default()),
    ]);
    
    let Some(path) = get_path() else { return log::error!("No path provided") };
    let (model, material, anims) = match meshes.load(&world, AssetContent::LocalPath(path), SkeletalMeshLoadType::GLTF) {
        Ok(loaded) => loaded,
        Err(err) => return log::error!("Failed to load model: {err:#}"),
    };
    let mut animator = Animator::empty();
    animator.load_animations(anims);

    spawn(&world, vec![
        Box::new(Transform::new(Vec3::new(0.0, -1.0, 0.0), Quat::IDENTITY, Vec3::ONE * 3.0)),
        Box::new(MaterialRef::new(material)),
        Box::new(animator),
        Box::new(MeshRef::new(model)),
    ]);
}

#[cfg_attr(target_arch = "wasm32", allow(unused_variables))]
fn update(
    world: World,
    egui: Res<EguiCtx>,
    skvault: Res<SkeletalMeshVault>,
    mut query: Query<(&'static mut Animator, &'static MeshRef)>,
    mut data: ResMut<ViewerData>
) {
    // collected so the windows below can each walk the models
    let mut models = Vec::new();
    while let Some((_id, model)) = query.next().unwrap() {
        models.push(model);
    }

    egui::Window::new("Animations").show(&egui.context, |ui| {
        // the native file dialog isn't available on the web
        #[cfg(not(target_arch = "wasm32"))]
        if ui.button("Add animations...").clicked() {
            let Some(path) = rfd::FileDialog::new()
                .set_title("Select a GLTF/GLB file")
                .pick_file() else { return };

            let Ok((mesh, _, anims)) = skvault.load(&world, AssetContent::LocalPath(path), SkeletalMeshLoadType::GLTF)
                else { return };
            data.anim_mesh_handle_storage.push(mesh);
            for (animator, _) in models.iter_mut() {
                animator.load_animations(anims.clone());
            }
        }

        for (animator, _) in models.iter_mut() {
            let animations = animator.animations()
                .iter()
                .map(|a| a.0.clone())
                .collect::<Vec<_>>();

            ui.horizontal(|ui| {
                ui.label("Looping");
                ui.checkbox(&mut data.loop_animations, "");
            });

            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    for anim in animations.into_iter() {
                        if ui.button(&anim).clicked() {
                            animator.play(anim, data.loop_animations);
                        }
                    }
                });
        }
    });

    egui::Window::new("Mesh").show(&egui.context, |ui| {
        for (_, handle) in models.iter() {
            let Some(handle) = handle.0.as_any().downcast_ref::<SkeletalMeshHandle>() else { continue };
            let Some(mesh) = skvault.get(handle) else { continue };

            for (_, mesh) in mesh.meshes() {
                let mut is_visible = !handle.invisible_bones().contains(&mesh.label);

                if ui.checkbox(&mut is_visible, mesh.label.clone()).changed() {
                    if is_visible {
                        handle.invisible_bones().remove(&mesh.label);
                    } else {
                        handle.invisible_bones().insert(mesh.label.clone());
                    }
                }
            }
        }
    });
}

fn get_path() -> Option<PathBuf> {
    // Skip args[0] (the binary name) and look for the first real argument
    if let Some(arg) = std::env::args().nth(1) {
        Some(PathBuf::from(arg))
    } else {
        #[cfg(not(target_arch = "wasm32"))]
        return rfd::FileDialog::new()
            .set_title("Select a GLTF/GLB file")
            .pick_file();
        #[cfg(target_arch = "wasm32")]
        return None;
    }
}
