// Instant Replay native engine v4.
//
// The video thread is the sole owner of the canvas/snapshot/pipeline/pending
// slot ring and the capture watcher heartbeat. Capture threads publish only
// intrusive Ref<VideoFrame> leases into preallocated SPSC queues. Audio threads
// publish only Ref<AudioBlock> leases into the replay rings. Save work is a
// control-plane job served by one save thread.
#include "core.hpp"

#include <cmath>
#include <cwchar>
#include <objbase.h>
#include <unordered_map>

namespace ir
{
    namespace
    {
        constexpr uint32_t kAudioAccumFrames = 16384; // amortize WASAPI packets into pooled blocks

        // Global last-error store; the other v4 translation units call these.
        std::mutex g_error_mutex;
        std::string g_error;

        // Save progress / last job cache. IR_SaveProgress must never touch an
        // evicted SaveJob: the weak reference is the only live-object check.
        std::mutex g_last_mutex;
        std::weak_ptr<SaveJob> g_last_job;
        uint64_t g_last_id = 0;
        std::atomic<int> g_last_progress{-1};

        void remember_last_job(const std::shared_ptr<SaveJob> &job)
        {
            std::lock_guard<std::mutex> lock(g_last_mutex);
            g_last_job = job;
            g_last_id = job ? job->id : 0;
            g_last_progress.store(job ? job->progress.load() : 0, std::memory_order_relaxed);
        }

        void clear_last_job(int progress)
        {
            std::lock_guard<std::mutex> lock(g_last_mutex);
            g_last_job.reset();
            g_last_id = 0;
            g_last_progress.store(progress, std::memory_order_relaxed);
        }

        void publish_last_progress(const std::shared_ptr<SaveJob> &job, int progress)
        {
            if (!job)
                return;
            job->progress.store(progress, std::memory_order_relaxed);
            std::lock_guard<std::mutex> lock(g_last_mutex);
            if (g_last_id == job->id)
                g_last_progress.store(progress, std::memory_order_relaxed);
        }

        std::shared_ptr<SaveJob> retain_global_last_job(uint64_t id)
        {
            std::lock_guard<std::mutex> lock(g_last_mutex);
            if (g_last_id != id)
                return {};
            return g_last_job.lock();
        }

        // Balances every COM apartment we enter; RPC_E_CHANGED_MODE is left alone.
        struct ComGuard
        {
            HRESULT hr = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
            ~ComGuard()
            {
                if (hr == S_OK || hr == S_FALSE)
                    CoUninitialize();
            }
        };

        // v4 does not clone the 512 MiB video ring: run_save() snapshots pooled
        // references, then allocates the mixed PCM (2x int32 + 2x int16 per
        // frame) plus AAC/muxer buffers. A 64 MiB fixed reserve with 4x the audio
        // replay bytes covers that working set without the old engine's clone tax.
        bool save_memory_available(uint64_t audio_bytes, std::string *detail = nullptr)
        {
            constexpr uint64_t reserve = 64ull * 1024 * 1024;
            constexpr uint64_t audio_factor = 4;
            const uint64_t max = std::numeric_limits<uint64_t>::max();
            const uint64_t workspace =
                audio_bytes > (max - reserve) / audio_factor ? max : audio_bytes * audio_factor;
            const uint64_t required = reserve + workspace;

            MEMORYSTATUSEX mem = {};
            mem.dwLength = sizeof(mem);
            if (!GlobalMemoryStatusEx(&mem))
            {
                if (detail)
                    *detail = "GlobalMemoryStatusEx failed";
                return false;
            }
            if (detail)
            {
                *detail = "available " + std::to_string(mem.ullAvailPageFile / (1024ull * 1024ull)) +
                          " MiB, required " + std::to_string(required / (1024ull * 1024ull)) + " MiB";
            }
            return mem.ullAvailPageFile >= required;
        }

        Scaler scaler_from_env()
        {
            wchar_t buf[32];
            DWORD n = GetEnvironmentVariableW(L"IR_SCALER", buf, 32);
            if (n > 0 && n < 32 && _wcsicmp(buf, L"bilinear") == 0)
                return Scaler::Bilinear;
            return Scaler::Bicubic;
        }

        // One producer thread owns each accumulator. Stop flushes it only after
        // the corresponding AudioSource::stop() has joined that producer.
        struct AudioAccumulator
        {
            std::vector<int16_t> pcm;
            uint32_t frames = 0; // stereo audio frames, or mono samples for mic
            int64_t pts = -1;

            void clear() noexcept
            {
                pcm.clear();
                frames = 0;
                pts = -1;
            }
        };

        struct VideoInit
        {
            uint32_t canvas_w = 2, canvas_h = 2;
            uint32_t cap_w = 2, cap_h = 2;
            bool hd = false;
            Scaler scaler = Scaler::Bicubic;
            VideoOptions options;
        };

        struct Pending
        {
            uint32_t slot = 0;
            int64_t pts = 0;
            Ref<GpuFrame> frame;
        };
    } // namespace

    int current_save_progress()
    {
        std::lock_guard<std::mutex> lock(g_last_mutex);
        if (auto job = g_last_job.lock())
            return job->progress.load(std::memory_order_relaxed);
        return g_last_progress.load(std::memory_order_relaxed);
    }

    void clear_save_progress() { clear_last_job(-1); }

    void set_last_error(const std::string &message)
    {
        std::lock_guard<std::mutex> lock(g_error_mutex);
        g_error = message;
    }

    std::string last_error()
    {
        std::lock_guard<std::mutex> lock(g_error_mutex);
        return g_error;
    }

    std::string wide_to_utf8(const wchar_t *s, size_t len)
    {
        if (!s || len == 0)
            return {};
        int n = WideCharToMultiByte(CP_UTF8, 0, s, (int)len, nullptr, 0, nullptr, nullptr);
        std::string out(n > 0 ? n : 0, '\0');
        if (n > 0)
            WideCharToMultiByte(CP_UTF8, 0, s, (int)len, out.data(), n, nullptr, nullptr);
        return out;
    }

    std::wstring utf8_to_wide(const std::string &s)
    {
        if (s.empty())
            return {};
        int n = MultiByteToWideChar(CP_UTF8, 0, s.data(), (int)s.size(), nullptr, 0);
        std::wstring out(n > 0 ? n : 0, L'\0');
        if (n > 0)
            MultiByteToWideChar(CP_UTF8, 0, s.data(), (int)s.size(), out.data(), n);
        return out;
    }

    // -------------------------------------------------------------------------
    // Engine facade
    // -------------------------------------------------------------------------
    class Engine
    {
    public:
        Engine() = default;
        ~Engine()
        {
            try
            {
                stop();
            }
            catch (...)
            {
            }
        }

        int start(int monitor_index, int ring_seconds, int fps, int bitrate_kbps, int cap_w, int cap_h, bool audio,
                  bool mic, const wchar_t *mic_device, int pref, int method, int64_t *start_qpc);
        void stop();

        int set_target(HWND window, uint32_t options);
        int set_source(int method, int monitor_index, HWND window, uint32_t options);
        int set_canvas_hint(int w, int h);
        int set_paused(bool paused);

        int begin_save(const std::wstring &path, int seconds_back, uint64_t *job_id);
        std::shared_ptr<SaveJob> retain_job(uint64_t id);
        int poll_save(uint64_t id, int *progress, int *result);

        void status(int *buffering, int *ring_seconds, int64_t *encoded, int64_t *dropped, int *has_error,
                    int64_t *a_frames, int64_t *a_events, int64_t *a_got, int64_t *heartbeat);
        void mic_status(int64_t *frames, int64_t *events, int64_t *got, int64_t *fail_hr, int *sys_peak,
                        int *mic_peak, wchar_t *name, int name_cap);
        void audio_diagnostics(int which, int64_t *activate_hr, int64_t *init_hr, int64_t *start_hr, int *tag,
                               int *rate, int *channels, int *bits, int *is_float, int *convert_failures,
                               int64_t *last_fail_hr);
        void telemetry(int64_t *captured, int64_t *paced, int64_t *rejected, int64_t *encoded, int64_t *dropped,
                       int *encoder, int64_t *last_pts);
        void timing(double *map_ms, double *encode_ms, double *render_ms, double *submit_ms, double *snapshot_ms,
                    double *unmap_ms, int64_t *frames);

        int active_capture_method() const { return active_method_.load(); }
        int last_hresult() const { return last_hr_.load(); }
        int last_fail_stage() const { return fail_stage_.load(); }
        int last_fail_hr() const { return fail_hr_.load(); }
        int last_capture_hr();
        int active_encoder() const { return active_encoder_.load(); }
        int hw_init_hr() const { return hw_init_hr_.load(); }

    private:
        void fail(int stage, HRESULT hr, const std::string &message);
        int post_control(ControlKind kind, int a = 0, int b = 0, int c = 0, int d = 0, HWND window = nullptr,
                         std::chrono::milliseconds timeout = std::chrono::milliseconds(1000));
        void handle_command(const std::shared_ptr<ControlCommand> &cmd);

        HRESULT open_capture(const CaptureTarget &target, CaptureMethod requested);
        int apply_switch_source(int method, int monitor_index, HWND window, uint32_t options);
        void restart_system_audio(HWND window);
        void watch_target();
        void drain_raw_queues();

        void signal_ack(const Ref<VideoFrame> &frame) noexcept;
        bool process_frame(Ref<VideoFrame> frame);
        bool ensure_canvas_size(uint32_t w, uint32_t h);
        bool recreate_canvas(uint32_t w, uint32_t h);
        bool ensure_pipeline();
        bool create_video_encoders(std::string *err);
        bool encoder_uses_textures() const;
        bool encode_slot(Pending &pending);
        bool flush_pending();
        void push_video(Ref<PacketRef> frame);

        void push_system(const int16_t *data, uint32_t frames, int64_t qpc, bool discontinuity = false);
        void push_mic(const int16_t *data, uint32_t frames, int64_t qpc, bool discontinuity = false);
        void flush_audio_accumulator(bool mic);

        bool video_init(const VideoInit &init);
        void video_loop();
        void video_exit();
        void watchdog_loop();

        void ensure_save_thread();
        void stop_save();
        void save_loop();
        int run_save(const std::shared_ptr<SaveJob> &job);
        static void finish_job(const std::shared_ptr<SaveJob> &job, int result, int progress);
        void register_job(const std::shared_ptr<SaveJob> &job);
        void unregister_job(uint64_t id);
        uint64_t audio_bytes_snapshot();
        bool has_video_frames();

