// Screen/window capture: Windows Graphics Capture (raw WinRT ABI, no C++/WinRT) and DXGI desktop
// duplication, plus monitor/window enumeration.
#pragma once
#include "gpu_pipeline.h"

namespace ir {

struct MonInfo {
    HMONITOR handle = nullptr;
    RECT rect = {};
    int w = 0, h = 0;
    bool primary = false;
};
struct WindowInfo {
    HWND handle = nullptr;
    std::wstring title;
    int w = 0, h = 0;
};

std::vector<MonInfo> enumerate_monitors(); // primary first
std::vector<WindowInfo> enumerate_windows(); // sorted by title
bool window_size(HWND window, bool client, int* w, int* h);
// Finds the adapter that owns `monitor` (falls back to the default adapter).
ComPtr<IDXGIAdapter> adapter_for_monitor(HMONITOR monitor);
bool dxgi_duplication_supported();

struct CaptureTarget {
    HWND window = nullptr; // null = whole monitor
    HMONITOR monitor = nullptr;
    uint32_t options = CAP_CURSOR | CAP_KEEP_CORNERS;
    int fps = 60; // configured recording rate; used for frame pacing / duplicate rejection
};

struct CapturedFrame {
    ID3D11Texture2D* texture = nullptr; // valid only during the callback
    D3D11_BOX box = {};                 // region of `texture` to use (already cropped)
    int64_t pts100ns = 0;               // QPC time domain
};

using FrameCallback = std::function<void(const CapturedFrame&)>;

class CaptureSource {
public:
    virtual ~CaptureSource() = default;
    virtual HRESULT start() = 0;
    virtual void stop() = 0;
    HRESULT last_hr() const { return last_hr_.load(); }
    bool closed() const { return closed_.load(); } // the target window went away
    int stage() const { return stage_.load(); }

protected:
    std::atomic<HRESULT> last_hr_{S_OK};
    std::atomic<bool> closed_{false};
    std::atomic<int> stage_{0};
};

std::unique_ptr<CaptureSource> make_wgc_capture(const GpuDevice& gpu, const CaptureTarget& target, FrameCallback cb);
std::unique_ptr<CaptureSource> make_dxgi_capture(const GpuDevice& gpu, const CaptureTarget& target, FrameCallback cb);

} // namespace ir
