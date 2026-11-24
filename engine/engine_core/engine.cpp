#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <windowsx.h>

#include <audioclient.h>
#include <d3d11.h>
#include <dxgi.h>
#include <dxgi1_2.h>

#define INITGUID
#include <mmdeviceapi.h>
#include <functiondiscoverykeys_devpkey.h>
#undef INITGUID

#include <ksmedia.h>
#include <mfapi.h>
#include <mferror.h>
#include <mfidl.h>
#include <mfreadwrite.h>
#include <mftransform.h>
#include <mmsystem.h>
#include <objbase.h>
#include <propvarutil.h>
#include <wrl/client.h>

#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Graphics.Capture.h>
#include <winrt/Windows.Graphics.DirectX.h>
#include <winrt/Windows.Graphics.DirectX.Direct3D11.h>
#include <Windows.Graphics.Capture.Interop.h>
#include <Windows.Graphics.DirectX.Direct3D11.interop.h>

#include <algorithm>
#include <atomic>
#include <chrono>
#include <cmath>
#include <cstdarg>
#include <cstdio>
#include <cstring>
#include <cwchar>
#include <deque>
#include <memory>
#include <mutex>
#include <new>
#include <optional>
#include <string>
#include <thread>
#include <type_traits>
#include <utility>
#include <vector>

#include "engine.hpp"

using Microsoft::WRL::ComPtr;

namespace
{

    constexpr uint64_t kEngineMagic = 0x4952454e47494e45ull;
    constexpr size_t kRingByteBudget = 320ull * 1024 * 1024;
    constexpr size_t kAudioByteBudget = 24ull * 1024 * 1024;
    constexpr size_t kMaxSaveFrames = 5400;
    constexpr int kMaxOutputWidth = 3840;
    constexpr int kMaxOutputHeight = 2160;


    std::mutex g_errorMutex;
    std::string g_lastErrorUtf8;
    std::atomic<int> g_saveProgress{-1};
    std::once_flag g_mfOnce;

    void SetIrError(const char *format, ...) noexcept
    {
        char text[1024];
        va_list args;
        va_start(args, format);
        std::vsnprintf(text, sizeof(text), format, args);
        va_end(args);

        std::lock_guard<std::mutex> lock(g_errorMutex);
        g_lastErrorUtf8 = text;
    }

    void ClearIrError() noexcept
    {
        std::lock_guard<std::mutex> lock(g_errorMutex);
        g_lastErrorUtf8.clear();
    }

    std::string GetIrError() noexcept
    {
        std::lock_guard<std::mutex> lock(g_errorMutex);
        return g_lastErrorUtf8;
    }

    int64_t Qpc100Ns() noexcept
    {
        LARGE_INTEGER counter{};
        LARGE_INTEGER frequency{};
        if (!QueryPerformanceCounter(&counter) || !QueryPerformanceFrequency(&frequency) || frequency.QuadPart == 0)
            return static_cast<int64_t>(std::chrono::duration_cast<std::chrono::nanoseconds>(
                                            std::chrono::steady_clock::now().time_since_epoch())
                                            .count() /
                                        100);
        return static_cast<int64_t>((counter.QuadPart * 10000000ll) / frequency.QuadPart);
    }

    double QpcMs(int64_t start) noexcept
    {
        return static_cast<double>(Qpc100Ns() - start) / 10000.0;
    }

    int Align16(int value) noexcept
    {
        return (value + 15) & ~15;
    }

    bool LowMemory() noexcept
    {
        MEMORYSTATUSEX status{};
        status.dwLength = sizeof(status);
        if (!GlobalMemoryStatusEx(&status))
            return false;
        return status.ullAvailPhys < (512ull * 1024 * 1024) || status.dwMemoryLoad > 94;
    }

    void SafeCopyWchar(wchar_t *destination, int capacity, const std::wstring &source)
    {
        if (!destination || capacity <= 0)
            return;
        const size_t count = std::min<size_t>(source.size(), static_cast<size_t>(capacity - 1));
        if (count)
            std::memcpy(destination, source.data(), count * sizeof(wchar_t));
        destination[count] = L'\0';
    }


    struct VideoFrame
    {
        int64_t pts100ns = 0;
        int width = 0;
        int height = 0;
        std::vector<uint8_t> nv12;
    };

    struct AudioBlock
    {
        int64_t pts100ns = 0;
        std::vector<int16_t> pcm;
    };

    struct AudioDiag
    {
        std::atomic<long> activateHr{0};
        std::atomic<long> initHr{0};
        std::atomic<long> startHr{0};
        std::atomic<long> lastFailHr{0};
        std::atomic<int> tag{0};
        std::atomic<int> rate{0};
        std::atomic<int> channels{0};
        std::atomic<int> bits{0};
        std::atomic<int> isFloat{0};
        std::atomic<int> convertFailures{0};
        std::atomic<long> failCode{0};
        std::atomic<int> peak{0};
        std::atomic<int64_t> frames{0};
        std::atomic<int64_t> events{0};
        std::atomic<int64_t> got{0};
        std::wstring activeName;
        std::mutex nameMutex;
    };

    struct Engine;


    class AudioCapture
    {
    public:
        AudioCapture(Engine *owner, bool microphone, std::wstring requestedDevice, HWND processWindow)
            : m_owner(owner), m_microphone(microphone), m_requested(std::move(requestedDevice)),
              m_processWindow(processWindow)
        {
        }

        ~AudioCapture()
        {
            Stop();
            if (m_comInitializedInStart)
            {
                CoUninitialize();
                m_comInitializedInStart = false;
            }
        }

        bool Start();
        void Stop();
        void Run();

        AudioDiag &Diag() { return m_diag; }

    private:
        Engine *m_owner = nullptr;
        bool m_microphone = false;
        std::wstring m_requested;
        HWND m_processWindow = nullptr;

        ComPtr<IAudioClient> m_client;
        ComPtr<IAudioCaptureClient> m_capture;
        HANDLE m_event = nullptr;
        std::thread m_thread;
        std::atomic<bool> m_stop{false};

        int m_formatTag = 0;
        int m_rate = 0;
        int m_channels = 0;
        int m_bits = 0;
        bool m_isFloat = false;
        std::vector<uint8_t> m_formatBytes;
        bool m_comInitializedInStart = false;

        AudioDiag m_diag;

        void ConvertAndAppend(const BYTE *data, UINT32 frames, DWORD flags);
    };


    struct CaptureConfig
    {
        int method = IR_CAPTURE_WGC;
        int monitorIndex = 0;
        int fps = 30;
        int ringSeconds = 30;
        int capWidth = 0;
        int capHeight = 0;
        HWND window = nullptr;
        uint32_t options = 0;
    };

    struct CaptureContext
    {
        Engine *engine = nullptr;
        std::atomic<bool> stop{false};
        std::atomic<bool> paused{false};

        CaptureConfig config;

        ComPtr<ID3D11Device> device;
        ComPtr<ID3D11DeviceContext> context;
        ComPtr<ID3D11Texture2D> staging;
        D3D11_TEXTURE2D_DESC stagingDesc{};

        winrt::Windows::Graphics::Capture::GraphicsCaptureItem item{nullptr};
        winrt::Windows::Graphics::Capture::Direct3D11CaptureFramePool pool{nullptr};
        winrt::Windows::Graphics::Capture::GraphicsCaptureSession session{nullptr};

        ComPtr<IDXGIOutputDuplication> duplication;

        int64_t lastSourceTick = -1;
        int outputWidth = 0;
        int outputHeight = 0;
    };

    struct SaveJob
    {
        uint64_t id = 0;
        std::atomic<int> progress{0};
        std::atomic<int> result{IR_PENDING};
        std::thread worker;
    };

    struct Engine
    {
        uint64_t magic = kEngineMagic;
        std::mutex mutex;

        CaptureConfig config;
        bool running = false;
        bool paused = false;
        bool destroyRequested = false;
        int lastFailStage = IR_FAIL_NONE;
        long lastHresult = 0;
        long lastFailHr = 0;
        long lastCaptureHr = 0;
        long hwInitHr = 0;
        int activeEncoder = IR_ENC_H264_SOFTWARE;

        int bitrateKbps = 6000;
        int encoderPreference = 0;
        uint32_t encoderProbe = 0;
        bool captureAudioRequested = false;
        bool captureMicRequested = false;
        std::wstring micRequested;

        std::shared_ptr<CaptureContext> capture;
        std::thread captureThread;

        std::mutex ringMutex;
        std::deque<VideoFrame> ring;
        size_t ringBytes = 0;
        size_t maxRingFrames = 600;

        std::mutex audioMutex;
        std::deque<AudioBlock> systemAudio;
        std::deque<AudioBlock> micAudio;
        size_t systemAudioBytes = 0;
        size_t micAudioBytes = 0;

        std::unique_ptr<AudioCapture> systemCapture;
        std::unique_ptr<AudioCapture> micCapture;

        std::mutex jobsMutex;
        std::vector<std::shared_ptr<SaveJob>> jobs;
        uint64_t nextJobId = 1;

        std::atomic<int64_t> captured{0};
        std::atomic<int64_t> paced{0};
        std::atomic<int64_t> rejected{0};
        std::atomic<int64_t> encoded{0};
        std::atomic<int64_t> dropped{0};
        std::atomic<int64_t> audioFrames{0};
        std::atomic<int64_t> audioEvents{0};
        std::atomic<int64_t> audioGot{0};
        std::atomic<int64_t> heartbeat{0};
        std::atomic<int64_t> lastPts{0};
        std::atomic<int64_t> startQpc{0};
        std::atomic<bool> hasError{false};

        std::mutex timingMutex;
        double timingMapMs = 0;
        double timingEncodeMs = 0;
        double timingRenderMs = 0;
        double timingSubmitMs = 0;
        double timingSnapshotMs = 0;
        double timingUnmapMs = 0;
        int64_t timingFrames = 0;

        AudioDiag systemDiag;
        AudioDiag micDiag;
    };

    Engine *FromHandle(void *handle) noexcept
    {
        auto *engine = static_cast<Engine *>(handle);
        return (engine && engine->magic == kEngineMagic) ? engine : nullptr;
    }

    void AppendSystemAudio(Engine *engine, AudioBlock block)
    {
        std::lock_guard<std::mutex> lock(engine->audioMutex);
        size_t bytes = block.pcm.size() * sizeof(int16_t);
        while (!engine->systemAudio.empty() &&
               (engine->systemAudioBytes + bytes > kAudioByteBudget || engine->systemAudio.size() > 2400))
        {
            engine->systemAudioBytes -= engine->systemAudio.front().pcm.size() * sizeof(int16_t);
            engine->systemAudio.pop_front();
        }
        engine->systemAudioBytes += bytes;
        engine->systemAudio.push_back(std::move(block));
        engine->audioFrames.fetch_add(1, std::memory_order_relaxed);
    }

    void AppendMicAudio(Engine *engine, AudioBlock block)
    {
        std::lock_guard<std::mutex> lock(engine->audioMutex);
        size_t bytes = block.pcm.size() * sizeof(int16_t);
        while (!engine->micAudio.empty() &&
               (engine->micAudioBytes + bytes > kAudioByteBudget || engine->micAudio.size() > 2400))
        {
            engine->micAudioBytes -= engine->micAudio.front().pcm.size() * sizeof(int16_t);
            engine->micAudio.pop_front();
        }
        engine->micAudioBytes += bytes;
        engine->micAudio.push_back(std::move(block));
    }

