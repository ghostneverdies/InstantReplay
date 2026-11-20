// Instant Replay native engine v4 — capture sources.
//
// Whole-screen capture uses DXGI Desktop Duplication; window capture uses raw WinRT ABI
// Windows Graphics Capture.  The two paths are deliberately separate classes; the only
// shared object is the pooled VideoFrame lease handed to the engine's FrameSink.
//
// Capture threads never touch the canvas, snapshot, pipeline, encoder, pending_ queue or
// any other GPU-conversion state.  DXGI composes the mouse pointer with capture-owned
// GPU resources (no CPU readback / Map anywhere in this file).

#include "core.hpp"
#include <dwmapi.h>
#include <future>
#include <psapi.h>
#include <roapi.h>
#include <windows.foundation.h>
#include <windows.graphics.directx.direct3d11.h>
#include <winstring.h>
#include <windows.graphics.capture.h>
#include <windows.graphics.h>
#include <d3dcompiler.h> // types + D3DCompile signature only; loaded dynamically

namespace ir
{

    namespace WF = ABI::Windows::Foundation;
    namespace WGC = ABI::Windows::Graphics::Capture;
    namespace WGD = ABI::Windows::Graphics::DirectX;
    namespace WGD11 = ABI::Windows::Graphics::DirectX::Direct3D11;

    // ------------------------------------------------------------------------------------------------
    // Enumeration
    // ------------------------------------------------------------------------------------------------

    namespace
    {

        BOOL CALLBACK monitor_enum(HMONITOR m, HDC, LPRECT, LPARAM param)
        {
            MONITORINFO mi = {sizeof(mi)};
            if (!GetMonitorInfoW(m, &mi))
                return TRUE;
            MonInfo info;
            info.handle = m;
            info.rect = mi.rcMonitor;
            info.w = mi.rcMonitor.right - mi.rcMonitor.left;
            info.h = mi.rcMonitor.bottom - mi.rcMonitor.top;
            info.primary = (mi.dwFlags & MONITORINFOF_PRIMARY) != 0;
            reinterpret_cast<std::vector<MonInfo> *>(param)->push_back(info);
            return TRUE;
        }

        std::wstring lower(std::wstring s)
        {
            for (auto &c : s)
                c = (wchar_t)towlower(c);
            return s;
        }

        std::wstring process_base_name(DWORD pid)
        {
            HANDLE p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
            if (!p)
                return L"";
            wchar_t path[MAX_PATH * 4];
            DWORD len = (DWORD)(sizeof(path) / sizeof(path[0]));
            std::wstring name;
            if (QueryFullProcessImageNameW(p, 0, path, &len))
            {
                std::wstring full(path, len);
                size_t slash = full.find_last_of(L"\\/");
                name = lower(slash == std::wstring::npos ? full : full.substr(slash + 1));
            }
            CloseHandle(p);
            return name;
        }

        bool excluded_process(const std::wstring &n)
        {
            static const wchar_t *const list[] = {L"explorer.exe", L"shellexperiencehost.exe", L"startmenuexperiencehost.exe",
                                                   L"searchhost.exe", L"searchapp.exe", L"textinputhost.exe",
                                                   L"applicationframehost.exe", L"lockapp.exe", L"dwm.exe"};
            if (n.empty())
                return true;
            for (auto *e : list)
                if (n == e)
                    return true;
            return false;
        }

        BOOL CALLBACK window_enum(HWND w, LPARAM param)
        {
            if (!IsWindowVisible(w) || GetAncestor(w, GA_ROOTOWNER) != w)
                return TRUE;
            if (GetWindowLongPtrW(w, GWL_EXSTYLE) & WS_EX_TOOLWINDOW)
                return TRUE;
            DWORD cloaked = 0;
            DwmGetWindowAttribute(w, DWMWA_CLOAKED, &cloaked, sizeof(cloaked));
            if (cloaked)
                return TRUE;
            wchar_t cls[256];
            int cl = GetClassNameW(w, cls, 256);
            static const wchar_t *const skip_classes[] = {L"Progman", L"WorkerW", L"Shell_TrayWnd", L"Shell_SecondaryTrayWnd",
                                                           L"Windows.UI.Core.CoreWindow", L"CabinetWClass", L"ExploreWClass"};
            for (auto *c : skip_classes)
                if (cl > 0 && std::wstring(cls, cl) == c)
                    return TRUE;
            DWORD pid = 0;
            GetWindowThreadProcessId(w, &pid);
            if (pid == GetCurrentProcessId() || excluded_process(process_base_name(pid)))
                return TRUE;
            wchar_t title[512];
            int tl = GetWindowTextW(w, title, 512);
            if (tl <= 0)
                return TRUE;
            std::wstring t(title, tl);
            if (t == L"Program Manager" || t == L"Windows Input Experience" || t == L"Microsoft Text Input Application")
                return TRUE;
            int ww, hh;
            if (!window_size(w, false, &ww, &hh))
                return TRUE;
            WindowInfo info;
            info.handle = w;
            info.title = t;
            info.w = ww;
            info.h = hh;
            reinterpret_cast<std::vector<WindowInfo> *>(param)->push_back(std::move(info));
            return TRUE;
        }

        // The foreground "active" window used by CAP_FOLLOW.
        HWND active_capture_window()
        {
            auto windows = enumerate_windows();
            if (windows.empty())
                return nullptr;
            HWND foreground = GetAncestor(GetForegroundWindow(), GA_ROOT);
            for (const auto &w : windows)
                if (w.handle == foreground)
                    return w.handle;
            return nullptr;
        }

        // Mirrors the Rust FramePacer: 2 ms tolerance; fps == 0 disables rate limiting but
        // still rejects non-advancing / duplicate timestamps.
        struct FramePacer
        {
            int64_t tolerance = 20'000;
            int64_t last_seen = 0;
            int64_t next = 0;

            explicit FramePacer(int64_t t = 20'000) : tolerance(std::max<int64_t>(0, t)) {}

            bool would_accept(int64_t time, uint32_t fps) const
            {
                if (time <= 0 || time <= last_seen)
                    return false;
                if (fps == 0)
                    return true;
                return time + slack(fps) >= next;
            }

            bool accept(int64_t time, uint32_t fps)
            {
                if (!would_accept(time, fps))
                    return false;
                last_seen = time;
                if (fps == 0)
                    return true;
                if (next == 0)
                    next = time;
                const int64_t period = SECOND_100NS / (int64_t)fps;
                if (tolerance > 0 && time - next >= period)
                    next = time + period;
                else
                {
                    const int64_t late = std::max<int64_t>(0, time - next);
                    next += (late / period + 1) * period;
                }
                return true;
            }

        private:
            int64_t slack(uint32_t fps) const { return std::min<int64_t>(tolerance, 1'250'000 / (int64_t)fps); }
        };

        struct RoGuard
        {
            bool active = false;
            ~RoGuard()
            {
                if (active)
                    RoUninitialize();
            }
        };

    } // namespace

