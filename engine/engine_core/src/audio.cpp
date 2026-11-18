#include "audio.h"

#include <audioclient.h>
#include <propkeydef.h>
#include <functiondiscoverykeys_devpkey.h>
#include <mmdeviceapi.h>
#include <propvarutil.h>

namespace ir {

namespace {

// Layout-compatible copies of the audioclientactivationparams.h types (that header needs a recent SDK).
enum IrActivationType { IR_ACTIVATION_DEFAULT = 0, IR_ACTIVATION_PROCESS_LOOPBACK = 1 };
enum IrLoopbackMode { IR_LOOPBACK_INCLUDE_TREE = 0, IR_LOOPBACK_EXCLUDE_TREE = 1 };
struct IrLoopbackParams {
    DWORD target_process_id;
    IrLoopbackMode mode;
};
struct IrActivationParams {
    IrActivationType type;
    IrLoopbackParams loopback;
};
const wchar_t* const kProcessLoopbackDevice = L"VAD\\Process_Loopback";

// CoInitializeEx only adds an apartment reference for S_OK/S_FALSE; RPC_E_CHANGED_MODE means the
// calling thread is already initialized and must not be uninitialized here.
struct ComGuard {
    HRESULT hr = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
    ~ComGuard() { if (hr == S_OK || hr == S_FALSE) CoUninitialize(); }
};

WAVEFORMATEX pcm_format(WORD channels) {
    WAVEFORMATEX f = {};
    f.wFormatTag = WAVE_FORMAT_PCM;
    f.nChannels = channels;
    f.nSamplesPerSec = AUDIO_RATE;
    f.wBitsPerSample = 16;
    f.nBlockAlign = (WORD)(channels * 2);
    f.nAvgBytesPerSec = AUDIO_RATE * f.nBlockAlign;
    return f;
}

ComPtr<IMMDeviceEnumerator> enumerator() {
    ComPtr<IMMDeviceEnumerator> e;
    CoCreateInstance(__uuidof(MMDeviceEnumerator), nullptr, CLSCTX_ALL, IID_PPV_ARGS(&e));
    return e;
}

std::wstring friendly_name(IMMDevice* dev) {
    ComPtr<IPropertyStore> props;
    if (FAILED(dev->OpenPropertyStore(STGM_READ, &props))) return L"";
    PROPVARIANT v;
    PropVariantInit(&v);
    std::wstring name;
    if (SUCCEEDED(props->GetValue(PKEY_Device_FriendlyName, &v)) && v.vt == VT_LPWSTR && v.pwszVal) name = v.pwszVal;
    PropVariantClear(&v);
    return name;
}

// Completion handler for ActivateAudioInterfaceAsync (agile: signals from an MTA worker).
class ActivateHandler final : public IActivateAudioInterfaceCompletionHandler, public IAgileObject {
public:
    HANDLE done = CreateEventW(nullptr, TRUE, FALSE, nullptr);
    ~ActivateHandler() { if (done) CloseHandle(done); }
    ULONG STDMETHODCALLTYPE AddRef() override { return ++refs_; }
    ULONG STDMETHODCALLTYPE Release() override {
        ULONG r = --refs_;
        if (r == 0) delete this;
        return r;
    }
    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void** out) override {
        if (iid == __uuidof(IUnknown) || iid == __uuidof(IActivateAudioInterfaceCompletionHandler)) {
            *out = static_cast<IActivateAudioInterfaceCompletionHandler*>(this);
        } else if (iid == __uuidof(IAgileObject)) {
            *out = static_cast<IAgileObject*>(this);
        } else {
            *out = nullptr;
            return E_NOINTERFACE;
        }
        AddRef();
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE ActivateCompleted(IActivateAudioInterfaceAsyncOperation*) override {
        SetEvent(done);
        return S_OK;
    }

private:
    std::atomic<ULONG> refs_{1};
};

HRESULT process_client(DWORD pid, ComPtr<IAudioClient>* out) {
    IrActivationParams params = {};
    params.type = IR_ACTIVATION_PROCESS_LOOPBACK;
    params.loopback.target_process_id = pid;
    params.loopback.mode = IR_LOOPBACK_INCLUDE_TREE;
    PROPVARIANT pv;
    PropVariantInit(&pv);
    pv.vt = VT_BLOB;
    pv.blob.cbSize = sizeof(params);
    pv.blob.pBlobData = reinterpret_cast<BYTE*>(&params);

    auto* handler = new ActivateHandler();
    ComPtr<IActivateAudioInterfaceAsyncOperation> op;
    HRESULT hr = ActivateAudioInterfaceAsync(kProcessLoopbackDevice, __uuidof(IAudioClient), &pv, handler, &op);
    if (SUCCEEDED(hr)) {
        WaitForSingleObject(handler->done, 5000);
        HRESULT activate_hr = E_FAIL;
        ComPtr<IUnknown> unk;
        hr = op->GetActivateResult(&activate_hr, &unk);
        if (SUCCEEDED(hr)) hr = activate_hr;
        if (SUCCEEDED(hr)) hr = unk.As(out);
    }
    handler->Release();
    return hr;
}

// Keeps the render endpoint flowing so loopback capture delivers packets during silence.
class Silence {
public:
    bool start(IMMDevice* dev) {
        if (FAILED(dev->Activate(__uuidof(IAudioClient), CLSCTX_ALL, nullptr, reinterpret_cast<void**>(client_.GetAddressOf())))) return false;
        WAVEFORMATEX f = pcm_format(2);
        if (FAILED(client_->Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                                       1'000'000, 0, &f, nullptr))) return false;
        if (FAILED(client_->GetService(IID_PPV_ARGS(&render_)))) return false;
        if (FAILED(client_->GetBufferSize(&capacity_))) return false;
        return SUCCEEDED(client_->Start());
    }
    void feed() {
        UINT32 padding = 0;
        if (!client_ || FAILED(client_->GetCurrentPadding(&padding)) || padding >= capacity_) return;
        UINT32 n = capacity_ - padding;
        BYTE* data = nullptr;
        if (SUCCEEDED(render_->GetBuffer(n, &data))) render_->ReleaseBuffer(n, AUDCLNT_BUFFERFLAGS_SILENT);
    }
    ~Silence() { if (client_) client_->Stop(); }

private:
    ComPtr<IAudioClient> client_;
    ComPtr<IAudioRenderClient> render_;
    UINT32 capacity_ = 0;
};

int peak_of(const int16_t* s, size_t count) {
    int p = 0;
    for (size_t i = 0; i < count; ++i) {
        int v = s[i] < 0 ? -(int)s[i] : s[i];
        if (v > p) p = v;
    }
    return std::min(p, 32767);
}

} // namespace

