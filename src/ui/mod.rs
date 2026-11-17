use anarchy::{Event, Query, Res, ResMut, Resource, SystemPin, World, after, and, before, pin};
use crate::{
    App, AtlasTexturesPlugin, AtlasTextureVault, BindableAssetVault, BindlessArrayTextureVault,
    BindlessTexturesPlugin, Frame, Graphics, Plugin, WindowDimensions, WindowEvent,
};
use getset::Getters;
use magician_vgpu::{
    Buffer, ChunkedBuffer, LoadOp, MutableBuffer, PassAttachment, PassTarget, Pipeline,
    ShaderSource, ShaderType, StoreOp, TreeBuffer, glam::Vec2,
};
use mutual::CowData;
use winit::event::{ElementState, MouseButton};

use crate::ui::shader::{
    SDFRawBezier, SDFRawGlyph, SDFRawMetadata, SDFRawRectangle, SDFRawShaderData, SDFRawShape,
    SDFRawStyle, SDFRawTextureRect,
};

pub mod data;
pub mod ecs;
pub mod edit;
pub mod fonts;
pub mod nodes;
pub mod shader;
pub mod widgets;

pub use data::*;
pub use ecs::*;
pub use edit::*;
pub use fonts::*;
pub use nodes::*;
pub use widgets::*;

#[derive(Debug, Default, Resource)]
pub struct UIInputTracker {
    pub cursor_position: Vec2,
    pub is_cursor_down: bool,
}

/// ECS plugin that registers GPU resources and a render pass for 2D SDF UI.
pub struct UIPlugin;
impl Plugin for UIPlugin {
    fn build(self, app: App) -> App {
        app.add_plugin(BindlessTexturesPlugin)
            .add_plugin(AtlasTexturesPlugin)
            .add_resource(UIInputTracker::default())
            .add_resource(UiInput::default())
            .add_resource(UiFocus::default())
            .add_resource(EntityUiElements::default())
            .add_resource(UiClipboard::default())
            .on_render_startup_with(init_resources, pin(SystemPin::Start))
            .on_render_startup_with(init_clipboard, pin(SystemPin::Start))
            .on_render_update_with(read_ui_input, pin(SystemPin::Start))
            .on_render_update_with(pick_ui, and(pin(SystemPin::Start), after(read_ui_input)))
            .on_render_update_with(hover_text_spans, and(pin(SystemPin::Start), after(pick_ui)))
            .on_render_update_with(scroll_nodes, and(pin(SystemPin::Start), after(pick_ui)))
            // drawn over the main pass, which runs in the `Normal` pin
            .on_render_update_with(layout_entity_ui, and(pin(SystemPin::End), before(ui_render_pass)))
            .on_render_update_with(ui_render_pass, pin(SystemPin::End))
    }
}

/// GPU buffers, bind group, and pipeline used by the UI render pass.
///
/// `pipeline` is the SDF render pipeline, `bind_group` wires all uniform buffers
/// to that pipeline, `metadata_buffer` holds per-frame screen size/time/mode,
/// `shapes_buffer` stores the flattened element tree, and the remaining chunked
/// buffers hold deduplicated styles, rectangle radii, bezier curves, and glyph
/// headers referenced by shapes. `previous_frame` keeps the tree drawn last frame until the
/// next one is uploaded, so its chunk handles stay reserved: unchanged chunks are then found
/// again by content instead of being freed and uploaded again every frame.
#[derive(Resource, Getters)]
#[getset(get = "pub")]
pub struct UIRenderResources {
    pub pipeline: CowData<Pipeline>,
    pub bind_group: wgpu::BindGroup,
    pub metadata_buffer: MutableBuffer<SDFRawMetadata>,
    pub shapes_buffer: TreeBuffer<SDFRawShape>,
    pub styles_buffer: ChunkedBuffer<SDFRawStyle>,
    pub rectangles_buffer: ChunkedBuffer<SDFRawRectangle>,
    pub bezier_buffer: ChunkedBuffer<SDFRawBezier>,
    pub glyphs_buffer: ChunkedBuffer<SDFRawGlyph>,
    pub texture_rects_buffer: ChunkedBuffer<SDFRawTextureRect>,
    pub previous_frame: std::sync::Mutex<Option<SDFElement>>,
}