    std::vector<MonInfo> enumerate_monitors()
    {
        std::vector<MonInfo> list;
        EnumDisplayMonitors(nullptr, nullptr, monitor_enum, reinterpret_cast<LPARAM>(&list));
        std::stable_sort(list.begin(), list.end(), [](const MonInfo &a, const MonInfo &b)
                         { return a.primary && !b.primary; });
        return list;
    }

    std::vector<WindowInfo> enumerate_windows()
    {
        std::vector<WindowInfo> list;
        EnumWindows(window_enum, reinterpret_cast<LPARAM>(&list));
        std::sort(list.begin(), list.end(), [](const WindowInfo &a, const WindowInfo &b)
                  { return lower(a.title) < lower(b.title); });
        return list;
    }

    bool window_size(HWND window, bool client, int *w, int *h)
    {
        if (!IsWindow(window) || IsIconic(window))
            return false;
        RECT r = {};
        if (client)
        {
            if (!GetClientRect(window, &r))
                return false;
        }
        else if (FAILED(DwmGetWindowAttribute(window, DWMWA_EXTENDED_FRAME_BOUNDS, &r, sizeof(r))))
        {
            return false;
        }
        int ww = r.right - r.left, hh = r.bottom - r.top;
        if (ww < 2 || hh < 2)
            return false;
        *w = ww;
        *h = hh;
        return true;
    }

    ComPtr<IDXGIAdapter> adapter_for_monitor(HMONITOR monitor)
    {
        ComPtr<IDXGIFactory1> factory;
        if (FAILED(CreateDXGIFactory1(IID_PPV_ARGS(&factory))))
            return nullptr;
        for (UINT a = 0;; ++a)
        {
            ComPtr<IDXGIAdapter1> adapter;
            if (factory->EnumAdapters1(a, &adapter) == DXGI_ERROR_NOT_FOUND)
                break;
            for (UINT o = 0;; ++o)
            {
                ComPtr<IDXGIOutput> output;
                if (adapter->EnumOutputs(o, &output) == DXGI_ERROR_NOT_FOUND)
                    break;
                DXGI_OUTPUT_DESC desc;
                if (SUCCEEDED(output->GetDesc(&desc)) && desc.Monitor == monitor)
                {
                    ComPtr<IDXGIAdapter> base;
                    if (FAILED(adapter.As(&base)))
                        return nullptr;
                    return base;
                }
            }
        }
        return nullptr;
    }

    bool dxgi_duplication_supported()
    {
        auto mons = enumerate_monitors();
        if (mons.empty())
            return false;
        ComPtr<IDXGIAdapter> adapter = adapter_for_monitor(mons[0].handle);
        GpuContext gpu;
        if (FAILED(create_gpu_device(adapter.Get(), &gpu)))
            return false;
        ComPtr<IDXGIDevice> dxgi;
        if (FAILED(gpu.dev.As(&dxgi)))
            return false;
        ComPtr<IDXGIAdapter> a;
        if (FAILED(dxgi->GetAdapter(&a)))
            return false;
        for (UINT o = 0;; ++o)
        {
            ComPtr<IDXGIOutput> out;
            if (a->EnumOutputs(o, &out) == DXGI_ERROR_NOT_FOUND)
                break;
            ComPtr<IDXGIOutput1> out1;
            ComPtr<IDXGIOutputDuplication> dup;
            if (SUCCEEDED(out.As(&out1)) && SUCCEEDED(out1->DuplicateOutput(gpu.dev.Get(), &dup)))
                return true;
        }
        return false;
    }

    // ------------------------------------------------------------------------------------------------
    // Windows Graphics Capture (raw ABI) — window path
    // ------------------------------------------------------------------------------------------------

    namespace
    {

        const GUID kIidItemInterop = {0x3628E81B, 0x3CAC, 0x4C60, {0xB7, 0xF4, 0x23, 0xCE, 0x0E, 0x0C, 0x33, 0x56}};
        const GUID kIidDxgiAccess = {0xA9B3D012, 0x3DF2, 0x4EE3, {0xB8, 0xD1, 0x86, 0x95, 0xF4, 0x57, 0xD3, 0xC1}};
        const GUID kIidSession2 = {0x2C39AE40, 0x7D2E, 0x5044, {0x80, 0x4E, 0x8B, 0x67, 0x99, 0xD4, 0xCF, 0x9E}};
        const GUID kIidSession3 = {0xF2CDD966, 0x22AE, 0x5EA1, {0x95, 0x96, 0x3A, 0x28, 0x93, 0x44, 0xC3, 0xBE}};
        const GUID kIidSession5 = {0x67C0EA62, 0x1F85, 0x5061, {0x92, 0x5A, 0x23, 0x9B, 0xE0, 0xAC, 0x09, 0xCB}};
        const GUID kIidSession6 = {0xD7419236, 0xBE20, 0x5E9F, {0xBC, 0xD6, 0xC4, 0xE9, 0x8F, 0xD6, 0xAF, 0xDC}};

        struct IrItemInterop : public IUnknown
        {
            virtual HRESULT STDMETHODCALLTYPE CreateForWindow(HWND window, REFIID riid, void **result) = 0;
            virtual HRESULT STDMETHODCALLTYPE CreateForMonitor(HMONITOR monitor, REFIID riid, void **result) = 0;
        };
        struct IrDxgiAccess : public IUnknown
        {
            virtual HRESULT STDMETHODCALLTYPE GetInterface(REFIID riid, void **object) = 0;
        };
        struct IrSession2 : public IInspectable
        {
            virtual HRESULT STDMETHODCALLTYPE get_IsCursorCaptureEnabled(boolean *value) = 0;
            virtual HRESULT STDMETHODCALLTYPE put_IsCursorCaptureEnabled(boolean value) = 0;
        };
        struct IrSession3 : public IInspectable
        {
            virtual HRESULT STDMETHODCALLTYPE get_IsBorderRequired(boolean *value) = 0;
            virtual HRESULT STDMETHODCALLTYPE put_IsBorderRequired(boolean value) = 0;
        };
        struct IrSession5 : public IInspectable
        {
            virtual HRESULT STDMETHODCALLTYPE get_MinUpdateInterval(WF::TimeSpan *value) = 0;
            virtual HRESULT STDMETHODCALLTYPE put_MinUpdateInterval(WF::TimeSpan value) = 0;
        };
        struct IrSession6 : public IInspectable
        {
            virtual HRESULT STDMETHODCALLTYPE get_IncludeSecondaryWindows(boolean *value) = 0;
            virtual HRESULT STDMETHODCALLTYPE put_IncludeSecondaryWindows(boolean value) = 0;
        };

        struct HStr
        {
            HSTRING h = nullptr;
            explicit HStr(const wchar_t *s) { WindowsCreateString(s, (UINT32)wcslen(s), &h); }
            ~HStr()
            {
                if (h)
                    WindowsDeleteString(h);
            }
        };

        template <class T>
        HRESULT activation_factory(const wchar_t *name, REFIID iid, T **out)
        {
            HStr s(name);
            return RoGetActivationFactory(s.h, iid, reinterpret_cast<void **>(out));
        }

