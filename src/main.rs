use cocoa::{appkit::NSView, base::id as cocoa_id};
use core_graphics_types::geometry::CGSize;
use std::collections::HashSet;
use std::ffi::c_void;

use dolly::glam::{Mat4, Quat, Vec3};
use dolly::prelude::{CameraRig, LeftHanded, Position, Smooth, YawPitch};
use metal::*;
use objc::{rc::autoreleasepool, runtime::YES};
use std::mem;
use std::path::PathBuf;

use winit::dpi::LogicalSize;
use winit::event::{DeviceEvent, ElementState, VirtualKeyCode};
use winit::event_loop::EventLoop;
use winit::{
    event::{Event, WindowEvent},
    event_loop::ControlFlow,
};

use winit::platform::macos::WindowExtMacOS;
use winit::window::{CursorGrabMode, WindowBuilder};

fn prepare_render_pass_descriptor(descriptor: &RenderPassDescriptorRef, texture: &TextureRef) {
    let color_attachment = descriptor.color_attachments().object_at(0).unwrap();

    color_attachment.set_texture(Some(texture));
    color_attachment.set_load_action(MTLLoadAction::Clear);
    color_attachment.set_clear_color(MTLClearColor::new(0.2, 0.2, 0.25, 1.0));
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
    //TODO: We have to lock the cursor in the middle

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

    let draw_size = window.inner_size();
    layer.set_drawable_size(CGSize::new(draw_size.width as f64, draw_size.height as f64));

    let library_path = PathBuf::from("shaders.metallib");
    let library = device.new_library_with_file(library_path).unwrap();

    let mesh = library.get_function("mesh_function", None).unwrap();
    let frag = library.get_function("fragment_function", None).unwrap();

    let pipeline_state_desc = MeshRenderPipelineDescriptor::new();
    pipeline_state_desc
        .color_attachments()
        .object_at(0)
        .unwrap()
        .set_pixel_format(MTLPixelFormat::BGRA8Unorm);
    pipeline_state_desc.set_mesh_function(Some(&mesh));
    pipeline_state_desc.set_fragment_function(Some(&frag));

    let pipeline_state = device
        .new_mesh_render_pipeline_state(&pipeline_state_desc)
        .unwrap();

    let command_queue = device.new_command_queue();

    let mut camera_rig = CameraRig::<LeftHanded>::builder()
        .with(Position::new(Vec3::Y))
        .with(YawPitch::new())
        .with(Smooth::new_position_rotation(1.0, 1.0))
        .build();

    events_loop.run(move |event, _, control_flow| {
        let mut pressed_keys = HashSet::new();

        autoreleasepool(|| {
            *control_flow = ControlFlow::Poll;

            match event {
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                    WindowEvent::Resized(size) => {
                        layer.set_drawable_size(CGSize::new(size.width as f64, size.height as f64));
                    }
                    WindowEvent::KeyboardInput { input, .. } => {
                        if let Some(key_code) = input.virtual_keycode {
                            if key_code == VirtualKeyCode::Escape {
                                *control_flow = ControlFlow::ExitWithCode(0);
                            }

                            match input.state {
                                ElementState::Pressed => {
                                    if !pressed_keys.contains(&key_code) {
                                        pressed_keys.insert(key_code);
                                    }
                                }
                                ElementState::Released => {
                                    if pressed_keys.contains(&key_code) {
                                        pressed_keys.remove(&key_code);
                                    }
                                }
                            }
                        }
                    }
                    _ => (),
                },
                Event::MainEventsCleared => {
                    window.request_redraw();
                }
                Event::RedrawRequested(_) => {
                    let delta_time = 1. / 120.; //TODO:

                    //Camera update
                    let mut delta_pos = Vec3::ZERO;
                    if pressed_keys.contains(&VirtualKeyCode::W) {
                        delta_pos += Vec3::new(0.0, 0.0, 1.0);
                    }
                    if pressed_keys.contains(&VirtualKeyCode::A) {
                        delta_pos += Vec3::new(-1.0, 0.0, 0.0);
                    }
                    if pressed_keys.contains(&VirtualKeyCode::S) {
                        delta_pos += Vec3::new(0.0, 0.0, -1.0);
                    }
                    if pressed_keys.contains(&VirtualKeyCode::D) {
                        delta_pos += Vec3::new(1.0, 0.0, 0.0);
                    }
                    delta_pos = camera_rig.final_transform.rotation * delta_pos * 2.0;

                    if pressed_keys.contains(&VirtualKeyCode::Space) {
                        delta_pos += Vec3::new(0.0, -1.0, 0.0);
                    }
                    if pressed_keys.contains(&VirtualKeyCode::LShift) {
                        delta_pos += Vec3::new(0.0, 1.0, 0.0);
                    }

                    camera_rig
                        .driver_mut::<Position>()
                        .translate(-delta_pos * delta_time * 10.0);
                    camera_rig.update(delta_time);

                    let final_transform = camera_rig.final_transform;
                    let fov = 90.0f32;

                    let mut projection_matrix = Mat4::perspective_lh(
                        fov.to_radians(),
                        window.inner_size().width as f32 / window.inner_size().height as f32,
                        0.1,
                        1000.0,
                    );
                    projection_matrix.y_axis.y *= -1.0;

                    let view_projection_matrix = projection_matrix
                        * Mat4::look_at_lh(
                            final_transform.position,
                            final_transform.position + final_transform.forward(),
                            final_transform.up(),
                        )
                        * Mat4::from_rotation_translation(Quat::IDENTITY, Vec3::new(0.0, 0.0, 1.0));

                    //Metal commands

                    let drawable = match layer.next_drawable() {
                        Some(drawable) => drawable,
                        None => return,
                    };

                    let render_pass_descriptor = RenderPassDescriptor::new();

                    prepare_render_pass_descriptor(&render_pass_descriptor, drawable.texture());

                    let command_buffer = command_queue.new_command_buffer();
                    let encoder =
                        command_buffer.new_render_command_encoder(&render_pass_descriptor);

                    encoder.set_mesh_bytes(
                        0,
                        mem::size_of::<Mat4>() as _,
                        &view_projection_matrix as *const _ as *const _,
                    );

                    encoder.set_render_pipeline_state(&pipeline_state);
                    encoder.draw_mesh_threads(
                        MTLSize::new(1, 1, 1),
                        MTLSize::new(1, 1, 1),
                        MTLSize::new(1, 1, 1),
                    );

                    encoder.end_encoding();

                    command_buffer.present_drawable(&drawable);
                    command_buffer.commit();
                }
                Event::DeviceEvent { event, .. } => {
                    if let DeviceEvent::MouseMotion { delta } = event {
                        camera_rig
                            .driver_mut::<YawPitch>()
                            .rotate_yaw_pitch(0.3 * delta.0 as f32, -0.3 * delta.1 as f32);
                    }
                }
                _ => {}
            }
        });
    });
}
