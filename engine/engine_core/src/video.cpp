// Instant Replay native engine v4 - video data plane.
//
// Single translation unit port of the verified GPU pipeline, FFmpeg encoder and
// Media Foundation encoder. The public contract lives in core.hpp.
//
// Locked invariants implemented here:
//   * canvas snapshot + render from the snapshot SRV (one full CopyResource);
//   * six pipeline slots / six surfaces / five frames in flight;
//   * slot identity: GpuFrame.slot == pipeline slot == AVFrame gpu_frames[slot];
//   * GPU encoders release slots from the AVBufferRef free callback, CPU path
//     uses MappedFrame (Unmap outside any D3D11Multithread guard, then release);
//   * open-time GPU -> CPU staging fallback for the SAME encoder;
//   * keyframes are one contiguous [extradata][picture] PacketRef sample.

#include "core.hpp"

#include <array>
#include <cctype>
#include <cmath>
#include <cstdio>
#include <cwctype>

extern "C"
{
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/channel_layout.h>
#include <libavutil/dict.h>
#include <libavutil/error.h>
#include <libavutil/frame.h>
#include <libavutil/hwcontext.h>
#include <libavutil/hwcontext_d3d11va.h>
#include <libavutil/mem.h>
#include <libavutil/opt.h>
#include <libavutil/rational.h>
#include <libswresample/swresample.h>
}

#include <codecapi.h>
#include <mferror.h>
#include <mfapi.h>
#include <mfidl.h>
#include <mftransform.h>
#include <strmif.h>

namespace shaders
{
#include "../build/gen/obs_vs_pos.h"
#include "../build/gen/obs_vs_texpos_left.h"
#include "../build/gen/obs_ps_y.h"
#include "../build/gen/obs_ps_uv_wide.h"
#include "../build/gen/obs_vs_bicubic.h"
#include "../build/gen/obs_ps_bicubic.h"
#include "../build/gen/obs_vs_bilinear.h"
#include "../build/gen/obs_ps_bilinear.h"
} // namespace shaders

namespace ir
{

    namespace
    {

        // ---------------------------------------------------------------------
        // FFmpeg loading (delay-loaded 5.1.2 DLLs from engine\encoder)
        // ---------------------------------------------------------------------
        const char *const kLibs[] = {"avutil-57.dll", "swresample-4.dll", "avcodec-59.dll", "avformat-59.dll"};
        std::mutex g_load_mutex;
        bool g_loaded = false;

        std::wstring module_dir()
        {
            HMODULE self = nullptr;
            GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                               reinterpret_cast<LPCWSTR>(&module_dir), &self);
            wchar_t buf[1024];
            DWORD n = GetModuleFileNameW(self, buf, 1024);
            if (n == 0 || n >= 1024)
                return L"";
            std::wstring p(buf, n);
            size_t slash = p.find_last_of(L"\\/");
            return slash == std::wstring::npos ? L"" : p.substr(0, slash);
        }

        bool file_exists(const std::wstring &p)
        {
            DWORD a = GetFileAttributesW(p.c_str());
            return a != INVALID_FILE_ATTRIBUTES && !(a & FILE_ATTRIBUTE_DIRECTORY);
        }
        bool dir_exists(const std::wstring &p)
        {
            DWORD a = GetFileAttributesW(p.c_str());
            return a != INVALID_FILE_ATTRIBUTES && (a & FILE_ATTRIBUTE_DIRECTORY);
        }

        std::wstring encoder_dir()
        {
            wchar_t env[1024];
            DWORD n = GetEnvironmentVariableW(L"IR_FFMPEG_DIR", env, 1024);
            if (n > 0 && n < 1024)
                return std::wstring(env, n);
            std::wstring dir = module_dir();
            if (dir.empty())
                return L"encoder";
            size_t slash = dir.find_last_of(L"\\/");
            if (slash != std::wstring::npos)
            {
                std::wstring sibling = dir.substr(0, slash) + L"\\encoder";
                if (dir_exists(sibling))
                    return sibling;
            }
            std::wstring flat = dir + L"\\encoder";
            if (dir_exists(flat))
                return flat;
            return slash != std::wstring::npos ? dir.substr(0, slash) + L"\\encoder" : flat;
        }

        std::string av_err(int code)
        {
            char buf[128] = {};
            av_strerror(code, buf, sizeof(buf));
            return std::string(buf) + " (" + std::to_string(code) + ")";
        }

        const AVRational kTb100ns = {1, 10'000'000};

        bool is_hevc_name(const std::string &n) { return n.rfind("hevc_", 0) == 0 || n == "libx265"; }
        bool is_h264_name(const std::string &n) { return n.rfind("h264_", 0) == 0 || n == "libx264"; }

        // NVENC and AMF advertise AV_CODEC_CAP_HARDWARE. FFmpeg's QSV encoders do not,
        // even though they are hardware-only, so they must be excluded from this check.
        bool needs_hardware_capability_flag(const std::string &n)
        {
            return n.find("nvenc") != std::string::npos ||
                   n.find("amf") != std::string::npos;
        }

        // Software encoders (and the probe) must stay on the CPU path. MF is handled separately.
        bool supports_gpu_frames(const std::string &n)
        {
            return n == "h264_nvenc" || n == "hevc_nvenc" || n == "h264_amf" || n == "hevc_amf" ||
                   n == "h264_qsv" || n == "hevc_qsv";
        }

        // ---------------------------------------------------------------------
        // Annex-B helpers - exact port of the verified heuristics.
        // ---------------------------------------------------------------------
        struct Nal
        {
            size_t start, end;
        };

        std::vector<Nal> nal_units(const uint8_t *d, size_t n)
        {
            std::vector<Nal> out;
            if (!d)
                return out;
            size_t i = 0;
            long cur = -1;
            while (i + 3 <= n)
            {
                if (d[i] == 0 && d[i + 1] == 0 && d[i + 2] == 1)
                {
                    if (cur >= 0)
                    {
                        size_t e = i;
                        while (e > (size_t)cur && d[e - 1] == 0)
                            --e;
                        out.push_back({(size_t)cur, e});
                    }
                    cur = (long)(i + 3);
                    i += 3;
                }
                else
                {
                    ++i;
                }
            }
            if (cur >= 0 && (size_t)cur < n)
                out.push_back({(size_t)cur, n});
            return out;
        }

        bool has_param_sets(const uint8_t *d, size_t n)
        {
            bool vps = false, hevc_sps = false, hevc_pps = false;
            bool avc_sps = false, avc_pps = false;
            const auto units = nal_units(d, n);
            const size_t limit = std::min<size_t>(6, units.size());
            for (size_t i = 0; i < limit; ++i)
            {
                const Nal &u = units[i];
                if (u.end <= u.start)
                    continue;
                uint8_t b = d[u.start];
                switch ((b >> 1) & 0x3f)
                {
                case 32:
                    vps = true;
                    break;
                case 33:
                    hevc_sps = true;
                    break;
                case 34:
                    hevc_pps = true;
                    break;
                default:
                    break;
                }
                switch (b & 0x1f)
                {
                case 7:
                    avc_sps = true;
                    break;
                case 8:
                    avc_pps = true;
                    break;
                default:
                    break;
                }
            }
            return (vps && hevc_sps && hevc_pps) || (avc_sps && avc_pps);
        }

        bool params_are_hevc(const std::vector<uint8_t> &ps)
        {
            for (auto &u : nal_units(ps.data(), ps.size()))
            {
                if (u.end <= u.start)
                    continue;
                uint8_t b = ps[u.start];
                if (((b >> 1) & 0x3f) == 32 || ((b >> 1) & 0x3f) == 33)
                    return true;
                if ((b & 0x1f) == 7 || (b & 0x1f) == 8)
                    return false;
            }
            return false;
        }

        std::vector<uint8_t> extract_param_sets(const uint8_t *d, size_t n)
        {
            std::vector<uint8_t> vps, sps, pps, avc_sps, avc_pps;
            auto store = [&](std::vector<uint8_t> &dst, const Nal &u)
            {
                dst.insert(dst.end(), {0, 0, 0, 1});
                dst.insert(dst.end(), d + u.start, d + u.end);
            };
            for (auto &u : nal_units(d, n))
            {
                if (u.end <= u.start)
                    continue;
                uint8_t b = d[u.start];
                switch ((b >> 1) & 0x3f)
                {
                case 32:
                    if (vps.empty())
                        store(vps, u);
                    break;
                case 33:
                    if (sps.empty())
                        store(sps, u);
                    break;
                case 34:
                    if (pps.empty())
                        store(pps, u);
                    break;
                default:
                    break;
                }
                switch (b & 0x1f)
                {
                case 7:
                    if (avc_sps.empty())
                        store(avc_sps, u);
                    break;
                case 8:
                    if (avc_pps.empty())
                        store(avc_pps, u);
                    break;
                default:
                    break;
                }
            }
            std::vector<uint8_t> out;
            if (!vps.empty() && !sps.empty() && !pps.empty())
            {
                out.insert(out.end(), vps.begin(), vps.end());
                out.insert(out.end(), sps.begin(), sps.end());
                out.insert(out.end(), pps.begin(), pps.end());
            }
            else if (!avc_sps.empty() && !avc_pps.empty())
            {
                out.insert(out.end(), avc_sps.begin(), avc_sps.end());
                out.insert(out.end(), avc_pps.begin(), avc_pps.end());
            }
            return out;
        }

        bool packet_has_picture(const uint8_t *d, size_t n, bool hevc)
        {
            bool found_nal = false;
            for (auto &u : nal_units(d, n))
            {
                found_nal = true;
                if (u.end <= u.start)
                    continue;
                uint8_t b = d[u.start];
                if (hevc)
                {
                    if (((b >> 1) & 0x3f) <= 21)
                        return true;
                }
                else
                {
                    int t = b & 0x1f;
                    if (t == 1 || t == 5)
                        return true;
                }
            }
            // A length-prefixed AVCC/HVCC packet has no start codes, so it must be kept.
            return !found_nal;
        }

        // ---------------------------------------------------------------------
        // Codec option strings - preserved verbatim from the verified encoder.
        // ---------------------------------------------------------------------
        const char *nvenc_preset(Preset p)
        {
            return p == Preset::Speed ? "p1" : p == Preset::Quality ? "p6" : "p1";
        }
        const char *nvenc_rc(Rate r) { return r == Rate::Cbr ? "cbr" : "vbr"; }
        const char *amf_preset(Preset p)
        {
            return p == Preset::Speed ? "speed" : p == Preset::Quality ? "quality" : "balanced";
        }
        const char *amf_usage(Preset p)
        {
            return p == Preset::Speed ? "ultralowlatency" : p == Preset::Quality ? "high_quality" : "transcoding";
        }
        const char *amf_rc(Rate r) { return r == Rate::Cbr ? "cbr" : "vbr_peak"; }
        const char *qsv_preset(Preset p)
        {
            return p == Preset::Speed ? "veryfast" : p == Preset::Quality ? "slow" : "medium";
        }

        using Pairs = std::vector<std::pair<const char *, const char *>>;

        Pairs codec_options(const std::string &name, const VideoOptions &o)
        {
            const bool cbr = o.rate == Rate::Cbr;
            const bool quality = o.preset == Preset::Quality;
            const bool speed = o.preset == Preset::Speed;
            if (name == "hevc_nvenc" || name == "h264_nvenc")
            {
                // Low latency / no AQ avoids send_frame stalls on first-generation encoders.
                // kSlots textures, never more than kSlots-1 frames in flight.
                const std::string surfaces = std::to_string(ObsVideoPipeline::kSlots);
                const std::string depth = std::to_string(ObsVideoPipeline::kSlots - 1);
                Pairs v = {{"preset", nvenc_preset(o.preset)},
                           {"tune", "ll"},
                           {"rc", nvenc_rc(o.rate)},
                           {"profile", name[0] == 'h' && name[1] == 'e' ? "main" : "high"},
                           {"forced-idr", "1"},
                           {"spatial-aq", "0"},
                           {"temporal-aq", "0"},
                           {"multipass", "disabled"},
                           {"rc-lookahead", "0"},
                           {"zerolatency", "1"},
                           {"surfaces", surfaces.c_str()},
                           {"async_depth", depth.c_str()}};
                return v;
            }
            if (name == "hevc_amf" || name == "h264_amf")
            {
                Pairs v = {{"usage", amf_usage(o.preset)},
                           {"quality", amf_preset(o.preset)},
                           {"rc", amf_rc(o.rate)},
                           {"profile", name == "hevc_amf" ? "main" : "high"},
                           {"forced_idr", "1"},
                           {"header_spacing", "1"},
                           {"preanalysis", quality ? "1" : "0"},
                           {"vbaq", "1"},
                           {"enforce_hrd", cbr ? "1" : "0"}};
                if (name == "hevc_amf")
                    v.push_back({"latency", speed ? "1" : "0"});
                return v;
            }
            if (name == "hevc_qsv" || name == "h264_qsv")
            {
                return {{"preset", qsv_preset(o.preset)},
                        {"profile", name == "hevc_qsv" ? "main" : "high"},
                        {"forced_idr", "1"},
                        {"look_ahead", cbr ? "0" : "1"},
                        {"low_delay_brc", cbr ? "1" : "0"}};
            }
            if (name == "libx265")
            {
                Pairs v = {{"preset", "veryfast"}, {"profile", "main"}, {"forced-idr", "1"}};
                if (cbr)
                    v.push_back({"x265-params", "hrd=1:repeat-headers=1"});
                return v;
            }
            if (name == "libx264")
                return {{"preset", "veryfast"}, {"profile", "high"}, {"forced-idr", "1"}};
            return {};
        }

        // ---------------------------------------------------------------------
        // Selection helpers.
        // ---------------------------------------------------------------------
        std::vector<std::string> candidates(Enc enc, CodecSel codec)
        {
            const bool hevc = codec == CodecSel::Hevc;
            const char *pre = hevc ? "hevc_" : "h264_";
            switch (enc)
            {
            case Enc::Nvenc:
                return {std::string(pre) + "nvenc"};
            case Enc::Amf:
                return {std::string(pre) + "amf"};
            case Enc::Qsv:
                return {std::string(pre) + "qsv"};
            case Enc::X264:
                return {hevc ? "libx265" : "libx264"};
            default:
                // Auto order: NVENC -> AMF -> QSV; MF and x264 are handled by select_video.
                return {std::string(pre) + "nvenc", std::string(pre) + "amf", std::string(pre) + "qsv"};
            }
        }

        uint32_t availability_bit(const std::string &n)
        {
            if (n == "h264_nvenc")
                return ENC_H264_NVENC;
            if (n == "hevc_nvenc")
                return ENC_HEVC_NVENC;
            if (n == "h264_amf")
                return ENC_H264_AMF;
            if (n == "hevc_amf")
                return ENC_HEVC_AMF;
            if (n == "h264_qsv")
                return ENC_H264_QSV;
            if (n == "hevc_qsv")
                return ENC_HEVC_QSV;
            if (n == "libx264")
                return ENC_H264_X264;
            if (n == "libx265")
                return ENC_HEVC_X265;
            return 0xFFFFFFFFu;
        }

        const char *label_of(const std::string &n)
        {
            if (n.find("nvenc") != std::string::npos)
                return "NVIDIA NVENC";
            if (n.find("amf") != std::string::npos)
                return "AMD AMF";
            if (n.find("qsv") != std::string::npos)
                return "Intel Quick Sync";
            return n == "libx264" ? "x264 (CPU)" : n == "libx265" ? "x265 (CPU)" : n.c_str();
        }

        const char *container_for(const std::string &path)
        {
            size_t dot = path.find_last_of('.');
            std::string ext = dot == std::string::npos ? "" : path.substr(dot + 1);
            for (auto &ch : ext)
                ch = (char)std::tolower((unsigned char)ch);
            if (ext == "mkv" || ext == "mka")
                return "matroska";
            if (ext == "mov")
                return "mov";
            if (ext == "ts" || ext == "m2ts" || ext == "mts")
                return "mpegts";
            if (ext == "flv")
                return "flv";
            return "mp4";
        }

