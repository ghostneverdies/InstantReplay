#include "mf_encoder.h"

#include <algorithm>
#include <chrono>
#include <codecapi.h>
#include <cwctype>
#include <mferror.h>
#include <mfapi.h>
#include <mfidl.h>
#include <mftransform.h>
#include <strmif.h>

#ifdef __MINGW32__
// Only for compile-checking with MinGW, whose headers predate these Windows 10 SDK declarations.
// MSVC gets the real ones from <mfapi.h>.
static const GUID MFT_ENUM_ADAPTER_LUID = {0x1d39518c, 0xe220, 0x4da8, {0xa0, 0x7f, 0xba, 0x17, 0x25, 0x52, 0xd6, 0xb1}};
extern "C" HRESULT WINAPI MFTEnum2(GUID, UINT32, const MFT_REGISTER_TYPE_INFO*, const MFT_REGISTER_TYPE_INFO*,
                                    IMFAttributes*, IMFActivate***, UINT32*);
#endif

namespace ir {

namespace {

// Friendly-name attributes on IMFActivate. Neither is declared by the SDK headers we build against.
// kMftName aliases MFT_FRIENDLY_NAME_Attribute, the wide name hardware MFTs actually publish.
static const GUID kMftFriendlyNameUtf8 = {0x314ff923, 0x2c6d, 0x4cd2, {0x8d, 0x32, 0x20, 0xe0, 0xa3, 0xe0, 0xd6, 0x8e}};
static const GUID kMftName = {0x314ffbae, 0x5b41, 0x4c95, {0x9c, 0x19, 0x4e, 0x7d, 0x58, 0x6f, 0xac, 0xe3}};

struct MfRuntime {
    HRESULT com_hr = E_FAIL;
    HRESULT mf_hr = E_FAIL;
    bool com = false; // true only when we own a COM reference and must CoUninitialize
    bool mf = false;
    MfRuntime() {
        com_hr = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
        if (com_hr == RPC_E_CHANGED_MODE) {
            // FFmpeg's QSV/NVENC probes may put the calling thread in STA before us. MF works in
            // STA too, so do not fail - just remember that we do not own this apartment.
            com_hr = S_FALSE;
        } else {
            com = SUCCEEDED(com_hr); // S_OK / S_FALSE both leave COM initialized on this thread
        }
        if (SUCCEEDED(com_hr)) {
            mf_hr = MFStartup(MF_VERSION, MFSTARTUP_FULL);
            mf = SUCCEEDED(mf_hr);
        }
    }
    ~MfRuntime() {
        if (mf) MFShutdown();
        if (com) CoUninitialize();
    }
};
MfRuntime& mf_runtime() {
    // COM apartments and MFStartup are per-thread: probe and encode may run on different threads.
    static thread_local MfRuntime rt;
    return rt;
}

HRESULT set_u32(ICodecAPI* api, const GUID& key, uint32_t value) {
    VARIANT v;
    VariantInit(&v);
    v.vt = VT_UI4;
    v.ulVal = value;
    return api->SetValue(&key, &v);
}
HRESULT set_bool(ICodecAPI* api, const GUID& key, bool value) {
    VARIANT v;
    VariantInit(&v);
    v.vt = VT_BOOL;
    v.boolVal = value ? VARIANT_TRUE : VARIANT_FALSE;
    return api->SetValue(&key, &v);
}

std::wstring activate_name(IMFActivate* act) {
    wchar_t buf[256] = {};
    UINT32 len = 0;
    if (SUCCEEDED(act->GetString(kMftFriendlyNameUtf8, buf, 256, &len)) && len) return std::wstring(buf, len);
    if (SUCCEEDED(act->GetString(kMftName, buf, 256, &len)) && len) return std::wstring(buf, len);
    return std::wstring();
}

// Intel/Arc/QSV first, NVIDIA last: NVIDIA's hybrid MFT is broken and must never win over QSV.
int activate_rank(IMFActivate* act) {
    const std::wstring name = activate_name(act);
    std::wstring lower(name.size(), L'\0');
    for (size_t i = 0; i < name.size(); ++i) lower[i] = (wchar_t)towlower(name[i]);
    auto has = [&](const wchar_t* s) { return lower.find(s) != std::wstring::npos; };
    if (has(L"intel") || has(L"arc") || has(L"iris") || has(L"qsv") || has(L"quick sync")) return 2;
    if (has(L"nvidia") || has(L"nvenc")) return 0;
    return 1;
}
void rank_activates(IMFActivate** list, UINT32 count) {
    if (!list || count < 2) return;
    std::stable_sort(list, list + count, [](IMFActivate* a, IMFActivate* b) {
        return activate_rank(a) > activate_rank(b);
    });
}

struct NalRange { size_t start, end; };
std::vector<NalRange> annexb_units(const uint8_t* d, size_t n) {
    std::vector<NalRange> out;
    if (!d) return out;
    size_t i = 0;
    long cur = -1;
    while (i + 3 <= n) {
        if (d[i] == 0 && d[i + 1] == 0 && d[i + 2] == 1) {
            if (cur >= 0) {
                size_t e = i;
                while (e > (size_t)cur && d[e - 1] == 0) --e;
                out.push_back({(size_t)cur, e});
            }
            cur = (long)(i + 3);
            i += 3;
        } else {
            ++i;
        }
    }
    if (cur >= 0 && (size_t)cur < n) out.push_back({(size_t)cur, n});
    return out;
}
// Never prepend a partial header: require AVC SPS+PPS or HEVC VPS+SPS+PPS.
bool has_complete_param_sets(const uint8_t* d, size_t n, bool hevc) {
    bool vps = false, sps = false, pps = false;
    for (const auto& u : annexb_units(d, n)) {
        if (u.end <= u.start || u.start >= n) continue;
        const uint8_t b = d[u.start];
        if (hevc) {
            const int t = (b >> 1) & 0x3f;
            if (t == 32) vps = true;
            else if (t == 33) sps = true;
            else if (t == 34) pps = true;
        } else {
            const int t = b & 0x1f;
            if (t == 7) sps = true;
            else if (t == 8) pps = true;
        }
    }
    return hevc ? (vps && sps && pps) : (sps && pps);
}
bool has_key_nal(const uint8_t* d, size_t n, bool hevc) {
    for (const auto& u : annexb_units(d, n)) {
        if (u.end <= u.start || u.start >= n) continue;
        const uint8_t b = d[u.start];
        const int t = hevc ? ((b >> 1) & 0x3f) : (b & 0x1f);
        if (hevc ? (t >= 16 && t <= 21) : (t == 5)) return true;
    }
    return false;
}

std::vector<uint8_t> read_seq_header(IMFTransform* mft) {
    std::vector<uint8_t> header;
    ComPtr<IMFMediaType> cur;
    if (FAILED(mft->GetOutputCurrentType(0, &cur)) || !cur) return header;
    UINT32 size = 0;
    if (FAILED(cur->GetBlobSize(MF_MT_MPEG_SEQUENCE_HEADER, &size)) || size <= 4) return header;
    header.resize(size);
    UINT32 written = 0;
    if (FAILED(cur->GetBlob(MF_MT_MPEG_SEQUENCE_HEADER, header.data(), size, &written)) || written <= 4) {
        header.clear();
        return header;
    }
    header.resize(written);
    const bool sc3 = header[0] == 0 && header[1] == 0 && header[2] == 1;
    const bool sc4 = header[0] == 0 && header[1] == 0 && header[2] == 0 && header[3] == 1;
    if (!sc3 && !sc4) header.clear();
    return header;
}

} // namespace

struct MfEncoder::Impl {
    ComPtr<IMFTransform> mft;
    ComPtr<ICodecAPI> api;
    ComPtr<IMFMediaEventGenerator> events;
    bool async = false;
    bool provides_samples = false;
    DWORD out_size = 0;
    int w = 0, h = 0;
    int pending_need = 0; // async: METransformNeedInput events not yet answered
    LONGLONG frame_dur = SECOND_100NS / 60;
    std::vector<uint8_t> seq_header;
    bool wrote_header = false;
    bool hevc = false;
};

MfEncoder::~MfEncoder() {
    if (p_ && p_->mft) {
        p_->mft->ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
    }
}

static HRESULT enumerate_hardware(MfCodec codec, const GpuDevice* gpu, IMFActivate*** list, UINT32* count) {
    (void)gpu;
    MFT_REGISTER_TYPE_INFO in_type = {MFMediaType_Video, MFVideoFormat_NV12};
    MFT_REGISTER_TYPE_INFO out_type = {MFMediaType_Video, codec == MfCodec::Hevc ? MFVideoFormat_HEVC : MFVideoFormat_H264};
    const UINT32 flags = MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER;
    // Do NOT filter by the D3D device's LUID. On hybrid laptops/desktops the primary adapter is
    // often the discrete GPU; a LUID-filtered enumeration then returns only that vendor's MFT and
    // hides the Intel Quick Sync MFT we actually want. Enumerate every hardware MFT and let
    // rank_activates() put Intel/Arc above NVIDIA; create() then tries them in that order, so a
    // broken candidate (e.g. NVIDIA's on a hybrid machine) simply fails and the next one is tried.
    HRESULT hr = MFTEnumEx(MFT_CATEGORY_VIDEO_ENCODER, flags, &in_type, &out_type, list, count);
    if (SUCCEEDED(hr) && *list) rank_activates(*list, *count);
    return hr;
}

std::unique_ptr<MfEncoder> MfEncoder::create(MfCodec codec, const GpuDevice* gpu, int w, int h, int fps,
                                              uint32_t bps, Rate rate, Preset preset, std::string* err) {
    auto fail = [&](const char* what, HRESULT hr) -> std::unique_ptr<MfEncoder> {
        char buf[160];
        std::snprintf(buf, sizeof(buf), "Media Foundation %s failed (0x%08X)", what, (unsigned)hr);
        if (err) *err = buf;
        return nullptr;
    };
    MfRuntime& rt = mf_runtime();
    if (FAILED(rt.com_hr)) return fail("COM initialization", rt.com_hr); // e.g. RPC_E_CHANGED_MODE
    if (FAILED(rt.mf_hr)) return fail("startup", rt.mf_hr);
    w &= ~1;
    h &= ~1;
    if (w < 2 || h < 2) return fail("output size", E_INVALIDARG);

    IMFActivate** list = nullptr;
    UINT32 count = 0;
    HRESULT hr = enumerate_hardware(codec, gpu, &list, &count);
    if (FAILED(hr) || count == 0) {
        if (list) CoTaskMemFree(list);
        return fail("hardware encoder enumeration", FAILED(hr) ? hr : MF_E_TOPO_CODEC_NOT_FOUND);
    }

    auto enc = std::unique_ptr<MfEncoder>(new MfEncoder());
    enc->codec_ = codec;
    HRESULT last = E_FAIL;
    bool created = false;
    for (UINT32 i = 0; i < count && !created; ++i) {
        auto impl = std::make_unique<Impl>();
        impl->w = w;
        impl->h = h;
        impl->hevc = codec == MfCodec::Hevc;
        impl->frame_dur = SECOND_100NS / std::max(1, fps);
        hr = list[i]->ActivateObject(IID_PPV_ARGS(&impl->mft));
        if (FAILED(hr) || !impl->mft) { last = FAILED(hr) ? hr : E_NOINTERFACE; continue; }

        ComPtr<IMFAttributes> attrs;
        if (SUCCEEDED(impl->mft->GetAttributes(&attrs)) && attrs) {
            UINT32 is_async = 0;
            if (SUCCEEDED(attrs->GetUINT32(MF_TRANSFORM_ASYNC, &is_async)) && is_async) {
                impl->async = true;
                attrs->SetUINT32(MF_TRANSFORM_ASYNC_UNLOCK, TRUE);
                impl->mft.As(&impl->events);
            }
            attrs->SetUINT32(MF_LOW_LATENCY, TRUE);
        }
        if (impl->async && !impl->events) { last = E_NOINTERFACE; continue; }

        // Do NOT attach a DXGI device manager here. The engine always feeds the MFT system-memory
        // NV12 via encode_memory(), so no manager is required. Worse, on hybrid systems the D3D
        // device belongs to the primary (often NVIDIA) adapter: MFT_MESSAGE_SET_D3D_MANAGER on an
        // Intel MFT then fails with E_INVALIDARG and leaves the transform unusable, so creation
        // falls through to the broken NVIDIA MFT - exactly the FFmpeg mfenc bug we are avoiding.
        // Keeping the MFT in system-memory mode lets the Intel candidate configure and encode.
        (void)gpu;

        impl->mft.As(&impl->api);
        if (impl->api) {
            set_u32(impl->api.Get(), CODECAPI_AVEncCommonRateControlMode,
                    rate == Rate::Cbr ? eAVEncCommonRateControlMode_CBR : eAVEncCommonRateControlMode_PeakConstrainedVBR);
            set_u32(impl->api.Get(), CODECAPI_AVEncCommonMeanBitRate, bps);
            if (rate == Rate::Vbr) set_u32(impl->api.Get(), CODECAPI_AVEncCommonMaxBitRate, (uint32_t)((uint64_t)bps * 3 / 2));
            set_u32(impl->api.Get(), CODECAPI_AVEncMPVGOPSize, (uint32_t)(fps * 2));
            set_u32(impl->api.Get(), CODECAPI_AVEncMPVDefaultBPictureCount, 0);
            set_bool(impl->api.Get(), CODECAPI_AVLowLatencyMode, true);
            set_u32(impl->api.Get(), CODECAPI_AVEncCommonQualityVsSpeed,
                    preset == Preset::Speed ? 20 : preset == Preset::Quality ? 100 : 60);
        }

        const bool hd = w >= 1280 || h > 576;
        ComPtr<IMFMediaType> out_t;
        if (FAILED(MFCreateMediaType(&out_t)) || !out_t) { last = E_OUTOFMEMORY; continue; }
        out_t->SetGUID(MF_MT_MAJOR_TYPE, MFMediaType_Video);
        out_t->SetGUID(MF_MT_SUBTYPE, codec == MfCodec::Hevc ? MFVideoFormat_HEVC : MFVideoFormat_H264);
        out_t->SetUINT32(MF_MT_AVG_BITRATE, bps);
        MFSetAttributeSize(out_t.Get(), MF_MT_FRAME_SIZE, w, h);
        MFSetAttributeRatio(out_t.Get(), MF_MT_FRAME_RATE, fps, 1);
        MFSetAttributeRatio(out_t.Get(), MF_MT_PIXEL_ASPECT_RATIO, 1, 1);
        out_t->SetUINT32(MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive);
        out_t->SetUINT32(MF_MT_MPEG2_PROFILE, codec == MfCodec::Hevc ? (UINT32)eAVEncH265VProfile_Main_420_8 : (UINT32)eAVEncH264VProfile_High);
        out_t->SetUINT32(MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235);
        out_t->SetUINT32(MF_MT_YUV_MATRIX, hd ? MFVideoTransferMatrix_BT709 : MFVideoTransferMatrix_BT601);
        out_t->SetUINT32(MF_MT_VIDEO_PRIMARIES, hd ? MFVideoPrimaries_BT709 : MFVideoPrimaries_SMPTE170M);
        out_t->SetUINT32(MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709);
        hr = impl->mft->SetOutputType(0, out_t.Get(), 0);
        if (FAILED(hr)) { last = hr; continue; }

        ComPtr<IMFMediaType> in_t;
        if (FAILED(MFCreateMediaType(&in_t)) || !in_t) { last = E_OUTOFMEMORY; continue; }
        in_t->SetGUID(MF_MT_MAJOR_TYPE, MFMediaType_Video);
        in_t->SetGUID(MF_MT_SUBTYPE, MFVideoFormat_NV12);
        MFSetAttributeSize(in_t.Get(), MF_MT_FRAME_SIZE, w, h);
        MFSetAttributeRatio(in_t.Get(), MF_MT_FRAME_RATE, fps, 1);
        MFSetAttributeRatio(in_t.Get(), MF_MT_PIXEL_ASPECT_RATIO, 1, 1);
        in_t->SetUINT32(MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive);
        in_t->SetUINT32(MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235);
        in_t->SetUINT32(MF_MT_YUV_MATRIX, hd ? MFVideoTransferMatrix_BT709 : MFVideoTransferMatrix_BT601);
        in_t->SetUINT32(MF_MT_VIDEO_PRIMARIES, hd ? MFVideoPrimaries_BT709 : MFVideoPrimaries_SMPTE170M);
        in_t->SetUINT32(MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709);
        hr = impl->mft->SetInputType(0, in_t.Get(), 0);
        if (FAILED(hr)) { last = hr; continue; }

        MFT_OUTPUT_STREAM_INFO info = {};
        if (SUCCEEDED(impl->mft->GetOutputStreamInfo(0, &info))) {
            impl->provides_samples = (info.dwFlags & (MFT_OUTPUT_STREAM_PROVIDES_SAMPLES | MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES)) != 0;
            impl->out_size = std::max<DWORD>(info.cbSize, 1u << 20);
        } else {
            impl->out_size = std::max<DWORD>((DWORD)(w * h), 1u << 20);
        }

        impl->seq_header = read_seq_header(impl->mft.Get());

        impl->mft->ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0);
        impl->mft->ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0);
        impl->mft->ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0);
        enc->p_ = std::move(impl);
        // No DXGI device manager is attached, so GPU-texture input is intentionally unavailable;
        // the engine always feeds system-memory NV12 via encode_memory().
        enc->texture_ok_ = false;
        created = true;
    }
    for (UINT32 i = 0; i < count; ++i) list[i]->Release();
    CoTaskMemFree(list);
    if (!created) {
        enc->init_hr_ = last;
        return fail("encoder setup", last);
    }
    return enc;
}

