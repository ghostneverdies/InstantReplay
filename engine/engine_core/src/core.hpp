// Instant Replay native engine v4 — shared primitives and interfaces.
//
// This header owns the data-plane contract for the five-file rewrite:
//   * intrusive, pooled leases (no shared_ptr control blocks) for frames,
//     packets, audio blocks, keepalives and acknowledgement objects;
//   * fixed-capacity, allocation-free SPSC-style queues;
//   * a stop-aware control bus whose commands are always acknowledged;
//   * the single Clock shared by video and audio lanes.
//
// Control-plane objects (save jobs, control commands, callbacks) may use
// std::shared_ptr. Nothing in the data plane may.

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
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <deque>
#include <functional>
#include <limits>
#include <memory>
#include <mutex>
#include <new>
#include <semaphore>
#include <span>
#include <string>
#include <thread>
#include <utility>
#include <vector>

namespace ir
{

    using Microsoft::WRL::ComPtr;

    // -------------------------------------------------------------------------
    // ABI constants — must match InstantReplay.Native/IrNative.cs exactly.
    // -------------------------------------------------------------------------
    constexpr int IR_OK = 0;
    constexpr int IR_FAIL = -1;
    constexpr int IR_NOT_ENOUGH_DATA = -30;
    constexpr int IR_LOW_MEMORY = -31;
    // -40 is retired with the old external encoder stack; do not reuse it.
    constexpr int IR_PENDING = -999;

    constexpr int STAGE_CREATE_DEVICE = 1;
    constexpr int STAGE_CREATE_ITEM = 2;
    constexpr int STAGE_FRAME_POOL = 3;
    constexpr int STAGE_START_CAPTURE = 4;
    constexpr int STAGE_ENCODER = 5;
    constexpr int STAGE_AUDIO = 6;
    constexpr int STAGE_MUX = 7;
    // 8 is retired with the old external encoder stack; do not renumber 1..7.

    constexpr int64_t SECOND_100NS = 10'000'000;
    constexpr uint32_t AUDIO_RATE = 48000;
    constexpr uint32_t AUDIO_CHANNELS = 2;
    constexpr int64_t RING_BUDGET_BYTES = 512ll * 1024 * 1024;

    constexpr uint32_t CAP_CLIENT_AREA = 1;
    constexpr uint32_t CAP_CURSOR = 2;
    constexpr uint32_t CAP_BORDER = 4;
    constexpr uint32_t CAP_KEEP_CORNERS = 8;
    constexpr uint32_t CAP_SECONDARY = 16;
    constexpr uint32_t CAP_FOLLOW = 32;
    constexpr uint32_t CAP_PROCESS_AUDIO = 64;
    constexpr uint32_t CAP_SWITCHED = 128;

    constexpr int KIND_UNKNOWN = 0;

    // Bits 0..7 are retired with the old external encoder stack.
    // Media Foundation hardware MFTs, split by vendor. "VP12" is the Intel
    // Gen12 / Arc / Core Ultra family; it still encodes H.264 or HEVC.
    constexpr uint32_t ENC_H264_MF_QSV = 1u << 8;
    constexpr uint32_t ENC_HEVC_MF_QSV = 1u << 9;
    constexpr uint32_t ENC_H264_MF_VP12 = 1u << 10;
    constexpr uint32_t ENC_HEVC_MF_VP12 = 1u << 11;
    constexpr uint32_t ENC_H264_MF_NVENC = 1u << 12;
    constexpr uint32_t ENC_HEVC_MF_NVENC = 1u << 13;
    constexpr uint32_t ENC_H264_MF_AMF = 1u << 14;
    constexpr uint32_t ENC_HEVC_MF_AMF = 1u << 15;
    constexpr uint32_t ENC_AV1_MF_QSV = 1u << 16;
    constexpr uint32_t ENC_AV1_MF_VP12 = 1u << 17;
    constexpr uint32_t ENC_AV1_MF_NVENC = 1u << 18;
    constexpr uint32_t ENC_AV1_MF_AMF = 1u << 19;
    constexpr uint32_t ENC_H264_MF_SOFTWARE = 1u << 20;
    constexpr uint32_t ENC_HEVC_MF_SOFTWARE = 1u << 21;
    constexpr uint32_t ENC_AV1_MF_SOFTWARE = 1u << 22;
    constexpr uint32_t ENC_ALL = (1u << 23) - 1;

    // Compact per-codec availability packed into IR_Start's pref bits 24..31.
    // Managed code derives this byte from the full probe mask for the selected
    // codec only, so 8 bits are enough for every encoder variant.
    constexpr uint32_t AVAIL_MF_QSV = 1u << 4;
    constexpr uint32_t AVAIL_MF_VP12 = 1u << 5;
    constexpr uint32_t AVAIL_MF_NVENC = 1u << 6;
    constexpr uint32_t AVAIL_MF_AMF = 1u << 7;