        // ---------------------------------------------------------------------
        // D3D11 helpers.
        // ---------------------------------------------------------------------
        HRESULT make_texture(ID3D11Device *dev, uint32_t w, uint32_t h, DXGI_FORMAT fmt, D3D11_USAGE usage, UINT bind,
                             UINT cpu, ID3D11Texture2D **out)
        {
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

        UINT choose_nv12_bind_flags(ID3D11Device *dev, uint32_t w, uint32_t h)
        {
            constexpr UINT base = D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE;
            const UINT candidates[] = {
                base | D3D11_BIND_DECODER | D3D11_BIND_VIDEO_ENCODER,
                base | D3D11_BIND_VIDEO_ENCODER,
                base | D3D11_BIND_DECODER,
                base,
            };
            for (UINT flags : candidates)
            {
                ComPtr<ID3D11Texture2D> probe;
                if (SUCCEEDED(make_texture(dev, w, h, DXGI_FORMAT_NV12, D3D11_USAGE_DEFAULT, flags, 0, &probe)))
                    return flags;
            }
            return base;
        }

        // Mirrors the uniforms in obs_nv12.hlsl (cbuffer ConvertParams).
        struct ConvertParams
        {
            float width, height, width_i, height_i;
            float width_d2, height_d2, width_x2_i, height_x2_i;
            float color_vec0[4], color_vec1[4], color_vec2[4];
            float pad0[4];
        };
        // Mirrors cbuffer ScaleParams in obs_scale_*.hlsl.
        struct ScaleParams
        {
            float base_dimension[2];
            float base_dimension_i[2];
            float undistort_factor;
            float multiplier;
            float pad0[2];
        };
        static_assert(sizeof(ConvertParams) % 16 == 0, "constant buffer must be 16-byte aligned");
        static_assert(sizeof(ScaleParams) % 16 == 0, "constant buffer must be 16-byte aligned");

        // ---------------------------------------------------------------------
        // PacketRef / GpuFrame buffer release thunks.
        // ---------------------------------------------------------------------
        void release_packet_buffer(void *, uint8_t *data) noexcept
        {
            if (!data)
                return;
            AVBufferRef *ref = reinterpret_cast<AVBufferRef *>(data);
            av_buffer_unref(&ref);
        }

        // One AVBufferRef per pipeline slot. The buffer owns an extra texture reference and
        // the exact GpuFrame lease for that slot; the free callback releases both.
        struct D3d11BufferCtx
        {
            Ref<GpuFrame> frame;
            ID3D11Texture2D *tex = nullptr;
            ~D3d11BufferCtx()
            {
                if (tex)
                    tex->Release();
            }
        };

        struct D3d11PoolCtx
        {
            ObsVideoPipeline *pipe = nullptr; // acquire_slot() mutates the slot pool
            std::mutex mutex;
            int next = 0;
        };

        void free_d3d11_buffer(void *opaque, uint8_t *data)
        {
            auto *bc = static_cast<D3d11BufferCtx *>(opaque);
            av_free(data);
            delete bc;
        }

        AVBufferRef *alloc_d3d11_buffer(void *opaque, size_t)
        {
            auto *pc = static_cast<D3d11PoolCtx *>(opaque);
            if (!pc || !pc->pipe)
                return nullptr;
            int slot;
            {
                std::lock_guard<std::mutex> lock(pc->mutex);
                slot = pc->next++;
            }
            if (slot < 0 || slot >= ObsVideoPipeline::kSlots)
                return nullptr; // never hand out more than the six registered textures
            ID3D11Texture2D *tex = pc->pipe->nv12_texture((uint32_t)slot);
            if (!tex)
                return nullptr;
            Ref<GpuFrame> frame = pc->pipe->acquire_slot((uint32_t)slot);
            if (!frame)
                return nullptr;
            auto *bc = new D3d11BufferCtx();
            bc->frame = std::move(frame);
            tex->AddRef();
            bc->tex = tex;
            auto *desc = static_cast<AVD3D11FrameDescriptor *>(av_mallocz(sizeof(AVD3D11FrameDescriptor)));
            if (!desc)
            {
                delete bc;
                return nullptr;
            }
            desc->texture = tex;
            desc->index = 0;
            AVBufferRef *buf = av_buffer_create(reinterpret_cast<uint8_t *>(desc), sizeof(AVD3D11FrameDescriptor),
                                                free_d3d11_buffer, bc, 0);
            if (!buf)
            {
                av_free(desc);
                delete bc;
                return nullptr;
            }
            return buf;
        }

        void free_d3d11_pool(void *opaque) { delete static_cast<D3d11PoolCtx *>(opaque); }

        // MF friendly-name GUIDs (not declared by the SDK headers we build against).
        static const GUID kMftFriendlyNameUtf8 = {0x314ff923, 0x2c6d, 0x4cd2, {0x8d, 0x32, 0x20, 0xe0, 0xa3, 0xe0, 0xd6, 0x8e}};
        static const GUID kMftName = {0x314ffbae, 0x5b41, 0x4c95, {0x9c, 0x19, 0x4e, 0x7d, 0x58, 0x6f, 0xac, 0xe3}};

    } // namespace

    // -------------------------------------------------------------------------
    // Device / LUID / colour.
    // -------------------------------------------------------------------------
    HRESULT create_gpu_device(IDXGIAdapter *adapter, GpuContext *out)
    {
        ComPtr<ID3D11Device> dev;
        ComPtr<ID3D11DeviceContext> ctx;
        // FFmpeg's D3D11VA hardware context needs ID3D11VideoDevice/ID3D11VideoContext, which require
        // D3D11_CREATE_DEVICE_VIDEO_SUPPORT. Some drivers refuse it; keep the BGRA-only fallback.
        UINT flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT;
        HRESULT hr = D3D11CreateDevice(adapter, adapter ? D3D_DRIVER_TYPE_UNKNOWN : D3D_DRIVER_TYPE_HARDWARE, nullptr,
                                       flags, nullptr, 0, D3D11_SDK_VERSION, &dev, nullptr, &ctx);
        if (FAILED(hr))
        {
            dev.Reset();
            ctx.Reset();
            flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT;
            hr = D3D11CreateDevice(adapter, adapter ? D3D_DRIVER_TYPE_UNKNOWN : D3D_DRIVER_TYPE_HARDWARE, nullptr,
                                   flags, nullptr, 0, D3D11_SDK_VERSION, &dev, nullptr, &ctx);
        }
        if (FAILED(hr))
            return hr;
        ComPtr<ID3D11Multithread> mt;
        if (SUCCEEDED(ctx.As(&mt)))
            mt->SetMultithreadProtected(TRUE);
        out->dev = dev;
        out->ctx = ctx;
        out->mt = mt;
        return S_OK;
    }

    bool device_luid(ID3D11Device *dev, LUID *luid)
    {
        ComPtr<IDXGIDevice> dxgi;
        if (FAILED(dev->QueryInterface(IID_PPV_ARGS(&dxgi))))
            return false;
        ComPtr<IDXGIAdapter> adapter;
        if (FAILED(dxgi->GetAdapter(&adapter)))
            return false;
        DXGI_ADAPTER_DESC desc;
        if (FAILED(adapter->GetDesc(&desc)))
            return false;
        *luid = desc.AdapterLuid;
        return true;
    }

    void obs_color_vectors(bool hd, float vec[3][4])
    {
        const float kr = hd ? 0.2126f : 0.299f;
        const float kb = hd ? 0.0722f : 0.114f;
        const float kg = 1.0f - kr - kb;
        const float yscale = 219.0f / 255.0f, cscale = 224.0f / 255.0f;
        vec[0][0] = yscale * kr;
        vec[0][1] = yscale * kg;
        vec[0][2] = yscale * kb;
        vec[0][3] = 16.0f / 255.0f;
        const float ku = cscale / (2.0f * (1.0f - kb));
        vec[1][0] = -ku * kr;
        vec[1][1] = -ku * kg;
        vec[1][2] = ku * (1.0f - kb);
        vec[1][3] = 128.0f / 255.0f;
        const float kv = cscale / (2.0f * (1.0f - kr));
        vec[2][0] = kv * (1.0f - kr);
        vec[2][1] = -kv * kg;
        vec[2][2] = -kv * kb;
        vec[2][3] = 128.0f / 255.0f;
    }

    // -------------------------------------------------------------------------
    // GpuCanvas.
    // -------------------------------------------------------------------------
    struct GpuCanvas::Impl
    {
        GpuContext gpu;
        ComPtr<ID3D11Texture2D> tex;
        ComPtr<ID3D11RenderTargetView> rtv;
        uint32_t w = 0, h = 0;
    };

    GpuCanvas::GpuCanvas() : p_(std::make_unique<Impl>()) {}
    GpuCanvas::~GpuCanvas() = default;
    GpuCanvas::GpuCanvas(GpuCanvas &&) noexcept = default;
    GpuCanvas &GpuCanvas::operator=(GpuCanvas &&) noexcept = default;

    HRESULT GpuCanvas::create(const GpuContext &gpu, uint32_t w, uint32_t h)
    {
        auto fresh = std::make_unique<Impl>();
        HRESULT hr = make_texture(gpu.dev.Get(), w, h, DXGI_FORMAT_B8G8R8A8_UNORM, D3D11_USAGE_DEFAULT,
                                  D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE, 0, &fresh->tex);
        if (FAILED(hr))
            return hr;
        hr = gpu.dev->CreateRenderTargetView(fresh->tex.Get(), nullptr, &fresh->rtv);
        if (FAILED(hr))
            return hr;
        const float black[4] = {0, 0, 0, 1};
        gpu.ctx->ClearRenderTargetView(fresh->rtv.Get(), black);
        fresh->gpu = gpu;
        fresh->w = w;
        fresh->h = h;
        p_ = std::move(fresh);
        return S_OK;
    }

    void GpuCanvas::submit(ID3D11Texture2D *src, const D3D11_BOX &box, uint32_t dx, uint32_t dy, bool clear)
    {
        if (!p_ || !p_->gpu.ctx)
            return;
        ScopedMt lock(p_->gpu);
        if (clear)
        {
            const float black[4] = {0, 0, 0, 1};
            p_->gpu.ctx->ClearRenderTargetView(p_->rtv.Get(), black);
        }
        p_->gpu.ctx->CopySubresourceRegion(p_->tex.Get(), 0, dx, dy, 0, src, 0, &box);
    }

    void GpuCanvas::snapshot_into(ID3D11Texture2D *dst)
    {
        if (!p_ || !p_->gpu.ctx || !dst)
            return;
        ScopedMt lock(p_->gpu);
        p_->gpu.ctx->CopyResource(dst, p_->tex.Get()); // the single full-resource copy
    }

    uint32_t GpuCanvas::width() const noexcept { return p_ ? p_->w : 0; }
    uint32_t GpuCanvas::height() const noexcept { return p_ ? p_->h : 0; }

    const GpuContext &GpuCanvas::gpu() const noexcept
    {
        static const GpuContext empty;
        return p_ ? p_->gpu : empty;
    }

    // -------------------------------------------------------------------------
    // ObsVideoPipeline.
    // -------------------------------------------------------------------------
    struct ObsVideoPipeline::Impl
    {
        struct Slot
        {
            ComPtr<ID3D11Texture2D> nv12;                 // one NV12 texture, R8 + R8G8 RTVs
            ComPtr<ID3D11RenderTargetView> rtv_y, rtv_uv;
            ComPtr<ID3D11Texture2D> stage_nv12;
            ComPtr<ID3D11Texture2D> tex_y, tex_uv;        // split-plane fallback
            ComPtr<ID3D11Texture2D> stage_y, stage_uv;
            bool mapped = false;
        };

        // Fixed pool of exactly kSlots GpuFrames. acquire(slot) returns that exact object;
        // the AVBufferRef free callback (GPU) or MappedFrame destructor (CPU) recycles it.
        struct SlotPool final : PoolBase
        {
            std::array<GpuFrame, kSlots> frames;
            std::array<int, kSlots> free_list;
            std::array<bool, kSlots> leased{};
            int free_count = kSlots;
            std::mutex mutex;

            SlotPool()
            {
                for (int i = 0; i < kSlots; ++i)
                {
                    frames[i].attach_pool(this);
                    frames[i].slot = (uint32_t)i;
                    free_list[i] = i;
                }
            }

            Ref<GpuFrame> acquire(int slot)
            {
                if (slot < 0 || slot >= kSlots)
                    return {};
                std::lock_guard<std::mutex> lock(mutex);
                for (int i = 0; i < free_count; ++i)
                {
                    if (free_list[i] != slot)
                        continue;
                    for (int j = i + 1; j < free_count; ++j)
                        free_list[j - 1] = free_list[j];
                    --free_count;
                    leased[slot] = true;
                    frames[slot].slot = (uint32_t)slot; // identity must survive recycling
                    return Ref<GpuFrame>(&frames[slot], false); // refs are already 1
                }
                return {};
            }

            void recycle(RefCounted *base) noexcept override
            {
                auto *f = static_cast<GpuFrame *>(base);
                const int slot = (int)f->slot;
                f->on_recycle();
                f->reset_refs();
                if (slot < 0 || slot >= kSlots)
                    return;
                std::lock_guard<std::mutex> lock(mutex);
                leased[slot] = false;
                if (free_count < kSlots)
                    free_list[free_count++] = slot;
            }
        };

        GpuContext gpu;
        uint32_t canvas_w = 0, canvas_h = 0, out_w = 0, out_h = 0;
        bool hd = true, planar_rtv = false;
        UINT nv12_bind = 0;
        Scaler scaler = Scaler::Bicubic;

        ComPtr<ID3D11VertexShader> vs_pos, vs_texpos_left, vs_scale;
        ComPtr<ID3D11PixelShader> ps_y, ps_uv, ps_scale;
        ComPtr<ID3D11SamplerState> sampler;
        ComPtr<ID3D11RasterizerState> raster;
        ComPtr<ID3D11BlendState> blend;
        ComPtr<ID3D11DepthStencilState> depth;
        ComPtr<ID3D11Buffer> cb_convert, cb_scale;

        ComPtr<ID3D11Texture2D> scaled;
        ComPtr<ID3D11RenderTargetView> scaled_rtv;
        ComPtr<ID3D11ShaderResourceView> scaled_srv;

        Slot slots[kSlots];
        SlotPool frame_pool;

        HRESULT create_impl(const GpuContext &g, uint32_t cw, uint32_t ch, uint32_t ow, uint32_t oh,
                            bool highdef, Scaler sc);
        HRESULT create_state();
        HRESULT create_slot(Slot &s);
        void draw_pass(ID3D11RenderTargetView *rtv, uint32_t w, uint32_t h, ID3D11VertexShader *vs,
                       ID3D11PixelShader *ps, ID3D11ShaderResourceView *src, ID3D11Buffer *cb);
        void unmap(int slot);

        static void unmap_cb(void *owner, int slot) { static_cast<Impl *>(owner)->unmap(slot); }
    };

