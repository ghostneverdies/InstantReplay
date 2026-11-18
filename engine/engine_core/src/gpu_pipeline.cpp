#include "gpu_pipeline.h"

namespace shaders {
#include "gen/obs_vs_pos.h"
#include "gen/obs_vs_texpos_left.h"
#include "gen/obs_ps_y.h"
#include "gen/obs_ps_uv_wide.h"
#include "gen/obs_vs_bicubic.h"
#include "gen/obs_ps_bicubic.h"
#include "gen/obs_vs_bilinear.h"
#include "gen/obs_ps_bilinear.h"
} // namespace shaders

namespace ir {

namespace {

// Mirrors the uniforms in obs_nv12.hlsl (cbuffer ConvertParams).
struct ConvertParams {
    float width, height, width_i, height_i;
    float width_d2, height_d2, width_x2_i, height_x2_i;
    float color_vec0[4], color_vec1[4], color_vec2[4];
    float pad0[4];
};
// Mirrors cbuffer ScaleParams in obs_scale_*.hlsl.
struct ScaleParams {
    float base_dimension[2];
    float base_dimension_i[2];
    float undistort_factor;
    float multiplier;
    float pad0[2];
};
static_assert(sizeof(ConvertParams) % 16 == 0, "constant buffer must be 16-byte aligned");
static_assert(sizeof(ScaleParams) % 16 == 0, "constant buffer must be 16-byte aligned");

HRESULT make_texture(ID3D11Device* dev, uint32_t w, uint32_t h, DXGI_FORMAT fmt, D3D11_USAGE usage, UINT bind,
                     UINT cpu, ID3D11Texture2D** out) {
    D3D11_TEXTURE2D_DESC d = {};
    d.Width = w;
    d.Height = h;
    d.MipLevels = 1;
    d.ArraySize = 1;
    d.Format = fmt;
    d.SampleDesc.Count = 1;
    d.Usage = usage;
    d.BindFlags = bind;
    d.CPUAccessFlags = cpu;
    return dev->CreateTexture2D(&d, nullptr, out);
}

} // namespace

HRESULT create_gpu_device(IDXGIAdapter* adapter, GpuDevice* out) {
    ComPtr<ID3D11Device> dev;
    ComPtr<ID3D11DeviceContext> ctx;
    HRESULT hr = D3D11CreateDevice(adapter, adapter ? D3D_DRIVER_TYPE_UNKNOWN : D3D_DRIVER_TYPE_HARDWARE, nullptr,
                                   D3D11_CREATE_DEVICE_BGRA_SUPPORT, nullptr, 0, D3D11_SDK_VERSION, &dev, nullptr,
                                   &ctx);
    if (FAILED(hr)) return hr;
    ComPtr<ID3D11Multithread> mt;
    if (SUCCEEDED(ctx.As(&mt))) mt->SetMultithreadProtected(TRUE);
    out->dev = dev;
    out->ctx = ctx;
    out->mt = mt;
    return S_OK;
}

bool device_luid(ID3D11Device* dev, LUID* luid) {
    ComPtr<IDXGIDevice> dxgi;
    if (FAILED(dev->QueryInterface(IID_PPV_ARGS(&dxgi)))) return false;
    ComPtr<IDXGIAdapter> adapter;
    if (FAILED(dxgi->GetAdapter(&adapter))) return false;
    DXGI_ADAPTER_DESC desc;
    if (FAILED(adapter->GetDesc(&desc))) return false;
    *luid = desc.AdapterLuid;
    return true;
}

// ------------------------------------------------------------------------------------------------
// Canvas
// ------------------------------------------------------------------------------------------------

HRESULT GpuCanvas::create(const GpuDevice& gpu, uint32_t w, uint32_t h) {
    // Build into temporaries: a failed create() leaves any previous canvas untouched.
    ComPtr<ID3D11Texture2D> tex;
    ComPtr<ID3D11RenderTargetView> rtv;
    HRESULT hr = make_texture(gpu.dev.Get(), w, h, DXGI_FORMAT_B8G8R8A8_UNORM, D3D11_USAGE_DEFAULT,
                              D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE, 0, &tex);
    if (FAILED(hr)) return hr;
    hr = gpu.dev->CreateRenderTargetView(tex.Get(), nullptr, &rtv);
    if (FAILED(hr)) return hr;
    const float black[4] = {0, 0, 0, 1};
    gpu.ctx->ClearRenderTargetView(rtv.Get(), black);
    gpu_ = gpu;
    w_ = w;
    h_ = h;
    tex_ = std::move(tex);
    rtv_ = std::move(rtv);
    return S_OK;
}

void GpuCanvas::submit(ID3D11Texture2D* src, const D3D11_BOX& box, uint32_t dx, uint32_t dy, bool clear) {
    if (gpu_.mt) gpu_.mt->Enter();
    if (clear) {
        const float black[4] = {0, 0, 0, 1};
        gpu_.ctx->ClearRenderTargetView(rtv_.Get(), black);
    }
    gpu_.ctx->CopySubresourceRegion(tex_.Get(), 0, dx, dy, 0, src, 0, &box);
    if (gpu_.mt) gpu_.mt->Leave();
}

void GpuCanvas::snapshot_into(ID3D11Texture2D* dst) {
    if (gpu_.mt) gpu_.mt->Enter();
    gpu_.ctx->CopyResource(dst, tex_.Get());
    if (gpu_.mt) gpu_.mt->Leave();
}

// ------------------------------------------------------------------------------------------------
// Colour: RGB -> limited-range YCbCr (same values OBS builds in video-matrices.c)
// ------------------------------------------------------------------------------------------------

void obs_color_vectors(bool hd, float vec[3][4]) {
    const float kr = hd ? 0.2126f : 0.299f;
    const float kb = hd ? 0.0722f : 0.114f;
    const float kg = 1.0f - kr - kb;
    const float yscale = 219.0f / 255.0f, cscale = 224.0f / 255.0f;
    // Y = 16/255 + yscale * (kr R + kg G + kb B)
    vec[0][0] = yscale * kr;
    vec[0][1] = yscale * kg;
    vec[0][2] = yscale * kb;
    vec[0][3] = 16.0f / 255.0f;
    // U = 128/255 + cscale * (B - Y') / (2 (1 - kb))
    const float ku = cscale / (2.0f * (1.0f - kb));
    vec[1][0] = -ku * kr;
    vec[1][1] = -ku * kg;
    vec[1][2] = ku * (1.0f - kb);
    vec[1][3] = 128.0f / 255.0f;
    // V = 128/255 + cscale * (R - Y') / (2 (1 - kr))
    const float kv = cscale / (2.0f * (1.0f - kr));
    vec[2][0] = kv * (1.0f - kr);
    vec[2][1] = -kv * kg;
    vec[2][2] = -kv * kb;
    vec[2][3] = 128.0f / 255.0f;
}

// ------------------------------------------------------------------------------------------------
// Pipeline
// ------------------------------------------------------------------------------------------------

HRESULT ObsVideoPipeline::create_state() {
    ID3D11Device* d = gpu_.dev.Get();
    HRESULT hr;
#define MK_VS(member, bytes) \
    hr = d->CreateVertexShader(bytes, sizeof(bytes), nullptr, &member); \
    if (FAILED(hr)) return hr;
#define MK_PS(member, bytes) \
    hr = d->CreatePixelShader(bytes, sizeof(bytes), nullptr, &member); \
    if (FAILED(hr)) return hr;
    MK_VS(vs_pos_, shaders::obs_vs_pos)
    MK_VS(vs_texpos_left_, shaders::obs_vs_texpos_left)
    MK_PS(ps_y_, shaders::obs_ps_y)
    MK_PS(ps_uv_, shaders::obs_ps_uv_wide)
    if (scaler_ == Scaler::Bicubic) {
        MK_VS(vs_scale_, shaders::obs_vs_bicubic)
        MK_PS(ps_scale_, shaders::obs_ps_bicubic)
    } else {
        MK_VS(vs_scale_, shaders::obs_vs_bilinear)
        MK_PS(ps_scale_, shaders::obs_ps_bilinear)
    }
#undef MK_VS
#undef MK_PS

    D3D11_SAMPLER_DESC sd = {};
    sd.Filter = D3D11_FILTER_MIN_MAG_MIP_LINEAR;
    sd.AddressU = sd.AddressV = sd.AddressW = D3D11_TEXTURE_ADDRESS_CLAMP;
    sd.MaxLOD = D3D11_FLOAT32_MAX;
    hr = d->CreateSamplerState(&sd, &sampler_);
    if (FAILED(hr)) return hr;

    D3D11_RASTERIZER_DESC rd = {};
    rd.FillMode = D3D11_FILL_SOLID;
    rd.CullMode = D3D11_CULL_NONE;
    rd.DepthClipEnable = TRUE;
    hr = d->CreateRasterizerState(&rd, &raster_);
    if (FAILED(hr)) return hr;

    D3D11_BLEND_DESC bd = {};
    bd.RenderTarget[0].RenderTargetWriteMask = D3D11_COLOR_WRITE_ENABLE_ALL;
    hr = d->CreateBlendState(&bd, &blend_);
    if (FAILED(hr)) return hr;

    D3D11_DEPTH_STENCIL_DESC dsd = {};
    hr = d->CreateDepthStencilState(&dsd, &depth_);
    if (FAILED(hr)) return hr;

    D3D11_BUFFER_DESC cb = {};
    cb.Usage = D3D11_USAGE_DEFAULT;
    cb.BindFlags = D3D11_BIND_CONSTANT_BUFFER;
    cb.ByteWidth = sizeof(ConvertParams);
    ConvertParams cp = {};
    cp.width = (float)out_w_;
    cp.height = (float)out_h_;
    cp.width_i = 1.0f / out_w_;
    cp.height_i = 1.0f / out_h_;
    cp.width_d2 = out_w_ * 0.5f;
    cp.height_d2 = out_h_ * 0.5f;
    cp.width_x2_i = 0.5f / out_w_;
    cp.height_x2_i = 0.5f / out_h_;
    float vec[3][4];
    obs_color_vectors(hd_, vec);
    std::memcpy(cp.color_vec0, vec[0], sizeof(vec[0]));
    std::memcpy(cp.color_vec1, vec[1], sizeof(vec[1]));
    std::memcpy(cp.color_vec2, vec[2], sizeof(vec[2]));
    D3D11_SUBRESOURCE_DATA init = {&cp, 0, 0};
    hr = d->CreateBuffer(&cb, &init, &cb_convert_);
    if (FAILED(hr)) return hr;

    cb.ByteWidth = sizeof(ScaleParams);
    ScaleParams sp = {};
    sp.base_dimension[0] = (float)canvas_w_;
    sp.base_dimension[1] = (float)canvas_h_;
    sp.base_dimension_i[0] = 1.0f / canvas_w_;
    sp.base_dimension_i[1] = 1.0f / canvas_h_;
    sp.undistort_factor = 1.0f;
    sp.multiplier = 1.0f;
    init.pSysMem = &sp;
    return d->CreateBuffer(&cb, &init, &cb_scale_);
}

HRESULT ObsVideoPipeline::create_slot(Slot& s) {
    ID3D11Device* d = gpu_.dev.Get();
    HRESULT hr;
    if (planar_rtv_) {
        hr = make_texture(d, out_w_, out_h_, DXGI_FORMAT_NV12, D3D11_USAGE_DEFAULT,
                          D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE, 0, &s.nv12);
        if (FAILED(hr)) return hr;
        D3D11_RENDER_TARGET_VIEW_DESC rv = {};
        rv.ViewDimension = D3D11_RTV_DIMENSION_TEXTURE2D;
        rv.Format = DXGI_FORMAT_R8_UNORM;
        hr = d->CreateRenderTargetView(s.nv12.Get(), &rv, &s.rtv_y);
        if (FAILED(hr)) return hr;
        rv.Format = DXGI_FORMAT_R8G8_UNORM;
        hr = d->CreateRenderTargetView(s.nv12.Get(), &rv, &s.rtv_uv);
        if (FAILED(hr)) return hr;
        return make_texture(d, out_w_, out_h_, DXGI_FORMAT_NV12, D3D11_USAGE_STAGING, 0, D3D11_CPU_ACCESS_READ,
                            &s.stage_nv12);
    }
    hr = make_texture(d, out_w_, out_h_, DXGI_FORMAT_R8_UNORM, D3D11_USAGE_DEFAULT, D3D11_BIND_RENDER_TARGET, 0,
                      &s.tex_y);
    if (FAILED(hr)) return hr;
    hr = make_texture(d, out_w_ / 2, out_h_ / 2, DXGI_FORMAT_R8G8_UNORM, D3D11_USAGE_DEFAULT,
                      D3D11_BIND_RENDER_TARGET, 0, &s.tex_uv);
    if (FAILED(hr)) return hr;
    hr = d->CreateRenderTargetView(s.tex_y.Get(), nullptr, &s.rtv_y);
    if (FAILED(hr)) return hr;
    hr = d->CreateRenderTargetView(s.tex_uv.Get(), nullptr, &s.rtv_uv);
    if (FAILED(hr)) return hr;
    hr = make_texture(d, out_w_, out_h_, DXGI_FORMAT_R8_UNORM, D3D11_USAGE_STAGING, 0, D3D11_CPU_ACCESS_READ,
                      &s.stage_y);
    if (FAILED(hr)) return hr;
    return make_texture(d, out_w_ / 2, out_h_ / 2, DXGI_FORMAT_R8G8_UNORM, D3D11_USAGE_STAGING, 0,
                        D3D11_CPU_ACCESS_READ, &s.stage_uv);
}

HRESULT ObsVideoPipeline::create(const GpuDevice& gpu, uint32_t canvas_w, uint32_t canvas_h, uint32_t out_w,
                                 uint32_t out_h, bool hd, Scaler scaler) {
    // Commit only after every resource exists; on failure the old pipeline stays usable.
    ObsVideoPipeline fresh;
    HRESULT hr = fresh.create_impl(gpu, canvas_w, canvas_h, out_w, out_h, hd, scaler);
    if (FAILED(hr)) return hr;
    *this = std::move(fresh);
    return S_OK;
}

HRESULT ObsVideoPipeline::create_impl(const GpuDevice& gpu, uint32_t canvas_w, uint32_t canvas_h, uint32_t out_w,
                                      uint32_t out_h, bool hd, Scaler scaler) {
    gpu_ = gpu;
    canvas_w_ = canvas_w;
    canvas_h_ = canvas_h;
    out_w_ = out_w & ~1u;
    out_h_ = out_h & ~1u;
    hd_ = hd;
    scaler_ = scaler;
    HRESULT hr = create_state();
    if (FAILED(hr)) return hr;

    if (canvas_w_ != out_w_ || canvas_h_ != out_h_) {
        hr = make_texture(gpu_.dev.Get(), out_w_, out_h_, DXGI_FORMAT_B8G8R8A8_UNORM, D3D11_USAGE_DEFAULT,
                          D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE, 0, &scaled_);
        if (FAILED(hr)) return hr;
        hr = gpu_.dev->CreateRenderTargetView(scaled_.Get(), nullptr, &scaled_rtv_);
        if (FAILED(hr)) return hr;
        hr = gpu_.dev->CreateShaderResourceView(scaled_.Get(), nullptr, &scaled_srv_);
        if (FAILED(hr)) return hr;
    }

    // Prefer NV12 plane views; fall back to separate R8 / R8G8 textures when the driver refuses.
    planar_rtv_ = true;
    bool ok = true;
    for (auto& s : slots_) {
        if (FAILED(create_slot(s))) {
            ok = false;
            break;
        }
    }
    if (!ok) {
        for (auto& s : slots_) s = Slot();
        planar_rtv_ = false;
        for (auto& s : slots_) {
            hr = create_slot(s);
            if (FAILED(hr)) return hr;
        }
    }
    return S_OK;
}

void ObsVideoPipeline::draw_pass(ID3D11RenderTargetView* rtv, uint32_t w, uint32_t h, ID3D11VertexShader* vs,
                                 ID3D11PixelShader* ps, ID3D11ShaderResourceView* src, ID3D11Buffer* cb) {
    ID3D11DeviceContext* c = gpu_.ctx.Get();
    ID3D11RenderTargetView* rtvs[1] = {rtv};
    c->OMSetRenderTargets(1, rtvs, nullptr);
    D3D11_VIEWPORT vp = {0, 0, (float)w, (float)h, 0.0f, 1.0f};
    c->RSSetViewports(1, &vp);
    c->RSSetState(raster_.Get());
    const float factor[4] = {0, 0, 0, 0};
    c->OMSetBlendState(blend_.Get(), factor, 0xffffffff);
    c->OMSetDepthStencilState(depth_.Get(), 0);
    c->IASetInputLayout(nullptr);
    c->IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
    c->VSSetShader(vs, nullptr, 0);
    c->PSSetShader(ps, nullptr, 0);
    ID3D11Buffer* cbs[1] = {cb};
    c->VSSetConstantBuffers(0, 1, cbs);
    c->PSSetConstantBuffers(0, 1, cbs);
    ID3D11ShaderResourceView* srvs[1] = {src};
    c->PSSetShaderResources(0, 1, srvs);
    ID3D11SamplerState* samplers[1] = {sampler_.Get()};
    c->PSSetSamplers(0, 1, samplers);
    c->Draw(3, 0);
    ID3D11ShaderResourceView* none[1] = {nullptr};
    c->PSSetShaderResources(0, 1, none);
    ID3D11RenderTargetView* nort[1] = {nullptr};
    c->OMSetRenderTargets(1, nort, nullptr);
}

void ObsVideoPipeline::render(ID3D11ShaderResourceView* source, int slot) {
    if (gpu_.mt) gpu_.mt->Enter();
    ID3D11DeviceContext* c = gpu_.ctx.Get();
    ID3D11ShaderResourceView* convert_src = source;

    if (scaled_) {
        draw_pass(scaled_rtv_.Get(), out_w_, out_h_, vs_scale_.Get(), ps_scale_.Get(), source, cb_scale_.Get());
        convert_src = scaled_srv_.Get();
    }

    Slot& s = slots_[slot];
    // NV12_Y: full-resolution luma. NV12_UV: half-resolution, left-sited chroma.
    draw_pass(s.rtv_y.Get(), out_w_, out_h_, vs_pos_.Get(), ps_y_.Get(), convert_src, cb_convert_.Get());
    draw_pass(s.rtv_uv.Get(), out_w_ / 2, out_h_ / 2, vs_texpos_left_.Get(), ps_uv_.Get(), convert_src,
              cb_convert_.Get());

    if (planar_rtv_) {
        c->CopyResource(s.stage_nv12.Get(), s.nv12.Get());
    } else {
        c->CopyResource(s.stage_y.Get(), s.tex_y.Get());
        c->CopyResource(s.stage_uv.Get(), s.tex_uv.Get());
    }
    if (gpu_.mt) gpu_.mt->Leave();
}

bool ObsVideoPipeline::map(int slot, Nv12Mapped* out) {
    Slot& s = slots_[slot];
    ID3D11DeviceContext* c = gpu_.ctx.Get();
    D3D11_MAPPED_SUBRESOURCE m = {};
    if (planar_rtv_) {
        if (FAILED(c->Map(s.stage_nv12.Get(), 0, D3D11_MAP_READ, 0, &m))) return false;
        s.mapped = true;
        out->y = static_cast<const uint8_t*>(m.pData);
        out->y_stride = m.RowPitch;
        out->uv = out->y + (size_t)m.RowPitch * out_h_;
        out->uv_stride = m.RowPitch;
        return true;
    }
    D3D11_MAPPED_SUBRESOURCE my = {}, muv = {};
    if (FAILED(c->Map(s.stage_y.Get(), 0, D3D11_MAP_READ, 0, &my))) return false;
    if (FAILED(c->Map(s.stage_uv.Get(), 0, D3D11_MAP_READ, 0, &muv))) {
        c->Unmap(s.stage_y.Get(), 0);
        return false;
    }
    s.mapped = true;
    out->y = static_cast<const uint8_t*>(my.pData);
    out->y_stride = my.RowPitch;
    out->uv = static_cast<const uint8_t*>(muv.pData);
    out->uv_stride = muv.RowPitch;
    return true;
}

void ObsVideoPipeline::unmap(int slot) {
    Slot& s = slots_[slot];
    if (!s.mapped) return;
    ID3D11DeviceContext* c = gpu_.ctx.Get();
    if (planar_rtv_) {
        c->Unmap(s.stage_nv12.Get(), 0);
    } else {
        c->Unmap(s.stage_y.Get(), 0);
        c->Unmap(s.stage_uv.Get(), 0);
    }
    s.mapped = false;
}

} // namespace ir