        // Lifecycle. Serializes start/stop; never held while joining the video thread
        // by a command that the video thread must acknowledge.
        std::mutex lifecycle_mutex_;
        std::atomic<bool> running_{false};
        std::thread video_thread_;
        std::thread watchdog_thread_;
        std::atomic<bool> watchdog_stop_{false};

        // Single shared clock and stop-aware control bus.
        Clock clock_;
        ControlBus ctl_;
        std::mutex ctl_post_mutex_;
        std::atomic<uint64_t> ctl_seq_{0};

        // Preallocated data-plane queues and pools.
        SpscQueue<Ref<VideoFrame>> q_raw_wgc_{kRawWgc};
        SpscQueue<Ref<VideoFrame>> q_raw_dxgi_{kRawDxgi};
        ObjectPool<VideoFrame> frame_pool_{8};
        ObjectPool<Keepalive> keepalive_pool_{kKeepalivePool};
        ObjectPool<FrameAck> ack_pool_{kAckPool};
        ObjectPool<PacketRef> packet_pool_{kPacketPool, 4096};
        ObjectPool<AudioBlock> audio_pool_{kAudioPool, kAudioAccumFrames};

        // Capture (capture objects are control-plane; their frames are data-plane).
        std::mutex capture_mutex_;
        std::unique_ptr<CaptureSource> capture_;
        CaptureTarget target_;
        CaptureMethod method_ = CaptureMethod::Wgc;
        std::atomic<int> active_method_{0};
        std::atomic<int> cap_hr_{0};

        // GPU device outlives every canvas, pipeline, encoder and capture source.
        GpuContext gpu_;

        // ---- owned exclusively by the video thread ----
        GpuCanvas canvas_;
        uint32_t max_w_ = 0, max_h_ = 0;
        uint64_t canvas_gen_ = 0;
        uint32_t last_place_w_ = 0, last_place_h_ = 0, last_dx_ = 0, last_dy_ = 0;
        bool need_clear_ = true;
        std::atomic<int64_t> latest_pts_{-1};
        uint64_t frame_seq_ = 0;
        VideoPipeline pipe_;
        bool pipe_ready_ = false;
        uint64_t pipe_gen_ = ~0ull;
        ComPtr<ID3D11Texture2D> snap_;
        ComPtr<ID3D11ShaderResourceView> snap_srv_;
        std::deque<Pending> pending_;
        uint32_t next_slot_ = 0;
        bool first_frame_ = true;
        int64_t next_segment_pts_ = 0;
        VideoSelection selection_;
        std::unique_ptr<MfEncoder> mf_;
        VideoOptions video_options_;
        CodecSel active_codec_ = CodecSel::H264; // resolved from the encoder that actually opened
        Scaler scaler_ = Scaler::Bicubic;

        // Replay rings. Only intrusive leases are stored.
        std::mutex ring_mutex_;
        std::deque<Ref<PacketRef>> vring_;
        int64_t vbytes_ = 0;
        std::deque<Ref<AudioBlock>> sys_ring_, mic_ring_;
        AudioAccumulator sys_acc_, mic_acc_;

        // Audio producers. Ownership is guarded because the video thread can
        // restart system audio while stop() tears it down.
        std::mutex audio_owner_mutex_;
        std::unique_ptr<AudioSource> sys_audio_, mic_audio_;
        AudioCounters sys_counters_, mic_counters_;
        bool want_audio_ = false;
        HWND audio_target_ = nullptr;
        std::wstring mic_device_;

        // Shared configuration.
        std::mutex config_mutex_;
        std::atomic<int> ring_seconds_{60};
        int fps_ = 30;
        uint32_t bps_ = 0, out_w_ = 0, out_h_ = 0;
        int pref_ = 0, monitor_index_ = 0;
        uint32_t options_ = CAP_CURSOR | CAP_KEEP_CORNERS;
        HWND pending_window_ = nullptr;
        uint32_t pending_options_ = CAP_CLIENT_AREA | CAP_CURSOR | CAP_KEEP_CORNERS;
        uint32_t hint_w_ = 0, hint_h_ = 0;

        // Telemetry / timing.
        std::atomic<int64_t> captured_{0}, encoded_{0}, dropped_{0}, heartbeat_{0};
        std::atomic<int64_t> t_map_{0}, t_encode_{0}, t_render_{0}, t_submit_{0}, t_snapshot_{0}, t_unmap_{0}, t_frames_{0};
        std::atomic<bool> has_error_{false};
        std::atomic<int> last_hr_{0}, fail_stage_{0}, fail_hr_{0}, active_encoder_{0}, hw_init_hr_{0};

        // Save control plane.
        SpscQueue<std::shared_ptr<SaveJob>> q_save_{kSaveQueue};
        std::atomic<bool> save_stopped_{false}; // SpscQueue has no public stopped() accessor
        std::mutex save_lifecycle_mutex_;
        std::thread save_thread_;
        std::mutex jobs_mutex_;
        std::unordered_map<uint64_t, std::shared_ptr<SaveJob>> jobs_;
        std::deque<uint64_t> job_order_;
        std::atomic<uint64_t> next_job_id_{1};
    };

    // -------------------------------------------------------------------------
    // Diagnostics and control
    // -------------------------------------------------------------------------
    void Engine::fail(int stage, HRESULT hr, const std::string &message)
    {
        fail_stage_.store(stage);
        fail_hr_.store((int)hr);
        last_hr_.store((int)hr);
        has_error_.store(true);
        if (!message.empty())
            set_last_error(message);
    }

    int Engine::post_control(ControlKind kind, int a, int b, int c, int d, HWND window,
                             std::chrono::milliseconds timeout)
    {
        std::lock_guard<std::mutex> lock(ctl_post_mutex_);
        // Increment before post: the video thread can wake, call pop() and wait
        // for the command to be published without ever blocking forever.
        ctl_seq_.fetch_add(1, std::memory_order_release);
        auto cmd = ctl_.post(kind, a, b, c, d, window);
        return ControlBus::wait(cmd, timeout);
    }

    void Engine::handle_command(const std::shared_ptr<ControlCommand> &cmd)
    {
        if (!cmd)
            return;
        int rc = IR_FAIL;
        switch (cmd->kind)
        {
        case ControlKind::SwitchSource:
            rc = apply_switch_source(cmd->values[0], cmd->values[1], cmd->window,
                                     (uint32_t)cmd->values[2]);
            break;
        case ControlKind::Pause:
            clock_.set_paused(cmd->values[0] != 0);
            rc = IR_OK;
            break;
        case ControlKind::ReconfigureCanvas:
            need_clear_ = true;
            rc = IR_OK;
            break;
        case ControlKind::SetNeedClear:
            need_clear_ = true;
            rc = IR_OK;
            break;
        case ControlKind::Stop:
            running_.store(false);
            q_raw_wgc_.stop();
            q_raw_dxgi_.stop();
            rc = IR_OK;
            break;
        }
        ControlBus::complete(cmd, rc);
    }

    int Engine::last_capture_hr()
    {
        std::lock_guard<std::mutex> lock(capture_mutex_);
        return capture_ ? (int)capture_->last_hr() : cap_hr_.load();
    }

    void Engine::drain_raw_queues()
    {
        Ref<VideoFrame> frame;
        while (q_raw_wgc_.try_pop(frame))
            frame.reset();
        while (q_raw_dxgi_.try_pop(frame))
            frame.reset();
    }

    // -------------------------------------------------------------------------
    // Capture lifecycle â€” capture threads only publish Ref<VideoFrame>.
    // -------------------------------------------------------------------------
    HRESULT Engine::open_capture(const CaptureTarget &target, CaptureMethod requested)
    {
        std::lock_guard<std::mutex> lock(capture_mutex_);
        if (capture_)
        {
            capture_->stop(); // joins the producer before any queue is reset
            capture_.reset();
        }
        target_ = target;

        auto attempt = [&](CaptureMethod m) -> HRESULT
        {
            drain_raw_queues();
            q_raw_wgc_.reset();
            q_raw_dxgi_.reset();

            FrameSink sink = [this, m](Ref<VideoFrame> frame)
            {
                SpscQueue<Ref<VideoFrame>> &q = m == CaptureMethod::Dxgi ? q_raw_dxgi_ : q_raw_wgc_;
                q.push(std::move(frame)); // stop-aware; q is stopped on teardown
            };

            std::unique_ptr<CaptureSource> src;
            if (m == CaptureMethod::Wgc)
                src = make_wgc_capture(gpu_, target, &frame_pool_, &keepalive_pool_, std::move(sink));
            else
                src = make_dxgi_capture(gpu_, target, &frame_pool_, &keepalive_pool_, &ack_pool_, std::move(sink));

            HRESULT hr = src->start();
            if (SUCCEEDED(hr))
            {
                capture_ = std::move(src);
                active_method_.store((int)m);
                method_ = m;
                return hr;
            }
            cap_hr_.store((int)hr);
            fail_stage_.store(src->stage() ? src->stage() : STAGE_START_CAPTURE);
            return hr;
        };

        HRESULT hr = attempt(requested);
        if (FAILED(hr))
        {
            const CaptureMethod other = requested == CaptureMethod::Wgc ? CaptureMethod::Dxgi : CaptureMethod::Wgc;
            const HRESULT hr2 = attempt(other);
            if (SUCCEEDED(hr2))
                return hr2;
        }
        return hr;
    }