    void AppendVideo(Engine *engine, VideoFrame frame)
    {
        std::lock_guard<std::mutex> lock(engine->ringMutex);
        const size_t bytes = frame.nv12.size();
        const size_t maxFrames = std::max<size_t>(2, engine->maxRingFrames);
        while (!engine->ring.empty() &&
               (engine->ringBytes + bytes > kRingByteBudget || engine->ring.size() >= maxFrames))
        {
            engine->ringBytes -= engine->ring.front().nv12.size();
            engine->ring.pop_front();
            engine->dropped.fetch_add(1, std::memory_order_relaxed);
        }
        engine->ringBytes += bytes;
        engine->ring.push_back(std::move(frame));
    }


    BOOL CALLBACK MonitorEnumProc(HMONITOR monitor, HDC, LPRECT, LPARAM param)
    {
        auto *monitors = reinterpret_cast<std::vector<std::pair<HMONITOR, RECT>> *>(param);
        MONITORINFOEXW info{};
        info.cbSize = sizeof(info);
        if (GetMonitorInfoW(monitor, &info))
            monitors->emplace_back(monitor, info.rcMonitor);
        return TRUE;
    }

    std::vector<std::pair<HMONITOR, RECT>> EnumerateMonitors()
    {
        std::vector<std::pair<HMONITOR, RECT>> monitors;
        EnumDisplayMonitors(nullptr, nullptr, MonitorEnumProc, reinterpret_cast<LPARAM>(&monitors));
        return monitors;
    }

    HMONITOR FindMonitor(int index)
    {
        auto monitors = EnumerateMonitors();
        if (monitors.empty())
            return nullptr;
        if (index < 0 || static_cast<size_t>(index) >= monitors.size())
            index = 0;
        return monitors[static_cast<size_t>(index)].first;
    }

    BOOL CALLBACK WindowEnumProc(HWND window, LPARAM param)
    {
        auto *windows = reinterpret_cast<std::vector<HWND> *>(param);
        if (!IsWindowVisible(window))
            return TRUE;
        if (GetWindowTextLengthW(window) <= 0)
            return TRUE;
        windows->push_back(window);
        return TRUE;
    }

    std::wstring DeviceFriendlyName(IMMDevice *device)
    {
        std::wstring name;
        if (!device)
            return name;
        ComPtr<IPropertyStore> properties;
        if (FAILED(device->OpenPropertyStore(STGM_READ, properties.GetAddressOf())))
            return name;
        PROPVARIANT value{};
        PropVariantInit(&value);
        if (SUCCEEDED(properties->GetValue(PKEY_Device_FriendlyName, &value)) && value.pwszVal)
            name = value.pwszVal;
        PropVariantClear(&value);
        return name;
    }

