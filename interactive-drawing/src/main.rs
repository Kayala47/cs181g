// Based on the Vulkano triangle example.

// Triangle example Copyright (c) 2016 The vulkano developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or https://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

use std::cmp::min;
use std::sync::Arc;
use vulkano::buffer::{BufferUsage, CpuAccessibleBuffer, TypedBufferAccess};
use vulkano::command_buffer::{AutoCommandBufferBuilder, CommandBufferUsage, SubpassContents};
use vulkano::descriptor_set::PersistentDescriptorSet;
use vulkano::device::physical::{PhysicalDevice, PhysicalDeviceType};
use vulkano::device::{Device, DeviceExtensions, Features};
use vulkano::format::Format;
use vulkano::image::ImageCreateFlags;
use vulkano::image::{
    view::ImageView, ImageAccess, ImageDimensions, ImageUsage, StorageImage, SwapchainImage,
};
use vulkano::instance::Instance;
use vulkano::pipeline::graphics::input_assembly::InputAssemblyState;
use vulkano::pipeline::graphics::vertex_input::BuffersDefinition;
use vulkano::pipeline::graphics::viewport::{Viewport, ViewportState};
use vulkano::pipeline::{GraphicsPipeline, Pipeline, PipelineBindPoint};
use vulkano::render_pass::{Framebuffer, RenderPass, Subpass};
use vulkano::sampler::{Filter, MipmapMode, Sampler, SamplerAddressMode};
use vulkano::swapchain::{self, AcquireError, Swapchain, SwapchainCreationError};
use vulkano::sync::{self, FlushError, GpuFuture};
use vulkano::Version;
use vulkano_win::VkSurfaceBuild;
use winit::event::{Event, VirtualKeyCode, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Window, WindowBuilder};

// We'll make our Color type an RGBA8888 pixel.
type Color = (u8, u8, u8, u8);

const WIDTH: usize = 320;
const HEIGHT: usize = 240;

pub struct Rect {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}

impl Rect {
    pub fn new(x: usize, y: usize, w: usize, h: usize) -> Rect {
        Rect { x, y, w, h }
    }
}

// Here's what clear looks like, though we won't use it
#[allow(dead_code)]
fn clear(fb: &mut [Color], c: Color) {
    fb.fill(c);
}

#[allow(dead_code)]
fn line(fb: &mut [Color], x0: usize, x1: usize, y: usize, c: Color) {
    assert!(y < HEIGHT);
    assert!(x0 <= x1);
    assert!(x1 < WIDTH);
    fb[y * WIDTH + x0..(y * WIDTH + x1)].fill(c);
}

#[allow(dead_code)]
fn rectangle(fb: &mut [Color], r: Rect, c: Color) {
    assert!(r.w + r.x <= WIDTH);
    assert!(r.h + r.y <= HEIGHT);

    for i in (r.y)..(r.y + r.h) {
        line(fb, r.x, r.x + r.w, i, c);
    }
}

#[allow(dead_code)]
fn outlined_rect(fb: &mut [Color], r: Rect, c: Color) {
    assert!(r.w + r.x <= WIDTH);
    assert!(r.h + r.y <= HEIGHT);

    for i in (r.y)..(r.y + r.h) {
        line(fb, r.x, r.x + r.w, i, c);
    }

    let new_c = (255, 255, 255, 0);
    //cover back up the parts we weren't supposed to color
    for i in (r.y + 1)..(r.y + r.h - 1) {
        line(fb, r.x, r.x + r.w - 1, i, new_c);
    }
}