    int Engine::apply_switch_source(int method, int monitor_index, HWND window, uint32_t options)
    {
        if (!running_.load())
            return IR_FAIL;
        options &= 255;
        if (method == (int)CaptureMethod::Wgc && !window)
            return IR_FAIL; // WGC requires a window target

        {
            std::lock_guard<std::mutex> lock(config_mutex_);
            auto mons = enumerate_monitors();
            if (!mons.empty())
                monitor_index_ = (monitor_index < 0 || (size_t)monitor_index >= mons.size()) ? 0 : monitor_index;
            if (window)
            {
                int w = 0, h = 0;
                if (!window_size(window, (options & CAP_CLIENT_AREA) != 0, &w, &h))
                    return IR_FAIL;
                pending_window_ = window;
            }
            else
            {
                pending_window_ = nullptr;
            }
            options_ = options;
            audio_target_ = (window && (options & CAP_PROCESS_AUDIO)) ? window : nullptr;

            CaptureTarget t;
            t.fps = fps_;
            t.window = window;
            if (!window && !mons.empty())
                t.monitor = mons[(size_t)monitor_index_].handle;
            t.options = options | CAP_SWITCHED;

            need_clear_ = true; // video-thread-owned
            HRESULT hr = open_capture(t, method == (int)CaptureMethod::Dxgi ? CaptureMethod::Dxgi : CaptureMethod::Wgc);
            if (FAILED(hr))
            {
                fail(STAGE_START_CAPTURE, hr,
                     "Switching the capture source failed (0x" + std::to_string((unsigned)hr) + ").");
                return IR_FAIL;
            }
            has_error_.store(false);
        }
        restart_system_audio((window && (options & CAP_PROCESS_AUDIO)) ? window : nullptr);
        return IR_OK;
    }

    void Engine::restart_system_audio(HWND window)
    {
        if (!want_audio_ || !running_.load())
            return;
        std::lock_guard<std::mutex> lock(audio_owner_mutex_);
        if (!running_.load())
            return;
        if (sys_audio_)
            sys_audio_->stop();
        sys_audio_ = std::make_unique<AudioSource>(false, L"", window,
                                                   [this](const int16_t *d, uint32_t n, int64_t q, bool disc)
                                                   { push_system(d, n, q, disc); }, &sys_counters_);
        sys_audio_->start();
    }

    void Engine::watch_target()
    {
        // Called only from the video thread's heartbeat.
        if (!running_.load())
            return;
        {
            std::lock_guard<std::mutex> lock(capture_mutex_);
            if (!capture_ || !capture_->closed())
                return;
        }

        std::lock_guard<std::mutex> lock(config_mutex_);
        if (!running_.load())
            return;
        auto mons = enumerate_monitors();
        CaptureTarget t;
        t.fps = fps_;
        t.options = options_ & ~CAP_CLIENT_AREA;
        if (!mons.empty())
            t.monitor = mons[(size_t)std::clamp(monitor_index_, 0, (int)mons.size() - 1)].handle;
        open_capture(t, method_);
        if (running_.load())
        {
            audio_target_ = nullptr;
            restart_system_audio(nullptr);
        }
    }

    // -------------------------------------------------------------------------
    // Video lane: canvas, snapshot, scaling/conversion, delayed encode.
    // -------------------------------------------------------------------------
    void Engine::signal_ack(const Ref<VideoFrame> &frame) noexcept
    {
        if (frame && frame->ack)
            frame->ack->signal();
    }

    bool Engine::recreate_canvas(uint32_t w, uint32_t h)
    {
        GpuCanvas fresh;
        if (FAILED(fresh.create(gpu_, w, h)))
            return false;
        canvas_ = std::move(fresh);
        ++canvas_gen_;
        pipe_ready_ = false;
        need_clear_ = true;
        return true;
    }

    bool Engine::ensure_canvas_size(uint32_t w, uint32_t h)
    {
        const uint32_t nw = std::max(max_w_, w), nh = std::max(max_h_, h);
        if (!canvas_.width() || nw != max_w_ || nh != max_h_)
        {
            max_w_ = nw;
            max_h_ = nh;
            const auto [cw, ch] = canvas_size_for(max_w_, max_h_, out_w_, out_h_);
            if (!recreate_canvas(cw, ch))
                return false;
        }
        return ensure_pipeline();
    }

    bool Engine::create_video_encoders(std::string *err)
    {
        hw_init_hr_.store(0);
        mf_.reset();
        selection_ = VideoSelection();
        std::string local;
        std::string &message = err ? *err : local;
        message.clear();
        if (!select_video(video_options_, &selection_, &message))
            return false;

        const auto try_create = [&](MfVendor vendor, std::string *why)
        {
            auto mf = MfEncoder::create(selection_.mf, vendor, &gpu_, (int)out_w_, (int)out_h_, fps_, bps_,
                                        video_options_.rate, video_options_.preset, &packet_pool_, why);
            if (!mf)
                return false;
            selection_.mf_vendor = vendor;
            mf_ = std::move(mf);
            active_encoder_.store(mf_->kind_code());
            active_codec_ = selection_.mf == MfCodec::Av1   ? CodecSel::Av1
                            : selection_.mf == MfCodec::Hevc ? CodecSel::Hevc
                                                              : CodecSel::H264;
            return true;
        };

        if (try_create(selection_.mf_vendor, &message))
            return true;

        // Auto may fall through to the remaining vendors when the first choice
        // is enumerated but cannot actually be opened on this machine.
        if (video_options_.enc == Enc::Auto)
        {
            const MfVendor order[] = {MfVendor::Qsv, MfVendor::Vp12, MfVendor::Nvenc, MfVendor::Amf,
                                      MfVendor::Software};
            for (MfVendor vendor : order)
            {
                if (vendor == selection_.mf_vendor)
                    continue;
                if (video_options_.avail && !(video_options_.avail & mf_availability_bit(selection_.mf, vendor)))
                    continue;
                std::string why;
                if (try_create(vendor, &why))
                    return true;
            }
        }
        hw_init_hr_.store((int)E_FAIL);
        message = message.empty() ? "Media Foundation encoder could not be created." : message;
        return false;
    }

    bool Engine::encoder_uses_textures() const
    {
        return mf_ && mf_->accepts_textures();
    }

    bool Engine::ensure_pipeline()
    {
        // Video thread only. Encodes anything rendered into the old pipeline
        // before its textures and encoder are replaced.
        if (pipe_ready_ && pipe_gen_ == canvas_gen_)
            return true;
        if (!canvas_.width())
            return false;
        if (!flush_pending())
            return false;

        D3D11_TEXTURE2D_DESC d = {};
        d.Width = canvas_.width();
        d.Height = canvas_.height();
        d.MipLevels = d.ArraySize = 1;
        d.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
        d.SampleDesc.Count = 1;
        d.Usage = D3D11_USAGE_DEFAULT;
        d.BindFlags = D3D11_BIND_SHADER_RESOURCE;
        ComPtr<ID3D11Texture2D> snap;
        ComPtr<ID3D11ShaderResourceView> snap_srv;
        HRESULT hr = gpu_.dev->CreateTexture2D(&d, nullptr, &snap);
        if (SUCCEEDED(hr))
            hr = gpu_.dev->CreateShaderResourceView(snap.Get(), nullptr, &snap_srv);

        VideoPipeline fresh;
        if (SUCCEEDED(hr))
        {
            const bool hd = out_w_ >= 1280 || out_h_ > 576;
            hr = fresh.create(gpu_, canvas_.width(), canvas_.height(), out_w_, out_h_, hd, scaler_);
        }
        if (FAILED(hr))
        {
            fail(STAGE_ENCODER, hr, "Could not create the GPU video pipeline (0x" + std::to_string((unsigned)hr) + ").");
            return false;
        }
        // The new encoder wraps these exact textures: create it before publishing.
        std::string err;
        if (!create_video_encoders(&err))
        {
            fail(STAGE_ENCODER, E_FAIL, err.empty() ? "Could not recreate the video encoder." : err);
            return false;
        }
        snap_ = std::move(snap);
        snap_srv_ = std::move(snap_srv);
        pipe_ = std::move(fresh);
        pipe_ready_ = true;
        pipe_gen_ = canvas_gen_;
        next_slot_ = 0;
        return true;
    }

    void Engine::push_video(Ref<PacketRef> frame)
    {
        if (!frame)
            return;
        ++encoded_;
        const int64_t bytes = (int64_t)frame->payload.size();
        std::lock_guard<std::mutex> lock(ring_mutex_);
        vbytes_ += bytes;
        const int64_t newest = frame->pts100ns;
        vring_.push_back(std::move(frame));
        // Retain one extra GOP beyond the requested replay window. The save
        // path needs a keyframe at or before its trim point; without slack a
        // full-length save can start late and come out one or two seconds short.
        constexpr int64_t SAVE_PREROLL_SLACK = 2 * SECOND_100NS;
        const int64_t cutoff = newest - (int64_t)ring_seconds_.load() * SECOND_100NS - SAVE_PREROLL_SLACK;
        const int64_t required_start = cutoff;
        // Keep the last keyframe at or before the cutoff so a save can start on one.
        long k = -1;
        for (size_t i = 0; i < vring_.size(); ++i)
        {
            if (vring_[i]->pts100ns > cutoff)
                break;
            if (vring_[i]->keyframe)
                k = (long)i;
        }
        for (long i = 0; i < k; ++i)
        {
            vbytes_ -= (int64_t)vring_.front()->payload.size();
            vring_.pop_front();
        }
        while (vbytes_ > RING_BUDGET_BYTES && vring_.size() > 1)
        {
            size_t next_key = 1;
            while (next_key < vring_.size() && !vring_[next_key]->keyframe)
                ++next_key;
            if (next_key >= vring_.size())
                break;
            // Budget trimming must not discard the save pre-roll we just kept.
            if (vring_[next_key]->pts100ns > required_start)
                break;
            for (size_t i = 0; i < next_key; ++i)
            {
                vbytes_ -= (int64_t)vring_.front()->payload.size();
                vring_.pop_front();
            }
        }
    }

    bool Engine::encode_slot(Pending &pending)
    {
        const uint32_t slot = pending.slot;
        const int64_t pts = pending.pts;
        bool idr = first_frame_;
        if (video_options_.segmented && (idr || pts >= next_segment_pts_))
        {
            idr = true;
            next_segment_pts_ = pts + SECOND_100NS;
        }
        std::vector<Ref<PacketRef>> out;
        std::string err;
        bool ok = false;
        if (encoder_uses_textures())
        {
            const int64_t t0 = qpc100ns();
            ID3D11Texture2D *tex = pipe_.nv12_texture(slot);
            if (!tex)
            {
                ++dropped_;
                return true;
            }
            ok = mf_->encode_texture(tex, pts, idr, out, &err);
            t_encode_ += qpc100ns() - t0;
            ++t_frames_;
        }
        else
        {
            const int64_t t0 = qpc100ns();
            MappedFrame mapped = pipe_.map(slot, std::move(pending.frame));
            const int64_t t1 = qpc100ns();
            if (!mapped)
            {
                ++dropped_;
                return true;
            }
            const auto y = mapped.y(), uv = mapped.uv();
            const size_t y_stride = mapped.y_stride(), uv_stride = mapped.uv_stride();
            ok = mf_->encode_memory(y.data(), y_stride, uv.data(), uv_stride, pts, idr, out, &err);
            const int64_t t2 = qpc100ns();
            t_map_ += t1 - t0;
            t_encode_ += t2 - t1;
            ++t_frames_;
            // MappedFrame's destructor performs the Unmap outside any MT guard.
            const int64_t t3 = qpc100ns();
            mapped = MappedFrame();
            t_unmap_ += qpc100ns() - t3;
        }
        if (!ok)
        {
            fail(STAGE_ENCODER, E_FAIL, "Video encoder failed: " + err);
            return false;
        }
        for (auto &packet : out)
            push_video(std::move(packet));
        return true;
    }