    std::vector<std::pair<std::wstring, ComPtr<IMMDevice>>> EnumerateMicrophoneDevices()
    {
        std::vector<std::pair<std::wstring, ComPtr<IMMDevice>>> devices;
        ComPtr<IMMDeviceEnumerator> enumerator;
        HRESULT hr = CoCreateInstance(__uuidof(MMDeviceEnumerator), nullptr, CLSCTX_ALL,
                                      IID_PPV_ARGS(enumerator.GetAddressOf()));
        if (FAILED(hr))
            return devices;
        ComPtr<IMMDeviceCollection> collection;
        if (FAILED(enumerator->EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE, collection.GetAddressOf())))
            return devices;
        UINT count = 0;
        if (FAILED(collection->GetCount(&count)))
            return devices;
        for (UINT i = 0; i < count; ++i)
        {
            ComPtr<IMMDevice> device;
            if (SUCCEEDED(collection->Item(i, device.GetAddressOf())))
                devices.emplace_back(DeviceFriendlyName(device.Get()), device);
        }
        return devices;
    }


    bool AudioCapture::Start()
    {
        HRESULT hr = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
        if (hr == S_OK)
            m_comInitializedInStart = true;
        else if (hr != S_FALSE && hr != RPC_E_CHANGED_MODE)
        {
            m_diag.failCode.store(-1);
            m_diag.activateHr.store(hr);
            SetIrError("CoInitializeEx failed for %s audio: 0x%08lX", m_microphone ? "mic" : "system", hr);
            return false;
        }

        ComPtr<IMMDeviceEnumerator> enumerator;
        hr = CoCreateInstance(__uuidof(MMDeviceEnumerator), nullptr, CLSCTX_ALL,
                              IID_PPV_ARGS(enumerator.GetAddressOf()));
        m_diag.activateHr.store(hr);
        if (FAILED(hr))
        {
            m_diag.failCode.store(-1);
            SetIrError("MMDeviceEnumerator failed for %s audio: 0x%08lX", m_microphone ? "mic" : "system", hr);
            return false;
        }

        ComPtr<IMMDevice> device;
        if (m_microphone && !m_requested.empty())
        {
            for (auto &candidate : EnumerateMicrophoneDevices())
            {
                if (candidate.first == m_requested)
                {
                    device = candidate.second;
                    break;
                }
            }
            if (!device)
            {
                m_diag.failCode.store(-3);
                m_diag.lastFailHr.store(HRESULT_FROM_WIN32(ERROR_NOT_FOUND));
                SetIrError("Microphone '%S' was not found", m_requested.c_str());
                return false;
            }
            {
                std::lock_guard<std::mutex> lock(m_diag.nameMutex);
                m_diag.activeName = m_requested;
            }
        }
        else
        {
            EDataFlow flow = m_microphone ? eCapture : eRender;
            hr = enumerator->GetDefaultAudioEndpoint(flow, eMultimedia, device.GetAddressOf());
            if (FAILED(hr))
            {
                m_diag.failCode.store(-3);
                m_diag.lastFailHr.store(hr);
                SetIrError("No active %s endpoint: 0x%08lX", m_microphone ? "capture" : "render", hr);
                return false;
            }
            {
                std::lock_guard<std::mutex> lock(m_diag.nameMutex);
                m_diag.activeName = DeviceFriendlyName(device.Get());
            }
        }

        hr = device->Activate(__uuidof(IAudioClient), CLSCTX_ALL, nullptr,
                              reinterpret_cast<void **>(m_client.GetAddressOf()));
        if (FAILED(hr))
        {
            m_diag.failCode.store(-2);
            m_diag.lastFailHr.store(hr);
            SetIrError("IAudioClient activate failed: 0x%08lX", hr);
            return false;
        }

        WAVEFORMATEX *mix = nullptr;
        hr = m_client->GetMixFormat(&mix);
        if (FAILED(hr) || !mix)
        {
            m_diag.failCode.store(-2);
            m_diag.lastFailHr.store(hr);
            SetIrError("GetMixFormat failed: 0x%08lX", hr);
            return false;
        }
        const size_t formatBytes = sizeof(WAVEFORMATEX) + mix->cbSize;
        m_formatBytes.resize(formatBytes);
        std::memcpy(m_formatBytes.data(), mix, formatBytes);
        CoTaskMemFree(mix);

        const WAVEFORMATEX *format = reinterpret_cast<const WAVEFORMATEX *>(m_formatBytes.data());
        m_formatTag = format->wFormatTag;
        m_rate = static_cast<int>(format->nSamplesPerSec);
        m_channels = static_cast<int>(format->nChannels);
        m_bits = static_cast<int>(format->wBitsPerSample);
        m_isFloat = (m_formatTag == WAVE_FORMAT_IEEE_FLOAT);
        if (m_formatTag == WAVE_FORMAT_EXTENSIBLE && formatBytes >= sizeof(WAVEFORMATEX) + sizeof(GUID))
        {
            GUID subformat{};
            std::memcpy(&subformat, m_formatBytes.data() + sizeof(WAVEFORMATEX), sizeof(GUID));
            m_isFloat = (subformat == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT);
            m_formatTag = m_isFloat ? WAVE_FORMAT_IEEE_FLOAT : WAVE_FORMAT_PCM;
        }
        m_diag.tag.store(m_formatTag);
        m_diag.rate.store(m_rate);
        m_diag.channels.store(m_channels);
        m_diag.bits.store(m_bits);
        m_diag.isFloat.store(m_isFloat ? 1 : 0);

        DWORD flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
        if (!m_microphone)
            flags |= AUDCLNT_STREAMFLAGS_LOOPBACK;
        hr = m_client->Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 2000000, 0,
                                  reinterpret_cast<WAVEFORMATEX *>(m_formatBytes.data()), nullptr);
        m_diag.initHr.store(hr);
        if (FAILED(hr))
        {
            m_diag.failCode.store(-2);
            m_diag.lastFailHr.store(hr);
            SetIrError("IAudioClient::Initialize failed for %s audio: 0x%08lX", m_microphone ? "mic" : "system", hr);
            return false;
        }

        hr = m_client->GetService(__uuidof(IAudioCaptureClient),
                                  reinterpret_cast<void **>(m_capture.GetAddressOf()));
        if (FAILED(hr))
        {
            m_diag.failCode.store(-2);
            m_diag.lastFailHr.store(hr);
            SetIrError("IAudioCaptureClient lookup failed: 0x%08lX", hr);
            return false;
        }

        m_event = CreateEventW(nullptr, FALSE, FALSE, nullptr);
        if (!m_event)
        {
            m_diag.failCode.store(-2);
            m_diag.lastFailHr.store(HRESULT_FROM_WIN32(GetLastError()));
            return false;
        }
        hr = m_client->SetEventHandle(m_event);
        if (FAILED(hr))
        {
            m_diag.failCode.store(-2);
            m_diag.lastFailHr.store(hr);
            SetIrError("SetEventHandle failed: 0x%08lX", hr);
            return false;
        }

        hr = m_client->Start();
        m_diag.startHr.store(hr);
        if (FAILED(hr))
        {
            m_diag.failCode.store(-2);
            m_diag.lastFailHr.store(hr);
            SetIrError("IAudioClient::Start failed: 0x%08lX", hr);
            return false;
        }

        m_stop.store(false);
        m_thread = std::thread(&AudioCapture::Run, this);
        return true;
    }

    void AudioCapture::Stop()
    {
        m_stop.store(true);
        if (m_event)
            SetEvent(m_event);
        if (m_thread.joinable())
            m_thread.join();
        if (m_client)
            m_client->Stop();
        if (m_event)
        {
            CloseHandle(m_event);
            m_event = nullptr;
        }
    }

    void AudioCapture::ConvertAndAppend(const BYTE *data, UINT32 frames, DWORD flags)
    {
        if (!data || frames == 0)
            return;
        m_diag.events.fetch_add(1, std::memory_order_relaxed);
        const double ratio = static_cast<double>(m_rate) / 48000.0;
        const size_t outFrames = std::max<size_t>(1, static_cast<size_t>(std::llround(frames / ratio)));
        std::vector<int16_t> pcm(outFrames * 2);

        const bool silent = (flags & AUDCLNT_BUFFERFLAGS_SILENT) != 0;
        auto readSample = [&](size_t frame, int channel, double &sample)
        {
            if (silent || frame >= frames)
            {
                sample = 0.0;
                return;
            }
            size_t index = 0;
            double value = 0.0;
            if (m_isFloat)
            {
                const float *source = reinterpret_cast<const float *>(data) + frame * m_channels;
                index = static_cast<size_t>(channel < m_channels ? channel : m_channels - 1);
                value = source[index];
            }
            else if (m_bits == 16)
            {
                const int16_t *source = reinterpret_cast<const int16_t *>(data) + frame * m_channels;
                index = static_cast<size_t>(channel < m_channels ? channel : m_channels - 1);
                value = static_cast<double>(source[index]) / 32768.0;
            }
            else if (m_bits == 32)
            {
                const int32_t *source = reinterpret_cast<const int32_t *>(data) + frame * m_channels;
                index = static_cast<size_t>(channel < m_channels ? channel : m_channels - 1);
                value = static_cast<double>(source[index]) / 2147483648.0;
            }
            else
            {
                m_diag.convertFailures.fetch_add(1, std::memory_order_relaxed);
                value = 0.0;
            }
            sample = std::clamp(value, -1.0, 1.0);
        };

        for (size_t i = 0; i < outFrames; ++i)
        {
            const double sourceFrame = static_cast<double>(i) * ratio;
            const size_t lo = static_cast<size_t>(sourceFrame);
            const size_t hi = std::min<size_t>(frames - 1, lo + 1);
            const double fraction = sourceFrame - static_cast<double>(lo);
            double l0 = 0, l1 = 0, r0 = 0, r1 = 0;
            readSample(lo, 0, l0);
            readSample(hi, 0, l1);
            readSample(lo, m_channels > 1 ? 1 : 0, r0);
            readSample(hi, m_channels > 1 ? 1 : 0, r1);
            const double left = l0 + (l1 - l0) * fraction;
            const double right = r0 + (r1 - r0) * fraction;
            pcm[i * 2] = static_cast<int16_t>(std::clamp(left, -1.0, 1.0) * 32767.0);
            pcm[i * 2 + 1] = static_cast<int16_t>(std::clamp(right, -1.0, 1.0) * 32767.0);
            const int peak = static_cast<int>(std::max(std::abs(pcm[i * 2]), std::abs(pcm[i * 2 + 1])));
            m_diag.peak.store(std::max(m_diag.peak.load(std::memory_order_relaxed), peak), std::memory_order_relaxed);
        }

        AudioBlock block{Qpc100Ns(), std::move(pcm)};
        m_diag.frames.fetch_add(static_cast<int64_t>(outFrames), std::memory_order_relaxed);
        m_diag.got.fetch_add(1, std::memory_order_relaxed);
        if (m_owner)
        {
            m_owner->audioEvents.fetch_add(1, std::memory_order_relaxed);
            m_owner->audioGot.fetch_add(1, std::memory_order_relaxed);
        }
        if (m_owner && (!m_owner->paused || m_microphone))
        {
            if (m_microphone)
                AppendMicAudio(m_owner, std::move(block));
            else
                AppendSystemAudio(m_owner, std::move(block));
        }
    }

    void AudioCapture::Run()
    {
        HRESULT hr = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
        const bool initialized = (hr == S_OK);
        HANDLE waits[2] = {m_event, CreateEventW(nullptr, TRUE, FALSE, nullptr)};
        if (!waits[1])
            waits[1] = m_event;

        while (!m_stop.load(std::memory_order_relaxed))
        {
            const DWORD result = WaitForMultipleObjects(2, waits, FALSE, 200);
            if (result == WAIT_OBJECT_0 || result == WAIT_OBJECT_0 + 1)
            {
                BYTE *data = nullptr;
                UINT32 frames = 0;
                DWORD flags = 0;
                UINT64 position = 0;
                UINT64 qpcPosition = 0;
                while (m_capture && SUCCEEDED(m_capture->GetBuffer(&data, &frames, &flags, &position, &qpcPosition)))
                {
                    if (frames == 0)
                        break;
                    ConvertAndAppend(data, frames, flags);
                    m_capture->ReleaseBuffer(frames);
                }
            }
        }

        if (m_client)
            m_client->Stop();
        if (waits[1] && waits[1] != m_event)
            CloseHandle(waits[1]);
        if (initialized)
            CoUninitialize();
    }


    void ComputeOutputSize(int sourceWidth, int sourceHeight, const CaptureConfig &config, int &outW, int &outH)
    {
        outW = std::max(2, sourceWidth);
        outH = std::max(2, sourceHeight);

        double scale = 1.0;
        if (config.capWidth > 0 && outW > config.capWidth)
            scale = std::min(scale, static_cast<double>(config.capWidth) / outW);
        if (config.capHeight > 0 && outH > config.capHeight)
            scale = std::min(scale, static_cast<double>(config.capHeight) / outH);
        if (outW > kMaxOutputWidth)
            scale = std::min(scale, static_cast<double>(kMaxOutputWidth) / outW);
        if (outH > kMaxOutputHeight)
            scale = std::min(scale, static_cast<double>(kMaxOutputHeight) / outH);

        outW = Align16(std::max(16, static_cast<int>(std::floor(outW * scale))));
        outH = Align16(std::max(16, static_cast<int>(std::floor(outH * scale))));
        outW = std::min(outW, kMaxOutputWidth);
        outH = std::min(outH, kMaxOutputHeight);
    }

    void ConvertBgraToNv12(const uint8_t *src, size_t srcPitch, int srcW, int srcH,
                           int outW, int outH, std::vector<uint8_t> &out)
    {
        out.resize(static_cast<size_t>(outW) * outH * 3 / 2);
        uint8_t *yPlane = out.data();
        uint8_t *uvPlane = yPlane + static_cast<size_t>(outW) * outH;

        const bool bt709 = srcH >= 720;
        const double kr = bt709 ? 0.2126 : 0.299;
        const double kb = bt709 ? 0.0722 : 0.114;
        const double kg = 1.0 - kr - kb;

        auto sample = [&](int x, int y, int &r, int &g, int &b)
        {
            const int sx = std::min(srcW - 1, std::max(0, x * srcW / outW));
            const int sy = std::min(srcH - 1, std::max(0, y * srcH / outH));
            const uint8_t *pixel = src + static_cast<size_t>(sy) * srcPitch + static_cast<size_t>(sx) * 4;
            b = pixel[0];
            g = pixel[1];
            r = pixel[2];
        };

        for (int y = 0; y < outH; ++y)
        {
            for (int x = 0; x < outW; ++x)
            {
                int r = 0, g = 0, b = 0;
                sample(x, y, r, g, b);
                const double luma = kr * r + kg * g + kb * b;
                yPlane[static_cast<size_t>(y) * outW + x] =
                    static_cast<uint8_t>(std::clamp(16.0 + 219.0 * luma / 255.0, 16.0, 235.0));
            }
        }

        for (int y = 0; y < outH / 2; ++y)
        {
            for (int x = 0; x < outW / 2; ++x)
            {
                int r = 0, g = 0, b = 0;
                sample(x * 2, y * 2, r, g, b);
                const double cb = (b - (kr * r + kg * g + kb * b)) / (2.0 * (1.0 - kb));
                const double cr = (r - (kr * r + kg * g + kb * b)) / (2.0 * (1.0 - kr));
                const size_t index = static_cast<size_t>(y) * outW + x * 2;
                uvPlane[index] = static_cast<uint8_t>(std::clamp(128.0 + 224.0 * cb / 255.0, 16.0, 240.0));
                uvPlane[index + 1] = static_cast<uint8_t>(std::clamp(128.0 + 224.0 * cr / 255.0, 16.0, 240.0));
            }
        }
    }

    struct IDxgiInterfaceAccess : IUnknown
    {
        virtual HRESULT STDMETHODCALLTYPE GetInterface(REFIID riid, void **object) = 0;
    };

    const GUID IID_IDxgiInterfaceAccessLocal =
        {0xA9B3D012, 0x3DF2, 0x4EE3, {0xB8, 0xD1, 0x86, 0x95, 0xF4, 0x57, 0xD3, 0xC1}};


    HRESULT CreateD3D11Device(ComPtr<ID3D11Device> &device, ComPtr<ID3D11DeviceContext> &context, long *hwHr)
    {
        D3D_FEATURE_LEVEL levels[] = {D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_1, D3D_FEATURE_LEVEL_10_0};
        D3D_FEATURE_LEVEL selected = D3D_FEATURE_LEVEL_11_0;
        UINT flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT;
#if defined(_DEBUG)
        flags |= D3D11_CREATE_DEVICE_DEBUG;
#endif
        HRESULT hr = D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr, flags,
                                       levels, ARRAYSIZE(levels), D3D11_SDK_VERSION,
                                       device.GetAddressOf(), &selected, context.GetAddressOf());
        if (hwHr)
            *hwHr = hr;
        return hr;
    }

    HRESULT EnsureStaging(CaptureContext &ctx, const D3D11_TEXTURE2D_DESC &desc)
    {
        if (ctx.staging && ctx.stagingDesc.Width == desc.Width && ctx.stagingDesc.Height == desc.Height &&
            ctx.stagingDesc.Format == desc.Format)
            return S_OK;
        D3D11_TEXTURE2D_DESC stagingDesc{};
        stagingDesc.Width = desc.Width;
        stagingDesc.Height = desc.Height;
        stagingDesc.MipLevels = 1;
        stagingDesc.ArraySize = 1;
        stagingDesc.Format = desc.Format;
        stagingDesc.SampleDesc.Count = 1;
        stagingDesc.Usage = D3D11_USAGE_STAGING;
        stagingDesc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
        HRESULT hr = ctx.device->CreateTexture2D(&stagingDesc, nullptr, ctx.staging.ReleaseAndGetAddressOf());
        if (SUCCEEDED(hr))
            ctx.stagingDesc = stagingDesc;
        return hr;
    }

    void ProcessTexture(CaptureContext &ctx, ID3D11Texture2D *texture, int64_t sourceTick)
    {
        Engine *engine = ctx.engine;
        if (!engine || !texture)
            return;

        const int64_t stageStart = Qpc100Ns();
        engine->captured.fetch_add(1, std::memory_order_relaxed);
        if (sourceTick >= 0)
        {
            if (sourceTick <= ctx.lastSourceTick)
            {
                engine->rejected.fetch_add(1, std::memory_order_relaxed);
                return;
            }
            ctx.lastSourceTick = sourceTick;
        }

        D3D11_TEXTURE2D_DESC desc{};
        texture->GetDesc(&desc);
        if (!desc.Width || !desc.Height)
            return;

        int outW = 0, outH = 0;
        ComputeOutputSize(static_cast<int>(desc.Width), static_cast<int>(desc.Height), ctx.config, outW, outH);
        if (ctx.outputWidth != outW || ctx.outputHeight != outH)
        {
            ctx.outputWidth = outW;
            ctx.outputHeight = outH;
        }

        HRESULT hr = EnsureStaging(ctx, desc);
        if (FAILED(hr))
        {
            engine->lastHresult = hr;
            engine->hasError.store(true);
            return;
        }
        ctx.context->CopyResource(ctx.staging.Get(), texture);

        D3D11_MAPPED_SUBRESOURCE mapped{};
        hr = ctx.context->Map(ctx.staging.Get(), 0, D3D11_MAP_READ, 0, &mapped);
        if (FAILED(hr))
        {
            engine->lastHresult = hr;
            return;
        }
        const double mapMs = QpcMs(stageStart);
        const int64_t convertStart = Qpc100Ns();

        VideoFrame frame;
        frame.pts100ns = Qpc100Ns();
        frame.width = outW;
        frame.height = outH;
        ConvertBgraToNv12(static_cast<const uint8_t *>(mapped.pData), mapped.RowPitch,
                          static_cast<int>(desc.Width), static_cast<int>(desc.Height), outW, outH, frame.nv12);
        ctx.context->Unmap(ctx.staging.Get(), 0);
        const double convertMs = QpcMs(convertStart);

        engine->paced.fetch_add(1, std::memory_order_relaxed);
        engine->encoded.fetch_add(1, std::memory_order_relaxed);
        engine->lastPts.store(frame.pts100ns, std::memory_order_relaxed);
        engine->heartbeat.fetch_add(1, std::memory_order_relaxed);

        {
            std::lock_guard<std::mutex> lock(engine->timingMutex);
            const double total = mapMs + convertMs;
            auto blend = [&](double &field)
            { field = field * 0.9 + total * 0.1; };
            blend(engine->timingMapMs);
            blend(engine->timingEncodeMs);
            engine->timingRenderMs = engine->timingRenderMs * 0.9 + (convertMs * 0.7) * 0.1;
            engine->timingSubmitMs = engine->timingSubmitMs * 0.9 + (convertMs * 0.3) * 0.1;
            engine->timingSnapshotMs = engine->timingSnapshotMs * 0.9 + mapMs * 0.1;
            engine->timingUnmapMs = engine->timingUnmapMs * 0.9 + mapMs * 0.1;
            engine->timingFrames++;
        }

        if (!ctx.paused.load(std::memory_order_relaxed))
            AppendVideo(engine, std::move(frame));
    }

    void RunWgcCapture(CaptureContext &ctx)
    {
        Engine *engine = ctx.engine;
        winrt::init_apartment(winrt::apartment_type::multi_threaded);

        HRESULT hr = CreateD3D11Device(ctx.device, ctx.context, &engine->hwInitHr);
        if (FAILED(hr))
        {
            engine->lastHresult = hr;
            engine->lastFailHr = hr;
            engine->lastFailStage = IR_FAIL_DEVICE;
            engine->hasError.store(true);
            SetIrError("D3D11 device creation failed: 0x%08lX", hr);
            return;
        }

        try
        {
            ComPtr<IDXGIDevice> dxgiDevice;
            hr = ctx.device.As(&dxgiDevice);
            if (FAILED(hr))
                throw winrt::hresult_error(hr, L"IDXGIDevice query failed");

            ComPtr<IInspectable> inspectableDevice;
            hr = CreateDirect3D11DeviceFromDXGIDevice(dxgiDevice.Get(), inspectableDevice.GetAddressOf());
            if (FAILED(hr))
                throw winrt::hresult_error(hr, L"CreateDirect3D11DeviceFromDXGIDevice failed");
            winrt::Windows::Foundation::IInspectable deviceInspectable;
            winrt::attach_abi(deviceInspectable, inspectableDevice.Detach());
            auto graphicsDevice = deviceInspectable.as<
                winrt::Windows::Graphics::DirectX::Direct3D11::IDirect3DDevice>();

            auto interop = winrt::get_activation_factory<
                winrt::Windows::Graphics::Capture::GraphicsCaptureItem, IGraphicsCaptureItemInterop>();
            if (!interop)
                throw winrt::hresult_error(E_NOINTERFACE, L"IGraphicsCaptureItemInterop unavailable");

            if (ctx.config.window)
            {
                hr = interop->CreateForWindow(reinterpret_cast<HWND>(ctx.config.window),
                                              winrt::guid_of<winrt::Windows::Graphics::Capture::GraphicsCaptureItem>(),
                                              winrt::put_abi(ctx.item));
            }
            else
            {
                HMONITOR monitor = FindMonitor(ctx.config.monitorIndex);
                if (!monitor)
                    throw winrt::hresult_error(HRESULT_FROM_WIN32(ERROR_NOT_FOUND), L"No display monitor");
                hr = interop->CreateForMonitor(monitor,
                                               winrt::guid_of<winrt::Windows::Graphics::Capture::GraphicsCaptureItem>(),
                                               winrt::put_abi(ctx.item));
            }
            if (FAILED(hr))
                throw winrt::hresult_error(hr, L"GraphicsCaptureItem creation failed");

            const auto size = ctx.item.Size();
            winrt::Windows::Graphics::SizeInt32 poolSize{size.Width > 0 ? size.Width : 1920,
                                                         size.Height > 0 ? size.Height : 1080};
            ctx.pool = winrt::Windows::Graphics::Capture::Direct3D11CaptureFramePool::CreateFreeThreaded(
                graphicsDevice,
                winrt::Windows::Graphics::DirectX::DirectXPixelFormat::B8G8R8A8UIntNormalized,
                3, poolSize);
            ctx.session = ctx.pool.CreateCaptureSession(ctx.item);

            try
            {
                auto session2 = ctx.session.as<winrt::Windows::Graphics::Capture::IGraphicsCaptureSession2>();
                session2.IsCursorCaptureEnabled((ctx.config.options & IR_CAP_CURSOR) != 0);
                auto session3 = ctx.session.as<winrt::Windows::Graphics::Capture::IGraphicsCaptureSession3>();
                session3.IsBorderRequired((ctx.config.options & IR_CAP_BORDER) != 0);
            }
            catch (...)
            {
            }

            ctx.session.StartCapture();
            engine->lastCaptureHr = S_OK;
        }
        catch (const winrt::hresult_error &error)
        {
            engine->lastHresult = error.code().value;
            engine->lastCaptureHr = error.code().value;
            engine->lastFailHr = error.code().value;
            engine->lastFailStage = IR_FAIL_CAPTURE;
            engine->hasError.store(true);
            SetIrError("WGC start failed: 0x%08X %S", static_cast<unsigned>(error.code().value), error.message().c_str());
            winrt::uninit_apartment();
            return;
        }
        catch (const std::exception &error)
        {
            engine->lastFailStage = IR_FAIL_CAPTURE;
            engine->hasError.store(true);
            SetIrError("WGC start failed: %s", error.what());
            winrt::uninit_apartment();
            return;
        }

        const int64_t intervalMs = std::max<int64_t>(1, 1000 / std::max(1, ctx.config.fps));
        int64_t nextTick = std::chrono::duration_cast<std::chrono::milliseconds>(
                               std::chrono::steady_clock::now().time_since_epoch())
                               .count();

        while (!ctx.stop.load(std::memory_order_relaxed))
        {
            if (ctx.paused.load(std::memory_order_relaxed))
            {
                Sleep(10);
                nextTick = std::chrono::duration_cast<std::chrono::milliseconds>(
                               std::chrono::steady_clock::now().time_since_epoch())
                               .count();
                continue;
            }

            try
            {
                auto frame = ctx.pool.TryGetNextFrame();
                if (frame)
                {
                    try
                    {
                        auto surface = frame.Surface();
                        auto *surfaceUnknown = reinterpret_cast<IUnknown *>(winrt::get_abi(surface));
                        ComPtr<IDxgiInterfaceAccess> access;
                        hr = surfaceUnknown->QueryInterface(IID_IDxgiInterfaceAccessLocal,
                                                            reinterpret_cast<void **>(access.GetAddressOf()));
                        if (SUCCEEDED(hr))
                        {
                            ComPtr<ID3D11Texture2D> texture;
                            hr = access->GetInterface(IID_ID3D11Texture2D,
                                                      reinterpret_cast<void **>(texture.GetAddressOf()));
                            if (SUCCEEDED(hr))
                            {
                                const int64_t tick = frame.SystemRelativeTime().count();
                                ProcessTexture(ctx, texture.Get(), tick);
                            }
                        }
                    }
                    catch (const winrt::hresult_error &error)
                    {
                        engine->lastHresult = error.code().value;
                    }
                }
                else
                {
                    Sleep(1);
                }
            }
            catch (const winrt::hresult_error &error)
            {
                engine->lastHresult = error.code().value;
                if (error.code().value == DXGI_ERROR_DEVICE_REMOVED || error.code().value == DXGI_ERROR_DEVICE_RESET)
                {
                    engine->lastFailStage = IR_FAIL_FRAME;
                    engine->hasError.store(true);
                    break;
                }
            }
            catch (...)
            {
                engine->hasError.store(true);
            }

            const int64_t now = std::chrono::duration_cast<std::chrono::milliseconds>(
                                    std::chrono::steady_clock::now().time_since_epoch())
                                    .count();
            nextTick += intervalMs;
            if (nextTick < now)
                nextTick = now;
            const int64_t delay = nextTick - now;
            if (delay > 0)
                Sleep(static_cast<DWORD>(std::min<int64_t>(delay, 50)));

            if ((engine->heartbeat.fetch_add(1, std::memory_order_relaxed) & 63) == 0)
                engine->heartbeat.fetch_add(1, std::memory_order_relaxed);
        }

        try
        {
            ctx.session.Close();
        }
        catch (...)
        {
        }
        ctx.session = nullptr;
        ctx.pool = nullptr;
        ctx.item = nullptr;
        ctx.staging.Reset();
        ctx.context.Reset();
        ctx.device.Reset();
        winrt::uninit_apartment();
    }

    void RunDxgiCapture(CaptureContext &ctx)
    {
        Engine *engine = ctx.engine;
        HRESULT hr = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
        const bool comInitialized = (hr == S_OK);

        hr = CreateD3D11Device(ctx.device, ctx.context, &engine->hwInitHr);
        if (FAILED(hr))
        {
            engine->lastHresult = hr;
            engine->lastFailHr = hr;
            engine->lastFailStage = IR_FAIL_DEVICE;
            engine->hasError.store(true);
            SetIrError("D3D11 device creation failed: 0x%08lX", hr);
            if (comInitialized)
                CoUninitialize();
            return;
        }

        ComPtr<IDXGIDevice> dxgiDevice;
        ComPtr<IDXGIAdapter> adapter;
        ComPtr<IDXGIOutput> output;
        bool started = false;

        auto createDuplication = [&]() -> HRESULT
        {
            hr = ctx.device.As(&dxgiDevice);
            if (FAILED(hr))
                return hr;
            hr = dxgiDevice->GetAdapter(adapter.GetAddressOf());
            if (FAILED(hr))
                return hr;
            auto monitors = EnumerateMonitors();
            unsigned outputIndex = 0;
            if (!monitors.empty())
            {
                for (unsigned i = 0; i < 16; ++i)
                {
                    ComPtr<IDXGIOutput> candidate;
                    if (FAILED(adapter->EnumOutputs(i, candidate.GetAddressOf())))
                        break;
                    DXGI_OUTPUT_DESC desc{};
                    if (FAILED(candidate->GetDesc(&desc)))
                        continue;
                    if (desc.Monitor == monitors[std::min<size_t>(ctx.config.monitorIndex >= 0
                                                                      ? static_cast<size_t>(ctx.config.monitorIndex)
                                                                      : 0,
                                                                  monitors.size() - 1)]
                                            .first)
                    {
                        output = candidate;
                        outputIndex = i;
                        break;
                    }
                }
            }
            if (!output)
            {
                hr = adapter->EnumOutputs(0, output.GetAddressOf());
                outputIndex = 0;
                if (FAILED(hr))
                    return hr;
            }
            (void)outputIndex;
            ComPtr<IDXGIOutput1> output1;
            hr = output.As(&output1);
            if (FAILED(hr))
                return hr;
            ctx.duplication.Reset();
            hr = output1->DuplicateOutput(ctx.device.Get(), ctx.duplication.GetAddressOf());
            return hr;
        };

        hr = createDuplication();
        if (FAILED(hr))
        {
            engine->lastHresult = hr;
            engine->lastCaptureHr = hr;
            engine->lastFailHr = hr;
            engine->lastFailStage = IR_FAIL_CAPTURE;
            engine->hasError.store(true);
            SetIrError("DXGI DuplicateOutput failed: 0x%08lX", hr);
            if (comInitialized)
                CoUninitialize();
            return;
        }
        started = true;
        engine->lastCaptureHr = S_OK;

        const int64_t intervalMs = std::max<int64_t>(1, 1000 / std::max(1, ctx.config.fps));
        DWORD timeout = static_cast<DWORD>(std::min<int64_t>(100, intervalMs + 8));

        while (!ctx.stop.load(std::memory_order_relaxed) && started)
        {
            if (ctx.paused.load(std::memory_order_relaxed))
            {
                Sleep(10);
                continue;
            }

            ComPtr<IDXGIResource> resource;
            DXGI_OUTDUPL_FRAME_INFO info{};
            hr = ctx.duplication->AcquireNextFrame(timeout, &info, resource.GetAddressOf());
            if (hr == DXGI_ERROR_WAIT_TIMEOUT)
                continue;
            if (hr == DXGI_ERROR_ACCESS_LOST || hr == DXGI_ERROR_DEVICE_REMOVED || hr == DXGI_ERROR_DEVICE_RESET)
            {
                engine->lastHresult = hr;
                engine->lastCaptureHr = hr;
                hr = createDuplication();
                if (FAILED(hr))
                {
                    engine->hasError.store(true);
                    engine->lastFailStage = IR_FAIL_FRAME;
                    break;
                }
                continue;
            }
            if (FAILED(hr))
            {
                engine->lastHresult = hr;
                engine->lastCaptureHr = hr;
                Sleep(5);
                continue;
            }

            ComPtr<ID3D11Texture2D> texture;
            if (SUCCEEDED(resource.As(&texture)))
            {
                const int64_t tick = info.LastPresentTime.QuadPart > 0 ? info.LastPresentTime.QuadPart : -1;
                ProcessTexture(ctx, texture.Get(), tick);
            }
            ctx.duplication->ReleaseFrame();
            engine->heartbeat.fetch_add(1, std::memory_order_relaxed);
        }

        ctx.duplication.Reset();
        ctx.staging.Reset();
        ctx.context.Reset();
        ctx.device.Reset();
        if (comInitialized)
            CoUninitialize();
    }

    void CaptureThreadMain(std::shared_ptr<CaptureContext> context)
    {
        if (!context)
            return;
        if (context->config.method == IR_CAPTURE_DXGI)
            RunDxgiCapture(*context);
        else
            RunWgcCapture(*context);
    }

    void StopCaptureThread(Engine &engine)
    {
        if (engine.capture)
            engine.capture->stop.store(true);
        if (engine.captureThread.joinable())
            engine.captureThread.join();
        engine.captureThread = {};
        engine.capture.reset();
    }

    void StartCaptureThread(Engine &engine)
    {
        StopCaptureThread(engine);
        auto context = std::make_shared<CaptureContext>();
        context->engine = &engine;
        context->config = engine.config;
        context->paused.store(engine.paused);
        engine.capture = context;
        engine.captureThread = std::thread(CaptureThreadMain, context);
    }


    template <typename T>
    void SetAttribute(IMFAttributes *attributes, const GUID &key, T value)
    {
        if constexpr (std::is_same_v<T, uint32_t>)
            attributes->SetUINT32(key, static_cast<UINT32>(value));
        else if constexpr (std::is_same_v<T, uint64_t>)
            attributes->SetUINT64(key, static_cast<UINT64>(value));
        else if constexpr (std::is_same_v<T, GUID>)
            attributes->SetGUID(key, value);
    }

    ComPtr<IMFSample> MakeSample(const void *data, size_t bytes, int64_t time100ns, int64_t duration100ns)
    {
        ComPtr<IMFSample> sample;
        if (FAILED(MFCreateSample(sample.GetAddressOf())))
            return {};
        ComPtr<IMFMediaBuffer> buffer;
        if (FAILED(MFCreateMemoryBuffer(static_cast<DWORD>(std::max<size_t>(1, bytes)), buffer.GetAddressOf())))
            return {};
        BYTE *target = nullptr;
        DWORD maximum = 0;
        DWORD current = 0;
        if (FAILED(buffer->Lock(&target, &maximum, &current)))
            return {};
        if (bytes)
            std::memcpy(target, data, bytes);
        buffer->Unlock();
        buffer->SetCurrentLength(static_cast<DWORD>(bytes));
        sample->AddBuffer(buffer.Get());
        sample->SetSampleTime(time100ns);
        sample->SetSampleDuration(duration100ns);
        return sample;
    }

    ComPtr<IMFActivate> FindEncoderActivate(int codec, int vendorIndex);

    GUID OutputSubtypeForCodec(int codec)
    {
        if (codec == 1)
            return MFVideoFormat_HEVC;
        if (codec == 3)
            return MFVideoFormat_AV1;
        return MFVideoFormat_H264;
    }

    std::vector<uint8_t> LetterboxFrame(const VideoFrame &frame, int baseW, int baseH)
    {
        std::vector<uint8_t> canvas(static_cast<size_t>(baseW) * baseH * 3 / 2);
        const size_t lumaBytes = static_cast<size_t>(baseW) * baseH;
        std::fill(canvas.begin(), canvas.begin() + lumaBytes, static_cast<uint8_t>(16));
        std::fill(canvas.begin() + lumaBytes, canvas.end(), static_cast<uint8_t>(128));

        if (frame.nv12.empty() || frame.width <= 0 || frame.height <= 0 ||
            frame.width > baseW || frame.height > baseH)
            return canvas;

        int x = (baseW - frame.width) / 2;
        int y = (baseH - frame.height) / 2;
        x &= ~1;
        y &= ~1;

        const uint8_t *src = frame.nv12.data();
        uint8_t *dstY = canvas.data() + static_cast<size_t>(y) * baseW + x;
        for (int row = 0; row < frame.height; ++row)
        {
            std::memcpy(dstY + static_cast<size_t>(row) * baseW,
                        src + static_cast<size_t>(row) * frame.width,
                        static_cast<size_t>(frame.width));
        }

        const uint8_t *srcUv = src + static_cast<size_t>(frame.width) * frame.height;
        uint8_t *dstUv = canvas.data() + lumaBytes + static_cast<size_t>(y / 2) * baseW + x;
        for (int row = 0; row < frame.height / 2; ++row)
        {
            std::memcpy(dstUv + static_cast<size_t>(row) * baseW,
                        srcUv + static_cast<size_t>(row) * frame.width,
                        static_cast<size_t>(frame.width));
        }
        return canvas;
    }

    HRESULT WriteMp4(const std::wstring &path,
                     const std::vector<VideoFrame> &video,
                     const std::vector<AudioBlock> &audio,
                     int fps,
                     int bitrateKbps,
                     int codec,
                     int vendorIndex,
                     std::atomic<int> &progress)
    {
        std::call_once(g_mfOnce, []
                       { MFStartup(MF_VERSION, MFSTARTUP_LITE); });

        ComPtr<IMFAttributes> sinkAttributes;
        if (vendorIndex >= 0 && vendorIndex < 4)
        {
            ComPtr<IMFActivate> encoder = FindEncoderActivate(codec, vendorIndex);
            if (encoder)
            {
                if (SUCCEEDED(MFCreateAttributes(sinkAttributes.GetAddressOf(), 1)))
                    sinkAttributes->SetUnknown(MF_SINK_WRITER_ENCODER_CONFIG, encoder.Get());
                else
                    sinkAttributes.Reset();
            }
        }

        ComPtr<IMFSinkWriter> writer;
        HRESULT hr = MFCreateSinkWriterFromURL(path.c_str(), nullptr, sinkAttributes.Get(),
                                               writer.GetAddressOf());
        if (FAILED(hr) && sinkAttributes)
        {
            sinkAttributes.Reset();
            hr = MFCreateSinkWriterFromURL(path.c_str(), nullptr, nullptr, writer.GetAddressOf());
        }
        if (FAILED(hr))
        {
            SetIrError("MFCreateSinkWriterFromURL failed: 0x%08X", static_cast<unsigned>(hr));
            return hr;
        }

        int baseW = 2;
        int baseH = 2;
        for (const auto &frame : video)
        {
            baseW = std::max(baseW, frame.width);
            baseH = std::max(baseH, frame.height);
        }
        baseW &= ~1;
        baseH &= ~1;
        const UINT32 frameRate = static_cast<UINT32>(std::max(1, fps));
        const int64_t videoDuration = 10000000ll / frameRate;

        ComPtr<IMFMediaType> inputVideo;
        MFCreateMediaType(inputVideo.GetAddressOf());
        SetAttribute(inputVideo.Get(), MF_MT_MAJOR_TYPE, MFMediaType_Video);
        SetAttribute(inputVideo.Get(), MF_MT_SUBTYPE, MFVideoFormat_NV12);
        SetAttribute(inputVideo.Get(), MF_MT_AVG_BITRATE, static_cast<uint32_t>(std::max(100000, bitrateKbps * 1000)));
        SetAttribute(inputVideo.Get(), MF_MT_INTERLACE_MODE, static_cast<uint32_t>(MFVideoInterlace_Progressive));
        MFSetAttributeSize(inputVideo.Get(), MF_MT_FRAME_SIZE, baseW, baseH);
        MFSetAttributeRatio(inputVideo.Get(), MF_MT_FRAME_RATE, frameRate, 1);
        MFSetAttributeRatio(inputVideo.Get(), MF_MT_PIXEL_ASPECT_RATIO, 1, 1);
        SetAttribute(inputVideo.Get(), MF_MT_YUV_MATRIX, static_cast<uint32_t>(baseH >= 720 ? MFVideoTransferMatrix_BT709 : MFVideoTransferMatrix_BT601));
        SetAttribute(inputVideo.Get(), MF_MT_VIDEO_NOMINAL_RANGE, static_cast<uint32_t>(MFNominalRange_16_235));

        ComPtr<IMFMediaType> outputVideo;
        MFCreateMediaType(outputVideo.GetAddressOf());
        SetAttribute(outputVideo.Get(), MF_MT_MAJOR_TYPE, MFMediaType_Video);
        SetAttribute(outputVideo.Get(), MF_MT_SUBTYPE, OutputSubtypeForCodec(codec));
        SetAttribute(outputVideo.Get(), MF_MT_AVG_BITRATE, static_cast<uint32_t>(std::max(100000, bitrateKbps * 1000)));
        SetAttribute(outputVideo.Get(), MF_MT_INTERLACE_MODE, static_cast<uint32_t>(MFVideoInterlace_Progressive));
        MFSetAttributeSize(outputVideo.Get(), MF_MT_FRAME_SIZE, baseW, baseH);
        MFSetAttributeRatio(outputVideo.Get(), MF_MT_FRAME_RATE, frameRate, 1);
        MFSetAttributeRatio(outputVideo.Get(), MF_MT_PIXEL_ASPECT_RATIO, 1, 1);

        DWORD videoStream = 0;
        hr = writer->AddStream(outputVideo.Get(), &videoStream);
        if (FAILED(hr))
        {
            SetIrError("AddStream(video) failed: 0x%08X", static_cast<unsigned>(hr));
            return hr;
        }
        hr = writer->SetInputMediaType(videoStream, inputVideo.Get(), nullptr);
        if (FAILED(hr))
        {
            SetIrError("SetInputMediaType(video) failed: 0x%08X", static_cast<unsigned>(hr));
            return hr;
        }

        DWORD audioStream = 0xFFFFFFFF;
        if (!audio.empty())
        {
            ComPtr<IMFMediaType> inputAudio;
            MFCreateMediaType(inputAudio.GetAddressOf());
            SetAttribute(inputAudio.Get(), MF_MT_MAJOR_TYPE, MFMediaType_Audio);
            SetAttribute(inputAudio.Get(), MF_MT_SUBTYPE, MFAudioFormat_PCM);
            SetAttribute(inputAudio.Get(), MF_MT_AUDIO_NUM_CHANNELS, 2u);
            SetAttribute(inputAudio.Get(), MF_MT_AUDIO_SAMPLES_PER_SECOND, 48000u);
            SetAttribute(inputAudio.Get(), MF_MT_AUDIO_BITS_PER_SAMPLE, 16u);
            SetAttribute(inputAudio.Get(), MF_MT_AUDIO_BLOCK_ALIGNMENT, 4u);
            SetAttribute(inputAudio.Get(), MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 192000u);
            SetAttribute(inputAudio.Get(), MF_MT_ALL_SAMPLES_INDEPENDENT, 1u);

            ComPtr<IMFMediaType> outputAudio;
            MFCreateMediaType(outputAudio.GetAddressOf());
            SetAttribute(outputAudio.Get(), MF_MT_MAJOR_TYPE, MFMediaType_Audio);
            SetAttribute(outputAudio.Get(), MF_MT_SUBTYPE, MFAudioFormat_AAC);
            SetAttribute(outputAudio.Get(), MF_MT_AUDIO_NUM_CHANNELS, 2u);
            SetAttribute(outputAudio.Get(), MF_MT_AUDIO_SAMPLES_PER_SECOND, 48000u);
            SetAttribute(outputAudio.Get(), MF_MT_AUDIO_BITS_PER_SAMPLE, 16u);
            SetAttribute(outputAudio.Get(), MF_MT_AUDIO_BLOCK_ALIGNMENT, 4u);
            SetAttribute(outputAudio.Get(), MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 24000u);

            hr = writer->AddStream(outputAudio.Get(), &audioStream);
            if (FAILED(hr))
            {
                SetIrError("AddStream(audio) failed: 0x%08lX", hr);
                return hr;
            }
            hr = writer->SetInputMediaType(audioStream, inputAudio.Get(), nullptr);
            if (FAILED(hr))
            {
                SetIrError("SetInputMediaType(audio) failed: 0x%08lX", hr);
                return hr;
            }
        }

        hr = writer->BeginWriting();
        if (FAILED(hr))
        {
            SetIrError("BeginWriting failed: 0x%08X", static_cast<unsigned>(hr));
            return hr;
        }
        progress.store(20, std::memory_order_relaxed);

        const int64_t base = video.front().pts100ns;
        std::vector<uint8_t> canvas;
        for (size_t i = 0; i < video.size(); ++i)
        {
            const VideoFrame &frame = video[i];
            const uint8_t *pixels = frame.nv12.data();
            size_t bytes = frame.nv12.size();
            if (frame.width != baseW || frame.height != baseH)
            {
                canvas = LetterboxFrame(frame, baseW, baseH);
                pixels = canvas.data();
                bytes = canvas.size();
            }
            auto sample = MakeSample(pixels, bytes,
                                     std::max<int64_t>(0, frame.pts100ns - base), videoDuration);
            if (!sample)
            {
                SetIrError("Failed to allocate video sample %llu", static_cast<unsigned long long>(i));
                return E_OUTOFMEMORY;
            }
            hr = writer->WriteSample(videoStream, sample.Get());
            if (FAILED(hr))
            {
                SetIrError("WriteSample(video) failed: 0x%08X", static_cast<unsigned>(hr));
                return hr;
            }
            if ((i & 31) == 0)
                progress.store(20 + static_cast<int>(60 * (i + 1) / video.size()), std::memory_order_relaxed);
        }

        for (size_t i = 0; i < audio.size(); ++i)
        {
            const AudioBlock &block = audio[i];
            const int64_t duration = static_cast<int64_t>(block.pcm.size() / 2) * 10000000ll / 48000ll;
            auto sample = MakeSample(block.pcm.data(), block.pcm.size() * sizeof(int16_t),
                                     std::max<int64_t>(0, block.pts100ns - base), duration);
            if (!sample)
                continue;
            hr = writer->WriteSample(audioStream, sample.Get());
            if (FAILED(hr))
            {
                SetIrError("WriteSample(audio) failed: 0x%08lX", hr);
                return hr;
            }
        }

        progress.store(92, std::memory_order_relaxed);
        hr = writer->Finalize();
        if (FAILED(hr))
        {
            SetIrError("SinkWriter::Finalize failed: 0x%08X", static_cast<unsigned>(hr));
            return hr;
        }
        progress.store(100, std::memory_order_relaxed);
        return S_OK;
    }

    void SaveWorker(std::shared_ptr<SaveJob> job,
                    std::wstring path,
                    std::vector<VideoFrame> video,
                    std::vector<AudioBlock> audio,
                    int fps,
                    int bitrate,
                    int codec,
                    int vendorIndex)
    {
        HRESULT hr = WriteMp4(path, video, audio, fps, bitrate, codec, vendorIndex, job->progress);
        if (FAILED(hr) && vendorIndex >= 0 && vendorIndex < 4)
        {
            ClearIrError();
            hr = WriteMp4(path, video, audio, fps, bitrate, codec, 4, job->progress);
        }
        job->result.store(SUCCEEDED(hr) ? IR_OK : IR_FAIL, std::memory_order_release);
        g_saveProgress.store(job->progress.load(std::memory_order_relaxed), std::memory_order_relaxed);
    }


    uint32_t EncoderBit(int codec, int vendor)
    {
        static const uint32_t bits[3][5] = {
            {IR_ENC_H264_QSV_BIT, IR_ENC_H264_VP12_BIT, IR_ENC_H264_NVENC_BIT, IR_ENC_H264_AMF_BIT, IR_ENC_H264_SOFTWARE_BIT},
            {IR_ENC_HEVC_QSV_BIT, IR_ENC_HEVC_VP12_BIT, IR_ENC_HEVC_NVENC_BIT, IR_ENC_HEVC_AMF_BIT, IR_ENC_HEVC_SOFTWARE_BIT},
            {IR_ENC_AV1_QSV_BIT, IR_ENC_AV1_VP12_BIT, IR_ENC_AV1_NVENC_BIT, IR_ENC_AV1_AMF_BIT, IR_ENC_AV1_SOFTWARE_BIT}};
        const int c = std::clamp(codec, 0, 2);
        const int v = std::clamp(vendor, 0, 4);
        return bits[c][v];
    }

    int VendorIndexFromName(const std::wstring &name)
    {
        if (name.find(L"NVIDIA") != std::wstring::npos || name.find(L"NVENC") != std::wstring::npos)
            return 2;
        if (name.find(L"AMD") != std::wstring::npos || name.find(L"AMF") != std::wstring::npos)
            return 3;
        if (name.find(L"Intel") != std::wstring::npos || name.find(L"QSV") != std::wstring::npos)
        {
            if (name.find(L"VPL") != std::wstring::npos || name.find(L"Gen12") != std::wstring::npos ||
                name.find(L"Arc") != std::wstring::npos)
                return 1;
            return 0;
        }
        return 4;
    }

    std::wstring ReadMftFriendlyName(IMFActivate *activation)
    {
        UINT32 length = 0;
        activation->GetString(MFT_FRIENDLY_NAME_Attribute, nullptr, 0, &length);
        if (!length)
            return {};

        std::vector<wchar_t> buffer(static_cast<size_t>(length) + 1, 0);
        UINT32 written = 0;
        const HRESULT hr = activation->GetString(
            MFT_FRIENDLY_NAME_Attribute, buffer.data(), static_cast<UINT32>(buffer.size()), &written);
        if (FAILED(hr))
            return {};
        return std::wstring(buffer.data(), std::min<UINT32>(written, length));
    }

    uint32_t ProbeOneSubtype(const GUID &subtype, int codec)
    {
        MFT_REGISTER_TYPE_INFO output{};
        output.guidMajorType = MFMediaType_Video;
        output.guidSubtype = subtype;
        IMFActivate **activations = nullptr;
        UINT32 count = 0;
        HRESULT hr = MFTEnumEx(MFT_CATEGORY_VIDEO_ENCODER,
                               MFT_ENUM_FLAG_SYNCMFT | MFT_ENUM_FLAG_ASYNCMFT |
                                   MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER,
                               nullptr, &output, &activations, &count);
        if (FAILED(hr))
            return 0;

        uint32_t mask = 0;
        for (UINT32 i = 0; i < count; ++i)
        {
            const std::wstring name = ReadMftFriendlyName(activations[i]);
            const int vendor = VendorIndexFromName(name);
            mask |= EncoderBit(codec, vendor);
            activations[i]->Release();
        }
        CoTaskMemFree(activations);
        return mask;
    }

    ComPtr<IMFActivate> FindEncoderActivate(int codec, int vendorIndex)
    {
        MFT_REGISTER_TYPE_INFO output{};
        output.guidMajorType = MFMediaType_Video;
        output.guidSubtype = OutputSubtypeForCodec(codec);

        const UINT32 flags = vendorIndex >= 0 && vendorIndex < 4
                                 ? MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SYNCMFT | MFT_ENUM_FLAG_ASYNCMFT |
                                       MFT_ENUM_FLAG_SORTANDFILTER
                                 : MFT_ENUM_FLAG_SYNCMFT | MFT_ENUM_FLAG_ASYNCMFT | MFT_ENUM_FLAG_SORTANDFILTER;

        IMFActivate **activations = nullptr;
        UINT32 count = 0;
        if (FAILED(MFTEnumEx(MFT_CATEGORY_VIDEO_ENCODER, flags, nullptr, &output,
                             &activations, &count)))
            return {};

        ComPtr<IMFActivate> selected;
        for (UINT32 i = 0; i < count; ++i)
        {
            if (!selected && VendorIndexFromName(ReadMftFriendlyName(activations[i])) == vendorIndex)
            {
                selected.Attach(activations[i]);
                activations[i] = nullptr;
                break;
            }
        }
        for (UINT32 i = 0; i < count; ++i)
        {
            if (activations[i])
                activations[i]->Release();
        }
        CoTaskMemFree(activations);
        return selected;
    }

    int EncoderCodeFromPreference(int preference, uint32_t probe)
    {
        const int codec = ((preference >> 15) & 3);
        const int vendor = preference & 0xFF;
        int index = 4;
        if (vendor == 5)
            index = 0;
        else if (vendor == 6)
            index = 1;
        else if (vendor == 7)
            index = 2;
        else if (vendor == 8)
            index = 3;
        else if (vendor == 0)
        {
            const int codecIndex = codec == 1 ? 1 : codec == 3 ? 2
                                                               : 0;
            if (probe & EncoderBit(codecIndex, 0))
                index = 0;
            else if (probe & EncoderBit(codecIndex, 1))
                index = 1;
            else if (probe & EncoderBit(codecIndex, 2))
                index = 2;
            else if (probe & EncoderBit(codecIndex, 3))
                index = 3;
        }

        if (codec == 1)
            return IR_ENC_HEVC_QSV + index;
        if (codec == 3)
            return IR_ENC_AV1_QSV + index;
        return IR_ENC_H264_QSV + index;
    }

}


