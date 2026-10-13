use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::{RingAudio, SharedState, SECOND_100NS};

use windows::{
    core::*,
    Win32::{
        Foundation::*,
        Media::Audio::*,
        System::{
            Com::{StructuredStorage::PropVariantToStringAlloc, *},
            Threading::*,
        },
    },
};
struct ComApartment;
#[implement(IActivateAudioInterfaceCompletionHandler)]
struct ActivationCompleted {
    ready: Arc<AtomicBool>,
}
impl IActivateAudioInterfaceCompletionHandler_Impl for ActivationCompleted_Impl {
    fn ActivateCompleted(&self, _: Ref<'_, IActivateAudioInterfaceAsyncOperation>) -> Result<()> {
        self.ready.store(true, Ordering::Release);
        Ok(())
    }
}
unsafe fn process_client(pid: u32) -> Result<IAudioClient> {
    use windows::Win32::System::{Com::StructuredStorage::*, Variant::VT_BLOB};
    let activation = AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: pid,
                ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            },
        },
    };
    let blob =
        CoTaskMemAlloc(std::mem::size_of_val(&activation)) as *mut AUDIOCLIENT_ACTIVATION_PARAMS;
    if blob.is_null() {
        return Err(Error::from_hresult(E_OUTOFMEMORY));
    }
    blob.write(activation);
    let mut params = PROPVARIANT::default();
    params.Anonymous.Anonymous = std::mem::ManuallyDrop::new(PROPVARIANT_0_0 {
        vt: VT_BLOB,
        Anonymous: PROPVARIANT_0_0_0 {
            blob: BLOB {
                cbSize: std::mem::size_of_val(&activation) as u32,
                pBlobData: blob as _,
            },
        },
        ..Default::default()
    });
    let ready = Arc::new(AtomicBool::new(false));
    let handler: IActivateAudioInterfaceCompletionHandler = ActivationCompleted {
        ready: ready.clone(),
    }
    .into();
    let op = ActivateAudioInterfaceAsync(
        VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
        &IAudioClient::IID,
        Some(&params),
        &handler,
    )?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !ready.load(Ordering::Acquire) {
        if std::time::Instant::now() >= deadline {
            return Err(Error::from_hresult(HRESULT(0x800705B4u32 as i32)));
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let mut status = S_OK;
    let mut object = None;
    op.GetActivateResult(&mut status, &mut object)?;
    status.ok()?;
    object.ok_or_else(|| Error::from_hresult(E_FAIL))?.cast()
}
impl ComApartment {
    unsafe fn new() -> windows::core::Result<Self> {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        Ok(Self)
    }
}
impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}
unsafe fn enumerator() -> windows::core::Result<IMMDeviceEnumerator> {
    CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
}
#[repr(C)]
struct PropertyKey {
    fmtid: GUID,
    pid: u32,
}
unsafe fn device_name(device: &IMMDevice) -> windows::core::Result<String> {
    use std::ffi::c_void;
    use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PROPVARIANT};
    type OpenStore = unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> HRESULT;
    type GetValue =
        unsafe extern "system" fn(*mut c_void, *const PropertyKey, *mut PROPVARIANT) -> HRESULT;
    let table = *(device.as_raw() as *const *const usize);
    let open: OpenStore = std::mem::transmute(*table.add(4));
    let mut pointer = std::ptr::null_mut();
    open(device.as_raw(), 0, &mut pointer).ok()?;
    let store = IUnknown::from_raw(pointer);
    let table = *(store.as_raw() as *const *const usize);
    let get: GetValue = std::mem::transmute(*table.add(5));
    let key = PropertyKey {
        fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
        pid: 14,
    };
    let mut property = PROPVARIANT::default();
    get(store.as_raw(), &key, &mut property).ok()?;
    let result = PropVariantToStringAlloc(&property);
    let _ = PropVariantClear(&mut property);
    let value = result?;
    let text = value.to_string();
    CoTaskMemFree(Some(value.0 as _));
    Ok(text?)
}

pub const OUT_RATE: u32 = 48000;
pub const OUT_CHANNELS: u32 = 2;

const MIX_BLOCK_FRAMES: i64 = 48_000;
const MIX_CHUNK_FRAMES: usize = 480;
const BOTH_GAIN: f32 = 0.707_106_8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sink {
    System,
    Microphone,
}