    bool Engine::flush_pending()
    {
        while (!pending_.empty())
        {
            Pending p = std::move(pending_.front());
            pending_.pop_front();
            if (!encode_slot(p))
            {
                running_.store(false);
                return false;
            }
            first_frame_ = false;
        }
        return true;
    }

    bool Engine::process_frame(Ref<VideoFrame> frame)
    {
        ++captured_;
        const int64_t pts = clock_.timeline(frame->pts100ns);
        if (pts < 0)
        {
            signal_ack(frame); // paused / pre-resume frame
            return false;
        }
        const uint32_t w = frame->box.right - frame->box.left;
        const uint32_t h = frame->box.bottom - frame->box.top;
        if (w < 2 || h < 2)
        {
            signal_ack(frame);
            return false;
        }
        if (!ensure_canvas_size(w, h))
        {
            signal_ack(frame);
            return false;
        }

        const uint32_t dx = ((canvas_.width() - w) / 2) & ~1u;
        const uint32_t dy = ((canvas_.height() - h) / 2) & ~1u;
        const bool moved = w != last_place_w_ || h != last_place_h_ || dx != last_dx_ || dy != last_dy_;
        const int64_t s0 = qpc100ns();
        canvas_.submit(frame->texture.Get(), frame->box, dx, dy, need_clear_ || moved);
        t_submit_ += qpc100ns() - s0;
        // The GPU copy is queued; release the capture resource immediately.
        signal_ack(frame);
        need_clear_ = false;
        last_place_w_ = w;
        last_place_h_ = h;
        last_dx_ = dx;
        last_dy_ = dy;

        const int64_t lp = latest_pts_.load();
        if (pts <= lp)
        {
            ++dropped_;
            latest_pts_.store(lp + 1);
        }
        else
        {
            latest_pts_.store(pts);
        }
        ++frame_seq_;
        return true;
    }

    bool Engine::video_init(const VideoInit &init)
    {
        video_options_ = init.options;
        scaler_ = init.scaler;
        max_w_ = init.cap_w;
        max_h_ = init.cap_h;
        canvas_gen_ = 0;
        if (!recreate_canvas(init.canvas_w, init.canvas_h) || !ensure_pipeline())
            return false;
        pending_.clear();
        next_slot_ = 0;
        first_frame_ = true;
        next_segment_pts_ = 0;
        need_clear_ = true;
        latest_pts_.store(-1);
        frame_seq_ = 0;
        return true;
    }

    void Engine::video_loop()
    {
        uint64_t seen_ctl = ctl_seq_.load(std::memory_order_acquire);
        int64_t last_alive = qpc100ns();
        int64_t pts = 0;

        while (running_.load())
        {
            ++heartbeat_;

            // Control commands first, so Stop/Pause are not starved by frames.
            const uint64_t seq = ctl_seq_.load(std::memory_order_acquire);
            while (seen_ctl != seq)
            {
                auto cmd = ctl_.pop();
                ++seen_ctl;
                if (!cmd)
                    break;
                handle_command(cmd);
            }
            if (!running_.load())
                break;

            SpscQueue<Ref<VideoFrame>> &q = active_method_.load() == (int)CaptureMethod::Dxgi
                                                 ? q_raw_dxgi_
                                                 : q_raw_wgc_;
            Ref<VideoFrame> frame;
            const bool have_new = q.try_pop(frame) && process_frame(std::move(frame));
            if (have_new)
            {
                pts = latest_pts_.load();
                const int64_t p0 = qpc100ns();
                canvas_.snapshot_into(snap_.Get());
                t_snapshot_ += qpc100ns() - p0;
                const uint32_t slot = next_slot_;
                next_slot_ = (next_slot_ + 1) % VideoPipeline::kSlots;
                // GPU encoders already hold the six slot leases (returned by their
                // slot leases). Only the CPU staging path needs an engine-held lease,
                // released by MappedFrame after Map/Unmap.
                const bool gpu_path = encoder_uses_textures();
                Ref<GpuFrame> lease;
                if (!gpu_path)
                    lease = pipe_.acquire_slot(slot);
                if (!gpu_path && !lease)
                {
                    ++dropped_;
                }
                else
                {
                    const int64_t r0 = qpc100ns();
                    pipe_.render(snap_srv_.Get(), slot, !gpu_path);
                    t_render_ += qpc100ns() - r0;
                    pending_.push_back(Pending{slot, pts, std::move(lease)});
                    // Keep a couple of frames in flight so Map() has less chance
                    // to stall a slow whole-screen capture.
                    while (pending_.size() > 2) // kPendingMax: at most 3 outstanding
                    {
                        Pending p = std::move(pending_.front());
                        pending_.pop_front();
                        if (!encode_slot(p))
                        {
                            running_.store(false);
                            break;
                        }
                        first_frame_ = false;
                    }
                }
            }
            else if (!pending_.empty())
            {
                Pending p = std::move(pending_.front());
                pending_.pop_front();
                if (!encode_slot(p))
                {
                    running_.store(false);
                    break;
                }
                first_frame_ = false;
            }

            const int64_t now = qpc100ns();
            if (now - last_alive > SECOND_100NS / 4)
            {
                last_alive = now;
                watch_target();
            }
            if (!have_new && pending_.empty())
                std::this_thread::sleep_for(std::chrono::milliseconds(1));
        }
    }

    void Engine::video_exit()
    {
        // Drain the one-frame-delayed staging slot and flush the active encoder
        // while every GPU texture is still alive.
        if (mf_)
        {
            if (pipe_ready_)
                flush_pending();
            std::vector<Ref<PacketRef>> out;
            std::string err;
            const bool ok = mf_->flush(out, &err);
            if (ok)
            {
                for (auto &packet : out)
                    push_video(std::move(packet));
            }
            else
            {
                fail(STAGE_ENCODER, E_FAIL, "Video encoder flush failed: " + err);
            }
        }
        mf_.reset();
        selection_ = VideoSelection();
        pending_.clear();
        pipe_ = VideoPipeline();
        pipe_ready_ = false;
        snap_.Reset();
        snap_srv_.Reset();
        canvas_ = GpuCanvas();
    }

    void Engine::watchdog_loop()
    {
        // The video thread's heartbeat only advances while that lane is live.
        // If it stops for three seconds, the capture queue will fill and the
        // engine would otherwise keep reporting a healthy, growing replay ring.
        int64_t last = heartbeat_.load();
        int64_t stamp = qpc100ns();
        while (!watchdog_stop_.load())
        {
            Sleep(250);
            if (!running_.load())
            {
                last = heartbeat_.load();
                stamp = qpc100ns();
                continue;
            }
            const int64_t now = qpc100ns();
            const int64_t hb = heartbeat_.load();
            if (hb != last)
            {
                last = hb;
                stamp = now;
                continue;
            }
            if (now - stamp > 3 * SECOND_100NS)
            {
                fail(STAGE_ENCODER, E_FAIL,
                     "Video pipeline stopped responding. Capturing was stopped; restart buffering to recover.");
                running_.store(false);
                q_raw_wgc_.stop();
                q_raw_dxgi_.stop();
                return;
            }
        }
    }

    // -------------------------------------------------------------------------
    // Audio producers: two dedicated WASAPI threads, one shared Clock.
    // -------------------------------------------------------------------------
    void Engine::flush_audio_accumulator(bool mic)
    {
        AudioAccumulator &acc = mic ? mic_acc_ : sys_acc_;
        if (!acc.frames || acc.pcm.empty())
        {
            acc.clear();
            return;
        }

        Ref<AudioBlock> block = audio_pool_.acquire();
        if (!block)
        {
            // Recycle the oldest replay block before dropping live audio.
            std::lock_guard<std::mutex> lock(ring_mutex_);
            auto &ring = mic ? mic_ring_ : sys_ring_;
            if (!ring.empty())
                ring.pop_front();
            block = audio_pool_.acquire();
        }
        if (!block)
        {
            acc.clear();
            return;
        }
        block->pcm.assign(acc.pcm.begin(), acc.pcm.end());
        block->frames = mic ? (uint32_t)acc.pcm.size() : acc.frames;
        block->pts100ns = acc.pts;
        {
            std::lock_guard<std::mutex> lock(ring_mutex_);
            auto &ring = mic ? mic_ring_ : sys_ring_;
            ring.push_back(std::move(block));
            const int64_t cutoff = acc.pts - ((int64_t)ring_seconds_.load() + 3) * SECOND_100NS;
            while (!ring.empty() && ring.front()->pts100ns < cutoff)
                ring.pop_front();
        }
        acc.clear();
    }

    void Engine::push_system(const int16_t *data, uint32_t frames, int64_t qpc, bool discontinuity)
    {
        const int64_t pts = clock_.timeline(qpc);
        if (discontinuity)
            flush_audio_accumulator(false); // WASAPI reported a broken stream: never bridge it
        if (pts < 0 || !running_.load())
        {
            flush_audio_accumulator(false); // never bridge a pause boundary
            return;
        }
        if (sys_acc_.frames && sys_acc_.pts >= 0)
        {
            if (pts < sys_acc_.pts)
                flush_audio_accumulator(false);
            else
            {
                const int64_t end = sys_acc_.pts + sys_acc_.frames * SECOND_100NS / AUDIO_RATE;
                const int64_t gap = pts - end;
                if (gap >= 2 * SECOND_100NS)
                    flush_audio_accumulator(false); // source restart: leave a real gap
                else if (gap * AUDIO_RATE > SECOND_100NS)
                {
                    const uint32_t missing = (uint32_t)(gap * AUDIO_RATE / SECOND_100NS);
                    sys_acc_.pcm.insert(sys_acc_.pcm.end(), (size_t)missing * AUDIO_CHANNELS, 0);
                    sys_acc_.frames += missing;
                }
            }
        }
        if (!sys_acc_.frames)
            sys_acc_.pts = pts;
        sys_acc_.pcm.insert(sys_acc_.pcm.end(), data, data + (size_t)frames * AUDIO_CHANNELS);
        sys_acc_.frames += frames;
        if (sys_acc_.frames >= kAudioAccumFrames)
            flush_audio_accumulator(false);
    }