fn main() {
    let mut now_keys = [false; 255];
    let mut prev_keys = now_keys.clone();

    let c = (255, 0, 0, 0);
    let required_extensions = vulkano_win::required_extensions();
    let instance = Instance::new(None, Version::V1_1, &required_extensions, None).unwrap();
    let event_loop = EventLoop::new();
    let surface = WindowBuilder::new()
        .build_vk_surface(&event_loop, instance.clone())
        .unwrap();
    let device_extensions = DeviceExtensions {
        khr_swapchain: true,
        ..DeviceExtensions::none()
    };
    let (physical_device, queue_family) = PhysicalDevice::enumerate(&instance)
        .filter(|&p| p.supported_extensions().is_superset_of(&device_extensions))
        .filter_map(|p| {
            p.queue_families()
                .find(|&q| q.supports_graphics() && surface.is_supported(q).unwrap_or(false))
                .map(|q| (p, q))
        })
        .min_by_key(|(p, _)| match p.properties().device_type {
            PhysicalDeviceType::DiscreteGpu => 0,
            PhysicalDeviceType::IntegratedGpu => 1,
            PhysicalDeviceType::VirtualGpu => 2,
            PhysicalDeviceType::Cpu => 3,
            PhysicalDeviceType::Other => 4,
        })
        .unwrap();
    let (device, mut queues) = Device::new(
        physical_device,
        &Features::none(),
        &physical_device
            .required_extensions()
            .union(&device_extensions),
        [(queue_family, 0.5)].iter().cloned(),
    )
    .unwrap();
    let queue = queues.next().unwrap();
    let (mut swapchain, images) = {
        let caps = surface.capabilities(physical_device).unwrap();
        let composite_alpha = caps.supported_composite_alpha.iter().next().unwrap();
        let format = caps.supported_formats[0].0;
        let dimensions: [u32; 2] = surface.window().inner_size().into();
        Swapchain::start(device.clone(), surface.clone())
            .num_images(caps.min_image_count)
            .format(format)
            .dimensions(dimensions)
            .usage(ImageUsage::color_attachment())
            .sharing_mode(&queue)
            .composite_alpha(composite_alpha)
            .build()
            .unwrap()
    };

    // We now create a buffer that will store the shape of our triangle.
    #[derive(Default, Debug, Clone)]
    struct Vertex {
        position: [f32; 2],
        uv: [f32; 2],
    }
    vulkano::impl_vertex!(Vertex, position, uv);

    let vertex_buffer = CpuAccessibleBuffer::from_iter(
        device.clone(),
        BufferUsage::all(),
        false,
        [
            Vertex {
                position: [-1.0, -1.0],
                uv: [0.0, 0.0],
            },
            Vertex {
                position: [3.0, -1.0],
                uv: [2.0, 0.0],
            },
            Vertex {
                position: [-1.0, 3.0],
                uv: [0.0, 2.0],
            },
        ]
        .iter()
        .cloned(),
    )
    .unwrap();
    mod vs {
        vulkano_shaders::shader! {
            ty: "vertex",
            src: "
                #version 450

                layout(location = 0) in vec2 position;
                layout(location = 1) in vec2 uv;
                layout(location = 0) out vec2 out_uv;
                void main() {
                    gl_Position = vec4(position, 0.0, 1.0);
                    out_uv = uv;
                }
            "
        }
    }

    mod fs {
        vulkano_shaders::shader! {
            ty: "fragment",
            src: "
                #version 450

                layout(set = 0, binding = 0) uniform sampler2D tex;
                layout(location = 0) in vec2 uv;
                layout(location = 0) out vec4 f_color;

                void main() {
                    f_color = texture(tex, uv);
                }
            "
        }
    }

    let vs = vs::load(device.clone()).unwrap();
    let fs = fs::load(device.clone()).unwrap();

    // Here's our (2D drawing) framebuffer.
    let mut fb2d = [(128, 64, 64, 255); WIDTH * HEIGHT];
    // We'll work on it locally, and copy it to a GPU buffer every frame.
    // Then on the GPU, we'll copy it into an Image.
    let fb2d_buffer = CpuAccessibleBuffer::from_iter(
        device.clone(),
        BufferUsage::transfer_source(),
        false,
        (0..WIDTH * HEIGHT).map(|_| (255_u8, 0_u8, 0_u8, 0_u8)),
    )
    .unwrap();
    // Let's set up the Image we'll copy into:
    let dimensions = ImageDimensions::Dim2d {
        width: WIDTH as u32,
        height: HEIGHT as u32,
        array_layers: 1,
    };
    //image n GPU
    let fb2d_image = StorageImage::with_usage(
        device.clone(),
        dimensions,
        Format::R8G8B8A8_UNORM,
        ImageUsage {
            // This part is key!
            transfer_destination: true,
            sampled: true,
            storage: true,
            transfer_source: false,
            color_attachment: false,
            depth_stencil_attachment: false,
            transient_attachment: false,
            input_attachment: false,
        },
        ImageCreateFlags::default(),
        std::iter::once(queue_family),
    )
    .unwrap();
    // Get a view on it to use as a texture:
    let fb2d_texture = ImageView::new(fb2d_image.clone()).unwrap();

    let fb2d_sampler = Sampler::new(
        device.clone(),
        Filter::Linear,
        Filter::Linear,
        MipmapMode::Nearest,
        SamplerAddressMode::Repeat,
        SamplerAddressMode::Repeat,
        SamplerAddressMode::Repeat,
        0.0,
        1.0,
        0.0,
        0.0,
    )
    .unwrap();

    let render_pass = vulkano::single_pass_renderpass!(
        device.clone(),
        attachments: {
            color: {
                // Pro move: We're going to cover the screen completely. Trust us!
                load: DontCare,
                store: Store,
                format: swapchain.format(),
                samples: 1,
            }
        },
        pass: {
            color: [color],
            depth_stencil: {}
        }
    )
    .unwrap();
    let pipeline = GraphicsPipeline::start()
        .vertex_input_state(BuffersDefinition::new().vertex::<Vertex>())
        .vertex_shader(vs.entry_point("main").unwrap(), ())
        .input_assembly_state(InputAssemblyState::new())
        .viewport_state(ViewportState::viewport_dynamic_scissor_irrelevant())
        .fragment_shader(fs.entry_point("main").unwrap(), ())
        .render_pass(Subpass::from(render_pass.clone(), 0).unwrap())
        .build(device.clone())
        .unwrap();

    let layout = pipeline.layout().descriptor_set_layouts().get(0).unwrap();
    let mut set_builder = PersistentDescriptorSet::start(layout.clone());

    set_builder
        .add_sampled_image(fb2d_texture, fb2d_sampler)
        .unwrap();

    let set = set_builder.build().unwrap();

    let mut viewport = Viewport {
        origin: [0.0, 0.0],
        dimensions: [0.0, 0.0],
        depth_range: 0.0..1.0,
    };

    let mut framebuffers = window_size_dependent_setup(&images, render_pass.clone(), &mut viewport);
    let mut recreate_swapchain = false;
    let mut previous_frame_end = Some(sync::now(device.clone()).boxed());

    let mut x: f32 = 0.0;
    let mut y: f32 = 0.0;

    let mut ax: f32 = 0.0;
    let mut ay: f32 = 0.0;
    let mut vx: f32 = 0.0;
    let mut vy: f32 = 0.0;

    let mut hor_dir: f32 = 1.0;
    let mut ver_dir: f32 = 1.0;

    let mut accel: f32 = 0.5;

    let mut decay: f32 = 0.3;

    let mut y2: usize = 2;
    let mut x2: usize = 2;
    event_loop.run(move |event, _, control_flow| {
        match event {
            // NewEvents: Let's start processing events.
            Event::NewEvents(_) => {
                // Leave now_keys alone, but copy over all changed keys
                prev_keys.copy_from_slice(&now_keys);
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(_),
                ..
            } => {
                recreate_swapchain = true;
            }
            // WindowEvent->KeyboardInput: Keyboard input!
            Event::WindowEvent {
                // Note this deeply nested pattern match
                //TODO: this is where we can add mouse input
                event:
                    WindowEvent::KeyboardInput {
                        input:
                            winit::event::KeyboardInput {
                                // Which serves to filter out only events we actually want
                                virtual_keycode: Some(keycode),
                                state,
                                ..
                            },
                        ..
                    },
                ..
            } => {
                // It also binds these handy variable names!
                match state {
                    winit::event::ElementState::Pressed => {
                        // VirtualKeycode is an enum with a defined representation
                        now_keys[keycode as usize] = true;
                    }
                    winit::event::ElementState::Released => {
                        now_keys[keycode as usize] = false;
                    }
                }
            }
            // MainEventsCleared: Buttons and stuff have all been handled.  Update stuff!
            Event::MainEventsCleared => {
                {
                    // We need to synchronize here to send new data to the GPU.
                    // We can't send the new framebuffer until the previous frame is done being drawn.
                    // Dropping the future will block until it's done.
                    if let Some(mut fut) = previous_frame_end.take() {
                        fut.cleanup_finished();
                    }
                }

                // We can actually handle events now that we know what they all are.
                if now_keys[VirtualKeyCode::Escape as usize] {
                    *control_flow = ControlFlow::Exit;
                }

                if now_keys[VirtualKeyCode::Down as usize] {
                    if !now_keys[VirtualKeyCode::LControl as usize] {
                        ay += accel;
                    }
                    // if now_keys[VirtualKeyCode::LShift as usize] {
                    //     y2 += accel;
                    // }
                }
                if now_keys[VirtualKeyCode::Right as usize] && x < (WIDTH - 1) as f32 {
                    if !now_keys[VirtualKeyCode::LControl as usize] {
                        ax += accel;
                    }
                    // if now_keys[VirtualKeyCode::LShift as usize] {
                    //     x2 += accel;
                    // }
                }

                if !now_keys[VirtualKeyCode::Right as usize]
                    && !now_keys[VirtualKeyCode::Down as usize]
                {
                    ax = 0.0;
                    ay = 0.0;
                }

                if now_keys[VirtualKeyCode::LShift as usize] {
                    //move to muddy ground
                    decay = 0.5;
                    accel = 0.2;
                } else {
                    decay = 0.3;
                    accel = 0.5;
                }

                // Exercise for the reader: Tie y to mouse movement

                // It's debatable whether the following code should live here or in the drawing section.
                // First clear the framebuffer...
                clear(&mut fb2d, (128, 64, 64, 255));
                // Then draw our line:
                vx += ax;
                vy += ay;
                x += vx;
                y += vy;
                if now_keys[VirtualKeyCode::Left as usize] && x > 0.0 {
                    // if x > 0.0 && !now_keys[VirtualKeyCode::LControl as usize] {
                    //     ax -= accel;
                    // }
                    x = 0.0;
                    vx = 0.0;
                    ax = 0.0;
                    // if x2 > 0 && now_keys[VirtualKeyCode::LShift as usize] {
                    //     x2 -= accel;
                    // }
                }

                if x > (WIDTH * 2 / 3) as f32 {
                    x = (WIDTH * 2 / 3) as f32
                }
                y += vy;
                if y > (HEIGHT * 2 / 3) as f32 {
                    y = (HEIGHT * 2 / 3) as f32
                }

                if now_keys[VirtualKeyCode::Up as usize] {
                    // if y > 0.0 && !now_keys[VirtualKeyCode::LControl as usize] {
                    //     ay -= accel;
                    // }
                    y = 0.0;
                    vy = 0.0;
                    ay = 0.0;
                    // if y2 > 0 && now_keys[VirtualKeyCode::LShift as usize] {
                    //     y2 -= accel;
                    // }
                }

                let rect = Rect::new(x as usize, y as usize, WIDTH / 3, HEIGHT / 3);

                rectangle(&mut fb2d, rect, c);

                if vx - decay > 0.0 {
                    vx -= decay
                } else {
                    vx = 0.0;
                }
                if vy - decay > 0.0 {
                    vy -= decay
                } else {
                    vy = 0.0;
                }

                // vx = min(vx - 1.0, 0.0);
                // vy = min(vy - 1.0, 0.0);

                // let rect2 = Rect::new(x2 + 10, y2, WIDTH / 2, HEIGHT / 2);
                // outlined_rect(&mut fb2d, rect2, c);

                // Now we can copy into our buffer.
                {
                    let writable_fb = &mut *fb2d_buffer.write().unwrap();
                    writable_fb.copy_from_slice(&fb2d); //copy frame buffer into GPU
                }

                if recreate_swapchain {
                    let dimensions: [u32; 2] = surface.window().inner_size().into();
                    let (new_swapchain, new_images) =
                        match swapchain.recreate().dimensions(dimensions).build() {
                            Ok(r) => r,
                            Err(SwapchainCreationError::UnsupportedDimensions) => return,
                            Err(e) => panic!("Failed to recreate swapchain: {:?}", e),
                        };

                    swapchain = new_swapchain;
                    framebuffers = window_size_dependent_setup(
                        &new_images,
                        render_pass.clone(),
                        &mut viewport,
                    );
                    recreate_swapchain = false;
                }
                let (image_num, suboptimal, acquire_future) =
                    match swapchain::acquire_next_image(swapchain.clone(), None) {
                        Ok(r) => r,
                        Err(AcquireError::OutOfDate) => {
                            recreate_swapchain = true;
                            return;
                        }
                        Err(e) => panic!("Failed to acquire next image: {:?}", e),
                    };
                if suboptimal {
                    recreate_swapchain = true;
                }

                let mut builder = AutoCommandBufferBuilder::primary(
                    device.clone(),
                    queue.family(),
                    CommandBufferUsage::OneTimeSubmit,
                )
                .unwrap();

                builder
                    // Now copy that framebuffer buffer into the framebuffer image
                    .copy_buffer_to_image(fb2d_buffer.clone(), fb2d_image.clone())
                    .unwrap()
                    // And resume our regularly scheduled programming
                    .begin_render_pass(
                        framebuffers[image_num].clone(),
                        SubpassContents::Inline,
                        std::iter::once(vulkano::format::ClearValue::None),
                    )
                    .unwrap()
                    .set_viewport(0, [viewport.clone()])
                    .bind_pipeline_graphics(pipeline.clone())
                    .bind_descriptor_sets(
                        PipelineBindPoint::Graphics,
                        pipeline.layout().clone(),
                        0,
                        set.clone(),
                    )
                    .bind_vertex_buffers(0, vertex_buffer.clone())
                    .draw(vertex_buffer.len() as u32, 1, 0, 0)
                    .unwrap()
                    .end_render_pass()
                    .unwrap();

                let command_buffer = builder.build().unwrap();

                let future = acquire_future
                    .then_execute(queue.clone(), command_buffer)
                    .unwrap()
                    .then_swapchain_present(queue.clone(), swapchain.clone(), image_num)
                    .then_signal_fence_and_flush();

                match future {
                    Ok(future) => {
                        previous_frame_end = Some(future.boxed());
                    }
                    Err(FlushError::OutOfDate) => {
                        recreate_swapchain = true;
                        previous_frame_end = Some(sync::now(device.clone()).boxed());
                    }
                    Err(e) => {
                        println!("Failed to flush future: {:?}", e);
                        previous_frame_end = Some(sync::now(device.clone()).boxed());
                    }
                }
            }
            _ => (), //important: matches all other events
        }
    });
}

fn window_size_dependent_setup(
    images: &[Arc<SwapchainImage<Window>>],
    render_pass: Arc<RenderPass>,
    viewport: &mut Viewport,
) -> Vec<Arc<Framebuffer>> {
    let dimensions = images[0].dimensions().width_height();
    viewport.dimensions = [dimensions[0] as f32, dimensions[1] as f32];

    images
        .iter()
        .map(|image| {
            let view = ImageView::new(image.clone()).unwrap();
            Framebuffer::start(render_pass.clone())
                .add(view)
                .unwrap()
                .build()
                .unwrap()
        })
        .collect::<Vec<_>>()
}