    HRESULT ObsVideoPipeline::Impl::create_state()
    {
        ID3D11Device *d = gpu.dev.Get();
        HRESULT hr;
#define MK_VS(member, bytes)                                            \
    hr = d->CreateVertexShader(bytes, sizeof(bytes), nullptr, &member); \
    if (FAILED(hr))                                                     \
        return hr;
#define MK_PS(member, bytes)                                           \
    hr = d->CreatePixelShader(bytes, sizeof(bytes), nullptr, &member); \
    if (FAILED(hr))                                                    \
        return hr;
        MK_VS(vs_pos, shaders::obs_vs_pos)
        MK_VS(vs_texpos_left, shaders::obs_vs_texpos_left)
        MK_PS(ps_y, shaders::obs_ps_y)
        MK_PS(ps_uv, shaders::obs_ps_uv_wide)
        if (scaler == Scaler::Bicubic)
        {
            MK_VS(vs_scale, shaders::obs_vs_bicubic)
            MK_PS(ps_scale, shaders::obs_ps_bicubic)
        }
        else
        {
            MK_VS(vs_scale, shaders::obs_vs_bilinear)
            MK_PS(ps_scale, shaders::obs_ps_bilinear)
        }
#undef MK_VS
#undef MK_PS

        D3D11_SAMPLER_DESC sd = {};
        sd.Filter = D3D11_FILTER_MIN_MAG_MIP_LINEAR;
        sd.AddressU = sd.AddressV = sd.AddressW = D3D11_TEXTURE_ADDRESS_CLAMP;
        sd.MaxLOD = D3D11_FLOAT32_MAX;
        hr = d->CreateSamplerState(&sd, &sampler);
        if (FAILED(hr))
            return hr;

        D3D11_RASTERIZER_DESC rd = {};
        rd.FillMode = D3D11_FILL_SOLID;
        rd.CullMode = D3D11_CULL_NONE;
        rd.DepthClipEnable = TRUE;
        hr = d->CreateRasterizerState(&rd, &raster);
        if (FAILED(hr))
            return hr;

        D3D11_BLEND_DESC bd = {};
        bd.RenderTarget[0].RenderTargetWriteMask = D3D11_COLOR_WRITE_ENABLE_ALL;
        hr = d->CreateBlendState(&bd, &blend);
        if (FAILED(hr))
            return hr;

        D3D11_DEPTH_STENCIL_DESC dsd = {};
        hr = d->CreateDepthStencilState(&dsd, &depth);
        if (FAILED(hr))
            return hr;

        D3D11_BUFFER_DESC cb = {};
        cb.Usage = D3D11_USAGE_DEFAULT;
        cb.BindFlags = D3D11_BIND_CONSTANT_BUFFER;
        cb.ByteWidth = sizeof(ConvertParams);
        ConvertParams cp = {};
        cp.width = (float)out_w;
        cp.height = (float)out_h;
        cp.width_i = 1.0f / out_w;
        cp.height_i = 1.0f / out_h;
        cp.width_d2 = out_w * 0.5f;
        cp.height_d2 = out_h * 0.5f;
        cp.width_x2_i = 0.5f / out_w;
        cp.height_x2_i = 0.5f / out_h;
        float vec[3][4];
        obs_color_vectors(hd, vec);
        std::memcpy(cp.color_vec0, vec[0], sizeof(vec[0]));
        std::memcpy(cp.color_vec1, vec[1], sizeof(vec[1]));
        std::memcpy(cp.color_vec2, vec[2], sizeof(vec[2]));
        D3D11_SUBRESOURCE_DATA init = {&cp, 0, 0};
        hr = d->CreateBuffer(&cb, &init, &cb_convert);
        if (FAILED(hr))
            return hr;

        cb.ByteWidth = sizeof(ScaleParams);
        ScaleParams sp = {};
        sp.base_dimension[0] = (float)canvas_w;
        sp.base_dimension[1] = (float)canvas_h;
        sp.base_dimension_i[0] = 1.0f / canvas_w;
        sp.base_dimension_i[1] = 1.0f / canvas_h;
        sp.undistort_factor = 1.0f;
        sp.multiplier = 1.0f;
        init.pSysMem = &sp;
        return d->CreateBuffer(&cb, &init, &cb_scale);
    }

    HRESULT ObsVideoPipeline::Impl::create_slot(Slot &s)
    {
        ID3D11Device *d = gpu.dev.Get();
        HRESULT hr;
        if (planar_rtv)
        {
            hr = make_texture(d, out_w, out_h, DXGI_FORMAT_NV12, D3D11_USAGE_DEFAULT, nv12_bind, 0, &s.nv12);
            if (FAILED(hr))
                return hr;
            D3D11_RENDER_TARGET_VIEW_DESC rv = {};
            rv.ViewDimension = D3D11_RTV_DIMENSION_TEXTURE2D;
            rv.Format = DXGI_FORMAT_R8_UNORM;
            hr = d->CreateRenderTargetView(s.nv12.Get(), &rv, &s.rtv_y);
            if (FAILED(hr))
                return hr;
            rv.Format = DXGI_FORMAT_R8G8_UNORM;
            hr = d->CreateRenderTargetView(s.nv12.Get(), &rv, &s.rtv_uv);
            if (FAILED(hr))
                return hr;
            return make_texture(d, out_w, out_h, DXGI_FORMAT_NV12, D3D11_USAGE_STAGING, 0, D3D11_CPU_ACCESS_READ,
                                &s.stage_nv12);
        }
        hr = make_texture(d, out_w, out_h, DXGI_FORMAT_R8_UNORM, D3D11_USAGE_DEFAULT, D3D11_BIND_RENDER_TARGET, 0,
                          &s.tex_y);
        if (FAILED(hr))
            return hr;
        hr = make_texture(d, out_w / 2, out_h / 2, DXGI_FORMAT_R8G8_UNORM, D3D11_USAGE_DEFAULT,
                          D3D11_BIND_RENDER_TARGET, 0, &s.tex_uv);
        if (FAILED(hr))
            return hr;
        hr = d->CreateRenderTargetView(s.tex_y.Get(), nullptr, &s.rtv_y);
        if (FAILED(hr))
            return hr;
        hr = d->CreateRenderTargetView(s.tex_uv.Get(), nullptr, &s.rtv_uv);
        if (FAILED(hr))
            return hr;
        hr = make_texture(d, out_w, out_h, DXGI_FORMAT_R8_UNORM, D3D11_USAGE_STAGING, 0, D3D11_CPU_ACCESS_READ,
                          &s.stage_y);
        if (FAILED(hr))
            return hr;
        return make_texture(d, out_w / 2, out_h / 2, DXGI_FORMAT_R8G8_UNORM, D3D11_USAGE_STAGING, 0,
                            D3D11_CPU_ACCESS_READ, &s.stage_uv);
    }

    HRESULT ObsVideoPipeline::Impl::create_impl(const GpuContext &g, uint32_t cw, uint32_t ch, uint32_t ow,
                                                uint32_t oh, bool highdef, Scaler sc)
    {
        gpu = g;
        canvas_w = cw;
        canvas_h = ch;
        out_w = ow & ~1u;
        out_h = oh & ~1u;
        hd = highdef;
        scaler = sc;
        HRESULT hr = create_state();
        if (FAILED(hr))
            return hr;

        if (canvas_w != out_w || canvas_h != out_h)
        {
            hr = make_texture(gpu.dev.Get(), out_w, out_h, DXGI_FORMAT_B8G8R8A8_UNORM, D3D11_USAGE_DEFAULT,
                              D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE, 0, &scaled);
            if (FAILED(hr))
                return hr;
            hr = gpu.dev->CreateRenderTargetView(scaled.Get(), nullptr, &scaled_rtv);
            if (FAILED(hr))
                return hr;
            hr = gpu.dev->CreateShaderResourceView(scaled.Get(), nullptr, &scaled_srv);
            if (FAILED(hr))
                return hr;
        }

        // Prefer NV12 plane views; fall back to separate R8/R8G8 textures when the driver refuses.
        nv12_bind = choose_nv12_bind_flags(gpu.dev.Get(), out_w, out_h);
        planar_rtv = true;
        bool ok = true;
        for (auto &s : slots)
        {
            if (FAILED(create_slot(s)))
            {
                ok = false;
                break;
            }
        }
        if (!ok)
        {
            for (auto &s : slots)
                s = Slot();
            planar_rtv = false;
            for (auto &s : slots)
            {
                hr = create_slot(s);
                if (FAILED(hr))
                    return hr;
            }
        }
        return S_OK;
    }

    void ObsVideoPipeline::Impl::draw_pass(ID3D11RenderTargetView *rtv, uint32_t w, uint32_t h,
                                           ID3D11VertexShader *vs, ID3D11PixelShader *ps,
                                           ID3D11ShaderResourceView *src, ID3D11Buffer *cb)
    {
        ID3D11DeviceContext *c = gpu.ctx.Get();
        ID3D11RenderTargetView *rtvs[1] = {rtv};
        c->OMSetRenderTargets(1, rtvs, nullptr);
        D3D11_VIEWPORT vp = {0, 0, (float)w, (float)h, 0.0f, 1.0f};
        c->RSSetViewports(1, &vp);
        c->RSSetState(raster.Get());
        const float factor[4] = {0, 0, 0, 0};
        c->OMSetBlendState(blend.Get(), factor, 0xffffffff);
        c->OMSetDepthStencilState(depth.Get(), 0);
        c->IASetInputLayout(nullptr);
        c->IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        c->VSSetShader(vs, nullptr, 0);
        c->PSSetShader(ps, nullptr, 0);
        ID3D11Buffer *cbs[1] = {cb};
        c->VSSetConstantBuffers(0, 1, cbs);
        c->PSSetConstantBuffers(0, 1, cbs);
        ID3D11ShaderResourceView *srvs[1] = {src};
        c->PSSetShaderResources(0, 1, srvs);
        ID3D11SamplerState *samplers[1] = {sampler.Get()};
        c->PSSetSamplers(0, 1, samplers);
        c->Draw(3, 0);
        ID3D11ShaderResourceView *none[1] = {nullptr};
        c->PSSetShaderResources(0, 1, none);
        ID3D11RenderTargetView *nort[1] = {nullptr};
        c->OMSetRenderTargets(1, nort, nullptr);
    }

    void ObsVideoPipeline::Impl::unmap(int slot)
    {
        if (slot < 0 || slot >= kSlots)
            return;
        Slot &s = slots[slot];
        if (!s.mapped)
            return;
        ID3D11DeviceContext *c = gpu.ctx.Get();
        if (planar_rtv)
        {
            c->Unmap(s.stage_nv12.Get(), 0);
        }
        else
        {
            c->Unmap(s.stage_y.Get(), 0);
            c->Unmap(s.stage_uv.Get(), 0);
        }
        s.mapped = false;
    }

    ObsVideoPipeline::ObsVideoPipeline() : p_(std::make_unique<Impl>()) {}
    ObsVideoPipeline::~ObsVideoPipeline() = default;
    ObsVideoPipeline::ObsVideoPipeline(ObsVideoPipeline &&) noexcept = default;
    ObsVideoPipeline &ObsVideoPipeline::operator=(ObsVideoPipeline &&) noexcept = default;

    HRESULT ObsVideoPipeline::create(const GpuContext &gpu, uint32_t canvas_w, uint32_t canvas_h,
                                     uint32_t out_w, uint32_t out_h, bool hd, Scaler scaler)
    {
        auto fresh = std::make_unique<Impl>();
        HRESULT hr = fresh->create_impl(gpu, canvas_w, canvas_h, out_w, out_h, hd, scaler);
        if (FAILED(hr))
            return hr;
        p_ = std::move(fresh);
        return S_OK;
    }

    void ObsVideoPipeline::render(ID3D11ShaderResourceView *source, uint32_t slot, bool stage_copy)
    {
        if (!p_ || !p_->gpu.ctx || slot >= (uint32_t)kSlots)
            return;
        ScopedMt lock(p_->gpu);
        ID3D11DeviceContext *c = p_->gpu.ctx.Get();
        ID3D11ShaderResourceView *convert_src = source;

        if (p_->scaled)
        {
            p_->draw_pass(p_->scaled_rtv.Get(), p_->out_w, p_->out_h, p_->vs_scale.Get(), p_->ps_scale.Get(), source,
                          p_->cb_scale.Get());
            convert_src = p_->scaled_srv.Get();
        }

        Impl::Slot &s = p_->slots[slot];
        // NV12_Y: full-resolution luma; NV12_UV: half-resolution, left-sited chroma.
        p_->draw_pass(s.rtv_y.Get(), p_->out_w, p_->out_h, p_->vs_pos.Get(), p_->ps_y.Get(), convert_src,
                      p_->cb_convert.Get());
        p_->draw_pass(s.rtv_uv.Get(), p_->out_w / 2, p_->out_h / 2, p_->vs_texpos_left.Get(), p_->ps_uv.Get(),
                      convert_src, p_->cb_convert.Get());

        if (stage_copy)
        {
            if (p_->planar_rtv)
            {
                c->CopyResource(s.stage_nv12.Get(), s.nv12.Get());
            }
            else
            {
                c->CopyResource(s.stage_y.Get(), s.tex_y.Get());
                c->CopyResource(s.stage_uv.Get(), s.tex_uv.Get());
            }
        }
    }

    bool ObsVideoPipeline::has_nv12_texture() const noexcept { return p_ && p_->planar_rtv; }

    ID3D11Texture2D *ObsVideoPipeline::nv12_texture(uint32_t slot) const noexcept
    {
        if (!p_ || !p_->planar_rtv || slot >= (uint32_t)kSlots)
            return nullptr;
        return p_->slots[slot].nv12.Get();
    }

    UINT ObsVideoPipeline::nv12_bind_flags() const noexcept { return p_ ? p_->nv12_bind : 0u; }

    MappedFrame ObsVideoPipeline::map(uint32_t slot, Ref<GpuFrame> frame) noexcept
    {
        if (!p_ || slot >= (uint32_t)kSlots)
            return {};
        Impl::Slot &s = p_->slots[slot];
        if (s.mapped)
            return {};
        ID3D11DeviceContext *c = p_->gpu.ctx.Get();
        D3D11_MAPPED_SUBRESOURCE m = {};
        // Map/Unmap deliberately runs outside any D3D11Multithread guard: MappedFrame's
        // destructor performs the Unmap and must not take the device lock.
        if (p_->planar_rtv)
        {
            if (FAILED(c->Map(s.stage_nv12.Get(), 0, D3D11_MAP_READ, 0, &m)))
                return {};
            s.mapped = true;
            const uint8_t *y = static_cast<const uint8_t *>(m.pData);
            const size_t row = m.RowPitch;
            const uint8_t *uv = y + row * p_->out_h;
            return MappedFrame(p_.get(), (int)slot, &Impl::unmap_cb, std::move(frame),
                               std::span<const uint8_t>(y, row * p_->out_h),
                               std::span<const uint8_t>(uv, row * (p_->out_h / 2)), row, row);
        }
        D3D11_MAPPED_SUBRESOURCE my = {}, muv = {};
        if (FAILED(c->Map(s.stage_y.Get(), 0, D3D11_MAP_READ, 0, &my)))
            return {};
        if (FAILED(c->Map(s.stage_uv.Get(), 0, D3D11_MAP_READ, 0, &muv)))
        {
            c->Unmap(s.stage_y.Get(), 0);
            return {};
        }
        s.mapped = true;
        return MappedFrame(p_.get(), (int)slot, &Impl::unmap_cb, std::move(frame),
                           std::span<const uint8_t>(static_cast<const uint8_t *>(my.pData),
                                                    (size_t)my.RowPitch * p_->out_h),
                           std::span<const uint8_t>(static_cast<const uint8_t *>(muv.pData),
                                                    (size_t)muv.RowPitch * (p_->out_h / 2)),
                           (size_t)my.RowPitch, (size_t)muv.RowPitch);
    }

    uint32_t ObsVideoPipeline::out_w() const noexcept { return p_ ? p_->out_w : 0; }
    uint32_t ObsVideoPipeline::out_h() const noexcept { return p_ ? p_->out_h : 0; }
    bool ObsVideoPipeline::uses_plane_views() const noexcept { return p_ && p_->planar_rtv; }

    void ObsVideoPipeline::flush_and_wait()
    {
        if (!p_ || !p_->gpu.dev || !p_->gpu.ctx)
            return;
        D3D11_QUERY_DESC qd = {};
        qd.Query = D3D11_QUERY_EVENT;
        ComPtr<ID3D11Query> q;
        if (FAILED(p_->gpu.dev->CreateQuery(&qd, &q)))
        {
            p_->gpu.ctx->Flush();
            return;
        }
        p_->gpu.ctx->End(q.Get());
        p_->gpu.ctx->Flush();
        for (;;)
        {
            BOOL done = FALSE;
            HRESULT hr = p_->gpu.ctx->GetData(q.Get(), &done, sizeof(done), 0);
            if (SUCCEEDED(hr) && done)
                break;
            if (FAILED(hr))
                break;
            SwitchToThread();
        }
    }

    Ref<GpuFrame> ObsVideoPipeline::acquire_slot(uint32_t slot)
    {
        if (!p_ || slot >= (uint32_t)kSlots)
            return {};
        Ref<GpuFrame> f = p_->frame_pool.acquire((int)slot);
        if (!f)
            return {};
        Impl::Slot &s = p_->slots[slot];
        f->nv12 = s.nv12;
        f->staging = p_->planar_rtv ? s.stage_nv12 : ComPtr<ID3D11Texture2D>();
        f->plane_y = s.tex_y;
        f->plane_uv = s.tex_uv;
        return f;
    }

