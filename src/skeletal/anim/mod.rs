use std::{collections::LinkedList, f32::consts::PI};

use ahash::AHashMap;
use anarchy::Component;
use getset::Getters;
use anarchy::{Query, Res};
use crate::{AssetVault, Handle, RenderDeltaTime, Transform};
use magician_vgpu::glam::{Mat4, Quat, Vec3};

use crate::skeletal::{data::{Animation, Interpolation, ModelBone, PreProcessAnimation}, model::SkeletalMesh};

pub mod vault;

pub use vault::*;

/// A basic animator that allows for low level control of the current animation
/// state of a `SkeletalMesh` or anything else that uses this `Animator`.
#[derive(Getters, Component)]
#[getset(get = "pub")]
pub struct Animator {
    animations: AHashMap<String, Animation>,
    load_queue: LinkedList<Handle<AnimationSet>>,
    state: Option<AnimatorState>
}

impl std::fmt::Debug for Animator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Animator")
            .field("animations", &self.animations.keys().collect::<Vec<_>>())
            .field("queued", &self.load_queue.len())
            .field("state", &self.state)
            .finish()
    }
}

/// The general animation state of the `Animator`.
#[derive(Debug)]
pub struct AnimatorState {
    animation: String,
    /// Seconds played, advanced by `RenderDeltaTime` each frame.
    time: f32,
    is_looping: bool
}

impl Animator {
    /// Create a new `Animator` from the raw animation and state data.
    pub fn from_raw(
        animations: AHashMap<String, Animation>,
        state: Option<AnimatorState>
    ) -> Self {
        Self {
            animations,
            load_queue: LinkedList::new(),
            state
        }
    }

    /// Create a new empty animator with no animations.
    pub fn empty() -> Self {
        Self {
            animations: AHashMap::default(),
            load_queue: LinkedList::new(),
            state: None
        }
    }

    /// Create a new `Animator` for a `SkeletalMesh` and a map of `PreProcessAnimation`.
    /// This allows animations from any source to be applied to any `SkeletalMesh`.  The
    /// mesh provided should at a minimum be using the same skeleton as the object this
    /// `Animator` will be applied too.
    pub fn new(
        mesh: &SkeletalMesh,
        animations: &AHashMap<String, PreProcessAnimation>
    ) -> Self {
        let animations = animations.into_iter()
            .map(|(id, pre_process)| (id.clone(), Animation::from_preprocessed_animation(&pre_process, mesh.node_id_map(), true)))
            .collect::<AHashMap<_, _>>();

        Self {
            animations,
            load_queue: LinkedList::new(),
            state: None
        }
    }

    /// Queue an animation set to be added to this animator.  If an animation from this set is played
    /// before the animation is loaded, no animation will play until it is loaded.
    pub fn load_animations(
        &mut self,
        handle: Handle<AnimationSet>
    ) {
        self.load_queue.push_back(handle);
    }

    /// Add an animation to this `Animator`.
    pub fn add_animation(
        &mut self,
        id: impl Into<String>,
        animation: Animation
    ) {
        self.animations.insert(id.into(), animation);
    }

    /// Add a `PreProcessAnimation` to this `Animator`.  The given mesh should 
    /// at a minimum be using a similar skeleton to the mesh that this `Animator` 
    /// will be applied too.
    pub fn add_preprocessed_animation(
        &mut self,
        mesh: &SkeletalMesh,
        id: impl Into<String>,
        animation: PreProcessAnimation
    ) {
        self.animations.insert(id.into(), Animation::from_preprocessed_animation(&animation, mesh.node_id_map(), true));
    }

    /// Add an iterator of `Animation`s to this `Animator`.
    pub fn add_animations(
        &mut self,
        iter: impl Iterator<Item = (String, Animation)>
    ) {
        self.animations.extend(iter);
    }

    /// Add an iterator of `PreProcessAnimation`s to this `Animator`.  The 
    /// given mesh should at a minimum be using a similar skeleton to the 
    /// mesh that this `Animator` will be applied too.
    pub fn add_preprocessed_animations(
        &mut self,
        mesh: &SkeletalMesh,
        iter: impl Iterator<Item = (String, PreProcessAnimation)>
    ) {
        self.animations.extend(
            iter.map(|(id, pre_process)| {
                (
                    id,
                    Animation::from_preprocessed_animation(&pre_process, mesh.node_id_map(), true)
                )
            })
        );
    }

    /// Play a specific animation associated with the given ID.  If no animation
    /// with the given ID exists, no animation will play.  The `looping` flag
    /// determines if the animation should loop or should stop when the animation 
    /// is over.
    pub fn play(&mut self, anim_id: impl Into<String>, looping: bool) {
        self.state = Some(AnimatorState { 
            animation: anim_id.into(), 
            time: 0.0,
            is_looping: looping 
        })
    }

    /// Stop any active animations in this `Animator`.
    pub fn stop(&mut self) {
        self.state = None;
    }

    /// Internal function to drain and load the animation `load_queue`.  `AnimationSet` handles may
    /// be added back to the `load_queue` if the animations where not loaded into the vault yet.
    pub(crate) fn exec_anim_queue(&mut self, vault: &AnimationVault, parent: &SkeletalMesh) {
        // drain and load all queued animation handles
        let anim_handles = self.load_queue.clone();
        self.load_queue.clear();
        for handle in anim_handles.into_iter() {
            // get animations, add back to load queue on fail (implied not loaded yet)
            let Some(animations) = vault.get(&handle) else {
                self.load_queue.push_back(handle);
                continue;
            };

            for (anim_id, anim) in animations.iter() {
                let anim = Animation::from_preprocessed_animation(&anim, parent.node_id_map(), true);
                self.animations.insert(anim_id.clone(), anim);
            }
        }
    }

