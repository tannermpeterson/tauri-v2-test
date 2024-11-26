use std::{fs, sync::Mutex};

use image::GenericImageView;
use tauri::{async_runtime::block_on, AppHandle, Manager, PhysicalSize, RunEvent, WindowEvent};
use tokio::time::{sleep_until, Duration, Instant};
use wgpu::{include_wgsl, util::DeviceExt as _, BufferBindingType};

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    tex_coords: [f32; 2],
}

const VERTICES: &[Vertex] = &[
    Vertex {
        position: [-0.45, 0.9, 0.0],
        tex_coords: [0.0, 0.0],
    },
    Vertex {
        position: [-0.45, 0.0, 0.0],
        tex_coords: [0.0, 1.0],
    },
    Vertex {
        position: [0.45, 0.0, 0.0],
        tex_coords: [1.0, 1.0],
    },
    Vertex {
        position: [0.45, 0.9, 0.0],
        tex_coords: [1.0, 0.0],
    },
];

const INDICES: &[u16] = &[0, 1, 2, 0, 2, 3];

const NUM_FRAMES: u32 = 11;
// current limit seems to be ~10ms
const FRAME_RATE: Duration = Duration::from_millis(100);

struct GpuState<'a> {
    surface: wgpu::Surface<'a>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    diffuse_bind_group: wgpu::BindGroup,
    diffuse_texture: wgpu::Texture,
    texture_size: wgpu::Extent3d,
    frame_idx: Option<u32>,
    start_time: Option<Instant>,
    min_threshold: u32,
    max_threshold: u32,
    threshold_buffer: wgpu::Buffer,
    threshold_bind_group: wgpu::BindGroup,
}

// TODO
// 1) don't use a new webview (Is this already done?)
//  - create components around the video player that do not have transparent backgrounds
//
// IDEAS:
//  - Set up tauri command to allow stopping the video
//  - instead of looping inside the command, create another task that just sends the next frame idx
//    periodically according to the frame rate
//  ? make some resizable component in the FE, send the size and position down to rust, have that
//    control where the video is rendered in the shader

fn next_triangle(app_handle: &AppHandle, new_size: Option<PhysicalSize<u32>>) -> bool {
    let gpu_state_mutex = app_handle.state::<Mutex<GpuState>>();

    // TODO try removing these inner brackets
    {
        let mut gpu_state = gpu_state_mutex.lock().unwrap();

        // check and see if reconfig is needed
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

        // update frame idx if necessary
        let next_frame_idx = if let Some(start_time) = gpu_state.start_time {
            let whole_periods_elapsed =
                Instant::now().duration_since(start_time).as_millis() / FRAME_RATE.as_millis();
            let next_frame_idx = (whole_periods_elapsed % (NUM_FRAMES as u128)) as u32;
            // TODO remove debug
            if let Some(frame_idx) = gpu_state.frame_idx {
                if next_frame_idx != frame_idx && next_frame_idx > (frame_idx + 1) % NUM_FRAMES {
                    println!("\n\n------------- FRAME(S) DROPPED -------------\n\n")
                }
            }
            Some(next_frame_idx)
        } else {
            None
        };
        // if on a new frame idx, update the image
        if next_frame_idx != gpu_state.frame_idx {
            gpu_state.frame_idx = next_frame_idx;
            let img_name = if let Some(frame_idx) = next_frame_idx {
                format!("happy-tree-{}", frame_idx + 1)
            } else {
                "default".to_string()
            };
            let diffuse_bytes =
                fs::read(format!("./video-imgs/{}.png", img_name)).expect("should read");
            let diffuse_image = image::load_from_memory(&diffuse_bytes).unwrap();
            let diffuse_rgba = diffuse_image.to_rgba8();
            gpu_state.queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &gpu_state.diffuse_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &diffuse_rgba,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * gpu_state.texture_size.width),
                    rows_per_image: Some(gpu_state.texture_size.height),
                },
                gpu_state.texture_size,
            );
        }

        // handle thresholding
        gpu_state.queue.write_buffer(
            &gpu_state.threshold_buffer,
            0,
            bytemuck::cast_slice(&[gpu_state.min_threshold, gpu_state.max_threshold]),
        );

        // render
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
            rpass.set_bind_group(0, &gpu_state.diffuse_bind_group, &[]);
            rpass.set_bind_group(1, &gpu_state.threshold_bind_group, &[]);
            rpass.set_vertex_buffer(0, gpu_state.vertex_buffer.slice(..));
            rpass.set_index_buffer(gpu_state.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            rpass.draw_indexed(0..INDICES.len() as u32, 0, 0..1);
        }

        gpu_state.queue.submit(Some(encoder.finish()));
        frame.present();

        next_frame_idx.is_some()
    }
}

