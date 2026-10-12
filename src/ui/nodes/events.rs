//! Events sent as the pointer moves over and presses `UINode`s with ids. Read them with an
//! `Event<E>` system parameter.

/// The pointer moved onto the node `id`.
#[derive(Debug, Clone)]
pub struct UINodeStartHoverEvent { pub id: String }

/// The pointer left the node `id`.
#[derive(Debug, Clone)]
pub struct UINodeEndHoverEvent { pub id: String }

/// The left button went down over the node `id`.
#[derive(Debug, Clone)]
pub struct UINodePressedEvent { pub id: String }

/// The left button went up, or the pointer left, while the node `id` was pressed.
#[derive(Debug, Clone)]
pub struct UINodeReleasedEvent { pub id: String }