IR_API int IR_Create(void **out_engine)
{
    if (!out_engine)
        return IR_FAIL;
    *out_engine = nullptr;
    try
    {
        ClearIrError();
        auto *engine = new (std::nothrow) Engine();
        if (!engine)
        {
            SetIrError("Out of memory creating engine");
            return IR_LOW_MEMORY;
        }
        *out_engine = engine;
        return IR_OK;
    }
    catch (const std::exception &error)
    {
        SetIrError("IR_Create failed: %s", error.what());
        return IR_FAIL;
    }
}

IR_API void IR_Destroy(void *handle)
{
    Engine *engine = FromHandle(handle);
    if (!engine)
        return;
    {
        std::lock_guard<std::mutex> lock(engine->mutex);
        engine->destroyRequested = true;
    }
    engine->systemCapture.reset();
    engine->micCapture.reset();
    StopCaptureThread(*engine);
    {
        std::lock_guard<std::mutex> lock(engine->jobsMutex);
        for (auto &job : engine->jobs)
        {
            if (job->worker.joinable())
                job->worker.join();
        }
        engine->jobs.clear();
    }
    engine->magic = 0;
    delete engine;
}

IR_API int IR_Start(void *handle,
                    int monitor_index,
                    int ring_seconds,
                    int fps,
                    int bitrate_kbps,
                    int cap_w,
                    int cap_h,
                    int capture_audio,
                    int capture_mic,
                    const wchar_t *mic_device,
                    int encoder_preference,
                    int capture_method,
                    int64_t *start_qpc_100ns)
{
    Engine *engine = FromHandle(handle);
    if (!engine)
        return IR_FAIL;
    try
    {
        std::lock_guard<std::mutex> lock(engine->mutex);
        if (engine->running)
            return IR_FAIL;
        ClearIrError();

        if (capture_method != IR_CAPTURE_WGC && capture_method != IR_CAPTURE_DXGI &&
            capture_method != IR_CAPTURE_EXTERNAL)
            return IR_FAIL;

        engine->config.method = capture_method;
        engine->config.monitorIndex = monitor_index;
        engine->config.fps = std::clamp(fps, 1, 240);
        engine->config.ringSeconds = std::max(1, ring_seconds);
        engine->config.capWidth = std::max(0, cap_w);
        engine->config.capHeight = std::max(0, cap_h);
        engine->config.window = nullptr;
        engine->config.options = 0;
        engine->bitrateKbps = std::max(1, bitrate_kbps);
        engine->encoderPreference = encoder_preference;
        engine->encoderProbe = IR_ProbeEncoders();
        engine->activeEncoder = EncoderCodeFromPreference(encoder_preference, engine->encoderProbe);
        engine->captureAudioRequested = capture_audio != 0;
        engine->captureMicRequested = capture_mic != 0;
        engine->micRequested = mic_device ? mic_device : L"";
        engine->maxRingFrames = static_cast<size_t>(std::clamp(
            engine->config.ringSeconds * engine->config.fps, 2, 14400));
        engine->paused = false;
        engine->hasError.store(false);
        engine->lastFailStage = IR_FAIL_NONE;
        engine->lastHresult = 0;
        engine->lastFailHr = 0;
        engine->lastCaptureHr = 0;

        {
            std::lock_guard<std::mutex> ringLock(engine->ringMutex);
            engine->ring.clear();
            engine->ringBytes = 0;
        }
        {
            std::lock_guard<std::mutex> audioLock(engine->audioMutex);
            engine->systemAudio.clear();
            engine->micAudio.clear();
            engine->systemAudioBytes = 0;
            engine->micAudioBytes = 0;
        }

        const int64_t startQpc = Qpc100Ns();
        if (start_qpc_100ns)
            *start_qpc_100ns = startQpc;
        engine->startQpc.store(startQpc, std::memory_order_relaxed);
        engine->running = true;

        StartCaptureThread(*engine);

        if (engine->captureAudioRequested)
        {
            engine->systemCapture = std::make_unique<AudioCapture>(engine, false, std::wstring(), nullptr);
            if (!engine->systemCapture->Start())
            {
                engine->lastFailStage = IR_FAIL_AUDIO;
                engine->systemCapture.reset();
            }
        }
        if (engine->captureMicRequested)
        {
            engine->micCapture = std::make_unique<AudioCapture>(engine, true, engine->micRequested, nullptr);
            if (!engine->micCapture->Start())
            {
                engine->lastFailStage = IR_FAIL_AUDIO;
                engine->micCapture.reset();
            }
        }
        return IR_OK;
    }
    catch (const std::exception &error)
    {
        SetIrError("IR_Start failed: %s", error.what());
        engine->running = false;
        return IR_FAIL;
    }
    catch (...)
    {
        SetIrError("IR_Start failed with an unknown exception");
        engine->running = false;
        return IR_FAIL;
    }
}

