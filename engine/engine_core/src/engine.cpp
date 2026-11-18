// Engine core: clock, ring buffers, capture -> canvas -> OBS-style GPU pipeline -> encoder thread,
// audio mixing, Save, and the exported C ABI used by InstantReplay.Native.
#include "audio.h"
#include "capture.h"
#include "ffmpeg_rt.h"
#include "gpu_pipeline.h"
#include "mf_encoder.h"

#include <cmath>
#include <limits>
#include <new>
#include <utility>

namespace ir {

namespace {

struct AudioBlock {
    std::vector<int16_t> pcm; // interleaved (stereo for system, mono for mic)
    uint32_t frames = 0;
    int64_t pts = 0;
};

uint32_t even_up(uint32_t v) { return (v + 1) & ~1u; }

// Balance every COM apartment we enter: CoInitializeEx only adds a reference for S_OK/S_FALSE
// (RPC_E_CHANGED_MODE means the thread is already in another apartment, so leave it alone).
struct ComGuard {
    HRESULT hr = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
    ~ComGuard() { if (hr == S_OK || hr == S_FALSE) CoUninitialize(); }
};

// Port of the old engine's fit_resolution(): keep the monitor aspect ratio and round down to even.
std::pair<uint32_t, uint32_t> fit_resolution(int desk_w, int desk_h, int cap_w, int cap_h) {
    int w = desk_w, h = desk_h;
    if (cap_w > 0 && cap_h > 0) {
        const double scale = std::min((double)cap_w / std::max(1, w), (double)cap_h / std::max(1, h));
        if (scale < 1.0) { w = (int)(w * scale); h = (int)(h * scale); }
    } else if (cap_w > 0) {
        const double scale = (double)cap_w / std::max(1, w);
        if (scale < 1.0) { h = (int)(h * scale); w = cap_w; }
    } else if (cap_h > 0) {
        const double scale = (double)cap_h / std::max(1, h);
        if (scale < 1.0) { w = (int)(w * scale); h = cap_h; }
    }
    return { (uint32_t)std::max(2, w / 2 * 2), (uint32_t)std::max(2, h / 2 * 2) };
}

// Same committed-memory reserve the Rust engine used before cloning the replay ring.
bool save_memory_available(uint64_t audio_bytes) {
    MEMORYSTATUSEX mem = {};
    mem.dwLength = sizeof(mem);
    if (!GlobalMemoryStatusEx(&mem)) return false;
    constexpr uint64_t reserve = 512ull * 1024 * 1024;
    const uint64_t max = std::numeric_limits<uint64_t>::max();
    const uint64_t workspace = audio_bytes > (max - reserve) / 3 ? max : audio_bytes * 3;
    return mem.ullAvailPageFile >= reserve + workspace;
}

Scaler scaler_from_env() {
    wchar_t buf[32];
    DWORD n = GetEnvironmentVariableW(L"IR_SCALER", buf, 32);
    if (n > 0 && n < 32 && _wcsicmp(buf, L"bilinear") == 0) return Scaler::Bilinear;
    return Scaler::Bicubic;
}

} // namespace

std::atomic<int> g_save_progress{-1};

class Engine {
public:
    Engine() = default;
    ~Engine() { stop(); }

    int start(int monitor_index, int ring_seconds, int fps, int bitrate_kbps, int cap_w, int cap_h, bool audio,
              bool mic, const wchar_t* mic_device, int pref, int method, int64_t* start_qpc);
    void stop();
    int set_target(HWND window, uint32_t options);
    int set_source(int method, int monitor_index, HWND window, uint32_t options);
    int set_canvas_hint(int w, int h);
    int set_paused(bool paused);
    int save(const std::string& path, int seconds_back);
    void status(int* buffering, int* ring_seconds, int64_t* encoded, int64_t* dropped, int* has_error, int64_t* a_frames,
                int64_t* a_events, int64_t* a_got, int64_t* heartbeat);
    void mic_status(int64_t* frames, int64_t* events, int64_t* got, int64_t* fail_hr, int* sys_peak, int* mic_peak,
                    wchar_t* name, int name_cap);

    int active_capture_method() const { return active_method_.load(); }
    int last_hresult() const { return last_hr_.load(); }
    int last_fail_stage() const { return fail_stage_.load(); }
    int last_fail_hr() const { return fail_hr_.load(); }
    int last_capture_hr() const { return capture_ ? (int)capture_->last_hr() : cap_hr_.load(); }
    int active_encoder() const { return active_encoder_.load(); }
    int hw_init_hr() const { return hw_init_hr_.load(); }

private:
    // ---- clock ----
    // Video and application audio: dropped while paused (and anything older than the resume instant).
    int64_t timeline(int64_t qpc) {
        std::lock_guard<std::mutex> lock(clock_mutex_);
        if (paused_ || qpc < resume_qpc_) return -1;
        int64_t t = qpc - start_qpc_ - paused_total_;
        return t < 0 ? 0 : t;
    }
    // Microphone: keeps its own continuous clock, so an enabled mic records straight through a pause.
    int64_t mic_timeline(int64_t qpc) const {
        int64_t t = qpc - start_qpc_;
        return t < 0 ? -1 : t;
    }

    void fail(int stage, HRESULT hr, const std::string& message) {
        fail_stage_ = stage;
        fail_hr_ = (int)hr;
        last_hr_ = (int)hr;
        has_error_ = true;
        if (!message.empty()) set_last_error(message);
    }

    HRESULT open_capture(const CaptureTarget& target, CaptureMethod method);
    void on_frame(const CapturedFrame& f);
    void video_thread();
    bool ensure_pipeline();
    bool encode_slot(int slot, int64_t pts, bool idr);
    void push_video(EncodedFrame&& f);
    void push_system(const int16_t* data, uint32_t frames, int64_t qpc);
    void push_mic(const int16_t* data, uint32_t frames, int64_t qpc);
    void flush_pending();
    void watch_target();
    void restart_system_audio(HWND window);

    std::mutex ctl_mutex_;
    std::mutex stop_mutex_; // serializes stop() so the video thread can take ctl_mutex_ while we join
    std::atomic<bool> running_{false};
    std::atomic<bool> stopping_{false};

