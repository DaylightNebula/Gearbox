use std::ops::{Deref, DerefMut};

use anarchy::Resource;
use derive_more::{Deref, DerefMut};
use magician_vgpu::{RenderFrame, VirtualGpu};

/// Resource wrapping the app's `VirtualGpu`. Inserted into the `World` once the window has
/// been created, giving systems access to the device, queue, surface, and window.
#[derive(Resource, Deref, DerefMut)]
pub struct Graphics(pub(crate) VirtualGpu);

impl std::fmt::Debug for Graphics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Graphics").finish_non_exhaustive()
    }
}

/// Resource holding the `RenderFrame` currently in progress, if any. Populated at the start of
/// each render frame and taken back out to be submitted once the render schedule has run;
/// dereferencing while empty panics.
#[derive(Resource, Default)]
pub struct Frame(Option<RenderFrame>);

impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Frame").field("in_progress", &self.0.is_some()).finish()
    }
}

impl Frame {
    /// Store the frame for the current render pass, replacing any previous one.
    pub fn store(&mut self, frame: RenderFrame) {
        self.0 = Some(frame);
    }

    /// Take the current frame out, leaving this resource empty.
    pub fn take(&mut self) -> Option<RenderFrame> {
        self.0.take()
    }
}

impl Deref for Frame {
    type Target = RenderFrame;
    fn deref(&self) -> &Self::Target {
        self.0.as_ref().expect("Empty frame")
    }
}

impl DerefMut for Frame {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.as_mut().expect("Empty frame")
    }
}