    // -------------------------------------------------------------------------
    // FfVideo.
    // -------------------------------------------------------------------------
    struct FfVideo::Impl
    {
        AVCodecContext *ctx = nullptr;
        AVFrame *frame = nullptr; // CPU staging path only
        AVPacket *pkt = nullptr;
        std::string name;
        std::vector<uint8_t> extradata;
        int w = 0, h = 0;
        bool flushed = false;

        bool gpu = false;
        bool qsv = false;
        AVBufferRef *d3d11_dev_ref = nullptr;
        AVBufferRef *d3d11_frames_ref = nullptr;
        AVBufferRef *qsv_dev_ref = nullptr;
        AVBufferRef *qsv_frames_ref = nullptr;
        AVBufferPool *pool = nullptr; // caller-owned: AVHWFramesContext does not free ctx->pool
        std::vector<AVFrame *> gpu_frames; // one per pipeline slot, identity-locked

        ObjectPool<PacketRef> *packets = nullptr;
        std::unique_ptr<ObjectPool<PacketRef>> local_packets;

        ~Impl()
        {
            release_gpu();
            if (frame)
                av_frame_free(&frame);
            if (pkt)
                av_packet_free(&pkt);
            if (ctx)
                avcodec_free_context(&ctx);
        }

        void release_gpu()
        {
            for (AVFrame *&f : gpu_frames)
                av_frame_free(&f);
            gpu_frames.clear();
            av_buffer_unref(&qsv_frames_ref);
            av_buffer_unref(&qsv_dev_ref);
            av_buffer_unref(&d3d11_frames_ref);
            av_buffer_unref(&d3d11_dev_ref);
            if (pool)
                av_buffer_pool_uninit(&pool);
            gpu = qsv = false;
        }

        Ref<PacketRef> acquire_packet()
        {
            if (packets)
                return packets->acquire();
            if (!local_packets)
                local_packets = std::make_unique<ObjectPool<PacketRef>>(256, 4096);
            return local_packets->acquire();
        }

        // Non-keyframes borrow FFmpeg's buffer (no copy). Keyframes become one contiguous
        // [extradata][picture] PacketRef sample, never two packets.
        bool emit(AVPacket *pkt, std::vector<Ref<PacketRef>> &out, std::string *err)
        {
            const bool hevc = name.rfind("hevc_", 0) == 0;
            if (!pkt->data || pkt->size <= 0 || !packet_has_picture(pkt->data, (size_t)pkt->size, hevc))
                return true;
            const bool key = (pkt->flags & AV_PKT_FLAG_KEY) != 0;
            const int64_t pts = pkt->pts != AV_NOPTS_VALUE ? pkt->pts : pkt->dts;
            Ref<PacketRef> pr = acquire_packet();
            if (!pr)
            {
                if (err)
                    *err = "packet pool exhausted";
                return false;
            }
            if (key && !extradata.empty() && !has_param_sets(pkt->data, (size_t)pkt->size))
            {
                const size_t total = extradata.size() + (size_t)pkt->size;
                pr->owned_.resize(total);
                std::memcpy(pr->owned_.data(), extradata.data(), extradata.size());
                std::memcpy(pr->owned_.data() + extradata.size(), pkt->data, (size_t)pkt->size);
                pr->set(nullptr, nullptr, std::span<const uint8_t>(pr->owned_.data(), pr->owned_.size()), true, pts);
            }
            else if (pkt->buf)
            {
                AVBufferRef *ref = av_buffer_ref(pkt->buf);
                if (!ref)
                {
                    if (err)
                        *err = "out of memory";
                    return false;
                }
                pr->set(ref, &release_packet_buffer,
                        std::span<const uint8_t>(pkt->data, (size_t)pkt->size), key, pts);
            }
            else
            {
                pr->owned_.assign(pkt->data, pkt->data + pkt->size);
                pr->set(nullptr, nullptr, std::span<const uint8_t>(pr->owned_.data(), pr->owned_.size()), key, pts);
            }
            out.push_back(std::move(pr));
            return true;
        }

        bool receive_packets(std::vector<Ref<PacketRef>> &out, std::string *err)
        {
            for (;;)
            {
                int rc = avcodec_receive_packet(ctx, pkt);
                if (rc == AVERROR(EAGAIN) || rc == AVERROR_EOF)
                    return true;
                if (rc < 0)
                {
                    if (err)
                        *err = av_err(rc);
                    return false;
                }
                const bool ok = emit(pkt, out, err);
                av_packet_unref(pkt);
                if (!ok)
                    return false;
            }
        }

        bool setup_gpu_frames(const GpuContext *gpu, const ObsVideoPipeline *pipe, const VideoOptions &o,
                              const std::string &codec_name, std::string *err)
        {
            auto fail = [&](const char *what, int rc) -> bool
            {
                if (err)
                    *err = std::string(what) + ": " + av_err(rc);
                release_gpu();
                return false;
            };
            if (!gpu || !gpu->dev || !gpu->ctx || !pipe || !pipe->has_nv12_texture())
                return fail("GPU texture pipeline is unavailable", AVERROR(EINVAL));

            qsv = codec_name.find("qsv") != std::string::npos;
            d3d11_dev_ref = av_hwdevice_ctx_alloc(AV_HWDEVICE_TYPE_D3D11VA);
            if (!d3d11_dev_ref)
                return fail("could not allocate the D3D11 device context", AVERROR(ENOMEM));
            auto *dev_ctx = reinterpret_cast<AVHWDeviceContext *>(d3d11_dev_ref->data);
            auto *dev_hw = reinterpret_cast<AVD3D11VADeviceContext *>(dev_ctx->hwctx);
            dev_hw->device = gpu->dev.Get();
            dev_hw->device->AddRef();
            dev_hw->device_context = gpu->ctx.Get();
            dev_hw->device_context->AddRef();
            int rc = av_hwdevice_ctx_init(d3d11_dev_ref);
            if (rc < 0)
                return fail("could not initialise the D3D11 device context", rc);

            auto *pc = new D3d11PoolCtx();
            pc->pipe = const_cast<ObsVideoPipeline *>(pipe); // acquire_slot() owns the slot pool
            pool = av_buffer_pool_init2(sizeof(AVD3D11FrameDescriptor), pc, alloc_d3d11_buffer, free_d3d11_pool);
            if (!pool)
            {
                delete pc;
                return fail("could not create the D3D11 frame pool", AVERROR(ENOMEM));
            }

            d3d11_frames_ref = av_hwframe_ctx_alloc(d3d11_dev_ref);
            if (!d3d11_frames_ref)
                return fail("could not allocate the D3D11 frames context", AVERROR(ENOMEM));
            auto *frames = reinterpret_cast<AVHWFramesContext *>(d3d11_frames_ref->data);
            frames->width = o.w;
            frames->height = o.h;
            frames->format = AV_PIX_FMT_D3D11;
            frames->sw_format = AV_PIX_FMT_NV12;
            frames->initial_pool_size = ObsVideoPipeline::kSlots;
            frames->pool = pool;
            auto *frames_hw = reinterpret_cast<AVD3D11VAFramesContext *>(frames->hwctx);
            frames_hw->texture = nullptr; // one texture per pool entry, not an array texture
            frames_hw->BindFlags = pipe->nv12_bind_flags();
            rc = av_hwframe_ctx_init(d3d11_frames_ref);
            if (rc < 0)
                return fail("could not initialise the D3D11 frames context", rc);

            // Our allocator cannot fill this array itself; QSV's frames_derive_to() reads it directly.
            // Entry i must describe pipeline texture i, independent of AVBufferPool's LIFO order.
            if (!frames_hw->texture_infos)
                return fail("the D3D11 frames context has no texture descriptors", AVERROR(EINVAL));
            for (int i = 0; i < ObsVideoPipeline::kSlots; ++i)
            {
                frames_hw->texture_infos[i].texture = pipe->nv12_texture((uint32_t)i);
                frames_hw->texture_infos[i].index = 0;
            }

            // AVBufferPool is LIFO in FFmpeg 5.1.2, so av_hwframe_get_buffer() does not hand
            // back the pipeline textures in slot order. Allocate the six D3D11 frames first,
            // identify each one by its texture pointer, and file it under that slot.
            struct D3d11FramesGuard
            {
                std::array<AVFrame *, ObsVideoPipeline::kSlots> frames{};
                ~D3d11FramesGuard()
                {
                    for (AVFrame *&f : frames)
                        av_frame_free(&f);
                }
            } d3d11_guard;
            std::array<AVFrame *, ObsVideoPipeline::kSlots> d3d11_by_slot{};

            for (int i = 0; i < ObsVideoPipeline::kSlots; ++i)
            {
                AVFrame *f = av_frame_alloc();
                if (!f)
                    return fail("could not allocate a hardware frame", AVERROR(ENOMEM));
                d3d11_guard.frames[i] = f;
                f->format = AV_PIX_FMT_D3D11;
                f->width = o.w;
                f->height = o.h;
                rc = av_hwframe_get_buffer(d3d11_frames_ref, f, 0);
                if (rc < 0)
                    return fail("could not allocate a hardware frame buffer", rc);
                auto *tex = reinterpret_cast<ID3D11Texture2D *>(f->data[0]);
                if (!tex)
                    return fail("hardware frame has no texture", AVERROR(EINVAL));
                int slot = -1;
                for (int s = 0; s < ObsVideoPipeline::kSlots; ++s)
                {
                    if (tex == pipe->nv12_texture((uint32_t)s))
                    {
                        slot = s;
                        break;
                    }
                }
                if (slot < 0 || d3d11_by_slot[slot])
                    return fail("hardware frame slot identity mismatch", AVERROR(EINVAL));
                d3d11_by_slot[slot] = f;
            }

            if (qsv)
            {
                rc = av_hwdevice_ctx_create_derived(&qsv_dev_ref, AV_HWDEVICE_TYPE_QSV, d3d11_dev_ref, 0);
                if (rc < 0 || !qsv_dev_ref)
                    return fail("could not derive the Quick Sync device", rc < 0 ? rc : AVERROR(ENOSYS));
                rc = av_hwframe_ctx_create_derived(&qsv_frames_ref, AV_PIX_FMT_QSV, qsv_dev_ref, d3d11_frames_ref, 0);
                if (rc < 0 || !qsv_frames_ref)
                    return fail("could not derive Quick Sync frames", rc < 0 ? rc : AVERROR(ENOSYS));

                // Map each identified D3D11 frame into the derived QSV frames context.
                // qsv_map_to() picks the surface whose D3D11 texture matches the source, and
                // the mapping keeps a reference to the source frame (and therefore its slot).
                gpu_frames.resize(ObsVideoPipeline::kSlots, nullptr);
                for (int slot = 0; slot < ObsVideoPipeline::kSlots; ++slot)
                {
                    AVFrame *f = av_frame_alloc();
                    if (!f)
                        return fail("could not allocate a hardware frame", AVERROR(ENOMEM));
                    gpu_frames[slot] = f;
                    f->format = AV_PIX_FMT_QSV;
                    f->width = o.w;
                    f->height = o.h;
                    f->hw_frames_ctx = av_buffer_ref(qsv_frames_ref);
                    if (!f->hw_frames_ctx)
                        return fail("could not reference the hardware frames context", AVERROR(ENOMEM));
                    rc = av_hwframe_map(f, d3d11_by_slot[slot], 0);
                    if (rc < 0 || !f->data[3])
                        return fail("could not map a texture into a Quick Sync frame",
                                    rc < 0 ? rc : AVERROR(EINVAL));
                    f->extended_data = f->data;
                }
                // The QSV frames now own references to the D3D11 source frames.
            }
            else
            {
                gpu_frames.resize(ObsVideoPipeline::kSlots, nullptr);
                for (int slot = 0; slot < ObsVideoPipeline::kSlots; ++slot)
                    gpu_frames[slot] = d3d11_by_slot[slot];
                // Ownership moved to gpu_frames_; do not let the guard free them.
                for (AVFrame *&f : d3d11_guard.frames)
                    f = nullptr;
            }
            this->gpu = true;
            return true;
        }

        static std::unique_ptr<FfVideo> open(const std::string &name, const VideoOptions &o,
                                             const GpuContext *gpu, const ObsVideoPipeline *pipe,
                                             ObjectPool<PacketRef> *packets, std::string *err)
        {
            if (!ff_load())
            {
                if (err)
                    *err = last_error();
                return nullptr;
            }
            const AVCodec *codec = avcodec_find_encoder_by_name(name.c_str());
            if (!codec)
            {
                if (err)
                    *err = "not included in this FFmpeg build";
                return nullptr;
            }
            if (needs_hardware_capability_flag(name) && !(codec->capabilities & AV_CODEC_CAP_HARDWARE))
            {
                if (err)
                    *err = "codec is not a hardware encoder";
                return nullptr;
            }
            auto impl = std::make_unique<Impl>();
            impl->name = name;
            impl->packets = packets;
            impl->ctx = avcodec_alloc_context3(codec);
            if (!impl->ctx)
            {
                if (err)
                    *err = "out of memory";
                return nullptr;
            }
            AVCodecContext *c = impl->ctx;
            const bool hevc = is_hevc_name(name), h264 = is_h264_name(name);
            int64_t bps = o.bps;
            if (h264)
            {
                bps = o.preset == Preset::Quality ? bps * 4 / 3 : o.preset == Preset::Balanced ? bps * 7 / 5
                                                                                              : bps * 3 / 2;
            }
            c->width = o.w;
            c->height = o.h;
            c->time_base = kTb100ns;
            c->framerate = {o.fps, 1};
            c->pix_fmt = name == "libx264" ? AV_PIX_FMT_YUV420P : AV_PIX_FMT_NV12;
            c->gop_size = o.fps * 2;
            c->max_b_frames = 0;
            c->profile = hevc ? 1 : 100;
            c->bit_rate = bps;
            if (o.rate == Rate::Cbr)
            {
                c->rc_max_rate = bps;
                c->rc_buffer_size = (int)std::min<int64_t>(bps, INT32_MAX);
            }
            else
            {
                c->rc_max_rate = bps * 3 / 2;
                c->rc_buffer_size = (int)std::min<int64_t>(bps * 2, INT32_MAX);
            }
            c->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
            const bool highdef = o.w >= 1280 || o.h > 576;
            c->color_range = AVCOL_RANGE_MPEG;
            c->colorspace = highdef ? AVCOL_SPC_BT709 : AVCOL_SPC_SMPTE170M;
            c->color_primaries = highdef ? AVCOL_PRI_BT709 : AVCOL_PRI_SMPTE170M;
            c->color_trc = highdef ? AVCOL_TRC_BT709 : AVCOL_TRC_SMPTE170M;

            // Set up GPU frames before avcodec_open2(): the encoder inspects hw_frames_ctx during init.
            if (gpu && pipe && pipe->has_nv12_texture() && supports_gpu_frames(name))
            {
                std::string gpu_err;
                if (impl->setup_gpu_frames(gpu, pipe, o, name, &gpu_err))
                {
                    c->pix_fmt = impl->qsv ? AV_PIX_FMT_QSV : AV_PIX_FMT_D3D11;
                    c->hw_frames_ctx = av_buffer_ref(impl->qsv ? impl->qsv_frames_ref : impl->d3d11_frames_ref);
                    if (!c->hw_frames_ctx)
                    {
                        if (err)
                            *err = "could not reference the encoder hardware frames context";
                        return nullptr;
                    }
                }
                else
                {
                    // Open-time GPU -> CPU staging fallback within this same encoder.
                    if (err && err->empty())
                        *err = gpu_err;
                    c->pix_fmt = name == "libx264" ? AV_PIX_FMT_YUV420P : AV_PIX_FMT_NV12;
                }
            }

            AVDictionary *dict = nullptr;
            for (auto &kv : codec_options(name, o))
                av_dict_set(&dict, kv.first, kv.second, 0);
            int rc = avcodec_open2(c, codec, &dict);
            av_dict_free(&dict);
            if (rc < 0)
            {
                if (err)
                    *err = av_err(rc);
                return nullptr;
            }
            if (c->extradata && c->extradata_size > 0)
                impl->extradata.assign(c->extradata, c->extradata + c->extradata_size);
            impl->pkt = av_packet_alloc();
            if (!impl->pkt)
            {
                if (err)
                    *err = "out of memory";
                return nullptr;
            }
            if (!impl->gpu)
            {
                impl->frame = av_frame_alloc();
                if (!impl->frame)
                {
                    if (err)
                        *err = "out of memory";
                    return nullptr;
                }
                impl->frame->width = o.w;
                impl->frame->height = o.h;
                impl->frame->format = c->pix_fmt;
                rc = av_frame_get_buffer(impl->frame, 0);
                if (rc < 0)
                {
                    if (err)
                        *err = av_err(rc);
                    return nullptr;
                }
            }
            impl->w = o.w;
            impl->h = o.h;
            std::unique_ptr<FfVideo> v(new FfVideo());
            v->p_ = std::move(impl);
            return v;
        }
    };