impl Sink {
    fn label(self) -> &'static str {
        match self {
            Sink::System => "audio",
            Sink::Microphone => "mic",
        }
    }

    fn note_event(self, shared: &SharedState) {
        match self {
            Sink::System => shared.audio_events.fetch_add(1, Ordering::Relaxed),
            Sink::Microphone => shared.mic_events.fetch_add(1, Ordering::Relaxed),
        };
    }

    fn note_fail(self, shared: &SharedState, code: i64) {
        match self {
            Sink::System => shared.audio_fail_hr.store(code, Ordering::Relaxed),
            Sink::Microphone => shared.mic_fail_hr.store(code, Ordering::Relaxed),
        };
    }
}

pub fn mic_devices() -> Vec<String> {
    unsafe {
        let Ok(_apartment) = ComApartment::new() else {
            return Vec::new();
        };
        let Ok(enumerator) = enumerator() else {
            return Vec::new();
        };
        let Ok(devices) = enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE) else {
            return Vec::new();
        };
        let mut names = Vec::new();
        for index in 0..devices.GetCount().unwrap_or(0) {
            if let Ok(device) = devices.Item(index) {
                if let Ok(name) = device_name(&device) {
                    if !name.trim().is_empty() && !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
        }
        names
    }
}
unsafe fn resolve_mic_device(name: Option<&str>) -> windows::core::Result<(IMMDevice, String)> {
    let wanted = name
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Error::new(E_INVALIDARG, "No microphone selected"))?;
    let devices = enumerator()?.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)?;
    for index in 0..devices.GetCount()? {
        let device = devices.Item(index)?;
        let name = device_name(&device)?;
        if name == wanted {
            return Ok((device, name));
        }
    }
    Err(Error::new(
        E_INVALIDARG,
        "The selected microphone is unavailable",
    ))
}

pub struct CubicResampler {
    in_bits: u32,
    in_channels: u32,
    in_bytes_per_frame: usize,
    pub dec: u32,
    step: f64,
    mono: bool,
    out_mono: bool,
    pos: f64,
    hist_l: Vec<f64>,
    hist_r: Vec<f64>,
    dec_acc_l: f64,
    dec_acc_r: f64,
    dec_count: u32,
    ready: bool,
}

impl CubicResampler {
    pub fn new() -> Self {
        CubicResampler {
            in_bits: 16,
            in_channels: 2,
            in_bytes_per_frame: 4,
            dec: 1,
            step: 1.0,
            mono: false,
            out_mono: false,
            pos: 0.0,
            hist_l: Vec::new(),
            hist_r: Vec::new(),
            dec_acc_l: 0.0,
            dec_acc_r: 0.0,
            dec_count: 0,
            ready: false,
        }
    }

    pub fn passthrough(in_rate: u32, in_channels: u32, in_bits: u32) -> bool {
        in_rate == OUT_RATE && in_channels == OUT_CHANNELS && in_bits == 16
    }

    pub fn init(&mut self, in_rate: u32, in_channels: u32, in_bits: u32) -> bool {
        if in_rate < 8000 || in_rate > 384000 || !matches!(in_bits, 16 | 24 | 32) {
            return false;
        }
        self.in_bits = in_bits;
        self.in_channels = in_channels;
        self.in_bytes_per_frame = in_channels as usize * (in_bits as usize / 8);

        let ratio = in_rate as f64 / OUT_RATE as f64;
        let dec = if ratio >= 1.5 { ratio as u32 } else { 1 };
        self.dec = dec.max(1);
        self.step = (in_rate as f64 / self.dec as f64) / OUT_RATE as f64;
        self.mono = in_channels == 1;
        self.out_mono = false;

        self.hist_l = vec![0.0; 3];
        self.hist_r = vec![0.0; 3];
        self.pos = 0.0;
        self.dec_acc_l = 0.0;
        self.dec_acc_r = 0.0;
        self.dec_count = 0;
        self.ready = true;
        true
    }

    pub fn init_mono(&mut self, in_rate: u32, in_channels: u32, in_bits: u32) -> bool {
        if !self.init(in_rate, in_channels, in_bits) {
            return false;
        }
        self.out_mono = true;
        true
    }

    pub fn output_bytes_per_frame(&self) -> usize {
        if self.out_mono {
            2
        } else {
            4
        }
    }