std::vector<std::wstring> mic_devices() {
    std::vector<std::wstring> names;
    ComGuard com;
    auto en = enumerator();
    if (!en) return names;
    ComPtr<IMMDeviceCollection> coll;
    if (FAILED(en->EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE, &coll))) return names;
    UINT n = 0;
    coll->GetCount(&n);
    for (UINT i = 0; i < n; ++i) {
        ComPtr<IMMDevice> d;
        if (SUCCEEDED(coll->Item(i, &d))) {
            std::wstring name = friendly_name(d.Get());
            if (!name.empty()) names.push_back(name);
        }
    }
    return names;
}

AudioSource::AudioSource(bool mic, std::wstring device_name, HWND target, AudioPush push, AudioCounters* counters)
    : mic_(mic), device_name_(std::move(device_name)), target_(target), push_(std::move(push)), counters_(counters) {}

AudioSource::~AudioSource() { stop(); }

void AudioSource::start() {
    if (running_.exchange(true)) return;
    thread_ = std::thread([this] { run(); });
}

void AudioSource::stop() {
    running_ = false;
    if (thread_.joinable()) thread_.join();
}

void AudioSource::run() {
    HRESULT co = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
    if (FAILED(co) && co != RPC_E_CHANGED_MODE) {
        counters_->fail_hr = co;
        return;
    }
    while (running_) {
        if (!run_once()) {
            for (int i = 0; i < 10 && running_; ++i) Sleep(50); // retry after a short pause
        }
    }
    if (SUCCEEDED(co)) CoUninitialize();
}

