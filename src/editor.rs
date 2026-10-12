//! A basic egui editor for viewing and editing components through anarchy's reflection,
//! enabled with the `editor` feature.

use anarchy::{ComponentID, EntityID, Res, ResMut, Resource, TypeInfo, TypeKind, Value, VariantKind, World};
use egui::{Color32, RichText, Ui};
use magician_vgpu::glam::{EulerRot, Quat};

use crate::{App, EguiCtx, EguiPlugin, Plugin};

/// Plugin that draws an entity list and a component inspector with egui each frame.
/// Components show up once they derive [`Reflect`](anarchy::Reflect) (non-generic types
/// are registered automatically). Added by [`App::new`] when the `editor` feature is on.
pub struct EditorPlugin;
impl Plugin for EditorPlugin {
    fn build(self, app: App) -> App {
        app.add_plugin(EguiPlugin)
            .add_resource(EditorState::default())
            .on_render_update(draw_editor)
    }
}

/// The editor's UI state.
#[derive(Debug, Default, Resource)]
pub struct EditorState {
    /// The entity shown in the inspector.
    pub selected: Option<EntityID>,
    /// The last edit that failed, shown in the inspector until the next edit.
    pub error: Option<String>
}

fn draw_editor(
    world: World,
    egui: Res<EguiCtx>,
    mut state: ResMut<EditorState>
) {
    let mut entities: Vec<EntityID> = world.entity_ids().collect();
    entities.sort_unstable();
    if state.selected.is_some_and(|selected| !entities.contains(&selected)) {
        state.selected = None;
    }

    egui::Window::new("Entities").default_pos([16.0, 16.0]).show(&egui.context, |ui| {
        egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
            for &entity in &entities {
                let names = components(&world, entity).into_iter()
                    .filter_map(|(_, info)| info.map(|(name, _)| name))
                    .collect::<Vec<_>>()
                    .join(", ");
                let selected = state.selected == Some(entity);
                if ui.selectable_label(selected, format!("Entity {entity}  {names}")).clicked() {
                    state.selected = Some(entity);
                }
            }
        });
    });

    let Some(entity) = state.selected else { return };
    let mut edits = Vec::new();
    egui::Window::new(format!("Entity {entity}")).id(egui::Id::new("gearbox_inspector")).default_pos([16.0, 460.0]).show(&egui.context, |ui| {
        if let Some(error) = &state.error {
            ui.label(RichText::new(error).color(ui.visuals().error_fg_color));
        }
        for (id, info) in components(&world, entity) {
            let Some((name, info)) = info else {
                ui.label(RichText::new(format!("component {id} (not reflected)")).weak());
                continue
            };
            let value = match world.reflect_component(entity, id, |component| component.to_value()) {
                Ok(value) => value,
                Err(error) => { ui.label(RichText::new(format!("{name}: {error:#}")).color(ui.visuals().error_fg_color)); continue }
            };
            let header = egui::CollapsingHeader::new(&name).id_salt(id).default_open(true).show(ui, |ui| {
                let mut changes = Vec::new();
                value_ui(ui, info, &[], &value, String::new(), &mut changes);
                edits.extend(changes.into_iter().map(|(path, value)| (id, path, value)));
            });
            if !info.docs.is_empty() { header.header_response.on_hover_text(info.docs); }
        }
    });

    for (id, path, value) in edits {
        state.error = world.set_component_path(entity, id, &path, &value).err().map(|error| format!("{path}: {error:#}"));
    }
}

/// The components on `entity`, with the name and info of those registered for reflection.
fn components(world: &World, entity: EntityID) -> Vec<(ComponentID, Option<(String, &'static TypeInfo)>)> {
    let ids = world.component_ids(entity).unwrap_or_default();
    // the registry guard is dropped before returning, as the world's reflect calls take it too
    let registry = world.type_registry();
    ids.into_iter()
        .map(|id| (id, registry.get_component(id).map(|registration| (registry.name_of(registration).to_string(), registration.info))))
        .collect()
}

/// `path` extended with the field `name`.
fn join(path: &str, name: &str) -> String {
    if path.is_empty() { name.to_string() } else { format!("{path}.{name}") }
}

/// The hint `key` as a number.
fn hint_number(hints: &[(&'static str, Value)], key: &str) -> Option<f64> {
    match hints.iter().find(|(k, _)| *k == key).map(|(_, value)| value)? {
        Value::Int(int) => Some(*int as f64),
        Value::Float(float) => Some(*float),
        _ => None
    }
}

/// Draws widgets for `value` of type `info` at `path`, pushing a `(path, value)` for each change.
fn value_ui(ui: &mut Ui, info: &'static TypeInfo, hints: &[(&'static str, Value)], value: &Value, path: String, edits: &mut Vec<(String, Value)>) {
    if info.type_name == std::any::type_name::<Quat>() {
        return quat_ui(ui, value, path, edits);
    }
    match &info.kind {
        TypeKind::Value => leaf_ui(ui, hints, value, path, edits),
        // small nested structs of plain values, like vectors, fit on one line
        TypeKind::Struct(kind) if !path.is_empty() && kind.fields().len() <= 4 && kind.fields().iter().all(|field| matches!((field.ty)().kind, TypeKind::Value)) => {
            ui.horizontal(|ui| {
                for (field, field_value) in kind.fields().iter().zip(field_values(kind, value)) {
                    let label = ui.label(field.name);
                    if !field.docs.is_empty() { label.on_hover_text(field.docs); }
                    leaf_ui(ui, &field.hints, field_value, join(&path, field.name), edits);
                }
            });
        }
        TypeKind::Struct(kind) => fields_ui(ui, kind, value, &path, edits),
        TypeKind::Enum(variants) => {
            let Value::Enum { variant, value: fields } = value else { return leaf_ui(ui, hints, value, path, edits) };
            egui::ComboBox::from_id_salt(("gearbox_variant", &path)).selected_text(variant.as_str()).show_ui(ui, |ui| {
                for option in variants {
                    if ui.selectable_label(option.name == variant, option.name).on_hover_text(option.docs).clicked() && option.name != variant {
                        edits.push((path.clone(), Value::Enum { variant: option.name.into(), value: Box::new(Value::Unit) }));
                    }
                }
            });
            if let Some(current) = variants.iter().find(|option| option.name == variant) && !current.kind.fields().is_empty() {
                fields_ui(ui, &current.kind, fields, &path, edits);
            }
        }
        TypeKind::List { item, .. } => {
            let Value::List(items) = value else { return leaf_ui(ui, hints, value, path, edits) };
            ui.vertical(|ui| {
                if items.is_empty() { ui.label(RichText::new("empty").weak()); }
                for (i, item_value) in items.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("[{i}]"));
                        value_ui(ui, item(), &[], item_value, format!("{path}[{i}]"), edits);
                    });
                }
            });
        }
    }
}