        void close_winrt(IUnknown *obj)
        {
            if (!obj)
                return;
            ComPtr<WF::IClosable> c;
            if (SUCCEEDED(obj->QueryInterface(IID_PPV_ARGS(&c))))
                c->Close();
        }

        // Plain-function destroy callback stored in a pooled Keepalive.  The context is an
        // owned IUnknown reference to the WinRT frame; closing it and releasing the ref is
        // the entire close action.  Safe to call only once (capture.cpp clears the callback
        // immediately after invoking it).
        void close_wgc_keepalive(void *ctx) noexcept
        {
            auto *unk = static_cast<IUnknown *>(ctx);
            if (!unk)
                return;
            close_winrt(unk);
            unk->Release();
        }

        class ClosedHandler final : public WF::ITypedEventHandler<WGC::GraphicsCaptureItem *, IInspectable *>
        {
        public:
            explicit ClosedHandler(std::atomic<bool> *flag) : flag_(flag) {}

            HRESULT STDMETHODCALLTYPE QueryInterface(REFIID riid, void **ppv) override
            {
                if (!ppv)
                    return E_POINTER;
                *ppv = nullptr;
                if (riid == IID_IUnknown || riid == HandlerIid())
                {
                    *ppv = static_cast<ClosedHandler *>(this);
                    AddRef();
                    return S_OK;
                }
                return E_NOINTERFACE;
            }
            ULONG STDMETHODCALLTYPE AddRef() override { return (ULONG)InterlockedIncrement(&refs_); }
            ULONG STDMETHODCALLTYPE Release() override
            {
                ULONG n = (ULONG)InterlockedDecrement(&refs_);
                if (!n)
                    delete this;
                return n;
            }
            HRESULT STDMETHODCALLTYPE Invoke(WGC::IGraphicsCaptureItem *, IInspectable *) override
            {
                flag_->store(true);
                return S_OK;
            }

        private:
            static REFIID HandlerIid() { return __uuidof(WF::ITypedEventHandler<WGC::GraphicsCaptureItem *, IInspectable *>); }
            volatile long refs_ = 1;
            std::atomic<bool> *flag_;
        };

        class WgcCapture final : public CaptureSource
        {
        public:
            WgcCapture(const GpuContext &gpu, const CaptureTarget &t, ObjectPool<VideoFrame> *frames,
                       ObjectPool<Keepalive> *keepalives, FrameSink sink)
                : CaptureSource(std::move(sink)), gpu_(gpu), target_(t), frames_(frames), keepalives_(keepalives)
            {
            }
            ~WgcCapture() override { stop(); }

            HRESULT start() override
            {
                stage_ = STAGE_CREATE_ITEM;
                std::promise<HRESULT> ready;
                auto signal = ready.get_future();
                running_ = true;
                // WinRT state (including RoInitialize) is created on the worker thread only.
                thread_ = std::thread([this, p = std::move(ready)]() mutable
                                      { worker(std::move(p)); });
                HRESULT hr = signal.get();
                if (FAILED(hr))
                {
                    running_ = false;
                    if (thread_.joinable())
                        thread_.join();
                    cleanup();
                    return fail(hr);
                }
                return S_OK;
            }

            void stop() override
            {
                running_ = false;
                if (thread_.joinable())
                    thread_.join();
                for (auto &r : outstanding_)
                    r.reset();
                cleanup();
            }

        private:
            HRESULT fail(HRESULT hr)
            {
                last_hr_ = hr;
                return hr;
            }

            enum class DeliverResult
            {
                None,
                Recreate
            };

            void worker(std::promise<HRESULT> ready)
            {
                RoGuard ro;
                HRESULT hr = RoInitialize(RO_INIT_MULTITHREADED);
                if (hr == RPC_E_CHANGED_MODE)
                {
                    ready.set_value(fail(hr));
                    return;
                }
                if (FAILED(hr))
                {
                    ready.set_value(fail(hr));
                    return;
                }
                ro.active = true;
                SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

                bool signalled = false;
                try
                {
                    while (running_)
                    {
                        closed_ = false;
                        pacer_ = FramePacer();
                        HRESULT setup = init();
                        if (FAILED(setup))
                        {
                            if (!signalled)
                                ready.set_value(setup);
                            else
                            {
                                last_hr_ = setup;
                                closed_ = true;
                            }
                            break;
                        }
                        if (!signalled)
                        {
                            ready.set_value(S_OK);
                            signalled = true;
                        }
                        run();
                        teardown();
                        if (!running_)
                            break;
                        if (closed_ && !(target_.options & CAP_FOLLOW))
                            break;
                        if (follow_)
                        {
                            follow_ = false;
                            continue;
                        }
                        break;
                    }
                }
                catch (...)
                {
                    if (!signalled)
                        ready.set_value(fail(E_FAIL));
                    signalled = true;
                }
                if (!signalled)
                    ready.set_value(fail(running_ ? E_FAIL : E_ABORT));
            }

