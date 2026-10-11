use std::sync::{Arc, atomic::Ordering};

use web_time::Instant;
use winit::{
    application::ApplicationHandler,
    event::StartCause,
    event_loop::{ActiveEventLoop, ControlFlow},
    window::{Window, WindowId},
};

use crate::{App, DeviceEvent, Pacer, RenderRate, WindowEvent};

/// How long to wait before retrying a frame that couldn't be drawn, e.g. while the window is
/// hidden. Becoming visible again or resizing retries straight away.
#[cfg(not(target_arch = "wasm32"))]
const SKIPPED_FRAME_RETRY: std::time::Duration = std::time::Duration::from_millis(100);

pub(crate) struct AppWrapper {
    pub(crate) app: App,
    /// Applies the [`RenderRate`] cap.
    pub(crate) render_pacer: Pacer,
}

impl AppWrapper {
    /// Returns the deadline to wait for if the render rate cap says it's too early to render,
    /// otherwise claims the current frame slot and returns `None`.
    fn frame_too_early(&mut self) -> Option<Instant> {
        // web redraws are already paced by requestAnimationFrame
        if cfg!(target_arch = "wasm32") { return None }
        let rate = self.app.world.resource::<RenderRate>().map(|rate| rate.0).unwrap_or(0);
        self.render_pacer.too_early(rate)
    }

    /// Request a redraw, if the window exists yet.
    fn request_redraw(&self) {
        if let Ok(graphics) = self.app.world.resource::<crate::Graphics>() {
            graphics.window().request_redraw();
        }
    }

    /// Try a skipped frame again later, rather than redrawing in a busy loop.
    #[allow(unused_variables)]
    fn retry_skipped_frame(&self, event_loop: &ActiveEventLoop) {
        // natively wake up after a delay, `new_events` then requests the redraw
        #[cfg(not(target_arch = "wasm32"))]
        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + SKIPPED_FRAME_RETRY));
        // web redraws are paced by requestAnimationFrame, so they can't spin
        #[cfg(target_arch = "wasm32")]
        self.request_redraw();
    }

    fn present_mode(&self) -> wgpu::PresentMode {
        if self.app.vsync { wgpu::PresentMode::AutoVsync } else { wgpu::PresentMode::AutoNoVsync }
    }
}

impl ApplicationHandler<()> for AppWrapper {
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        // a frame was deferred by the render rate cap, its time has come
        if let StartCause::ResumeTimeReached { .. } = cause {
            event_loop.set_control_flow(ControlFlow::Wait);
            self.request_redraw();
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        #[allow(unused_mut)]
        let mut window_attributes = Window::default_attributes();

        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::{JsCast, UnwrapThrowExt};
            use winit::platform::web::WindowAttributesExtWebSys;

            const CANVAS_ID: &str = "canvas";

            let window = wgpu::web_sys::window().unwrap_throw();
            let document = window.document().unwrap_throw();
            let canvas = document.get_element_by_id(CANVAS_ID).unwrap_throw();
            let html_canvas_element = canvas.unchecked_into();
            window_attributes = window_attributes.with_canvas(Some(html_canvas_element));
        }

        let window = Arc::new(event_loop.create_window(window_attributes).unwrap());

        #[cfg(not(target_arch = "wasm32"))]
        {
            // If we are not on web we can use pollster to
            // await the window creation
            // self.state = Some(pollster::block_on(State::new(window)).unwrap());

            use crate::{Frame, Graphics};
            use magician_vgpu::VirtualGpu;

            let size = window.inner_size();
            let mut vgpu = pollster::block_on(VirtualGpu::new(window));
            vgpu.config_mut().present_mode = self.present_mode();
            self.app.world.insert_resource(Graphics(vgpu));
            self.app.world.insert_resource(Frame::default());
            // configure now, some platforms (macOS) request a redraw before the first resize
            crate::app::configure_surface(&self.app.world, size.width, size.height);
        }

        #[cfg(target_arch = "wasm32")]
        {
            // wasm can't block on the GPU setup future, so spawn it and hand the
            // resources to the (shared, clonable) world once it resolves
            use crate::{Frame, Graphics};
            use magician_vgpu::VirtualGpu;

            let world = self.app.world.clone();
            let present_mode = self.present_mode();
            wasm_bindgen_futures::spawn_local(async move {
                let size = window.inner_size();
                let mut vgpu = VirtualGpu::new(window).await;
                vgpu.config_mut().present_mode = present_mode;
                world.insert_resource(Graphics(vgpu));
                world.insert_resource(Frame::default());
                // configure now, a redraw may be requested before the first resize
                crate::app::configure_surface(&world, size.width, size.height);
            });
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        // stops the update thread after its current pass
        self.app.running.store(false, Ordering::Release);
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        self.app.world.send_event(DeviceEvent(event));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: winit::event::WindowEvent,
    ) {
        self.app.world.send_event(WindowEvent(event.clone()));

        match &event {
            winit::event::WindowEvent::CloseRequested => event_loop.exit(),
            winit::event::WindowEvent::Resized(size) => {
                self.app.resize(size.width, size.height);
                self.request_redraw();
            }
            winit::event::WindowEvent::Occluded(false) => self.request_redraw(),
            winit::event::WindowEvent::RedrawRequested => {
                if let Some(deadline) = self.frame_too_early() {
                    event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
                    return;
                }
                match self.app.render() {
                    Ok(true) => {}
                    Ok(false) => self.retry_skipped_frame(event_loop),
                    Err(err) => {
                        log::error!("Failed to render frame: {err:#}");
                        self.retry_skipped_frame(event_loop);
                    }
                }
            }
            _ => {}
        }
    }
}
