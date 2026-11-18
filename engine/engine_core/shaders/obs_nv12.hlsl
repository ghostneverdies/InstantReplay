/*
 * NV12 format conversion, hand-translated from OBS Studio's libobs/data/format_conversion.effect
 * (techniques NV12_Y and NV12_UV). Copyright (C) 2023 by Lain Bailey <lain@obsproject.com>.
 * OBS Studio is licensed GPL-2.0-or-later; this file is distributed under GPL-3.0-or-later with
 * the rest of Instant Replay. OBS's effect syntax (uniform/techniques/sampler_state) was converted
 * to plain HLSL by hand; libobs is not used.
 *
 * Two passes per frame, each drawn as one full-screen triangle:
 *   NV12_Y : VSPos          + PS_Y        -> R8_UNORM   render target, full resolution
 *   NV12_UV: VSTexPos_Left  + PS_UV_Wide  -> R8G8_UNORM render target, half resolution
 * The UV pass samples the source twice per output pixel (left-sited chroma, bilinear), exactly as
 * OBS does, so chroma positioning matches OBS recordings.
 */

cbuffer ConvertParams : register(b0)
{
    float width;
    float height;
    float width_i;
    float height_i;
    float width_d2;
    float height_d2;
    float width_x2_i;
    float height_x2_i;
    float4 color_vec0;
    float4 color_vec1;
    float4 color_vec2;
    float4 pad0;
};

Texture2D image : register(t0);
SamplerState def_sampler : register(s0); // Filter = Linear, AddressU/V = Clamp (created by the engine)

struct FragPos {
    float4 pos : SV_Position;
};

struct VertTexPosWide {
    float3 uuv : TEXCOORD0;
    float4 pos : SV_Position;
};

struct FragTexWide {
    float3 uuv : TEXCOORD0;
};

FragPos VSPos(uint id : SV_VertexID)
{
    float idHigh = float(id >> 1);
    float idLow = float(id & uint(1));

    float x = idHigh * 4.0 - 1.0;
    float y = idLow * 4.0 - 1.0;

    FragPos vert_out;
    vert_out.pos = float4(x, y, 0.0, 1.0);
    return vert_out;
}

VertTexPosWide VSTexPos_Left(uint id : SV_VertexID)
{
    float idHigh = float(id >> 1);
    float idLow = float(id & uint(1));

    float x = idHigh * 4.0 - 1.0;
    float y = idLow * 4.0 - 1.0;

    float u_right = idHigh * 2.0;
    float u_left = u_right - width_i;
    float v = 1.0 - idLow * 2.0; // obs_glsl_compile is false on Direct3D

    VertTexPosWide vert_out;
    vert_out.uuv = float3(u_left, u_right, v);
    vert_out.pos = float4(x, y, 0.0, 1.0);
    return vert_out;
}

float PS_Y(FragPos frag_in) : SV_Target
{
    float3 rgb = image.Load(int3(frag_in.pos.xy, 0)).rgb;
    float y = dot(color_vec0.xyz, rgb) + color_vec0.w;
    return y;
}

float2 PS_UV_Wide(FragTexWide frag_in) : SV_Target
{
    float3 rgb_left = image.Sample(def_sampler, frag_in.uuv.xz).rgb;
    float3 rgb_right = image.Sample(def_sampler, frag_in.uuv.yz).rgb;
    float3 rgb = (rgb_left + rgb_right) * 0.5;

    float u = dot(color_vec1.xyz, rgb) + color_vec1.w;
    float v = dot(color_vec2.xyz, rgb) + color_vec2.w;
    return float2(u, v);
}