impl std::fmt::Debug for UIRenderResources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UIRenderResources").finish_non_exhaustive()
    }
}

/// Connects `UiClipboard` to the OS clipboard once there's a window.
fn init_clipboard(mut clipboard: ResMut<UiClipboard>, graphics: Res<Graphics>) {
    use winit::raw_window_handle::HasDisplayHandle;
    let display = graphics.window().and_then(|window| window.display_handle().ok()).map(|handle| handle.as_raw());
    *clipboard = UiClipboard::new(display);
}

/// Allocates UI GPU buffers, bind group, and pipeline on render startup.
fn init_resources(
    world: World,
    graphics: Res<Graphics>,
    bindless_vault: Res<BindlessArrayTextureVault>,
    atlas_vault: Res<AtlasTextureVault>,
) {
    // where the device can read storage buffers in fragment shaders (not WebGL2), the shapes
    // live in storage buffers and only the 16 bit pointers cap them. Elsewhere each is a
    // fixed-size `array<T, N>` inside a single uniform binding, so N is capped by the
    // device's max_uniform_buffer_binding_size (on WebGL2 commonly ~16KB, far below
    // native's 64KB+). MAX_BUFFER_ELEMENTS is the ceiling; shrink per-type if the device
    // can't fit that many.
    let limits = graphics.device().limits();
    let storage = limits.max_storage_buffers_per_shader_stage >= STORAGE_BINDINGS;
    let (usage, element_limit, cap) = if storage {
        (wgpu::BufferUsages::STORAGE, limits.max_storage_buffer_binding_size as u64, MAX_STORAGE_ELEMENTS)
    } else {
        (wgpu::BufferUsages::UNIFORM, limits.max_uniform_buffer_binding_size as u64, MAX_BUFFER_ELEMENTS)
    };
    let usage = usage | wgpu::BufferUsages::COPY_DST;
    let fit = |size: usize| (element_limit / size as u64).min(cap as u64) as u32;
    let max_shapes = fit(std::mem::size_of::<SDFRawShape>());
    let max_styles = fit(std::mem::size_of::<SDFRawStyle>());
    let max_rectangles = fit(std::mem::size_of::<SDFRawRectangle>());
    let max_bezier = fit(std::mem::size_of::<SDFRawBezier>());
    let max_glyphs = fit(std::mem::size_of::<SDFRawGlyph>());
    let max_texture_rects = fit(std::mem::size_of::<SDFRawTextureRect>());

    // create metadata buffer
    let metadata_buffer = MutableBuffer::new(
        &*graphics,
        &SDFRawMetadata {
            screen_dimensions: (1.0.into(), 1.0.into()),
            time: 1.0.into(),
            mode: SDFMode::Normal as u32,
        },
        wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    );

    // create shapes buffer
    let shapes_buffer = TreeBuffer::new(
        &*graphics,
        usage,
        max_shapes,
    );

    // create styles buffer
    let styles_buffer = ChunkedBuffer::new(
        &*graphics,
        usage,
        max_styles,
    );

    // create rectangles buffer
    let rectangles_buffer = ChunkedBuffer::new(
        &*graphics,
        usage,
        max_rectangles,
    );

    // create bezier's buffer
    let bezier_buffer = ChunkedBuffer::new(
        &*graphics,
        usage,
        max_bezier,
    );

    // create glyphs buffer
    let glyphs_buffer = ChunkedBuffer::new(
        &*graphics,
        usage,
        max_glyphs,
    );

    // create texture rects buffer (atlas-backend texture placements; unused but harmless
    // when the bindless backend is active)
    let texture_rects_buffer = ChunkedBuffer::new(
        &*graphics,
        usage,
        max_texture_rects,
    );

    // create bind group layout
    let shape_entry = |binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: if storage { wgpu::ShaderStages::FRAGMENT } else { wgpu::ShaderStages::VERTEX_FRAGMENT },
        ty: wgpu::BindingType::Buffer {
            ty: if storage { wgpu::BufferBindingType::Storage { read_only: true } } else { wgpu::BufferBindingType::Uniform },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    let bind_group_layout =
        graphics
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    shape_entry(1),
                    shape_entry(2),
                    shape_entry(3),
                    shape_entry(4),
                    shape_entry(5),
                    shape_entry(6),
                ],
                label: Some("SDF UI BGL"),
            });

    // create bind group
    let bind_group = graphics
        .device()
        .create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("SDF UI BG"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: metadata_buffer.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: shapes_buffer.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: styles_buffer.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: rectangles_buffer.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: bezier_buffer.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: glyphs_buffer.buffer().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: texture_rects_buffer.buffer().as_entire_binding(),
                },
            ],
        });

    // pick the texture backend: a true bindless array where supported, falling back to a
    // single shared atlas texture (see crate::TextureVault) elsewhere -- both the group 1
    // bind group layout and the fragment shader's texture-sampling code depend on which
    // the sample runs inside the shape tree walk's loop, so it takes an explicit mip level
    // (textures have only one): FXC on dx12 can't compile implicit-gradient samples there
    let bindless = *graphics.supports_bindless_arrays();
    let (texture_group_src, sample_body_src, group1_layout) = if bindless {
        (
            "@group(1) @binding(0) var ui_textures: binding_array<texture_2d<f32>>;\n@group(1) @binding(1) var ui_sampler: sampler;",
            "return textureSampleLevel(ui_textures[ptr], ui_sampler, local_uv, 0.0);",
            bindless_vault.bind_group_layout(&*graphics),
        )
    } else {
        (
            "@group(1) @binding(0) var ui_atlas_page: texture_2d<f32>;\n@group(1) @binding(1) var<uniform> ui_atlas_size: vec4<f32>;\n@group(1) @binding(2) var ui_sampler: sampler;",
            "let rect = texture_rect_at(ptr).rect;\n    let atlas_uv = (rect.xy + local_uv * rect.zw) / ui_atlas_size.xy;\n    return textureSampleLevel(ui_atlas_page, ui_sampler, atlas_uv, 0.0);",
            atlas_vault.bind_group_layout(&*graphics),
        )
    };

    // create pipeline
    let pipeline = Pipeline::builder("UI Pipeline")
        .source(
            ShaderType::Vertex,
            ShaderSource {
                source: include_str!("shaders/no_vertex_screen.wgsl").into(),
                main_function: "vs_final".into(),
            },
        )
        .source(
            ShaderType::Fragment,
            ShaderSource {
                // the shape buffers' declarations depend on the binding type, and uniform
                // arrays must be sized to the buffer capacities above (`max_*`)
                source: include_str!("shaders/main.wgsl")
                    .replacen(
                        "__SHAPE_BUFFERS__",
                        &shape_buffers_src(storage, [
                            max_shapes,
                            max_styles,
                            max_rectangles,
                            max_bezier,
                            max_glyphs,
                            max_texture_rects,
                        ]),
                        1,
                    )
                    .replacen("__TEXTURE_GROUP__", texture_group_src, 1)
                    .replacen("__SAMPLE_TEXTURE_BODY__", sample_body_src, 1),
                main_function: "fs_final".into(),
            },
        )
        .layout_raw::<SDFRawShaderData>(0, bind_group_layout)
        .layout_raw::<BindlessArrayTextureVault>(1, group1_layout)
        .build(&*graphics);

    world.insert_resource(UIRenderResources {
        pipeline: CowData::new(pipeline),
        bind_group,
        metadata_buffer,
        shapes_buffer,
        styles_buffer,
        rectangles_buffer,
        bezier_buffer,
        glyphs_buffer,
        texture_rects_buffer,
        previous_frame: std::sync::Mutex::new(None),
    });
}