// ---- encoding ----

static bool process_output(MfEncoder::Impl& p, std::vector<EncodedFrame>& out, std::string* err, bool* need_more) {
    MFT_OUTPUT_DATA_BUFFER buf = {};
    ComPtr<IMFSample> own;
    if (!p.provides_samples) {
        ComPtr<IMFMediaBuffer> mb;
        if (FAILED(MFCreateSample(&own)) || !own || FAILED(MFCreateMemoryBuffer(p.out_size, &mb)) || !mb) {
            if (err) *err = "Media Foundation could not allocate an output sample";
            return false;
        }
        own->AddBuffer(mb.Get());
        buf.pSample = own.Get();
    }
    DWORD status = 0;
    HRESULT hr = p.mft->ProcessOutput(0, 1, &buf, &status);
    if (need_more) *need_more = false;
    if (hr == MF_E_TRANSFORM_NEED_MORE_INPUT) {
        if (need_more) *need_more = true;
        if (buf.pEvents) buf.pEvents->Release();
        return true;
    }
    if (hr == MF_E_TRANSFORM_STREAM_CHANGE) {
        // Pick a replacement type that still matches what we configured, not blindly index 0.
        ComPtr<IMFMediaType> chosen;
        const GUID want = p.hevc ? MFVideoFormat_HEVC : MFVideoFormat_H264;
        for (UINT32 idx = 0;; ++idx) {
            ComPtr<IMFMediaType> t;
            HRESULT th = p.mft->GetOutputAvailableType(0, idx, &t);
            if (FAILED(th) || !t) break;
            GUID subtype = {};
            UINT32 tw = 0, thh = 0;
            if (FAILED(t->GetGUID(MF_MT_SUBTYPE, &subtype)) || subtype != want) continue;
            if (FAILED(MFGetAttributeSize(t.Get(), MF_MT_FRAME_SIZE, &tw, &thh))) continue;
            if (tw != (UINT32)p.w || thh != (UINT32)p.h) continue;
            chosen = t;
            break;
        }
        if (chosen && SUCCEEDED(p.mft->SetOutputType(0, chosen.Get(), 0))) {
            p.seq_header = read_seq_header(p.mft.Get());
            p.wrote_header = false;
        }
        if (buf.pEvents) buf.pEvents->Release();
        return true;
    }
    if (FAILED(hr)) {
        if (buf.pEvents) buf.pEvents->Release();
        char m[96];
        std::snprintf(m, sizeof(m), "Media Foundation ProcessOutput failed (0x%08X)", (unsigned)hr);
        if (err) *err = m;
        return false;
    }
    IMFSample* sample = buf.pSample;
    if (sample) {
        ComPtr<IMFMediaBuffer> mb;
        if (SUCCEEDED(sample->ConvertToContiguousBuffer(&mb)) && mb) {
            BYTE* data = nullptr;
            DWORD len = 0;
            if (SUCCEEDED(mb->Lock(&data, nullptr, &len)) && data && len > 0) {
                EncodedFrame f;
                const bool key_nal = has_key_nal(data, len, p.hevc);
                UINT32 clean = 0;
                f.keyframe = key_nal || (SUCCEEDED(sample->GetUINT32(MFSampleExtension_CleanPoint, &clean)) && clean != 0);
                LONGLONG t = 0;
                sample->GetSampleTime(&t);
                f.pts100ns = t;
                const bool packet_params = has_complete_param_sets(data, len, p.hevc);
                if (!packet_params) {
                    if (p.seq_header.empty()) p.seq_header = read_seq_header(p.mft.Get()); // lazy re-read
                    if (has_complete_param_sets(p.seq_header.data(), p.seq_header.size(), p.hevc) &&
                        (f.keyframe || !p.wrote_header)) {
                        f.data = p.seq_header;
                        f.data.insert(f.data.end(), data, data + len);
                    } else {
                        f.data.assign(data, data + len);
                    }
                } else {
                    f.data.assign(data, data + len);
                }
                if (f.keyframe || packet_params) p.wrote_header = true;
                out.push_back(std::move(f));
                mb->Unlock();
            }
        }
    }
    if (p.provides_samples && buf.pSample) buf.pSample->Release();
    if (buf.pEvents) buf.pEvents->Release();
    return true;
}

