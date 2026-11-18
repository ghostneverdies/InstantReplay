// FFmpeg 5.1.2 access: DLLs loaded on demand from engine\encoder, hardware/software video encoders,
// encoder probing with real frames, and the AAC + container writer used by Save.
#pragma once
#include "ir_common.h"

namespace ir {

enum class Enc { Auto, Nvenc, Amf, Qsv, X264 };
enum class Rate { Cbr, Vbr };
enum class CodecSel { Auto, Hevc, H264 };
enum class Preset { Speed, Balanced, Quality };
enum class MfCodec { H264, Hevc };

struct VideoOptions {
    Enc enc = Enc::Auto;
    CodecSel codec = CodecSel::Auto;
    Rate rate = Rate::Cbr;
    Preset preset = Preset::Balanced;
    int w = 0, h = 0, fps = 30;
    uint32_t bps = 0;
    uint32_t avail = 0; // ENC_* bits from the last probe; 0 = unknown, try everything
};

// True when an Annex-B access unit already carries SPS/PPS (H.264) or VPS/SPS/PPS (HEVC).
bool annexb_has_param_sets(const uint8_t* data, size_t size);

// Decodes the packed preference int that managed code passes to IR_Start.
VideoOptions options_from_pref(int pref, int w, int h, int fps, uint32_t bps);

// Loads avutil/swresample/avcodec/avformat from the encoder folder (once). Sets the last error on failure.
bool ff_load();

class FfVideo {
public:
    ~FfVideo();
    static std::unique_ptr<FfVideo> open(const std::string& name, const VideoOptions& o, std::string* err);
    bool encode_nv12(const uint8_t* y, size_t y_stride, const uint8_t* uv, size_t uv_stride, int64_t pts100ns,
                     bool idr, std::vector<EncodedFrame>& out, std::string* err);
    bool flush(std::vector<EncodedFrame>& out, std::string* err);
    int kind_code() const;
    const std::string& name() const;
    struct Impl; // public so the .cpp helpers can name it; defined only in ffmpeg_rt.cpp

private:
    FfVideo() = default;
    std::unique_ptr<Impl> p_;
};

struct VideoSelection {
    enum Kind { Ffmpeg, MediaFoundation } kind = Ffmpeg;
    std::unique_ptr<FfVideo> video; // when kind == Ffmpeg
    MfCodec mf = MfCodec::H264;     // when kind == MediaFoundation
};

// Auto walks NVENC -> AMF -> Quick Sync -> Media Foundation (if allowed) -> x264 and only uses encoders
// the last probe found working. An explicit choice tries only that encoder (never falls back).
bool select_video(const VideoOptions& o, bool allow_mf, VideoSelection* out, std::string* err);

// Opens `name`, encodes a few synthetic frames and reports whether it produced output.
bool probe_ffmpeg_encoder(const char* name, int w, int h, int fps, uint32_t bps);

struct VideoSample {
    const uint8_t* data;
    size_t size;
    bool keyframe;
    int64_t pts100ns;
};
struct AudioChunk {
    const uint8_t* data; // interleaved s16 stereo
    uint32_t frames;
    int64_t pts100ns;
};

// Encodes `audio` to AAC and writes video + audio to `path` (container chosen from the extension).
int mux_until(const std::string& utf8_path, const std::vector<VideoSample>& video,
              const std::vector<AudioChunk>& audio, int fps, int w, int h, int seconds_back, int64_t end_time);

} // namespace ir