/// Most elements of each kind the uniform UI buffers hold. Shaders address them with 16 bit
/// pointers, and a font's glyph curves alone run past 1000.
const MAX_BUFFER_ELEMENTS: u32 = 4096;

/// Most elements of each kind the storage UI buffers hold, one glyph being one shape. The 16
/// bit pointers allow up to `0xFFFE`.
const MAX_STORAGE_ELEMENTS: u32 = 16384;

/// Storage buffers the UI shader binds.
const STORAGE_BINDINGS: u32 = 6;

/// One shape buffer of the UI shader, in binding order (bindings 1 to 6).
///
/// Fields:
/// * name: The array's name in the shader.
/// * ty: The WGSL struct of one element.
/// * accessor: The shader function that reads element `i`.
/// * words: How many `vec4<u32>` one element takes (its `SDFRaw*` size / 16).
/// * unpack: WGSL that builds the struct from words `w0`, `w1`, ... of a packed element.
struct ShapeBuffer {
    name: &'static str,
    ty: &'static str,
    accessor: &'static str,
    words: u32,
    unpack: &'static str,
}

/// The UI shader's shape buffers, matching the `SDFRaw*` layouts in [`shader`].
const SHAPE_BUFFERS: [ShapeBuffer; 6] = [
    ShapeBuffer {
        name: "shapes",
        ty: "SDFShape",
        accessor: "shape_at",
        words: 2,
        unpack: "SDFShape(bitcast<vec2<f32>>(w0.xy), bitcast<vec2<f32>>(w0.zw), w1.x, w1.y, w1.z)",
    },
    ShapeBuffer {
        name: "styles",
        ty: "SDFStyle",
        accessor: "style_at",
        words: 3,
        unpack: "SDFStyle(bitcast<vec4<f32>>(w0), bitcast<vec4<f32>>(w1), bitcast<f32>(w2.x), w2.y)",
    },
    ShapeBuffer {
        name: "rectangles",
        ty: "SDFRectangle",
        accessor: "rectangle_at",
        words: 2,
        unpack: "SDFRectangle(bitcast<vec4<f32>>(w0), bitcast<vec4<f32>>(w1))",
    },
    ShapeBuffer {
        name: "bezier",
        ty: "SDFBezier",
        accessor: "bezier_at",
        words: 2,
        unpack: "SDFBezier(bitcast<vec2<f32>>(w0.xy), bitcast<vec2<f32>>(w0.zw), bitcast<vec2<f32>>(w1.xy), bitcast<f32>(w1.z))",
    },
    ShapeBuffer {
        name: "glyphs",
        ty: "SDFGlyph",
        accessor: "glyph_at",
        words: 1,
        unpack: "SDFGlyph(w0.x, w0.y, w0.z, w0.w)",
    },
    ShapeBuffer {
        name: "texture_rects",
        ty: "SDFTextureRect",
        accessor: "texture_rect_at",
        words: 1,
        unpack: "SDFTextureRect(bitcast<vec4<f32>>(w0))",
    },
];

