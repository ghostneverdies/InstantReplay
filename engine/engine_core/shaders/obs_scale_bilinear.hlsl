/*
 * Bilinear scaling: OBS Studio's default effect (libobs/data/default.effect, technique Draw),
 * which is a single linear-filtered sample. Hand-translated; OBS Studio is GPL-2.0-or-later and
 * this file is distributed under GPL-3.0-or-later.
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
SamplerState def_sampler : register(s0); // Filter = Linear, AddressU/V = Clamp

struct VertOut {
    float2 uv : TEXCOORD0;
    float4 pos : SV_Position;
};

VertOut VSDefault(uint id : SV_VertexID)
{
    float idHigh = float(id >> 1);
    float idLow = float(id & uint(1));

    VertOut vert_out;
    vert_out.uv = float2(idHigh * 2.0, 1.0 - idLow * 2.0);
    vert_out.pos = float4(idHigh * 4.0 - 1.0, idLow * 4.0 - 1.0, 0.0, 1.0);
    return vert_out;
}

float4 PSDrawBilinear(VertOut v_in) : SV_Target
{
    return image.Sample(def_sampler, v_in.uv);
}