// Async MFTs: pump queued events without blocking. Sync MFTs: pull output until more input is needed.
static bool pump(MfEncoder::Impl& p, std::vector<EncodedFrame>& out, std::string* err, bool wait_for_need) {
    if (!p.async) {
        for (;;) {
            bool need = false;
            size_t before = out.size();
            if (!process_output(p, out, err, &need)) return false;
            if (need || out.size() == before) return true;
        }
    }
    // A stalled MFT must never block this thread: poll with NO_WAIT and a bounded deadline.
    const auto deadline = std::chrono::steady_clock::now() + std::chrono::milliseconds(200);
    for (;;) {
        if (wait_for_need && p.pending_need == 0 && std::chrono::steady_clock::now() >= deadline) {
            if (err) *err = "Media Foundation encoder timed out waiting for input";
            return false;
        }
        ComPtr<IMFMediaEvent> ev;
        HRESULT hr = p.events->GetEvent(MF_EVENT_FLAG_NO_WAIT, &ev);
        if (hr == MF_E_NO_EVENTS_AVAILABLE) {
            if (wait_for_need && p.pending_need == 0) {
                if (std::chrono::steady_clock::now() >= deadline) {
                    if (err) *err = "Media Foundation encoder timed out waiting for input";
                    return false;
                }
                Sleep(1);
                continue;
            }
            return true;
        }
        if (FAILED(hr)) {
            char m[96];
            std::snprintf(m, sizeof(m), "Media Foundation event queue failed (0x%08X)", (unsigned)hr);
            if (err) *err = m;
            return false;
        }
        MediaEventType type = MEUnknown;
        ev->GetType(&type);
        if (type == METransformNeedInput) {
            ++p.pending_need;
            if (wait_for_need) return true;
        } else if (type == METransformHaveOutput) {
            if (!process_output(p, out, err, nullptr)) return false;
        } else if (type == METransformDrainComplete) {
            return true;
        } else if (type == MEError) {
            HRESULT status = S_OK;
            ev->GetStatus(&status);
            char m[96];
            std::snprintf(m, sizeof(m), "Media Foundation encoder error (0x%08X)", (unsigned)status);
            if (err) *err = m;
            return false;
        }
    }
}