            HRESULT init()
            {
                stage_ = STAGE_CREATE_ITEM;
                HRESULT hr;
                ComPtr<IrItemInterop> interop;
                hr = activation_factory(L"Windows.Graphics.Capture.GraphicsCaptureItem", kIidItemInterop, interop.GetAddressOf());
                if (FAILED(hr))
                    return fail(hr);
                if (target_.window)
                    hr = interop->CreateForWindow(target_.window, IID_PPV_ARGS(&item_));
                else
                    hr = interop->CreateForMonitor(target_.monitor, IID_PPV_ARGS(&item_));
                if (FAILED(hr))
                    return fail(hr);

                stage_ = STAGE_FRAME_POOL;
                ComPtr<IDXGIDevice> dxgi;
                hr = gpu_.dev.As(&dxgi);
                if (FAILED(hr))
                    return fail(hr);
                if (!d3d11_)
                {
                    d3d11_ = LoadLibraryW(L"d3d11.dll");
                    if (!d3d11_)
                        return fail(HRESULT_FROM_WIN32(GetLastError()));
                }
                using CreateFn = HRESULT(WINAPI *)(IDXGIDevice *, IInspectable **);
                auto create = reinterpret_cast<CreateFn>(GetProcAddress(d3d11_, "CreateDirect3D11DeviceFromDXGIDevice"));
                if (!create)
                    return fail(E_NOTIMPL);
                ComPtr<IInspectable> insp;
                hr = create(dxgi.Get(), &insp);
                if (FAILED(hr))
                    return fail(hr);
                hr = insp.As(&winrt_device_);
                if (FAILED(hr))
                    return fail(hr);

                ABI::Windows::Graphics::SizeInt32 size = {};
                hr = item_->get_Size(&size);
                if (FAILED(hr))
                    return fail(hr);
                if (size.Width < 2 || size.Height < 2)
                    return fail(E_INVALIDARG);
                size_ = size;
                hr = activation_factory(L"Windows.Graphics.Capture.Direct3D11CaptureFramePool",
                                        __uuidof(WGC::IDirect3D11CaptureFramePoolStatics), statics_.GetAddressOf());
                if (FAILED(hr))
                    return fail(hr);
                ComPtr<WGC::IDirect3D11CaptureFramePoolStatics2> statics2;
                if (SUCCEEDED(statics_.As(&statics2)))
                    hr = statics2->CreateFreeThreaded(winrt_device_.Get(), WGD::DirectXPixelFormat_B8G8R8A8UIntNormalized,
                                                      (int)kRawWgc, size, &pool_);
                else
                    hr = statics_->Create(winrt_device_.Get(), WGD::DirectXPixelFormat_B8G8R8A8UIntNormalized,
                                          (int)kRawWgc, size, &pool_);
                if (FAILED(hr))
                    return fail(hr);

                stage_ = STAGE_START_CAPTURE;
                hr = pool_->CreateCaptureSession(item_.Get(), &session_);
                if (FAILED(hr))
                    return fail(hr);
                ComPtr<IrSession2> s2;
                if (SUCCEEDED(session_->QueryInterface(kIidSession2, reinterpret_cast<void **>(s2.GetAddressOf()))))
                    s2->put_IsCursorCaptureEnabled((target_.options & CAP_CURSOR) ? 1 : 0);
                ComPtr<IrSession3> s3;
                if (SUCCEEDED(session_->QueryInterface(kIidSession3, reinterpret_cast<void **>(s3.GetAddressOf()))))
                    s3->put_IsBorderRequired((target_.options & CAP_BORDER) ? 1 : 0);

                ComPtr<IrSession5> s5;
                if (SUCCEEDED(session_->QueryInterface(kIidSession5, reinterpret_cast<void **>(s5.GetAddressOf()))))
                {
                    const int64_t fps = std::max<int64_t>(1, target_.fps);
                    WF::TimeSpan interval = {};
                    // Feed WGC at twice the target rate; FramePacer remains the FPS limiter.
                    interval.Duration = std::clamp<int64_t>(SECOND_100NS / (int64_t)fps / 2, 10'000, 1'000'000);
                    s5->put_MinUpdateInterval(interval);
                }
                if (target_.options & CAP_SECONDARY)
                {
                    ComPtr<IrSession6> s6;
                    if (SUCCEEDED(session_->QueryInterface(kIidSession6, reinterpret_cast<void **>(s6.GetAddressOf()))))
                        s6->put_IncludeSecondaryWindows(1);
                }

                auto *handler = new ClosedHandler(&closed_);
                closed_handler_ = handler;
                handler->Release();
                hr = item_->add_Closed(closed_handler_.Get(), &closed_token_);
                if (FAILED(hr))
                    return fail(hr);

                hr = session_->StartCapture();
                if (FAILED(hr))
                    return fail(hr);
                return S_OK;
            }

            void teardown()
            {
                if (item_ && closed_token_.value)
                    item_->remove_Closed(closed_token_);
                closed_token_ = {};
                closed_handler_.Reset();
                close_winrt(session_.Get());
                close_winrt(pool_.Get());
                session_.Reset();
                pool_.Reset();
                statics_.Reset();
                item_.Reset();
                winrt_device_.Reset();
            }

            void cleanup()
            {
                teardown();
                if (d3d11_)
                {
                    FreeLibrary(d3d11_);
                    d3d11_ = nullptr;
                }
            }

            void run()
            {
                HANDLE timer = CreateWaitableTimerExW(nullptr, nullptr, CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, TIMER_ALL_ACCESS);
                int64_t last_alive_check = 0;
                int64_t last_follow_check = 0;
                while (running_ && !closed_)
                {
                    ComPtr<WGC::IDirect3D11CaptureFrame> frame;
                    HRESULT hr = pool_->TryGetNextFrame(&frame);
                    if (SUCCEEDED(hr) && frame)
                    {
                        ABI::Windows::Graphics::SizeInt32 wanted = {};
                        DeliverResult result = deliver(frame.Get(), &wanted);
                        frame.Reset();
                        if (result == DeliverResult::Recreate)
                        {
                            hr = pool_->Recreate(winrt_device_.Get(), WGD::DirectXPixelFormat_B8G8R8A8UIntNormalized,
                                                 (int)kRawWgc, wanted);
                            if (SUCCEEDED(hr))
                                size_ = wanted;
                            else
                                last_hr_ = hr;
                        }
                        continue;
                    }
                    int64_t now = qpc100ns();
                    if (target_.window && now - last_alive_check > SECOND_100NS / 4)
                    {
                        last_alive_check = now;
                        if (!IsWindow(target_.window))
                        {
                            closed_ = true;
                            break;
                        }
                    }
                    if ((target_.options & CAP_FOLLOW) && now - last_follow_check > SECOND_100NS / 5)
                    {
                        last_follow_check = now;
                        HWND active = active_capture_window();
                        if (active && active != target_.window)
                        {
                            target_.window = active;
                            follow_ = true;
                            break;
                        }
                    }
                    if (timer)
                    {
                        LARGE_INTEGER due;
                        due.QuadPart = -10'000;
                        SetWaitableTimer(timer, &due, 0, nullptr, nullptr, FALSE);
                        WaitForSingleObject(timer, 5);
                    }
                    else
                    {
                        Sleep(1);
                    }
                }
                if (timer)
                    CloseHandle(timer);
            }

            void prune_outstanding() noexcept
            {
                for (auto &r : outstanding_)
                {
                    // One ref is ours; anything more means the video lane still holds the frame.
                    if (r && r->use_count() <= 1)
                        r.reset();
                }
            }

            size_t outstanding_count() const noexcept
            {
                size_t n = 0;
                for (const auto &r : outstanding_)
                    if (r)
                        ++n;
                return n;
            }

            DeliverResult deliver(WGC::IDirect3D11CaptureFrame *frame, ABI::Windows::Graphics::SizeInt32 *wanted)
            {
                DeliverResult result = DeliverResult::None;
                ABI::Windows::Graphics::SizeInt32 content = {};
                if (FAILED(frame->get_ContentSize(&content)))
                    return result;
                if (content.Width < 2 || content.Height < 2)
                    return result;
                if (content.Width != size_.Width || content.Height != size_.Height)
                {
                    *wanted = content;
                    result = DeliverResult::Recreate;
                }

                WF::TimeSpan rel = {};
                HRESULT hr = frame->get_SystemRelativeTime(&rel);
                const int64_t stamp = SUCCEEDED(hr) && rel.Duration > 0 ? rel.Duration : qpc100ns();
                if (!pacer_.accept(stamp, (uint32_t)std::max(0, target_.fps)))
                {
                    ++rejected_;
                    return result;
                }
                ++paced_;

                ComPtr<WGD11::IDirect3DSurface> surface;
                if (FAILED(frame->get_Surface(&surface)))
                    return result;
                ComPtr<IrDxgiAccess> access;
                if (FAILED(surface->QueryInterface(kIidDxgiAccess, reinterpret_cast<void **>(access.GetAddressOf()))))
                    return result;
                ComPtr<ID3D11Texture2D> tex;
                if (FAILED(access->GetInterface(IID_PPV_ARGS(&tex))))
                    return result;

                D3D11_TEXTURE2D_DESC desc;
                tex->GetDesc(&desc);
                if (desc.Width < 2 || desc.Height < 2)
                    return result;
                D3D11_BOX box = {0, 0, 0, std::min<UINT>(desc.Width, (UINT)content.Width),
                                 std::min<UINT>(desc.Height, (UINT)content.Height), 1};
                if (target_.window && (target_.options & CAP_CLIENT_AREA))
                    crop_to_client(box);
                if (box.right - box.left < 2 || box.bottom - box.top < 2)
                    return result;

                prune_outstanding();
                if (outstanding_count() >= kRawWgc || !frames_ || !keepalives_ || !sink_)
                {
                    ++rejected_;
                    return result;
                }
                Ref<VideoFrame> vf = frames_->acquire();
                if (!vf)
                {
                    ++rejected_;
                    return result;
                }
                Ref<Keepalive> ka = keepalives_->acquire();
                if (!ka)
                {
                    ++rejected_;
                    return result;
                }

                // Store the WinRT close action in a pooled Keepalive with a plain function
                // pointer callback, then invoke it here: the D3D11 texture above is already
                // owned by the VideoFrame, so the WinRT frame need not stay open.
                IUnknown *owned = static_cast<IUnknown *>(frame);
                owned->AddRef();
                ka->set(&close_wgc_keepalive, owned);
                ka->destroy(ka->context);
                ka->destroy = nullptr;
                ka->context = nullptr;

                vf->texture = tex;
                vf->box = box;
                vf->pts100ns = stamp;
                vf->keepalive = ka;

                for (auto &slot : outstanding_)
                {
                    if (!slot)
                    {
                        slot = ka;
                        break;
                    }
                }
                emit(std::move(vf));
                return result;
            }

            void crop_to_client(D3D11_BOX &box)
            {
                RECT frame = {};
                if (FAILED(DwmGetWindowAttribute(target_.window, DWMWA_EXTENDED_FRAME_BOUNDS, &frame, sizeof(frame))))
                    return;
                RECT client = {};
                if (!GetClientRect(target_.window, &client))
                    return;
                POINT origin = {0, 0};
                ClientToScreen(target_.window, &origin);
                LONG left = std::max<LONG>(0, origin.x - frame.left), top = std::max<LONG>(0, origin.y - frame.top);
                LONG right = std::min<LONG>((LONG)box.right, left + (client.right - client.left));
                LONG bottom = std::min<LONG>((LONG)box.bottom, top + (client.bottom - client.top));
                if (right - left >= 2 && bottom - top >= 2)
                {
                    box.left = left;
                    box.top = top;
                    box.right = right;
                    box.bottom = bottom;
                }
            }

            GpuContext gpu_;
            CaptureTarget target_;
            ObjectPool<VideoFrame> *frames_ = nullptr;
            ObjectPool<Keepalive> *keepalives_ = nullptr;
            Ref<Keepalive> outstanding_[kRawWgc]{};
            HMODULE d3d11_ = nullptr;
            ComPtr<WGC::IGraphicsCaptureItem> item_;
            ComPtr<WGD11::IDirect3DDevice> winrt_device_;
            ComPtr<WGC::IDirect3D11CaptureFramePoolStatics> statics_;
            ComPtr<WGC::IDirect3D11CaptureFramePool> pool_;
            ComPtr<WGC::IGraphicsCaptureSession> session_;
            ComPtr<WF::ITypedEventHandler<WGC::GraphicsCaptureItem *, IInspectable *>> closed_handler_;
            EventRegistrationToken closed_token_ = {};
            ABI::Windows::Graphics::SizeInt32 size_ = {};
            FramePacer pacer_;
            std::atomic<bool> running_{false};
            std::atomic<bool> follow_{false};
            std::thread thread_;
        };

