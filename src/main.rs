mod free_cam;
mod mesh;
mod shader_compiler;
mod texture;

use std::{mem, ptr::NonNull};

use dolly::glam::{Mat4, Vec3};
use glam::{EulerRot, Quat};
use objc2::{
    ffi::NSUInteger,
    rc::{autoreleasepool, Retained},
    runtime::{MessageReceiver, ProtocolObject},
};
use objc2_core_foundation::CGSize;
use objc2_metal::{
    MTLClearColor, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue, MTLCompareFunction,
    MTLCreateSystemDefaultDevice, MTLDepthStencilDescriptor, MTLDevice, MTLDrawable, MTLLibrary,
    MTLLoadAction, MTLMeshRenderPipelineDescriptor, MTLPipelineOption, MTLPixelFormat,
    MTLRenderCommandEncoder, MTLRenderPassDescriptor, MTLRenderStages, MTLResourceOptions,
    MTLResourceUsage, MTLSamplerDescriptor, MTLSize, MTLStoreAction, MTLTexture,
    MTLTextureDescriptor, MTLTextureType, MTLTextureUsage,
};
use objc2_quartz_core::{CAMetalDrawable, CAMetalLayer};
use sdl3::{
    event::{Event, WindowEvent},
    keyboard::Keycode,
    sys::{
        metal::{SDL_Metal_CreateView, SDL_Metal_DestroyView, SDL_Metal_GetLayer},
        mouse::{SDL_HideCursor, SDL_SetWindowRelativeMouseMode, SDL_ShowCursor},
        video::SDL_SetWindowMouseGrab,
    },
};

use crate::{
    free_cam::FreeCam,
    mesh::MeshBuffers,
    shader_compiler::{compile, DescriptorTableEntry, ShaderKind},
    texture::ModelTexture,
};

#[derive(Copy, Clone)]
#[repr(C)]
struct UniformData {
    view_projection_matrix: Mat4,
    render_type: u32,
}

fn prepare_render_pass_descriptor(
    descriptor: &MTLRenderPassDescriptor,
    texture: &ProtocolObject<dyn MTLTexture>,
) {
    unsafe {
        let color_attachment = descriptor.colorAttachments().objectAtIndexedSubscript(0);
        color_attachment.setTexture(Some(texture));
        color_attachment.setLoadAction(MTLLoadAction::Clear);
        color_attachment.setClearColor(MTLClearColor {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
            alpha: 1.0,
        });
        color_attachment.setStoreAction(MTLStoreAction::Store);
    }
}