    // -------------------------------------------------------------------------
    // Locked v4 limits. These are invariants, not tuning knobs.
    // -------------------------------------------------------------------------
    constexpr int kVideoSlots = 6;        // VideoPipeline::kSlots
    constexpr int kSurfaces = 6;          // encoder surfaces
    constexpr int kAsyncDepth = 5;        // <= kVideoSlots - 1; 7 caused corruption
    constexpr size_t kPendingMax = 3;     // encode when pending_.size() > 2
    constexpr size_t kRawWgc = 3;
    constexpr size_t kRawDxgi = 1;
    constexpr size_t kSaveQueue = 4;
    constexpr size_t kJobRing = 8;
    constexpr size_t kPacketPool = 2048;
    constexpr size_t kAudioPool = 256;
    constexpr size_t kKeepalivePool = 32;
    constexpr size_t kAckPool = 32;

    static_assert(kAsyncDepth <= kVideoSlots - 1, "async_depth must be <= kSlots - 1");
    static_assert(kSurfaces == kVideoSlots, "encoder surface count must equal pipeline slot count");

    inline int64_t qpc_to_100ns(int64_t ticks)
    {
        static const int64_t freq = []
        {
            LARGE_INTEGER f;
            QueryPerformanceFrequency(&f);
            return std::max<int64_t>(1, f.QuadPart);
        }();
        const int64_t secs = ticks / freq;
        const int64_t rem = ticks % freq;
        return secs * SECOND_100NS + rem * SECOND_100NS / freq;
    }

    inline int64_t qpc100ns()
    {
        LARGE_INTEGER t;
        QueryPerformanceCounter(&t);
        return qpc_to_100ns(t.QuadPart);
    }

    void set_last_error(const std::string &message);
    std::string last_error();
    std::string wide_to_utf8(const wchar_t *s, size_t len);
    std::wstring utf8_to_wide(const std::string &s);
    inline bool succeeded(HRESULT hr) { return hr >= 0; }

    inline uint32_t even_up(uint32_t v) { return (v + 1u) & ~1u; }

    inline std::pair<uint32_t, uint32_t> fit_resolution(int desk_w, int desk_h, int cap_w, int cap_h)
    {
        int w = desk_w, h = desk_h;
        if (cap_w > 0 && cap_h > 0)
        {
            const double scale = std::min((double)cap_w / std::max(1, w), (double)cap_h / std::max(1, h));
            if (scale < 1.0)
            {
                w = (int)(w * scale);
                h = (int)(h * scale);
            }
        }
        else if (cap_w > 0)
        {
            const double scale = (double)cap_w / std::max(1, w);
            if (scale < 1.0)
            {
                h = (int)(h * scale);
                w = cap_w;
            }
        }
        else if (cap_h > 0)
        {
            const double scale = (double)cap_h / std::max(1, h);
            if (scale < 1.0)
            {
                w = (int)(w * scale);
                h = cap_h;
            }
        }
        return {(uint32_t)std::max(2, w / 2 * 2), (uint32_t)std::max(2, h / 2 * 2)};
    }

    inline std::pair<uint32_t, uint32_t> canvas_size_for(uint32_t source_w, uint32_t source_h,
                                                         uint32_t out_w, uint32_t out_h)
    {
        uint32_t cw = std::max(2u, source_w), ch = std::max(2u, source_h);
        if ((uint64_t)cw * out_h < (uint64_t)ch * out_w)
            cw = (uint32_t)(((uint64_t)ch * out_w + out_h - 1) / out_h);
        else
            ch = (uint32_t)(((uint64_t)cw * out_h + out_w - 1) / out_w);
        return {even_up(cw), even_up(ch)};
    }

    // -------------------------------------------------------------------------
    // Enums shared by capture, video, audio and engine translation units.
    // -------------------------------------------------------------------------
    enum class CaptureMethod : int
    {
        Wgc = 0,
        Dxgi = 1
    };

    enum class Scaler
    {
        Bicubic,
        Bilinear
    };

    enum class Enc
    {
        Auto = 0,
        MfQsv = 5,
        MfVp12 = 6,
        MfNvenc = 7,
        MfAmf = 8,
        MfSoftware = 11
    };

    enum class Rate
    {
        Cbr,
        Vbr
    };

    enum class CodecSel
    {
        Auto,
        Hevc,
        H264,
        Av1
    };

    // Seven user-facing quality/speed levels, ultra fast -> slowest. The
    // ordinal is the value passed through IR_Start's pref bits 12..14.
    enum class Preset
    {
        UltraFast = 0,
        VeryFast,
        Fast,
        Balanced,
        Slow,
        Slower,
        Slowest
    };

    enum class MfCodec
    {
        H264,
        Hevc,
        Av1
    };

    enum class MfVendor
    {
        Qsv,      // Intel Quick Sync MFT
        Vp12,     // Intel Gen12 / Arc / Core Ultra MFT
        Nvenc,    // NVIDIA MFT
        Amf,      // AMD MFT
        Software  // Microsoft system-memory MFT
    };

    enum class SaveState : int
    {
        Queued = 0,
        Running = 1,
        Done = 2
    };

    struct VideoOptions
    {
        Enc enc = Enc::Auto;
        CodecSel codec = CodecSel::Auto;
        Rate rate = Rate::Cbr;
        Preset preset = Preset::Balanced;
        int w = 0, h = 0, fps = 30;
        uint32_t bps = 0;
        uint32_t avail = 0;
        bool segmented = false;
    };