    void Engine::push_mic(const int16_t *data, uint32_t frames, int64_t qpc, bool discontinuity)
    {
        const int64_t pts = clock_.mic_timeline(qpc);
        if (discontinuity)
            flush_audio_accumulator(true); // WASAPI reported a broken stream: never bridge it
        if (pts < 0 || !running_.load())
        {
            flush_audio_accumulator(true);
            return;
        }
        if (mic_acc_.frames && mic_acc_.pts >= 0)
        {
            if (pts < mic_acc_.pts)
                flush_audio_accumulator(true);
            else
            {
                const int64_t end = mic_acc_.pts + mic_acc_.frames * SECOND_100NS / AUDIO_RATE;
                const int64_t gap = pts - end;
                if (gap >= 2 * SECOND_100NS)
                    flush_audio_accumulator(true);
                else if (gap * AUDIO_RATE > SECOND_100NS)
                {
                    const uint32_t missing = (uint32_t)(gap * AUDIO_RATE / SECOND_100NS);
                    mic_acc_.pcm.insert(mic_acc_.pcm.end(), missing, 0); // mono
                    mic_acc_.frames += missing;
                }
            }
        }
        if (!mic_acc_.frames)
            mic_acc_.pts = pts;
        mic_acc_.pcm.insert(mic_acc_.pcm.end(), data, data + frames); // mono
        mic_acc_.frames += frames;
        if (mic_acc_.frames >= kAudioAccumFrames)
            flush_audio_accumulator(true);
    }

    // -------------------------------------------------------------------------
    // Start / stop
    // -------------------------------------------------------------------------
    int Engine::start(int monitor_index, int ring_seconds, int fps, int bitrate_kbps, int cap_w, int cap_h, bool audio,
                      bool mic, const wchar_t *mic_device, int pref, int method, int64_t *start_qpc)
    {
        std::lock_guard<std::mutex> life(lifecycle_mutex_);
        if (running_.load())
            return IR_OK;
        set_last_error("");
        has_error_.store(false);
        fail_stage_.store(0);
        fail_hr_.store(0);
        last_hr_.store(0);
        cap_hr_.store(0);
        hw_init_hr_.store(0);
        encoded_.store(0);
        dropped_.store(0);
        heartbeat_.store(0);
        captured_.store(0);

        ComGuard com;

        auto mons = enumerate_monitors();
        if (mons.empty())
        {
            fail(STAGE_CREATE_ITEM, E_FAIL, "No display was found to capture.");
            return IR_FAIL;
        }
        const int resolved_monitor = (monitor_index < 0 || (size_t)monitor_index >= mons.size()) ? 0 : monitor_index;
        const MonInfo &mon = mons[(size_t)resolved_monitor];

        ring_seconds_.store(std::max(5, ring_seconds));
        const int resolved_fps = std::clamp(fps, 1, 240);
        const uint32_t bps = (uint32_t)std::max(250, bitrate_kbps) * 1000u;
        const auto [out_w, out_h] = fit_resolution(mon.w, mon.h, cap_w, cap_h);
        video_options_ = options_from_pref(pref, (int)out_w, (int)out_h, resolved_fps, bps);
        std::string selection_error;
        if (!validate_video_selection(video_options_, &selection_error))
        {
            fail(STAGE_ENCODER, E_FAIL, selection_error);
            return IR_FAIL;
        }

        {
            std::lock_guard<std::mutex> lock(config_mutex_);
            monitor_index_ = resolved_monitor;
            fps_ = resolved_fps;
            bps_ = bps;
            out_w_ = out_w;
            out_h_ = out_h;
            pref_ = pref;
            want_audio_ = audio;
            mic_device_ = mic_device ? mic_device : L"";
            options_ = pending_options_;
        }
        {
            std::lock_guard<std::mutex> lock(ring_mutex_);
            vring_.clear();
            sys_ring_.clear();
            mic_ring_.clear();
            vbytes_ = 0;
        }
        sys_acc_.clear();
        mic_acc_.clear();
        clock_.start(qpc100ns(), mic);
        if (start_qpc)
            *start_qpc = clock_.start_qpc();

        ComPtr<IDXGIAdapter> adapter = adapter_for_monitor(mon.handle);
        HRESULT hr = create_gpu_device(adapter.Get(), &gpu_);
        if (FAILED(hr))
            hr = create_gpu_device(nullptr, &gpu_);
        if (FAILED(hr))
        {
            fail(STAGE_CREATE_DEVICE, hr, "Could not create a Direct3D 11 device.");
            return IR_FAIL;
        }

        int initial_cap_w = mon.w, initial_cap_h = mon.h;
        {
            std::lock_guard<std::mutex> lock(config_mutex_);
            const HWND win = (pending_window_ && IsWindow(pending_window_)) ? pending_window_ : nullptr;
            if (win && window_size(win, (pending_options_ & CAP_CLIENT_AREA) != 0, &initial_cap_w, &initial_cap_h) &&
                (initial_cap_w < 2 || initial_cap_h < 2))
            {
                initial_cap_w = mon.w;
                initial_cap_h = mon.h;
            }
            if (hint_w_ >= 2 && hint_h_ >= 2)
            {
                initial_cap_w = (int)hint_w_;
                initial_cap_h = (int)hint_h_;
            }
        }
        const auto [canvas_w, canvas_h] = canvas_size_for((uint32_t)std::max(2, initial_cap_w),
                                                          (uint32_t)std::max(2, initial_cap_h), out_w, out_h);

        VideoInit init;
        init.canvas_w = canvas_w;
        init.canvas_h = canvas_h;
        init.cap_w = (uint32_t)std::max(2, initial_cap_w);
        init.cap_h = (uint32_t)std::max(2, initial_cap_h);
        init.hd = out_w >= 1280 || out_h > 576;
        init.scaler = scaler_from_env();
        init.options = video_options_;

        q_raw_wgc_.reset();
        q_raw_dxgi_.reset();
        ctl_.reset();
        ctl_seq_.store(0);

        std::binary_semaphore init_done{0};
        std::atomic<int> init_rc{IR_FAIL};
        running_.store(true);
        video_thread_ = std::thread([this, init, &init_done, &init_rc]
        {
            int rc = IR_FAIL;
            try
            {
                rc = video_init(init) ? IR_OK : IR_FAIL;
            }
            catch (const std::exception &e)
            {
                fail(STAGE_ENCODER, E_FAIL, std::string("Video initialization failed: ") + e.what());
            }
            catch (...)
            {
                fail(STAGE_ENCODER, E_FAIL, "Video initialization failed.");
            }
            if (rc != IR_OK)
            {
                video_exit();
                init_rc.store(rc);
                init_done.release();
                return;
            }
            init_rc.store(IR_OK);
            init_done.release();
            try
            {
                video_loop();
            }
            catch (const std::exception &e)
            {
                fail(STAGE_ENCODER, E_FAIL, std::string("Video thread failed: ") + e.what());
            }
            catch (...)
            {
                fail(STAGE_ENCODER, E_FAIL, "Video thread failed.");
            }
            video_exit();
        });
        init_done.acquire();
        if (init_rc.load() != IR_OK)
        {
            running_.store(false);
            q_raw_wgc_.stop();
            q_raw_dxgi_.stop();
            if (video_thread_.joinable())
                video_thread_.join();
            if (!has_error_.load())
                fail(STAGE_ENCODER, E_FAIL, "Video pipeline could not be created.");
            return IR_FAIL;
        }

        CaptureTarget t;
        t.fps = resolved_fps;
        {
            std::lock_guard<std::mutex> lock(config_mutex_);
            t.monitor = mon.handle;
            t.window = (pending_window_ && IsWindow(pending_window_)) ? pending_window_ : nullptr;
            t.options = options_;
        }
        hr = open_capture(t, method == (int)CaptureMethod::Dxgi ? CaptureMethod::Dxgi : CaptureMethod::Wgc);
        if (FAILED(hr))
        {
            running_.store(false);
            q_raw_wgc_.stop();
            q_raw_dxgi_.stop();
            if (video_thread_.joinable())
                video_thread_.join();
            fail(fail_stage_.load() ? fail_stage_.load() : STAGE_START_CAPTURE, hr,
                 "Screen capture could not start (0x" + std::to_string((unsigned)hr) + ").");
            return IR_FAIL;
        }

        ensure_save_thread();
        watchdog_stop_.store(false);
        watchdog_thread_ = std::thread([this] { watchdog_loop(); });
        {
            std::lock_guard<std::mutex> lock(config_mutex_);
            audio_target_ = (t.window && (t.options & CAP_PROCESS_AUDIO)) ? t.window : nullptr;
        }
        {
            std::lock_guard<std::mutex> lock(audio_owner_mutex_);
            if (audio)
            {
                sys_audio_ = std::make_unique<AudioSource>(false, L"", audio_target_,
                                                           [this](const int16_t *d, uint32_t n, int64_t q, bool disc)
                                                           { push_system(d, n, q, disc); }, &sys_counters_);
                sys_audio_->start();
            }
            if (mic)
            {
                mic_audio_ = std::make_unique<AudioSource>(true, mic_device_, nullptr,
                                                           [this](const int16_t *d, uint32_t n, int64_t q, bool disc)
                                                           { push_mic(d, n, q, disc); }, &mic_counters_);
                mic_audio_->start();
            }
        }
        return IR_OK;
    }

