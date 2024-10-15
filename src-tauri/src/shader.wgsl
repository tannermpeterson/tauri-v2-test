// Vertex shader

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
};

@vertex
fn vs_main(
    model: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    out.tex_coords = model.tex_coords;
    out.clip_position = vec4<f32>(model.position, 1.0);
    return out;
}

// Fragment shader

@group(0) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(0) @binding(1)
var s_diffuse: sampler;
@group(1) @binding(0)
var<uniform> min_max_threshold: vec2<u32>;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var tex_sample = textureSample(t_diffuse, s_diffuse, in.tex_coords);
    let lum = (0.2126*tex_sample.r + 0.7152*tex_sample.g + 0.0722*tex_sample.b) * 100;
    if (lum <= f32(min_max_threshold.x)) {
        tex_sample = vec4<f32>(0.0, 0.0, 0.0, tex_sample.a);
    } else if (lum >= f32(min_max_threshold.y)) {
        tex_sample = vec4<f32>(1.0, 1.0, 1.0, tex_sample.a);
    }
    return tex_sample;
}