/// The values of the fields in `kind`, in order, from a struct or tuple `value`.
fn field_values<'v>(kind: &VariantKind, value: &'v Value) -> Vec<&'v Value> {
    static MISSING: Value = Value::Unit;
    match value.unannotated() {
        Value::Struct(_) => kind.fields().iter().map(|field| value.field(field.name).unwrap_or(&MISSING)).collect(),
        Value::Tuple(fields) => fields.iter().collect(),
        _ => Vec::new()
    }
}

/// A grid of the fields in `kind`, with their values from `value`.
fn fields_ui(ui: &mut Ui, kind: &VariantKind, value: &Value, path: &str, edits: &mut Vec<(String, Value)>) {
    egui::Grid::new(("gearbox_fields", path)).num_columns(2).show(ui, |ui| {
        for (field, field_value) in kind.fields().iter().zip(field_values(kind, value)) {
            let label = ui.label(field.name);
            if !field.docs.is_empty() { label.on_hover_text(field.docs); }
            value_ui(ui, (field.ty)(), &field.hints, field_value, join(path, field.name), edits);
            ui.end_row();
        }
    });
}

/// A widget for a leaf value: a checkbox, number, text box or label.
fn leaf_ui(ui: &mut Ui, hints: &[(&'static str, Value)], value: &Value, path: String, edits: &mut Vec<(String, Value)>) {
    let range = hint_number(hints, "min").zip(hint_number(hints, "max"));
    match value {
        Value::Bool(current) => {
            let mut new = *current;
            if ui.checkbox(&mut new, "").changed() { edits.push((path, Value::Bool(new))); }
        }
        Value::Int(current) => {
            let mut new = *current as i64;
            let changed = match range {
                Some((min, max)) => ui.add(egui::Slider::new(&mut new, min as i64..=max as i64)).changed(),
                None => ui.add(egui::DragValue::new(&mut new)).changed()
            };
            if changed { edits.push((path, Value::Int(new as i128))); }
        }
        Value::Float(current) => {
            let mut new = *current;
            let changed = match range {
                Some((min, max)) => ui.add(egui::Slider::new(&mut new, min..=max)).changed(),
                None => ui.add(egui::DragValue::new(&mut new).speed(0.05)).changed()
            };
            if changed { edits.push((path, Value::Float(new))); }
        }
        Value::Str(current) => {
            let mut new = current.clone();
            if ui.text_edit_singleline(&mut new).changed() { edits.push((path, Value::Str(new))); }
        }
        other => { ui.label(RichText::new(other.to_string()).color(Color32::GRAY)); }
    }
}

/// A quaternion shown as XYZ Euler angles in degrees, written back as a whole quaternion.
fn quat_ui(ui: &mut Ui, value: &Value, path: String, edits: &mut Vec<(String, Value)>) {
    let component = |name| match value.field(name) { Some(Value::Float(f)) => *f as f32, Some(Value::Int(i)) => *i as f32, _ => 0.0 };
    let quat = Quat::from_xyzw(component("x"), component("y"), component("z"), component("w")).normalize();
    let (x, y, z) = quat.to_euler(EulerRot::XYZ);
    let mut angles = [x.to_degrees(), y.to_degrees(), z.to_degrees()];
    let mut changed = false;
    ui.horizontal(|ui| {
        for (angle, axis) in angles.iter_mut().zip(["x", "y", "z"]) {
            changed |= ui.add(egui::DragValue::new(angle).prefix(format!("{axis}: ")).suffix("°").speed(1.0)).changed();
        }
    });
    if changed {
        let [x, y, z] = angles.map(f32::to_radians);
        let quat = Quat::from_euler(EulerRot::XYZ, x, y, z);
        edits.push((path, Value::Struct(vec![
            ("x".into(), quat.x.into()),
            ("y".into(), quat.y.into()),
            ("z".into(), quat.z.into()),
            ("w".into(), quat.w.into())
        ])));
    }
}