    FfVideo::~FfVideo() = default;
    FfVideo::FfVideo(FfVideo &&) noexcept = default;
    FfVideo &FfVideo::operator=(FfVideo &&) noexcept = default;

    std::unique_ptr<FfVideo> FfVideo::open(const std::string &name, const VideoOptions &o, std::string *err)
    {
        return FfVideo::Impl::open(name, o, nullptr, nullptr, nullptr, err);
    }

    std::unique_ptr<FfVideo> FfVideo::open(const std::string &name, const VideoOptions &o,
                                           const GpuContext *gpu, const ObsVideoPipeline *pipe,
                                           ObjectPool<PacketRef> *packets, std::string *err)
    {
        if (gpu && pipe && pipe->has_nv12_texture() && supports_gpu_frames(name))
        {
            std::string gpu_err;
            auto v = FfVideo::Impl::open(name, o, gpu, pipe, packets, &gpu_err);
            if (v)
                return v;
            // A driver that refuses D3D11/QSV surfaces must still record via CPU staging.
            if (err)
                *err = gpu_err.empty() ? "GPU encoder path failed" : gpu_err;
        }
        return FfVideo::Impl::open(name, o, nullptr, nullptr, packets, err);
    }

    bool FfVideo::encode_nv12(const uint8_t *y, size_t y_stride, const uint8_t *uv, size_t uv_stride,
                              int64_t pts100ns, bool idr, std::vector<Ref<PacketRef>> &out, std::string *err)
    {
        Impl &p = *p_;
        if (p.flushed)
        {
            if (err)
                *err = "encoder was already flushed";
            return false;
        }
        const size_t w = (size_t)p.w, h = (size_t)p.h;
        if (y_stride < w || uv_stride < w)
        {
            if (err)
                *err = "NV12 frame is smaller than the encoder size";
            return false;
        }
        int rc = av_frame_make_writable(p.frame);
        if (rc < 0)
        {
            if (err)
                *err = av_err(rc);
            return false;
        }
        AVFrame *f = p.frame;
        for (size_t row = 0; row < h; ++row)
            std::memcpy(f->data[0] + row * f->linesize[0], y + row * y_stride, w);
        if (p.name == "libx264")
        {
            for (size_t row = 0; row < h / 2; ++row)
            {
                const uint8_t *src = uv + row * uv_stride;
                uint8_t *u = f->data[1] + row * f->linesize[1];
                uint8_t *v = f->data[2] + row * f->linesize[2];
                for (size_t x = 0; x < w / 2; ++x)
                {
                    u[x] = src[x * 2];
                    v[x] = src[x * 2 + 1];
                }
            }
        }
        else
        {
            for (size_t row = 0; row < h / 2; ++row)
                std::memcpy(f->data[1] + row * f->linesize[1], uv + row * uv_stride, w);
        }
        f->pts = pts100ns;
        f->pict_type = idr ? AV_PICTURE_TYPE_I : AV_PICTURE_TYPE_NONE;
        rc = avcodec_send_frame(p.ctx, f);
        if (rc == AVERROR(EAGAIN))
        {
            if (!p.receive_packets(out, err))
                return false;
            rc = avcodec_send_frame(p.ctx, f);
        }
        if (rc < 0)
        {
            if (err)
                *err = av_err(rc);
            return false;
        }
        return p.receive_packets(out, err);
    }

    bool FfVideo::encode_texture(uint32_t slot, int64_t pts100ns, bool idr,
                                 std::vector<Ref<PacketRef>> &out, std::string *err)
    {
        Impl &p = *p_;
        if (!p.gpu || slot >= p.gpu_frames.size() || !p.gpu_frames[slot])
        {
            if (err)
                *err = "this encoder cannot accept GPU textures";
            return false;
        }
        if (p.flushed)
        {
            if (err)
                *err = "encoder was already flushed";
            return false;
        }
        AVFrame *f = p.gpu_frames[slot];
        f->pts = pts100ns;
        f->pict_type = idr ? AV_PICTURE_TYPE_I : AV_PICTURE_TYPE_NONE;
        int rc = avcodec_send_frame(p.ctx, f);
        if (rc == AVERROR(EAGAIN))
        {
            if (!p.receive_packets(out, err))
                return false;
            rc = avcodec_send_frame(p.ctx, f);
        }
        if (rc < 0)
        {
            if (err)
                *err = av_err(rc);
            return false;
        }
        return p.receive_packets(out, err);
    }

    bool FfVideo::accepts_textures() const noexcept { return p_ && p_->gpu; }

    bool FfVideo::flush(std::vector<Ref<PacketRef>> &out, std::string *err)
    {
        Impl &p = *p_;
        if (p.flushed)
            return true;
        p.flushed = true;
        int rc = avcodec_send_frame(p.ctx, nullptr);
        if (rc < 0 && rc != AVERROR_EOF)
        {
            if (err)
                *err = av_err(rc);
            return false;
        }
        return p.receive_packets(out, err);
    }

    int FfVideo::kind_code() const noexcept
    {
        if (!p_)
            return KIND_UNKNOWN;
        const std::string &n = p_->name;
        if (n == "hevc_nvenc")
            return 16;
        if (n == "hevc_amf")
            return 17;
        if (n == "hevc_qsv")
            return 18;
        if (n == "libx265")
            return 19;
        if (n == "hevc_mf")
            return 20;
        if (n == "h264_nvenc")
            return 26;
        if (n == "h264_amf")
            return 27;
        if (n == "h264_qsv")
            return 28;
        if (n == "libx264")
            return 29;
        return 30;
    }

    const std::string &FfVideo::name() const noexcept
    {
        static const std::string empty;
        return p_ ? p_->name : empty;
    }

    namespace
    {

        // ---------------------------------------------------------------------
        // Media Foundation helpers.
        // ---------------------------------------------------------------------
        struct MfRuntime
        {
            HRESULT com_hr = E_FAIL;
            HRESULT mf_hr = E_FAIL;
            bool com = false; // true only when we own a COM reference and must CoUninitialize
            bool mf = false;
            MfRuntime()
            {
                com_hr = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
                if (com_hr == RPC_E_CHANGED_MODE)
                {
                    // FFmpeg's QSV/NVENC probes may put the calling thread in STA before us.
                    // MF works in STA too; just remember that we do not own this apartment.
                    com_hr = S_FALSE;
                }
                else
                {
                    com = SUCCEEDED(com_hr); // S_OK / S_FALSE both leave COM initialized
                }
                if (SUCCEEDED(com_hr))
                {
                    mf_hr = MFStartup(MF_VERSION, MFSTARTUP_FULL);
                    mf = SUCCEEDED(mf_hr);
                }
            }
            ~MfRuntime()
            {
                if (mf)
                    MFShutdown();
                if (com)
                    CoUninitialize();
            }
        };

        MfRuntime &mf_runtime()
        {
            // COM apartments and MFStartup are per-thread: probe and encode may run on different threads.
            static thread_local MfRuntime rt;
            return rt;
        }

        HRESULT set_u32(ICodecAPI *api, const GUID &key, uint32_t value)
        {
            VARIANT v;
            VariantInit(&v);
            v.vt = VT_UI4;
            v.ulVal = value;
            return api->SetValue(&key, &v);
        }

        HRESULT set_bool(ICodecAPI *api, const GUID &key, bool value)
        {
            VARIANT v;
            VariantInit(&v);
            v.vt = VT_BOOL;
            v.boolVal = value ? VARIANT_TRUE : VARIANT_FALSE;
            return api->SetValue(&key, &v);
        }

        std::wstring activate_name(IMFActivate *act)
        {
            wchar_t buf[256] = {};
            UINT32 len = 0;
            if (SUCCEEDED(act->GetString(kMftFriendlyNameUtf8, buf, 256, &len)) && len)
                return std::wstring(buf, len);
            if (SUCCEEDED(act->GetString(kMftName, buf, 256, &len)) && len)
                return std::wstring(buf, len);
            return std::wstring();
        }

        // Intel/Arc/QSV first, NVIDIA last: NVIDIA's hybrid MFT is broken and must never win over QSV.
        int activate_rank(IMFActivate *act)
        {
            const std::wstring name = activate_name(act);
            std::wstring lower(name.size(), L'\0');
            for (size_t i = 0; i < name.size(); ++i)
                lower[i] = (wchar_t)towlower(name[i]);
            auto has = [&](const wchar_t *s) { return lower.find(s) != std::wstring::npos; };
            if (has(L"intel") || has(L"arc") || has(L"iris") || has(L"qsv") || has(L"quick sync"))
                return 2;
            if (has(L"nvidia") || has(L"nvenc"))
                return 0;
            return 1;
        }

        bool activate_matches_luid(IMFActivate *act, const LUID *want)
        {
            if (!act || !want)
                return false;
            UINT32 size = 0;
            if (FAILED(act->GetBlob(MFT_ENUM_ADAPTER_LUID, nullptr, 0, &size)) || size < sizeof(LUID))
                return false;
            LUID got = {};
            UINT32 written = 0;
            if (FAILED(act->GetBlob(MFT_ENUM_ADAPTER_LUID, reinterpret_cast<UINT8 *>(&got), sizeof(got), &written)) ||
                written < sizeof(got))
                return false;
            return got.LowPart == want->LowPart && got.HighPart == want->HighPart;
        }

        // LUID match first, then the Intel/Arc-before-NVIDIA vendor order from activate_rank().
        void rank_activates(IMFActivate **list, UINT32 count, const LUID *luid)
        {
            if (!list || count < 2)
                return;
            LUID want = luid ? *luid : LUID{};
            const bool has_luid = luid != nullptr;
            std::stable_sort(list, list + count, [&](IMFActivate *a, IMFActivate *b)
                             {
                                 int ra = activate_rank(a) + (has_luid && activate_matches_luid(a, &want) ? 4 : 0);
                                 int rb = activate_rank(b) + (has_luid && activate_matches_luid(b, &want) ? 4 : 0);
                                 return ra > rb;
                             });
        }

        struct NalRange
        {
            size_t start, end;
        };

        std::vector<NalRange> annexb_units(const uint8_t *d, size_t n)
        {
            std::vector<NalRange> out;
            if (!d)
                return out;
            size_t i = 0;
            long cur = -1;
            while (i + 3 <= n)
            {
                if (d[i] == 0 && d[i + 1] == 0 && d[i + 2] == 1)
                {
                    if (cur >= 0)
                    {
                        size_t e = i;
                        while (e > (size_t)cur && d[e - 1] == 0)
                            --e;
                        out.push_back({(size_t)cur, e});
                    }
                    cur = (long)(i + 3);
                    i += 3;
                }
                else
                {
                    ++i;
                }
            }
            if (cur >= 0 && (size_t)cur < n)
                out.push_back({(size_t)cur, n});
            return out;
        }

        // Never prepend a partial header: require AVC SPS+PPS or HEVC VPS+SPS+PPS.
        bool has_complete_param_sets(const uint8_t *d, size_t n, bool hevc)
        {
            bool vps = false, sps = false, pps = false;
            for (const auto &u : annexb_units(d, n))
            {
                if (u.end <= u.start || u.start >= n)
                    continue;
                const uint8_t b = d[u.start];
                if (hevc)
                {
                    const int t = (b >> 1) & 0x3f;
                    if (t == 32)
                        vps = true;
                    else if (t == 33)
                        sps = true;
                    else if (t == 34)
                        pps = true;
                }
                else
                {
                    const int t = b & 0x1f;
                    if (t == 7)
                        sps = true;
                    else if (t == 8)
                        pps = true;
                }
            }
            return hevc ? (vps && sps && pps) : (sps && pps);
        }

        bool has_key_nal(const uint8_t *d, size_t n, bool hevc)
        {
            for (const auto &u : annexb_units(d, n))
            {
                if (u.end <= u.start || u.start >= n)
                    continue;
                const uint8_t b = d[u.start];
                const int t = hevc ? ((b >> 1) & 0x3f) : (b & 0x1f);
                if (hevc ? (t >= 16 && t <= 21) : (t == 5))
                    return true;
            }
            return false;
        }

        std::vector<uint8_t> read_seq_header(IMFTransform *mft)
        {
            std::vector<uint8_t> header;
            ComPtr<IMFMediaType> cur;
            if (FAILED(mft->GetOutputCurrentType(0, &cur)) || !cur)
                return header;
            UINT32 size = 0;
            if (FAILED(cur->GetBlobSize(MF_MT_MPEG_SEQUENCE_HEADER, &size)) || size <= 4)
                return header;
            header.resize(size);
            UINT32 written = 0;
            if (FAILED(cur->GetBlob(MF_MT_MPEG_SEQUENCE_HEADER, header.data(), size, &written)) || written <= 4)
            {
                header.clear();
                return header;
            }
            header.resize(written);
            const bool sc3 = header[0] == 0 && header[1] == 0 && header[2] == 1;
            const bool sc4 = header[0] == 0 && header[1] == 0 && header[2] == 0 && header[3] == 1;
            if (!sc3 && !sc4)
                header.clear();
            return header;
        }

    } // namespace

    struct MfEncoder::Impl
    {
        ComPtr<IMFTransform> mft;
        ComPtr<ICodecAPI> api;
        ComPtr<IMFMediaEventGenerator> events;
        ComPtr<IMFDXGIDeviceManager> manager; // kept alive for DXGI-surface input
        bool texture = false;                 // manager attached; encode_texture() is safe
        bool async = false;
        bool provides_samples = false;
        DWORD out_size = 0;
        int w = 0, h = 0;
        int pending_need = 0; // async: METransformNeedInput events not yet answered
        LONGLONG frame_dur = SECOND_100NS / 60;
        std::vector<uint8_t> seq_header;
        bool wrote_header = false;
        bool hevc = false;
        MfCodec codec = MfCodec::H264;
        HRESULT init_hr = S_OK;

        ObjectPool<PacketRef> packets{256, 4096};

        static HRESULT enumerate_hardware(MfCodec codec, const GpuContext *gpu, IMFActivate ***list, UINT32 *count)
        {
            MFT_REGISTER_TYPE_INFO in_type = {MFMediaType_Video, MFVideoFormat_NV12};
            MFT_REGISTER_TYPE_INFO out_type = {MFMediaType_Video,
                                               codec == MfCodec::Hevc ? MFVideoFormat_HEVC : MFVideoFormat_H264};
            const UINT32 flags = MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER;
            // Do NOT filter enumeration by the D3D device's LUID. On hybrid systems that hides the Intel
            // Quick Sync MFT when the primary adapter is discrete. Enumerate every hardware MFT and rank
            // LUID matches first; create() attaches the DXGI manager only to matching transforms.
            HRESULT hr = MFTEnumEx(MFT_CATEGORY_VIDEO_ENCODER, flags, &in_type, &out_type, list, count);
            if (SUCCEEDED(hr) && *list)
            {
                LUID luid = {};
                rank_activates(*list, *count, gpu && gpu->dev && device_luid(gpu->dev.Get(), &luid) ? &luid : nullptr);
            }
            return hr;
        }