fn main() {
    autoreleasepool(|_| {
        unsafe {
            let sdl = sdl3::init().unwrap();
            let video_subsystem = sdl.video().unwrap();

            let window = video_subsystem
                .window("Metal Example", 2560, 1440)
                .position_centered()
                .resizable()
                .build()
                .unwrap();

            SDL_HideCursor();
            SDL_SetWindowMouseGrab(window.raw(), true);
            SDL_SetWindowRelativeMouseMode(window.raw(), true);

            let mut event_pump = sdl.event_pump().unwrap();

            let mut running = true;

            std::env::set_var("MTL_DEBUG_LAYER", "1");
            std::env::set_var("MTL_LOG_LEVEL", "4");

            let device = MTLCreateSystemDefaultDevice().unwrap();

            let view = SDL_Metal_CreateView(window.raw());
            let layer = SDL_Metal_GetLayer(view);

            let layer: Retained<CAMetalLayer> = {
                let ptr = layer as *mut CAMetalLayer;
                Retained::retain(ptr).expect("Failed to get metal layer")
            };

            layer.setDevice(Some(&device));
            layer.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
            layer.setPresentsWithTransaction(false);
            layer.setDrawableSize(CGSize::new(window.size().0 as _, window.size().1 as _));

            let (_, mesh) = compile(
                &device,
                "shaders/geometry.hlsl",
                "geometry_mesh",
                ShaderKind::Mesh,
            );
            let (_, frag) = compile(
                &device,
                "shaders/geometry.hlsl",
                "geometry_pixel",
                ShaderKind::Fragment,
            );

            let pipeline_state_desc = MTLMeshRenderPipelineDescriptor::new();
            pipeline_state_desc
                .colorAttachments()
                .objectAtIndexedSubscript(0)
                .setPixelFormat(MTLPixelFormat::BGRA8Unorm);
            pipeline_state_desc.setDepthAttachmentPixelFormat(MTLPixelFormat::Depth32Float);
            pipeline_state_desc.setMeshFunction(Some(&mesh));
            pipeline_state_desc.setFragmentFunction(Some(&frag));

            let pipeline_state = device
                .newRenderPipelineStateWithMeshDescriptor_options_reflection_error(
                    &pipeline_state_desc,
                    MTLPipelineOption::empty(),
                    None,
                )
                .unwrap();

            let command_queue = device.newCommandQueue().unwrap();

            //Create depth texture
            let depth_texture_descriptor = MTLTextureDescriptor::new();
            depth_texture_descriptor.setTextureType(MTLTextureType::Type2D);
            depth_texture_descriptor.setPixelFormat(MTLPixelFormat::Depth32Float);
            depth_texture_descriptor.setWidth(window.size().0 as _);
            depth_texture_descriptor.setHeight(window.size().1 as _);
            depth_texture_descriptor.setDepth(1);
            depth_texture_descriptor.setMipmapLevelCount(1);
            depth_texture_descriptor.setSampleCount(1);
            depth_texture_descriptor.setArrayLength(1);
            depth_texture_descriptor.setResourceOptions(MTLResourceOptions::StorageModePrivate);
            depth_texture_descriptor.setUsage(MTLTextureUsage::RenderTarget);

            let depth_texture = device
                .newTextureWithDescriptor(&depth_texture_descriptor)
                .unwrap();

            let depth_stencil_descriptor = MTLDepthStencilDescriptor::new();
            depth_stencil_descriptor.setDepthCompareFunction(MTLCompareFunction::LessEqual);
            depth_stencil_descriptor.setDepthWriteEnabled(true);

            let depth_stencil_state = device
                .newDepthStencilStateWithDescriptor(&depth_stencil_descriptor)
                .unwrap();

            let mut camera = FreeCam::new();

            let mut uniform_data = UniformData {
                view_projection_matrix: camera
                    .vp_matrix(window.size().0 as f32 / window.size().1 as f32),
                render_type: 0,
            };

            let mesh_buffers = unsafe { MeshBuffers::new(&device, "shepherd.obj") }.unwrap();
            //TODO: we dont want to hardcode this in the future
            let mesh_buffers2 = unsafe { MeshBuffers::new(&device, "angel.obj") }.unwrap();

            let texture = ModelTexture::new(&device, "shepherd.png");
            let texture2 = ModelTexture::new(&device, "angel.png");

            while running {
                for event in event_pump.poll_iter() {
                    match event {
                        Event::Quit { .. } => {
                            running = false;
                        }
                        Event::Window { win_event, .. } => {
                            match win_event {
                                WindowEvent::Resized(width, height) => {
                                    layer.setDrawableSize(CGSize::new(width as _, height as _));
                                }
                                _ => {}
                            }
                        }
                        Event::KeyDown { keycode, .. } => {
                            if let Some(keycode) = keycode {
                                if keycode == Keycode::Escape {
                                    running = false;
                                } else if keycode == Keycode::_1 {
                                    uniform_data.render_type = 0;
                                } else if keycode == Keycode::_2 {
                                    uniform_data.render_type = 1;
                                }

                                camera.key_event(true, keycode);
                            }
                        }
                        Event::KeyUp { keycode, .. } => {
                            if let Some(keycode) = keycode {
                                camera.key_event(false, keycode);
                            }
                        }
                        Event::MouseMotion { xrel, yrel, .. } => {
                            camera.mouse_movement((xrel, yrel));
                        }
                        _ => {}
                    }
                }

                //Loop

                let delta_time = 1. / 60.; //TODO:

                uniform_data.view_projection_matrix =
                    camera.vp_matrix(window.size().0 as f32 / window.size().1 as f32);

                let mut uniform_data2 = uniform_data;
                uniform_data2.view_projection_matrix *= Mat4::from_rotation_translation(
                    Quat::from_euler(EulerRot::XYZ, 0., 90.0f32.to_radians(), 0.),
                    Vec3::new(0., 0., 0.5),
                );

                uniform_data.view_projection_matrix *= Mat4::from_scale(Vec3::new(2., 2., 2.))
                    * Mat4::from_translation(Vec3::new(-0.1, -0.2, -0.1));

                camera.update(delta_time);

                let drawable = match layer.nextDrawable() {
                    Some(drawable) => drawable,
                    None => continue,
                };

                let render_pass_descriptor = MTLRenderPassDescriptor::new();

                prepare_render_pass_descriptor(&render_pass_descriptor, &drawable.texture());

                let render_pass_depth_attachment_descriptor =
                    render_pass_descriptor.depthAttachment();
                render_pass_depth_attachment_descriptor.setClearDepth(1.);
                render_pass_depth_attachment_descriptor.setLoadAction(MTLLoadAction::Clear);
                render_pass_depth_attachment_descriptor.setStoreAction(MTLStoreAction::DontCare);
                render_pass_depth_attachment_descriptor.setTexture(Some(&depth_texture));

                let command_buffer = command_queue.commandBuffer().unwrap();

                let encoder = command_buffer
                    .renderCommandEncoderWithDescriptor(&render_pass_descriptor)
                    .unwrap();

                let uniform_data_buffer = device
                    .newBufferWithBytes_length_options(
                        NonNull::new(&mut uniform_data as *mut _ as *mut _).unwrap(),
                        mem::size_of::<UniformData>() as _,
                        MTLResourceOptions::StorageModeShared,
                    )
                    .unwrap();

                let uniform_data_buffer2 = device
                    .newBufferWithBytes_length_options(
                        NonNull::new(&mut uniform_data2 as *mut _ as *mut _).unwrap(),
                        mem::size_of::<UniformData>() as _,
                        MTLResourceOptions::StorageModeShared,
                    )
                    .unwrap();

                let sampler_desc = MTLSamplerDescriptor::new();

                let sampler = device.newSamplerStateWithDescriptor(&sampler_desc).unwrap();

                let mut mesh_arguments = [
                    DescriptorTableEntry::buffer(&mesh_buffers.vertex_buffer, 0),
                    DescriptorTableEntry::buffer(&mesh_buffers.meshlet_buffer, 0),
                    DescriptorTableEntry::buffer(&mesh_buffers.meshlet_data_buffer, 0),
                    DescriptorTableEntry::buffer(&uniform_data_buffer, 0),
                ];

                let mut frag_arguments = [
                    DescriptorTableEntry::texture(&texture.texture, 0., 0),
                    DescriptorTableEntry::buffer(&uniform_data_buffer, 0),
                    DescriptorTableEntry::sampler(&sampler, 0.),
                ];

                encoder.setMeshBytes_length_atIndex(
                    NonNull::new(mesh_arguments.as_mut_ptr().cast()).unwrap(),
                    mem::size_of::<DescriptorTableEntry>() * 4,
                    2,
                );
                encoder.setFragmentBytes_length_atIndex(
                    NonNull::new(frag_arguments.as_mut_ptr().cast()).unwrap(),
                    mem::size_of::<DescriptorTableEntry>() * 3,
                    2,
                );

                encoder.useResource_usage_stages(
                    mesh_buffers.vertex_buffer.as_ref(),
                    MTLResourceUsage::Read,
                    MTLRenderStages::Mesh,
                );
                encoder.useResource_usage_stages(
                    mesh_buffers.meshlet_buffer.as_ref(),
                    MTLResourceUsage::Read,
                    MTLRenderStages::Mesh,
                );
                encoder.useResource_usage_stages(
                    mesh_buffers.meshlet_data_buffer.as_ref(),
                    MTLResourceUsage::Read,
                    MTLRenderStages::Mesh,
                );
                encoder.useResource_usage_stages(
                    uniform_data_buffer.as_ref(),
                    MTLResourceUsage::Read,
                    MTLRenderStages::Mesh | MTLRenderStages::Fragment,
                );
                encoder.useResource_usage_stages(
                    texture.texture.as_ref(),
                    MTLResourceUsage::Read,
                    MTLRenderStages::Fragment,
                );

                encoder.setRenderPipelineState(&pipeline_state);
                encoder.setDepthStencilState(Some(&depth_stencil_state));

                encoder.drawMeshThreadgroups_threadsPerObjectThreadgroup_threadsPerMeshThreadgroup(
                    MTLSize {
                        width: ((mesh_buffers.num_meshlets * 32 + 31) / 32) as NSUInteger,
                        height: 1,
                        depth: 1,
                    },
                    MTLSize {
                        width: 1,
                        height: 1,
                        depth: 1,
                    },
                    MTLSize {
                        width: 32,
                        height: 1,
                        depth: 1,
                    },
                );

                //Render second hardcoded mesh xDD

                let mut mesh_arguments = [
                    DescriptorTableEntry::buffer(&mesh_buffers2.vertex_buffer, 0),
                    DescriptorTableEntry::buffer(&mesh_buffers2.meshlet_buffer, 0),
                    DescriptorTableEntry::buffer(&mesh_buffers2.meshlet_data_buffer, 0),
                    DescriptorTableEntry::buffer(&uniform_data_buffer2, 0),
                ];

                let mut frag_arguments = [
                    DescriptorTableEntry::texture(&texture2.texture, 0., 0),
                    DescriptorTableEntry::buffer(&uniform_data_buffer2, 0),
                    DescriptorTableEntry::sampler(&sampler, 0.),
                ];

                encoder.setMeshBytes_length_atIndex(
                    NonNull::new(mesh_arguments.as_mut_ptr().cast()).unwrap(),
                    mem::size_of::<DescriptorTableEntry>() * 4,
                    2,
                );
                encoder.setFragmentBytes_length_atIndex(
                    NonNull::new(frag_arguments.as_mut_ptr().cast()).unwrap(),
                    mem::size_of::<DescriptorTableEntry>() * 3,
                    2,
                );

                encoder.drawMeshThreadgroups_threadsPerObjectThreadgroup_threadsPerMeshThreadgroup(
                    MTLSize {
                        width: ((mesh_buffers2.num_meshlets * 32 + 31) / 32) as NSUInteger,
                        height: 1,
                        depth: 1,
                    },
                    MTLSize {
                        width: 1,
                        height: 1,
                        depth: 1,
                    },
                    MTLSize {
                        width: 32,
                        height: 1,
                        depth: 1,
                    },
                );

                encoder.useResource_usage_stages(
                    mesh_buffers2.vertex_buffer.as_ref(),
                    MTLResourceUsage::Read,
                    MTLRenderStages::Mesh,
                );
                encoder.useResource_usage_stages(
                    mesh_buffers2.meshlet_buffer.as_ref(),
                    MTLResourceUsage::Read,
                    MTLRenderStages::Mesh,
                );
                encoder.useResource_usage_stages(
                    mesh_buffers2.meshlet_data_buffer.as_ref(),
                    MTLResourceUsage::Read,
                    MTLRenderStages::Mesh,
                );
                encoder.useResource_usage_stages(
                    uniform_data_buffer2.as_ref(),
                    MTLResourceUsage::Read,
                    MTLRenderStages::Mesh | MTLRenderStages::Fragment,
                );
                encoder.useResource_usage_stages(
                    texture2.texture.as_ref(),
                    MTLResourceUsage::Read,
                    MTLRenderStages::Fragment,
                );

                encoder.endEncoding();

                let drawable: Retained<ProtocolObject<dyn MTLDrawable>> =
                    Retained::cast_unchecked(drawable);

                command_buffer.presentDrawable(&drawable);
                command_buffer.commit();
                command_buffer.waitUntilCompleted();
            }

            SDL_Metal_DestroyView(view);
        }
    });
}