    // -------------------------------------------------------------------------
    // Intrusive, pooled data-plane primitives.
    // -------------------------------------------------------------------------
    class RefCounted;

    class PoolBase
    {
    public:
        virtual ~PoolBase() = default;
        virtual void recycle(RefCounted *object) noexcept = 0;
    };

    class RefCounted
    {
    public:
        RefCounted() = default;
        RefCounted(const RefCounted &) = delete;
        RefCounted &operator=(const RefCounted &) = delete;
        virtual ~RefCounted() = default;

        void add_ref() noexcept { refs_.fetch_add(1, std::memory_order_relaxed); }
        bool release() noexcept { return refs_.fetch_sub(1, std::memory_order_acq_rel) == 1; }
        uint32_t use_count() const noexcept { return refs_.load(std::memory_order_relaxed); }
        void reset_refs() noexcept { refs_.store(1, std::memory_order_relaxed); }
        void attach_pool(PoolBase *pool) noexcept { pool_ = pool; }
        virtual void on_recycle() noexcept {}
        virtual void recycle_self() noexcept
        {
            if (pool_)
                pool_->recycle(this);
            else
                delete this;
        }

    private:
        std::atomic<uint32_t> refs_{1};
        PoolBase *pool_ = nullptr;
    };

    template <class T>
    class Ref
    {
    public:
        Ref() noexcept = default;
        Ref(T *p, bool add_ref = true) noexcept : p_(p)
        {
            if (p_ && add_ref)
                p_->add_ref();
        }
        Ref(const Ref &other) noexcept : p_(other.p_)
        {
            if (p_)
                p_->add_ref();
        }
        Ref(Ref &&other) noexcept : p_(other.p_) { other.p_ = nullptr; }
        Ref &operator=(const Ref &other) noexcept
        {
            if (this != &other)
                reset(other.p_, true);
            return *this;
        }
        Ref &operator=(Ref &&other) noexcept
        {
            if (this != &other)
            {
                reset(nullptr);
                p_ = other.p_;
                other.p_ = nullptr;
            }
            return *this;
        }
        ~Ref() { reset(); }

        T *get() const noexcept { return p_; }
        T *operator->() const noexcept { return p_; }
        T &operator*() const noexcept { return *p_; }
        explicit operator bool() const noexcept { return p_ != nullptr; }

        void reset(T *p = nullptr, bool add_ref = true) noexcept
        {
            if (p_ && p_->release())
                p_->recycle_self();
            p_ = p;
            if (p_ && add_ref)
                p_->add_ref();
        }

    private:
        T *p_ = nullptr;
    };

    template <class T>
    class ObjectPool final : public PoolBase
    {
    public:
        explicit ObjectPool(size_t count, size_t reserve_bytes = 0)
            : batch_(std::max<size_t>(1, count)), reserve_bytes_(reserve_bytes)
        {
            storage_.reserve(count);
            free_.reserve(count);
            grow(batch_);
        }

        Ref<T> acquire()
        {
            std::lock_guard<std::mutex> lock(mutex_);
            if (free_.empty())
            {
                // A replay ring legitimately holds ring_seconds * fps packets:
                // up to 120s * 120fps = 14400, far more than the initial batch.
                // Grow on demand rather than stalling the video thread when the
                // pool runs dry. On allocation failure, return an empty Ref so
                // the caller can report the error instead of crashing.
                try
                {
                    grow(batch_);
                }
                catch (...)
                {
                    return {};
                }
            }
            if (free_.empty())
                return {};
            T *obj = free_.back();
            free_.pop_back();
            return Ref<T>(obj, false); // refs are already 1
        }

        void recycle(RefCounted *base) noexcept override
        {
            T *obj = static_cast<T *>(base);
            obj->on_recycle();
            obj->reset_refs();
            std::lock_guard<std::mutex> lock(mutex_);
            free_.push_back(obj);
        }

        size_t available() const
        {
            std::lock_guard<std::mutex> lock(mutex_);
            return free_.size();
        }

    private:
        void grow(size_t count)
        {
            for (size_t i = 0; i < count; ++i)
            {
                auto obj = std::make_unique<T>(reserve_bytes_);
                obj->attach_pool(this);
                free_.push_back(obj.get());
                storage_.push_back(std::move(obj));
            }
        }

        std::vector<std::unique_ptr<T>> storage_;
        std::vector<T *> free_;
        size_t batch_ = 1;
        size_t reserve_bytes_ = 0;
        mutable std::mutex mutex_;
    };

    // Releases a WGC frame / DXGI resource on the last intrusive release.
    struct Keepalive : RefCounted
    {
        void (*destroy)(void *) = nullptr;
        void *context = nullptr;

        explicit Keepalive(size_t = 0) noexcept {}
        void set(void (*fn)(void *), void *ctx) noexcept
        {
            destroy = fn;
            context = ctx;
        }
        void on_recycle() noexcept override
        {
            destroy = nullptr;
            context = nullptr;
        }
    };

