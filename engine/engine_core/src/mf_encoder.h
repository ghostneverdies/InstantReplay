// Direct Media Foundation hardware encoder (no FFmpeg). Used only as an Auto fallback, e.g. for Intel
// 13th-gen+ graphics and Arc where FFmpeg 5.1's libmfx Quick Sync path cannot be used. Hardware
// transforms only: it never falls back to Microsoft's software encoder.
#pragma once
#include "ffmpeg_rt.h"
#include "gpu_pipeline.h"

namespace ir {

class MfEncoder {
public:
    ~MfEncoder();
    // `gpu` may be null (memory input only). On success the encoder is ready for frames.
    static std::unique_ptr<MfEncoder> create(MfCodec codec, const GpuDevice* gpu, int w, int h, int fps,
                                             uint32_t bps, Rate rate, Preset preset, std::string* err);
    bool encode_texture(ID3D11Texture2D* nv12, int64_t pts100ns, bool idr, std::vector<EncodedFrame>& out,
                        std::string* err);
    bool encode_memory(const uint8_t* y, size_t y_stride, const uint8_t* uv, size_t uv_stride, int64_t pts100ns,
                       bool idr, std::vector<EncodedFrame>& out, std::string* err);
    bool flush(std::vector<EncodedFrame>& out, std::string* err);
    int kind_code() const { return codec_ == MfCodec::Hevc ? 20 : 30; }
    bool accepts_textures() const { return texture_ok_; }
    HRESULT init_hr() const { return init_hr_; }

private:
    MfEncoder() = default;
public:
    struct Impl; // defined in mf_encoder.cpp
private:
    std::unique_ptr<Impl> p_;
    MfCodec codec_ = MfCodec::H264;
    bool texture_ok_ = false;
    HRESULT init_hr_ = S_OK;
};

// Encodes synthetic frames through a fresh device and reports whether the hardware MFT produced output.
bool probe_mf_encoder(MfCodec codec, int w, int h, int fps, uint32_t bps, HRESULT* hr_out, std::string* err = nullptr);

} // namespace ir
