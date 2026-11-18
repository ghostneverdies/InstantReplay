#include "ffmpeg_rt.h"

#include <cstdio>
#include <string>

extern "C" {
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/channel_layout.h>
#include <libavutil/dict.h>
#include <libavutil/error.h>
#include <libavutil/frame.h>
#include <libavutil/mem.h>
#include <libavutil/opt.h>
#include <libavutil/rational.h>
}

namespace ir {

namespace {

const char* const kLibs[] = {"avutil-57.dll", "swresample-4.dll", "avcodec-59.dll", "avformat-59.dll"};
std::mutex g_load_mutex;
bool g_loaded = false;

std::wstring module_dir() {
    HMODULE self = nullptr;
    GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                       reinterpret_cast<LPCWSTR>(&module_dir), &self);
    wchar_t buf[1024];
    DWORD n = GetModuleFileNameW(self, buf, 1024);
    if (n == 0 || n >= 1024) return L"";
    std::wstring p(buf, n);
    size_t slash = p.find_last_of(L"\\/");
    return slash == std::wstring::npos ? L"" : p.substr(0, slash);
}

bool file_exists(const std::wstring& p) {
    DWORD a = GetFileAttributesW(p.c_str());
    return a != INVALID_FILE_ATTRIBUTES && !(a & FILE_ATTRIBUTE_DIRECTORY);
}
bool dir_exists(const std::wstring& p) {
    DWORD a = GetFileAttributesW(p.c_str());
    return a != INVALID_FILE_ATTRIBUTES && (a & FILE_ATTRIBUTE_DIRECTORY);
}

std::wstring encoder_dir() {
    wchar_t env[1024];
    DWORD n = GetEnvironmentVariableW(L"IR_FFMPEG_DIR", env, 1024);
    if (n > 0 && n < 1024) return std::wstring(env, n);
    std::wstring dir = module_dir();
    if (dir.empty()) return L"encoder";
    size_t slash = dir.find_last_of(L"\\/");
    if (slash != std::wstring::npos) {
        std::wstring sibling = dir.substr(0, slash) + L"\\encoder";
        if (dir_exists(sibling)) return sibling;
    }
    std::wstring flat = dir + L"\\encoder";
    if (dir_exists(flat)) return flat;
    return slash != std::wstring::npos ? dir.substr(0, slash) + L"\\encoder" : flat;
}

std::string av_err(int code) {
    char buf[128] = {};
    av_strerror(code, buf, sizeof(buf));
    return std::string(buf) + " (" + std::to_string(code) + ")";
}

const AVRational kTb100ns = {1, 10'000'000};

bool is_hevc_name(const std::string& n) { return n.rfind("hevc_", 0) == 0 || n == "libx265"; }
bool is_h264_name(const std::string& n) { return n.rfind("h264_", 0) == 0 || n == "libx264"; }

const char* nvenc_preset(Preset p) { return p == Preset::Speed ? "p2" : p == Preset::Quality ? "p6" : "p4"; }
const char* nvenc_rc(Rate r) { return r == Rate::Cbr ? "cbr" : "vbr"; }
const char* amf_preset(Preset p) { return p == Preset::Speed ? "speed" : p == Preset::Quality ? "quality" : "balanced"; }
const char* amf_usage(Preset p) {
    return p == Preset::Speed ? "ultralowlatency" : p == Preset::Quality ? "high_quality" : "transcoding";
}
const char* amf_rc(Rate r) { return r == Rate::Cbr ? "cbr" : "vbr_peak"; }
const char* qsv_preset(Preset p) { return p == Preset::Speed ? "veryfast" : p == Preset::Quality ? "slow" : "medium"; }

using Pairs = std::vector<std::pair<const char*, const char*>>;

Pairs codec_options(const std::string& name, const VideoOptions& o) {
    const bool cbr = o.rate == Rate::Cbr;
    const bool quality = o.preset == Preset::Quality;
    const bool speed = o.preset == Preset::Speed;
    if (name == "hevc_nvenc" || name == "h264_nvenc") {
        Pairs v = {{"preset", nvenc_preset(o.preset)},
                   {"tune", "hq"},
                   {"rc", nvenc_rc(o.rate)},
                   {"profile", name[0] == 'h' && name[1] == 'e' ? "main" : "high"},
                   {"forced-idr", "1"},
                   {"spatial-aq", "1"},
                   {"temporal-aq", quality ? "1" : "0"},
                   {"multipass", quality ? "qres" : "disabled"},
                   {"rc-lookahead", cbr ? "0" : "20"}};
        if (name == "hevc_nvenc") v.push_back({"zerolatency", speed ? "1" : "0"});
        return v;
    }
    if (name == "hevc_amf" || name == "h264_amf") {
        Pairs v = {{"usage", amf_usage(o.preset)},
                   {"quality", amf_preset(o.preset)},
                   {"rc", amf_rc(o.rate)},
                   {"profile", name == "hevc_amf" ? "main" : "high"},
                   {"forced_idr", "1"},
                   {"header_spacing", "1"},
                   {"preanalysis", quality ? "1" : "0"},
                   {"vbaq", "1"},
                   {"enforce_hrd", cbr ? "1" : "0"}};
        if (name == "hevc_amf") v.push_back({"latency", speed ? "1" : "0"});
        return v;
    }
    if (name == "hevc_qsv" || name == "h264_qsv") {
        return {{"preset", qsv_preset(o.preset)},
                {"profile", name == "hevc_qsv" ? "main" : "high"},
                {"forced_idr", "1"},
                {"look_ahead", cbr ? "0" : "1"},
                {"low_delay_brc", cbr ? "1" : "0"}};
    }
    if (name == "libx265") {
        Pairs v = {{"preset", "veryfast"}, {"profile", "main"}, {"forced-idr", "1"}};
        if (cbr) v.push_back({"x265-params", "hrd=1:repeat-headers=1"});
        return v;
    }
    if (name == "libx264") return {{"preset", "veryfast"}, {"profile", "high"}, {"forced-idr", "1"}};
    return {};
}

// ---- Annex-B helpers ----
struct Nal { size_t start, end; };

std::vector<Nal> nal_units(const uint8_t* d, size_t n) {
    std::vector<Nal> out;
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

bool has_param_sets(const uint8_t* d, size_t n) {
    bool vps = false, hevc_sps = false, hevc_pps = false;
    bool avc_sps = false, avc_pps = false;
    const auto units = nal_units(d, n);
    const size_t limit = std::min<size_t>(6, units.size());
    for (size_t i = 0; i < limit; ++i) {
        const Nal& u = units[i];
        if (u.end <= u.start) continue;
        uint8_t b = d[u.start];
        switch ((b >> 1) & 0x3f) {
        case 32: vps = true; break;
        case 33: hevc_sps = true; break;
        case 34: hevc_pps = true; break;
        default: break;
        }
        switch (b & 0x1f) {
        case 7: avc_sps = true; break;
        case 8: avc_pps = true; break;
        default: break;
        }
    }
    return (vps && hevc_sps && hevc_pps) || (avc_sps && avc_pps);
}

bool params_are_hevc(const std::vector<uint8_t>& ps) {
    for (auto& u : nal_units(ps.data(), ps.size())) {
        if (u.end <= u.start) continue;
        uint8_t b = ps[u.start];
        if (((b >> 1) & 0x3f) == 32 || ((b >> 1) & 0x3f) == 33) return true;
        if ((b & 0x1f) == 7 || (b & 0x1f) == 8) return false;
    }
    return false;
}

std::vector<uint8_t> extract_param_sets(const uint8_t* d, size_t n) {
    std::vector<uint8_t> vps, sps, pps, avc_sps, avc_pps;
    auto store = [&](std::vector<uint8_t>& dst, const Nal& u) {
        dst.insert(dst.end(), {0, 0, 0, 1});
        dst.insert(dst.end(), d + u.start, d + u.end);
    };
    for (auto& u : nal_units(d, n)) {
        if (u.end <= u.start) continue;
        uint8_t b = d[u.start];
        switch ((b >> 1) & 0x3f) {
        case 32: if (vps.empty()) store(vps, u); break;
        case 33: if (sps.empty()) store(sps, u); break;
        case 34: if (pps.empty()) store(pps, u); break;
        default: break;
        }
        switch (b & 0x1f) {
        case 7: if (avc_sps.empty()) store(avc_sps, u); break;
        case 8: if (avc_pps.empty()) store(avc_pps, u); break;
        default: break;
        }
    }
    std::vector<uint8_t> out;
    if (!vps.empty() && !sps.empty() && !pps.empty()) {
        out.insert(out.end(), vps.begin(), vps.end());
        out.insert(out.end(), sps.begin(), sps.end());
        out.insert(out.end(), pps.begin(), pps.end());
    } else if (!avc_sps.empty() && !avc_pps.empty()) {
        out.insert(out.end(), avc_sps.begin(), avc_sps.end());
        out.insert(out.end(), avc_pps.begin(), avc_pps.end());
    }
    return out;
}

bool packet_has_picture(const uint8_t* d, size_t n, bool hevc) {
    bool found_nal = false;
    for (auto& u : nal_units(d, n)) {
        found_nal = true;
        if (u.end <= u.start) continue;
        uint8_t b = d[u.start];
        if (hevc) {
            if (((b >> 1) & 0x3f) <= 21) return true;
        } else {
            int t = b & 0x1f;
            if (t == 1 || t == 5) return true;
        }
    }
    // A length-prefixed AVCC/HVCC packet has no start codes, so it must be kept.
    return !found_nal;
}

} // namespace

// ------------------------------------------------------------------------------------------------

bool annexb_has_param_sets(const uint8_t* d, size_t n) { return has_param_sets(d, n); }

bool ff_load() {
    std::lock_guard<std::mutex> lock(g_load_mutex);
    if (g_loaded) return true;
    const std::wstring dir = encoder_dir();
    std::string missing;
    for (const char* lib : kLibs) {
        if (!file_exists(dir + L"\\" + utf8_to_wide(lib))) missing += (missing.empty() ? "" : ", ") + std::string(lib);
    }
    if (!missing.empty()) {
        set_last_error("FFmpeg libraries not found in '" + wide_to_utf8(dir.c_str(), dir.size()) + "': missing " +
                       missing + ". Run engine\\encoder\\fetch-ffmpeg.ps1 to install FFmpeg 5.1.2.");
        return false;
    }
    // Load by full path first: the delay-load thunks then resolve "avcodec-59.dll" to these modules.
    for (const char* lib : kLibs) {
        std::wstring full = dir + L"\\" + utf8_to_wide(lib);
        if (!LoadLibraryExW(full.c_str(), nullptr, LOAD_WITH_ALTERED_SEARCH_PATH)) {
            set_last_error(std::string("Could not load '") + wide_to_utf8(full.c_str(), full.size()) +
                           "' (error " + std::to_string(GetLastError()) + ")");
            return false;
        }
    }
    unsigned util = avutil_version() >> 16, codec = avcodec_version() >> 16, format = avformat_version() >> 16;
    if (util != 57 || codec != 59 || format != 59) {
        set_last_error("Unsupported FFmpeg build: found avutil " + std::to_string(util) + " / avcodec " +
                       std::to_string(codec) + " / avformat " + std::to_string(format) +
                       ", need 57 / 59 / 59 (FFmpeg 5.1.x).");
        return false;
    }
    g_loaded = true;
    return true;
}

VideoOptions options_from_pref(int pref, int w, int h, int fps, uint32_t bps) {
    VideoOptions o;
    switch (pref & 0xFF) {
    case 1: o.enc = Enc::Nvenc; break;
    case 2: o.enc = Enc::Amf; break;
    case 3: o.enc = Enc::Qsv; break;
    case 4: o.enc = Enc::X264; break;
    default: o.enc = Enc::Auto; break;
    }
    o.rate = ((pref >> 8) & 1) ? Rate::Vbr : Rate::Cbr;
    switch ((pref >> 12) & 3) {
    case 1: o.codec = CodecSel::Hevc; break;
    case 2: o.codec = CodecSel::H264; break;
    default: o.codec = CodecSel::Auto; break;
    }
    switch ((pref >> 10) & 3) {
    case 0: o.preset = Preset::Speed; break;
    case 2: o.preset = Preset::Quality; break;
    default: o.preset = Preset::Balanced; break;
    }
    o.w = w & ~1;
    o.h = h & ~1;
    o.fps = std::max(1, fps);
    o.bps = bps;
    o.avail = ((uint32_t)pref >> 16) & ENC_ALL;
    return o;
}

// ---- FfVideo ----

struct FfVideo::Impl {
    AVCodecContext* ctx = nullptr;
    AVFrame* frame = nullptr;
    AVPacket* pkt = nullptr;
    std::string name;
    std::vector<uint8_t> extradata;
    int w = 0, h = 0;
    bool flushed = false;
    ~Impl() {
        if (frame) av_frame_free(&frame);
        if (pkt) av_packet_free(&pkt);
        if (ctx) avcodec_free_context(&ctx);
    }
};

FfVideo::~FfVideo() = default;
const std::string& FfVideo::name() const { return p_->name; }

int FfVideo::kind_code() const {
    const std::string& n = p_->name;
    if (n == "hevc_nvenc") return 16;
    if (n == "hevc_amf") return 17;
    if (n == "hevc_qsv") return 18;
    if (n == "libx265") return 19;
    if (n == "hevc_mf") return 20;
    if (n == "h264_nvenc") return 26;
    if (n == "h264_amf") return 27;
    if (n == "h264_qsv") return 28;
    if (n == "libx264") return 29;
    return 30;
}

std::unique_ptr<FfVideo> FfVideo::open(const std::string& name, const VideoOptions& o, std::string* err) {
    if (!ff_load()) {
        if (err) *err = last_error();
        return nullptr;
    }
    const AVCodec* codec = avcodec_find_encoder_by_name(name.c_str());
    if (!codec) {
        if (err) *err = "not included in this FFmpeg build";
        return nullptr;
    }
    auto impl = std::make_unique<Impl>();
    impl->name = name;
    impl->ctx = avcodec_alloc_context3(codec);
    if (!impl->ctx) {
        if (err) *err = "out of memory";
        return nullptr;
    }
    AVCodecContext* c = impl->ctx;
    const bool hevc = is_hevc_name(name), h264 = is_h264_name(name);
    int64_t bps = o.bps;
    if (h264) {
        bps = o.preset == Preset::Quality ? bps * 4 / 3 : o.preset == Preset::Balanced ? bps * 7 / 5 : bps * 3 / 2;
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
    if (o.rate == Rate::Cbr) {
        c->rc_max_rate = bps;
        c->rc_buffer_size = (int)std::min<int64_t>(bps, INT32_MAX);
    } else {
        c->rc_max_rate = bps * 3 / 2;
        c->rc_buffer_size = (int)std::min<int64_t>(bps * 2, INT32_MAX);
    }
    c->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
    // OBS default: limited ("TV") range, BT.709 for HD and BT.601 below.
    const bool hd = o.w >= 1280 || o.h > 576;
    c->color_range = AVCOL_RANGE_MPEG;
    c->colorspace = hd ? AVCOL_SPC_BT709 : AVCOL_SPC_SMPTE170M;
    c->color_primaries = hd ? AVCOL_PRI_BT709 : AVCOL_PRI_SMPTE170M;
    c->color_trc = hd ? AVCOL_TRC_BT709 : AVCOL_TRC_SMPTE170M;

    AVDictionary* dict = nullptr;
    for (auto& kv : codec_options(name, o)) av_dict_set(&dict, kv.first, kv.second, 0);
    int rc = avcodec_open2(c, codec, &dict);
    av_dict_free(&dict);
    if (rc < 0) {
        if (err) *err = av_err(rc);
        return nullptr;
    }
    if (c->extradata && c->extradata_size > 0) impl->extradata.assign(c->extradata, c->extradata + c->extradata_size);
    impl->frame = av_frame_alloc();
    impl->pkt = av_packet_alloc();
    if (!impl->frame || !impl->pkt) {
        if (err) *err = "out of memory";
        return nullptr;
    }
    impl->frame->width = o.w;
    impl->frame->height = o.h;
    impl->frame->format = c->pix_fmt;
    rc = av_frame_get_buffer(impl->frame, 0);
    if (rc < 0) {
        if (err) *err = av_err(rc);
        return nullptr;
    }
    impl->w = o.w;
    impl->h = o.h;
    std::unique_ptr<FfVideo> v(new FfVideo());
    v->p_ = std::move(impl);
    return v;
}

static bool receive_packets(FfVideo::Impl& p, std::vector<EncodedFrame>& out, std::string* err) {
    for (;;) {
        int rc = avcodec_receive_packet(p.ctx, p.pkt);
        if (rc == AVERROR(EAGAIN) || rc == AVERROR_EOF) return true;
        if (rc < 0) {
            if (err) *err = av_err(rc);
            return false;
        }
        const bool hevc = p.name.rfind("hevc_", 0) == 0;
        if (p.pkt->data && p.pkt->size > 0 && packet_has_picture(p.pkt->data, p.pkt->size, hevc)) {
            EncodedFrame f;
            f.keyframe = (p.pkt->flags & AV_PKT_FLAG_KEY) != 0;
            f.pts100ns = p.pkt->pts != AV_NOPTS_VALUE ? p.pkt->pts : p.pkt->dts;
            if (f.keyframe && !p.extradata.empty() && !has_param_sets(p.pkt->data, p.pkt->size)) {
                f.data = p.extradata;
                f.data.insert(f.data.end(), p.pkt->data, p.pkt->data + p.pkt->size);
            } else {
                f.data.assign(p.pkt->data, p.pkt->data + p.pkt->size);
            }
            out.push_back(std::move(f));
        }
        av_packet_unref(p.pkt);
    }
}

bool FfVideo::encode_nv12(const uint8_t* y, size_t y_stride, const uint8_t* uv, size_t uv_stride, int64_t pts,
                          bool idr, std::vector<EncodedFrame>& out, std::string* err) {
    Impl& p = *p_;
    if (p.flushed) {
        if (err) *err = "encoder was already flushed";
        return false;
    }
    const size_t w = p.w, h = p.h;
    if (y_stride < w || uv_stride < w) {
        if (err) *err = "NV12 frame is smaller than the encoder size";
        return false;
    }
    int rc = av_frame_make_writable(p.frame);
    if (rc < 0) {
        if (err) *err = av_err(rc);
        return false;
    }
    AVFrame* f = p.frame;
    for (size_t row = 0; row < h; ++row) std::memcpy(f->data[0] + row * f->linesize[0], y + row * y_stride, w);
    if (p.name == "libx264") {
        for (size_t row = 0; row < h / 2; ++row) {
            const uint8_t* src = uv + row * uv_stride;
            uint8_t* u = f->data[1] + row * f->linesize[1];
            uint8_t* v = f->data[2] + row * f->linesize[2];
            for (size_t x = 0; x < w / 2; ++x) {
                u[x] = src[x * 2];
                v[x] = src[x * 2 + 1];
            }
        }
    } else {
        for (size_t row = 0; row < h / 2; ++row)
            std::memcpy(f->data[1] + row * f->linesize[1], uv + row * uv_stride, w);
    }
    f->pts = pts;
    f->pict_type = idr ? AV_PICTURE_TYPE_I : AV_PICTURE_TYPE_NONE;
    rc = avcodec_send_frame(p.ctx, f);
    if (rc == AVERROR(EAGAIN)) {
        if (!receive_packets(p, out, err)) return false;
        rc = avcodec_send_frame(p.ctx, f);
    }
    if (rc < 0) {
        if (err) *err = av_err(rc);
        return false;
    }
    return receive_packets(p, out, err);
}

bool FfVideo::flush(std::vector<EncodedFrame>& out, std::string* err) {
    Impl& p = *p_;
    if (p.flushed) return true;
    p.flushed = true;
    int rc = avcodec_send_frame(p.ctx, nullptr);
    if (rc < 0 && rc != AVERROR_EOF) {
        if (err) *err = av_err(rc);
        return false;
    }
    return receive_packets(p, out, err);
}

// ---- selection & probing ----

namespace {

std::vector<std::string> candidates(Enc enc, CodecSel codec) {
    const bool hevc = codec == CodecSel::Hevc;
    const char* pre = hevc ? "hevc_" : "h264_";
    switch (enc) {
    case Enc::Nvenc: return {std::string(pre) + "nvenc"};
    case Enc::Amf: return {std::string(pre) + "amf"};
    case Enc::Qsv: return {std::string(pre) + "qsv"};
    case Enc::X264: return {hevc ? "libx265" : "libx264"};
    default: return {std::string(pre) + "nvenc", std::string(pre) + "amf", std::string(pre) + "qsv"};
    }
}

uint32_t availability_bit(const std::string& n) {
    if (n == "h264_nvenc") return ENC_H264_NVENC;
    if (n == "hevc_nvenc") return ENC_HEVC_NVENC;
    if (n == "h264_amf") return ENC_H264_AMF;
    if (n == "hevc_amf") return ENC_HEVC_AMF;
    if (n == "h264_qsv") return ENC_H264_QSV;
    if (n == "hevc_qsv") return ENC_HEVC_QSV;
    if (n == "libx264") return ENC_H264_X264;
    if (n == "libx265") return ENC_HEVC_X265;
    return 0xFFFFFFFFu;
}

const char* label_of(const std::string& n) {
    if (n.find("nvenc") != std::string::npos) return "NVIDIA NVENC";
    if (n.find("amf") != std::string::npos) return "AMD AMF";
    if (n.find("qsv") != std::string::npos) return "Intel Quick Sync";
    return n == "libx264" ? "x264 (CPU)" : n == "libx265" ? "x265 (CPU)" : n.c_str();
}

} // namespace

bool probe_ffmpeg_encoder(const char* name, int w, int h, int fps, uint32_t bps) {
    if (!ff_load()) return false;
    VideoOptions o;
    o.codec = is_hevc_name(name) ? CodecSel::Hevc : CodecSel::H264;
    o.w = w & ~1;
    o.h = h & ~1;
    o.fps = std::max(1, fps);
    o.bps = bps;
    std::string err;
    auto v = FfVideo::open(name, o, &err);
    if (!v) return false;
    std::vector<uint8_t> yp((size_t)o.w * o.h, 16), uvp((size_t)o.w * o.h / 2, 128);
    const int64_t dur = SECOND_100NS / o.fps;
    std::vector<EncodedFrame> out;
    for (int i = 0; i < 5; ++i) {
        if (!v->encode_nv12(yp.data(), o.w, uvp.data(), o.w, i * dur, false, out, &err)) return false;
        for (auto& f : out) if (!f.data.empty()) return true;
    }
    if (!v->flush(out, &err)) return false;
    for (auto& f : out) if (!f.data.empty()) return true;
    return false;
}

bool select_video(const VideoOptions& o, bool allow_mf, VideoSelection* out, std::string* err) {
    if (!ff_load()) {
        if (err) *err = last_error();
        return false;
    }
    const bool auto_mode = o.enc == Enc::Auto;
    const bool known = o.avail != 0;
    std::string tried;
    for (const std::string& name : candidates(o.enc, o.codec)) {
        const uint32_t bit = availability_bit(name);
        // The stored probe only prunes Auto; an explicit choice is always attempted.
        if (auto_mode && bit != 0xFFFFFFFFu && known && !(o.avail & bit)) continue;
        std::string e;
        auto v = FfVideo::open(name, o, &e);
        if (v) {
            out->kind = VideoSelection::Ffmpeg;
            out->video = std::move(v);
            return true;
        }
        tried += std::string(tried.empty() ? "" : "; ") + label_of(name) + ": " + e;
    }
    if (auto_mode) {
        const bool hevc = o.codec == CodecSel::Hevc;
        const uint32_t mf_bit = hevc ? ENC_HEVC_MF : ENC_H264_MF;
        if (allow_mf && (!known || (o.avail & mf_bit))) {
            out->kind = VideoSelection::MediaFoundation;
            out->mf = hevc ? MfCodec::Hevc : MfCodec::H264;
            return true;
        }
        if (!hevc) {
            std::string e;
            auto v = FfVideo::open("libx264", o, &e);
            if (v) {
                out->kind = VideoSelection::Ffmpeg;
                out->video = std::move(v);
                return true;
            }
            tried += std::string(tried.empty() ? "" : "; ") + "x264 (CPU): " + e;
        }
    }
    if (err) *err = tried.empty() ? "No working video encoder is available." : "No usable encoder. " + tried;
    return false;
}

// ------------------------------------------------------------------------------------------------
// Save: AAC encode + container write
// ------------------------------------------------------------------------------------------------

namespace {

struct AacPackets {
    std::vector<AVPacket*> list;
    ~AacPackets() { for (auto* p : list) av_packet_free(&p); }
};

bool drain_audio(AVCodecContext* ctx, AacPackets& out, std::string* err) {
    for (;;) {
        AVPacket* pkt = av_packet_alloc();
        if (!pkt) {
            if (err) *err = "out of memory";
            return false;
        }
        int rc = avcodec_receive_packet(ctx, pkt);
        if (rc < 0) {
            av_packet_free(&pkt);
            if (rc == AVERROR(EAGAIN) || rc == AVERROR_EOF) return true;
            if (err) *err = av_err(rc);
            return false;
        }
        out.list.push_back(pkt);
    }
}

bool encode_aac(const std::vector<std::pair<const uint8_t*, size_t>>& slices, AVCodecContext* ctx, int rate,
                int64_t delay_samples, AacPackets& out, std::string* err) {
    const int ch = 2;
    const size_t frame_size = (size_t)std::max(256, ctx->frame_size);
    std::vector<float> planes[2];
    int64_t next_pts = delay_samples;
    auto send = [&](size_t count) -> bool {
        AVFrame* f = av_frame_alloc();
        if (!f) {
            if (err) *err = "out of memory";
            return false;
        }
        f->nb_samples = (int)count;
        f->format = AV_SAMPLE_FMT_FLTP;
        f->sample_rate = rate;
        int rc = av_channel_layout_copy(&f->ch_layout, &ctx->ch_layout);
        if (rc >= 0) rc = av_frame_get_buffer(f, 0);
        if (rc < 0) {
            av_frame_free(&f);
            if (err) *err = av_err(rc);
            return false;
        }
        for (int c = 0; c < ch; ++c) std::memcpy(f->data[c], planes[c].data(), count * sizeof(float));
        f->pts = next_pts;
        next_pts += (int64_t)count;
        rc = avcodec_send_frame(ctx, f);
        av_frame_free(&f);
        for (int c = 0; c < ch; ++c) planes[c].erase(planes[c].begin(), planes[c].begin() + count);
        if (rc < 0) {
            if (err) *err = av_err(rc);
            return false;
        }
        return drain_audio(ctx, out, err);
    };
    for (auto& s : slices) {
        const size_t frames = s.second / (ch * 2);
        for (size_t n = 0; n < frames; ++n) {
            for (int c = 0; c < ch; ++c) {
                int16_t v;
                std::memcpy(&v, s.first + (n * ch + c) * 2, 2);
                planes[c].push_back(v / 32768.0f);
            }
            if (planes[0].size() >= frame_size && !send(frame_size)) return false;
        }
    }
    if (!planes[0].empty() && !send(planes[0].size())) return false;
    int rc = avcodec_send_frame(ctx, nullptr);
    if (rc < 0 && rc != AVERROR_EOF) {
        if (err) *err = av_err(rc);
        return false;
    }
    return drain_audio(ctx, out, err);
}

const char* container_for(const std::string& path) {
    size_t dot = path.find_last_of('.');
    std::string ext = dot == std::string::npos ? "" : path.substr(dot + 1);
    for (auto& ch : ext) ch = (char)std::tolower((unsigned char)ch);
    if (ext == "mkv" || ext == "mka") return "matroska";
    if (ext == "mov") return "mov";
    if (ext == "ts" || ext == "m2ts" || ext == "mts") return "mpegts";
    if (ext == "flv") return "flv";
    return "mp4";
}

} // namespace

int mux_until(const std::string& path, const std::vector<VideoSample>& v, const std::vector<AudioChunk>& a, int fps,
              int w, int h, int seconds_back, int64_t end_time) {
    if (v.empty()) return IR_NOT_ENOUGH_DATA;
    if (!ff_load()) return IR_FFMPEG_UNAVAILABLE;
    fps = std::max(1, fps);
    w = std::max(2, w);
    h = std::max(2, h);
    const int64_t trim_start = seconds_back > 0 ? end_time - (int64_t)seconds_back * SECOND_100NS : INT64_MIN;

    size_t start = 0;
    if (seconds_back > 0) {
        long before = -1;
        for (size_t i = 0; i < v.size(); ++i) {
            if (v[i].pts100ns > trim_start) break;
            if (v[i].keyframe) before = (long)i;
        }
        if (before >= 0) {
            start = (size_t)before;
        } else {
            start = 0;
            for (size_t i = 0; i < v.size(); ++i) if (v[i].keyframe) { start = i; break; }
        }
    }
    const size_t count = v.size() - start;
    const VideoSample* s = v.data() + start;
    const int64_t first_pts = (count == 1 && trim_start != INT64_MIN) ? std::max(s[0].pts100ns, trim_start)
                                                                    : s[0].pts100ns;
    const int64_t frame_dur = SECOND_100NS / fps;
    const int64_t last_end = std::max(end_time, s[count - 1].pts100ns + frame_dur);
    if (last_end - first_pts < SECOND_100NS / 2) return IR_NOT_ENOUGH_DATA;

    std::vector<uint8_t> extradata;
    for (size_t i = 0; i < count; ++i) {
        if (s[i].keyframe) { extradata = extract_param_sets(s[i].data, s[i].size); break; }
    }
    if (extradata.empty()) {
        set_last_error("The buffered video has no HEVC VPS/SPS/PPS or AVC SPS/PPS parameter sets.");
        return IR_FAIL;
    }

    // Clip audio chunks to [first_pts, last_end) with sample accuracy.
    std::vector<std::pair<const uint8_t*, size_t>> slices;
    int64_t audio_first = -1;
    for (const auto& c : a) {
        const int64_t dur = (int64_t)c.frames * SECOND_100NS / AUDIO_RATE;
        if (c.pts100ns + dur <= first_pts || c.pts100ns >= last_end) continue;
        int64_t skip = 0, keep = c.frames;
        if (c.pts100ns < first_pts) skip = (first_pts - c.pts100ns) * AUDIO_RATE / SECOND_100NS;
        const int64_t end_frame = (last_end - c.pts100ns) * AUDIO_RATE / SECOND_100NS;
        if (end_frame < keep) keep = end_frame;
        keep -= skip;
        if (keep <= 0) continue;
        if (audio_first < 0) audio_first = c.pts100ns + skip * SECOND_100NS / AUDIO_RATE;
        slices.push_back({c.data + skip * 4, (size_t)keep * 4});
    }
    const int64_t audio_delay =
        slices.empty() ? 0 : std::max<int64_t>(0, (audio_first - first_pts)) * AUDIO_RATE / SECOND_100NS;

    std::string err;
    auto fail = [&](const std::string& m) { set_last_error(m); return IR_FAIL; };

    AVCodecContext* aac = nullptr;
    AacPackets packets;
    if (!slices.empty()) {
        const AVCodec* codec = avcodec_find_encoder_by_name("aac");
        if (!codec) return fail("This FFmpeg build has no AAC encoder.");
        aac = avcodec_alloc_context3(codec);
        if (!aac) return fail("out of memory");
        aac->sample_fmt = AV_SAMPLE_FMT_FLTP;
        aac->sample_rate = AUDIO_RATE;
        av_channel_layout_default(&aac->ch_layout, 2);
        aac->bit_rate = 192000;
        aac->time_base = {1, (int)AUDIO_RATE};
        aac->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
        int rc = avcodec_open2(aac, codec, nullptr);
        if (rc < 0) {
            avcodec_free_context(&aac);
            return fail("AAC encoder: " + av_err(rc));
        }
        if (!encode_aac(slices, aac, AUDIO_RATE, audio_delay, packets, &err)) {
            avcodec_free_context(&aac);
            return fail("AAC encoding failed: " + err);
        }
    }

    const char* muxer = container_for(path);
    AVFormatContext* fmt = nullptr;
    int rc = avformat_alloc_output_context2(&fmt, nullptr, muxer, path.c_str());
    if (rc < 0 || !fmt) {
        if (aac) avcodec_free_context(&aac);
        return fail(std::string("Could not create the ") + muxer + " muxer: " + av_err(rc));
    }
    bool io_open = false;
    auto cleanup = [&](bool remove_file) {
        if (io_open) avio_closep(&fmt->pb);
        avformat_free_context(fmt);
        if (aac) avcodec_free_context(&aac);
        if (remove_file) DeleteFileW(utf8_to_wide(path).c_str());
    };

    AVStream* vst = avformat_new_stream(fmt, nullptr);
    if (!vst) { cleanup(false); return fail("out of memory"); }
    vst->time_base = {1, 90000};
    vst->avg_frame_rate = {fps, 1};
    AVCodecParameters* par = vst->codecpar;
    par->codec_type = AVMEDIA_TYPE_VIDEO;
    par->codec_id = params_are_hevc(extradata) ? AV_CODEC_ID_HEVC : AV_CODEC_ID_H264;
    par->format = AV_PIX_FMT_YUV420P;
    par->width = w;
    par->height = h;
    const bool hd = w >= 1280 || h > 576;
    par->color_range = AVCOL_RANGE_MPEG;
    par->color_space = hd ? AVCOL_SPC_BT709 : AVCOL_SPC_SMPTE170M;
    par->color_primaries = hd ? AVCOL_PRI_BT709 : AVCOL_PRI_SMPTE170M;
    par->color_trc = hd ? AVCOL_TRC_BT709 : AVCOL_TRC_SMPTE170M;
    // Owned by the stream: avformat_free_context() in cleanup() releases it via av_freep (no double free).
    par->extradata = static_cast<uint8_t*>(av_mallocz(extradata.size() + AV_INPUT_BUFFER_PADDING_SIZE));
    if (!par->extradata) { cleanup(false); return fail("out of memory"); }
    std::memcpy(par->extradata, extradata.data(), extradata.size());
    par->extradata_size = (int)extradata.size();

    AVStream* ast = nullptr;
    if (aac) {
        ast = avformat_new_stream(fmt, nullptr);
        if (!ast) { cleanup(false); return fail("out of memory"); }
        ast->time_base = {1, (int)AUDIO_RATE};
        rc = avcodec_parameters_from_context(ast->codecpar, aac);
        if (rc < 0) { cleanup(false); return fail(av_err(rc)); }
    }

    if (!(fmt->oformat->flags & AVFMT_NOFILE)) {
        rc = avio_open(&fmt->pb, path.c_str(), AVIO_FLAG_WRITE);
        if (rc < 0) { cleanup(false); return fail("Could not open '" + path + "': " + av_err(rc)); }
        io_open = true;
    }
    AVDictionary* opts = nullptr;
    if (std::strcmp(muxer, "mp4") == 0 || std::strcmp(muxer, "mov") == 0) av_dict_set(&opts, "movflags", "+faststart", 0);
    rc = avformat_write_header(fmt, &opts);
    av_dict_free(&opts);
    if (rc < 0) { cleanup(true); return fail("Writing the container header failed: " + av_err(rc)); }

    // Monotonic timestamps in the video stream's time base; equal/decreasing input PTS are bumped.
    struct VideoTs { int64_t pts, duration; };
    std::vector<VideoTs> video_ts(count);
    int64_t previous = INT64_MIN;
    for (size_t i = 0; i < count; ++i) {
        const int64_t rel = std::max<int64_t>(0, std::max(s[i].pts100ns, first_pts) - first_pts);
        int64_t pts = av_rescale_q(rel, kTb100ns, vst->time_base);
        if (pts <= previous) pts = previous + 1;
        const int64_t next_rel = i + 1 < count
                                     ? std::max<int64_t>(0, std::max(s[i + 1].pts100ns, first_pts) - first_pts)
                                     : std::max(last_end - first_pts, rel + 1);
        const int64_t duration = std::max<int64_t>(1, av_rescale_q(next_rel, kTb100ns, vst->time_base) - pts);
        previous = pts;
        video_ts[i] = {pts, duration};
    }
    struct Item { AVPacket* pkt; int64_t sort; };
    std::vector<Item> items;
    auto free_items = [&]() {
        for (auto& it : items) av_packet_free(&it.pkt);
        items.clear();
    };
    for (size_t i = 0; i < count; ++i) {
        AVPacket* pkt = av_packet_alloc();
        if (!pkt) { free_items(); cleanup(true); return fail("out of memory"); }
        int alloc_rc = av_new_packet(pkt, (int)s[i].size);
        if (alloc_rc < 0) {
            av_packet_free(&pkt);
            free_items();
            cleanup(true);
            return fail("Could not allocate a video packet: " + av_err(alloc_rc));
        }
        std::memcpy(pkt->data, s[i].data, s[i].size);
        pkt->pts = pkt->dts = video_ts[i].pts;
        pkt->duration = video_ts[i].duration;
        pkt->stream_index = vst->index;
        if (s[i].keyframe) pkt->flags |= AV_PKT_FLAG_KEY;
        items.push_back({pkt, video_ts[i].pts * 1000000 / vst->time_base.den});
    }
    if (ast) {
        for (AVPacket*& p : packets.list) {
            p->stream_index = ast->index;
            items.push_back({p, p->pts * 1000000 / (int64_t)AUDIO_RATE});
            p = nullptr; // ownership moves to `items`
        }
        packets.list.clear();
    }
    std::stable_sort(items.begin(), items.end(), [](const Item& x, const Item& y) { return x.sort < y.sort; });
    int result = IR_OK;
    for (auto& it : items) {
        if (result == IR_OK) {
            rc = av_interleaved_write_frame(fmt, it.pkt);
            if (rc < 0) { set_last_error("Writing a packet failed: " + av_err(rc)); result = IR_FAIL; }
        }
        av_packet_free(&it.pkt);
    }
    if (result == IR_OK) {
        rc = av_write_trailer(fmt);
        if (rc < 0) { set_last_error("Finalizing the file failed: " + av_err(rc)); result = IR_FAIL; }
    }
    cleanup(result != IR_OK);
    return result;
}

} // namespace ir