    // Pooled acknowledgement object for DXGI duplication. No raw semaphore
    // pointer crosses the capture/video boundary.
    struct FrameAck : RefCounted
    {
        explicit FrameAck(size_t = 0) noexcept {}
        void signal() noexcept
        {
            std::lock_guard<std::mutex> lock(mutex_);
            consumed_ = true;
            cv_.notify_one();
        }
        bool wait_for(std::chrono::milliseconds timeout) noexcept
        {
            std::unique_lock<std::mutex> lock(mutex_);
            return cv_.wait_for(lock, timeout, [&] { return consumed_; });
        }
        void on_recycle() noexcept override
        {
            std::lock_guard<std::mutex> lock(mutex_);
            consumed_ = false;
        }

    private:
        std::mutex mutex_;
        std::condition_variable cv_;
        bool consumed_ = false;
    };

    struct VideoFrame : RefCounted
    {
        explicit VideoFrame(size_t = 0) noexcept {}
        ComPtr<ID3D11Texture2D> texture;
        D3D11_BOX box{};
        int64_t pts100ns = 0;
        Ref<Keepalive> keepalive;
        Ref<FrameAck> ack;

        void on_recycle() noexcept override
        {
            texture.Reset();
            box = {};
            pts100ns = 0;
            keepalive.reset();
            ack.reset();
        }
    };

    // A GpuFrame keeps the exact pipeline slot it was acquired for. The slot is
    // returned by the encoder's slot lease on the GPU path and by the
    // MappedFrame destructor on the CPU path; it is never renumbered.
    struct GpuFrame : RefCounted
    {
        explicit GpuFrame(size_t = 0) noexcept {}
        uint32_t slot = 0;
        ComPtr<ID3D11Texture2D> nv12;
        ComPtr<ID3D11Texture2D> staging;
        ComPtr<ID3D11Texture2D> plane_y;
        ComPtr<ID3D11Texture2D> plane_uv;

        void on_recycle() noexcept override
        {
            // `slot` is a permanent identity: it must survive recycling so the
            // pool returns the exact lease to the same pipeline slot/AVFrame.
            nv12.Reset();
            staging.Reset();
            plane_y.Reset();
            plane_uv.Reset();
        }
    };

    // Invalid on failure (operator bool == false). The destructor unmaps outside
    // any D3D11Multithread guard and releases the CPU-path slot lease.
    class MappedFrame
    {
    public:
        MappedFrame() noexcept = default;
        MappedFrame(void *owner, int slot, void (*unmap)(void *, int), Ref<GpuFrame> frame,
                    std::span<const uint8_t> y, std::span<const uint8_t> uv,
                    size_t y_stride, size_t uv_stride) noexcept
            : owner_(owner), slot_(slot), unmap_(unmap), frame_(std::move(frame)), y_(y), uv_(uv),
              y_stride_(y_stride), uv_stride_(uv_stride)
        {
        }
        MappedFrame(const MappedFrame &) = delete;
        MappedFrame &operator=(const MappedFrame &) = delete;
        MappedFrame(MappedFrame &&other) noexcept
            : owner_(other.owner_), slot_(other.slot_), unmap_(other.unmap_), frame_(std::move(other.frame_)),
              y_(other.y_), uv_(other.uv_), y_stride_(other.y_stride_), uv_stride_(other.uv_stride_)
        {
            other.owner_ = nullptr;
            other.unmap_ = nullptr;
            other.y_ = {};
            other.uv_ = {};
            other.y_stride_ = 0;
            other.uv_stride_ = 0;
        }
        MappedFrame &operator=(MappedFrame &&other) noexcept
        {
            if (this != &other)
            {
                unmap_now();
                owner_ = other.owner_;
                slot_ = other.slot_;
                unmap_ = other.unmap_;
                frame_ = std::move(other.frame_);
                y_ = other.y_;
                uv_ = other.uv_;
                y_stride_ = other.y_stride_;
                uv_stride_ = other.uv_stride_;
                other.owner_ = nullptr;
                other.unmap_ = nullptr;
                other.y_ = {};
                other.uv_ = {};
                other.y_stride_ = 0;
                other.uv_stride_ = 0;
            }
            return *this;
        }
        ~MappedFrame() { unmap_now(); }

        explicit operator bool() const noexcept { return unmap_ != nullptr && static_cast<bool>(frame_); }
        std::span<const uint8_t> y() const noexcept { return y_; }
        std::span<const uint8_t> uv() const noexcept { return uv_; }
        size_t y_stride() const noexcept { return y_stride_; }
        size_t uv_stride() const noexcept { return uv_stride_; }

    private:
        void unmap_now() noexcept
        {
            if (unmap_ && owner_)
                unmap_(owner_, slot_);
            owner_ = nullptr;
            unmap_ = nullptr;
            y_ = {};
            uv_ = {};
            y_stride_ = 0;
            uv_stride_ = 0;
            frame_.reset();
        }

        void *owner_ = nullptr;
        int slot_ = -1;
        void (*unmap_)(void *, int) = nullptr;
        Ref<GpuFrame> frame_;
        std::span<const uint8_t> y_;
        std::span<const uint8_t> uv_;
        size_t y_stride_ = 0;
        size_t uv_stride_ = 0;
    };