    // config
    std::atomic<int> ring_seconds_{60};
    int fps_ = 30;
    uint32_t bps_ = 0;
    uint32_t out_w_ = 0, out_h_ = 0;
    int pref_ = 0;
    int monitor_index_ = 0;
    uint32_t options_ = CAP_CURSOR | CAP_KEEP_CORNERS;

    // clock
    std::mutex clock_mutex_;
    int64_t start_qpc_ = 0, paused_total_ = 0, pause_began_ = 0, resume_qpc_ = 0;
    bool paused_ = false;
    bool mic_continuous_ = false;

    // pending capture target (IR_SetCaptureTarget / IR_SetCanvasHint, honoured by the next start)
    HWND pending_window_ = nullptr;
    uint32_t pending_options_ = CAP_CLIENT_AREA | CAP_CURSOR | CAP_KEEP_CORNERS;
    uint32_t hint_w_ = 0, hint_h_ = 0;

    // gpu
    GpuDevice gpu_;
    std::mutex canvas_mutex_;
    GpuCanvas canvas_;
    uint32_t max_w_ = 0, max_h_ = 0;
    uint64_t canvas_gen_ = 0;
    uint32_t last_place_w_ = 0, last_place_h_ = 0, last_dx_ = 0, last_dy_ = 0;
    bool need_clear_ = true;
    int64_t latest_pts_ = -1;
    uint64_t frame_seq_ = 0;
    std::condition_variable_any frame_cv_;
    int64_t last_pts_ = -1;

    // capture
    std::mutex capture_mutex_;
    std::unique_ptr<CaptureSource> capture_;
    CaptureTarget target_;
    CaptureMethod method_ = CaptureMethod::Wgc;
    std::atomic<int> active_method_{0};
    std::atomic<int> cap_hr_{0};

    // encoder
    std::thread video_thread_;
    VideoSelection selection_;
    std::unique_ptr<MfEncoder> mf_;
    VideoOptions video_options_;
    ObsVideoPipeline pipe_;
    bool pipe_ready_ = false;
    uint64_t pipe_gen_ = ~0ull;
    ComPtr<ID3D11Texture2D> snap_;
    ComPtr<ID3D11ShaderResourceView> snap_srv_;
    struct Pending { int slot; int64_t pts; };
    std::deque<Pending> pending_;
    int next_slot_ = 0;
    bool first_frame_ = true;
    Scaler scaler_ = Scaler::Bicubic;

    // rings
    std::mutex ring_mutex_;
    std::deque<std::shared_ptr<EncodedFrame>> vring_;
    int64_t vbytes_ = 0;
    std::deque<AudioBlock> sys_ring_, mic_ring_;

    // audio
    std::unique_ptr<AudioSource> sys_audio_, mic_audio_;
    AudioCounters sys_counters_, mic_counters_;
    bool want_audio_ = false;
    std::wstring mic_device_;