        // ------------------------------------------------------------------------------------------------
        // DXGI desktop duplication — whole-screen path
        // ------------------------------------------------------------------------------------------------

        // GPU-only cursor compositor.  The captured desktop is never read back to the CPU;
        // a tiny sprite texture is uploaded with UpdateSubresource and alpha-blended into
        // the published desktop texture with a fullscreen triangle.  d3dcompiler.dll is
        // loaded dynamically so the engine DLL keeps its current link dependencies.
        class CursorCompositor
        {
        public:
            ~CursorCompositor() = default;

            HRESULT init(ID3D11Device *dev)
            {
                if (ok_)
                    return S_OK;
                ComPtr<ID3DBlob> vs, ps, err;
                HRESULT hr = compile(vs.GetAddressOf(), ps.GetAddressOf(), err.GetAddressOf());
                if (FAILED(hr))
                    return hr;
                hr = dev->CreateVertexShader(vs->GetBufferPointer(), vs->GetBufferSize(), nullptr, vs_.GetAddressOf());
                if (FAILED(hr))
                    return hr;
                hr = dev->CreatePixelShader(ps->GetBufferPointer(), ps->GetBufferSize(), nullptr, ps_.GetAddressOf());
                if (FAILED(hr))
                    return hr;

                D3D11_BLEND_DESC bd = {};
                bd.RenderTarget[0].BlendEnable = TRUE;
                bd.RenderTarget[0].SrcBlend = D3D11_BLEND_SRC_ALPHA;
                bd.RenderTarget[0].DestBlend = D3D11_BLEND_INV_SRC_ALPHA;
                bd.RenderTarget[0].BlendOp = D3D11_BLEND_OP_ADD;
                bd.RenderTarget[0].SrcBlendAlpha = D3D11_BLEND_ONE;
                bd.RenderTarget[0].DestBlendAlpha = D3D11_BLEND_INV_SRC_ALPHA;
                bd.RenderTarget[0].BlendOpAlpha = D3D11_BLEND_OP_ADD;
                bd.RenderTarget[0].RenderTargetWriteMask = D3D11_COLOR_WRITE_ENABLE_ALL;
                hr = dev->CreateBlendState(&bd, blend_.GetAddressOf());
                if (FAILED(hr))
                    return hr;

                D3D11_SAMPLER_DESC sd = {};
                sd.Filter = D3D11_FILTER_MIN_MAG_MIP_POINT;
                sd.AddressU = D3D11_TEXTURE_ADDRESS_CLAMP;
                sd.AddressV = D3D11_TEXTURE_ADDRESS_CLAMP;
                sd.AddressW = D3D11_TEXTURE_ADDRESS_CLAMP;
                sd.MaxLOD = 16.0f;
                hr = dev->CreateSamplerState(&sd, sampler_.GetAddressOf());
                if (FAILED(hr))
                    return hr;
                ok_ = true;
                return S_OK;
            }

            bool ok() const noexcept { return ok_; }
            ID3D11VertexShader *vs() const noexcept { return vs_.Get(); }
            ID3D11PixelShader *ps() const noexcept { return ps_.Get(); }
            ID3D11BlendState *blend() const noexcept { return blend_.Get(); }
            ID3D11SamplerState *sampler() const noexcept { return sampler_.Get(); }