    // One contiguous muxer sample. The byte span always points into owned_.
    struct PacketRef : RefCounted
    {
        explicit PacketRef(size_t reserve = 0)
        {
            if (reserve)
                owned_.reserve(reserve);
        }
        std::vector<uint8_t> owned_;
        std::span<const uint8_t> payload;
        // Number of leading payload bytes that are codec configuration rather
        // than sample data. The writer consumes these when configuring streams.
        size_t codec_data = 0;
        bool keyframe = false;
        int64_t pts100ns = 0;

        void set(std::span<const uint8_t> bytes, bool key, int64_t pts, size_t config_bytes = 0) noexcept
        {
            payload = bytes;
            codec_data = config_bytes;
            keyframe = key;
            pts100ns = pts;
        }
        void on_recycle() noexcept override
        {
            owned_.clear();
            payload = {};
            codec_data = 0;
            keyframe = false;
            pts100ns = 0;
        }
    };

    struct AudioBlock : RefCounted
    {
        explicit AudioBlock(size_t reserve_frames = 8192) { pcm.reserve(reserve_frames * AUDIO_CHANNELS); }
        std::vector<int16_t> pcm;
        uint32_t frames = 0;
        int64_t pts100ns = 0;

        void set(const int16_t *data, uint32_t n, int64_t pts)
        {
            pcm.assign(data, data + n);
            frames = n;
            pts100ns = pts;
        }
        std::span<const int16_t> samples() const noexcept { return {pcm.data(), pcm.size()}; }
        void on_recycle() noexcept override
        {
            pcm.clear();
            frames = 0;
            pts100ns = 0;
        }
    };

    // Transient encoder output. PacketRef is the only stored data-plane type.
    struct EncodedFrame
    {
        std::vector<uint8_t> data;
        bool keyframe = false;
        int64_t pts100ns = 0;
    };

    // -------------------------------------------------------------------------
    // Fixed-capacity queue. No allocations after construction; stop-aware.
    // -------------------------------------------------------------------------
    template <class T>
    class SpscQueue
    {
    public:
        explicit SpscQueue(size_t capacity) : slots_(capacity), capacity_(capacity) {}

        bool push(T value)
        {
            std::unique_lock<std::mutex> lock(mutex_);
            not_full_.wait(lock, [&] { return count_ < capacity_ || stopped_; });
            if (stopped_ || count_ == capacity_)
                return false;
            slots_[tail_] = std::move(value);
            tail_ = (tail_ + 1) % capacity_;
            ++count_;
            lock.unlock();
            not_empty_.notify_one();
            return true;
        }

        bool try_push(T value)
        {
            std::unique_lock<std::mutex> lock(mutex_);
            if (stopped_ || count_ == capacity_)
                return false;
            slots_[tail_] = std::move(value);
            tail_ = (tail_ + 1) % capacity_;
            ++count_;
            lock.unlock();
            not_empty_.notify_one();
            return true;
        }

        bool pop(T &out)
        {
            std::unique_lock<std::mutex> lock(mutex_);
            not_empty_.wait(lock, [&] { return count_ != 0 || stopped_; });
            if (count_ == 0)
                return false;
            out = std::move(slots_[head_]);
            head_ = (head_ + 1) % capacity_;
            --count_;
            lock.unlock();
            not_full_.notify_one();
            return true;
        }

        bool try_pop(T &out)
        {
            std::unique_lock<std::mutex> lock(mutex_);
            if (count_ == 0)
                return false;
            out = std::move(slots_[head_]);
            head_ = (head_ + 1) % capacity_;
            --count_;
            lock.unlock();
            not_full_.notify_one();
            return true;
        }

        void stop()
        {
            {
                std::lock_guard<std::mutex> lock(mutex_);
                stopped_ = true;
            }
            not_full_.notify_all();
            not_empty_.notify_all();
        }

        void reset()
        {
            std::lock_guard<std::mutex> lock(mutex_);
            head_ = tail_ = count_ = 0;
            stopped_ = false;
        }

        size_t size() const
        {
            std::lock_guard<std::mutex> lock(mutex_);
            return count_;
        }

    private:
        std::vector<T> slots_;
        size_t capacity_ = 0;
        size_t head_ = 0, tail_ = 0, count_ = 0;
        bool stopped_ = false;
        mutable std::mutex mutex_;
        std::condition_variable not_full_, not_empty_;
    };

    // -------------------------------------------------------------------------
    // Shared clock. Video/application audio follow pause; mic stays continuous.
    // -------------------------------------------------------------------------
    class Clock
    {
    public:
        void start(int64_t qpc, bool mic_continuous)
        {
            std::lock_guard<std::mutex> lock(mutex_);
            start_ = qpc;
            paused_total_ = 0;
            pause_began_ = 0;
            resume_ = 0;
            paused_ = false;
            mic_continuous_ = mic_continuous;
        }

        int64_t timeline(int64_t qpc) const
        {
            std::lock_guard<std::mutex> lock(mutex_);
            if (paused_ || qpc < resume_)
                return -1;
            const int64_t t = qpc - start_ - paused_total_;
            return t < 0 ? 0 : t;
        }