    // stats
    std::atomic<int64_t> encoded_{0}, dropped_{0}, heartbeat_{0};
    std::atomic<bool> has_error_{false};
    std::atomic<int> last_hr_{0}, fail_stage_{0}, fail_hr_{0}, active_encoder_{0}, hw_init_hr_{0};
};

// ------------------------------------------------------------------------------------------------

HRESULT Engine::open_capture(const CaptureTarget& target, CaptureMethod method) {
    std::lock_guard<std::mutex> lock(capture_mutex_);
    if (capture_) {
        capture_->stop();
        capture_.reset();
    }
    target_ = target;
    auto cb = [this](const CapturedFrame& f) { on_frame(f); };
    auto try_method = [&](CaptureMethod m) -> HRESULT {
        auto src = m == CaptureMethod::Wgc ? make_wgc_capture(gpu_, target, cb) : make_dxgi_capture(gpu_, target, cb);
        HRESULT hr = src->start();
        if (SUCCEEDED(hr)) {
            capture_ = std::move(src);
            active_method_ = (int)m;
            method_ = m;
        } else {
            cap_hr_ = (int)hr;
            fail_stage_ = src->stage() ? src->stage() : STAGE_START_CAPTURE;
        }
        return hr;
    };
    HRESULT hr = try_method(method);
    if (FAILED(hr)) {
        // Fall back to the other method (e.g. WGC unavailable on this Windows build).
        HRESULT hr2 = try_method(method == CaptureMethod::Wgc ? CaptureMethod::Dxgi : CaptureMethod::Wgc);
        if (SUCCEEDED(hr2)) return hr2;
    }
    return hr;
}

void Engine::on_frame(const CapturedFrame& f) {
    if (!running_) return;
    const int64_t pts = timeline(f.pts100ns);
    if (pts < 0) return; // paused
    const uint32_t w = f.box.right - f.box.left, h = f.box.bottom - f.box.top;
    if (w < 2 || h < 2) return;

    std::lock_guard<std::mutex> lock(canvas_mutex_);
    const uint32_t nw = std::max(max_w_, w), nh = std::max(max_h_, h);
    if (!canvas_.width() || nw != max_w_ || nh != max_h_) {
        max_w_ = nw;
        max_h_ = nh;
        // Canvas keeps the output aspect ratio so scaling never distorts; the source is centred with black bars.
        uint32_t cw = max_w_, ch = max_h_;
        if ((uint64_t)cw * out_h_ < (uint64_t)ch * out_w_) cw = (uint32_t)(((uint64_t)ch * out_w_ + out_h_ - 1) / out_h_);
        else ch = (uint32_t)(((uint64_t)cw * out_h_ + out_w_ - 1) / out_w_);
        cw = even_up(cw);
        ch = even_up(ch);
        GpuCanvas fresh;
        if (FAILED(fresh.create(gpu_, cw, ch))) return;
        canvas_ = std::move(fresh);
        ++canvas_gen_;
        need_clear_ = true;
    }
    const uint32_t dx = ((canvas_.width() - w) / 2) & ~1u, dy = ((canvas_.height() - h) / 2) & ~1u;
    const bool moved = w != last_place_w_ || h != last_place_h_ || dx != last_dx_ || dy != last_dy_;
    canvas_.submit(f.texture, f.box, dx, dy, need_clear_ || moved);
    need_clear_ = false;
    last_place_w_ = w;
    last_place_h_ = h;
    last_dx_ = dx;
    last_dy_ = dy;
    if (pts <= latest_pts_) {
        ++dropped_; // two frames with the same timestamp: keep only the later content
        latest_pts_ += 1;
    } else {
        latest_pts_ = pts;
    }
    ++frame_seq_;
    frame_cv_.notify_all();
}

bool Engine::ensure_pipeline() {
    // Called with canvas_mutex_ held.
    if (pipe_ready_ && pipe_gen_ == canvas_gen_) return true;
    if (!canvas_.width()) return false;
    flush_pending();
    D3D11_TEXTURE2D_DESC d = {};
    d.Width = canvas_.width();
    d.Height = canvas_.height();
    d.MipLevels = d.ArraySize = 1;
    d.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
    d.SampleDesc.Count = 1;
    d.Usage = D3D11_USAGE_DEFAULT;
    d.BindFlags = D3D11_BIND_SHADER_RESOURCE;
    snap_.Reset();
    snap_srv_.Reset();
    HRESULT hr = gpu_.dev->CreateTexture2D(&d, nullptr, &snap_);
    if (SUCCEEDED(hr)) hr = gpu_.dev->CreateShaderResourceView(snap_.Get(), nullptr, &snap_srv_);
    if (SUCCEEDED(hr)) {
        pipe_ = ObsVideoPipeline();
        const bool hd = out_w_ >= 1280 || out_h_ > 576;
        hr = pipe_.create(gpu_, canvas_.width(), canvas_.height(), out_w_, out_h_, hd, scaler_);
    }
    if (FAILED(hr)) {
        fail(STAGE_ENCODER, hr, "Could not create the GPU video pipeline (0x" + std::to_string((unsigned)hr) + ").");
        return false;
    }
    pipe_ready_ = true;
    pipe_gen_ = canvas_gen_;
    next_slot_ = 0;
    return true;
}

void Engine::push_video(EncodedFrame&& f) {
    ++encoded_;
    auto frame = std::make_shared<EncodedFrame>(std::move(f));
    std::lock_guard<std::mutex> lock(ring_mutex_);
    vbytes_ += (int64_t)frame->data.size();
    const int64_t newest = frame->pts100ns;
    vring_.push_back(std::move(frame));
    const int64_t cutoff = newest - (int64_t)ring_seconds_.load() * SECOND_100NS;
    // Keep the last keyframe at or before the cutoff so a save can always start on a keyframe.
    long k = -1;
    for (size_t i = 0; i < vring_.size(); ++i) {
        if (vring_[i]->pts100ns > cutoff) break;
        if (vring_[i]->keyframe) k = (long)i;
    }
    for (long i = 0; i < k; ++i) {
        vbytes_ -= (int64_t)vring_.front()->data.size();
        vring_.pop_front();
    }
    while (vbytes_ > RING_BUDGET_BYTES && vring_.size() > 1) {
        size_t next_key = 1;
        while (next_key < vring_.size() && !vring_[next_key]->keyframe) ++next_key;
        if (next_key >= vring_.size()) break;
        for (size_t i = 0; i < next_key; ++i) {
            vbytes_ -= (int64_t)vring_.front()->data.size();
            vring_.pop_front();
        }
    }
}

bool Engine::encode_slot(int slot, int64_t pts, bool idr) {
    Nv12Mapped m;
    if (!pipe_.map(slot, &m)) {
        ++dropped_;
        return true;
    }
    std::vector<EncodedFrame> out;
    std::string err;
    bool ok;
    if (mf_) ok = mf_->encode_memory(m.y, m.y_stride, m.uv, m.uv_stride, pts, idr, out, &err);
    else ok = selection_.video->encode_nv12(m.y, m.y_stride, m.uv, m.uv_stride, pts, idr, out, &err);
    pipe_.unmap(slot);
    if (!ok) {
        fail(STAGE_ENCODER, E_FAIL, "Video encoder failed: " + err);
        return false;
    }
    for (auto& f : out) push_video(std::move(f));
    return true;
}

void Engine::flush_pending() {
    while (!pending_.empty()) {
        Pending p = pending_.front();
        pending_.pop_front();
        if (!encode_slot(p.slot, p.pts, first_frame_)) { running_ = false; return; }
        first_frame_ = false;
    }
}

void Engine::video_thread() {
    ComGuard com;
    uint64_t seen = 0;
    int64_t last_alive = qpc100ns();
    while (running_) {
        ++heartbeat_;
        bool have_new = false;
        int64_t pts = 0;
        {
            std::unique_lock<std::mutex> lock(canvas_mutex_);
            frame_cv_.wait_for(lock, std::chrono::milliseconds(20), [&] { return frame_seq_ != seen || !running_; });
            if (!running_) break;
            if (frame_seq_ != seen) {
                seen = frame_seq_;
                pts = latest_pts_;
                if (ensure_pipeline()) {
                    canvas_.snapshot_into(snap_.Get());
                    have_new = true;
                } else {
                    const HRESULT hr = has_error_ ? (HRESULT)last_hr_.load() : E_FAIL;
                    const std::string message = has_error_ ? last_error() : "Video pipeline could not be created.";
                    running_ = false;
                    frame_cv_.notify_all();
                    fail(STAGE_ENCODER, hr, message);
                    break;
                }
            }
        }
        if (have_new) {
            const int slot = next_slot_;
            next_slot_ = (next_slot_ + 1) % ObsVideoPipeline::kSlots;
            pipe_.render(snap_srv_.Get(), slot);
            pending_.push_back({slot, pts});
            // One-frame delay before mapping, like OBS's staging ring, so Map() never stalls the GPU.
            while (pending_.size() > 1) {
                Pending p = pending_.front();
                pending_.pop_front();
                if (!encode_slot(p.slot, p.pts, first_frame_)) { running_ = false; break; }
                first_frame_ = false;
            }
        } else if (!pending_.empty()) {
            Pending p = pending_.front();
            pending_.pop_front();
            if (!encode_slot(p.slot, p.pts, first_frame_)) { running_ = false; break; }
            first_frame_ = false;
        }
        const int64_t now = qpc100ns();
        if (now - last_alive > SECOND_100NS / 4) {
            last_alive = now;
            watch_target();
        }
    }
    // Drain the one-frame-delayed staging slot and flush the active encoder before teardown,
    // otherwise the final B-frames never reach the ring.
    if (mf_ || selection_.video) {
        if (pipe_ready_) flush_pending();
        std::vector<EncodedFrame> out;
        std::string err;
        const bool ok = mf_ ? mf_->flush(out, &err) : selection_.video->flush(out, &err);
        if (ok) {
            for (auto& f : out) push_video(std::move(f));
        } else {
            fail(STAGE_ENCODER, E_FAIL, "Video encoder flush failed: " + err);
        }
    }
}

void Engine::restart_system_audio(HWND window) {
    if (!want_audio_) return;
    if (sys_audio_) sys_audio_->stop();
    sys_audio_ = std::make_unique<AudioSource>(false, L"", window,
        [this](const int16_t* d, uint32_t n, int64_t q) { push_system(d, n, q); }, &sys_counters_);
    sys_audio_->start();
}

void Engine::watch_target() {
    std::lock_guard<std::mutex> ctl(ctl_mutex_);
    if (!running_) return;
    // The captured window closed: keep recording by switching to the whole monitor.
    CaptureSource* cap = nullptr;
    {
        std::lock_guard<std::mutex> lock(capture_mutex_);
        cap = capture_.get();
        if (!cap || !cap->closed()) return;
    }
    auto mons = enumerate_monitors();
    CaptureTarget t;
    t.fps = fps_;
    t.options = options_ & ~CAP_CLIENT_AREA;
    if (!mons.empty()) t.monitor = mons[(size_t)std::clamp(monitor_index_, 0, (int)mons.size() - 1)].handle;
    open_capture(t, method_);
    if (running_) restart_system_audio(nullptr);
}

void Engine::push_system(const int16_t* data, uint32_t frames, int64_t qpc) {
    const int64_t pts = timeline(qpc);
    if (pts < 0 || !running_) return;
    AudioBlock b;
    b.pcm.assign(data, data + (size_t)frames * 2);
    b.frames = frames;
    b.pts = pts;
    std::lock_guard<std::mutex> lock(ring_mutex_);
    sys_ring_.push_back(std::move(b));
    const int64_t cutoff = pts - ((int64_t)ring_seconds_.load() + 3) * SECOND_100NS;
    while (!sys_ring_.empty() && sys_ring_.front().pts < cutoff) sys_ring_.pop_front();
}

void Engine::push_mic(const int16_t* data, uint32_t frames, int64_t qpc) {
    const int64_t pts = mic_timeline(qpc);
    if (pts < 0 || !running_) return;
    AudioBlock b;
    b.pcm.assign(data, data + frames);
    b.frames = frames;
    b.pts = pts;
    std::lock_guard<std::mutex> lock(ring_mutex_);
    mic_ring_.push_back(std::move(b));
    const int64_t cutoff = pts - ((int64_t)ring_seconds_.load() + 3) * SECOND_100NS;
    while (!mic_ring_.empty() && mic_ring_.front().pts < cutoff) mic_ring_.pop_front();
}

// ------------------------------------------------------------------------------------------------

int Engine::start(int monitor_index, int ring_seconds, int fps, int bitrate_kbps, int cap_w, int cap_h, bool audio,
                  bool mic, const wchar_t* mic_device, int pref, int method, int64_t* start_qpc) {
    std::unique_lock<std::mutex> ctl(ctl_mutex_);
    if (stopping_) return IR_FAIL;
    if (running_) return IR_OK;
    set_last_error("");
    has_error_ = false;
    fail_stage_ = fail_hr_ = last_hr_ = cap_hr_ = hw_init_hr_ = 0;
    encoded_ = dropped_ = heartbeat_ = 0;

    ComGuard com;
    if (!ff_load()) {
        fail(STAGE_FFMPEG, E_FAIL, "");
        return IR_FFMPEG_UNAVAILABLE;
    }

    auto mons = enumerate_monitors();
    if (mons.empty()) {
        fail(STAGE_CREATE_ITEM, E_FAIL, "No display was found to capture.");
        return IR_FAIL;
    }
    monitor_index_ = (monitor_index < 0 || (size_t)monitor_index >= mons.size()) ? 0 : monitor_index;
    const MonInfo& mon = mons[(size_t)monitor_index_];

    ring_seconds_.store(std::max(5, ring_seconds));
    fps_ = std::clamp(fps, 1, 240);
    bps_ = (uint32_t)std::max(250, bitrate_kbps) * 1000u;
    const auto [out_w, out_h] = fit_resolution(mon.w, mon.h, cap_w, cap_h);
    out_w_ = out_w;
    out_h_ = out_h;
    pref_ = pref;
    want_audio_ = audio;
    scaler_ = scaler_from_env();

    video_options_ = options_from_pref(pref, (int)out_w_, (int)out_h_, fps_, bps_);
    std::string err;
    selection_ = VideoSelection();
    if (!select_video(video_options_, true, &selection_, &err)) {
        fail(STAGE_ENCODER, E_FAIL, err);
        return IR_FAIL;
    }

    // One device on the adapter that owns the monitor: capture, scaling and conversion all stay on it.
    ComPtr<IDXGIAdapter> adapter = adapter_for_monitor(mon.handle);
    HRESULT hr = create_gpu_device(adapter.Get(), &gpu_);
    if (FAILED(hr)) hr = create_gpu_device(nullptr, &gpu_);
    if (FAILED(hr)) {
        fail(STAGE_CREATE_DEVICE, hr, "Could not create a Direct3D 11 device.");
        return IR_FAIL;
    }

    mf_.reset();
    if (selection_.kind == VideoSelection::MediaFoundation) {
        err.clear();
        mf_ = MfEncoder::create(selection_.mf, &gpu_, (int)out_w_, (int)out_h_, fps_, bps_,
                                video_options_.rate, video_options_.preset, &err);
        if (!mf_) {
            hw_init_hr_ = E_FAIL;
            if (video_options_.enc == Enc::Auto && selection_.mf == MfCodec::H264) {
                // MF is only an Auto fallback: when its transform cannot be created, H.264 can still
                // use libx264. HEVC stays hardware-only and reports the MF failure below.
                VideoOptions fallback = video_options_;
                fallback.avail &= ~ENC_H264_MF;
                std::string fallback_err;
                selection_ = VideoSelection();
                if (select_video(fallback, false, &selection_, &fallback_err) && selection_.video) {
                    hw_init_hr_ = 0;
                    active_encoder_ = selection_.video->kind_code();
                } else {
                    fail(STAGE_ENCODER, E_FAIL, "Media Foundation H.264 encoder failed (" + err +
                                                    "); software fallback failed: " + fallback_err);
                    return IR_FAIL;
                }
            } else {
                fail(STAGE_ENCODER, E_FAIL, err.empty() ? "Media Foundation encoder could not be created." : err);
                return IR_FAIL;
            }
        } else {
            active_encoder_ = mf_->kind_code();
        }
    } else {
        active_encoder_ = selection_.video->kind_code();
    }

    {
        std::lock_guard<std::mutex> lock(ring_mutex_);
        vring_.clear();
        sys_ring_.clear();
        mic_ring_.clear();
        vbytes_ = 0;
    }
    {
        std::lock_guard<std::mutex> lock(canvas_mutex_);
        canvas_ = GpuCanvas();
        max_w_ = hint_w_;
        max_h_ = hint_h_;
        pipe_ready_ = false;
        pipe_gen_ = ~0ull;
        pending_.clear();
        latest_pts_ = -1;
        frame_seq_ = 0;
        need_clear_ = true;
        first_frame_ = true;
        canvas_gen_ = 0;
    }
    {
        std::lock_guard<std::mutex> lock(clock_mutex_);
        start_qpc_ = qpc100ns();
        paused_total_ = 0;
        paused_ = false;
        resume_qpc_ = 0;
        mic_continuous_ = mic;
    }
    if (start_qpc) *start_qpc = start_qpc_;

    options_ = pending_options_;
    CaptureTarget t;
    t.fps = fps_;
    t.monitor = mon.handle;
    t.window = (pending_window_ && IsWindow(pending_window_)) ? pending_window_ : nullptr;
    t.options = options_;
    running_ = true;
    video_thread_ = std::thread([this] { video_thread(); });

    hr = open_capture(t, method == (int)CaptureMethod::Dxgi ? CaptureMethod::Dxgi : CaptureMethod::Wgc);
    if (FAILED(hr)) {
        running_ = false;
        stopping_ = true;
        frame_cv_.notify_all();
        std::thread worker = std::move(video_thread_);
        // The worker may be inside watch_target() waiting for ctl_mutex_; release it so it can exit.
        ctl.unlock();
        if (worker.joinable()) worker.join();
        ctl.lock();
        stopping_ = false;
        fail(fail_stage_ ? fail_stage_.load() : STAGE_START_CAPTURE, hr,
             "Screen capture could not start (0x" + std::to_string((unsigned)hr) + ").");
        return IR_FAIL;
    }

    if (audio) {
        const HWND audio_window = (t.window && (t.options & CAP_PROCESS_AUDIO)) ? t.window : nullptr;
        sys_audio_ = std::make_unique<AudioSource>(false, L"", audio_window,
            [this](const int16_t* d, uint32_t n, int64_t q) { push_system(d, n, q); }, &sys_counters_);
        sys_audio_->start();
    }
    if (mic) {
        mic_audio_ = std::make_unique<AudioSource>(true, mic_device ? mic_device : L"", nullptr,
            [this](const int16_t* d, uint32_t n, int64_t q) { push_mic(d, n, q); }, &mic_counters_);
        mic_audio_->start();
    }
    return IR_OK;
}

void Engine::stop() {
    std::lock_guard<std::mutex> stop(stop_mutex_);
    std::thread worker;
    {
        std::lock_guard<std::mutex> ctl(ctl_mutex_);
        if (stopping_) return;
        stopping_ = true;
        running_ = false;
        frame_cv_.notify_all();
        worker = std::move(video_thread_);
    }
    // watch_target() takes ctl_mutex_, so join with the lock released to avoid a deadlock.
    if (worker.joinable()) worker.join();
    std::lock_guard<std::mutex> ctl(ctl_mutex_);
    // Always clear capture and audio: a watcher can recreate them, and a previous stop may have
    // been skipped while nothing was running.
    {
        std::lock_guard<std::mutex> lock(capture_mutex_);
        if (capture_) { capture_->stop(); capture_.reset(); }
    }
    if (sys_audio_) { sys_audio_->stop(); sys_audio_.reset(); }
    if (mic_audio_) { mic_audio_->stop(); mic_audio_.reset(); }
    {
        std::lock_guard<std::mutex> lock(canvas_mutex_);
        pending_.clear();
        pipe_ = ObsVideoPipeline();
        pipe_ready_ = false;
        snap_.Reset();
        snap_srv_.Reset();
        canvas_ = GpuCanvas();
    }
    mf_.reset();
    selection_ = VideoSelection();
    gpu_ = GpuDevice();
    {
        std::lock_guard<std::mutex> lock(ring_mutex_);
        vring_.clear();
        sys_ring_.clear();
        mic_ring_.clear();
        vbytes_ = 0;
    }
    stopping_ = false;
}

int Engine::set_target(HWND window, uint32_t options) {
    std::lock_guard<std::mutex> ctl(ctl_mutex_);
    if (running_) return IR_FAIL; // the target is chosen before start; IR_SetCaptureSource switches live
    int w, h;
    if (window && !window_size(window, (options & CAP_CLIENT_AREA) != 0, &w, &h)) return IR_FAIL;
    pending_window_ = window;
    pending_options_ = options & 255;
    return IR_OK;
}

int Engine::set_source(int method, int monitor_index, HWND window, uint32_t options) {
    std::lock_guard<std::mutex> ctl(ctl_mutex_);
    if (!running_) return IR_FAIL;
    options &= 255;
    if (method == (int)CaptureMethod::Wgc && !window) return IR_FAIL; // WGC needs a window target
    auto mons = enumerate_monitors();
    if (!mons.empty()) monitor_index_ = (monitor_index < 0 || (size_t)monitor_index >= mons.size()) ? 0 : monitor_index;
    if (window) {
        int w, h;
        if (!window_size(window, (options & CAP_CLIENT_AREA) != 0, &w, &h)) return IR_FAIL;
        pending_window_ = window;
    } else {
        pending_window_ = nullptr; // switching to display capture
    }
    options_ = options;
    CaptureTarget t;
    t.fps = fps_;
    t.window = window;
    if (!window && !mons.empty()) t.monitor = mons[(size_t)monitor_index_].handle;
    t.options = options | CAP_SWITCHED;
    {
        std::lock_guard<std::mutex> lock(canvas_mutex_);
        need_clear_ = true;
    }
    HRESULT hr = open_capture(t, method == (int)CaptureMethod::Dxgi ? CaptureMethod::Dxgi : CaptureMethod::Wgc);
    if (FAILED(hr)) {
        fail(STAGE_START_CAPTURE, hr, "Switching the capture source failed (0x" + std::to_string((unsigned)hr) + ").");
        return IR_FAIL;
    }
    has_error_ = false;
    restart_system_audio((window && (options & CAP_PROCESS_AUDIO)) ? window : nullptr);
    return IR_OK;
}

int Engine::set_canvas_hint(int w, int h) {
    std::lock_guard<std::mutex> ctl(ctl_mutex_);
    if (running_) return IR_FAIL;
    hint_w_ = (uint32_t)std::clamp(w, 0, 16384);
    hint_h_ = (uint32_t)std::clamp(h, 0, 16384);
    return IR_OK;
}

int Engine::set_paused(bool paused) {
    std::lock_guard<std::mutex> lock(clock_mutex_);
    const int64_t now = qpc100ns();
    if (paused && !paused_) {
        paused_ = true;
        pause_began_ = now;
    } else if (!paused && paused_) {
        paused_ = false;
        if (!mic_continuous_) paused_total_ += now - pause_began_;
        resume_qpc_ = now;
    }
    return IR_OK;
}

// ---- Save ----

static std::vector<int16_t> mix_range(int64_t t0, int64_t t1, const std::deque<AudioBlock>& sys,
                                      const std::deque<AudioBlock>& mic) {
    const int64_t n = (t1 - t0) * AUDIO_RATE / SECOND_100NS;
    std::vector<int32_t> acc((size_t)n * 2, 0);
    auto place = [&](const AudioBlock& b, bool mono) {
        const int64_t first = (b.pts - t0) * AUDIO_RATE / SECOND_100NS;
        for (uint32_t i = 0; i < b.frames; ++i) {
            const int64_t at = first + i;
            if (at < 0 || at >= n) continue;
            if (mono) {
                acc[(size_t)at * 2] += b.pcm[i];
                acc[(size_t)at * 2 + 1] += b.pcm[i];
            } else {
                acc[(size_t)at * 2] += b.pcm[(size_t)i * 2];
                acc[(size_t)at * 2 + 1] += b.pcm[(size_t)i * 2 + 1];
            }
        }
    };
    for (auto& b : sys) place(b, false);
    for (auto& b : mic) place(b, true);
    std::vector<int16_t> out(acc.size());
    for (size_t i = 0; i < acc.size(); ++i) out[i] = (int16_t)std::clamp(acc[i], -32768, 32767);
    return out;
}

int Engine::save(const std::string& path, int seconds_back) {
    g_save_progress = 0;
    uint64_t audio_bytes = 0;
    {
        std::lock_guard<std::mutex> lock(ring_mutex_);
        for (const auto& b : sys_ring_) audio_bytes += b.pcm.size() * sizeof(int16_t);
        for (const auto& b : mic_ring_) audio_bytes += b.pcm.size() * sizeof(int16_t);
    }
    if (!save_memory_available(audio_bytes)) {
        g_save_progress = -1;
        set_last_error("Not enough committed memory to save the clip.");
        return IR_LOW_MEMORY;
    }
    std::vector<std::shared_ptr<EncodedFrame>> frames;
    std::deque<AudioBlock> sys, mic;
    {
        std::lock_guard<std::mutex> lock(ring_mutex_);
        frames.assign(vring_.begin(), vring_.end());
        sys = sys_ring_;
        mic = mic_ring_;
    }
    if (frames.empty()) {
        g_save_progress = -1;
        return IR_NOT_ENOUGH_DATA;
    }
    try {
        const int64_t first = frames.front()->pts100ns;
        const int64_t dur = SECOND_100NS / std::max(1, fps_);
        const int64_t end = frames.back()->pts100ns + dur;
        std::vector<VideoSample> v;
        v.reserve(frames.size());
        for (auto& f : frames) v.push_back({f->data.data(), f->data.size(), f->keyframe, f->pts100ns});
        g_save_progress = 10;
        std::vector<int16_t> mixed;
        std::vector<AudioChunk> a;
        if (!sys.empty() || !mic.empty()) {
            int64_t t0 = first;
            if (seconds_back > 0) t0 = std::max(first, end - (int64_t)seconds_back * SECOND_100NS - 6 * SECOND_100NS);
            mixed = mix_range(t0, end, sys, mic);
            a.push_back({reinterpret_cast<const uint8_t*>(mixed.data()), (uint32_t)(mixed.size() / 2), t0});
        }
        g_save_progress = 30;
        int rc = mux_until(path, v, a, fps_, (int)out_w_, (int)out_h_, seconds_back, end);
        g_save_progress = rc == IR_OK ? 100 : -1;
        return rc;
    } catch (const std::bad_alloc&) {
        set_last_error("Not enough memory to save the clip.");
        g_save_progress = -1;
        return IR_LOW_MEMORY;
    }
}

void Engine::status(int* buffering, int* ring_seconds, int64_t* encoded, int64_t* dropped, int* has_error,
                    int64_t* a_frames, int64_t* a_events, int64_t* a_got, int64_t* heartbeat) {
    if (buffering) *buffering = running_ ? 1 : 0;
    if (ring_seconds) *ring_seconds = ring_seconds_.load();
    if (encoded) *encoded = encoded_;
    if (dropped) *dropped = dropped_;
    if (has_error) *has_error = has_error_ ? 1 : 0;
    if (a_frames) *a_frames = sys_counters_.frames;
    if (a_events) *a_events = sys_counters_.events;
    if (a_got) *a_got = sys_counters_.got_frames;
    if (heartbeat) *heartbeat = heartbeat_;
}

void Engine::mic_status(int64_t* frames, int64_t* events, int64_t* got, int64_t* fail_hr, int* sys_peak, int* mic_peak,
                        wchar_t* name, int name_cap) {
    if (sys_peak) *sys_peak = sys_counters_.peak;
    if (mic_peak) *mic_peak = mic_counters_.peak;
    if (frames) *frames = mic_counters_.frames;
    if (events) *events = mic_counters_.events;
    if (got) *got = mic_counters_.got_frames;
    if (fail_hr) *fail_hr = mic_counters_.fail_hr;
    if (name && name_cap > 0) {
        std::lock_guard<std::mutex> lock(mic_counters_.name_mutex);
        const size_t n = std::min<size_t>(mic_counters_.active_name.size(), (size_t)name_cap - 1);
        std::memcpy(name, mic_counters_.active_name.data(), n * sizeof(wchar_t));
        name[n] = 0;
    }
}

// ------------------------------------------------------------------------------------------------
// Encoder probe (IR_ProbeEncoders)
// ------------------------------------------------------------------------------------------------

static uint32_t probe_all_encoders() {
    constexpr int W = 1280, H = 720, FPS = 30;
    constexpr uint32_t BPS = 4'000'000;
    uint32_t mask = 0;
    struct Item { const char* name; uint32_t bit; };
    static const Item items[] = {
        {"h264_nvenc", ENC_H264_NVENC}, {"hevc_nvenc", ENC_HEVC_NVENC}, {"h264_amf", ENC_H264_AMF},
        {"hevc_amf", ENC_HEVC_AMF},     {"h264_qsv", ENC_H264_QSV},     {"hevc_qsv", ENC_HEVC_QSV},
        {"libx264", ENC_H264_X264},     {"libx265", ENC_HEVC_X265},
    };
    if (ff_load()) {
        for (const Item& it : items) {
            try {
                if (probe_ffmpeg_encoder(it.name, W, H, FPS, BPS)) mask |= it.bit;
            } catch (...) {}
        }
    }
    try {
        HRESULT hr;
        if (probe_mf_encoder(MfCodec::H264, W, H, FPS, BPS, &hr)) mask |= ENC_H264_MF;
        if (probe_mf_encoder(MfCodec::Hevc, W, H, FPS, BPS, &hr)) mask |= ENC_HEVC_MF;
    } catch (...) {}
    return mask;
}

} // namespace ir