    pub fn process(&mut self, bytes: &[u8], out: &mut Vec<u8>) -> bool {
        out.clear();
        if !self.ready || bytes.is_empty() {
            return false;
        }
        let frames = bytes.len() / self.in_bytes_per_frame;
        if frames == 0 {
            return false;
        }

        let mut p = 0usize;
        for _ in 0..frames {
            let (l, r) = decode_frame(bytes, p, self.in_bits, self.in_channels, self.mono);
            p += self.in_bytes_per_frame;

            self.dec_acc_l += l;
            self.dec_acc_r += r;
            self.dec_count += 1;
            if self.dec_count == self.dec {
                self.hist_l.push(self.dec_acc_l / self.dec as f64);
                self.hist_r.push(self.dec_acc_r / self.dec as f64);
                self.dec_acc_l = 0.0;
                self.dec_acc_r = 0.0;
                self.dec_count = 0;
            }
        }

        loop {
            let src = self.pos;
            let i0 = src as usize;
            let f = src - i0 as f64;
            if i0 + 3 >= self.hist_l.len() {
                break;
            }
            if i0 < 1 {
                self.pos += self.step;
                continue;
            }
            let l = cubic(
                self.hist_l[i0 - 1],
                self.hist_l[i0],
                self.hist_l[i0 + 1],
                self.hist_l[i0 + 2],
                f,
            );
            let r = cubic(
                self.hist_r[i0 - 1],
                self.hist_r[i0],
                self.hist_r[i0 + 1],
                self.hist_r[i0 + 2],
                f,
            );
            if self.out_mono {
                let sm = clamp16((l + r) * 0.5);
                out.push(sm as u8);
                out.push((sm >> 8) as u8);
            } else {
                let sl = clamp16(l);
                let sr = clamp16(r);
                out.push(sl as u8);
                out.push((sl >> 8) as u8);
                out.push(sr as u8);
                out.push((sr >> 8) as u8);
            }
            self.pos += self.step;
        }

        let keep_from = self.pos as usize;
        let drop = keep_from.saturating_sub(2);
        for _ in 0..drop {
            if self.hist_l.is_empty() || self.hist_r.is_empty() {
                break;
            }
            self.hist_l.remove(0);
            self.hist_r.remove(0);
            self.pos -= 1.0;
        }
        !out.is_empty()
    }
}

fn decode_frame(bytes: &[u8], p: usize, bits: u32, _channels: u32, mono: bool) -> (f64, f64) {
    match bits {
        32 => {
            let l = f32::from_le_bytes([bytes[p], bytes[p + 1], bytes[p + 2], bytes[p + 3]]) as f64;
            let r = if mono {
                l
            } else {
                f32::from_le_bytes([bytes[p + 4], bytes[p + 5], bytes[p + 6], bytes[p + 7]]) as f64
            };
            (l, r)
        }
        16 => {
            let l = i16::from_le_bytes([bytes[p], bytes[p + 1]]) as f64 / 32768.0;
            let r = if mono {
                l
            } else {
                i16::from_le_bytes([bytes[p + 2], bytes[p + 3]]) as f64 / 32768.0
            };
            (l, r)
        }
        _ => {
            let l = (((bytes[p + 2] as i32) << 24)
                | ((bytes[p + 1] as i32) << 16)
                | ((bytes[p] as i32) << 8))
                >> 8;
            let l = l as f64 / 8388608.0;
            let r = if mono {
                l
            } else {
                let r = (((bytes[p + 5] as i32) << 24)
                    | ((bytes[p + 4] as i32) << 16)
                    | ((bytes[p + 3] as i32) << 8))
                    >> 8;
                r as f64 / 8388608.0
            };
            (l, r)
        }
    }
}

fn cubic(a: f64, b: f64, c: f64, d: f64, t: f64) -> f64 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * ((2.0 * b)
        + (c - a) * t
        + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2
        + (3.0 * b - a - 3.0 * c + d) * t3)
}

fn clamp16(v: f64) -> i16 {
    let v = if v > 1.0 {
        1.0
    } else if v < -1.0 {
        -1.0
    } else {
        v
    };
    (v * 32767.0) as i16
}

pub fn passthrough_stereo(bytes: &[u8]) -> &[u8] {
    bytes
}

impl Default for CubicResampler {
    fn default() -> Self {
        Self::new()
    }
}