        bool process_output(std::vector<Ref<PacketRef>> &out, std::string *err, bool *need_more)
        {
            MFT_OUTPUT_DATA_BUFFER buf = {};
            ComPtr<IMFSample> own;
            if (!provides_samples)
            {
                ComPtr<IMFMediaBuffer> mb;
                if (FAILED(MFCreateSample(&own)) || !own || FAILED(MFCreateMemoryBuffer(out_size, &mb)) || !mb)
                {
                    if (err)
                        *err = "Media Foundation could not allocate an output sample";
                    return false;
                }
                own->AddBuffer(mb.Get());
                buf.pSample = own.Get();
            }
            DWORD status = 0;
            HRESULT hr = mft->ProcessOutput(0, 1, &buf, &status);
            if (need_more)
                *need_more = false;
            if (hr == MF_E_TRANSFORM_NEED_MORE_INPUT)
            {
                if (need_more)
                    *need_more = true;
                if (buf.pEvents)
                    buf.pEvents->Release();
                return true;
            }
            if (hr == MF_E_TRANSFORM_STREAM_CHANGE)
            {
                // Pick a replacement type that still matches what we configured, not blindly index 0.
                ComPtr<IMFMediaType> chosen;
                const GUID want = hevc ? MFVideoFormat_HEVC : MFVideoFormat_H264;
                for (UINT32 idx = 0;; ++idx)
                {
                    ComPtr<IMFMediaType> t;
                    HRESULT th = mft->GetOutputAvailableType(0, idx, &t);
                    if (FAILED(th) || !t)
                        break;
                    GUID subtype = {};
                    UINT32 tw = 0, thh = 0;
                    if (FAILED(t->GetGUID(MF_MT_SUBTYPE, &subtype)) || subtype != want)
                        continue;
                    if (FAILED(MFGetAttributeSize(t.Get(), MF_MT_FRAME_SIZE, &tw, &thh)))
                        continue;
                    if (tw != (UINT32)w || thh != (UINT32)h)
                        continue;
                    chosen = t;
                    break;
                }
                if (chosen && SUCCEEDED(mft->SetOutputType(0, chosen.Get(), 0)))
                {
                    seq_header = read_seq_header(mft.Get());
                    wrote_header = false;
                }
                if (buf.pEvents)
                    buf.pEvents->Release();
                return true;
            }
            if (FAILED(hr))
            {
                if (buf.pEvents)
                    buf.pEvents->Release();
                char m[96];
                std::snprintf(m, sizeof(m), "Media Foundation ProcessOutput failed (0x%08X)", (unsigned)hr);
                if (err)
                    *err = m;
                return false;
            }
            IMFSample *sample = buf.pSample;
            if (sample)
            {
                ComPtr<IMFMediaBuffer> mb;
                if (SUCCEEDED(sample->ConvertToContiguousBuffer(&mb)) && mb)
                {
                    BYTE *data = nullptr;
                    DWORD len = 0;
                    if (SUCCEEDED(mb->Lock(&data, nullptr, &len)) && data && len > 0)
                    {
                        std::vector<uint8_t> bytes;
                        const bool key_nal = has_key_nal(data, len, hevc);
                        UINT32 clean = 0;
                        const bool key = key_nal ||
                                         (SUCCEEDED(sample->GetUINT32(MFSampleExtension_CleanPoint, &clean)) && clean != 0);
                        LONGLONG t = 0;
                        sample->GetSampleTime(&t);
                        const bool packet_params = has_complete_param_sets(data, len, hevc);
                        if (!packet_params)
                        {
                            if (seq_header.empty())
                                seq_header = read_seq_header(mft.Get()); // lazy re-read
                            if (has_complete_param_sets(seq_header.data(), seq_header.size(), hevc) &&
                                (key || !wrote_header))
                            {
                                bytes = seq_header;
                                bytes.insert(bytes.end(), data, data + len);
                            }
                            else
                            {
                                bytes.assign(data, data + len);
                            }
                        }
                        else
                        {
                            bytes.assign(data, data + len);
                        }
                        if (key || packet_params)
                            wrote_header = true;
                        Ref<PacketRef> pr = packets.acquire();
                        if (!pr)
                        {
                            mb->Unlock();
                            if (err)
                                *err = "packet pool exhausted";
                            if (provides_samples && buf.pSample)
                                buf.pSample->Release();
                            if (buf.pEvents)
                                buf.pEvents->Release();
                            return false;
                        }
                        pr->owned_ = std::move(bytes);
                        pr->set(nullptr, nullptr,
                                std::span<const uint8_t>(pr->owned_.data(), pr->owned_.size()), key, t);
                        out.push_back(std::move(pr));
                        mb->Unlock();
                    }
                }
            }
            if (provides_samples && buf.pSample)
                buf.pSample->Release();
            if (buf.pEvents)
                buf.pEvents->Release();
            return true;
        }

        // Async MFTs: pump queued events without blocking. Sync MFTs: pull output until more input is needed.
        bool pump(std::vector<Ref<PacketRef>> &out, std::string *err, bool wait_for_need)
        {
            if (!async)
            {
                for (;;)
                {
                    bool need = false;
                    size_t before = out.size();
                    if (!process_output(out, err, &need))
                        return false;
                    if (need || out.size() == before)
                        return true;
                }
            }
            const auto deadline = std::chrono::steady_clock::now() + std::chrono::milliseconds(200);
            for (;;)
            {
                if (wait_for_need && pending_need == 0 && std::chrono::steady_clock::now() >= deadline)
                {
                    if (err)
                        *err = "Media Foundation encoder timed out waiting for input";
                    return false;
                }
                ComPtr<IMFMediaEvent> ev;
                HRESULT hr = events->GetEvent(MF_EVENT_FLAG_NO_WAIT, &ev);
                if (hr == MF_E_NO_EVENTS_AVAILABLE)
                {
                    if (wait_for_need && pending_need == 0)
                    {
                        if (std::chrono::steady_clock::now() >= deadline)
                        {
                            if (err)
                                *err = "Media Foundation encoder timed out waiting for input";
                            return false;
                        }
                        Sleep(1);
                        continue;
                    }
                    return true;
                }
                if (FAILED(hr))
                {
                    char m[96];
                    std::snprintf(m, sizeof(m), "Media Foundation event queue failed (0x%08X)", (unsigned)hr);
                    if (err)
                        *err = m;
                    return false;
                }
                MediaEventType type = MEUnknown;
                ev->GetType(&type);
                if (type == METransformNeedInput)
                {
                    ++pending_need;
                    if (wait_for_need)
                        return true;
                }
                else if (type == METransformHaveOutput)
                {
                    if (!process_output(out, err, nullptr))
                        return false;
                }
                else if (type == METransformDrainComplete)
                {
                    return true;
                }
                else if (type == MEError)
                {
                    HRESULT status = S_OK;
                    ev->GetStatus(&status);
                    char m[96];
                    std::snprintf(m, sizeof(m), "Media Foundation encoder error (0x%08X)", (unsigned)status);
                    if (err)
                        *err = m;
                    return false;
                }
            }
        }

        bool feed(IMFSample *sample, int64_t pts, bool idr, std::vector<Ref<PacketRef>> &out, std::string *err)
        {
            sample->SetSampleTime(pts);
            if (idr && api)
                set_u32(api.Get(), CODECAPI_AVEncVideoForceKeyFrame, 1);
            if (async)
            {
                while (pending_need == 0)
                { // output events can arrive before NeedInput; keep waiting
                    if (!pump(out, err, true))
                        return false;
                }
            }
            HRESULT hr = mft->ProcessInput(0, sample, 0);
            if (hr == MF_E_NOTACCEPTING && !async)
            {
                if (!pump(out, err, false))
                    return false;
                hr = mft->ProcessInput(0, sample, 0);
            }
            if (FAILED(hr))
            {
                char m[96];
                std::snprintf(m, sizeof(m), "Media Foundation ProcessInput failed (0x%08X)", (unsigned)hr);
                if (err)
                    *err = m;
                return false;
            }
            if (async && pending_need > 0)
                --pending_need;
            return pump(out, err, false);
        }
    };

    MfEncoder::~MfEncoder()
    {
        if (p_ && p_->mft)
            p_->mft->ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
    }

    MfEncoder::MfEncoder(MfEncoder &&) noexcept = default;
    MfEncoder &MfEncoder::operator=(MfEncoder &&) noexcept = default;