// ================================================================================================
// Exported C ABI (must match InstantReplay.Native/IrNative.cs)
// ================================================================================================

using ir::Engine;

#define IR_EXPORT extern "C" __declspec(dllexport)
#define GUARD(expr, fallback) \
    try { return (expr); } catch (...) { return (fallback); }

static void copy_wide(const std::wstring& text, wchar_t* out, int cap) {
    if (!out || cap <= 0) return;
    const size_t n = std::min<size_t>(text.size(), (size_t)cap - 1);
    std::memcpy(out, text.data(), n * sizeof(wchar_t));
    out[n] = 0;
}

IR_EXPORT int IR_Create(void** engine) {
    try {
        if (!engine) return ir::IR_FAIL;
        *engine = new Engine();
        return ir::IR_OK;
    } catch (...) {
        return ir::IR_LOW_MEMORY;
    }
}

IR_EXPORT void IR_Destroy(void* engine) {
    try { delete static_cast<Engine*>(engine); } catch (...) {}
}

IR_EXPORT int IR_Start(void* engine, int monitor_index, int ring_seconds, int fps, int bitrate_kbps, int cap_w, int cap_h,
                       int capture_audio, int capture_mic, const wchar_t* mic_device, int encoder_pref,
                       int capture_method, long long* start_qpc) {
    if (!engine) return ir::IR_FAIL;
    try {
        int64_t q = 0;
        int rc = static_cast<Engine*>(engine)->start(monitor_index, ring_seconds, fps, bitrate_kbps, cap_w, cap_h,
                                                     capture_audio != 0, capture_mic != 0, mic_device, encoder_pref,
                                                     capture_method, &q);
        if (start_qpc) *start_qpc = q;
        return rc;
    } catch (const std::exception& e) {
        ir::set_last_error(std::string("Engine start failed: ") + e.what());
        return ir::IR_FAIL;
    } catch (...) {
        return ir::IR_FAIL;
    }
}