bool AudioSource::run_once() {
    ComPtr<IMMDevice> device;
    ComPtr<IAudioClient> client;
    HRESULT hr = S_OK;
    const bool process_audio = !mic_ && target_ != nullptr;

    if (process_audio) {
        DWORD pid = 0;
        GetWindowThreadProcessId(target_, &pid);
        if (pid == 0) { counters_->fail_hr = E_INVALIDARG; return false; }
        hr = process_client(pid, &client);
    } else {
        auto en = enumerator();
        if (!en) { counters_->fail_hr = E_FAIL; return false; }
        if (!mic_) {
            hr = en->GetDefaultAudioEndpoint(eRender, eConsole, &device);
        } else {
            std::wstring chosen;
            if (!device_name_.empty()) {
                ComPtr<IMMDeviceCollection> coll;
                UINT n = 0;
                if (SUCCEEDED(en->EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE, &coll))) coll->GetCount(&n);
                for (UINT i = 0; i < n; ++i) {
                    ComPtr<IMMDevice> d;
                    if (SUCCEEDED(coll->Item(i, &d)) && friendly_name(d.Get()) == device_name_) { device = d; break; }
                }
            }
            if (!device) hr = en->GetDefaultAudioEndpoint(eCapture, eConsole, &device);
            if (device) {
                std::lock_guard<std::mutex> lock(counters_->name_mutex);
                counters_->active_name = friendly_name(device.Get());
            }
        }
        if (SUCCEEDED(hr) && device)
            hr = device->Activate(__uuidof(IAudioClient), CLSCTX_ALL, nullptr, reinterpret_cast<void**>(client.GetAddressOf()));
    }
    if (FAILED(hr) || !client) { counters_->fail_hr = FAILED(hr) ? hr : E_FAIL; return false; }

    const WORD channels = mic_ ? 1 : 2;
    WAVEFORMATEX fmt = pcm_format(channels);
    DWORD flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM |
                  AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY | (mic_ ? 0 : AUDCLNT_STREAMFLAGS_LOOPBACK);
    hr = client->Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 1'000'000, 0, &fmt, nullptr);
    if (FAILED(hr)) { counters_->fail_hr = hr; return false; }
    HANDLE ev = CreateEventW(nullptr, FALSE, FALSE, nullptr);
    client->SetEventHandle(ev);
    ComPtr<IAudioCaptureClient> capture;
    hr = client->GetService(IID_PPV_ARGS(&capture));
    if (FAILED(hr) || FAILED(hr = client->Start())) { CloseHandle(ev); counters_->fail_hr = hr; return false; }

    Silence silence;
    bool have_silence = !mic_ && device && silence.start(device.Get());
    const bool process_loopback = process_audio;
    int64_t last_packet = qpc100ns();
    bool ok = true;
    std::vector<int16_t> packet;

    while (running_ && ok) {
        if (have_silence) silence.feed();
        WaitForSingleObject(ev, 50);
        bool received = false;
        UINT32 next = 0;
        while (SUCCEEDED(capture->GetNextPacketSize(&next)) && next > 0) {
            received = true;
            BYTE* data = nullptr;
            UINT32 frames = 0;
            DWORD bflags = 0;
            UINT64 position = 0, qpc = 0;
            if (FAILED(capture->GetBuffer(&data, &frames, &bflags, &position, &qpc))) { ok = false; break; }
            const size_t samples = (size_t)frames * channels;
            packet.assign(samples, 0);
            const bool silent = (bflags & AUDCLNT_BUFFERFLAGS_SILENT) || !data;
            if (!silent) std::memcpy(packet.data(), data, samples * 2);
            capture->ReleaseBuffer(frames);

            counters_->events++;
            counters_->frames += frames;
            if (!silent) counters_->got_frames += frames;
            counters_->peak = peak_of(packet.data(), samples);
            // qpc from WASAPI is already in 100 ns units.
            const int64_t t = (qpc == 0 || (bflags & AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR)) ? qpc100ns() : (int64_t)qpc;
            push_(packet.data(), frames, t);
        }
        if (received) last_packet = qpc100ns();
        else if (process_loopback && qpc100ns() - last_packet >= 15'000'000) ok = false; // process went quiet/away
    }
    client->Stop();
    CloseHandle(ev);
    return ok;
}

} // namespace ir