/// Builds the WGSL declaring the shape buffers (group 0, bindings 1 to 6) and their `*_at(i)`
/// accessors, which is all main.wgsl uses to read them.
///
/// With storage buffers each is a runtime-sized struct array.  Otherwise (WebGL2) each is a
/// uniform array of `vec4<u32>` words, unpacked by its accessor: ANGLE's Direct3D 11 backend,
/// behind WebGL on Windows in every browser, takes minutes to link a shader that dynamically
/// indexes large uniform arrays of *structs*, while plain vector arrays link in milliseconds.
///
/// # Parameters
/// * storage: Whether the buffers are storage buffers.
/// * capacities: How many elements each buffer holds, in [`SHAPE_BUFFERS`] order.
fn shape_buffers_src(storage: bool, capacities: [u32; 6]) -> String {
    SHAPE_BUFFERS
        .iter()
        .zip(capacities)
        .enumerate()
        .map(|(index, (buffer, capacity))| {
            let ShapeBuffer { name, ty, accessor, words, unpack } = buffer;
            let binding = index + 1;
            if storage {
                return format!(
                    "@group(0) @binding({binding}) var<storage, read> {name}: array<{ty}>;\n\
                     fn {accessor}(i: u32) -> {ty} {{ return {name}[i]; }}\n"
                );
            }

            // read each word of the element, then rebuild the struct from them
            let loads: String = (0..*words)
                .map(|word| format!("    let w{word} = {name}[i * {words}u + {word}u];\n"))
                .collect();
            format!(
                "@group(0) @binding({binding}) var<uniform> {name}: array<vec4<u32>, {}>;\n\
                 fn {accessor}(i: u32) -> {ty} {{\n{loads}    return {unpack};\n}}\n",
                capacity.max(1) * words
            )
        })
        .collect()
}

