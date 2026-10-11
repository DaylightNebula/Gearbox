use gearbox::anarchy::{Res, ResMut, Resource};
use gearbox::{App, EguiCtx, EguiPlugin, egui::egui};

#[derive(Debug, Resource)]
pub struct TestData {
    speed: f32
}

fn main() -> gearbox::anyhow::Result<()> {
    App::new()
        .add_plugin(EguiPlugin)
        .on_render_update(on_render_update)
        .add_resource(TestData { speed: 0.5 })
        .run()
}

fn on_render_update(
    egui: Res<EguiCtx>,
    mut test_data: ResMut<TestData>
) {
    egui::Window::new("Debug").show(&egui.context, |ui| {
        ui.label("Hello from egui!");
        ui.add(egui::Slider::new(&mut test_data.speed, 0.0..=1.0).text("Speed"));
    });
}
