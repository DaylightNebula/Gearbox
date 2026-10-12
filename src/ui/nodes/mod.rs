use anarchy::Component;
use derive_more::{Deref, DerefMut};

use crate::ui::SDFElement;

pub mod data;
pub mod events;
pub mod render;

pub use data::*;
pub use events::*;
pub use render::*;

/// Root of a UINode that is to be rendered as SDFs.
#[derive(Debug, Default, Deref, DerefMut, Component)]
pub struct UINodeSDFRoot(pub UINode);

/// A component of raw elements that should be rendered.
/// This contains an element list that will be loaded and
/// rendered via the `UIPlugin`.  Multiple `UIRawElements`
/// will be rendered in order of priority with lowest first.
/// Elements from `UINodeSDFRoot` are treated as have a
/// priority value of zero.
#[derive(Default, Component)]
pub struct UIRawElements {
    pub elements: Vec<SDFElement>,
    pub priority: i32,
}

impl std::fmt::Debug for UIRawElements {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UIRawElements").field("elements", &self.elements.len()).field("priority", &self.priority).finish()
    }
}