        int64_t mic_timeline(int64_t qpc) const
        {
            std::lock_guard<std::mutex> lock(mutex_);
            const int64_t t = qpc - start_;
            return t < 0 ? -1 : t;
        }

        void set_paused(bool paused)
        {
            std::lock_guard<std::mutex> lock(mutex_);
            const int64_t now = qpc100ns();
            if (paused && !paused_)
            {
                paused_ = true;
                pause_began_ = now;
            }
            else if (!paused && paused_)
            {
                paused_ = false;
                if (!mic_continuous_)
                    paused_total_ += now - pause_began_;
                resume_ = now;
            }
        }

        int64_t start_qpc() const
        {
            std::lock_guard<std::mutex> lock(mutex_);
            return start_;
        }

    private:
        mutable std::mutex mutex_;
        int64_t start_ = 0, paused_total_ = 0, pause_began_ = 0, resume_ = 0;
        bool paused_ = false, mic_continuous_ = false;
    };

    // -------------------------------------------------------------------------
    // Stop-aware control bus. Teardown drains it and ACKs every pending command
    // with IR_FAIL; no setter can block forever on an abandoned command.
    // -------------------------------------------------------------------------
    enum class ControlKind : int
    {
        SwitchSource,
        Pause,
        ReconfigureCanvas,
        SetNeedClear,
        Stop
    };

    struct ControlCommand
    {
        explicit ControlCommand(ControlKind k) : kind(k) {}
        ControlKind kind;
        int values[4]{};
        HWND window = nullptr;
        std::atomic<int> result{IR_FAIL};
        std::binary_semaphore ack{0};
    };

    class ControlBus
    {
    public:
        std::shared_ptr<ControlCommand> post(ControlKind kind, int a = 0, int b = 0, int c = 0, int d = 0,
                                             HWND window = nullptr)
        {
            auto cmd = std::make_shared<ControlCommand>(kind);
            cmd->values[0] = a;
            cmd->values[1] = b;
            cmd->values[2] = c;
            cmd->values[3] = d;
            cmd->window = window;
            {
                std::lock_guard<std::mutex> lock(mutex_);
                if (stopped_)
                {
                    cmd->result.store(IR_FAIL);
                    cmd->ack.release();
                    return cmd;
                }
                queue_.push_back(cmd);
            }
            not_empty_.notify_one();
            return cmd;
        }

        std::shared_ptr<ControlCommand> pop()
        {
            std::unique_lock<std::mutex> lock(mutex_);
            not_empty_.wait(lock, [&] { return !queue_.empty() || stopped_; });
            if (queue_.empty())
                return {};
            auto cmd = queue_.front();
            queue_.pop_front();
            return cmd;
        }

        static int wait(const std::shared_ptr<ControlCommand> &cmd, std::chrono::milliseconds timeout = std::chrono::milliseconds(1000))
        {
            if (!cmd)
                return IR_FAIL;
            if (cmd->ack.try_acquire_for(timeout))
                return cmd->result.load();
            return IR_FAIL;
        }

        static void complete(const std::shared_ptr<ControlCommand> &cmd, int result)
        {
            if (!cmd)
                return;
            cmd->result.store(result);
            cmd->ack.release();
        }

        void stop_and_drain()
        {
            std::deque<std::shared_ptr<ControlCommand>> pending;
            {
                std::lock_guard<std::mutex> lock(mutex_);
                stopped_ = true;
                pending.swap(queue_);
            }
            not_empty_.notify_all();
            for (auto &cmd : pending)
                complete(cmd, IR_FAIL);
        }

        void reset()
        {
            std::lock_guard<std::mutex> lock(mutex_);
            queue_.clear();
            stopped_ = false;
        }

        bool stopped() const
        {
            std::lock_guard<std::mutex> lock(mutex_);
            return stopped_;
        }

    private:
        std::deque<std::shared_ptr<ControlCommand>> queue_;
        bool stopped_ = false;
        mutable std::mutex mutex_;
        std::condition_variable not_empty_;
    };

    // -------------------------------------------------------------------------
    // GPU interfaces. Implementations live in video.cpp.
    // -------------------------------------------------------------------------
    struct GpuContext
    {
        ComPtr<ID3D11Device> dev;
        ComPtr<ID3D11DeviceContext> ctx;
        ComPtr<ID3D11Multithread> mt;
    };

    class ScopedMt
    {
    public:
        explicit ScopedMt(const GpuContext &gpu) noexcept : mt_(gpu.mt.Get())
        {
            if (mt_)
                mt_->Enter();
        }
        ~ScopedMt()
        {
            if (mt_)
                mt_->Leave();
        }
        ScopedMt(const ScopedMt &) = delete;
        ScopedMt &operator=(const ScopedMt &) = delete;

    private:
        ID3D11Multithread *mt_ = nullptr;
    };

    HRESULT create_gpu_device(IDXGIAdapter *adapter, GpuContext *out);
    bool device_luid(ID3D11Device *dev, LUID *luid);