#[tauri::command]
async fn start_live_view(app_handle: AppHandle) {
    let gpu_state_mutex = app_handle.state::<Mutex<GpuState>>();
    let now: Instant;
    {
        let mut gpu_state = gpu_state_mutex.lock().unwrap();
        now = Instant::now();
        gpu_state.start_time = Some(now);
    }

    let mut deadline = now + FRAME_RATE;
    loop {
        if !next_triangle(&app_handle, None) {
            return;
        }
        sleep_until(deadline).await;
        deadline += FRAME_RATE;
    }
}

#[tauri::command]
async fn stop_live_view(app_handle: AppHandle) {
    let gpu_state_mutex = app_handle.state::<Mutex<GpuState>>();
    let mut gpu_state = gpu_state_mutex.lock().unwrap();
    gpu_state.start_time = None;
    // TODO make consts for these default values
    gpu_state.min_threshold = 0;
    gpu_state.max_threshold = 100;
}

#[tauri::command]
async fn set_min_threshold(app_handle: AppHandle, new_min_threshold: u32) {
    let gpu_state_mutex = app_handle.state::<Mutex<GpuState>>();
    let mut gpu_state = gpu_state_mutex.lock().unwrap();
    gpu_state.min_threshold = new_min_threshold;
}

#[tauri::command]
async fn set_max_threshold(app_handle: AppHandle, new_max_threshold: u32) {
    let gpu_state_mutex = app_handle.state::<Mutex<GpuState>>();
    let mut gpu_state = gpu_state_mutex.lock().unwrap();
    gpu_state.max_threshold = new_max_threshold;
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

            // vertex buffer
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
                        format: wgpu::VertexFormat::Float32x2,
                    },
                ],
            };

            // index buffer
            let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Index Buffer"),
                contents: bytemuck::cast_slice(INDICES),
                usage: wgpu::BufferUsages::INDEX,
            });

            // texture
            let texture_bind_group_layout =
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                multisampled: false,
                                view_dimension: wgpu::TextureViewDimension::D2,
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            // This should match the filterable field of the
                            // corresponding Texture entry above.
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                    label: Some("texture_bind_group_layout"),
                });

            let diffuse_bytes = fs::read("./video-imgs/default.png").expect("should read");
            let diffuse_image = image::load_from_memory(&diffuse_bytes).unwrap();
            let dimensions = diffuse_image.dimensions();

            let texture_size = wgpu::Extent3d {
                width: dimensions.0,
                height: dimensions.1,
                depth_or_array_layers: 1,
            };
            let diffuse_texture = device.create_texture(&wgpu::TextureDescriptor {
                size: texture_size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                label: Some("diffuse_texture"),
                view_formats: &[],
            });

            let diffuse_texture_view =
                diffuse_texture.create_view(&wgpu::TextureViewDescriptor::default());
            let diffuse_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Nearest,
                mipmap_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            });

            let diffuse_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&diffuse_texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&diffuse_sampler),
                    },
                ],
                label: Some("diffuse_bind_group"),
            });

            let diffuse_rgba = diffuse_image.to_rgba8();
            queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &diffuse_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &diffuse_rgba,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * dimensions.0),
                    rows_per_image: Some(dimensions.1),
                },
                texture_size,
            );

            // thresholds
            let min_threshold: u32 = 0;
            let max_threshold: u32 = 100;

            let threshold_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Threshold Buffer"),
                contents: bytemuck::cast_slice(&[min_threshold, max_threshold]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

            let threshold_bind_group_layout =
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                    label: Some("threshold_bind_group_layout"),
                });

            let threshold_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &threshold_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: threshold_buffer.as_entire_binding(),
                }],
                label: Some("camera_bind_group"),
            });

            // etc.
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&texture_bind_group_layout, &threshold_bind_group_layout],
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

            // TODO could probably just call fn here instead of duping code

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
                rpass.set_bind_group(0, &diffuse_bind_group, &[]);
                rpass.set_bind_group(1, &threshold_bind_group, &[]);
                rpass.set_vertex_buffer(0, vertex_buffer.slice(..));
                rpass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                rpass.draw_indexed(0..INDICES.len() as u32, 0, 0..1);
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
                diffuse_bind_group,
                diffuse_texture,
                texture_size,
                frame_idx: None,
                start_time: None,
                min_threshold,
                max_threshold,
                threshold_buffer,
                threshold_bind_group,
            };

            app.manage(Mutex::new(gpu_state));

            Ok(())
        })
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            start_live_view,
            stop_live_view,
            set_min_threshold,
            set_max_threshold,
        ])
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