        private:
            static HRESULT compile(ID3DBlob **vs, ID3DBlob **ps, ID3DBlob **err)
            {
                static const char source[] = R"HLSL(
struct VSOut { float4 pos : SV_POSITION; float2 uv : TEXCOORD0; };
VSOut vs(uint id : SV_VertexID)
{
    VSOut o;
    float2 p = float2(float((id << 1) & 2), float(id & 2));
    o.pos = float4(p * 2.0 - 1.0, 0.0, 1.0);
    o.uv = float2(p.x, 1.0f - p.y);
    return o;
}
Texture2D sprite : register(t0);
SamplerState samp : register(s0);
float4 ps(VSOut i) : SV_TARGET
{
    return sprite.Sample(samp, i.uv);
}
)HLSL";
                HMODULE mod = LoadLibraryW(L"d3dcompiler.dll");
                if (!mod)
                    return HRESULT_FROM_WIN32(GetLastError());
                using CompileFn = HRESULT(WINAPI *)(LPCVOID, SIZE_T, LPCSTR, const D3D_SHADER_MACRO *, ID3DInclude *,
                                                    LPCSTR, LPCSTR, UINT, UINT, ID3DBlob **, ID3DBlob **);
                auto fn = reinterpret_cast<CompileFn>(GetProcAddress(mod, "D3DCompile"));
                if (!fn)
                {
                    FreeLibrary(mod);
                    return E_NOTIMPL;
                }
                const SIZE_T len = sizeof(source) - 1;
                HRESULT hr = fn(source, len, nullptr, nullptr, nullptr, "vs", "vs_5_0", 0, 0, vs, err);
                if (SUCCEEDED(hr))
                    hr = fn(source, len, nullptr, nullptr, nullptr, "ps", "ps_5_0", 0, 0, ps, err);
                FreeLibrary(mod);
                return hr;
            }

            ComPtr<ID3D11VertexShader> vs_;
            ComPtr<ID3D11PixelShader> ps_;
            ComPtr<ID3D11BlendState> blend_;
            ComPtr<ID3D11SamplerState> sampler_;
            bool ok_ = false;
        };

        class DxgiPointer
        {
        public:
            void update(IDXGIOutputDuplication *dup, const DXGI_OUTDUPL_FRAME_INFO &info, int origin_x, int origin_y)
            {
                const bool fresh = info.LastMouseUpdateTime.QuadPart != 0;
                if (info.PointerPosition.Visible)
                {
                    at_.x = info.PointerPosition.Position.x - origin_x;
                    at_.y = info.PointerPosition.Position.y - origin_y;
                    visible_ = true;
                }
                else if (fresh)
                {
                    visible_ = false;
                }

                const UINT offered = info.PointerShapeBufferSize;
                if (!offered && have_shape_)
                    return;
                if (buf_.size() < std::max<UINT>(offered, 4096))
                    buf_.resize(std::max<UINT>(offered, 4096));
                UINT needed = 0;
                DXGI_OUTDUPL_POINTER_SHAPE_INFO shape = {};
                HRESULT hr = dup->GetFramePointerShape((UINT)buf_.size(), buf_.data(), &needed, &shape);
                if (FAILED(hr) && needed > buf_.size())
                {
                    buf_.resize(needed);
                    hr = dup->GetFramePointerShape((UINT)buf_.size(), buf_.data(), &needed, &shape);
                }
                if (SUCCEEDED(hr) && needed >= 4)
                {
                    len_ = needed;
                    shape_ = shape;
                    have_shape_ = true;
                    failed_ = false;
                }
                else
                {
                    failed_ = !have_shape_;
                }
            }

            void draw(ID3D11Device *dev, ID3D11DeviceContext *ctx, ID3D11Texture2D *desk,
                      ID3D11RenderTargetView *desk_rtv, CursorCompositor &comp)
            {
                if (!visible_ || !have_shape_ || failed_ || !comp.ok() || !desk_rtv)
                    return;
                D3D11_TEXTURE2D_DESC td = {};
                desk->GetDesc(&td);
                if (td.Width < 2 || td.Height < 2 || shape_.Width == 0 || shape_.Pitch == 0)
                    return;
                const int32_t sh = shape_.Type == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME ? shape_.Height / 2 : shape_.Height;
                if (sh <= 0)
                    return;
                const int x0 = std::max<int>(0, at_.x), y0 = std::max<int>(0, at_.y);
                const int x1 = std::min<int>(at_.x + (int)shape_.Width, (int)td.Width);
                const int y1 = std::min<int>(at_.y + sh, (int)td.Height);
                if (x1 <= x0 || y1 <= y0)
                    return;
                const UINT rw = (UINT)(x1 - x0), rh = (UINT)(y1 - y0);

                std::vector<uint32_t> pixels((size_t)rw * rh, 0u);
                build_sprite(pixels, rw, rh, at_.x - x0, at_.y - y0);

                D3D11_TEXTURE2D_DESC cur = {};
                if (sprite_)
                    sprite_->GetDesc(&cur);
                if (!sprite_ || cur.Width < rw || cur.Height < rh || cur.Format != DXGI_FORMAT_B8G8R8A8_UNORM)
                {
                    sprite_.Reset();
                    sprite_srv_.Reset();
                    D3D11_TEXTURE2D_DESC sd = {};
                    sd.Width = rw;
                    sd.Height = rh;
                    sd.MipLevels = sd.ArraySize = 1;
                    sd.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
                    sd.SampleDesc.Count = 1;
                    sd.Usage = D3D11_USAGE_DEFAULT;
                    sd.BindFlags = D3D11_BIND_SHADER_RESOURCE;
                    if (FAILED(dev->CreateTexture2D(&sd, nullptr, sprite_.GetAddressOf())))
                        return;
                    if (FAILED(dev->CreateShaderResourceView(sprite_.Get(), nullptr, sprite_srv_.GetAddressOf())))
                        return;
                }
                D3D11_BOX box = {0, 0, 0, rw, rh, 1};
                ctx->UpdateSubresource(sprite_.Get(), 0, &box, pixels.data(), rw * 4, 0);

                ID3D11RenderTargetView *prev_rtv = nullptr;
                ctx->OMGetRenderTargets(1, &prev_rtv, nullptr);
                UINT nv = D3D11_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE;
                D3D11_VIEWPORT prev_vp[D3D11_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE];
                ctx->RSGetViewports(&nv, prev_vp);

                D3D11_VIEWPORT vp = {(float)x0, (float)y0, (float)rw, (float)rh, 0.0f, 1.0f};
                ctx->RSSetViewports(1, &vp);
                ctx->OMSetRenderTargets(1, &desk_rtv, nullptr);
                const float zero[4] = {0, 0, 0, 0};
                ctx->OMSetBlendState(comp.blend(), zero, 0xFFFFFFFF);
                ctx->VSSetShader(comp.vs(), nullptr, 0);
                ctx->PSSetShader(comp.ps(), nullptr, 0);
                ctx->PSSetShaderResources(0, 1, sprite_srv_.GetAddressOf());
                ID3D11SamplerState *samp = comp.sampler();
                ctx->PSSetSamplers(0, 1, &samp);
                ctx->Draw(3, 0);

                ID3D11ShaderResourceView *none = nullptr;
                ctx->PSSetShaderResources(0, 1, &none);
                ctx->OMSetBlendState(nullptr, zero, 0xFFFFFFFF);
                ctx->OMSetRenderTargets(1, &prev_rtv, nullptr);
                if (prev_rtv)
                    prev_rtv->Release();
                ctx->RSSetViewports(nv, prev_vp);
            }