    class GpuCanvas
    {
    public:
        GpuCanvas();
        ~GpuCanvas();
        GpuCanvas(GpuCanvas &&) noexcept;
        GpuCanvas &operator=(GpuCanvas &&) noexcept;
        GpuCanvas(const GpuCanvas &) = delete;
        GpuCanvas &operator=(const GpuCanvas &) = delete;

        HRESULT create(const GpuContext &gpu, uint32_t w, uint32_t h);
        void submit(ID3D11Texture2D *src, const D3D11_BOX &box, uint32_t dx, uint32_t dy, bool clear);
        void snapshot_into(ID3D11Texture2D *dst);
        uint32_t width() const noexcept;
        uint32_t height() const noexcept;
        const GpuContext &gpu() const noexcept;

    private:
        struct Impl;
        std::unique_ptr<Impl> p_;
    };

    class VideoPipeline
    {
    public:
        static constexpr int kSlots = kVideoSlots;

        VideoPipeline();
        ~VideoPipeline();
        VideoPipeline(VideoPipeline &&) noexcept;
        VideoPipeline &operator=(VideoPipeline &&) noexcept;
        VideoPipeline(const VideoPipeline &) = delete;
        VideoPipeline &operator=(const VideoPipeline &) = delete;

        HRESULT create(const GpuContext &gpu, uint32_t canvas_w, uint32_t canvas_h,
                       uint32_t out_w, uint32_t out_h, bool hd, Scaler scaler);
        void render(ID3D11ShaderResourceView *source, uint32_t slot, bool stage_copy);
        bool has_nv12_texture() const noexcept;
        ID3D11Texture2D *nv12_texture(uint32_t slot) const noexcept;
        UINT nv12_bind_flags() const noexcept;
        MappedFrame map(uint32_t slot, Ref<GpuFrame> frame) noexcept;
        uint32_t out_w() const noexcept;
        uint32_t out_h() const noexcept;
        bool uses_plane_views() const noexcept;
        void flush_and_wait();

        // Slot pool: returns the exact slot requested by the caller; never renumbers.
        Ref<GpuFrame> acquire_slot(uint32_t slot);

    private:
        struct Impl;
        std::unique_ptr<Impl> p_;
    };

    static_assert(VideoPipeline::kSlots == 6, "slot count is frozen at 6");

    // -------------------------------------------------------------------------
    // Encoder interfaces. Implementations live in video.cpp.
    // -------------------------------------------------------------------------
    struct VideoSelection
    {
        MfCodec mf = MfCodec::H264;
        MfVendor mf_vendor = MfVendor::Qsv;
    };

    class MfEncoder
    {
    public:
        ~MfEncoder();
        MfEncoder(MfEncoder &&) noexcept;
        MfEncoder &operator=(MfEncoder &&) noexcept;
        MfEncoder(const MfEncoder &) = delete;
        MfEncoder &operator=(const MfEncoder &) = delete;

        static std::unique_ptr<MfEncoder> create(MfCodec codec, MfVendor vendor, const GpuContext *gpu, int w, int h, int fps,
                                                 uint32_t bps, Rate rate, Preset preset, ObjectPool<PacketRef> *packets,
                                                 std::string *err);
        bool encode_texture(ID3D11Texture2D *nv12, int64_t pts100ns, bool idr,
                            std::vector<Ref<PacketRef>> &out, std::string *err);
        bool encode_memory(const uint8_t *y, size_t y_stride, const uint8_t *uv, size_t uv_stride,
                           int64_t pts100ns, bool idr, std::vector<Ref<PacketRef>> &out, std::string *err);
        bool flush(std::vector<Ref<PacketRef>> &out, std::string *err);
        int kind_code() const noexcept;
        bool accepts_textures() const noexcept;
        HRESULT init_hr() const noexcept;

    private:
        MfEncoder() = default;
        struct Impl;
        std::unique_ptr<Impl> p_;
    };

    VideoOptions options_from_pref(int pref, int w, int h, int fps, uint32_t bps);
    bool select_video(const VideoOptions &o, VideoSelection *out, std::string *err);
    bool probe_mf_encoder(MfCodec codec, MfVendor vendor, int w, int h, int fps, uint32_t bps, HRESULT *hr_out,
                          std::string *err = nullptr);
    uint32_t mf_availability_bit(MfCodec codec, MfVendor vendor);
    const char *mf_vendor_label(MfVendor vendor);

    // Stateful PCM -> 48 kHz stereo s16 conversion. One instance lives for the
    // whole WASAPI stream; recreating an MF resampler per packet caused clicks.
    class PcmToS16Converter
    {
    public:
        PcmToS16Converter();
        ~PcmToS16Converter();

        bool initialize(int src_rate, int src_channels, bool src_float, int src_bits, std::string *err = nullptr);
        bool convert(const uint8_t *src, uint32_t frames, std::vector<int16_t> *out,
                     std::string *err = nullptr);
        void reset(); // discard resampler state after a WASAPI discontinuity

    private:
        struct Impl;
        std::unique_ptr<Impl> impl_;
    };

    struct VideoSample
    {
        const uint8_t *data;
        size_t size;
        size_t codec_data; // leading configuration bytes (see PacketRef)
        bool keyframe;
        int64_t pts100ns;
    };
    struct AudioChunk
    {
        const uint8_t *data;
        uint32_t frames;
        int64_t pts100ns;
    };