IR_API void IR_Stop(void *handle)
{
    Engine *engine = FromHandle(handle);
    if (!engine)
        return;
    std::unique_ptr<AudioCapture> systemCapture;
    std::unique_ptr<AudioCapture> micCapture;
    {
        std::lock_guard<std::mutex> lock(engine->mutex);
        engine->running = false;
        systemCapture = std::move(engine->systemCapture);
        micCapture = std::move(engine->micCapture);
    }
    systemCapture.reset();
    micCapture.reset();
    StopCaptureThread(*engine);
    {
        std::lock_guard<std::mutex> lock(engine->ringMutex);
        engine->ring.clear();
        engine->ringBytes = 0;
    }
    {
        std::lock_guard<std::mutex> lock(engine->audioMutex);
        engine->systemAudio.clear();
        engine->micAudio.clear();
        engine->systemAudioBytes = 0;
        engine->micAudioBytes = 0;
    }
    engine->lastFailStage = IR_FAIL_NONE;
    engine->lastFailHr = 0;
    engine->lastHresult = 0;
    engine->hasError.store(false);
}

IR_API int IR_SetCaptureTarget(void *handle, void *window, uint32_t options)
{
    Engine *engine = FromHandle(handle);
    if (!engine)
        return IR_FAIL;
    std::lock_guard<std::mutex> lock(engine->mutex);
    if (engine->running)
        return IR_FAIL;
    engine->config.window = reinterpret_cast<HWND>(window);
    engine->config.options = options;
    return IR_OK;
}