    void Engine::stop()
    {
        std::lock_guard<std::mutex> life(lifecycle_mutex_);
        watchdog_stop_.store(true);
        if (watchdog_thread_.joinable())
            watchdog_thread_.join();
        const bool was_running = running_.load();

        // Stop capture producers first. Stopping the queues first guarantees no
        // producer can remain blocked in push() while its source is joined.
        if (was_running && video_thread_.joinable())
        {
            q_raw_wgc_.stop();
            q_raw_dxgi_.stop();
            (void)post_control(ControlKind::Stop, 0, 0, 0, 0, nullptr, std::chrono::seconds(5));
        }
        else
        {
            q_raw_wgc_.stop();
            q_raw_dxgi_.stop();
        }
        running_.store(false);

        {
            std::lock_guard<std::mutex> lock(capture_mutex_);
            if (capture_)
            {
                capture_->stop();
                capture_.reset();
            }
        }
        {
            std::lock_guard<std::mutex> lock(audio_owner_mutex_);
            if (sys_audio_)
            {
                sys_audio_->stop();
                sys_audio_.reset();
            }
            if (mic_audio_)
            {
                mic_audio_->stop();
                mic_audio_.reset();
            }
        }
        flush_audio_accumulator(false);
        flush_audio_accumulator(true);

        if (video_thread_.joinable())
            video_thread_.join();

        ctl_.stop_and_drain();
        {
            std::lock_guard<std::mutex> lock(ctl_post_mutex_);
            ctl_seq_.fetch_add(1, std::memory_order_release);
        }
        stop_save();

        {
            std::lock_guard<std::mutex> lock(ring_mutex_);
            vring_.clear();
            sys_ring_.clear();
            mic_ring_.clear();
            vbytes_ = 0;
        }
        // GPU device is released last, after capture, video and save teardown.
        gpu_ = GpuContext();
        q_raw_wgc_.reset();
        q_raw_dxgi_.reset();
    }

    // -------------------------------------------------------------------------
    // Setters
    // -------------------------------------------------------------------------
    int Engine::set_target(HWND window, uint32_t options)
    {
        std::lock_guard<std::mutex> lock(config_mutex_);
        if (running_.load())
            return IR_FAIL; // chosen before start; IR_SetCaptureSource switches live
        int w = 0, h = 0;
        if (window && !window_size(window, (options & CAP_CLIENT_AREA) != 0, &w, &h))
            return IR_FAIL;
        pending_window_ = window;
        pending_options_ = options & 255;
        return IR_OK;
    }

    int Engine::set_source(int method, int monitor_index, HWND window, uint32_t options)
    {
        if (!running_.load())
            return IR_FAIL;
        return post_control(ControlKind::SwitchSource, method, monitor_index, (int)(options & 255), 0, window);
    }

    int Engine::set_canvas_hint(int w, int h)
    {
        std::lock_guard<std::mutex> lock(config_mutex_);
        if (running_.load())
            return IR_FAIL;
        hint_w_ = (uint32_t)std::clamp(w, 0, 16384);
        hint_h_ = (uint32_t)std::clamp(h, 0, 16384);
        return IR_OK;
    }

    int Engine::set_paused(bool paused)
    {
        if (running_.load())
            return post_control(ControlKind::Pause, paused ? 1 : 0);
        clock_.set_paused(paused);
        return IR_OK;
    }

    // -------------------------------------------------------------------------
    // Save control plane
    // -------------------------------------------------------------------------
    uint64_t Engine::audio_bytes_snapshot()
    {
        uint64_t bytes = 0;
        std::lock_guard<std::mutex> lock(ring_mutex_);
        for (const auto &b : sys_ring_)
            bytes += b->pcm.size() * sizeof(int16_t);
        for (const auto &b : mic_ring_)
            bytes += b->pcm.size() * sizeof(int16_t);
        return bytes;
    }

    bool Engine::has_video_frames()
    {
        std::lock_guard<std::mutex> lock(ring_mutex_);
        return !vring_.empty();
    }

    void Engine::register_job(const std::shared_ptr<SaveJob> &job)
    {
        std::lock_guard<std::mutex> lock(jobs_mutex_);
        jobs_[job->id] = job;
        job_order_.push_back(job->id);
        while (job_order_.size() > kJobRing)
        {
            // Eviction only unlinks the map entry: waiters/pollers keep their own
            // shared_ptr, so the SaveJob is never destroyed underneath them.
            jobs_.erase(job_order_.front());
            job_order_.pop_front();
        }
        remember_last_job(job);
    }

    void Engine::unregister_job(uint64_t id)
    {
        std::lock_guard<std::mutex> lock(jobs_mutex_);
        jobs_.erase(id);
        auto it = std::find(job_order_.begin(), job_order_.end(), id);
        if (it != job_order_.end())
            job_order_.erase(it);
    }

    std::shared_ptr<SaveJob> Engine::retain_job(uint64_t id)
    {
        std::lock_guard<std::mutex> lock(jobs_mutex_);
        auto it = jobs_.find(id);
        if (it != jobs_.end())
            return it->second;
        return retain_global_last_job(id);
    }

    int Engine::poll_save(uint64_t id, int *progress, int *result)
    {
        std::shared_ptr<SaveJob> job;
        {
            std::lock_guard<std::mutex> lock(jobs_mutex_);
            auto it = jobs_.find(id);
            if (it == jobs_.end())
                return 0;
            job = it->second; // strong reference before touching the job
        }
        if (progress)
            *progress = std::clamp(job->progress.load(std::memory_order_relaxed), 0, 100);
        if (result)
            *result = job->result.load(std::memory_order_relaxed);
        return 1;
    }

    void Engine::finish_job(const std::shared_ptr<SaveJob> &job, int result, int progress)
    {
        if (!job)
            return;
        job->result.store(result, std::memory_order_release);
        publish_last_progress(job, progress);
        job->state.store(SaveState::Done, std::memory_order_release);
        job->done.release();
    }

    void Engine::ensure_save_thread()
    {
        std::lock_guard<std::mutex> lock(save_lifecycle_mutex_);
        if (save_thread_.joinable())
            return;
        if (save_stopped_.load())
        {
            q_save_.reset();
            save_stopped_.store(false);
        }
        save_thread_ = std::thread([this] { save_loop(); });
    }

    void Engine::stop_save()
    {
        std::lock_guard<std::mutex> lock(save_lifecycle_mutex_);
        save_stopped_.store(true);
        q_save_.stop();
        if (save_thread_.joinable())
            save_thread_.join();
        std::shared_ptr<SaveJob> job;
        while (q_save_.try_pop(job))
            finish_job(job, IR_FAIL, -1);
    }

    int Engine::begin_save(const std::wstring &path, int seconds_back, uint64_t *job_id)
    {
        std::lock_guard<std::mutex> life(lifecycle_mutex_);
        if (!job_id)
            return IR_FAIL;
        *job_id = 0;
        try
        {
            std::string mem_detail;
            if (!save_memory_available(audio_bytes_snapshot(), &mem_detail))
            {
                clear_last_job(-1);
                set_last_error("Not enough committed memory to save the clip (" + mem_detail + ").");
                return IR_LOW_MEMORY;
            }
            if (!has_video_frames())
            {
                clear_last_job(-1);
                return IR_NOT_ENOUGH_DATA;
            }

            ensure_save_thread();
            auto job = std::make_shared<SaveJob>();
            job->id = next_job_id_.fetch_add(1) + 1;
            job->path = path;
            job->secondsBack = seconds_back;
            job->segmented = video_options_.segmented;
            job->progress.store(0, std::memory_order_relaxed);
            job->result.store(IR_PENDING, std::memory_order_relaxed);
            job->state.store(SaveState::Queued, std::memory_order_relaxed);
            register_job(job);
            if (!q_save_.try_push(job))
            {
                unregister_job(job->id);
                finish_job(job, IR_FAIL, -1);
                clear_last_job(-1);
                set_last_error("Save queue is full.");
                return IR_FAIL;
            }
            *job_id = job->id;
            return IR_OK;
        }
        catch (const std::bad_alloc &)
        {
            clear_last_job(-1);
            set_last_error("Not enough memory to save the clip.");
            return IR_LOW_MEMORY;
        }
        catch (...)
        {
            clear_last_job(-1);
            return IR_FAIL;
        }
    }

    void Engine::save_loop()
    {
        ComGuard com;
        std::shared_ptr<SaveJob> job;
        while (q_save_.pop(job))
        {
            if (save_stopped_.load())
            {
                finish_job(job, IR_FAIL, -1);
                continue;
            }
            job->state.store(SaveState::Running, std::memory_order_relaxed);
            int rc = IR_FAIL;
            try
            {
                rc = run_save(job);
            }
            catch (const std::bad_alloc &)
            {
                set_last_error("Not enough memory to save the clip.");
                rc = IR_LOW_MEMORY;
            }
            catch (...)
            {
                rc = IR_FAIL;
            }
            finish_job(job, rc, rc == IR_OK ? 100 : -1);
        }
    }

    static std::vector<int16_t> mix_range(int64_t t0, int64_t t1,
                                          const std::vector<Ref<AudioBlock>> &sys,
                                          const std::vector<Ref<AudioBlock>> &mic)
    {
        const int64_t n = (t1 - t0) * AUDIO_RATE / SECOND_100NS;
        std::vector<int32_t> acc((size_t)n * 2, 0);
        bool have_sys = false, have_mic = false;
        auto place = [&](const AudioBlock &b, bool mono, bool &seen)
        {
            const int64_t first = (b.pts100ns - t0) * AUDIO_RATE / SECOND_100NS;
            for (uint32_t i = 0; i < b.frames; ++i)
            {
                const int64_t at = first + i;
                if (at < 0 || at >= n)
                    continue;
                seen = true;
                if (mono)
                {
                    acc[(size_t)at * 2] += b.pcm[i];
                    acc[(size_t)at * 2 + 1] += b.pcm[i];
                }
                else
                {
                    acc[(size_t)at * 2] += b.pcm[(size_t)i * 2];
                    acc[(size_t)at * 2 + 1] += b.pcm[(size_t)i * 2 + 1];
                }
            }
        };
        for (const auto &b : sys)
            place(*b, false, have_sys);
        for (const auto &b : mic)
            place(*b, true, have_mic);

        // Leave headroom when both sources overlap. Scaling the whole chunk only
        // when it would clip keeps ordinary single-source audio byte-identical
        // while avoiding the hard-corner click of two clipped waveforms.
        int peak = 0;
        for (int v : acc)
            peak = std::max(peak, std::abs(v));
        const int limit = (have_sys && have_mic) ? 30000 : 32767;
        const double gain = peak > limit ? (double)limit / (double)peak : 1.0;

        std::vector<int16_t> out(acc.size());
        for (size_t i = 0; i < acc.size(); ++i)
        {
            long long v = gain < 1.0 ? std::llround((double)acc[i] * gain) : acc[i];
            out[i] = (int16_t)std::clamp(v, -32768LL, 32767LL);
        }
        return out;
    }