IR_EXPORT int IR_LastErrorMessage(unsigned char* buffer, int capacity) {
    try {
        const std::string msg = ir::last_error();
        if (!buffer || capacity <= 0) return (int)msg.size();
        const size_t n = std::min<size_t>(msg.size(), (size_t)capacity - 1);
        std::memcpy(buffer, msg.data(), n);
        buffer[n] = 0;
        return (int)n;
    } catch (...) {
        return 0;
    }
}

IR_EXPORT void IR_Stop(void* engine) {
    try { if (engine) static_cast<Engine*>(engine)->stop(); } catch (...) {}
}

IR_EXPORT int IR_SetCaptureTarget(void* engine, HWND window, unsigned options) {
    if (!engine) return ir::IR_FAIL;
    GUARD(static_cast<Engine*>(engine)->set_target(window, options), ir::IR_FAIL)
}

IR_EXPORT int IR_SetCaptureSource(void* engine, int method, int monitor_index, HWND window, unsigned options) {
    if (!engine) return ir::IR_FAIL;
    GUARD(static_cast<Engine*>(engine)->set_source(method, monitor_index, window, options), ir::IR_FAIL)
}

IR_EXPORT int IR_SetCanvasHint(void* engine, int width, int height) {
    if (!engine) return ir::IR_FAIL;
    GUARD(static_cast<Engine*>(engine)->set_canvas_hint(width, height), ir::IR_FAIL)
}