/// Flattens the UI tree, uploads it, and draws a fullscreen SDF pass over the frame.
pub(crate) fn ui_render_pass(
    world: World,
    mut input_tracker: ResMut<UIInputTracker>,
    graphics: Res<Graphics>,
    mut frame: ResMut<Frame>,
    resources: Res<UIRenderResources>,
    window_dimensions: Res<WindowDimensions>,
    bindless_vault: Res<BindlessArrayTextureVault>,
    atlas_vault: Res<AtlasTextureVault>,
    window_events: Event<WindowEvent>,
    entity_ui: Res<EntityUiElements>,
    mut roots: Query<&'static UINodeSDFRoot>,
    mut raw_elements: Query<&'static UIRawElements>,
) {
    for event in window_events.read() {
        match &*event {
            winit::event::WindowEvent::CursorMoved { position, .. } => {
                input_tracker.cursor_position = Vec2::new(position.x as f32, position.y as f32);
            }
            winit::event::WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                input_tracker.is_cursor_down = *state == ElementState::Pressed;
            }
            _ => {}
        }
    }

    // clone the roots out so listeners called during layout can lock them
    let mut nodes = vec![];
    while let Ok(Some((_, root))) = roots.next() {
        nodes.push(root.0.clone());
    }
    // entity nodes go under node trees, which hold the immediate mode windows
    let mut node_elements = entity_ui.elements.clone();
    node_elements.extend(layout_ui_nodes(
        &world,
        &input_tracker,
        &nodes,
        [window_dimensions.x as f32, window_dimensions.y as f32],
    ));

    // create and sort ui elements
    let mut elements = vec![];
    while let Ok(Some((_, raw))) = raw_elements.next() {
        elements.push((raw.elements.clone(), raw.priority));
    }
    let elements = if elements.is_empty() {
        node_elements
    } else {
        elements.push((node_elements, 0));
        elements.sort_by_key(|a| a.1);
        elements
            .into_iter()
            .flat_map(|a| a.0.into_iter())
            .collect::<Vec<_>>()
    };

    // create root element
    let root = SDFElement {
        center: Vec2::new(
            window_dimensions.x as f32 / 2.0,
            window_dimensions.y as f32 / 2.0,
        ),
        dimensions: Vec2::new(window_dimensions.x as f32, window_dimensions.y as f32),
        children: elements,
        ..Default::default()
    };

    if let Err(err) = draw_ui(root, &graphics, &mut frame, &resources, &bindless_vault, &atlas_vault) {
        log::error!("Failed to draw the UI: {err:#}");
    }
}

/// Uploads `root` and draws it over the frame.
fn draw_ui(
    root: SDFElement,
    graphics: &Graphics,
    frame: &mut Frame,
    resources: &UIRenderResources,
    bindless_vault: &BindlessArrayTextureVault,
    atlas_vault: &AtlasTextureVault,
) -> crate::anyhow::Result<()> {
    // upload new UI tree, then keep it (and its chunks) until the next frame's is uploaded
    resources.shapes_buffer.update(graphics, &root, &(resources, bindless_vault, atlas_vault))?;
    if let Ok(mut previous) = resources.previous_frame.lock() {
        *previous = Some(root);
    }

    // setup render pass
    let mut pass = frame.init_pass(
        &[PassAttachment {
            target: PassTarget::PassOutput,
            load_op: LoadOp::Load,
            store_op: StoreOp::Store,
        }],
        None,
    );

    // draw to the screen
    pass.use_pipeline(resources.pipeline().get_ref());
    pass.bind_raw(0, resources.bind_group());
    if *graphics.supports_bindless_arrays() {
        bindless_vault.bind(graphics, &mut pass, 1)?;
    } else {
        atlas_vault.bind(graphics, &mut pass, 1)?;
    }
    pass.pass_mut().draw(0..3, 0..1);
    Ok(())
}