static bool feed(MfEncoder::Impl& p, IMFSample* sample, int64_t pts, bool idr, std::vector<EncodedFrame>& out,
                 std::string* err) {
    sample->SetSampleTime(pts);
    if (idr && p.api) set_u32(p.api.Get(), CODECAPI_AVEncVideoForceKeyFrame, 1);
    if (p.async) {
        while (p.pending_need == 0) { // output events can arrive before NeedInput; keep waiting
            if (!pump(p, out, err, true)) return false;
        }
    }
    HRESULT hr = p.mft->ProcessInput(0, sample, 0);
    if (hr == MF_E_NOTACCEPTING && !p.async) {
        if (!pump(p, out, err, false)) return false;
        hr = p.mft->ProcessInput(0, sample, 0);
    }
    if (FAILED(hr)) {
        char m[96];
        std::snprintf(m, sizeof(m), "Media Foundation ProcessInput failed (0x%08X)", (unsigned)hr);
        if (err) *err = m;
        return false;
    }
    if (p.async && p.pending_need > 0) --p.pending_need;
    return pump(p, out, err, false);
}

bool MfEncoder::encode_texture(ID3D11Texture2D* nv12, int64_t pts, bool idr, std::vector<EncodedFrame>& out,
                                std::string* err) {
    ComPtr<IMFMediaBuffer> mb;
    HRESULT hr = MFCreateDXGISurfaceBuffer(__uuidof(ID3D11Texture2D), nv12, 0, FALSE, &mb);
    if (FAILED(hr) || !mb) {
        if (err) *err = "MFCreateDXGISurfaceBuffer failed";
        return false;
    }
    mb->SetCurrentLength(p_->w * p_->h * 3 / 2);
    ComPtr<IMFSample> sample;
    if (FAILED(MFCreateSample(&sample)) || !sample) {
        if (err) *err = "Media Foundation could not allocate an input sample";
        return false;
    }
    sample->AddBuffer(mb.Get());
    sample->SetSampleDuration(p_->frame_dur);
    return feed(*p_, sample.Get(), pts, idr, out, err);
}

