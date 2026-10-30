use std::sync::Arc;

use anarchy::{EntityBuilder, ResMut, Resource, World};
use gearbox::{App, Camera, GearboxRenderPlugin, Transform, anyhow};
use gearbox::ui::*;
use magician_vgpu::glam::{self, Vec2};

/// Everything the widgets below edit.
#[derive(Debug, Resource)]
struct Settings {
    enabled: bool,
    rate: u32,
    scale: f64,
    name: String,
    clicks: u32,
    picked: Option<String>,
    number: u8,
    popup: bool
}

fn main() -> anyhow::Result<()> {
    let font = Arc::new(SDFFont::new(include_bytes!("./LiberationSans-Regular.ttf"))?);
    App::new()
        .add_plugin(GearboxRenderPlugin)
        .add_plugin(UIPlugin)
        .add_plugin(WidgetPlugin::new(font))
        .add_resource(Settings { enabled: true, rate: 60, scale: 1.0, name: String::new(), clicks: 0, picked: None, number: 7, popup: false })
        .on_render_startup(setup)
        .on_render_update(settings_ui)
        .run()
}

fn setup(world: World) {
    // the UI draws over the camera's render
    world.spawn(
        EntityBuilder::new()
            .push(Transform::new(glam::Vec3::new(0.0, 0.0, 6.0), glam::Quat::IDENTITY, glam::Vec3::ONE))
            .push(Camera::default())
    );
}

fn settings_ui(mut widget: ResMut<Widget>, mut settings: ResMut<Settings>) {
    let settings = &mut *settings;
    widget.panel("Widgets", Vec2::splat(10.0), 320.0, |ui| {
        ui.checkbox(&mut settings.enabled, "Enabled");
        let enabled = settings.enabled;
        ui.log_slider("rate", &mut settings.rate, 1..=1000, enabled, " per second");
        ui.log_slider("scale", &mut settings.scale, 0.1..=10.0, enabled, " scale");
        ui.separator();

        ui.row(|ui| {
            ui.text_field("name", &mut settings.name, "Your name", 140.0);
            if ui.button_hint("Greet", "Counts the greetings") { settings.clicks += 1; }
            // opens on a right click of the widget added last
            ui.context_menu("greet", 160.0, |ui| {
                if ui.menu_item_shortcut("Greet", "Click") { settings.clicks += 1; }
                ui.submenu("Greet more", 120.0, |ui| {
                    if ui.menu_item("Twice") { settings.clicks += 2; }
                    if ui.menu_item("Five times") { settings.clicks += 5; }
                });
                ui.separator();
                if ui.menu_item("Reset") { settings.clicks = 0; }
            });
        });
        if settings.clicks > 0 { ui.label(&format!("Hello {} x{}", settings.name, settings.clicks)); }
        if settings.name.is_empty() { ui.colored_label("No name yet", ui.theme().error); }

        ui.row(|ui| {
            ui.label("Number");
            if let Some(typed) = ui.number_field("number", &settings.number.to_string(), 60.0)
                && let Ok(number) = typed.trim().parse() {
                settings.number = number;
            }
            if ui.button("Popup") { settings.popup = !settings.popup; }
        });
        ui.separator();

        let fruit = ["Apple", "Banana", "Cherry"].map(String::from).to_vec();
        let veg = (1..=20).map(|n| format!("Veg {n}")).collect();
        ui.sections("food", &[("Fruit", fruit, true), ("Vegetables", veg, true)], 10, |ui, name| {
            if ui.selectable(name, settings.picked.as_ref() == Some(name), name) { settings.picked = Some(name.clone()); }
        });
    });

    // popups draw over panels
    if settings.popup {
        widget.popup("picked", Vec2::new(200.0, 120.0), 180.0, |ui| {
            ui.label(&format!("Picked {}", settings.picked.as_deref().unwrap_or("nothing")));
            if ui.button("Close") { settings.popup = false; }
        });
    }
}