pub fn spawn(shared: Arc<SharedState>) -> std::thread::JoinHandle<()> {
    spawn_source(shared, None, Sink::System)
}
pub fn spawn_mic(
    shared: Arc<SharedState>,
    device_name: Option<String>,
) -> std::thread::JoinHandle<()> {
    spawn_source(shared, device_name, Sink::Microphone)
}
fn spawn_source(
    shared: Arc<SharedState>,
    device_name: Option<String>,
    sink: Sink,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name(format!("ir-{}", sink.label()))
        .spawn(move || unsafe {
            let result = (|| -> windows::core::Result<()> {
                let _apartment = ComApartment::new()?;
                if sink == Sink::System && shared.capture_options.load(Ordering::Acquire) & 64 != 0
                {
                    let window = HWND(shared.target_window.load(Ordering::Acquire) as *mut _);
                    let mut pid = 0;
                    windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(
                        window,
                        Some(&mut pid),
                    );
                    if pid == 0 {
                        return Err(Error::from_hresult(E_INVALIDARG));
                    }
                    return run_client(&shared, process_client(pid)?, None, sink);
                }
                let device = if sink == Sink::System {
                    enumerator()?.GetDefaultAudioEndpoint(eRender, eConsole)?
                } else {
                    let (device, name) = resolve_mic_device(device_name.as_deref())?;
                    *shared.mic_active.lock().unwrap_or_else(|e| e.into_inner()) = name;
                    device
                };
                run_source(&shared, &device, sink)
            })();
            if let Err(error) = result {
                sink.note_fail(&shared, error.code().0 as i64);
                if sink == Sink::System && shared.capture_options.load(Ordering::Acquire) & 64 != 0
                {
                    shared.has_error.store(true, Ordering::Release);
                }
            }
        })
        .expect("spawn audio worker")
}
struct AudioStream(IAudioClient);
impl Drop for AudioStream {
    fn drop(&mut self) {
        unsafe {
            let _ = self.0.Stop();
        }
    }
}
struct AudioEvent(HANDLE);
impl Drop for AudioEvent {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
fn pcm_format(channels: u16) -> WAVEFORMATEX {
    WAVEFORMATEX {
        wFormatTag: 1,
        nChannels: channels,
        nSamplesPerSec: OUT_RATE,
        nAvgBytesPerSec: OUT_RATE * channels as u32 * 2,
        nBlockAlign: channels * 2,
        wBitsPerSample: 16,
        cbSize: 0,
    }
}
unsafe fn run_source(
    shared: &Arc<SharedState>,
    device: &IMMDevice,
    sink: Sink,
) -> windows::core::Result<()> {
    let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
    run_client(shared, client, Some(device), sink)
}
unsafe fn run_client(
    shared: &Arc<SharedState>,
    client: IAudioClient,
    device: Option<&IMMDevice>,
    sink: Sink,
) -> windows::core::Result<()> {
    let channels = if sink == Sink::Microphone { 1 } else { 2 };
    let format = pcm_format(channels);
    let flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK
        | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
        | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY
        | if sink == Sink::System {
            AUDCLNT_STREAMFLAGS_LOOPBACK
        } else {
            0
        };
    client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 1_000_000, 0, &format, None)?;
    let event = AudioEvent(CreateEventW(None, false, false, None)?);
    client.SetEventHandle(event.0)?;
    let capture: IAudioCaptureClient = client.GetService()?;
    client.Start()?;
    let _stream = AudioStream(client);
    let silence = if sink == Sink::System {
        device.and_then(|device| Silence::start(device).ok())
    } else {
        None
    };
    let mut packet = Vec::new();
    while shared.running.load(Ordering::Acquire) {
        if let Some(silence) = &silence {
            silence.feed();
        }
        let _ = WaitForSingleObject(event.0, 50);
        while capture.GetNextPacketSize()? > 0 {
            let (mut data, mut frames, mut flags, mut timestamp) =
                (std::ptr::null_mut(), 0, 0, 0u64);
            capture.GetBuffer(
                &mut data,
                &mut frames,
                &mut flags,
                None,
                Some(&mut timestamp),
            )?;
            let length = frames as usize * channels as usize * 2;
            packet.resize(length, 0);
            if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                packet.fill(0);
            } else {
                packet.copy_from_slice(std::slice::from_raw_parts(data, length));
            }
            capture.ReleaseBuffer(frames)?;
            sink.note_event(shared);
            let time =
                if timestamp == 0 || flags & AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR.0 as u32 != 0 {
                    crate::qpc100ns()
                } else {
                    timestamp.min(i64::MAX as u64) as i64
                };
            let timestamp = if sink == Sink::Microphone {
                shared.microphone_time(time)
            } else {
                shared.capture_time(time)
            };
            let Some(pts) = timestamp else {
                continue;
            };
            match sink {
                Sink::System => shared.push_audio_at(&packet, frames, pts),
                Sink::Microphone => shared.push_mic_at(&packet, frames, pts),
            }
        }
    }
    Ok(())
}
struct Silence {
    stream: AudioStream,
    render: IAudioRenderClient,
    capacity: u32,
}
impl Silence {
    unsafe fn start(device: &IMMDevice) -> windows::core::Result<Self> {
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
        let format = pcm_format(2);
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
            1_000_000,
            0,
            &format,
            None,
        )?;
        let render = client.GetService()?;
        let capacity = client.GetBufferSize()?;
        client.Start()?;
        Ok(Self {
            stream: AudioStream(client),
            render,
            capacity,
        })
    }
    unsafe fn feed(&self) {
        if let Ok(padding) = self.stream.0.GetCurrentPadding() {
            let available = self.capacity.saturating_sub(padding);
            if available > 0 && self.render.GetBuffer(available).is_ok() {
                let _ = self
                    .render
                    .ReleaseBuffer(available, AUDCLNT_BUFFERFLAGS_SILENT.0 as u32);
            }
        }
    }
}

