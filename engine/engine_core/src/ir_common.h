// Instant Replay engine (C++17). Shared definitions: platform headers, ABI return codes,
// timing helpers, last-error store and encoder identifiers.
#pragma once

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef NOMINMAX
#define NOMINMAX
#endif
#ifndef _WIN32_WINNT
#define _WIN32_WINNT 0x0A00
#endif
#include <windows.h>
#include <d3d11_4.h>
#include <dxgi1_6.h>
#include <wrl/client.h>

#include <algorithm>
#include <atomic>
#include <chrono>
#include <condition_variable>
#include <cstdint>
#include <cstring>
#include <deque>
#include <functional>
#include <memory>
#include <mutex>
#include <string>
#include <thread>
#include <vector>

namespace ir {

using Microsoft::WRL::ComPtr;

// ---- ABI return codes (must match InstantReplay.Native/IrNative.cs) ----
constexpr int IR_OK = 0;
constexpr int IR_FAIL = -1;
constexpr int IR_NOT_ENOUGH_DATA = -30;
constexpr int IR_LOW_MEMORY = -31;
constexpr int IR_FFMPEG_UNAVAILABLE = -40;

// ---- Failure stages reported through IR_LastFailStage ----
constexpr int STAGE_CREATE_DEVICE = 1;
constexpr int STAGE_CREATE_ITEM = 2;
constexpr int STAGE_FRAME_POOL = 3;
constexpr int STAGE_START_CAPTURE = 4;
constexpr int STAGE_ENCODER = 5;
constexpr int STAGE_AUDIO = 6;
constexpr int STAGE_MUX = 7;
constexpr int STAGE_FFMPEG = 8;

constexpr int64_t SECOND_100NS = 10'000'000;
constexpr uint32_t AUDIO_RATE = 48000;
constexpr uint32_t AUDIO_CHANNELS = 2;
constexpr int64_t RING_BUDGET_BYTES = 512ll * 1024 * 1024;

// ---- Capture methods (values shared with the managed side) ----
enum class CaptureMethod : int { Wgc = 0, Dxgi = 1 };

// ---- Capture option bits (IR_SetCaptureTarget / IR_SetCaptureSource) ----
constexpr uint32_t CAP_CLIENT_AREA = 1;
constexpr uint32_t CAP_CURSOR = 2;
constexpr uint32_t CAP_BORDER = 4;
constexpr uint32_t CAP_KEEP_CORNERS = 8;
constexpr uint32_t CAP_SECONDARY = 16;
constexpr uint32_t CAP_FOLLOW = 32;
constexpr uint32_t CAP_PROCESS_AUDIO = 64;
constexpr uint32_t CAP_SWITCHED = 128;

// ---- Encoder identifiers reported through IR_ActiveEncoder ----
// 16..20 HEVC (nvenc, amf, qsv, x265, mf), 26..30 H.264 (same order).
constexpr int KIND_UNKNOWN = 0;

// ---- Availability bits returned by IR_ProbeEncoders (mirror EncoderAvailability in C#) ----
constexpr uint32_t ENC_H264_NVENC = 1u << 0;
constexpr uint32_t ENC_HEVC_NVENC = 1u << 1;
constexpr uint32_t ENC_H264_AMF = 1u << 2;
constexpr uint32_t ENC_HEVC_AMF = 1u << 3;
constexpr uint32_t ENC_H264_QSV = 1u << 4;
constexpr uint32_t ENC_HEVC_QSV = 1u << 5;
constexpr uint32_t ENC_H264_X264 = 1u << 6;
constexpr uint32_t ENC_HEVC_X265 = 1u << 7;
constexpr uint32_t ENC_H264_MF = 1u << 8;
constexpr uint32_t ENC_HEVC_MF = 1u << 9;
constexpr uint32_t ENC_ALL = (1u << 10) - 1;

inline int64_t qpc_to_100ns(int64_t ticks) {
    static const int64_t freq = [] {
        LARGE_INTEGER f;
        QueryPerformanceFrequency(&f);
        return std::max<int64_t>(1, f.QuadPart);
    }();
    // 128-bit safe: split into whole seconds and remainder.
    int64_t secs = ticks / freq, rem = ticks % freq;
    return secs * SECOND_100NS + rem * SECOND_100NS / freq;
}
inline int64_t qpc100ns() {
    LARGE_INTEGER t;
    QueryPerformanceCounter(&t);
    return qpc_to_100ns(t.QuadPart);
}

// ---- Last error text surfaced by IR_LastErrorMessage ----
void set_last_error(const std::string& message);
std::string last_error();

std::string wide_to_utf8(const wchar_t* s, size_t len);
std::wstring utf8_to_wide(const std::string& s);

struct EncodedFrame {
    std::vector<uint8_t> data;
    bool keyframe = false;
    int64_t pts100ns = 0;
};

inline bool succeeded(HRESULT hr) { return hr >= 0; }

} // namespace ir
