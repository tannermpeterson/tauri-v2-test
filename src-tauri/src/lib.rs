use std::{
    borrow::Cow,
    ops::Range,
    sync::Mutex,
    time::{Duration, SystemTime},
};

use tauri::{async_runtime::block_on, AppHandle, Manager, PhysicalSize, RunEvent, WindowEvent};
use tokio::time::sleep;

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

// TODO
// 1) don't use a new webview
//  - create components around the video player that are do not have transparent backgrounds
//  - figure out how to only draw to the section where the video player should be
//  - make sure things can be rendered on top of it
//  - make sure resizing works

fn next_triangle(app_handle: &AppHandle, new_size: Option<PhysicalSize<u32>>) {
    let surface = app_handle.state::<Mutex<wgpu::Surface>>();
    let render_pipeline = app_handle.state::<wgpu::RenderPipeline>();
    let device = app_handle.state::<wgpu::Device>();
    let queue = app_handle.state::<wgpu::Queue>();

    {
        let surface_ = surface.lock().unwrap();
        if let Some(new_size) = new_size {
            let config = app_handle.state::<Mutex<wgpu::SurfaceConfiguration>>();
            let mut config = config.lock().unwrap();
            config.width = if new_size.width > 0 {
                new_size.width
            } else {
                1
            };
            config.height = if new_size.height > 0 {
                new_size.height
            } else {
                1
            };
            surface_.configure(&device, &config);
        }
        let frame = surface_
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
            let now = (SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("failed getting dur since")
                .as_secs()
                % 4) as u32;
            let r = Range {
                start: now,
                end: now + 3,
            };
            rpass.draw(r, 0..1);
        }

        queue.submit(Some(encoder.finish()));
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
            let (device, queue) = block_on(
                adapter.request_device(
                    &wgpu::DeviceDescriptor {
                        label: None,
                        required_features: wgpu::Features::empty(),
                        memory_hints: wgpu::MemoryHints::Performance,
                        // Make sure we use the texture resolution limits from the adapter, so we can support images the size of the swapchain.
                        required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                            .using_resolution(adapter.limits()),
                    },
                    None,
                ),
            )
            .expect("Failed to create device");

            // Load the shaders from disk
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: None,
                source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(
                    r#"
@vertex
fn vs_main(@builtin(vertex_index) in_vertex_index: u32) -> @builtin(position) vec4<f32> {
    let x = f32(i32(in_vertex_index % 3) - 1) / 1.8;
    let y = f32(i32(in_vertex_index & 1u) * 2 - 1) / 1.8 + (1.0 / 3.0);
    return vec4<f32>(x, y, 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(0.2, 0.0, 1.0, 1.0);
}
"#,
                )),
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
                    buffers: &[],
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
                rpass.draw(0..1, 0..1);
            }

            queue.submit(Some(encoder.finish()));
            frame.present();

            app.manage(Mutex::new(surface));
            app.manage(render_pipeline);
            app.manage(device);
            app.manage(queue);
            app.manage(Mutex::new(config));

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
