/*
 * Bicubic scaling, hand-translated from OBS Studio's libobs/data/bicubic_scale.effect
 * ("bicubic sharper (better for downscaling)", adapted by OBS from the GPL bsnes shader).
 * Only the plain SDR `Draw` technique is used (no multiply, tonemap or undistort variants).
 * OBS Studio is GPL-2.0-or-later; this file is distributed under GPL-3.0-or-later.
 *
 * The OBS vertex shader reads sprite vertices and a ViewProj matrix. Here the same full-screen
 * triangle used by the NV12 passes replaces that: the destination uv in [0,1] is multiplied by the
 * source dimensions exactly like OBS's VSDefault (vert_out.uv = v_in.uv * base_dimension).
 */

cbuffer ScaleParams : register(b0)
{
    float2 base_dimension;
    float2 base_dimension_i;
    float undistort_factor;
    float multiplier;
    float2 pad0;
};

Texture2D image : register(t0);
SamplerState textureSampler : register(s0); // Filter = Linear, AddressU/V = Clamp

struct VertOut {
    float2 uv : TEXCOORD0;
    float4 pos : SV_Position;
};

struct FragData {
    float2 uv : TEXCOORD0;
};

VertOut VSDefault(uint id : SV_VertexID)
{
    float idHigh = float(id >> 1);
    float idLow = float(id & uint(1));

    VertOut vert_out;
    vert_out.uv = float2(idHigh * 2.0, 1.0 - idLow * 2.0) * base_dimension;
    vert_out.pos = float4(idHigh * 4.0 - 1.0, idLow * 4.0 - 1.0, 0.0, 1.0);
    return vert_out;
}

float4 weight4(float x)
{
    /* Sharper version.  May look better in some cases. B=0, C=0.75 */
    return float4(
        ((-0.75 * x + 1.5) * x - 0.75) * x,
        (1.25 * x - 2.25) * x * x + 1.0,
        ((-1.25 * x + 1.5) * x + 0.75) * x,
        (0.75 * x - 0.75) * x * x);
}

float4 DrawBicubic(FragData f_in)
{
    float2 pos = f_in.uv;
    float2 pos1 = floor(pos - 0.5) + 0.5;
    float2 f = pos - pos1;

    float4 rowtaps = weight4(f.x);
    float4 coltaps = weight4(f.y);

    float2 uv1 = pos1 * base_dimension_i;
    float2 uv0 = uv1 - base_dimension_i;
    float2 uv2 = uv1 + base_dimension_i;
    float2 uv3 = uv2 + base_dimension_i;

    float u_weight_sum = rowtaps.y + rowtaps.z;
    float u_middle_offset = rowtaps.z * base_dimension_i.x / u_weight_sum;
    float u_middle = uv1.x + u_middle_offset;

    float v_weight_sum = coltaps.y + coltaps.z;
    float v_middle_offset = coltaps.z * base_dimension_i.y / v_weight_sum;
    float v_middle = uv1.y + v_middle_offset;

    int2 coord_top_left = int2(max(uv0 * base_dimension, 0.5));
    int2 coord_bottom_right = int2(min(uv3 * base_dimension, base_dimension - 0.5));

    float4 top = image.Load(int3(coord_top_left, 0)) * rowtaps.x;
    top += image.Sample(textureSampler, float2(u_middle, uv0.y)) * u_weight_sum;
    top += image.Load(int3(coord_bottom_right.x, coord_top_left.y, 0)) * rowtaps.w;
    float4 total = top * coltaps.x;

    float4 middle = image.Sample(textureSampler, float2(uv0.x, v_middle)) * rowtaps.x;
    middle += image.Sample(textureSampler, float2(u_middle, v_middle)) * u_weight_sum;
    middle += image.Sample(textureSampler, float2(uv3.x, v_middle)) * rowtaps.w;
    total += middle * v_weight_sum;

    float4 bottom = image.Load(int3(coord_top_left.x, coord_bottom_right.y, 0)) * rowtaps.x;
    bottom += image.Sample(textureSampler, float2(u_middle, uv3.y)) * u_weight_sum;
    bottom += image.Load(int3(coord_bottom_right, 0)) * rowtaps.w;
    total += bottom * coltaps.w;

    return total;
}

float4 PSDrawBicubicRGBA(FragData f_in) : SV_Target
{
    return DrawBicubic(f_in);
}
