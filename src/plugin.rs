use crate::App;

/// Represents a plugin that can be added to an `App`.  This is meant to allow
/// for condensing and standardizing sharing and adding functionality to an `App`.
pub trait Plugin {
    /// Consuming this plugin to add it to the given `App`.
    fn build(self, app: App) -> App;
}