    int write_mf_file(const std::wstring &path, const std::vector<Ref<PacketRef>> &video,
                      const std::vector<AudioChunk> &audio, CodecSel codec, int fps, int w, int h, int seconds_back,
                      int64_t end_time, bool segmented);

    // Rejects unsupported encoder/codec/preset combinations before capture starts.
    bool validate_video_selection(const VideoOptions &o, std::string *err);

    // -------------------------------------------------------------------------
    // Capture interfaces. Whole-screen and window capture stay separate paths.
    // -------------------------------------------------------------------------
    struct MonInfo
    {
        HMONITOR handle = nullptr;
        RECT rect{};
        int w = 0, h = 0;
        bool primary = false;
    };

    struct WindowInfo
    {
        HWND handle = nullptr;
        std::wstring title;
        int w = 0, h = 0;
    };

    std::vector<MonInfo> enumerate_monitors();
    std::vector<WindowInfo> enumerate_windows();
    bool window_size(HWND window, bool client, int *w, int *h);
    ComPtr<IDXGIAdapter> adapter_for_monitor(HMONITOR monitor);
    bool dxgi_duplication_supported();

    struct CaptureTarget
    {
        HWND window = nullptr;
        HMONITOR monitor = nullptr;
        uint32_t options = CAP_CURSOR | CAP_KEEP_CORNERS;
        int fps = 60;
    };

    using FrameSink = std::function<void(Ref<VideoFrame>)>;

    class CaptureSource
    {
    public:
        explicit CaptureSource(FrameSink sink) : sink_(std::move(sink)) {}
        virtual ~CaptureSource() = default;
        virtual HRESULT start() = 0;
        virtual void stop() = 0;

        HRESULT last_hr() const { return last_hr_.load(); }
        bool closed() const { return closed_.load(); }
        int stage() const { return stage_.load(); }
        int64_t paced_frames() const { return paced_.load(); }
        int64_t rejected_frames() const { return rejected_.load(); }

    protected:
        void emit(Ref<VideoFrame> frame)
        {
            if (frame && sink_)
                sink_(std::move(frame));
        }

        FrameSink sink_;
        std::atomic<int64_t> paced_{0};
        std::atomic<int64_t> rejected_{0};
        std::atomic<HRESULT> last_hr_{S_OK};
        std::atomic<bool> closed_{false};
        std::atomic<int> stage_{0};
    };

    std::unique_ptr<CaptureSource> make_wgc_capture(const GpuContext &gpu, const CaptureTarget &target,
                                                     ObjectPool<VideoFrame> *frames,
                                                     ObjectPool<Keepalive> *keepalives, FrameSink sink);
    std::unique_ptr<CaptureSource> make_dxgi_capture(const GpuContext &gpu, const CaptureTarget &target,
                                                      ObjectPool<VideoFrame> *frames,
                                                      ObjectPool<Keepalive> *keepalives,
                                                      ObjectPool<FrameAck> *acks, FrameSink sink);

    // -------------------------------------------------------------------------
    // Audio interfaces. audio.cpp is a conservative port of the verified source.
    // -------------------------------------------------------------------------
    struct AudioCounters
    {
        std::atomic<int64_t> events{0};
        std::atomic<int64_t> frames{0};
        std::atomic<int64_t> got_frames{0};
        std::atomic<int64_t> fail_hr{0};
        std::atomic<int> peak{0};
        std::atomic<int64_t> activate_hr{0};
        std::atomic<int64_t> init_hr{0};
        std::atomic<int64_t> start_hr{0};
        std::atomic<int> mix_tag{0};
        std::atomic<int> mix_rate{0};
        std::atomic<int> mix_channels{0};
        std::atomic<int> mix_bits{0};
        std::atomic<int> mix_float{0};
        std::atomic<int> convert_failures{0};
        std::mutex name_mutex;
        std::wstring active_name;
    };

    using AudioPush = std::function<void(const int16_t *, uint32_t, int64_t, bool)>;

    class AudioSource
    {
    public:
        AudioSource(bool mic, std::wstring device_name, HWND target_window, AudioPush push, AudioCounters *counters);
        ~AudioSource();
        void start();
        void stop();

    private:
        void run();
        bool run_once();

        bool mic_ = false;
        std::wstring device_name_;
        HWND target_ = nullptr;
        AudioPush push_;
        AudioCounters *counters_ = nullptr;
        std::atomic<bool> running_{false};
        std::thread thread_;
    };

    std::vector<std::wstring> mic_devices();

    // -------------------------------------------------------------------------
    // Save control plane. shared_ptr is correct here; data plane uses Ref<>.
    // -------------------------------------------------------------------------
    struct SaveJob
    {
        uint64_t id = 0;
        std::wstring path;
        int secondsBack = 0;
        bool segmented = false;
        std::atomic<int> progress{0};
        std::atomic<int> result{IR_PENDING};
        std::atomic<SaveState> state{SaveState::Queued};
        std::binary_semaphore done{0};
    };

} // namespace ir