IR_EXPORT int IR_SetPaused(void* engine, int paused) {
    if (!engine) return ir::IR_FAIL;
    GUARD(static_cast<Engine*>(engine)->set_paused(paused != 0), ir::IR_FAIL)
}

IR_EXPORT int IR_GetWindowCount() {
    GUARD((int)ir::enumerate_windows().size(), 0)
}

IR_EXPORT void IR_GetWindowInfo(int index, HWND* window, wchar_t* title, int capacity, int* width, int* height) {
    try {
        if (window) *window = nullptr;
        if (title && capacity > 0) title[0] = 0;
        auto list = ir::enumerate_windows();
        if (index < 0 || (size_t)index >= list.size()) return;
        const auto& w = list[(size_t)index];
        if (window) *window = w.handle;
        if (width) *width = w.w;
        if (height) *height = w.h;
        copy_wide(w.title, title, capacity);
    } catch (...) {}
}

IR_EXPORT int IR_Save(void* engine, const wchar_t* out_path, int seconds_back) {
    if (!engine || !out_path) return ir::IR_FAIL;
    try {
        return static_cast<Engine*>(engine)->save(ir::wide_to_utf8(out_path, wcslen(out_path)), seconds_back);
    } catch (...) {
        ir::g_save_progress = -1;
        return ir::IR_FAIL;
    }
}