fn sample_index(pts100ns: i64) -> i64 {
    pts100ns.saturating_mul(OUT_RATE as i64) / SECOND_100NS
}

fn pts_of(sample: i64) -> i64 {
    sample.saturating_mul(SECOND_100NS) / OUT_RATE as i64
}

struct Span {
    start: i64,
    end: i64,
    idx: usize,
}

fn spans(ring: &[RingAudio], channels: usize) -> Vec<Span> {
    let mut v: Vec<Span> = ring
        .iter()
        .enumerate()
        .filter(|(_, s)| s.frames > 0 && s.data.len() >= channels * 2)
        .map(|(idx, s)| {
            let start = sample_index(s.pts100ns);
            Span {
                start,
                end: start + s.frames as i64,
                idx,
            }
        })
        .collect();
    v.sort_by_key(|s| s.start);
    v
}

fn clamp_spans(v: Vec<Span>, lo: i64) -> Vec<Span> {
    v.into_iter()
        .filter(|s| s.end > lo)
        .map(|s| Span {
            start: s.start.max(lo),
            end: s.end,
            idx: s.idx,
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn accumulate(
    spans: &[Span],
    cursor: &mut usize,
    base: i64,
    block_end: i64,
    ring: &[RingAudio],
    channels: usize,
    gain: f32,
    l: &mut [f32],
    r: &mut [f32],
) {
    let mut i = *cursor;
    while i < spans.len() && spans[i].end <= base {
        i += 1;
    }
    *cursor = i;
    let mut k = i;
    while k < spans.len() && spans[k].start < block_end {
        let span = &spans[k];
        let sample = &ring[span.idx];
        let from = span.start.max(base);
        let to = span.end.min(block_end);
        for si in from..to {
            let frame = (si - span.start) as usize;
            let o = frame * channels * 2;
            if o + 1 >= sample.data.len() {
                continue;
            }
            let lv = i16::from_le_bytes([sample.data[o], sample.data[o + 1]]) as f32 * gain;
            let d = (si - base) as usize;
            l[d] += lv;
            if channels >= 2 && o + 3 < sample.data.len() {
                r[d] += i16::from_le_bytes([sample.data[o + 2], sample.data[o + 3]]) as f32 * gain;
            } else {
                r[d] += lv;
            }
        }
        k += 1;
    }
}

fn to_i16(v: f32) -> i16 {
    let r = v.round();
    if r > i16::MAX as f32 {
        i16::MAX
    } else if r < i16::MIN as f32 {
        i16::MIN
    } else {
        r as i16
    }
}

pub fn mix(sys: &[RingAudio], mic: &[RingAudio], max_frames: i64) -> Vec<RingAudio> {
    let mut sys_spans = spans(sys, 2);
    let mut mic_spans = spans(mic, 1);

    let lo = sys_spans
        .iter()
        .chain(mic_spans.iter())
        .map(|s| s.start)
        .min();
    let hi = sys_spans
        .iter()
        .chain(mic_spans.iter())
        .map(|s| s.end)
        .max();
    let (Some(lo), Some(hi)) = (lo, hi) else {
        return Vec::new();
    };
    if hi <= lo {
        return Vec::new();
    }

    let floor = hi.saturating_sub(max_frames.max(1));
    let lo = lo.max(floor);
    if hi <= lo {
        return Vec::new();
    }

    sys_spans = clamp_spans(sys_spans, lo);
    mic_spans = clamp_spans(mic_spans, lo);

    let (sys_gain, mic_gain) = match (sys_spans.is_empty(), mic_spans.is_empty()) {
        (false, false) => (BOTH_GAIN, BOTH_GAIN),
        (false, true) => (1.0, 0.0),
        (true, false) => (0.0, 1.0),
        (true, true) => (0.0, 0.0),
    };

    let mut out: Vec<RingAudio> = Vec::new();
    let mut sys_cur = 0usize;
    let mut mic_cur = 0usize;
    let mut l: Vec<f32> = Vec::new();
    let mut r: Vec<f32> = Vec::new();
    let mut data: Vec<u8> = Vec::with_capacity(MIX_CHUNK_FRAMES * 4);
    let mut base = lo;

    while base < hi {
        let block_end = (base + MIX_BLOCK_FRAMES).min(hi);
        let n = (block_end - base) as usize;
        l.clear();
        l.resize(n, 0.0);
        r.clear();
        r.resize(n, 0.0);

        accumulate(
            &sys_spans,
            &mut sys_cur,
            base,
            block_end,
            sys,
            2,
            sys_gain,
            &mut l,
            &mut r,
        );
        accumulate(
            &mic_spans,
            &mut mic_cur,
            base,
            block_end,
            mic,
            1,
            mic_gain,
            &mut l,
            &mut r,
        );

        let mut off = 0usize;
        while off < n {
            let take = MIX_CHUNK_FRAMES.min(n - off);
            data.clear();
            for i in 0..take {
                let sl = to_i16(l[off + i]);
                let sr = to_i16(r[off + i]);
                data.push(sl as u8);
                data.push((sl >> 8) as u8);
                data.push(sr as u8);
                data.push((sr >> 8) as u8);
            }
            out.push(RingAudio {
                data: data.clone(),
                frames: take as u32,
                pts100ns: pts_of(base + off as i64),
            });
            off += take;
        }
        base = block_end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pts(frames: i64) -> i64 {
        frames * SECOND_100NS / OUT_RATE as i64
    }

    fn pcm16(samples: &[i16]) -> Vec<u8> {
        let mut v = Vec::with_capacity(samples.len() * 2);
        for s in samples {
            v.extend_from_slice(&s.to_le_bytes());
        }
        v
    }

    fn stereo_pkt(start_frame: i64, frames: u32, l: i16, r: i16) -> RingAudio {
        let mut data = Vec::with_capacity(frames as usize * 4);
        for _ in 0..frames {
            data.extend_from_slice(&l.to_le_bytes());
            data.extend_from_slice(&r.to_le_bytes());
        }
        RingAudio {
            data,
            frames,
            pts100ns: pts(start_frame),
        }
    }

    fn mono_pkt(start_frame: i64, frames: u32, v: i16) -> RingAudio {
        RingAudio {
            data: pcm16(&vec![v; frames as usize]),
            frames,
            pts100ns: pts(start_frame),
        }
    }

    fn flat(out: &[RingAudio]) -> Vec<i16> {
        let mut v = Vec::new();
        for p in out {
            assert_eq!(
                p.data.len(),
                p.frames as usize * 4,
                "every chunk must be stereo i16"
            );
            for c in p.data.chunks_exact(2) {
                v.push(i16::from_le_bytes([c[0], c[1]]));
            }
        }
        v
    }

    fn total_frames(out: &[RingAudio]) -> i64 {
        out.iter().map(|p| p.frames as i64).sum()
    }

    fn all(out: &[RingAudio], want: i16) -> Vec<i16> {
        let got = flat(out);
        assert!(!got.is_empty());
        assert!(got.iter().all(|&s| s == want), "expected all {want}");
        got
    }

    const WIDE: i64 = 1 << 40;

    fn head(v: i16) -> f32 {
        v as f32 * BOTH_GAIN
    }

    fn both(sys: i16, mic: i16) -> i16 {
        to_i16(head(sys) + head(mic))
    }

    fn sys_only(v: i16) -> i16 {
        to_i16(head(v))
    }

    fn mic_only(v: i16) -> i16 {
        to_i16(head(v))
    }

    #[test]
    fn no_sources_produces_nothing() {
        assert!(mix(&[], &[], WIDE).is_empty());
    }

    #[test]
    fn an_empty_source_is_not_a_source() {
        assert!(mix(&[], &[], WIDE).is_empty());
        assert!(mix(&[stereo_pkt(0, 0, 1, 1)], &[], WIDE).is_empty());
    }

    #[test]
    fn system_only_keeps_full_scale() {
        let out = mix(&[stereo_pkt(0, 480, 30000, -30000)], &[], WIDE);
        let got = flat(&out);
        assert_eq!(got.len(), 960, "480 frames of stereo i16");
        for f in got.chunks_exact(2) {
            assert_eq!(f, [30000, -30000]);
        }
    }

    #[test]
    fn mic_only_is_centred() {
        let out = mix(&[], &[mono_pkt(0, 480, 1234)], WIDE);
        assert_eq!(all(&out, 1234).len(), 960);
    }

    #[test]
    fn both_sources_sum_and_are_headroomed() {
        let out = mix(
            &[stereo_pkt(0, 480, 20000, 20000)],
            &[mono_pkt(0, 480, 20000)],
            WIDE,
        );
        assert_eq!(all(&out, both(20000, 20000)).len(), 960);
        assert!(head(20000) + head(20000) < 32767.0);
    }

    #[test]
    fn two_full_scale_sources_are_clamped_not_overflowed() {
        let out = mix(
            &[stereo_pkt(0, 480, i16::MAX, i16::MAX)],
            &[mono_pkt(0, 480, i16::MAX)],
            WIDE,
        );
        let got = flat(&out);
        assert_eq!(got.len(), 960);
        assert!(
            got.iter().all(|&s| s > 0 && s <= i16::MAX),
            "must stay inside i16, never wrap"
        );
    }

    #[test]
    fn negative_full_scale_sources_never_clip() {
        let out = mix(
            &[stereo_pkt(0, 480, i16::MIN, i16::MIN)],
            &[mono_pkt(0, 480, i16::MIN)],
            WIDE,
        );
        let got = flat(&out);
        assert!(
            got.iter().all(|&s| s < 0 && s >= i16::MIN),
            "must stay inside i16"
        );
    }

    #[test]
    fn misaligned_sources_line_up_by_pts_not_arrival() {
        let out = mix(
            &[stereo_pkt(0, 480, 1000, 1000)],
            &[mono_pkt(240, 480, 500)],
            WIDE,
        );
        assert_eq!(total_frames(&out), 720, "union of [0,480) and [240,720)");

        let got = flat(&out);
        let frame = |i: usize| (got[i * 2], got[i * 2 + 1]);

        assert!(
            (0..240).all(|i| frame(i) == (sys_only(1000), sys_only(1000))),
            "mic has not started yet"
        );
        let sum = both(1000, 500);
        assert!((240..480).all(|i| frame(i) == (sum, sum)), "both overlap");
        assert!(
            (480..720).all(|i| frame(i) == (mic_only(500), mic_only(500))),
            "system audio has ended"
        );
    }

    #[test]
    fn a_later_source_does_not_extend_the_window() {
        let out = mix(
            &[stereo_pkt(0, 4800, 100, 100)],
            &[mono_pkt(4800, 480, 100)],
            WIDE,
        );
        assert_eq!(total_frames(&out), 5280);
    }

    #[test]
    fn a_lone_stale_packet_cannot_blow_up_the_window() {
        let stale = mono_pkt(0, 480, 9000);
        let live = stereo_pkt(OUT_RATE as i64 * 600 + 480, 4800, 100, 100);
        let out = mix(&[live], &[stale], OUT_RATE as i64 * 61);
        assert_eq!(
            total_frames(&out),
            OUT_RATE as i64 * 61,
            "the window is the cap, not the distance back to the stale packet"
        );
    }

    #[test]
    fn a_recent_lone_packet_does_not_shorten_the_window() {
        let lone = mono_pkt(OUT_RATE as i64 * 600, 480, 9000);
        let live = stereo_pkt(OUT_RATE as i64 * 600 + 480, 4800, 100, 100);
        let out = mix(&[live], &[lone], OUT_RATE as i64 * 61);
        assert_eq!(total_frames(&out), 5280, "the union, not the cap");
    }

    #[test]
    fn everything_outside_the_cap_is_dropped() {
        let old = stereo_pkt(0, 480, 5000, 5000);
        let new = stereo_pkt(OUT_RATE as i64 * 30, 480, 100, 100);
        let out = mix(&[old, new], &[], OUT_RATE as i64 * 10);
        assert_eq!(total_frames(&out), OUT_RATE as i64 * 10);
        let got = flat(&out);
        assert!(
            !got.contains(&5000),
            "a packet entirely before the cap must not leak in"
        );
        let live_frames = OUT_RATE as i64 * 10 - 480;
        assert!(
            got[..live_frames as usize * 2].iter().all(|&s| s == 0),
            "the gap before the live packet is silence"
        );
        assert!(
            got[live_frames as usize * 2..].iter().all(|&s| s == 100),
            "system-only content passes at unity gain"
        );
    }

    #[test]
    fn output_pts_are_contiguous_and_ascending() {
        let sys: Vec<RingAudio> = (0..10)
            .map(|i| stereo_pkt(i * 480, 480, 300, -300))
            .collect();
        let mic: Vec<RingAudio> = (0..10).map(|i| mono_pkt(i * 480 + 120, 480, 100)).collect();
        let out = mix(&sys, &mic, WIDE);
        for w in out.windows(2) {
            assert_eq!(
                w[1].pts100ns - w[0].pts100ns,
                pts(480),
                "chunks must be evenly spaced"
            );
        }
        assert_eq!(total_frames(&out), 4920, "union of [0,4800) and [120,4920)");
    }

    #[test]
    fn unsorted_inputs_are_mixed_correctly() {
        let sys = vec![stereo_pkt(960, 480, 400, 400), stereo_pkt(0, 480, 100, 100)];
        let mic = vec![mono_pkt(240, 240, 50)];
        let out = mix(&sys, &mic, WIDE);
        assert_eq!(total_frames(&out), 1440);
        let got = flat(&out);
        let at = |f: usize| got[f * 2];
        assert_eq!(at(0), sys_only(100), "system only");
        assert_eq!(at(300), both(100, 50), "overlap");
        assert_eq!(at(600), 0, "neither source covers this frame");
        assert_eq!(at(1000), sys_only(400), "later system packet");
    }

    #[test]
    fn block_boundaries_do_not_split_or_drop_samples() {
        let frames = MIX_BLOCK_FRAMES * 2 + 733;
        let out = mix(&[stereo_pkt(0, frames as u32, 4321, -4321)], &[], WIDE);
        assert_eq!(total_frames(&out), frames);

        assert_eq!(out[0].frames as usize, MIX_CHUNK_FRAMES);
        let last = out.last().unwrap();
        assert_eq!(last.frames as usize, frames as usize % MIX_CHUNK_FRAMES);
        assert_eq!(out.len(), frames as usize / MIX_CHUNK_FRAMES + 1);
    }

    #[test]
    fn a_short_tail_becomes_its_own_chunk() {
        let out = mix(&[stereo_pkt(0, 100, 4321, -4321)], &[], WIDE);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].frames, 100);
    }

    #[test]
    fn truncated_packet_bytes_are_ignored_not_read_out_of_bounds() {
        let mut sys = stereo_pkt(0, 480, 500, 500);
        sys.data.truncate(sys.data.len() - 3);
        let out = mix(&[sys], &[mono_pkt(0, 480, 500)], WIDE);
        assert_eq!(total_frames(&out), 480);
        let got = flat(&out);
        assert_eq!(got.len(), 960);
        let whole = both(500, 500);
        assert!(
            got[..958].iter().all(|&s| s == whole),
            "every whole frame sums both sources"
        );
        assert_eq!(
            got[958],
            mic_only(500),
            "the last frame lost its right sample, so only the mic lands"
        );
    }

    #[test]
    fn a_zero_frame_packet_is_skipped() {
        let out = mix(&[stereo_pkt(0, 0, 1, 1)], &[mono_pkt(0, 480, 700)], WIDE);
        assert_eq!(total_frames(&out), 480);
        assert_eq!(all(&out, 700).len(), 960);
    }

    #[test]
    fn mono_resampler_emits_one_sample_per_frame() {
        let mut r = CubicResampler::new();
        assert!(r.init_mono(48_000, 1, 16));
        assert_eq!(r.output_bytes_per_frame(), 2);
        assert!(!CubicResampler::passthrough(48_000, 1, 16));

        let input = pcm16(&[1000i16; 4_800]);
        let mut out = Vec::new();
        assert!(r.process(&input, &mut out));
        assert!(!out.is_empty());
        assert_eq!(out.len() % 2, 0);
        let produced = out.len() / 2;
        assert!(
            (4_400..=4_900).contains(&produced),
            "1/100 s of 48 kHz in, about 4800 out, got {produced}"
        );
    }

    #[test]
    fn a_stereo_resampler_is_not_mono_after_init_mono() {
        let mut r = CubicResampler::new();
        assert!(r.init_mono(44_100, 2, 16));
        assert_eq!(r.output_bytes_per_frame(), 2);
        assert!(r.init(44_100, 2, 16));
        assert_eq!(r.output_bytes_per_frame(), 4, "init resets mono mode");
    }

    #[test]
    fn stereo_resampler_still_emits_two_samples_per_frame() {
        let mut r = CubicResampler::new();
        assert!(r.init(48_000, 2, 16));
        assert_eq!(r.output_bytes_per_frame(), 4);

        let input = pcm16(&vec![1000i16; 9_600 * 2]);
        assert_eq!(input.len(), 9_600 * 4, "9600 stereo frames");
        let mut out = Vec::new();
        assert!(r.process(&input, &mut out));
        assert_eq!(out.len() % 4, 0);
        let produced = out.len() / 4;
        assert!(
            (9_500..9_600).contains(&produced),
            "48 kHz to 48 kHz must be near 1:1, got {produced}"
        );
    }

    #[test]
    fn stereo_passthrough_is_still_recognised() {
        assert!(CubicResampler::passthrough(48_000, 2, 16));
        assert!(!CubicResampler::passthrough(44_100, 2, 16));
        assert!(!CubicResampler::passthrough(48_000, 2, 24));
    }
}