    std::unique_ptr<MfEncoder> MfEncoder::create(MfCodec codec, const GpuContext *gpu, int w, int h, int fps,
                                                 uint32_t bps, Rate rate, Preset preset, std::string *err)
    {
        auto fail = [&](const char *what, HRESULT hr) -> std::unique_ptr<MfEncoder>
        {
            char buf[160];
            std::snprintf(buf, sizeof(buf), "Media Foundation %s failed (0x%08X)", what, (unsigned)hr);
            if (err)
                *err = buf;
            return nullptr;
        };
        MfRuntime &rt = mf_runtime();
        if (FAILED(rt.com_hr))
            return fail("COM initialization", rt.com_hr);
        if (FAILED(rt.mf_hr))
            return fail("startup", rt.mf_hr);
        w &= ~1;
        h &= ~1;
        if (w < 2 || h < 2)
            return fail("output size", E_INVALIDARG);

        IMFActivate **list = nullptr;
        UINT32 count = 0;
        HRESULT hr = Impl::enumerate_hardware(codec, gpu, &list, &count);
        if (FAILED(hr) || count == 0)
        {
            if (list)
                CoTaskMemFree(list);
            return fail("hardware encoder enumeration", FAILED(hr) ? hr : MF_E_TOPO_CODEC_NOT_FOUND);
        }

        // A DXGI device manager lets a matching hardware MFT consume our NV12 textures directly.
        // Mismatched transforms (common on hybrid systems) stay in system-memory mode.
        LUID luid = {};
        const bool have_luid = gpu && gpu->dev && device_luid(gpu->dev.Get(), &luid);
        ComPtr<IMFDXGIDeviceManager> manager;
        UINT32 reset_token = 0;
        const bool manager_ready =
            have_luid && SUCCEEDED(MFCreateDXGIDeviceManager(&reset_token, &manager)) && manager &&
            SUCCEEDED(manager->ResetDevice(gpu->dev.Get(), reset_token));

        auto enc = std::unique_ptr<MfEncoder>(new MfEncoder());
        HRESULT last = E_FAIL;
        bool created = false;
        for (UINT32 i = 0; i < count && !created; ++i)
        {
            const bool matching = manager_ready && activate_matches_luid(list[i], &luid);
            // Try DXGI input first on a matching MFT; if the transform rejects it, retry the
            // same MFT with system-memory input before moving on to another vendor.
            const int attempts = matching ? 2 : 1;
            for (int attempt = 0; attempt < attempts && !created; ++attempt)
            {
                const bool use_texture = matching && attempt == 0;
                auto impl = std::make_unique<Impl>();
                impl->w = w;
                impl->h = h;
                impl->hevc = codec == MfCodec::Hevc;
                impl->codec = codec;
                impl->frame_dur = SECOND_100NS / std::max(1, fps);
                hr = list[i]->ActivateObject(IID_PPV_ARGS(&impl->mft));
                if (FAILED(hr) || !impl->mft)
                {
                    last = FAILED(hr) ? hr : E_NOINTERFACE;
                    continue;
                }

                ComPtr<IMFAttributes> attrs;
                if (SUCCEEDED(impl->mft->GetAttributes(&attrs)) && attrs)
                {
                    UINT32 is_async = 0;
                    if (SUCCEEDED(attrs->GetUINT32(MF_TRANSFORM_ASYNC, &is_async)) && is_async)
                    {
                        impl->async = true;
                        attrs->SetUINT32(MF_TRANSFORM_ASYNC_UNLOCK, TRUE);
                        impl->mft.As(&impl->events);
                    }
                    attrs->SetUINT32(MF_LOW_LATENCY, TRUE);
                }
                if (impl->async && !impl->events)
                {
                    last = E_NOINTERFACE;
                    continue;
                }

                if (use_texture)
                {
                    hr = impl->mft->ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, (ULONG_PTR)manager.Get());
                    if (FAILED(hr))
                    {
                        last = hr;
                        continue;
                    }
                    impl->manager = manager;
                    impl->texture = true;
                }

                impl->mft.As(&impl->api);
                if (impl->api)
                {
                    set_u32(impl->api.Get(), CODECAPI_AVEncCommonRateControlMode,
                            rate == Rate::Cbr ? eAVEncCommonRateControlMode_CBR
                                              : eAVEncCommonRateControlMode_PeakConstrainedVBR);
                    set_u32(impl->api.Get(), CODECAPI_AVEncCommonMeanBitRate, bps);
                    if (rate == Rate::Vbr)
                        set_u32(impl->api.Get(), CODECAPI_AVEncCommonMaxBitRate, (uint32_t)((uint64_t)bps * 3 / 2));
                    set_u32(impl->api.Get(), CODECAPI_AVEncMPVGOPSize, (uint32_t)(fps * 2));
                    set_u32(impl->api.Get(), CODECAPI_AVEncMPVDefaultBPictureCount, 0);
                    set_bool(impl->api.Get(), CODECAPI_AVLowLatencyMode, true);
                    set_u32(impl->api.Get(), CODECAPI_AVEncCommonQualityVsSpeed,
                            preset == Preset::Speed ? 20 : preset == Preset::Quality ? 100 : 60);
                }

                const bool highdef = w >= 1280 || h > 576;
                ComPtr<IMFMediaType> out_t;
                if (FAILED(MFCreateMediaType(&out_t)) || !out_t)
                {
                    last = E_OUTOFMEMORY;
                    continue;
                }
                out_t->SetGUID(MF_MT_MAJOR_TYPE, MFMediaType_Video);
                out_t->SetGUID(MF_MT_SUBTYPE, codec == MfCodec::Hevc ? MFVideoFormat_HEVC : MFVideoFormat_H264);
                out_t->SetUINT32(MF_MT_AVG_BITRATE, bps);
                MFSetAttributeSize(out_t.Get(), MF_MT_FRAME_SIZE, w, h);
                MFSetAttributeRatio(out_t.Get(), MF_MT_FRAME_RATE, fps, 1);
                MFSetAttributeRatio(out_t.Get(), MF_MT_PIXEL_ASPECT_RATIO, 1, 1);
                out_t->SetUINT32(MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive);
                out_t->SetUINT32(MF_MT_MPEG2_PROFILE,
                                 codec == MfCodec::Hevc ? (UINT32)eAVEncH265VProfile_Main_420_8
                                                        : (UINT32)eAVEncH264VProfile_High);
                out_t->SetUINT32(MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235);
                out_t->SetUINT32(MF_MT_YUV_MATRIX, highdef ? MFVideoTransferMatrix_BT709 : MFVideoTransferMatrix_BT601);
                out_t->SetUINT32(MF_MT_VIDEO_PRIMARIES, highdef ? MFVideoPrimaries_BT709
                                                                : MFVideoPrimaries_SMPTE170M);
                out_t->SetUINT32(MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709);
                hr = impl->mft->SetOutputType(0, out_t.Get(), 0);
                if (FAILED(hr))
                {
                    last = hr;
                    continue;
                }

                ComPtr<IMFMediaType> in_t;
                if (FAILED(MFCreateMediaType(&in_t)) || !in_t)
                {
                    last = E_OUTOFMEMORY;
                    continue;
                }
                in_t->SetGUID(MF_MT_MAJOR_TYPE, MFMediaType_Video);
                in_t->SetGUID(MF_MT_SUBTYPE, MFVideoFormat_NV12);
                MFSetAttributeSize(in_t.Get(), MF_MT_FRAME_SIZE, w, h);
                MFSetAttributeRatio(in_t.Get(), MF_MT_FRAME_RATE, fps, 1);
                MFSetAttributeRatio(in_t.Get(), MF_MT_PIXEL_ASPECT_RATIO, 1, 1);
                in_t->SetUINT32(MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive);
                in_t->SetUINT32(MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235);
                in_t->SetUINT32(MF_MT_YUV_MATRIX, highdef ? MFVideoTransferMatrix_BT709 : MFVideoTransferMatrix_BT601);
                in_t->SetUINT32(MF_MT_VIDEO_PRIMARIES, highdef ? MFVideoPrimaries_BT709
                                                                : MFVideoPrimaries_SMPTE170M);
                in_t->SetUINT32(MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709);
                hr = impl->mft->SetInputType(0, in_t.Get(), 0);
                if (FAILED(hr))
                {
                    last = hr;
                    continue;
                }

                MFT_OUTPUT_STREAM_INFO info = {};
                if (SUCCEEDED(impl->mft->GetOutputStreamInfo(0, &info)))
                {
                    impl->provides_samples = (info.dwFlags & (MFT_OUTPUT_STREAM_PROVIDES_SAMPLES |
                                                              MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES)) != 0;
                    impl->out_size = std::max<DWORD>(info.cbSize, 1u << 20);
                }
                else
                {
                    impl->out_size = std::max<DWORD>((DWORD)(w * h), 1u << 20);
                }

                impl->seq_header = read_seq_header(impl->mft.Get());

                impl->mft->ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0);
                impl->mft->ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0);
                impl->mft->ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0);
                enc->p_ = std::move(impl);
                created = true;
            }
        }
        for (UINT32 i = 0; i < count; ++i)
            list[i]->Release();
        CoTaskMemFree(list);
        if (!created)
        {
            enc->p_ = std::make_unique<Impl>();
            enc->p_->init_hr = last;
            return fail("encoder setup", last);
        }
        return enc;
    }

    bool MfEncoder::encode_texture(ID3D11Texture2D *nv12, int64_t pts100ns, bool idr,
                                   std::vector<Ref<PacketRef>> &out, std::string *err)
    {
        if (!p_ || !p_->texture || !p_->manager || !nv12)
        {
            if (err)
                *err = "this Media Foundation encoder cannot accept GPU textures";
            return false;
        }
        ComPtr<IMFMediaBuffer> mb;
        HRESULT hr = MFCreateDXGISurfaceBuffer(__uuidof(ID3D11Texture2D), nv12, 0, FALSE, &mb);
        if (FAILED(hr) || !mb)
        {
            if (err)
                *err = "MFCreateDXGISurfaceBuffer failed";
            return false;
        }
        mb->SetCurrentLength((DWORD)(p_->w * p_->h * 3 / 2));
        ComPtr<IMFSample> sample;
        if (FAILED(MFCreateSample(&sample)) || !sample)
        {
            if (err)
                *err = "Media Foundation could not allocate an input sample";
            return false;
        }
        sample->AddBuffer(mb.Get());
        sample->SetSampleDuration(p_->frame_dur);
        return p_->feed(sample.Get(), pts100ns, idr, out, err);
    }

    bool MfEncoder::encode_memory(const uint8_t *y, size_t y_stride, const uint8_t *uv, size_t uv_stride,
                                  int64_t pts100ns, bool idr, std::vector<Ref<PacketRef>> &out, std::string *err)
    {
        if (!p_)
        {
            if (err)
                *err = "Media Foundation encoder is not initialised";
            return false;
        }
        const size_t w = (size_t)p_->w, h = (size_t)p_->h;
        if (!y || !uv || w == 0 || h < 2 || y_stride < w || uv_stride < w)
        {
            if (err)
                *err = "Media Foundation input planes or strides are invalid";
            return false;
        }
        ComPtr<IMFMediaBuffer> mb;
        if (FAILED(MFCreateMemoryBuffer((DWORD)(w * h * 3 / 2), &mb)) || !mb)
        {
            if (err)
                *err = "Media Foundation could not allocate an input buffer";
            return false;
        }
        BYTE *dst = nullptr;
        if (FAILED(mb->Lock(&dst, nullptr, nullptr)) || !dst)
        {
            if (err)
                *err = "Media Foundation input buffer lock failed";
            return false;
        }
        for (size_t r = 0; r < h; ++r)
            std::memcpy(dst + r * w, y + r * y_stride, w);
        for (size_t r = 0; r < h / 2; ++r)
            std::memcpy(dst + w * h + r * w, uv + r * uv_stride, w);
        mb->Unlock();
        mb->SetCurrentLength((DWORD)(w * h * 3 / 2));
        ComPtr<IMFSample> sample;
        if (FAILED(MFCreateSample(&sample)) || !sample)
        {
            if (err)
                *err = "Media Foundation could not allocate an input sample";
            return false;
        }
        sample->AddBuffer(mb.Get());
        sample->SetSampleDuration(p_->frame_dur);
        return p_->feed(sample.Get(), pts100ns, idr, out, err);
    }

    bool MfEncoder::flush(std::vector<Ref<PacketRef>> &out, std::string *err)
    {
        if (!p_)
        {
            if (err)
                *err = "Media Foundation encoder is not initialised";
            return false;
        }
        Impl &p = *p_;
        HRESULT hr = p.mft->ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
        if (FAILED(hr))
        {
            char m[96];
            std::snprintf(m, sizeof(m), "Media Foundation end-of-stream failed (0x%08X)", (unsigned)hr);
            if (err)
                *err = m;
            return false;
        }
        hr = p.mft->ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0);
        if (FAILED(hr))
        {
            char m[96];
            std::snprintf(m, sizeof(m), "Media Foundation drain failed (0x%08X)", (unsigned)hr);
            if (err)
                *err = m;
            return false;
        }
        if (!p.async)
            return p.pump(out, err, false);
        for (int i = 0; i < 400; ++i)
        { // ~2 s ceiling
            ComPtr<IMFMediaEvent> ev;
            HRESULT ehr = p.events->GetEvent(MF_EVENT_FLAG_NO_WAIT, &ev);
            if (ehr == MF_E_NO_EVENTS_AVAILABLE)
            {
                Sleep(5);
                continue;
            }
            if (FAILED(ehr))
                return true;
            MediaEventType type = MEUnknown;
            ev->GetType(&type);
            if (type == METransformHaveOutput)
            {
                if (!p.process_output(out, err, nullptr))
                    return false;
            }
            else if (type == METransformDrainComplete)
            {
                return true;
            }
        }
        return true;
    }

    int MfEncoder::kind_code() const noexcept
    {
        return p_ && p_->hevc ? 20 : 30;
    }

    bool MfEncoder::accepts_textures() const noexcept { return p_ && p_->texture; }

    HRESULT MfEncoder::init_hr() const noexcept { return p_ ? p_->init_hr : E_FAIL; }

    bool probe_mf_encoder(MfCodec codec, int w, int h, int fps, uint32_t bps, HRESULT *hr_out, std::string *err_out)
    {
        if (hr_out)
            *hr_out = S_OK;
        std::string err;
        auto report = [&]() { if (err_out) *err_out = err; return false; };
        // Detection must exercise the same system-memory path the engine falls back to.
        // Passing a D3D device here would attach a DXGI manager on matching MFTs and probe GPU-only behaviour.
        auto enc = MfEncoder::create(codec, nullptr, w, h, fps, bps, Rate::Cbr, Preset::Balanced, &err);
        if (!enc)
        {
            if (hr_out)
                *hr_out = E_FAIL;
            return report();
        }
        w &= ~1;
        h &= ~1;
        std::vector<uint8_t> yp((size_t)w * h, 16), uvp((size_t)w * h / 2, 128);
        std::vector<Ref<PacketRef>> out;
        const int64_t dur = SECOND_100NS / std::max(1, fps);
        for (int i = 0; i < 12; ++i)
        {
            if (!enc->encode_memory(yp.data(), w, uvp.data(), w, i * dur, i == 0, out, &err))
                return report();
            for (auto &f : out)
                if (f && !f->payload.empty())
                    return true;
            Sleep(5);
        }
        if (!enc->flush(out, &err))
            return report();
        for (auto &f : out)
            if (f && !f->payload.empty())
                return true;
        err = "hardware MFT produced no encoded output";
        return report();
    }

    // -------------------------------------------------------------------------
    // FFmpeg load / options / probing / selection / audio conversion.
    // -------------------------------------------------------------------------
    bool annexb_has_param_sets(const uint8_t *data, size_t size) { return has_param_sets(data, size); }

    bool ff_load()
    {
        std::lock_guard<std::mutex> lock(g_load_mutex);
        if (g_loaded)
            return true;
        const std::wstring dir = encoder_dir();
        std::string missing;
        for (const char *lib : kLibs)
        {
            if (!file_exists(dir + L"\\" + utf8_to_wide(lib)))
                missing += (missing.empty() ? "" : ", ") + std::string(lib);
        }
        if (!missing.empty())
        {
            set_last_error("FFmpeg libraries not found in '" + wide_to_utf8(dir.c_str(), dir.size()) + "': missing " +
                           missing + ". Run engine\\encoder\\fetch-ffmpeg.ps1 to install FFmpeg 5.1.2.");
            return false;
        }
        // Load by full path first: the delay-load thunks then resolve "avcodec-59.dll" to these modules.
        for (const char *lib : kLibs)
        {
            std::wstring full = dir + L"\\" + utf8_to_wide(lib);
            if (!LoadLibraryExW(full.c_str(), nullptr, LOAD_WITH_ALTERED_SEARCH_PATH))
            {
                set_last_error(std::string("Could not load '") + wide_to_utf8(full.c_str(), full.size()) +
                               "' (error " + std::to_string(GetLastError()) + ")");
                return false;
            }
        }
        unsigned util = avutil_version() >> 16, codec = avcodec_version() >> 16, format = avformat_version() >> 16;
        if (util != 57 || codec != 59 || format != 59)
        {
            set_last_error("Unsupported FFmpeg build: found avutil " + std::to_string(util) + " / avcodec " +
                           std::to_string(codec) + " / avformat " + std::to_string(format) +
                           ", need 57 / 59 / 59 (FFmpeg 5.1.x).");
            return false;
        }
        g_loaded = true;
        return true;
    }

    VideoOptions options_from_pref(int pref, int w, int h, int fps, uint32_t bps)
    {
        VideoOptions o;
        switch (pref & 0xFF)
        {
        case 1:
            o.enc = Enc::Nvenc;
            break;
        case 2:
            o.enc = Enc::Amf;
            break;
        case 3:
            o.enc = Enc::Qsv;
            break;
        case 4:
            o.enc = Enc::X264;
            break;
        default:
            o.enc = Enc::Auto;
            break;
        }
        o.rate = ((pref >> 8) & 1) ? Rate::Vbr : Rate::Cbr;
        switch ((pref >> 12) & 3)
        {
        case 1:
            o.codec = CodecSel::Hevc;
            break;
        case 2:
            o.codec = CodecSel::H264;
            break;
        default:
            o.codec = CodecSel::Auto;
            break;
        }
        switch ((pref >> 10) & 3)
        {
        case 0:
            o.preset = Preset::Speed;
            break;
        case 2:
            o.preset = Preset::Quality;
            break;
        default:
            o.preset = Preset::Balanced;
            break;
        }
        o.w = w & ~1;
        o.h = h & ~1;
        o.fps = std::max(1, fps);
        o.bps = bps; // managed side already scaled the bitrate; CBR remains the default
        o.avail = ((uint32_t)pref >> 16) & ENC_ALL;
        return o;
    }

    bool probe_ffmpeg_encoder(const char *name, int w, int h, int fps, uint32_t bps, std::string *err)
    {
        if (!ff_load())
        {
            if (err)
                *err = last_error();
            return false;
        }
        VideoOptions o;
        o.codec = is_hevc_name(name) ? CodecSel::Hevc : CodecSel::H264;
        o.w = w & ~1;
        o.h = h & ~1;
        o.fps = std::max(1, fps);
        o.bps = bps;
        std::string local;
        std::string *perr = err ? err : &local;
        perr->clear();
        auto v = FfVideo::open(name, o, perr);
        if (!v)
            return false;
        std::vector<uint8_t> yp((size_t)o.w * o.h, 16), uvp((size_t)o.w * o.h / 2, 128);
        const int64_t dur = SECOND_100NS / o.fps;
        std::vector<Ref<PacketRef>> out;
        auto produced_picture = [&]()
        {
            for (auto &f : out)
                if (f && f->keyframe && !f->payload.empty())
                    return true;
            return false;
        };
        // A real encoder must emit a keyframe within a few frames. Any non-empty packet is not
        // enough: that can be a header-only packet from a half-initialised wrapper.
        for (int i = 0; i < 12; ++i)
        {
            out.clear();
            if (!v->encode_nv12(yp.data(), o.w, uvp.data(), o.w, i * dur, false, out, perr))
                return false;
            if (produced_picture())
                return true;
        }
        out.clear();
        if (!v->flush(out, perr))
            return false;
        return produced_picture();
    }

    bool convert_audio_to_s16_interleaved(const uint8_t *src, int src_rate, int src_channels,
                                          bool src_float, uint32_t frames,
                                          std::vector<int16_t> *out, std::string *err)
    {
        if (!out)
        {
            if (err)
                *err = "invalid output buffer";
            return false;
        }
        out->clear();
        if (!ff_load())
        {
            if (err)
                *err = last_error();
            return false;
        }
        if (frames == 0)
            return true;
        if (!src || src_rate <= 0 || src_channels <= 0)
        {
            if (err)
                *err = "invalid source format";
            return false;
        }

        const AVSampleFormat in_fmt = src_float ? AV_SAMPLE_FMT_FLT : AV_SAMPLE_FMT_S16;
        AVChannelLayout in_layout = {};
        av_channel_layout_default(&in_layout, src_channels);
        if (in_layout.nb_channels <= 0)
        {
            if (err)
                *err = "unsupported channel count";
            return false;
        }
        AVChannelLayout out_layout = {};
        out_layout.order = AV_CHANNEL_ORDER_NATIVE;
        out_layout.nb_channels = AUDIO_CHANNELS;
        out_layout.u.mask = AV_CH_LAYOUT_STEREO;

        SwrContext *swr = nullptr;
        int rc = swr_alloc_set_opts2(&swr, &out_layout, AV_SAMPLE_FMT_S16, AUDIO_RATE,
                                     &in_layout, in_fmt, src_rate, 0, nullptr);
        if (rc < 0 || !swr)
        {
            if (err)
                *err = rc < 0 ? av_err(rc) : "could not allocate audio converter";
            if (swr)
                swr_free(&swr);
            return false;
        }
        rc = swr_init(swr);
        if (rc < 0)
        {
            if (err)
                *err = av_err(rc);
            swr_free(&swr);
            return false;
        }

        const int capacity = (int)std::ceil((double)frames * AUDIO_RATE / (double)src_rate) + 8;
        out->assign((size_t)capacity * AUDIO_CHANNELS, 0);
        const uint8_t *in[1] = {src};
        uint8_t *dst[1] = {reinterpret_cast<uint8_t *>(out->data())};
        int written = swr_convert(swr, dst, capacity, in, (int)frames);
        if (written < 0)
        {
            if (err)
                *err = "audio conversion failed";
            swr_free(&swr);
            out->clear();
            return false;
        }
        if (written < capacity)
        {
            uint8_t *tail = reinterpret_cast<uint8_t *>(out->data()) +
                            (size_t)written * AUDIO_CHANNELS * sizeof(int16_t);
            int extra = swr_convert(swr, &tail, capacity - written, nullptr, 0);
            if (extra > 0)
                written += extra;
        }
        swr_free(&swr);
        out->resize((size_t)std::max(0, written) * AUDIO_CHANNELS);
        return true;
    }

    bool select_video(const VideoOptions &o, bool allow_mf, const GpuContext *gpu, const ObsVideoPipeline *pipe,
                      ObjectPool<PacketRef> *packets, VideoSelection *out, std::string *err)
    {
        if (!ff_load())
        {
            if (err)
                *err = last_error();
            return false;
        }
        const bool auto_mode = o.enc == Enc::Auto;
        const bool known = o.avail != 0;
        std::string tried;
        for (const std::string &name : candidates(o.enc, o.codec))
        {
            const uint32_t bit = availability_bit(name);
            // The stored probe only prunes Auto; an explicit choice is always attempted.
            if (auto_mode && bit != 0xFFFFFFFFu && known && !(o.avail & bit))
                continue;
            std::string e;
            auto v = FfVideo::open(name, o, gpu, pipe, packets, &e);
            if (v)
            {
                out->kind = VideoSelection::Ffmpeg;
                out->video = std::move(v);
                return true;
            }
            tried += std::string(tried.empty() ? "" : "; ") + label_of(name) + ": " + e;
        }
        if (auto_mode)
        {
            // MF is an automatic fallback only (H.264 or HEVC), and HEVC Auto stays hardware-only.
            const bool hevc = o.codec == CodecSel::Hevc;
            const uint32_t mf_bit = hevc ? ENC_HEVC_MF : ENC_H264_MF;
            if (allow_mf && (!known || (o.avail & mf_bit)))
            {
                out->kind = VideoSelection::MediaFoundation;
                out->mf = hevc ? MfCodec::Hevc : MfCodec::H264;
                return true;
            }
            if (!hevc)
            {
                std::string e;
                auto v = FfVideo::open("libx264", o, gpu, pipe, packets, &e);
                if (v)
                {
                    out->kind = VideoSelection::Ffmpeg;
                    out->video = std::move(v);
                    return true;
                }
                tried += std::string(tried.empty() ? "" : "; ") + "x264 (CPU): " + e;
            }
        }
        if (err)
            *err = tried.empty() ? "No working video encoder is available." : "No usable encoder. " + tried;
        return false;
    }

    bool select_video(const VideoOptions &o, bool allow_mf, VideoSelection *out, std::string *err)
    {
        return select_video(o, allow_mf, nullptr, nullptr, nullptr, out, err);
    }

    // -------------------------------------------------------------------------
    // Save: AAC encode + container write. Ported verbatim; the Ref<PacketRef>
    // overload simply projects payloads into VideoSample views and reuses it.
    // -------------------------------------------------------------------------
    namespace
    {

        struct AacPackets
        {
            std::vector<AVPacket *> list;
            ~AacPackets()
            {
                for (auto *p : list)
                    av_packet_free(&p);
            }
        };

        bool drain_audio(AVCodecContext *ctx, AacPackets &out, std::string *err)
        {
            for (;;)
            {
                AVPacket *pkt = av_packet_alloc();
                if (!pkt)
                {
                    if (err)
                        *err = "out of memory";
                    return false;
                }
                int rc = avcodec_receive_packet(ctx, pkt);
                if (rc < 0)
                {
                    av_packet_free(&pkt);
                    if (rc == AVERROR(EAGAIN) || rc == AVERROR_EOF)
                        return true;
                    if (err)
                        *err = av_err(rc);
                    return false;
                }
                out.list.push_back(pkt);
            }
        }

        bool encode_aac(const std::vector<std::pair<const uint8_t *, size_t>> &slices, AVCodecContext *ctx, int rate,
                        int64_t delay_samples, AacPackets &out, std::string *err)
        {
            const int ch = 2;
            const size_t frame_size = (size_t)std::max(256, ctx->frame_size);
            std::vector<float> planes[2];
            int64_t next_pts = delay_samples;
            auto send = [&](size_t count) -> bool
            {
                AVFrame *f = av_frame_alloc();
                if (!f)
                {
                    if (err)
                        *err = "out of memory";
                    return false;
                }
                f->nb_samples = (int)count;
                f->format = AV_SAMPLE_FMT_FLTP;
                f->sample_rate = rate;
                int rc = av_channel_layout_copy(&f->ch_layout, &ctx->ch_layout);
                if (rc >= 0)
                    rc = av_frame_get_buffer(f, 0);
                if (rc < 0)
                {
                    av_frame_free(&f);
                    if (err)
                        *err = av_err(rc);
                    return false;
                }
                for (int c = 0; c < ch; ++c)
                    std::memcpy(f->data[c], planes[c].data(), count * sizeof(float));
                f->pts = next_pts;
                next_pts += (int64_t)count;
                rc = avcodec_send_frame(ctx, f);
                av_frame_free(&f);
                for (int c = 0; c < ch; ++c)
                    planes[c].erase(planes[c].begin(), planes[c].begin() + count);
                if (rc < 0)
                {
                    if (err)
                        *err = av_err(rc);
                    return false;
                }
                return drain_audio(ctx, out, err);
            };
            for (auto &s : slices)
            {
                const size_t frames = s.second / (ch * 2);
                for (size_t n = 0; n < frames; ++n)
                {
                    for (int c = 0; c < ch; ++c)
                    {
                        int16_t v;
                        std::memcpy(&v, s.first + (n * ch + c) * 2, 2);
                        planes[c].push_back(v / 32768.0f);
                    }
                    if (planes[0].size() >= frame_size && !send(frame_size))
                        return false;
                }
            }
            if (!planes[0].empty() && !send(planes[0].size()))
                return false;
            int rc = avcodec_send_frame(ctx, nullptr);
            if (rc < 0 && rc != AVERROR_EOF)
            {
                if (err)
                    *err = av_err(rc);
                return false;
            }
            return drain_audio(ctx, out, err);
        }

    } // namespace

    int mux_until(const std::string &path, const std::vector<VideoSample> &v, const std::vector<AudioChunk> &a, int fps,
                  int w, int h, int seconds_back, int64_t end_time)
    {
        if (v.empty())
            return IR_NOT_ENOUGH_DATA;
        if (!ff_load())
            return IR_FFMPEG_UNAVAILABLE;
        fps = std::max(1, fps);
        w = std::max(2, w);
        h = std::max(2, h);
        const int64_t trim_start = seconds_back > 0 ? end_time - (int64_t)seconds_back * SECOND_100NS : INT64_MIN;

        size_t start = 0;
        if (seconds_back > 0)
        {
            long before = -1;
            for (size_t i = 0; i < v.size(); ++i)
            {
                if (v[i].pts100ns > trim_start)
                    break;
                if (v[i].keyframe)
                    before = (long)i;
            }
            if (before >= 0)
            {
                start = (size_t)before;
            }
            else
            {
                start = 0;
                for (size_t i = 0; i < v.size(); ++i)
                    if (v[i].keyframe)
                    {
                        start = i;
                        break;
                    }
            }
        }
        const size_t count = v.size() - start;
        const VideoSample *s = v.data() + start;
        const int64_t first_pts = (count == 1 && trim_start != INT64_MIN) ? std::max(s[0].pts100ns, trim_start)
                                                                          : s[0].pts100ns;
        const int64_t frame_dur = SECOND_100NS / fps;
        const int64_t last_end = std::max(end_time, s[count - 1].pts100ns + frame_dur);
        if (last_end - first_pts < SECOND_100NS / 2)
            return IR_NOT_ENOUGH_DATA;

        std::vector<uint8_t> extradata;
        for (size_t i = 0; i < count; ++i)
        {
            if (s[i].keyframe)
            {
                extradata = extract_param_sets(s[i].data, s[i].size);
                break;
            }
        }
        if (extradata.empty())
        {
            set_last_error("The buffered video has no HEVC VPS/SPS/PPS or AVC SPS/PPS parameter sets.");
            return IR_FAIL;
        }

        // Clip audio chunks to [first_pts, last_end) with sample accuracy.
        std::vector<std::pair<const uint8_t *, size_t>> slices;
        int64_t audio_first = -1;
        for (const auto &c : a)
        {
            const int64_t dur = (int64_t)c.frames * SECOND_100NS / AUDIO_RATE;
            if (c.pts100ns + dur <= first_pts || c.pts100ns >= last_end)
                continue;
            int64_t skip = 0, keep = c.frames;
            if (c.pts100ns < first_pts)
                skip = (first_pts - c.pts100ns) * AUDIO_RATE / SECOND_100NS;
            const int64_t end_frame = (last_end - c.pts100ns) * AUDIO_RATE / SECOND_100NS;
            if (end_frame < keep)
                keep = end_frame;
            keep -= skip;
            if (keep <= 0)
                continue;
            if (audio_first < 0)
                audio_first = c.pts100ns + skip * SECOND_100NS / AUDIO_RATE;
            slices.push_back({c.data + skip * 4, (size_t)keep * 4});
        }
        const int64_t audio_delay =
            slices.empty() ? 0 : std::max<int64_t>(0, (audio_first - first_pts)) * AUDIO_RATE / SECOND_100NS;

        std::string err;
        auto fail = [&](const std::string &m)
        { set_last_error(m); return IR_FAIL; };

        AVCodecContext *aac = nullptr;
        AacPackets packets;
        if (!slices.empty())
        {
            const AVCodec *codec = avcodec_find_encoder_by_name("aac");
            if (!codec)
                return fail("This FFmpeg build has no AAC encoder.");
            aac = avcodec_alloc_context3(codec);
            if (!aac)
                return fail("out of memory");
            aac->sample_fmt = AV_SAMPLE_FMT_FLTP;
            aac->sample_rate = AUDIO_RATE;
            av_channel_layout_default(&aac->ch_layout, 2);
            aac->bit_rate = 192000;
            aac->time_base = {1, (int)AUDIO_RATE};
            aac->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
            int rc = avcodec_open2(aac, codec, nullptr);
            if (rc < 0)
            {
                avcodec_free_context(&aac);
                return fail("AAC encoder: " + av_err(rc));
            }
            if (!encode_aac(slices, aac, AUDIO_RATE, audio_delay, packets, &err))
            {
                avcodec_free_context(&aac);
                return fail("AAC encoding failed: " + err);
            }
        }

        const char *muxer = container_for(path);
        AVFormatContext *fmt = nullptr;
        int rc = avformat_alloc_output_context2(&fmt, nullptr, muxer, path.c_str());
        if (rc < 0 || !fmt)
        {
            if (aac)
                avcodec_free_context(&aac);
            return fail(std::string("Could not create the ") + muxer + " muxer: " + av_err(rc));
        }
        bool io_open = false;
        auto cleanup = [&](bool remove_file)
        {
            if (io_open)
                avio_closep(&fmt->pb);
            avformat_free_context(fmt);
            if (aac)
                avcodec_free_context(&aac);
            if (remove_file)
                DeleteFileW(utf8_to_wide(path).c_str());
        };

        AVStream *vst = avformat_new_stream(fmt, nullptr);
        if (!vst)
        {
            cleanup(false);
            return fail("out of memory");
        }
        vst->time_base = {1, 90000};
        vst->avg_frame_rate = {fps, 1};
        AVCodecParameters *par = vst->codecpar;
        par->codec_type = AVMEDIA_TYPE_VIDEO;
        par->codec_id = params_are_hevc(extradata) ? AV_CODEC_ID_HEVC : AV_CODEC_ID_H264;
        par->format = AV_PIX_FMT_YUV420P;
        par->width = w;
        par->height = h;
        const bool highdef = w >= 1280 || h > 576;
        par->color_range = AVCOL_RANGE_MPEG;
        par->color_space = highdef ? AVCOL_SPC_BT709 : AVCOL_SPC_SMPTE170M;
        par->color_primaries = highdef ? AVCOL_PRI_BT709 : AVCOL_PRI_SMPTE170M;
        par->color_trc = highdef ? AVCOL_TRC_BT709 : AVCOL_TRC_SMPTE170M;
        // Owned by the stream: avformat_free_context() in cleanup() releases it via av_freep.
        par->extradata = static_cast<uint8_t *>(av_mallocz(extradata.size() + AV_INPUT_BUFFER_PADDING_SIZE));
        if (!par->extradata)
        {
            cleanup(false);
            return fail("out of memory");
        }
        std::memcpy(par->extradata, extradata.data(), extradata.size());
        par->extradata_size = (int)extradata.size();

        AVStream *ast = nullptr;
        if (aac)
        {
            ast = avformat_new_stream(fmt, nullptr);
            if (!ast)
            {
                cleanup(false);
                return fail("out of memory");
            }
            ast->time_base = {1, (int)AUDIO_RATE};
            rc = avcodec_parameters_from_context(ast->codecpar, aac);
            if (rc < 0)
            {
                cleanup(false);
                return fail(av_err(rc));
            }
        }

        if (!(fmt->oformat->flags & AVFMT_NOFILE))
        {
            rc = avio_open(&fmt->pb, path.c_str(), AVIO_FLAG_WRITE);
            if (rc < 0)
            {
                cleanup(false);
                return fail("Could not open '" + path + "': " + av_err(rc));
            }
            io_open = true;
        }
        AVDictionary *opts = nullptr;
        if (std::strcmp(muxer, "mp4") == 0 || std::strcmp(muxer, "mov") == 0)
            av_dict_set(&opts, "movflags", "+faststart", 0);
        rc = avformat_write_header(fmt, &opts);
        av_dict_free(&opts);
        if (rc < 0)
        {
            cleanup(true);
            return fail("Writing the container header failed: " + av_err(rc));
        }

        // Monotonic timestamps in the video stream's time base; equal/decreasing input PTS are bumped.
        struct VideoTs
        {
            int64_t pts, duration;
        };
        std::vector<VideoTs> video_ts(count);
        int64_t previous = INT64_MIN;
        for (size_t i = 0; i < count; ++i)
        {
            const int64_t rel = std::max<int64_t>(0, std::max(s[i].pts100ns, first_pts) - first_pts);
            int64_t pts = av_rescale_q(rel, kTb100ns, vst->time_base);
            if (pts <= previous)
                pts = previous + 1;
            const int64_t next_rel = i + 1 < count
                                         ? std::max<int64_t>(0, std::max(s[i + 1].pts100ns, first_pts) - first_pts)
                                         : std::max(last_end - first_pts, rel + 1);
            const int64_t duration = std::max<int64_t>(1, av_rescale_q(next_rel, kTb100ns, vst->time_base) - pts);
            previous = pts;
            video_ts[i] = {pts, duration};
        }
        struct Item
        {
            AVPacket *pkt;
            int64_t sort;
        };
        std::vector<Item> items;
        auto free_items = [&]()
        {
            for (auto &it : items)
                av_packet_free(&it.pkt);
            items.clear();
        };
        for (size_t i = 0; i < count; ++i)
        {
            AVPacket *pkt = av_packet_alloc();
            if (!pkt)
            {
                free_items();
                cleanup(true);
                return fail("out of memory");
            }
            int alloc_rc = av_new_packet(pkt, (int)s[i].size);
            if (alloc_rc < 0)
            {
                av_packet_free(&pkt);
                free_items();
                cleanup(true);
                return fail("Could not allocate a video packet: " + av_err(alloc_rc));
            }
            std::memcpy(pkt->data, s[i].data, s[i].size);
            pkt->pts = pkt->dts = video_ts[i].pts;
            pkt->duration = video_ts[i].duration;
            pkt->stream_index = vst->index;
            if (s[i].keyframe)
                pkt->flags |= AV_PKT_FLAG_KEY;
            items.push_back({pkt, video_ts[i].pts * 1000000 / vst->time_base.den});
        }
        if (ast)
        {
            for (AVPacket *&p : packets.list)
            {
                p->stream_index = ast->index;
                items.push_back({p, p->pts * 1000000 / (int64_t)AUDIO_RATE});
                p = nullptr; // ownership moves to `items`
            }
            packets.list.clear();
        }
        std::stable_sort(items.begin(), items.end(), [](const Item &x, const Item &y)
                         { return x.sort < y.sort; });
        int result = IR_OK;
        for (auto &it : items)
        {
            if (result == IR_OK)
            {
                rc = av_interleaved_write_frame(fmt, it.pkt);
                if (rc < 0)
                {
                    set_last_error("Writing a packet failed: " + av_err(rc));
                    result = IR_FAIL;
                }
            }
            av_packet_free(&it.pkt);
        }
        if (result == IR_OK)
        {
            rc = av_write_trailer(fmt);
            if (rc < 0)
            {
                set_last_error("Finalizing the file failed: " + av_err(rc));
                result = IR_FAIL;
            }
        }
        cleanup(result != IR_OK);
        return result;
    }

    int mux_until(const std::string &utf8_path, const std::vector<Ref<PacketRef>> &video,
                  const std::vector<AudioChunk> &audio, int fps, int w, int h, int seconds_back, int64_t end_time)
    {
        // Each PacketRef is already exactly one contiguous muxer sample; project it into a
        // VideoSample view so byte count, ordering and duration handling stay identical.
        std::vector<VideoSample> views;
        views.reserve(video.size());
        for (const auto &p : video)
        {
            if (!p)
                continue;
            views.push_back({p->payload.data(), p->payload.size(), p->keyframe, p->pts100ns});
        }
        return mux_until(utf8_path, views, audio, fps, w, h, seconds_back, end_time);
    }

} // namespace ir