    /// Internal function for calculation the transform matrices of each `ModelBone`
    /// in a `SkeletalMesh` during rendering.
    pub(crate) fn animate(&mut self, bone: &ModelBone) -> Option<Vec<Mat4>> {
        // get animation, animation state, and run time
        let Some(state) = self.state.as_ref() else { return None };
        let Some(animation) = self.animations.get(&state.animation) else { return None };
        let anim_time = state.time;

        // is not looping and the animation is over, cancel it
        if !state.is_looping && anim_time > animation.length {
            self.state = None;
            return None;
        }

        // animate bone matrices
        let mut nodes = Vec::new();
        animate_matrices(
            &mut nodes, 
            bone, animation, 
            &Mat4::IDENTITY, 
            anim_time % animation.length
        );
        return Some(nodes);
    }
}

/// Render system that advances every playing `Animator` by `RenderDeltaTime`, so playback
/// follows the app's clock (and is repeatable with a fixed time step).
pub fn advance_animators(mut animators: Query<&'static mut Animator>, delta_time: Res<RenderDeltaTime>) {
    while let Some((_id, mut animator)) = animators.next().unwrap() {
        if let Some(state) = animator.state.as_mut() { state.time += delta_time.0; }
    }
}

/// For each node in the node tree, its global matrix is added to the node_matrices list
pub fn animate_matrices(
    node_matrices: &mut Vec<Mat4>,
    node: &ModelBone,
    animation: &Animation,
    parent: &Mat4,
    time: f32
) {
    // calculate animated matrix, falling back to the node's authored rest pose
    // (not identity) when it has no animation channel of its own - this keeps
    // unanimated attachment nodes (props parented to a bone) at their correct
    // offset instead of snapping to their parent bone's origin.
    let local_matrix = animation.channels
        .get(&(node.id as usize)).map(|channel| {
            let translation = channel.positions
                .as_ref()
                .map(|(a, b, c)| compute_vector_interpolation(a, &b, &c, time))
                .unwrap_or(Vec3::ZERO);
            let rotation = channel.rotations
                .as_ref()
                .map(|(a, b, c)| compute_rotation_interpolation(a, &b, &c, time))
                .unwrap_or(Quat::IDENTITY);
            let scale = channel.scale
                .as_ref()
                .map(|(a, b, c)| compute_vector_interpolation(a, &b, &c, time))
                .unwrap_or(Vec3::ONE);

            Transform::new(translation, rotation, scale).as_matix()
        }).unwrap_or(node.transform);

    // calculate final matrix
    let matrix = parent * local_matrix;

    // animate children
    for node in &node.children {
        animate_matrices(node_matrices, node, animation, &matrix, time);
    }

    // save bone
    if node.id as usize >= node_matrices.len() {
        node_matrices.resize(node.id as usize + 1, Mat4::IDENTITY);
    }
    node_matrices[node.id as usize] = matrix;
}

/// Gets the before and after frame using the timing array
/// The frames are selected at the same indices that the given time value is between in the timing array
/// 
/// Returns (the before frame, the after frame, the time in the frame as a percentage)
fn get_timed_first_last<'a, T>(
    timing: &[f32],
    values: &'a [T],
    time: f32
) -> (&'a T, &'a T, f32) {
    // restrict timing to in range
    let time = time % timing.last().unwrap_or(&0.0);

    // get indices
    let mut last_idx = 0;
    while timing[last_idx] <= time { last_idx += 1; }
    let first_idx = last_idx.checked_sub(1).unwrap_or(0);

    // compile results
    (&values[first_idx], &values[last_idx], (time - timing[first_idx]) / (timing[last_idx] - timing[first_idx]))
}

/// Computes the animated interpolation of a list of vectors
fn compute_vector_interpolation(
    interpolation: &Interpolation,
    timing: &[f32],
    vectors: &[Vec3],
    time: f32
) -> Vec3 {
    let (first, last, time) = get_timed_first_last(timing, vectors, time);

    // return interpolated vectors
    return match interpolation {
        Interpolation::Linear => (*first * (1.0 - time)) + (*last * time),
        Interpolation::Wave => {
            let time = ((time - 0.5) * PI).sin() * 0.5 + 0.5;
            (*first * (1.0 - time)) + (*last * time)
        },
        Interpolation::Step => *first
    }
}

/// Computes the animated interpolation of a list of quaternion
fn compute_rotation_interpolation(
    interpolation: &Interpolation,
    timing: &[f32],
    rotations: &[Quat],
    time: f32
) -> Quat {
    let (first, last, time) = get_timed_first_last(timing, rotations, time);

    // return interpolated rotations
    // https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#appendix-c-interpolation
    return match interpolation {
        Interpolation::Linear => first.slerp(*last, time),
        Interpolation::Wave => {
            // calculate time values then slerp the final rotation
            let time = ((time - 0.5) * PI).sin() * 0.5 + 0.5;
            first.slerp(*last, time)
        },
        Interpolation::Step => *first
    };
}
