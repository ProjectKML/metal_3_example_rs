mod free_cam;
mod mesh;
mod texture;

use std::{mem, path::PathBuf};

use cocoa::{appkit::NSView, base::id as cocoa_id};
use core_graphics_types::geometry::CGSize;
use dolly::glam::Mat4;
use metal::*;
use objc::{rc::autoreleasepool, runtime::YES};
use winit::{
    dpi::LogicalSize,
    event::{DeviceEvent, Event, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    platform::macos::WindowExtMacOS,
    window::WindowBuilder,
};
use winit::window::Window;

use crate::free_cam::FreeCam;
use crate::mesh::MeshBuffers;
use crate::texture::ModelTexture;

#[repr(C)]
struct UniformData {
    view_projection_matrix: Mat4,
    render_type: u32
}

fn prepare_render_pass_descriptor(descriptor: &RenderPassDescriptorRef, texture: &TextureRef) {
    let color_attachment = descriptor.color_attachments().object_at(0).unwrap();

    color_attachment.set_texture(Some(texture));
    color_attachment.set_load_action(MTLLoadAction::Clear);
    color_attachment.set_clear_color(MTLClearColor::new(0.0, 0.0, 0.0, 1.0));
    color_attachment.set_store_action(MTLStoreAction::Store);
}

fn main() {
    let events_loop = EventLoop::new();
    let size = LogicalSize::new(1600, 900);

    let window = WindowBuilder::new()
        .with_inner_size(size)
        .with_title("Metal 3 Example 🦀".to_string())
        .build(&events_loop)
        .unwrap();

    window.set_cursor_visible(false);
    //TODO: We haves to lock the cursor in the middle

    let device = Device::system_default().expect("no device found");

    let layer = MetalLayer::new();
    layer.set_device(&device);
    layer.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
    layer.set_presents_with_transaction(false);

    unsafe {
        let view = window.ns_view() as cocoa_id;
        view.setWantsLayer(YES);
        view.setLayer(mem::transmute(layer.as_ref()));
    }

    layer.set_drawable_size(CGSize::new(1600 as _, 900 as _));

    let library_path = PathBuf::from("shaders.metallib");
    let library = device.new_library_with_file(library_path).unwrap();

    let mesh = library.get_function("mesh_function", None).unwrap();
    let frag = library.get_function("fragment_function", None).unwrap();

    let mut pipeline_state_desc = MeshRenderPipelineDescriptor::new();
    pipeline_state_desc
        .color_attachments()
        .object_at(0)
        .unwrap()
        .set_pixel_format(MTLPixelFormat::BGRA8Unorm);
    pipeline_state_desc.set_depth_attachment_pixel_format(MTLPixelFormat::Depth32Float);

    pipeline_state_desc.set_mesh_function(Some(&mesh));
    pipeline_state_desc.set_fragment_function(Some(&frag));

    let pipeline_state = device
        .new_mesh_render_pipeline_state(&pipeline_state_desc)
        .unwrap();

    let command_queue = device.new_command_queue();

    //Create depth texture
    let mut depth_texture_descriptor = TextureDescriptor::new();
    depth_texture_descriptor.set_texture_type(MTLTextureType::D2);
    depth_texture_descriptor.set_pixel_format(MTLPixelFormat::Depth32Float);
    depth_texture_descriptor.set_width(1600);
    depth_texture_descriptor.set_height(900);
    depth_texture_descriptor.set_depth(1);
    depth_texture_descriptor.set_mipmap_level_count(1);
    depth_texture_descriptor.set_sample_count(1);
    depth_texture_descriptor.set_array_length(1);
    depth_texture_descriptor.set_resource_options(MTLResourceOptions::StorageModePrivate);
    depth_texture_descriptor.set_usage(MTLTextureUsage::RenderTarget);

    let depth_texture = device.new_texture(&depth_texture_descriptor);

    let mut depth_stencil_descriptor = DepthStencilDescriptor::new();
    depth_stencil_descriptor.set_depth_compare_function(MTLCompareFunction::LessEqual);
    depth_stencil_descriptor.set_depth_write_enabled(true);

    let depth_stencil_state = device.new_depth_stencil_state(&depth_stencil_descriptor);

    let mut camera = FreeCam::new();

    let mut uniform_data = UniformData {
        view_projection_matrix: camera.vp_matrix(&window),
        render_type: 0,
    };

    let mesh_buffers = unsafe { MeshBuffers::new(&device, "angel.obj") }
        .unwrap();

    let texture = ModelTexture::new(&device, "angel.png");

    events_loop.run(move |event, _, control_flow| {
        autoreleasepool(|| {
            *control_flow = ControlFlow::Poll;

            uniform_data.view_projection_matrix = camera.vp_matrix(&window);

            match event {
                Event::WindowEvent { event, .. } => {
                    match event {
                        WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                        WindowEvent::Resized(size) => {
                            layer.set_drawable_size(CGSize::new(1600 as _, 900 as _));
                            //TODO: size.width is wrong
                            /*
                            layer.set_drawable_size(CGSize::new(
                                size.width as f64,
                                size.height as f64,
                            ));*/
                        }
                        WindowEvent::KeyboardInput { input, .. } => {
                            if let Some(key_code) = input.virtual_keycode {
                                if key_code == VirtualKeyCode::Escape {
                                    *control_flow = ControlFlow::ExitWithCode(0);
                                } else if key_code == VirtualKeyCode::Key1 {
                                    uniform_data.render_type = 0;
                                } else if key_code == VirtualKeyCode::Key2 {
                                    uniform_data.render_type = 1;
                                }

                                camera.key_event(input.state, key_code);
                            }
                        }
                        _ => (),
                    }
                }
                Event::MainEventsCleared => {
                    window.request_redraw();
                }
                Event::RedrawRequested(_) => {
                    let delta_time = 1. / 120.; //TODO:

                    camera.update(delta_time);

                    //Metal commands

                    let drawable = match layer.next_drawable() {
                        Some(drawable) => drawable,
                        None => return,
                    };

                    let render_pass_descriptor = RenderPassDescriptor::new();

                    prepare_render_pass_descriptor(&render_pass_descriptor, drawable.texture());

                    let render_pass_depth_attachment_descriptor = render_pass_descriptor.depth_attachment().unwrap();
                    render_pass_depth_attachment_descriptor.set_clear_depth(1.);
                    render_pass_depth_attachment_descriptor.set_load_action(MTLLoadAction::Clear);
                    render_pass_depth_attachment_descriptor.set_store_action(MTLStoreAction::DontCare);
                    render_pass_depth_attachment_descriptor.set_texture(Some(&depth_texture));

                    let command_buffer = command_queue.new_command_buffer();
                    let encoder =
                        command_buffer.new_render_command_encoder(&render_pass_descriptor);

                    encoder.set_mesh_bytes(
                        0,
                        mem::size_of::<UniformData>() as _,
                        &uniform_data as *const _ as *const _,
                    );

                    encoder.set_fragment_bytes(
                        0,
                        mem::size_of::<UniformData>() as _,
                        &uniform_data as *const _ as *const _
                    );

                    encoder.set_render_pipeline_state(&pipeline_state);
                    encoder.set_depth_stencil_state(&depth_stencil_state);

                    encoder.set_mesh_buffer(1, Some(&mesh_buffers.vertex_buffer), 0);
                    encoder.set_mesh_buffer(2, Some(&mesh_buffers.meshlet_buffer), 0);
                    encoder.set_mesh_buffer(3, Some(&mesh_buffers.meshlet_data_buffer), 0);

                    encoder.set_fragment_texture(0, Some(&texture.texture));

                    encoder.draw_mesh_threadgroups(
                        MTLSize::new(((mesh_buffers.num_meshlets * 32 + 31) / 32) as NSUInteger, 1, 1),
                        MTLSize::new(1, 1, 1),
                        MTLSize::new(32, 1, 1),
                    );

                    encoder.end_encoding();

                    command_buffer.present_drawable(&drawable);
                    command_buffer.commit();
                }
                Event::DeviceEvent { event, .. } => {
                    if let DeviceEvent::MouseMotion { delta } = event {
                        camera.mouse_movement(delta);
                    }
                }
                _ => {}
            }
        });
    });
}
