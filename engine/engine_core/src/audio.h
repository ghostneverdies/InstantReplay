// WASAPI capture: system loopback (whole mix, or one process tree when a window is targeted) and
// microphone. Both deliver 48 kHz signed 16-bit PCM (stereo system, mono mic); Windows resamples.
#pragma once
#include "ir_common.h"

namespace ir {

struct AudioCounters {
    std::atomic<int64_t> events{0};
    std::atomic<int64_t> frames{0};
    std::atomic<int64_t> got_frames{0};
    std::atomic<int64_t> fail_hr{0};
    std::atomic<int> peak{0};
    std::mutex name_mutex;
    std::wstring active_name;
};

using AudioPush = std::function<void(const int16_t* samples, uint32_t frames, int64_t qpc100ns)>;

class AudioSource {
public:
    // `mic` selects microphone vs system loopback. `target_window` (system only): capture just that window's
    // process tree instead of the whole mix. `device_name` (mic only): friendly name, empty = default.
    AudioSource(bool mic, std::wstring device_name, HWND target_window, AudioPush push, AudioCounters* counters);
    ~AudioSource();
    void start();
    void stop();

private:
    void run();
    bool run_once();
    bool mic_;
    std::wstring device_name_;
    HWND target_;
    AudioPush push_;
    AudioCounters* counters_;
    std::atomic<bool> running_{false};
    std::thread thread_;
};

std::vector<std::wstring> mic_devices();

} // namespace ir
