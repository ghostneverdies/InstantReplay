// OBS-style GPU video path: canvas -> (scale effect) -> NV12_Y / NV12_UV effects -> staging ring.
// Shaders are hand-translated from OBS Studio's .effect files (see ../shaders/*.hlsl).
#pragma once
#include "ir_common.h"

namespace ir {

struct GpuDevice {
    ComPtr<ID3D11Device> dev;
    ComPtr<ID3D11DeviceContext> ctx;
    ComPtr<ID3D11Multithread> mt;
};

// Creates a BGRA-capable hardware device on `adapter` (or the default adapter), multithread-protected.
HRESULT create_gpu_device(IDXGIAdapter* adapter, GpuDevice* out);
// Reads back the adapter LUID of a device (for matching Media Foundation transforms to it).
bool device_luid(ID3D11Device* dev, LUID* luid);

// The canvas is the fixed-size BGRA texture captured frames are placed into (centred, black bars).
// The capture thread writes it, the encoder thread snapshots it; both go through the device lock.
class GpuCanvas {
public:
    HRESULT create(const GpuDevice& gpu, uint32_t w, uint32_t h);
    // Copies the `box` region of `src` into the canvas at (dx,dy); `clear` wipes the bars first.
    void submit(ID3D11Texture2D* src, const D3D11_BOX& box, uint32_t dx, uint32_t dy, bool clear);
    // Copies the canvas into `dst` (same size) under the device lock.
    void snapshot_into(ID3D11Texture2D* dst);
    uint32_t width() const { return w_; }
    uint32_t height() const { return h_; }
    const GpuDevice& gpu() const { return gpu_; }

private:
    GpuDevice gpu_;
    ComPtr<ID3D11Texture2D> tex_;
    ComPtr<ID3D11RenderTargetView> rtv_;
    uint32_t w_ = 0, h_ = 0;
};

enum class Scaler { Bicubic, Bilinear };

struct Nv12Mapped {
    const uint8_t* y = nullptr;
    const uint8_t* uv = nullptr;
    size_t y_stride = 0;
    size_t uv_stride = 0;
};

class ObsVideoPipeline {
public:
    static constexpr int kSlots = 4;

    // `canvas_*` is the canvas size, `out_*` the encoded size (both even). `hd` selects BT.709 over BT.601.
    HRESULT create(const GpuDevice& gpu, uint32_t canvas_w, uint32_t canvas_h, uint32_t out_w, uint32_t out_h,
                   bool hd, Scaler scaler);

    // Scales + converts `source` (BGRA canvas snapshot) into ring slot `slot`, then queues a copy to its
    // staging surface. The caller owns slot lifetime (kSlots slots, keep at most kSlots-1 outstanding).
    void render(ID3D11ShaderResourceView* source, int slot);
    // True when an NV12 texture (all planes in one resource) is available for GPU consumers.
    bool has_nv12_texture() const { return planar_rtv_; }
    ID3D11Texture2D* nv12_texture(int slot) const { return planar_rtv_ ? slots_[slot].nv12.Get() : nullptr; }
    // Maps the staging surface of `slot`; blocks until the GPU copy finished.
    bool map(int slot, Nv12Mapped* out);
    void unmap(int slot);

    uint32_t out_w() const { return out_w_; }
    uint32_t out_h() const { return out_h_; }
    bool uses_plane_views() const { return planar_rtv_; }

private:
    struct Slot {
        // Preferred layout: one NV12 texture with an R8 view of plane 0 and an R8G8 view of plane 1.
        ComPtr<ID3D11Texture2D> nv12;
        ComPtr<ID3D11RenderTargetView> rtv_y, rtv_uv;
        ComPtr<ID3D11Texture2D> stage_nv12;
        // Fallback layout: separate R8 and R8G8 textures.
        ComPtr<ID3D11Texture2D> tex_y, tex_uv;
        ComPtr<ID3D11Texture2D> stage_y, stage_uv;
        bool mapped = false;
    };
    // Builds all resources on a temporary; create() only commits on success.
    HRESULT create_impl(const GpuDevice& gpu, uint32_t canvas_w, uint32_t canvas_h, uint32_t out_w, uint32_t out_h,
                        bool hd, Scaler scaler);
    HRESULT create_slot(Slot& slot);
    HRESULT create_state();
    void draw_pass(ID3D11RenderTargetView* rtv, uint32_t w, uint32_t h, ID3D11VertexShader* vs,
                   ID3D11PixelShader* ps, ID3D11ShaderResourceView* src, ID3D11Buffer* cb);

    GpuDevice gpu_;
    uint32_t canvas_w_ = 0, canvas_h_ = 0, out_w_ = 0, out_h_ = 0;
    bool hd_ = true;
    bool planar_rtv_ = false;
    Scaler scaler_ = Scaler::Bicubic;

    ComPtr<ID3D11VertexShader> vs_pos_, vs_texpos_left_, vs_scale_;
    ComPtr<ID3D11PixelShader> ps_y_, ps_uv_, ps_scale_;
    ComPtr<ID3D11SamplerState> sampler_;
    ComPtr<ID3D11RasterizerState> raster_;
    ComPtr<ID3D11BlendState> blend_;
    ComPtr<ID3D11DepthStencilState> depth_;
    ComPtr<ID3D11Buffer> cb_convert_, cb_scale_;

    ComPtr<ID3D11Texture2D> scaled_;
    ComPtr<ID3D11RenderTargetView> scaled_rtv_;
    ComPtr<ID3D11ShaderResourceView> scaled_srv_;

    Slot slots_[kSlots];
};

// RGB -> limited-range YCbCr rows in OBS's color_vec0/1/2 layout (xyz = coefficients, w = offset).
void obs_color_vectors(bool hd, float vec[3][4]);

} // namespace ir