IR_API int IR_SetCaptureSource(void *handle, int method, int monitor_index, void *window, uint32_t options)
{
    Engine *engine = FromHandle(handle);
    if (!engine)
        return IR_FAIL;
    try
    {
        std::lock_guard<std::mutex> lock(engine->mutex);
        engine->config.method = method;
        engine->config.monitorIndex = monitor_index;
        engine->config.window = reinterpret_cast<HWND>(window);
        engine->config.options = options;
        if (!engine->running)
            return IR_OK;
        StartCaptureThread(*engine);
        return IR_OK;
    }
    catch (...)
    {
        SetIrError("IR_SetCaptureSource failed");
        return IR_FAIL;
    }
}

IR_API int IR_SetCanvasHint(void *handle, int width, int height)
{
    Engine *engine = FromHandle(handle);
    if (!engine)
        return IR_FAIL;
    std::lock_guard<std::mutex> lock(engine->mutex);
    engine->config.capWidth = std::max(0, width);
    engine->config.capHeight = std::max(0, height);
    return IR_OK;
}

IR_API int IR_SetPaused(void *handle, int paused)
{
    Engine *engine = FromHandle(handle);
    if (!engine)
        return IR_FAIL;
    {
        std::lock_guard<std::mutex> lock(engine->mutex);
        engine->paused = paused != 0;
    }
    if (engine->capture)
        engine->capture->paused.store(engine->paused, std::memory_order_relaxed);
    return IR_OK;
}