        private:
            const uint8_t *row(UINT y, size_t bytes) const
            {
                const size_t start = (size_t)y * shape_.Pitch;
                if (shape_.Pitch == 0 || start > len_ || bytes > len_ - start)
                    return nullptr;
                return buf_.data() + start;
            }

            void build_sprite(std::vector<uint32_t> &pixels, UINT rw, UINT rh, int px, int py)
            {
                const auto put = [&](UINT x, UINT y, uint32_t c)
                {
                    const int dx = px + (int)x, dy = py + (int)y;
                    if (dx < 0 || dy < 0 || dx >= (int)rw || dy >= (int)rh)
                        return;
                    pixels[(size_t)dy * rw + dx] = c;
                };
                if (shape_.Type == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR)
                {
                    for (UINT y = 0; y < shape_.Height; ++y)
                    {
                        const uint8_t *s = row(y, (size_t)shape_.Width * 4);
                        if (!s)
                            continue;
                        for (UINT x = 0; x < shape_.Width; ++x)
                        {
                            const uint8_t *p = s + x * 4;
                            put(x, y, (uint32_t)p[0] | ((uint32_t)p[1] << 8) | ((uint32_t)p[2] << 16) | ((uint32_t)p[3] << 24));
                        }
                    }
                }
                else if (shape_.Type == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR)
                {
                    for (UINT y = 0; y < shape_.Height; ++y)
                    {
                        const uint8_t *s = row(y, (size_t)shape_.Width * 4);
                        if (!s)
                            continue;
                        for (UINT x = 0; x < shape_.Width; ++x)
                        {
                            const uint8_t *p = s + x * 4;
                            // The mask channel requests XOR with the background; without a
                            // readback we draw the colour opaquely instead.
                            put(x, y, 0xFF000000u | (uint32_t)p[0] | ((uint32_t)p[1] << 8) | ((uint32_t)p[2] << 16));
                        }
                    }
                }
                else if (shape_.Type == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME)
                {
                    const UINT half = shape_.Height / 2;
                    const size_t stride = ((size_t)shape_.Width + 31) / 32 * 4;
                    for (UINT y = 0; y < half; ++y)
                    {
                        const uint8_t *and_row = row(y, stride), *xor_row = row(y + half, stride);
                        if (!and_row || !xor_row)
                            continue;
                        for (UINT x = 0; x < shape_.Width; ++x)
                        {
                            const uint8_t bit = 0x80u >> (x & 7);
                            const bool and_bit = (and_row[x / 8] & bit) != 0;
                            const bool xor_bit = (xor_row[x / 8] & bit) != 0;
                            if (and_bit && !xor_bit)
                                continue;
                            put(x, y, (!and_bit && !xor_bit) ? 0xFF000000u : 0xFFFFFFFFu);
                        }
                    }
                }
            }

            std::vector<uint8_t> buf_;
            size_t len_ = 0;
            DXGI_OUTDUPL_POINTER_SHAPE_INFO shape_ = {};
            POINT at_ = {};
            bool visible_ = false;
            bool have_shape_ = false;
            bool failed_ = false;
            ComPtr<ID3D11Texture2D> sprite_;
            ComPtr<ID3D11ShaderResourceView> sprite_srv_;
        };