    int Engine::run_save(const std::shared_ptr<SaveJob> &job)
    {
        std::vector<Ref<PacketRef>> frames;
        std::vector<Ref<AudioBlock>> sys, mic;
        {
            std::lock_guard<std::mutex> lock(ring_mutex_);
            frames.assign(vring_.begin(), vring_.end());
            sys.assign(sys_ring_.begin(), sys_ring_.end());
            mic.assign(mic_ring_.begin(), mic_ring_.end());
        }
        if (frames.empty())
            return IR_NOT_ENOUGH_DATA;

        const int64_t first = frames.front()->pts100ns;
        const int64_t dur = SECOND_100NS / std::max(1, fps_);
        const int64_t end = frames.back()->pts100ns + dur;
        publish_last_progress(job, 10);

        std::vector<int16_t> mixed;
        std::vector<AudioChunk> audio;
        if (!sys.empty() || !mic.empty())
        {
            int64_t t0 = first;
            if (job->secondsBack > 0)
                t0 = std::max(first, end - (int64_t)job->secondsBack * SECOND_100NS - 6 * SECOND_100NS);
            // Round the generated PCM up to a whole 48 kHz frame so the writer
            // always has a sample covering the last partial 100 ns interval.
            // Use the exact 100 ns / 48 kHz ratio. Truncating it to 208 drifts
            // ~0.16% and makes the AAC writer see tiny repeated gaps/overlaps.
            const int64_t audio_frames =
                ((end - t0) * AUDIO_RATE + SECOND_100NS - 1) / SECOND_100NS;
            const int64_t audio_end = t0 + audio_frames * SECOND_100NS / AUDIO_RATE;
            mixed = mix_range(t0, audio_end, sys, mic);
            audio.push_back({reinterpret_cast<const uint8_t *>(mixed.data()), (uint32_t)(mixed.size() / 2), t0});
        }
        publish_last_progress(job, 30);
        return write_mf_file(job->path, frames, audio, active_codec_, fps_, (int)out_w_, (int)out_h_,
                             job->secondsBack, end, job->segmented);
    }

    // -------------------------------------------------------------------------
    // Status / telemetry getters
    // -------------------------------------------------------------------------
    void Engine::status(int *buffering, int *ring_seconds, int64_t *encoded, int64_t *dropped, int *has_error,
                        int64_t *a_frames, int64_t *a_events, int64_t *a_got, int64_t *heartbeat)
    {
        if (buffering)
            *buffering = running_.load() ? 1 : 0;
        if (ring_seconds)
            *ring_seconds = ring_seconds_.load();
        if (encoded)
            *encoded = encoded_;
        if (dropped)
            *dropped = dropped_;
        if (has_error)
            *has_error = has_error_ ? 1 : 0;
        if (a_frames)
            *a_frames = sys_counters_.frames;
        if (a_events)
            *a_events = sys_counters_.events;
        if (a_got)
            *a_got = sys_counters_.got_frames;
        if (heartbeat)
            *heartbeat = heartbeat_;
    }

    void Engine::mic_status(int64_t *frames, int64_t *events, int64_t *got, int64_t *fail_hr, int *sys_peak,
                            int *mic_peak, wchar_t *name, int name_cap)
    {
        if (sys_peak)
            *sys_peak = sys_counters_.peak;
        if (mic_peak)
            *mic_peak = mic_counters_.peak;
        if (frames)
            *frames = mic_counters_.frames;
        if (events)
            *events = mic_counters_.events;
        if (got)
            *got = mic_counters_.got_frames;
        if (fail_hr)
            *fail_hr = mic_counters_.fail_hr;
        if (name && name_cap > 0)
        {
            std::lock_guard<std::mutex> lock(mic_counters_.name_mutex);
            const size_t n = std::min<size_t>(mic_counters_.active_name.size(), (size_t)name_cap - 1);
            std::memcpy(name, mic_counters_.active_name.data(), n * sizeof(wchar_t));
            name[n] = 0;
        }
    }

    void Engine::audio_diagnostics(int which, int64_t *activate_hr, int64_t *init_hr, int64_t *start_hr, int *tag,
                                   int *rate, int *channels, int *bits, int *is_float, int *convert_failures,
                                   int64_t *last_fail_hr)
    {
        const AudioCounters &c = which == 1 ? mic_counters_ : sys_counters_;
        if (activate_hr)
            *activate_hr = c.activate_hr;
        if (init_hr)
            *init_hr = c.init_hr;
        if (start_hr)
            *start_hr = c.start_hr;
        if (tag)
            *tag = c.mix_tag;
        if (rate)
            *rate = c.mix_rate;
        if (channels)
            *channels = c.mix_channels;
        if (bits)
            *bits = c.mix_bits;
        if (is_float)
            *is_float = c.mix_float;
        if (convert_failures)
            *convert_failures = c.convert_failures;
        if (last_fail_hr)
            *last_fail_hr = c.fail_hr;
    }

    void Engine::telemetry(int64_t *captured, int64_t *paced, int64_t *rejected, int64_t *encoded, int64_t *dropped,
                           int *encoder, int64_t *last_pts)
    {
        if (captured)
            *captured = captured_;
        if (encoded)
            *encoded = encoded_;
        if (dropped)
            *dropped = dropped_;
        if (encoder)
            *encoder = active_encoder_;
        if (last_pts)
            *last_pts = latest_pts_.load();
        CaptureSource *cap = nullptr;
        {
            std::lock_guard<std::mutex> lock(capture_mutex_);
            cap = capture_.get();
        }
        if (paced)
            *paced = cap ? cap->paced_frames() : 0;
        if (rejected)
            *rejected = cap ? cap->rejected_frames() : 0;
    }

    void Engine::timing(double *map_ms, double *encode_ms, double *render_ms, double *submit_ms, double *snapshot_ms,
                        double *unmap_ms, int64_t *frames)
    {
        const int64_t n = t_frames_.load();
        if (frames)
            *frames = n;
        const double div = n > 0 ? (double)n : 1.0;
        if (map_ms)
            *map_ms = (double)t_map_.load() / div / 10000.0;
        if (encode_ms)
            *encode_ms = (double)t_encode_.load() / div / 10000.0;
        if (render_ms)
            *render_ms = (double)t_render_.load() / div / 10000.0;
        if (submit_ms)
            *submit_ms = (double)t_submit_.load() / div / 10000.0;
        if (snapshot_ms)
            *snapshot_ms = (double)t_snapshot_.load() / div / 10000.0;
        if (unmap_ms)
            *unmap_ms = (double)t_unmap_.load() / div / 10000.0;
    }

    // -------------------------------------------------------------------------
    // Probe
    // -------------------------------------------------------------------------
    static uint32_t probe_all_encoders()
    {
        constexpr int W = 1280, H = 720, FPS = 30;
        constexpr uint32_t BPS = 4'000'000;
        uint32_t mask = 0;
        const MfVendor vendors[] = {MfVendor::Qsv, MfVendor::Vp12, MfVendor::Nvenc, MfVendor::Amf,
                                    MfVendor::Software};
        const MfCodec codecs[] = {MfCodec::H264, MfCodec::Hevc, MfCodec::Av1};
        for (MfVendor vendor : vendors)
        {
            for (MfCodec codec : codecs)
            {
                try
                {
                    HRESULT hr = S_OK;
                    if (probe_mf_encoder(codec, vendor, W, H, FPS, BPS, &hr))
                        mask |= mf_availability_bit(codec, vendor);
                }
                catch (...)
                {
                }
            }
        }
        return mask;
    }

} // namespace ir

// =============================================================================
// Exported C ABI (must match InstantReplay.Native/IrNative.cs)
// =============================================================================

using ir::Engine;

#define IR_EXPORT extern "C" __declspec(dllexport)
#define GUARD(expr, fallback) \
    try                       \
    {                         \
        return (expr);        \
    }                         \
    catch (...)               \
    {                         \
        return (fallback);    \
    }

static void copy_wide(const std::wstring &text, wchar_t *out, int cap)
{
    if (!out || cap <= 0)
        return;
    const size_t n = std::min<size_t>(text.size(), (size_t)cap - 1);
    std::memcpy(out, text.data(), n * sizeof(wchar_t));
    out[n] = 0;
}

IR_EXPORT int IR_Create(void **engine)
{
    try
    {
        if (!engine)
            return ir::IR_FAIL;
        *engine = new Engine();
        return ir::IR_OK;
    }
    catch (...)
    {
        return ir::IR_LOW_MEMORY;
    }
}

IR_EXPORT void IR_Destroy(void *engine)
{
    try
    {
        delete static_cast<Engine *>(engine);
    }
    catch (...)
    {
    }
}

IR_EXPORT int IR_Start(void *engine, int monitor_index, int ring_seconds, int fps, int bitrate_kbps, int cap_w, int cap_h,
                       int capture_audio, int capture_mic, const wchar_t *mic_device, int encoder_pref,
                       int capture_method, long long *start_qpc)
{
    if (!engine)
        return ir::IR_FAIL;
    try
    {
        int64_t q = 0;
        int rc = static_cast<Engine *>(engine)->start(monitor_index, ring_seconds, fps, bitrate_kbps, cap_w, cap_h,
                                                     capture_audio != 0, capture_mic != 0, mic_device, encoder_pref,
                                                     capture_method, &q);
        if (start_qpc)
            *start_qpc = q;
        return rc;
    }
    catch (const std::exception &e)
    {
        ir::set_last_error(std::string("Engine start failed: ") + e.what());
        return ir::IR_FAIL;
    }
    catch (...)
    {
        return ir::IR_FAIL;
    }
}

IR_EXPORT int IR_LastErrorMessage(unsigned char *buffer, int capacity)
{
    try
    {
        const std::string msg = ir::last_error();
        if (!buffer || capacity <= 0)
            return (int)msg.size();
        const size_t n = std::min<size_t>(msg.size(), (size_t)capacity - 1);
        std::memcpy(buffer, msg.data(), n);
        buffer[n] = 0;
        return (int)n;
    }
    catch (...)
    {
        return 0;
    }
}