IR_EXPORT int IR_SaveProgress() { return ir::g_save_progress.load(); }

IR_EXPORT void IR_GetStatus(void* engine, int* buffering, int* ring_seconds, long long* encoded, long long* dropped,
                            int* has_error, long long* audio_frames, long long* audio_events, long long* audio_got,
                            long long* heartbeat) {
    try {
        if (!engine) return;
        static_assert(sizeof(long long) == sizeof(int64_t), "ABI");
        static_cast<Engine*>(engine)->status(buffering, ring_seconds, reinterpret_cast<int64_t*>(encoded),
                                             reinterpret_cast<int64_t*>(dropped), has_error,
                                             reinterpret_cast<int64_t*>(audio_frames),
                                             reinterpret_cast<int64_t*>(audio_events),
                                             reinterpret_cast<int64_t*>(audio_got), reinterpret_cast<int64_t*>(heartbeat));
    } catch (...) {}
}

IR_EXPORT int IR_GetMonitorCount() {
    GUARD((int)ir::enumerate_monitors().size(), 0)
}

IR_EXPORT void IR_GetMonitorInfo(int index, wchar_t* name, int name_cap, int* width, int* height) {
    try {
        auto mons = ir::enumerate_monitors();
        if (mons.empty()) return;
        const auto& m = mons[(index < 0 || (size_t)index >= mons.size()) ? 0 : (size_t)index];
        if (width) *width = m.w;
        if (height) *height = m.h;
        std::wstring text = (m.primary ? L"Primary - " : L"") + std::to_wstring(m.w) + L"x" + std::to_wstring(m.h);
        copy_wide(text, name, name_cap);
    } catch (...) {}
}