bool MfEncoder::encode_memory(const uint8_t* y, size_t ys, const uint8_t* uv, size_t uvs, int64_t pts, bool idr,
                               std::vector<EncodedFrame>& out, std::string* err) {
    const size_t w = p_->w, h = p_->h;
    if (!y || !uv || w == 0 || h < 2 || ys < w || uvs < w) { // caller passes strides; buffer lengths are not in this API
        if (err) *err = "Media Foundation input planes or strides are invalid";
        return false;
    }
    ComPtr<IMFMediaBuffer> mb;
    if (FAILED(MFCreateMemoryBuffer((DWORD)(w * h * 3 / 2), &mb)) || !mb) {
        if (err) *err = "Media Foundation could not allocate an input buffer";
        return false;
    }
    BYTE* dst = nullptr;
    if (FAILED(mb->Lock(&dst, nullptr, nullptr)) || !dst) {
        if (err) *err = "Media Foundation input buffer lock failed";
        return false;
    }
    for (size_t r = 0; r < h; ++r) std::memcpy(dst + r * w, y + r * ys, w);
    for (size_t r = 0; r < h / 2; ++r) std::memcpy(dst + w * h + r * w, uv + r * uvs, w);
    mb->Unlock();
    mb->SetCurrentLength((DWORD)(w * h * 3 / 2));
    ComPtr<IMFSample> sample;
    if (FAILED(MFCreateSample(&sample)) || !sample) {
        if (err) *err = "Media Foundation could not allocate an input sample";
        return false;
    }
    sample->AddBuffer(mb.Get());
    sample->SetSampleDuration(p_->frame_dur);
    return feed(*p_, sample.Get(), pts, idr, out, err);
}