IR_API int IR_GetWindowCount(void)
{
    std::vector<HWND> windows;
    EnumWindows(WindowEnumProc, reinterpret_cast<LPARAM>(&windows));
    return static_cast<int>(windows.size());
}

IR_API void IR_GetWindowInfo(int index, void **window, wchar_t *title, int title_capacity, int *width, int *height)
{
    if (window)
        *window = nullptr;
    if (width)
        *width = 0;
    if (height)
        *height = 0;
    SafeCopyWchar(title, title_capacity, L"");
    std::vector<HWND> windows;
    EnumWindows(WindowEnumProc, reinterpret_cast<LPARAM>(&windows));
    if (index < 0 || static_cast<size_t>(index) >= windows.size())
        return;
    HWND target = windows[static_cast<size_t>(index)];
    if (window)
        *window = reinterpret_cast<void *>(target);
    const int length = GetWindowTextW(target, title, title_capacity);
    if (length <= 0 && title && title_capacity > 0)
        title[0] = L'\0';
    RECT rect{};
    if (GetWindowRect(target, &rect))
    {
        if (width)
            *width = static_cast<int>(rect.right - rect.left);
        if (height)
            *height = static_cast<int>(rect.bottom - rect.top);
    }
}

IR_API int IR_BeginSave(void *handle, const wchar_t *out_path, int seconds_back, uint64_t *job_id)
{
    Engine *engine = FromHandle(handle);
    if (!engine || !out_path || !job_id)
        return IR_FAIL;
    *job_id = 0;
    if (LowMemory())
    {
        SetIrError("Insufficient committed memory for save");
        return IR_LOW_MEMORY;
    }

    try
    {
        int fps = 30;
        int bitrate = 6000;
        int codec = 2;
        int vendorIndex = 4;
        {
            std::lock_guard<std::mutex> lock(engine->mutex);
            fps = engine->config.fps;
            bitrate = engine->bitrateKbps;
            codec = (engine->encoderPreference >> 15) & 3;
            if (engine->activeEncoder >= 20)
                vendorIndex = std::clamp(engine->activeEncoder % 10, 0, 4);
        }

        std::vector<VideoFrame> video;
        std::vector<AudioBlock> audio;
        const int64_t now = Qpc100Ns();
        const int64_t cutoff = now - static_cast<int64_t>(std::max(1, seconds_back)) * 10000000ll;
        {
            std::lock_guard<std::mutex> lock(engine->ringMutex);
            for (const auto &frame : engine->ring)
            {
                if (frame.pts100ns >= cutoff)
                    video.push_back(frame);
            }
        }
        if (video.empty())
            return IR_NOT_ENOUGH_DATA;
        if (video.size() > kMaxSaveFrames)
            video.erase(video.begin(), video.end() - kMaxSaveFrames);
        {
            std::lock_guard<std::mutex> lock(engine->audioMutex);
            for (const auto &block : engine->systemAudio)
            {
                if (block.pts100ns >= cutoff)
                    audio.push_back(block);
            }
        }

        auto job = std::make_shared<SaveJob>();
        {
            std::lock_guard<std::mutex> lock(engine->jobsMutex);
            job->id = engine->nextJobId++;
            engine->jobs.push_back(job);
        }
        *job_id = job->id;
        g_saveProgress.store(0, std::memory_order_relaxed);
        const std::wstring path(out_path);
        job->worker = std::thread(SaveWorker, job, path, std::move(video), std::move(audio),
                                  fps, bitrate, codec, vendorIndex);
        return IR_OK;
    }
    catch (const std::bad_alloc &)
    {
        SetIrError("Out of memory copying replay frames");
        return IR_LOW_MEMORY;
    }
    catch (const std::exception &error)
    {
        SetIrError("IR_BeginSave failed: %s", error.what());
        return IR_FAIL;
    }
}

IR_API int IR_Save(void *handle, const wchar_t *out_path, int seconds_back)
{
    uint64_t jobId = 0;
    const int rc = IR_BeginSave(handle, out_path, seconds_back, &jobId);
    if (rc != IR_OK)
        return rc;

    for (;;)
    {
        int progress = 0;
        int result = IR_PENDING;
        if (IR_PollSave(handle, jobId, &progress, &result) != IR_OK)
            return IR_FAIL;
        if (result != IR_PENDING)
            return result;
        Sleep(25);
    }
}

IR_API int IR_PollSave(void *handle, uint64_t job_id, int *progress, int *result)
{
    Engine *engine = FromHandle(handle);
    if (!engine)
        return IR_FAIL;
    std::shared_ptr<SaveJob> found;
    {
        std::lock_guard<std::mutex> lock(engine->jobsMutex);
        for (auto it = engine->jobs.begin(); it != engine->jobs.end(); ++it)
        {
            if ((*it)->id == job_id)
            {
                found = *it;
                if (found->result.load(std::memory_order_acquire) != IR_PENDING)
                {
                    if (found->worker.joinable())
                        found->worker.join();
                    engine->jobs.erase(it);
                }
                break;
            }
        }
    }
    if (!found)
        return IR_FAIL;
    if (progress)
        *progress = found->progress.load(std::memory_order_relaxed);
    if (result)
        *result = found->result.load(std::memory_order_relaxed);
    return IR_OK;
}

IR_API int IR_SaveProgress(void)
{
    return g_saveProgress.load(std::memory_order_relaxed);
}