        class DxgiCapture final : public CaptureSource
        {
        public:
            DxgiCapture(const GpuContext &gpu, const CaptureTarget &t, ObjectPool<VideoFrame> *frames,
                        ObjectPool<FrameAck> *acks, FrameSink sink)
                : CaptureSource(std::move(sink)), gpu_(gpu), target_(t), frames_(frames), acks_(acks), pacer_(20'000)
            {
            }
            ~DxgiCapture() override { stop(); }

            HRESULT start() override
            {
                HMONITOR mon = target_.monitor;
                if (target_.window)
                    mon = MonitorFromWindow(target_.window, MONITOR_DEFAULTTOPRIMARY);
                if (!mon)
                {
                    auto mons = enumerate_monitors();
                    if (mons.empty())
                        return fail(E_FAIL);
                    mon = mons[0].handle;
                }
                monitor_ = mon;
                MONITORINFO mi = {sizeof(mi)};
                if (!GetMonitorInfoW(mon, &mi))
                    return fail(HRESULT_FROM_WIN32(GetLastError()));
                mon_rect_ = mi.rcMonitor;
                stage_ = STAGE_FRAME_POOL;
                HRESULT hr = open_duplication();
                if (FAILED(hr))
                    return fail(hr);
                running_ = true;
                thread_ = std::thread([this]
                                      { run(); });
                return S_OK;
            }

            void stop() override
            {
                running_ = false;
                if (thread_.joinable())
                    thread_.join();
                pending_ack_.reset();
                dup_.Reset();
                desk_rtv_.Reset();
                desk_.Reset();
                base_.Reset();
            }

        private:
            HRESULT fail(HRESULT hr)
            {
                last_hr_ = hr;
                return hr;
            }

            HRESULT open_duplication()
            {
                dup_.Reset();
                ComPtr<IDXGIDevice> dxgi;
                HRESULT hr = gpu_.dev.As(&dxgi);
                if (FAILED(hr))
                    return hr;
                ComPtr<IDXGIAdapter> adapter;
                hr = dxgi->GetAdapter(&adapter);
                if (FAILED(hr))
                    return hr;
                for (UINT o = 0;; ++o)
                {
                    ComPtr<IDXGIOutput> out;
                    if (adapter->EnumOutputs(o, &out) == DXGI_ERROR_NOT_FOUND)
                        break;
                    DXGI_OUTPUT_DESC d;
                    if (FAILED(out->GetDesc(&d)) || d.Monitor != monitor_)
                        continue;
                    ComPtr<IDXGIOutput1> out1;
                    if (FAILED(out.As(&out1)))
                        return E_NOINTERFACE;
                    hr = out1->DuplicateOutput(gpu_.dev.Get(), &dup_);
                    if (FAILED(hr))
                        return hr;
                    DXGI_OUTDUPL_DESC dd;
                    dup_->GetDesc(&dd);
                    if (dd.ModeDesc.Width < 2 || dd.ModeDesc.Height < 2)
                        return E_INVALIDARG;
                    D3D11_TEXTURE2D_DESC td = {};
                    td.Width = dd.ModeDesc.Width;
                    td.Height = dd.ModeDesc.Height;
                    td.MipLevels = td.ArraySize = 1;
                    td.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
                    td.SampleDesc.Count = 1;
                    td.Usage = D3D11_USAGE_DEFAULT;
                    td.BindFlags = D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE;
                    td.MiscFlags = D3D11_RESOURCE_MISC_GDI_COMPATIBLE;
                    base_.Reset();
                    desk_.Reset();
                    desk_rtv_.Reset();
                    hr = gpu_.dev->CreateTexture2D(&td, nullptr, &base_);
                    if (FAILED(hr))
                        return hr;
                    hr = gpu_.dev->CreateTexture2D(&td, nullptr, &desk_);
                    if (FAILED(hr))
                        return hr;
                    hr = gpu_.dev->CreateRenderTargetView(desk_.Get(), nullptr, desk_rtv_.GetAddressOf());
                    if (FAILED(hr))
                        return hr;
                    if (target_.options & CAP_CURSOR)
                        compositor_.init(gpu_.dev.Get());
                    return S_OK;
                }
                return DXGI_ERROR_NOT_FOUND;
            }

            void run()
            {
                have_frame_ = false;
                while (running_)
                {
                    // DXGI publishes at most one outstanding frame: wait for the video
                    // lane's acknowledgement before acquiring the next desktop image.
                    if (pending_ack_)
                    {
                        if (!pending_ack_->wait_for(std::chrono::milliseconds(20)))
                            continue;
                        pending_ack_.reset();
                    }
                    if (target_.window && !IsWindow(target_.window))
                    {
                        closed_ = true;
                        break;
                    }
                    DXGI_OUTDUPL_FRAME_INFO info = {};
                    ComPtr<IDXGIResource> res;
                    HRESULT hr = dup_->AcquireNextFrame(50, &info, &res);
                    if (hr == DXGI_ERROR_WAIT_TIMEOUT)
                        continue;
                    if (hr == DXGI_ERROR_ACCESS_LOST)
                    {
                        dup_.Reset();
                        have_frame_ = false;
                        while (running_ && FAILED(open_duplication()))
                            Sleep(100);
                        continue;
                    }
                    if (FAILED(hr))
                    {
                        last_hr_ = hr;
                        Sleep(10);
                        continue;
                    }
                    if (info.LastPresentTime.QuadPart)
                    {
                        ComPtr<ID3D11Texture2D> frame;
                        if (SUCCEEDED(res.As(&frame)))
                        {
                            ScopedMt lock(gpu_);
                            gpu_.ctx->CopyResource(base_.Get(), frame.Get());
                            have_frame_ = true;
                        }
                    }
                    if (target_.options & CAP_CURSOR)
                        pointer_.update(dup_.Get(), info, (int)mon_rect_.left, (int)mon_rect_.top);
                    dup_->ReleaseFrame();

                    if (!have_frame_)
                        continue;
                    int64_t tick = info.LastPresentTime.QuadPart;
                    if (target_.options & CAP_CURSOR)
                        tick = std::max<int64_t>(tick, info.LastMouseUpdateTime.QuadPart);
                    if (tick <= 0)
                        continue;
                    const int64_t pts = qpc_to_100ns(tick);
                    if (!pacer_.accept(pts, (uint32_t)std::max(0, target_.fps)))
                    {
                        ++rejected_;
                        continue;
                    }
                    ++paced_;
                    publish(pts);
                }
                pending_ack_.reset();
            }

            bool compute_box(D3D11_BOX &box)
            {
                if (!desk_ || !base_)
                    return false;
                D3D11_TEXTURE2D_DESC td = {};
                desk_->GetDesc(&td);
                if (td.Width < 2 || td.Height < 2)
                    return false;
                box = {0, 0, 0, td.Width, td.Height, 1};
                if (target_.window)
                {
                    RECT r = {};
                    bool ok = false;
                    if (target_.options & CAP_CLIENT_AREA)
                    {
                        RECT c = {};
                        POINT o = {};
                        if (GetClientRect(target_.window, &c) && ClientToScreen(target_.window, &o))
                        {
                            r = {o.x, o.y, o.x + c.right, o.y + c.bottom};
                            ok = true;
                        }
                    }
                    else
                    {
                        ok = SUCCEEDED(DwmGetWindowAttribute(target_.window, DWMWA_EXTENDED_FRAME_BOUNDS, &r, sizeof(r)));
                    }
                    if (!ok || IsIconic(target_.window))
                        return false;
                    LONG l = std::max<LONG>(r.left - mon_rect_.left, 0), t = std::max<LONG>(r.top - mon_rect_.top, 0);
                    LONG rr = std::min<LONG>(r.right - mon_rect_.left, (LONG)td.Width);
                    LONG bb = std::min<LONG>(r.bottom - mon_rect_.top, (LONG)td.Height);
                    if (rr - l < 2 || bb - t < 2)
                        return false;
                    box = {(UINT)l, (UINT)t, 0, (UINT)rr, (UINT)bb, 1};
                }
                return true;
            }

            void publish(int64_t pts)
            {
                D3D11_BOX box = {};
                if (!compute_box(box))
                    return;
                {
                    // Start from the clean desktop every time so a mouse-only frame never
                    // burns the previous cursor into the image.
                    ScopedMt lock(gpu_);
                    gpu_.ctx->CopyResource(desk_.Get(), base_.Get());
                    if (target_.options & CAP_CURSOR)
                        pointer_.draw(gpu_.dev.Get(), gpu_.ctx.Get(), desk_.Get(), desk_rtv_.Get(), compositor_);
                }

                if (!frames_ || !acks_ || !sink_)
                {
                    ++rejected_;
                    return;
                }
                Ref<VideoFrame> vf = frames_->acquire();
                if (!vf)
                {
                    ++rejected_;
                    return;
                }
                Ref<FrameAck> ack = acks_->acquire();
                if (!ack)
                {
                    ++rejected_;
                    return;
                }
                vf->texture = desk_;
                vf->box = box;
                vf->pts100ns = pts;
                vf->ack = ack;
                emit(std::move(vf));
                pending_ack_ = std::move(ack);
            }

            GpuContext gpu_;
            CaptureTarget target_;
            ObjectPool<VideoFrame> *frames_ = nullptr;
            ObjectPool<FrameAck> *acks_ = nullptr;
            FramePacer pacer_;
            HMONITOR monitor_ = nullptr;
            RECT mon_rect_ = {};
            ComPtr<IDXGIOutputDuplication> dup_;
            ComPtr<ID3D11Texture2D> base_;
            ComPtr<ID3D11Texture2D> desk_;
            ComPtr<ID3D11RenderTargetView> desk_rtv_;
            DxgiPointer pointer_;
            CursorCompositor compositor_;
            Ref<FrameAck> pending_ack_;
            bool have_frame_ = false;
            std::atomic<bool> running_{false};
            std::thread thread_;
        };

    } // namespace

    std::unique_ptr<CaptureSource> make_wgc_capture(const GpuContext &gpu, const CaptureTarget &t,
                                                     ObjectPool<VideoFrame> *frames,
                                                     ObjectPool<Keepalive> *keepalives, FrameSink sink)
    {
        return std::make_unique<WgcCapture>(gpu, t, frames, keepalives, std::move(sink));
    }

    std::unique_ptr<CaptureSource> make_dxgi_capture(const GpuContext &gpu, const CaptureTarget &t,
                                                      ObjectPool<VideoFrame> *frames,
                                                      ObjectPool<Keepalive> *, ObjectPool<FrameAck> *acks, FrameSink sink)
    {
        return std::make_unique<DxgiCapture>(gpu, t, frames, acks, std::move(sink));
    }

} // namespace ir
