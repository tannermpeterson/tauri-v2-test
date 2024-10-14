use std::{sync::Mutex, time::Duration};

use tauri::{async_runtime::block_on, AppHandle, Manager, PhysicalSize, RunEvent, WindowEvent};
use tokio::time::sleep;
use wgpu::{include_wgsl, util::DeviceExt as _};

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
}

const VERTICES: &[Vertex] = &[
    Vertex {
        position: [-0.45, 0.9, 0.0],
        color: [0.5, 0.5, 0.5],
    },
    Vertex {
        position: [-0.45, 0.0, 0.0],
        color: [0.3, 0.3, 0.3],
    },
    Vertex {
        position: [0.45, 0.0, 0.0],
        color: [0.5, 0.5, 0.5],
    },
    Vertex {
        position: [0.45, 0.9, 0.0],
        color: [0.7, 0.7, 0.7],
    },
];

const INDICES: &[u16] = &[0, 1, 2, 0, 2, 3];

struct GpuState<'a> {
    surface: wgpu::Surface<'a>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
}

// TODO
// 1) don't use a new webview
//  - create components around the video player that do not have transparent backgrounds
//
// IDEAS:
//  - add text input boxes to set min/max threshold and change to blue/red if pixels ar
//    darker/brighter than the threshold. Might need to make the image greyscale to do this
//  - Set a default texture when video is not playing
//  - Vary the image over time, or find some other images to shuffle through
//  ? make some resizable component in the FE, send the size and position down to rust, have that
//    control where the video is rendered in the shader

fn next_triangle(app_handle: &AppHandle, new_size: Option<PhysicalSize<u32>>) {
    let gpu_state_mutex = app_handle.state::<Mutex<GpuState>>();

    {
        let mut gpu_state = gpu_state_mutex.lock().unwrap();
        if let Some(new_size) = new_size {
            gpu_state.config.width = if new_size.width > 0 {
                new_size.width
            } else {
                1
            };
            gpu_state.config.height = if new_size.height > 0 {
                new_size.height
            } else {
                1
            };
            gpu_state
                .surface
                .configure(&gpu_state.device, &gpu_state.config);
        }
        let frame = gpu_state
            .surface
            .get_current_texture()
            .expect("Failed to acquire next swap chain texture");
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu_state
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            rpass.set_pipeline(&gpu_state.render_pipeline);
            // let now = (SystemTime::now()
            //     .duration_since(SystemTime::UNIX_EPOCH)
            //     .expect("failed getting dur since")
            //     .as_secs()
            //     % 4) as u32;
            // let r = Range {
            //     start: now,
            //     end: now + 3,
            // };
            // rpass.draw(r, 0..1);

            rpass.set_vertex_buffer(0, gpu_state.vertex_buffer.slice(..));
            rpass.set_index_buffer(gpu_state.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            rpass.draw_indexed(0..INDICES.len() as u32, 0, 0..1);
        }

        gpu_state.queue.submit(Some(encoder.finish()));
        frame.present();
    }
}

#[tauri::command]
async fn hello_triangle(app_handle: AppHandle) {
    loop {
        next_triangle(&app_handle, None);
        sleep(Duration::from_secs(1)).await;
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();
            let window_size = window.inner_size()?;

            let instance = wgpu::Instance::default();

            let surface = instance.create_surface(window).unwrap();
            let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                force_fallback_adapter: false,
                // Request an adapter which can render to our surface
                compatible_surface: Some(&surface),
            }))
            .expect("Failed to find an appropriate adapter");

            // Create the logical device and command queue
            let (device, queue) = block_on(adapter.request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    memory_hints: wgpu::MemoryHints::Performance,
                    // Make sure we use the texture resolution limits from the adapter, so we can support images the size of the swapchain.
                    required_limits: wgpu::Limits::default().using_resolution(adapter.limits()),
                },
                None,
            ))
            .expect("Failed to create device");

            // Load the shaders from disk
            let shader = device.create_shader_module(include_wgsl!("shader.wgsl"));

            let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Vertex Buffer"),
                contents: bytemuck::cast_slice(VERTICES),
                usage: wgpu::BufferUsages::VERTEX,
            });

            let vertex_buffer_layout = wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        offset: 0,
                        shader_location: 0,
                        format: wgpu::VertexFormat::Float32x3,
                    },
                    wgpu::VertexAttribute {
                        offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                        shader_location: 1,
                        format: wgpu::VertexFormat::Float32x3,
                    },
                ],
            };

            let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Index Buffer"),
                contents: bytemuck::cast_slice(INDICES),
                usage: wgpu::BufferUsages::INDEX,
            });

            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[],
                push_constant_ranges: &[],
            });

            let swapchain_capabilities = surface.get_capabilities(&adapter);
            let swapchain_format = swapchain_capabilities.formats[0];

            let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: "vs_main",
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[vertex_buffer_layout],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: "fs_main",
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(swapchain_format.into())],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: swapchain_format,
                width: window_size.width,
                height: window_size.height,
                present_mode: wgpu::PresentMode::Fifo,
                alpha_mode: swapchain_capabilities.alpha_modes[0],
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            };

            surface.configure(&device, &config);

            let frame = surface
                .get_current_texture()
                .expect("Failed to acquire next swap chain texture");
            let view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            {
                let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                rpass.set_pipeline(&render_pipeline);
                rpass.set_vertex_buffer(0, vertex_buffer.slice(..));
                rpass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                rpass.draw_indexed(0..1, 0, 0..1);
            }

            queue.submit(Some(encoder.finish()));
            frame.present();

            let gpu_state = GpuState {
                surface,
                device,
                queue,
                config,
                render_pipeline,
                vertex_buffer,
                index_buffer,
            };

            app.manage(Mutex::new(gpu_state));

            Ok(())
        })
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![greet, hello_triangle])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| match event {
            RunEvent::WindowEvent {
                label: _,
                event: WindowEvent::Resized(size),
                ..
            } => {
                next_triangle(&app_handle, Some(size));
            }
            _ => (),
        });
}