IR_API void IR_GetStatus(void *handle,
                         int *is_buffering,
                         int *ring_seconds,
                         int64_t *frames_encoded,
                         int64_t *frames_dropped,
                         int *has_error,
                         int64_t *audio_frames,
                         int64_t *audio_events,
                         int64_t *audio_got_frames,
                         int64_t *health_heartbeat)
{
    Engine *engine = FromHandle(handle);
    if (!engine)
    {
        if (is_buffering)
            *is_buffering = 0;
        if (ring_seconds)
            *ring_seconds = 0;
        if (frames_encoded)
            *frames_encoded = 0;
        if (frames_dropped)
            *frames_dropped = 0;
        if (has_error)
            *has_error = 1;
        if (audio_frames)
            *audio_frames = 0;
        if (audio_events)
            *audio_events = 0;
        if (audio_got_frames)
            *audio_got_frames = 0;
        if (health_heartbeat)
            *health_heartbeat = 0;
        return;
    }
    int seconds = 0;
    {
        std::lock_guard<std::mutex> lock(engine->ringMutex);
        if (engine->ring.size() >= 2)
        {
            const int64_t span = engine->ring.back().pts100ns - engine->ring.front().pts100ns;
            seconds = static_cast<int>(std::max<int64_t>(0, span / 10000000ll));
        }
    }
    if (is_buffering)
        *is_buffering = engine->running ? 1 : 0;
    if (ring_seconds)
        *ring_seconds = seconds;
    if (frames_encoded)
        *frames_encoded = engine->encoded.load(std::memory_order_relaxed);
    if (frames_dropped)
        *frames_dropped = engine->dropped.load(std::memory_order_relaxed);
    if (has_error)
        *has_error = engine->hasError.load(std::memory_order_relaxed) ? 1 : 0;
    if (audio_frames)
        *audio_frames = engine->audioFrames.load(std::memory_order_relaxed);
    if (audio_events)
        *audio_events = engine->audioEvents.load(std::memory_order_relaxed);
    if (audio_got_frames)
        *audio_got_frames = engine->audioGot.load(std::memory_order_relaxed);
    if (health_heartbeat)
        *health_heartbeat = engine->heartbeat.load(std::memory_order_relaxed);
}

IR_API void IR_GetTelemetry(void *handle,
                            int64_t *captured,
                            int64_t *paced,
                            int64_t *rejected,
                            int64_t *encoded,
                            int64_t *dropped,
                            int *encoder,
                            int64_t *last_pts)
{
    Engine *engine = FromHandle(handle);
    if (!engine)
        return;
    if (captured)
        *captured = engine->captured.load(std::memory_order_relaxed);
    if (paced)
        *paced = engine->paced.load(std::memory_order_relaxed);
    if (rejected)
        *rejected = engine->rejected.load(std::memory_order_relaxed);
    if (encoded)
        *encoded = engine->encoded.load(std::memory_order_relaxed);
    if (dropped)
        *dropped = engine->dropped.load(std::memory_order_relaxed);
    if (encoder)
        *encoder = engine->activeEncoder;
    if (last_pts)
        *last_pts = engine->lastPts.load(std::memory_order_relaxed);
}

IR_API void IR_GetTiming(void *handle,
                         double *map_ms,
                         double *encode_ms,
                         double *render_ms,
                         double *submit_ms,
                         double *snapshot_ms,
                         double *unmap_ms,
                         int64_t *frames)
{
    Engine *engine = FromHandle(handle);
    if (!engine)
        return;
    std::lock_guard<std::mutex> lock(engine->timingMutex);
    if (map_ms)
        *map_ms = engine->timingMapMs;
    if (encode_ms)
        *encode_ms = engine->timingEncodeMs;
    if (render_ms)
        *render_ms = engine->timingRenderMs;
    if (submit_ms)
        *submit_ms = engine->timingSubmitMs;
    if (snapshot_ms)
        *snapshot_ms = engine->timingSnapshotMs;
    if (unmap_ms)
        *unmap_ms = engine->timingUnmapMs;
    if (frames)
        *frames = engine->timingFrames;
}

IR_API void IR_AudioDiagnostics(void *handle,
                                int which,
                                int64_t *activate_hr,
                                int64_t *init_hr,
                                int64_t *start_hr,
                                int *tag,
                                int *rate,
                                int *channels,
                                int *bits,
                                int *is_float,
                                int *convert_failures,
                                int64_t *last_fail_hr)
{
    Engine *engine = FromHandle(handle);
    if (!engine)
        return;
    std::lock_guard<std::mutex> lock(engine->mutex);
    AudioDiag *diag = which == 1
                          ? (engine->micCapture ? &engine->micCapture->Diag() : &engine->micDiag)
                          : (engine->systemCapture ? &engine->systemCapture->Diag() : &engine->systemDiag);
    if (activate_hr)
        *activate_hr = diag->activateHr.load(std::memory_order_relaxed);
    if (init_hr)
        *init_hr = diag->initHr.load(std::memory_order_relaxed);
    if (start_hr)
        *start_hr = diag->startHr.load(std::memory_order_relaxed);
    if (tag)
        *tag = diag->tag.load(std::memory_order_relaxed);
    if (rate)
        *rate = diag->rate.load(std::memory_order_relaxed);
    if (channels)
        *channels = diag->channels.load(std::memory_order_relaxed);
    if (bits)
        *bits = diag->bits.load(std::memory_order_relaxed);
    if (is_float)
        *is_float = diag->isFloat.load(std::memory_order_relaxed);
    if (convert_failures)
        *convert_failures = diag->convertFailures.load(std::memory_order_relaxed);
    if (last_fail_hr)
        *last_fail_hr = diag->lastFailHr.load(std::memory_order_relaxed);
}

IR_API int IR_GetMonitorCount(void)
{
    return static_cast<int>(EnumerateMonitors().size());
}

IR_API void IR_GetMonitorInfo(int index, wchar_t *name, int name_capacity, int *width, int *height)
{
    SafeCopyWchar(name, name_capacity, L"");
    if (width)
        *width = 0;
    if (height)
        *height = 0;
    auto monitors = EnumerateMonitors();
    if (index < 0 || static_cast<size_t>(index) >= monitors.size())
        return;
    const auto &entry = monitors[static_cast<size_t>(index)];
    MONITORINFOEXW info{};
    info.cbSize = sizeof(info);
    if (GetMonitorInfoW(entry.first, &info))
    {
        SafeCopyWchar(name, name_capacity, info.szDevice);
        if (width)
            *width = static_cast<int>(info.rcMonitor.right - info.rcMonitor.left);
        if (height)
            *height = static_cast<int>(info.rcMonitor.bottom - info.rcMonitor.top);
    }
}

IR_API int IR_IsDxgiSupported(void)
{
    return 1;
}

IR_API int IR_DxgiDuplicationSupported(void)
{
    ComPtr<ID3D11Device> device;
    ComPtr<ID3D11DeviceContext> context;
    ComPtr<IDXGIDevice> dxgiDevice;
    ComPtr<IDXGIAdapter> adapter;
    ComPtr<IDXGIOutput> output;
    ComPtr<IDXGIOutput1> output1;
    ComPtr<IDXGIOutputDuplication> duplication;
    if (FAILED(CreateD3D11Device(device, context, nullptr)))
        return 0;
    if (FAILED(device.As(&dxgiDevice)))
        return 0;
    if (FAILED(dxgiDevice->GetAdapter(adapter.GetAddressOf())))
        return 0;
    if (FAILED(adapter->EnumOutputs(0, output.GetAddressOf())))
        return 0;
    if (FAILED(output.As(&output1)))
        return 0;
    return SUCCEEDED(output1->DuplicateOutput(device.Get(), duplication.GetAddressOf())) ? 1 : 0;
}

IR_API int IR_ActiveCaptureMethod(void *handle)
{
    Engine *engine = FromHandle(handle);
    return engine ? engine->config.method : IR_CAPTURE_WGC;
}

IR_API int IR_LastHresult(void *handle)
{
    Engine *engine = FromHandle(handle);
    return engine ? static_cast<int>(engine->lastHresult) : IR_FAIL;
}

IR_API int IR_LastFailStage(void *handle)
{
    Engine *engine = FromHandle(handle);
    return engine ? engine->lastFailStage : IR_FAIL_NONE;
}

IR_API int IR_LastFailHr(void *handle)
{
    Engine *engine = FromHandle(handle);
    return engine ? static_cast<int>(engine->lastFailHr) : 0;
}

IR_API int IR_LastCaptureHr(void *handle)
{
    Engine *engine = FromHandle(handle);
    return engine ? static_cast<int>(engine->lastCaptureHr) : 0;
}

IR_API int IR_ActiveEncoder(void *handle)
{
    Engine *engine = FromHandle(handle);
    return engine ? engine->activeEncoder : IR_ENC_H264_SOFTWARE;
}

IR_API int IR_HwInitHr(void *handle)
{
    Engine *engine = FromHandle(handle);
    return engine ? static_cast<int>(engine->hwInitHr) : 0;
}

IR_API int IR_GetMicDeviceCount(void)
{
    HRESULT co = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
    const bool initialized = (co == S_OK);
    const int count = static_cast<int>(EnumerateMicrophoneDevices().size());
    if (initialized)
        CoUninitialize();
    return count;
}

IR_API void IR_GetMicDeviceName(int index, wchar_t *name, int name_capacity)
{
    SafeCopyWchar(name, name_capacity, L"");
    HRESULT co = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
    const bool initialized = (co == S_OK);
    auto devices = EnumerateMicrophoneDevices();
    if (index >= 0 && static_cast<size_t>(index) < devices.size())
        SafeCopyWchar(name, name_capacity, devices[static_cast<size_t>(index)].first);
    if (initialized)
        CoUninitialize();
}

IR_API void IR_MicStatus(void *handle,
                         int64_t *mic_frames,
                         int64_t *mic_events,
                         int64_t *mic_got_frames,
                         int64_t *mic_fail_hr,
                         int *sys_peak,
                         int *mic_peak,
                         wchar_t *active_name,
                         int name_capacity)
{
    Engine *engine = FromHandle(handle);
    if (!engine)
        return;
    std::lock_guard<std::mutex> lock(engine->mutex);
    AudioDiag *diag = engine->micCapture ? &engine->micCapture->Diag() : &engine->micDiag;
    if (mic_frames)
        *mic_frames = diag->frames.load(std::memory_order_relaxed);
    if (mic_events)
        *mic_events = diag->events.load(std::memory_order_relaxed);
    if (mic_got_frames)
        *mic_got_frames = diag->got.load(std::memory_order_relaxed);
    if (mic_fail_hr)
        *mic_fail_hr = diag->failCode.load(std::memory_order_relaxed);
    AudioDiag *sys = engine->systemCapture ? &engine->systemCapture->Diag() : &engine->systemDiag;
    if (sys_peak)
        *sys_peak = sys->peak.load(std::memory_order_relaxed);
    if (mic_peak)
        *mic_peak = diag->peak.load(std::memory_order_relaxed);
    std::wstring name;
    {
        std::lock_guard<std::mutex> nameLock(diag->nameMutex);
        name = diag->activeName;
    }
    SafeCopyWchar(active_name, name_capacity, name);
}

IR_API const char *IR_Version(void)
{
    static const char *version = "InstantReplay native engine 5.0";
    return version;
}

IR_API uint32_t IR_ProbeEncoders(void)
{
    try
    {
        std::call_once(g_mfOnce, []
                       { MFStartup(MF_VERSION, MFSTARTUP_LITE); });
        HRESULT co = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
        const bool initialized = (co == S_OK);
        uint32_t mask = 0;
        mask |= ProbeOneSubtype(MFVideoFormat_H264, 0);
        mask |= ProbeOneSubtype(MFVideoFormat_HEVC, 1);
        mask |= ProbeOneSubtype(MFVideoFormat_AV1, 2);
        if (initialized)
            CoUninitialize();
        return mask;
    }
    catch (...)
    {
        return 0;
    }
}

IR_API int IR_LastErrorMessage(uint8_t *buffer, int capacity)
{
    if (!buffer || capacity <= 0)
        return 0;
    const std::string message = GetIrError();
    const size_t count = std::min<size_t>(message.size(), static_cast<size_t>(capacity - 1));
    if (count)
        std::memcpy(buffer, message.data(), count);
    buffer[count] = 0;
    return static_cast<int>(count);
}