IR_EXPORT void IR_Stop(void *engine)
{
    try
    {
        if (engine)
            static_cast<Engine *>(engine)->stop();
    }
    catch (...)
    {
    }
}

IR_EXPORT int IR_SetCaptureTarget(void *engine, HWND window, unsigned options)
{
    if (!engine)
        return ir::IR_FAIL;
    GUARD(static_cast<Engine *>(engine)->set_target(window, options), ir::IR_FAIL)
}

IR_EXPORT int IR_SetCaptureSource(void *engine, int method, int monitor_index, HWND window, unsigned options)
{
    if (!engine)
        return ir::IR_FAIL;
    GUARD(static_cast<Engine *>(engine)->set_source(method, monitor_index, window, options), ir::IR_FAIL)
}

IR_EXPORT int IR_SetCanvasHint(void *engine, int width, int height)
{
    if (!engine)
        return ir::IR_FAIL;
    GUARD(static_cast<Engine *>(engine)->set_canvas_hint(width, height), ir::IR_FAIL)
}

IR_EXPORT int IR_SetPaused(void *engine, int paused)
{
    if (!engine)
        return ir::IR_FAIL;
    GUARD(static_cast<Engine *>(engine)->set_paused(paused != 0), ir::IR_FAIL)
}

IR_EXPORT int IR_GetWindowCount(){
    GUARD((int)ir::enumerate_windows().size(), 0)}

IR_EXPORT void IR_GetWindowInfo(int index, HWND *window, wchar_t *title, int capacity, int *width, int *height)
{
    try
    {
        if (window)
            *window = nullptr;
        if (title && capacity > 0)
            title[0] = 0;
        auto list = ir::enumerate_windows();
        if (index < 0 || (size_t)index >= list.size())
            return;
        const auto &w = list[(size_t)index];
        if (window)
            *window = w.handle;
        if (width)
            *width = w.w;
        if (height)
            *height = w.h;
        copy_wide(w.title, title, capacity);
    }
    catch (...)
    {
    }
}

IR_EXPORT int IR_BeginSave(void *engine, const wchar_t *out_path, int seconds_back, unsigned long long *job_id)
{
    if (!engine || !out_path || !job_id)
        return ir::IR_FAIL;
    try
    {
        uint64_t id = 0;
        const int rc = static_cast<Engine *>(engine)->begin_save(std::wstring(out_path), seconds_back, &id);
        if (rc == ir::IR_OK)
            *job_id = (unsigned long long)id;
        return rc;
    }
    catch (...)
    {
        ir::clear_save_progress();
        return ir::IR_FAIL;
    }
}

IR_EXPORT int IR_PollSave(void *engine, unsigned long long job_id, int *progress, int *result)
{
    if (!engine)
        return 0;
    GUARD(static_cast<Engine *>(engine)->poll_save((uint64_t)job_id, progress, result), 0)
}

IR_EXPORT int IR_Save(void *engine, const wchar_t *out_path, int seconds_back)
{
    if (!engine || !out_path)
        return ir::IR_FAIL;
    try
    {
        unsigned long long id = 0;
        const int rc = IR_BeginSave(engine, out_path, seconds_back, &id);
        if (rc != ir::IR_OK)
            return rc;
        auto *e = static_cast<Engine *>(engine);
        std::shared_ptr<ir::SaveJob> job = e->retain_job((uint64_t)id);
        if (!job)
            return ir::IR_FAIL;
        // The strong reference is held for the entire wait: eviction may unlink
        // the record but must never destroy the object being awaited.
        job->done.acquire();
        return job->result.load();
    }
    catch (...)
    {
        ir::clear_save_progress();
        return ir::IR_FAIL;
    }
}

IR_EXPORT int IR_SaveProgress() { return ir::current_save_progress(); }

IR_EXPORT void IR_GetStatus(void *engine, int *buffering, int *ring_seconds, long long *encoded, long long *dropped,
                            int *has_error, long long *audio_frames, long long *audio_events, long long *audio_got,
                            long long *heartbeat)
{
    try
    {
        if (!engine)
            return;
        static_assert(sizeof(long long) == sizeof(int64_t), "ABI");
        static_cast<Engine *>(engine)->status(buffering, ring_seconds, reinterpret_cast<int64_t *>(encoded),
                                              reinterpret_cast<int64_t *>(dropped), has_error,
                                              reinterpret_cast<int64_t *>(audio_frames),
                                              reinterpret_cast<int64_t *>(audio_events),
                                              reinterpret_cast<int64_t *>(audio_got), reinterpret_cast<int64_t *>(heartbeat));
    }
    catch (...)
    {
    }
}

IR_EXPORT int IR_GetMonitorCount(){
    GUARD((int)ir::enumerate_monitors().size(), 0)}

IR_EXPORT void IR_GetMonitorInfo(int index, wchar_t *name, int name_cap, int *width, int *height)
{
    try
    {
        auto mons = ir::enumerate_monitors();
        if (mons.empty())
            return;
        const auto &m = mons[(index < 0 || (size_t)index >= mons.size()) ? 0 : (size_t)index];
        if (width)
            *width = m.w;
        if (height)
            *height = m.h;
        std::wstring text = (m.primary ? L"Primary - " : L"") + std::to_wstring(m.w) + L"x" + std::to_wstring(m.h);
        copy_wide(text, name, name_cap);
    }
    catch (...)
    {
    }
}

IR_EXPORT int IR_IsDxgiSupported(){
    GUARD(ir::enumerate_monitors().empty() ? 0 : 1, 0)}

IR_EXPORT int IR_DxgiDuplicationSupported(){
    GUARD(ir::dxgi_duplication_supported() ? 1 : 0, 0)}

IR_EXPORT int IR_ActiveCaptureMethod(void *engine)
{
    if (!engine)
        return 0;
    GUARD(static_cast<Engine *>(engine)->active_capture_method(), 0)
}
IR_EXPORT int IR_LastHresult(void *engine)
{
    if (!engine)
        return 0;
    GUARD(static_cast<Engine *>(engine)->last_hresult(), 0)
}
IR_EXPORT int IR_LastFailStage(void *engine)
{
    if (!engine)
        return 0;
    GUARD(static_cast<Engine *>(engine)->last_fail_stage(), 0)
}
IR_EXPORT int IR_LastFailHr(void *engine)
{
    if (!engine)
        return 0;
    GUARD(static_cast<Engine *>(engine)->last_fail_hr(), 0)
}
IR_EXPORT int IR_LastCaptureHr(void *engine)
{
    if (!engine)
        return 0;
    GUARD(static_cast<Engine *>(engine)->last_capture_hr(), 0)
}
IR_EXPORT int IR_ActiveEncoder(void *engine)
{
    if (!engine)
        return 0;
    GUARD(static_cast<Engine *>(engine)->active_encoder(), 0)
}
IR_EXPORT int IR_HwInitHr(void *engine)
{
    if (!engine)
        return 0;
    GUARD(static_cast<Engine *>(engine)->hw_init_hr(), 0)
}

IR_EXPORT int IR_GetMicDeviceCount(){
    GUARD((int)ir::mic_devices().size(), 0)}

IR_EXPORT void IR_GetMicDeviceName(int index, wchar_t *name, int name_cap)
{
    try
    {
        if (name && name_cap > 0)
            name[0] = 0;
        auto list = ir::mic_devices();
        if (index >= 0 && (size_t)index < list.size())
            copy_wide(list[(size_t)index], name, name_cap);
    }
    catch (...)
    {
    }
}

IR_EXPORT void IR_MicStatus(void *engine, long long *mic_frames, long long *mic_events, long long *mic_got,
                            long long *mic_fail_hr, int *sys_peak, int *mic_peak, wchar_t *active_name, int active_cap)
{
    try
    {
        if (!engine)
            return;
        static_cast<Engine *>(engine)->mic_status(reinterpret_cast<int64_t *>(mic_frames),
                                                  reinterpret_cast<int64_t *>(mic_events),
                                                  reinterpret_cast<int64_t *>(mic_got),
                                                  reinterpret_cast<int64_t *>(mic_fail_hr), sys_peak, mic_peak,
                                                  active_name, active_cap);
    }
    catch (...)
    {
    }
}

IR_EXPORT void IR_AudioDiagnostics(void *engine, int which, long long *activate_hr, long long *init_hr,
                                   long long *start_hr, int *tag, int *rate, int *channels, int *bits,
                                   int *is_float, int *convert_failures, long long *last_fail_hr)
{
    try
    {
        if (!engine)
            return;
        static_cast<Engine *>(engine)->audio_diagnostics(which,
                                                         reinterpret_cast<int64_t *>(activate_hr), reinterpret_cast<int64_t *>(init_hr),
                                                         reinterpret_cast<int64_t *>(start_hr), tag, rate, channels, bits, is_float,
                                                         convert_failures, reinterpret_cast<int64_t *>(last_fail_hr));
    }
    catch (...)
    {
    }
}

IR_EXPORT void IR_GetTelemetry(void *engine, long long *captured, long long *paced, long long *rejected,
                               long long *encoded, long long *dropped, int *encoder, long long *last_pts)
{
    try
    {
        if (!engine)
            return;
        static_cast<Engine *>(engine)->telemetry(
            reinterpret_cast<int64_t *>(captured), reinterpret_cast<int64_t *>(paced),
            reinterpret_cast<int64_t *>(rejected), reinterpret_cast<int64_t *>(encoded),
            reinterpret_cast<int64_t *>(dropped), encoder, reinterpret_cast<int64_t *>(last_pts));
    }
    catch (...)
    {
    }
}

IR_EXPORT void IR_GetTiming(void *engine, double *map_ms, double *encode_ms, double *render_ms,
                            double *submit_ms, double *snapshot_ms, double *unmap_ms, long long *frames)
{
    try
    {
        if (!engine)
            return;
        static_cast<Engine *>(engine)->timing(map_ms, encode_ms, render_ms, submit_ms, snapshot_ms,
                                              unmap_ms, reinterpret_cast<int64_t *>(frames));
    }
    catch (...)
    {
    }
}

IR_EXPORT const char *IR_Version() { return "cpp-obs-1.0"; }

IR_EXPORT unsigned IR_ProbeEncoders()
{
    try
    {
        return ir::probe_all_encoders();
    }
    catch (...)
    {
        return 0;
    }
}
