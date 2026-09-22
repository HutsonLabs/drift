//! Metal Shading Language source, compiled at runtime by [`Gpu`](crate::Gpu).
//!
//! The NV12 conversion constants are spliced in from [`crate::color`], so the shader and
//! its CPU reference ([`crate::color::nv12_to_rgb`]) cannot drift apart.

use crate::color::{DECODE_MATRIX, FLOOR_BIAS};

/// Compute kernel: NV12 (BT.709 full range, g-r-d's matrix) → BGRA8, one thread per pixel
/// of a single region rectangle, writing only inside that rectangle.
pub const NV12_KERNEL: &str = "nv12_to_bgra";
/// Compute kernel: fill a rectangle with one colour.
pub const FILL_KERNEL: &str = "fill_rect";
/// Vertex shader of the present pass (one full-viewport triangle).
pub const PRESENT_VERTEX: &str = "present_vs";
/// Fragment shader of the present pass (texel fetch at 1:1, bilinear otherwise).
pub const PRESENT_FRAGMENT: &str = "present_fs";

/// `constant` block of the region kernels: origin and extent of the rectangle.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct RegionParams {
    pub origin: [u32; 2],
    pub extent: [u32; 2],
}

/// `constant` block of the fill kernel.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct FillParams {
    /// RGBA, 0..1 (Metal swizzles to the BGRA texture itself).
    pub color: [f32; 4],
    pub region: RegionParams,
}

/// `constant` block of the present fragment shader.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct PresentParams {
    pub origin: [f32; 2],
    pub size: [f32; 2],
    /// 0 = texel fetch (bit-exact 1:1), 1 = bilinear.
    pub mode: u32,
    pub _pad: u32,
}

/// Full MSL source.
pub fn source() -> String {
    let row = |r: [f32; 3]| format!("float3({:.9}, {:.9}, {:.9})", r[0], r[1], r[2]);
    format!(
        r#"
#include <metal_stdlib>
using namespace metal;

struct RegionParams {{ uint2 origin; uint2 extent; }};
struct FillParams {{ float4 color; RegionParams region; }};
struct PresentParams {{ float2 origin; float2 size; uint mode; uint pad; }};

constant float3 M0 = {m0};
constant float3 M1 = {m1};
constant float3 M2 = {m2};
constant float BIAS = {bias:.3};

kernel void {nv12}(texture2d<float, access::read> y_plane [[texture(0)]],
                   texture2d<float, access::read> uv_plane [[texture(1)]],
                   texture2d<float, access::write> dst [[texture(2)]],
                   constant RegionParams &p [[buffer(0)]],
                   uint2 gid [[thread_position_in_grid]]) {{
    if (gid.x >= p.extent.x || gid.y >= p.extent.y) return;
    uint2 pos = p.origin + gid;
    float3 yuv = float3(y_plane.read(pos).r, uv_plane.read(pos / 2).rg) * 255.0
               + float3(BIAS, BIAS - 128.0, BIAS - 128.0);
    float3 rgb = float3(dot(M0, yuv), dot(M1, yuv), dot(M2, yuv)) / 255.0;
    dst.write(float4(saturate(rgb), 1.0), pos);
}}

kernel void {fill}(texture2d<float, access::write> dst [[texture(0)]],
                   constant FillParams &p [[buffer(0)]],
                   uint2 gid [[thread_position_in_grid]]) {{
    if (gid.x >= p.region.extent.x || gid.y >= p.region.extent.y) return;
    dst.write(p.color, p.region.origin + gid);
}}

struct VOut {{ float4 pos [[position]]; }};

vertex VOut {pvs}(uint vid [[vertex_id]]) {{
    float2 uv = float2((vid << 1) & 2, vid & 2);
    VOut o;
    o.pos = float4(uv * 2.0 - 1.0, 0.0, 1.0);
    return o;
}}

fragment float4 {pfs}(VOut in [[stage_in]],
                      texture2d<float> src [[texture(0)]],
                      constant PresentParams &p [[buffer(0)]]) {{
    float2 local = in.pos.xy - p.origin;
    if (p.mode == 0) {{
        return float4(src.read(uint2(local)).rgb, 1.0);
    }}
    constexpr sampler s(filter::linear, address::clamp_to_edge, coord::normalized);
    return float4(src.sample(s, local / p.size).rgb, 1.0);
}}
"#,
        m0 = row(DECODE_MATRIX[0]),
        m1 = row(DECODE_MATRIX[1]),
        m2 = row(DECODE_MATRIX[2]),
        bias = FLOOR_BIAS,
        nv12 = NV12_KERNEL,
        fill = FILL_KERNEL,
        pvs = PRESENT_VERTEX,
        pfs = PRESENT_FRAGMENT,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn source_embeds_the_decode_matrix() {
        let s = super::source();
        assert!(s.contains("constant float3 M0 = float3(1.003921628, 0.006761258, 1.578002453)"), "{s}");
        assert!(s.contains("constant float BIAS = 0.500"));
        assert_eq!(std::mem::size_of::<super::PresentParams>(), 24);
        assert_eq!(std::mem::size_of::<super::FillParams>(), 32);
    }
}