bool MfEncoder::flush(std::vector<EncodedFrame>& out, std::string* err) {
    Impl& p = *p_;
    HRESULT hr = p.mft->ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
    if (FAILED(hr)) {
        char m[96];
        std::snprintf(m, sizeof(m), "Media Foundation end-of-stream failed (0x%08X)", (unsigned)hr);
        if (err) *err = m;
        return false;
    }
    hr = p.mft->ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0);
    if (FAILED(hr)) {
        char m[96];
        std::snprintf(m, sizeof(m), "Media Foundation drain failed (0x%08X)", (unsigned)hr);
        if (err) *err = m;
        return false;
    }
    if (!p.async) return pump(p, out, err, false);
    for (int i = 0; i < 400; ++i) { // ~2 s ceiling
        ComPtr<IMFMediaEvent> ev;
        HRESULT ehr = p.events->GetEvent(MF_EVENT_FLAG_NO_WAIT, &ev);
        if (ehr == MF_E_NO_EVENTS_AVAILABLE) { Sleep(5); continue; }
        if (FAILED(ehr)) return true;
        MediaEventType type = MEUnknown;
        ev->GetType(&type);
        if (type == METransformHaveOutput) {
            if (!process_output(p, out, err, nullptr)) return false;
        } else if (type == METransformDrainComplete) {
            return true;
        }
    }
    return true;
}