IR_EXPORT int IR_IsDxgiSupported() {
    GUARD(ir::enumerate_monitors().empty() ? 0 : 1, 0)
}

IR_EXPORT int IR_DxgiDuplicationSupported() {
    GUARD(ir::dxgi_duplication_supported() ? 1 : 0, 0)
}

IR_EXPORT int IR_ActiveCaptureMethod(void* engine) {
    if (!engine) return 0;
    GUARD(static_cast<Engine*>(engine)->active_capture_method(), 0)
}
IR_EXPORT int IR_LastHresult(void* engine) {
    if (!engine) return 0;
    GUARD(static_cast<Engine*>(engine)->last_hresult(), 0)
}
IR_EXPORT int IR_LastFailStage(void* engine) {
    if (!engine) return 0;
    GUARD(static_cast<Engine*>(engine)->last_fail_stage(), 0)
}
IR_EXPORT int IR_LastFailHr(void* engine) {
    if (!engine) return 0;
    GUARD(static_cast<Engine*>(engine)->last_fail_hr(), 0)
}
IR_EXPORT int IR_LastCaptureHr(void* engine) {
    if (!engine) return 0;
    GUARD(static_cast<Engine*>(engine)->last_capture_hr(), 0)
}
IR_EXPORT int IR_ActiveEncoder(void* engine) {
    if (!engine) return 0;
    GUARD(static_cast<Engine*>(engine)->active_encoder(), 0)
}
IR_EXPORT int IR_HwInitHr(void* engine) {
    if (!engine) return 0;
    GUARD(static_cast<Engine*>(engine)->hw_init_hr(), 0)
}

IR_EXPORT int IR_GetMicDeviceCount() {
    GUARD((int)ir::mic_devices().size(), 0)
}

IR_EXPORT void IR_GetMicDeviceName(int index, wchar_t* name, int name_cap) {
    try {
        if (name && name_cap > 0) name[0] = 0;
        auto list = ir::mic_devices();
        if (index >= 0 && (size_t)index < list.size()) copy_wide(list[(size_t)index], name, name_cap);
    } catch (...) {}
}

IR_EXPORT void IR_MicStatus(void* engine, long long* mic_frames, long long* mic_events, long long* mic_got,
                            long long* mic_fail_hr, int* sys_peak, int* mic_peak, wchar_t* active_name, int active_cap) {
    try {
        if (!engine) return;
        static_cast<Engine*>(engine)->mic_status(reinterpret_cast<int64_t*>(mic_frames),
                                                 reinterpret_cast<int64_t*>(mic_events),
                                                 reinterpret_cast<int64_t*>(mic_got),
                                                 reinterpret_cast<int64_t*>(mic_fail_hr), sys_peak, mic_peak,
                                                 active_name, active_cap);
    } catch (...) {}
}

IR_EXPORT const char* IR_Version() { return "cpp-obs-1.0"; }

IR_EXPORT unsigned IR_ProbeEncoders() {
    try { return ir::probe_all_encoders(); } catch (...) { return 0; }
}