bool probe_mf_encoder(MfCodec codec, int w, int h, int fps, uint32_t bps, HRESULT* hr_out, std::string* err_out) {
    if (hr_out) *hr_out = S_OK;
    std::string err;
    auto report = [&]() { if (err_out) *err_out = err; return false; };
    GpuDevice gpu;
    HRESULT hr = create_gpu_device(nullptr, &gpu);
    if (FAILED(hr)) {
        if (hr_out) *hr_out = hr;
        err = "could not create a D3D device";
        return report();
    }
    auto enc = MfEncoder::create(codec, &gpu, w, h, fps, bps, Rate::Cbr, Preset::Balanced, &err);
    if (!enc) {
        if (hr_out) *hr_out = E_FAIL;
        return report();
    }
    w &= ~1;
    h &= ~1;
    std::vector<uint8_t> yp((size_t)w * h, 16), uvp((size_t)w * h / 2, 128);
    std::vector<EncodedFrame> out;
    const int64_t dur = SECOND_100NS / std::max(1, fps);
    for (int i = 0; i < 12; ++i) {
        if (!enc->encode_memory(yp.data(), w, uvp.data(), w, i * dur, i == 0, out, &err)) return report();
        for (auto& f : out) if (!f.data.empty()) return true;
        Sleep(5);
    }
    if (!enc->flush(out, &err)) return report();
    for (auto& f : out) if (!f.data.empty()) return true;
    err = "hardware MFT produced no encoded output";
    return report();
}

} // namespace ir
