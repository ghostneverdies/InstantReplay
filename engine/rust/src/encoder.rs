use rayon::prelude::*;
use std::time::Instant;


#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EncoderPref {
    Auto,
    Hardware,
    Software,
}

impl EncoderPref {
    pub fn from_i32(v: i32) -> Self {
        match v {
            1 => EncoderPref::Hardware,
            2 => EncoderPref::Software,
            _ => EncoderPref::Auto,
        }
    }

    pub fn resolved(self) -> Self {
        self.with_env_override()
    }

    fn with_env_override(self) -> Self {
        match std::env::var("IR_ENCODER").ok().as_deref() {
            Some("sw") | Some("software") => EncoderPref::Software,
            Some("hw") | Some("hardware") => EncoderPref::Hardware,
            Some("auto") => EncoderPref::Auto,
            _ => self,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EncoderKind {
    Software = 1,
    Hardware = 2,
    HardwareGpu = 4,
}

const HW_FAIL_LIMIT: u32 = 8;

#[derive(Clone, Copy, PartialEq, Eq)]
enum MfFlavour {
    Hardware,
    Software,
}

enum Inner {
    Mf(mf::MfEncoder),
}

pub struct VideoEncoder {
    inner: Inner,
    w: i32,
    h: i32,
    fps: i32,
    bps: u32,
    consecutive_err: u32,
    frames_since_key: u32,
    convert_ns: i64,
    pub hw_init_hr: i32,
}

impl VideoEncoder {
    pub fn create(pref: EncoderPref, w: i32, h: i32, fps: i32, bps: u32) -> Result<Self, i32> {
        let pref = pref.with_env_override();
        let mut hw_init_hr = 0;

        if pref != EncoderPref::Software {
            match mf::MfEncoder::create(w as u32, h as u32, fps as u32, bps) {
                Ok(m) => {
                    return Ok(VideoEncoder {
                        inner: Inner::Mf(m),
                        w,
                        h,
                        fps,
                        bps,
                        consecutive_err: 0,
                        frames_since_key: 0,
                        convert_ns: 0,
                        hw_init_hr: 0,
                    });
                }
                Err(hr) => hw_init_hr = hr,
            }
        }

        let sw = mf::MfEncoder::create_software(w as u32, h as u32, fps as u32, bps)?;
        Ok(VideoEncoder {
            inner: Inner::Mf(sw),
            w,
            h,
            fps,
            bps,
            consecutive_err: 0,
            frames_since_key: 0,
            convert_ns: 0,
            hw_init_hr,
        })
    }

    pub fn kind(&self) -> EncoderKind {
        match &self.inner {
            Inner::Mf(m) => match m.flavour() {
                MfFlavour::Hardware => EncoderKind::Hardware,
                MfFlavour::Software => EncoderKind::Software,
            },
        }
    }

    pub fn flush(&mut self, _out: &mut Vec<EncodedFrame>) {}

    pub fn force_idr(&mut self) {
        match &mut self.inner {
            Inner::Mf(m) => m.force_idr(),
        }
    }

    pub fn take_convert_ns(&mut self) -> i64 {
        std::mem::take(&mut self.convert_ns)
    }

    pub fn encode_bgra(
        &mut self,
        bgra: &[u8],
        src_w: u32,
        src_h: u32,
        pitch: u32,
        pts100ns: i64,
        out: &mut Vec<EncodedFrame>,
    ) -> Result<(), i32> {
        if self.frames_since_key >= (self.fps.max(1) as u32) * 2 {
            self.force_idr();
            self.frames_since_key = 0;
        }

        let before = out.len();
        let r = match &mut self.inner {
            Inner::Mf(m) => {
                let r = m.encode(bgra, src_w, src_h, pitch, pts100ns, out);
                self.convert_ns += std::mem::take(&mut m.convert_ns);
                r
            }
        };

        for f in &out[before..] {
            if f.keyframe {
                self.frames_since_key = 0;
            }
        }
        self.frames_since_key = self.frames_since_key.saturating_add(1);

        match r {
            Ok(()) => {
                self.consecutive_err = 0;
                Ok(())
            }
            Err(code) => {
                if self.kind() == EncoderKind::Hardware {
                    self.consecutive_err += 1;
                    if self.consecutive_err >= HW_FAIL_LIMIT {
                        if let Ok(sw) = mf::MfEncoder::create_software(
                            self.w as u32,
                            self.h as u32,
                            self.fps as u32,
                            self.bps,
                        ) {
                            self.inner = Inner::Mf(sw);
                            self.hw_init_hr = code;
                            self.consecutive_err = 0;
                            self.frames_since_key = self.fps.max(1) as u32 * 2;
                        }
                    }
                }
                Err(code)
            }
        }
    }
}


mod mf {
    use super::{bgra_to_nv12, EncodedFrame};
    use std::mem::ManuallyDrop;
    use std::time::{Duration, Instant};
    use windows::core::{GUID, Interface};
    use windows::Win32::Graphics::Direct3D11::ID3D11Device;
    use windows::Win32::Graphics::Dxgi::IDXGIDevice;
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_MULTITHREADED};

    type R<T> = Result<T, i32>;

    pub type Sample = IMFSample;

    fn e(err: windows::core::Error) -> i32 {
        err.code().0
    }

    unsafe fn adapter_luid(dev: &ID3D11Device) -> R<[u8; 8]> {
        let dxgi: IDXGIDevice = dev.cast().map_err(e)?;
        let adapter = dxgi.GetAdapter().map_err(e)?;
        let desc = adapter.GetDesc().map_err(e)?;
        let mut b = [0u8; 8];
        b[..4].copy_from_slice(&desc.AdapterLuid.LowPart.to_le_bytes());
        b[4..].copy_from_slice(&(desc.AdapterLuid.HighPart as u32).to_le_bytes());
        Ok(b)
    }

    pub fn surface_sample(tex: &windows::Win32::Graphics::Direct3D11::ID3D11Texture2D) -> R<Sample> {
        unsafe {
            let buf = MFCreateDXGISurfaceBuffer(&windows::Win32::Graphics::Direct3D11::ID3D11Texture2D::IID, tex, 0, false)
                .map_err(e)?;
            let max = buf.GetMaxLength().map_err(e)?;
            buf.SetCurrentLength(max).map_err(e)?;
            let s = MFCreateSample().map_err(e)?;
            s.AddBuffer(&buf).map_err(e)?;
            Ok(s)
        }
    }

    fn pack(hi: u32, lo: u32) -> u64 {
        ((hi as u64) << 32) | lo as u64
    }

    struct MfRuntime {
        com: bool,
        mf: bool,
    }

    impl MfRuntime {
        fn init() -> R<Self> {
            unsafe {
                let com = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
                let mut rt = MfRuntime { com, mf: false };
                MFStartup(MF_VERSION, MFSTARTUP_FULL).map_err(e)?;
                rt.mf = true;
                Ok(rt)
            }
        }
    }

    impl Drop for MfRuntime {
        fn drop(&mut self) {
            unsafe {
                if self.mf {
                    let _ = MFShutdown();
                }
                if self.com {
                    CoUninitialize();
                }
            }
        }
    }

    pub struct MfEncoder {
        transform: IMFTransform,
        events: Option<IMFMediaEventGenerator>,
        codec: Option<ICodecAPI>,
        flavour: super::MfFlavour,
        provides_samples: bool,
        out_buf_size: u32,
        pending_inputs: u32,
        width: u32,
        height: u32,
        frame_dur: i64,
        tmp_i420: Vec<u8>,
        seq_header: Vec<u8>,
        pub convert_ns: i64,
        submitted: u64,
        received: u64,
        _mgr: Option<IMFDXGIDeviceManager>,
        _rt: MfRuntime,
    }

    unsafe fn codec_u32(c: &ICodecAPI, key: &GUID, v: u32) {
        let var = VARIANT::from(v);
        let _ = c.SetValue(key, &var);
    }

    fn scan_nals(d: &[u8]) -> (bool, bool) {
        let (mut sps, mut idr) = (false, false);
        let mut i = 0;
        while i + 3 < d.len() {
            if d[i] == 0 && d[i + 1] == 0 && d[i + 2] == 1 {
                match d[i + 3] & 0x1F {
                    7 => sps = true,
                    5 => idr = true,
                    _ => {}
                }
                i += 3;
            } else {
                i += 1;
            }
        }
        (sps, idr)
    }

    impl MfEncoder {
        pub fn create(w: u32, h: u32, fps: u32, bps: u32) -> R<Self> {
            Self::create_inner(w, h, fps, bps, None, true)
        }

        pub fn create_software(w: u32, h: u32, fps: u32, bps: u32) -> R<Self> {
            Self::create_inner(w, h, fps, bps, None, false)
        }

        pub fn create_gpu(device: &ID3D11Device, w: u32, h: u32, fps: u32, bps: u32) -> R<Self> {
            Self::create_inner(w, h, fps, bps, Some(device), true)
        }

        fn create_inner(
            w: u32,
            h: u32,
            fps: u32,
            bps: u32,
            device: Option<&ID3D11Device>,
            want_hardware: bool,
        ) -> R<Self> {
            let w = w & !1;
            let h = h & !1;
            let rt = MfRuntime::init()?;
            unsafe {
                let in_info = MFT_REGISTER_TYPE_INFO {
                    guidMajorType: MFMediaType_Video,
                    guidSubtype: MFVideoFormat_NV12,
                };
                let out_info = MFT_REGISTER_TYPE_INFO {
                    guidMajorType: MFMediaType_Video,
                    guidSubtype: MFVideoFormat_H264,
                };
                let mut acts: *mut Option<IMFActivate> = std::ptr::null_mut();
                let mut count = 0u32;
                let mut mgr: Option<IMFDXGIDeviceManager> = None;
                let hw = if want_hardware { MFT_ENUM_FLAG_HARDWARE } else { MFT_ENUM_FLAG(0) };
                match device {
                    None => {
                        MFTEnumEx(
                            MFT_CATEGORY_VIDEO_ENCODER,
                            hw | MFT_ENUM_FLAG_SORTANDFILTER,
                            Some(&in_info as *const MFT_REGISTER_TYPE_INFO),
                            Some(&out_info as *const MFT_REGISTER_TYPE_INFO),
                            &mut acts,
                            &mut count,
                        )
                        .map_err(e)?;
                    }
                    Some(dev) => {
                        let luid = adapter_luid(dev)?;
                        let mut attrs: Option<IMFAttributes> = None;
                        MFCreateAttributes(&mut attrs, 1).map_err(e)?;
                        let attrs = attrs.ok_or(-121)?;
                        attrs.SetBlob(&MFT_ENUM_ADAPTER_LUID, &luid).map_err(e)?;
                        MFTEnum2(
                            MFT_CATEGORY_VIDEO_ENCODER,
                            MFT_ENUM_FLAG_ASYNCMFT | hw | MFT_ENUM_FLAG_SORTANDFILTER,
                            Some(&in_info as *const MFT_REGISTER_TYPE_INFO),
                            Some(&out_info as *const MFT_REGISTER_TYPE_INFO),
                            &attrs,
                            &mut acts,
                            &mut count,
                        )
                        .map_err(e)?;

                        let mut token = 0u32;
                        MFCreateDXGIDeviceManager(&mut token, &mut mgr).map_err(e)?;
                        mgr.as_ref().ok_or(-122)?.ResetDevice(dev, token).map_err(e)?;
                    }
                }

                let mut candidates: Vec<IMFActivate> = Vec::new();
                if !acts.is_null() {
                    let list = std::slice::from_raw_parts_mut(acts, count as usize);
                    for slot in list.iter_mut() {
                        if let Some(a) = slot.take() {
                            candidates.push(a);
                        }
                    }
                    CoTaskMemFree(Some(acts as *const std::ffi::c_void));
                }
                if candidates.is_empty() {
                    return Err(-100);
                }

                let flavour = if want_hardware {
                    super::MfFlavour::Hardware
                } else {
                    super::MfFlavour::Software
                };

                let mut last_err = -101;
                for act in &candidates {
                    match Self::open(act, w, h, fps, bps, mgr.as_ref(), flavour) {
                        Ok(mut enc) => {
                            enc._rt = rt;
                            enc._mgr = mgr;
                            return Ok(enc);
                        }
                        Err(code) => last_err = code,
                    }
                }
                Err(last_err)
            }
        }

        unsafe fn open(
            act: &IMFActivate,
            w: u32,
            h: u32,
            fps: u32,
            bps: u32,
            mgr: Option<&IMFDXGIDeviceManager>,
            flavour: super::MfFlavour,
        ) -> R<Self> {
            let transform: IMFTransform = act.ActivateObject().map_err(e)?;

            let attrs = transform.GetAttributes().map_err(e)?;
            let _ = attrs.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1);
            let _ = attrs.SetUINT32(&MF_LOW_LATENCY, 1);

            if let Some(m) = mgr {
                transform
                    .ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, m.as_raw() as usize)
                    .map_err(e)?;
            }

            let codec = transform.cast::<ICodecAPI>().ok();
            if let Some(c) = &codec {
                codec_u32(c, &CODECAPI_AVLowLatencyMode, 1);
                codec_u32(c, &CODECAPI_AVEncCommonRateControlMode, 0);
                codec_u32(c, &CODECAPI_AVEncCommonMeanBitRate, bps);
                codec_u32(c, &CODECAPI_AVEncMPVGOPSize, fps.max(1));
                codec_u32(c, &CODECAPI_AVEncMPVDefaultBPictureCount, 0);
            }

            let out_t = MFCreateMediaType().map_err(e)?;
            out_t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(e)?;
            out_t.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264).map_err(e)?;
            out_t.SetUINT32(&MF_MT_AVG_BITRATE, bps).map_err(e)?;
            out_t.SetUINT64(&MF_MT_FRAME_SIZE, pack(w, h)).map_err(e)?;
            out_t.SetUINT64(&MF_MT_FRAME_RATE, pack(fps.max(1), 1)).map_err(e)?;
            out_t.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1)).map_err(e)?;
            out_t
                .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
                .map_err(e)?;
            let _ = out_t.SetUINT32(&MF_MT_MPEG2_PROFILE, 77);
            let _ = out_t.SetUINT32(&MF_MT_MAX_KEYFRAME_SPACING, fps.max(1));
            if mgr.is_some() {
                let hd = w >= 1280 || h > 576;
                let _ = out_t.SetUINT32(&MF_MT_VIDEO_CHROMA_SITING, 5);
                let _ = out_t.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, 2);
                let _ = out_t.SetUINT32(&MF_MT_VIDEO_PRIMARIES, if hd { 2 } else { 3 });
                let _ = out_t.SetUINT32(&MF_MT_YUV_MATRIX, if hd { 1 } else { 2 });
                let _ = out_t.SetUINT32(&MF_MT_TRANSFER_FUNCTION, 5);
            }
            transform.SetOutputType(0, &out_t, 0).map_err(e)?;

            let in_t = MFCreateMediaType().map_err(e)?;
            in_t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(e)?;
            in_t.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12).map_err(e)?;
            in_t.SetUINT64(&MF_MT_FRAME_SIZE, pack(w, h)).map_err(e)?;
            in_t.SetUINT64(&MF_MT_FRAME_RATE, pack(fps.max(1), 1)).map_err(e)?;
            in_t.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1)).map_err(e)?;
            in_t
                .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
                .map_err(e)?;
            transform.SetInputType(0, &in_t, 0).map_err(e)?;

            let info = transform.GetOutputStreamInfo(0).map_err(e)?;
            let provides_samples = (info.dwFlags & 0x100) != 0;
            let out_buf_size = info.cbSize.max(1 << 20);

            let events: Option<IMFMediaEventGenerator> = transform.cast().ok();
            if events.is_none() {
                eprintln!("[mf] encoder has no IMFMediaEventGenerator: using synchronous pump");
            }

            transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)
                .map_err(e)?;
            transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)
                .map_err(e)?;

            let seq_header = Self::read_seq_header(&transform);

            Ok(MfEncoder {
                transform,
                events,
                codec,
                flavour,
                provides_samples,
                out_buf_size,
                pending_inputs: 0,
                width: w,
                height: h,
                frame_dur: 10_000_000 / fps.max(1) as i64,
                tmp_i420: Vec::new(),
                seq_header,
                convert_ns: 0,
                submitted: 0,
                received: 0,
                _mgr: None,
                _rt: MfRuntime { com: false, mf: false },
            })
        }

        unsafe fn read_seq_header(t: &IMFTransform) -> Vec<u8> {
            if let Ok(ty) = t.GetOutputCurrentType(0) {
                if let Ok(n) = ty.GetBlobSize(&MF_MT_MPEG_SEQUENCE_HEADER) {
                    let mut b = vec![0u8; n as usize];
                    if n > 4 && ty.GetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &mut b, None).is_ok() {
                        if b.starts_with(&[0, 0, 0, 1]) || b.starts_with(&[0, 0, 1]) {
                            return b;
                        }
                    }
                }
            }
            Vec::new()
        }

        pub fn force_idr(&mut self) {
            if let Some(c) = &self.codec {
                unsafe { codec_u32(c, &CODECAPI_AVEncVideoForceKeyFrame, 1) };
            }
        }

        pub fn flavour(&self) -> super::MfFlavour {
            self.flavour
        }

        pub fn encode(
            &mut self,
            bgra: &[u8],
            src_w: u32,
            src_h: u32,
            pitch: u32,
            pts100ns: i64,
            out: &mut Vec<EncodedFrame>,
        ) -> R<()> {
            unsafe {
                self.pump(out)?;

                let sync = self.events.is_none();
                if !sync {
                    let t0 = Instant::now();
                    while self.pending_inputs == 0 {
                        if t0.elapsed() > Duration::from_millis(200) {
                            return Err(-102);
                        }
                        std::thread::sleep(Duration::from_micros(500));
                        self.pump(out)?;
                    }
                }

                let len = (self.width as usize) * (self.height as usize) * 3 / 2;
                let mb = MFCreateMemoryBuffer(len as u32).map_err(e)?;
                let mut p: *mut u8 = std::ptr::null_mut();
                mb.Lock(&mut p, None, None).map_err(e)?;
                let dst = std::slice::from_raw_parts_mut(p, len);
                let t = Instant::now();
                let ok = bgra_to_nv12(bgra, src_w, src_h, pitch, self.width, self.height, dst, &mut self.tmp_i420);
                self.convert_ns += t.elapsed().as_nanos() as i64;
                let _ = mb.Unlock();
                if !ok {
                    return Err(-3);
                }
                mb.SetCurrentLength(len as u32).map_err(e)?;

                let sample = MFCreateSample().map_err(e)?;
                sample.AddBuffer(&mb).map_err(e)?;
                sample.SetSampleTime(pts100ns).map_err(e)?;
                sample.SetSampleDuration(self.frame_dur).map_err(e)?;
                self.transform.ProcessInput(0, &sample, 0).map_err(e)?;
                if !sync {
                    self.pending_inputs -= 1;
                }
                self.submitted += 1;

                if sync {
                    self.drain_sync(out)
                } else {
                    self.pump(out)
                }
            }
        }

        unsafe fn drain_sync(&mut self, out: &mut Vec<EncodedFrame>) -> R<()> {
            loop {
                if !self.drain_output(out)? {
                    return Ok(());
                }
            }
        }

        pub fn encode_surface(&mut self, sample: &Sample, pts100ns: i64, out: &mut Vec<EncodedFrame>) -> R<()> {
            unsafe {
                sample.SetSampleTime(pts100ns).map_err(e)?;
                sample.SetSampleDuration(self.frame_dur).map_err(e)?;

                self.pump(out)?;
                let t0 = Instant::now();
                while self.pending_inputs == 0 {
                    if t0.elapsed() > Duration::from_millis(200) {
                        return Err(-102);
                    }
                    std::thread::sleep(Duration::from_micros(500));
                    self.pump(out)?;
                }
                self.transform.ProcessInput(0, sample, 0).map_err(e)?;
                self.pending_inputs -= 1;
                self.submitted += 1;
                self.pump(out)
            }
        }

        pub fn wait_room(&mut self, max_inflight: u64, out: &mut Vec<EncodedFrame>) -> R<()> {
            unsafe {
                let t0 = Instant::now();
                loop {
                    self.pump(out)?;
                    if self.submitted.saturating_sub(self.received) < max_inflight {
                        return Ok(());
                    }
                    if t0.elapsed() > Duration::from_millis(50) {
                        return Err(-104);
                    }
                    std::thread::sleep(Duration::from_micros(500));
                }
            }
        }

        unsafe fn pump(&mut self, out: &mut Vec<EncodedFrame>) -> R<()> {
            if self.events.is_none() {
                return Ok(());
            }
            loop {
                let ev = match self.events.as_ref().expect("checked above").GetEvent(MF_EVENT_FLAG_NO_WAIT) {
                    Ok(ev) => ev,
                    Err(er) => {
                        if er.code() == MF_E_NO_EVENTS_AVAILABLE {
                            return Ok(());
                        }
                        return Err(e(er));
                    }
                };
                let ty = ev.GetType().map_err(e)?;
                if ty == METransformNeedInput.0 as u32 {
                    self.pending_inputs += 1;
                } else if ty == METransformHaveOutput.0 as u32 {
                    self.drain_output(out)?;
                }
            }
        }

        unsafe fn drain_output(&mut self, out: &mut Vec<EncodedFrame>) -> R<bool> {
            let mut buf = MFT_OUTPUT_DATA_BUFFER {
                dwStreamID: 0,
                pSample: ManuallyDrop::new(None),
                dwStatus: 0,
                pEvents: ManuallyDrop::new(None),
            };
            if !self.provides_samples {
                let s = MFCreateSample().map_err(e)?;
                let mb = MFCreateMemoryBuffer(self.out_buf_size).map_err(e)?;
                s.AddBuffer(&mb).map_err(e)?;
                buf.pSample = ManuallyDrop::new(Some(s));
            }
            let mut status = 0u32;
            let r = self
                .transform
                .ProcessOutput(0, std::slice::from_mut(&mut buf), &mut status);
            let sample = ManuallyDrop::take(&mut buf.pSample);
            ManuallyDrop::drop(&mut buf.pEvents);
            if let Err(er) = r {
                if er.code() == MF_E_TRANSFORM_NEED_MORE_INPUT {
                    return Ok(false);
                }
                return Err(e(er));
            }
            let Some(sample) = sample else {
                return Ok(false);
            };

            let mb = sample.ConvertToContiguousBuffer().map_err(e)?;
            let mut p: *mut u8 = std::ptr::null_mut();
            let mut len = 0u32;
            mb.Lock(&mut p, None, Some(&mut len as *mut u32)).map_err(e)?;
            let mut data = if p.is_null() {
                Vec::new()
            } else {
                std::slice::from_raw_parts(p, len as usize).to_vec()
            };
            let _ = mb.Unlock();
            if data.is_empty() {
                return Ok(false);
            }

            let pts = sample.GetSampleTime().unwrap_or(0);
            let (has_sps, has_idr) = scan_nals(&data);
            let key = has_idr || sample.GetUINT32(&MFSampleExtension_CleanPoint).unwrap_or(0) != 0;

            if key && !has_sps {
                if self.seq_header.is_empty() {
                    self.seq_header = Self::read_seq_header(&self.transform);
                }
                if !self.seq_header.is_empty() {
                    let mut with = self.seq_header.clone();
                    with.extend_from_slice(&data);
                    data = with;
                }
            }

            self.received += 1;
            out.push(EncodedFrame {
                data,
                keyframe: key,
                pts100ns: pts,
            });
            Ok(true)
        }
    }

    impl Drop for MfEncoder {
        fn drop(&mut self) {
            unsafe {
                let _ = self.transform.ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
                let _ = self.transform.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0);
            }
        }
    }
}


const GPU_RING: usize = 8;

pub struct GpuInput {
    pub device: windows::Win32::Graphics::Direct3D11::ID3D11Device,
    pub context: windows::Win32::Graphics::Direct3D11::ID3D11DeviceContext,
    mt: windows::Win32::Graphics::Direct3D11::ID3D11Multithread,
    latest: windows::Win32::Graphics::Direct3D11::ID3D11Texture2D,
    pub w: u32,
    pub h: u32,
}

impl GpuInput {
    pub fn create(
        device: &windows::Win32::Graphics::Direct3D11::ID3D11Device,
        context: &windows::Win32::Graphics::Direct3D11::ID3D11DeviceContext,
        w: u32,
        h: u32,
    ) -> Result<Self, i32> {
        gpu::create_input(device, context, w, h)
    }

    pub fn submit(&self, frame: &windows::Win32::Graphics::Direct3D11::ID3D11Texture2D) {
        gpu::submit(self, frame)
    }
}

pub struct GpuEncoder {
    samples: Vec<mf::Sample>,
    conv: gpu::Converter,
    mf: mf::MfEncoder,
    input: std::sync::Arc<GpuInput>,
    counter: u64,
    convert_ns: i64,
}

impl GpuEncoder {
    pub fn create(
        input: std::sync::Arc<GpuInput>,
        out_w: i32,
        out_h: i32,
        fps: i32,
        bps: u32,
        improved: bool,
    ) -> Result<Self, i32> {
        let (w, h) = ((out_w as u32) & !1, (out_h as u32) & !1);
        let mf = mf::MfEncoder::create_gpu(&input.device, w, h, fps.max(1) as u32, bps)?;
        let conv = gpu::Converter::create(&input, w, h, improved)?;
        let mut samples = Vec::with_capacity(GPU_RING);
        for o in conv.outputs() {
            samples.push(mf::surface_sample(&o.tex)?);
        }
        Ok(GpuEncoder { samples, conv, mf, input, counter: 0, convert_ns: 0 })
    }

    pub fn device_differs(&self, other: &GpuInput) -> bool {
        use windows::core::Interface;
        self.input.device.as_raw() != other.device.as_raw()
    }

    pub fn rebind(&mut self, input: std::sync::Arc<GpuInput>) -> Result<(), i32> {
        self.conv.rebind(&input)?;
        self.input = input;
        Ok(())
    }

    pub fn force_idr(&mut self) {
        self.mf.force_idr();
    }

    pub fn take_convert_ns(&mut self) -> i64 {
        std::mem::take(&mut self.convert_ns)
    }

    pub fn encode(&mut self, pts100ns: i64, out: &mut Vec<EncodedFrame>) -> Result<(), i32> {
        self.mf.wait_room(GPU_RING as u64 - 1, out)?;
        let idx = (self.counter % GPU_RING as u64) as usize;
        let t = Instant::now();
        self.conv.run(idx)?;
        self.convert_ns += t.elapsed().as_nanos() as i64;
        self.counter += 1;
        self.mf.encode_surface(&self.samples[idx], pts100ns, out)
    }
}

mod gpu {
    use super::{GpuInput, GPU_RING};
    use std::ffi::c_void;
    use windows::core::{s, Interface, PCSTR};
    use windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;
    use windows::Win32::Graphics::Direct3D::{ID3DBlob, D3D_SRV_DIMENSION_TEXTURE2D};
    use windows::Win32::Graphics::Direct3D11::*;
    use windows::Win32::Graphics::Dxgi::Common::*;

    type R<T> = Result<T, i32>;

    fn e(er: windows::core::Error) -> i32 {
        er.code().0
    }

    const HLSL: &str = include_str!("../shaders/wcap_shaders.hlsl");

    const BT709: [[f32; 4]; 6] = [
        [0.2126, 0.7152, 0.0722, 0.0],
        [-0.1146, -0.3854, 0.5, 0.0],
        [0.5, -0.4542, -0.0458, 0.0],
        [1.0, 0.0, 1.5748, 0.0],
        [1.0, -0.1873, -0.4681, 0.0],
        [1.0, 1.8556, 0.0, 0.0],
    ];
    const BT601: [[f32; 4]; 6] = [
        [0.299, 0.587, 0.114, 0.0],
        [-0.168736, -0.331264, 0.5, 0.0],
        [0.5, -0.418688, -0.081312, 0.0],
        [1.0, 0.0, 1.402, 0.0],
        [1.0, -0.344136, -0.714136, 0.0],
        [1.0, 1.772, 0.0, 0.0],
    ];

    fn div_up(a: u32, b: u32) -> u32 {
        (a + b - 1) / b
    }

    unsafe fn compile(dev: &ID3D11Device, entry: PCSTR) -> R<ID3D11ComputeShader> {
        let mut code: Option<ID3DBlob> = None;
        let mut errs: Option<ID3DBlob> = None;
        D3DCompile(
            HLSL.as_ptr() as *const c_void,
            HLSL.len(),
            s!("wcap_shaders.hlsl"),
            None,
            None,
            entry,
            s!("cs_5_0"),
            1 << 15,
            0,
            &mut code,
            Some(&mut errs as *mut Option<ID3DBlob>),
        )
        .map_err(e)?;
        let blob = code.ok_or(-130)?;
        let bytes = std::slice::from_raw_parts(blob.GetBufferPointer() as *const u8, blob.GetBufferSize());
        let mut cs: Option<ID3D11ComputeShader> = None;
        dev.CreateComputeShader(bytes, None, Some(&mut cs as *mut Option<ID3D11ComputeShader>))
            .map_err(e)?;
        cs.ok_or(-131)
    }

    unsafe fn texture(dev: &ID3D11Device, w: u32, h: u32, fmt: DXGI_FORMAT, bind: u32) -> R<ID3D11Texture2D> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: w,
            Height: h,
            MipLevels: 1,
            ArraySize: 1,
            Format: fmt,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: bind,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let mut t: Option<ID3D11Texture2D> = None;
        dev.CreateTexture2D(&desc, None, Some(&mut t as *mut Option<ID3D11Texture2D>))
            .map_err(e)?;
        t.ok_or(-132)
    }

    unsafe fn srv(dev: &ID3D11Device, tex: &ID3D11Texture2D, fmt: DXGI_FORMAT) -> R<ID3D11ShaderResourceView> {
        let desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
            Format: fmt,
            ViewDimension: D3D_SRV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_SRV { MostDetailedMip: 0, MipLevels: 1 },
            },
        };
        let mut v: Option<ID3D11ShaderResourceView> = None;
        dev.CreateShaderResourceView(tex, Some(&desc), Some(&mut v as *mut Option<ID3D11ShaderResourceView>))
            .map_err(e)?;
        v.ok_or(-133)
    }

    unsafe fn uav(dev: &ID3D11Device, tex: &ID3D11Texture2D, fmt: DXGI_FORMAT) -> R<ID3D11UnorderedAccessView> {
        let desc = D3D11_UNORDERED_ACCESS_VIEW_DESC {
            Format: fmt,
            ViewDimension: D3D11_UAV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_UNORDERED_ACCESS_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_UAV { MipSlice: 0 },
            },
        };
        let mut v: Option<ID3D11UnorderedAccessView> = None;
        dev.CreateUnorderedAccessView(tex, Some(&desc), Some(&mut v as *mut Option<ID3D11UnorderedAccessView>))
            .map_err(e)?;
        v.ok_or(-134)
    }

    pub fn create_input(dev: &ID3D11Device, ctx: &ID3D11DeviceContext, w: u32, h: u32) -> R<GpuInput> {
        unsafe {
            let mt: ID3D11Multithread = ctx.cast().map_err(e)?;
            let _ = mt.SetMultithreadProtected(true);
            let latest = texture(dev, w, h, DXGI_FORMAT_B8G8R8A8_TYPELESS, D3D11_BIND_SHADER_RESOURCE.0 as u32)?;
            Ok(GpuInput { device: dev.clone(), context: ctx.clone(), mt, latest, w, h })
        }
    }

    pub fn submit(g: &GpuInput, frame: &ID3D11Texture2D) {
        unsafe {
            g.mt.Enter();
            g.context.CopyResource(&g.latest, frame);
            g.mt.Leave();
        }
    }

    pub struct Nv12Out {
        pub tex: ID3D11Texture2D,
        uav_y: ID3D11UnorderedAccessView,
        uav_uv: ID3D11UnorderedAccessView,
        srv_uv: ID3D11ShaderResourceView,
    }

    struct Resize {
        mid_srv: ID3D11ShaderResourceView,
        mid_uav: ID3D11UnorderedAccessView,
        out_srv: ID3D11ShaderResourceView,
        out_uav: ID3D11UnorderedAccessView,
    }

    pub struct Converter {
        dev: ID3D11Device,
        ctx: ID3D11DeviceContext,
        mt: ID3D11Multithread,
        cb: ID3D11Buffer,
        single: Option<ID3D11ComputeShader>,
        pass1: Option<ID3D11ComputeShader>,
        pass2: Option<ID3D11ComputeShader>,
        resize_h: Option<ID3D11ComputeShader>,
        resize_v: Option<ID3D11ComputeShader>,
        out_w: u32,
        out_h: u32,
        in_w: u32,
        in_h: u32,
        src_srv: ID3D11ShaderResourceView,
        resize: Option<Resize>,
        outs: Vec<Nv12Out>,
    }

    impl Converter {
        pub fn outputs(&self) -> &[Nv12Out] {
            &self.outs
        }

        pub fn create(input: &GpuInput, out_w: u32, out_h: u32, improved: bool) -> R<Self> {
            unsafe {
                let dev = &input.device;

                let hd = out_w >= 1280 || out_h > 576;
                let m = if hd { &BT709 } else { &BT601 };
                let cb_desc = D3D11_BUFFER_DESC {
                    ByteWidth: (6 * 4 * std::mem::size_of::<f32>()) as u32,
                    Usage: D3D11_USAGE_IMMUTABLE,
                    BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                    CPUAccessFlags: 0,
                    MiscFlags: 0,
                    StructureByteStride: 0,
                };
                let cb_init = D3D11_SUBRESOURCE_DATA {
                    pSysMem: m.as_ptr() as *const c_void,
                    SysMemPitch: 0,
                    SysMemSlicePitch: 0,
                };
                let mut cb: Option<ID3D11Buffer> = None;
                dev.CreateBuffer(&cb_desc, Some(&cb_init), Some(&mut cb as *mut Option<ID3D11Buffer>))
                    .map_err(e)?;
                let cb = cb.ok_or(-135)?;

                let (single, pass1, pass2) = if improved {
                    (None, Some(compile(dev, s!("ConvertPass1"))?), Some(compile(dev, s!("ConvertPass2"))?))
                } else {
                    (Some(compile(dev, s!("ConvertSinglePass"))?), None, None)
                };

                let mut outs = Vec::with_capacity(GPU_RING);
                for _ in 0..GPU_RING {
                    let tex = texture(
                        dev,
                        out_w,
                        out_h,
                        DXGI_FORMAT_NV12,
                        (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_UNORDERED_ACCESS.0) as u32,
                    )?;
                    outs.push(Nv12Out {
                        uav_y: uav(dev, &tex, DXGI_FORMAT_R8_UNORM)?,
                        uav_uv: uav(dev, &tex, DXGI_FORMAT_R8G8_UNORM)?,
                        srv_uv: srv(dev, &tex, DXGI_FORMAT_R8G8_UNORM)?,
                        tex,
                    });
                }

                let src_srv = srv(dev, &input.latest, DXGI_FORMAT_B8G8R8A8_UNORM)?;
                let mut c = Converter {
                    dev: dev.clone(),
                    ctx: input.context.clone(),
                    mt: input.mt.clone(),
                    cb,
                    single,
                    pass1,
                    pass2,
                    resize_h: None,
                    resize_v: None,
                    out_w,
                    out_h,
                    in_w: input.w,
                    in_h: input.h,
                    src_srv,
                    resize: None,
                    outs,
                };
                c.build_resize()?;
                Ok(c)
            }
        }

        pub fn rebind(&mut self, input: &GpuInput) -> R<()> {
            unsafe {
                self.src_srv = srv(&self.dev, &input.latest, DXGI_FORMAT_B8G8R8A8_UNORM)?;
            }
            self.in_w = input.w;
            self.in_h = input.h;
            self.build_resize()
        }

        fn build_resize(&mut self) -> R<()> {
            if self.in_w == self.out_w && self.in_h == self.out_h {
                self.resize = None;
                return Ok(());
            }
            unsafe {
                if self.resize_h.is_none() {
                    self.resize_h = Some(compile(&self.dev, s!("ResizePassH"))?);
                    self.resize_v = Some(compile(&self.dev, s!("ResizePassV"))?);
                }
                let bind = (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_UNORDERED_ACCESS.0) as u32;
                let mid = texture(&self.dev, self.out_w, self.in_h, DXGI_FORMAT_B8G8R8A8_TYPELESS, bind)?;
                let out = texture(&self.dev, self.out_w, self.out_h, DXGI_FORMAT_B8G8R8A8_TYPELESS, bind)?;
                self.resize = Some(Resize {
                    mid_srv: srv(&self.dev, &mid, DXGI_FORMAT_B8G8R8A8_UNORM)?,
                    mid_uav: uav(&self.dev, &mid, DXGI_FORMAT_R32_UINT)?,
                    out_srv: srv(&self.dev, &out, DXGI_FORMAT_B8G8R8A8_UNORM)?,
                    out_uav: uav(&self.dev, &out, DXGI_FORMAT_R32_UINT)?,
                });
            }
            Ok(())
        }

        pub fn run(&self, idx: usize) -> R<()> {
            let out = &self.outs[idx];
            let ctx = &self.ctx;
            unsafe {
                self.mt.Enter();

                let conv_in: &ID3D11ShaderResourceView = match &self.resize {
                    None => &self.src_srv,
                    Some(r) => {
                        ctx.ClearState();
                        ctx.CSSetShader(self.resize_h.as_ref(), None);
                        ctx.CSSetShaderResources(0, Some(&[Some(self.src_srv.clone())]));
                        let u = [Some(r.mid_uav.clone())];
                        ctx.CSSetUnorderedAccessViews(0, 1, Some(u.as_ptr()), None);
                        ctx.Dispatch(div_up(self.out_w, 16), div_up(self.in_h, 16), 1);

                        ctx.ClearState();
                        ctx.CSSetShader(self.resize_v.as_ref(), None);
                        ctx.CSSetShaderResources(0, Some(&[Some(r.mid_srv.clone())]));
                        let u = [Some(r.out_uav.clone())];
                        ctx.CSSetUnorderedAccessViews(0, 1, Some(u.as_ptr()), None);
                        ctx.Dispatch(div_up(self.out_w, 16), div_up(self.out_h, 16), 1);
                        &r.out_srv
                    }
                };

                let cbs = [Some(self.cb.clone())];
                if let Some(single) = &self.single {
                    ctx.ClearState();
                    ctx.CSSetShader(Some(single), None);
                    ctx.CSSetConstantBuffers(0, Some(&cbs));
                    ctx.CSSetShaderResources(0, Some(&[Some(conv_in.clone())]));
                    let u = [Some(out.uav_y.clone()), Some(out.uav_uv.clone())];
                    ctx.CSSetUnorderedAccessViews(0, 2, Some(u.as_ptr()), None);
                    ctx.Dispatch(div_up(self.out_w / 2, 16), div_up(self.out_h / 2, 16), 1);
                } else if let (Some(p1), Some(p2)) = (&self.pass1, &self.pass2) {
                    ctx.ClearState();
                    ctx.CSSetShader(Some(p1), None);
                    ctx.CSSetConstantBuffers(0, Some(&cbs));
                    ctx.CSSetShaderResources(0, Some(&[Some(conv_in.clone())]));
                    let u = [Some(out.uav_uv.clone())];
                    ctx.CSSetUnorderedAccessViews(1, 1, Some(u.as_ptr()), None);
                    ctx.Dispatch(div_up(self.out_w / 2, 16), div_up(self.out_h / 2, 16), 1);

                    ctx.ClearState();
                    ctx.CSSetShader(Some(p2), None);
                    ctx.CSSetConstantBuffers(0, Some(&cbs));
                    ctx.CSSetShaderResources(0, Some(&[Some(conv_in.clone()), Some(out.srv_uv.clone())]));
                    let u = [Some(out.uav_y.clone())];
                    ctx.CSSetUnorderedAccessViews(0, 1, Some(u.as_ptr()), None);
                    ctx.Dispatch(div_up(self.out_w, 16), div_up(self.out_h, 16), 1);
                }

                ctx.ClearState();
                ctx.Flush();
                self.mt.Leave();
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod nv12_tests {
    use super::*;

    #[test]
    fn nv12_matches_i420_interleaved() {
        for &(w, h, dw, dh) in &[(64u32, 48u32, 64u32, 48u32), (64, 48, 32, 24)] {
            let mut bgra = vec![0u8; (w * h * 4) as usize];
            for (i, px) in bgra.chunks_mut(4).enumerate() {
                px[0] = (i * 7) as u8;
                px[1] = (i * 13) as u8;
                px[2] = (i * 29) as u8;
                px[3] = 255;
            }
            let n = (dw * dh * 3 / 2) as usize;
            let mut i420 = vec![0u8; n];
            let mut nv12 = vec![0u8; n];
            let mut tmp = Vec::new();
            assert!(bgra_to_i420(&bgra, w, h, w * 4, dw, dh, &mut i420));
            assert!(bgra_to_nv12(&bgra, w, h, w * 4, dw, dh, &mut nv12, &mut tmp));
            let ys = (dw * dh) as usize;
            assert_eq!(&i420[..ys], &nv12[..ys]);
            let cs = ys / 4;
            for i in 0..cs {
                assert_eq!(i420[ys + i], nv12[ys + 2 * i]);
                assert_eq!(i420[ys + cs + i], nv12[ys + 2 * i + 1]);
            }
        }
    }
}


pub struct EncodedFrame {
    pub data: Vec<u8>,
    pub keyframe: bool,
    pub pts100ns: i64,
}


pub fn bgra_to_i420(
    bgra: &[u8],
    src_w: u32,
    src_h: u32,
    row_pitch: u32,
    dst_w: u32,
    dst_h: u32,
    dst: &mut [u8],
) -> bool {
    let dst_w = dst_w & !1;
    let dst_h = dst_h & !1;
    if src_w < 2 || src_h < 2 || dst_w < 2 || dst_h < 2 {
        return false;
    }
    let needed = (dst_w * dst_h * 3 / 2) as usize;
    if dst.len() < needed || bgra.len() < (row_pitch as usize) * (src_h as usize) {
        return false;
    }

    let dw = dst_w as usize;
    let dh = dst_h as usize;
    let sw = src_w as usize;
    let sh = src_h as usize;

    if sw == dw && sh == dh {
        return bgra_to_i420_1to1(bgra, dw, dh, row_pitch, dst);
    }
    bgra_to_i420_scaled(bgra, sw, sh, row_pitch, dw, dh, dst)
}

fn bgra_to_i420_scaled(
    bgra: &[u8],
    sw: usize,
    sh: usize,
    row_pitch: u32,
    dw: usize,
    dh: usize,
    dst: &mut [u8],
) -> bool {
    let cw = dw / 2;
    let ch = dh / 2;

    let (y_plane, uv) = dst.split_at_mut(dw * dh);
    let (u_plane, v_plane) = uv.split_at_mut(cw * ch);

    let scale_x = sw as f64 / dw as f64;
    let scale_y = sh as f64 / dh as f64;
    let mut col_x0 = vec![0usize; dw];
    let mut col_x1 = vec![0usize; dw];
    let mut col_fx = vec![0f32; dw];
    for x in 0..dw {
        let sxv = ((x as f64 + 0.5) * scale_x) - 0.5;
        let x0 = (sxv.floor() as i64).clamp(0, sw as i64 - 2) as usize;
        col_x0[x] = x0;
        col_x1[x] = x0 + 1;
        col_fx[x] = (sxv - x0 as f64) as f32;
    }
    let mut row_y0 = vec![0usize; dh];
    let mut row_y1 = vec![0usize; dh];
    let mut row_fy = vec![0f32; dh];
    for y in 0..dh {
        let syv = ((y as f64 + 0.5) * scale_y) - 0.5;
        let y0 = (syv.floor() as i64).clamp(0, sh as i64 - 2) as usize;
        row_y0[y] = y0;
        row_y1[y] = y0 + 1;
        row_fy[y] = (syv - y0 as f64) as f32;
    }

    let mut cb_full = vec![0u8; dw * dh];
    let mut cr_full = vec![0u8; dw * dh];

    y_plane
        .par_chunks_mut(dw)
        .zip(cb_full.par_chunks_mut(dw))
        .zip(cr_full.par_chunks_mut(dw))
        .enumerate()
        .for_each(|(y, ((y_row, cb_row), cr_row))| {
            let row0 = &bgra[row_y0[y] * row_pitch as usize..];
            let row1 = &bgra[row_y1[y] * row_pitch as usize..];
            let fy = row_fy[y];
            for x in 0..dw {
                let x0 = col_x0[x] * 4;
                let x1 = col_x1[x] * 4;
                let fx = col_fx[x];

                let p00 = &row0[x0..];
                let p10 = &row0[x1..];
                let p01 = &row1[x0..];
                let p11 = &row1[x1..];
                let (b00, g00, r00) = (p00[0] as f32, p00[1] as f32, p00[2] as f32);
                let (b10, g10, r10) = (p10[0] as f32, p10[1] as f32, p10[2] as f32);
                let (b01, g01, r01) = (p01[0] as f32, p01[1] as f32, p01[2] as f32);
                let (b11, g11, r11) = (p11[0] as f32, p11[1] as f32, p11[2] as f32);

                let r = lerp(lerp(r00, r10, fx), lerp(r01, r11, fx), fy);
                let g = lerp(lerp(g00, g10, fx), lerp(g01, g11, fx), fy);
                let b = lerp(lerp(b00, b10, fx), lerp(b01, b11, fx), fy);

                y_row[x] = clamp_u8(16.0 + 65.738 / 256.0 * r + 129.057 / 256.0 * g + 25.064 / 256.0 * b);
                cb_row[x] = clamp_u8(-37.945 / 256.0 * r - 74.494 / 256.0 * g + 112.439 / 256.0 * b + 128.0);
                cr_row[x] = clamp_u8(112.439 / 256.0 * r - 94.154 / 256.0 * g - 18.285 / 256.0 * b + 128.0);
            }
        });

    u_plane
        .par_chunks_mut(cw)
        .zip(v_plane.par_chunks_mut(cw))
        .enumerate()
        .for_each(|(c, (u_row, v_row))| {
            let top = (c * 2) * dw;
            let bot = top + dw;
            let (cb_top, cb_bot) = (&cb_full[top..top + dw], &cb_full[bot..bot + dw]);
            let (cr_top, cr_bot) = (&cr_full[top..top + dw], &cr_full[bot..bot + dw]);
            for x in 0..cw {
                let x2 = x * 2;
                u_row[x] = ((cb_top[x2] as u32 + cb_top[x2 + 1] as u32 + cb_bot[x2] as u32
                    + cb_bot[x2 + 1] as u32
                    + 2)
                    >> 2) as u8;
                v_row[x] = ((cr_top[x2] as u32 + cr_top[x2 + 1] as u32 + cr_bot[x2] as u32
                    + cr_bot[x2 + 1] as u32
                    + 2)
                    >> 2) as u8;
            }
        });
    true
}

fn bgra_to_i420_1to1(bgra: &[u8], w: usize, h: usize, row_pitch: u32, dst: &mut [u8]) -> bool {
    debug_assert!(w >= 2 && h >= 2 && w % 2 == 0 && h % 2 == 0);
    let cw = w / 2;
    let rp = row_pitch as usize;
    let (y_plane, uv) = dst.split_at_mut(w * h);
    let (u_plane, v_plane) = uv.split_at_mut(cw * (h / 2));

    y_plane
        .par_chunks_mut(2 * w)
        .zip(u_plane.par_chunks_mut(cw))
        .zip(v_plane.par_chunks_mut(cw))
        .enumerate()
        .for_each(|(c, ((y_pair, u_row), v_row))| {
            let (y_top, y_bot) = y_pair.split_at_mut(w);
            let rt = &bgra[(2 * c) * rp..];
            let rb = &bgra[(2 * c + 1) * rp..];

            for x in 0..w {
                let p = &rt[x * 4..];
                y_top[x] = y601(p[2] as i32, p[1] as i32, p[0] as i32);
            }
            for x in 0..w {
                let p = &rb[x * 4..];
                y_bot[x] = y601(p[2] as i32, p[1] as i32, p[0] as i32);
            }

            for x in 0..cw {
                let o = x * 8;
                let p00 = &rt[o..];
                let p01 = &rt[o + 4..];
                let p10 = &rb[o..];
                let p11 = &rb[o + 4..];
                let b = (p00[0] as i32 + p01[0] as i32 + p10[0] as i32 + p11[0] as i32 + 2) >> 2;
                let g = (p00[1] as i32 + p01[1] as i32 + p10[1] as i32 + p11[1] as i32 + 2) >> 2;
                let r = (p00[2] as i32 + p01[2] as i32 + p10[2] as i32 + p11[2] as i32 + 2) >> 2;
                u_row[x] = u601(r, g, b);
                v_row[x] = v601(r, g, b);
            }
        });
    true
}

#[inline(always)]
fn y601(r: i32, g: i32, b: i32) -> u8 {
    clamp_i32(16 + ((16832 * r + 33037 * g + 6416 * b + 32768) >> 16))
}

#[inline(always)]
fn u601(r: i32, g: i32, b: i32) -> u8 {
    clamp_i32(128 + ((-9712 * r - 19070 * g + 28781 * b + 32768) >> 16))
}

#[inline(always)]
fn v601(r: i32, g: i32, b: i32) -> u8 {
    clamp_i32(128 + ((28781 * r - 24103 * g - 4681 * b + 32768) >> 16))
}

#[inline(always)]
fn clamp_i32(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

#[inline(always)]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[inline(always)]
fn clamp_u8(v: f32) -> u8 {
    let v = v.round();
    if v < 0.0 {
        0
    } else if v > 255.0 {
        255
    } else {
        v as u8
    }
}

pub fn bgra_to_nv12(
    bgra: &[u8],
    src_w: u32,
    src_h: u32,
    row_pitch: u32,
    dst_w: u32,
    dst_h: u32,
    dst: &mut [u8],
    tmp: &mut Vec<u8>,
) -> bool {
    let dw = (dst_w & !1) as usize;
    let dh = (dst_h & !1) as usize;
    if src_w < 2 || src_h < 2 || dw < 2 || dh < 2 {
        return false;
    }
    let needed = dw * dh * 3 / 2;
    let rp = row_pitch as usize;
    if dst.len() < needed || bgra.len() < rp * (src_h as usize) {
        return false;
    }

    if src_w as usize == dw && src_h as usize == dh {
        let (y_plane, uv_plane) = dst[..needed].split_at_mut(dw * dh);
        y_plane
            .par_chunks_mut(2 * dw)
            .zip(uv_plane.par_chunks_mut(dw))
            .enumerate()
            .for_each(|(c, (y_pair, uv_row))| {
                let (y_top, y_bot) = y_pair.split_at_mut(dw);
                let rt = &bgra[(2 * c) * rp..];
                let rb = &bgra[(2 * c + 1) * rp..];
                for x in 0..dw {
                    let p = &rt[x * 4..];
                    y_top[x] = y601(p[2] as i32, p[1] as i32, p[0] as i32);
                }
                for x in 0..dw {
                    let p = &rb[x * 4..];
                    y_bot[x] = y601(p[2] as i32, p[1] as i32, p[0] as i32);
                }
                for x in 0..dw / 2 {
                    let o = x * 8;
                    let (p00, p01, p10, p11) = (&rt[o..], &rt[o + 4..], &rb[o..], &rb[o + 4..]);
                    let b = (p00[0] as i32 + p01[0] as i32 + p10[0] as i32 + p11[0] as i32 + 2) >> 2;
                    let g = (p00[1] as i32 + p01[1] as i32 + p10[1] as i32 + p11[1] as i32 + 2) >> 2;
                    let r = (p00[2] as i32 + p01[2] as i32 + p10[2] as i32 + p11[2] as i32 + 2) >> 2;
                    uv_row[2 * x] = u601(r, g, b);
                    uv_row[2 * x + 1] = v601(r, g, b);
                }
            });
        return true;
    }

    if tmp.len() < needed {
        tmp.resize(needed, 0);
    }
    if !bgra_to_i420_scaled(bgra, src_w as usize, src_h as usize, row_pitch, dw, dh, &mut tmp[..needed]) {
        return false;
    }
    let ys = dw * dh;
    let cw = dw / 2;
    let cs = cw * (dh / 2);
    dst[..ys].copy_from_slice(&tmp[..ys]);
    let (u, v) = (&tmp[ys..ys + cs], &tmp[ys + cs..needed]);
    dst[ys..needed]
        .par_chunks_mut(dw)
        .enumerate()
        .for_each(|(r, row)| {
            for x in 0..cw {
                row[2 * x] = u[r * cw + x];
                row[2 * x + 1] = v[r * cw + x];
            }
        });
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, b: u8, g: u8, r: u8) -> Vec<u8> {
        let mut v = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..(w * h) {
            v.extend_from_slice(&[b, g, r, 255]);
        }
        v
    }

    fn yuv_of(b: u8, g: u8, r: u8) -> (u8, u8, u8) {
        let src = solid(8, 8, b, g, r);
        let mut dst = vec![0u8; 8 * 8 * 3 / 2];
        assert!(bgra_to_i420(&src, 8, 8, 8 * 4, 8, 8, &mut dst));
        let c = 8 * 8 / 4;
        (dst[0], dst[8 * 8], dst[8 * 8 + c])
    }

    fn close(a: u8, b: i32) {
        assert!(
            (a as i32 - b).abs() <= 1,
            "expected ~{}, got {}",
            b,
            a
        );
    }

    #[test]
    fn matches_bt601_limited_range() {
        let (y, u, v) = yuv_of(0, 0, 0);
        close(y, 16);
        close(u, 128);
        close(v, 128);

        let (y, u, v) = yuv_of(255, 255, 255);
        close(y, 235);
        close(u, 128);
        close(v, 128);

        let (y, u, v) = yuv_of(0, 0, 255);
        close(y, 81);
        close(u, 90);
        close(v, 240);

        let (y, u, v) = yuv_of(0, 255, 0);
        close(y, 145);
        close(u, 54);
        close(v, 34);

        let (y, u, v) = yuv_of(255, 0, 0);
        close(y, 41);
        close(u, 240);
        close(v, 110);
    }

    #[test]
    fn solid_color_survives_scaling() {
        for (dw, dh) in [(16u32, 16u32), (64, 64), (8, 8)] {
            let src = solid(64, 64, 40, 90, 200);
            let mut dst = vec![0u8; (dw * dh * 3 / 2) as usize];
            assert!(bgra_to_i420(&src, 64, 64, 64 * 4, dw, dh, &mut dst));
            let (y0, _, _) = {
                let mut probe = vec![0u8; 64 * 64 * 3 / 2];
                assert!(bgra_to_i420(&src, 64, 64, 64 * 4, 64, 64, &mut probe));
                (probe[0], 0, 0)
            };
            assert!(
                dst[..(dw * dh) as usize].iter().all(|&p| p == y0),
                "luma not uniform at {}x{}",
                dw,
                dh
            );
            let chroma = &dst[(dw * dh) as usize..];
            let csize = (dw * dh / 4) as usize;
            let (u_plane, v_plane) = chroma.split_at(csize);
            assert!(
                u_plane.iter().all(|&p| p == u_plane[0]),
                "U not uniform at {}x{}",
                dw,
                dh
            );
            assert!(
                v_plane.iter().all(|&p| p == v_plane[0]),
                "V not uniform at {}x{}",
                dw,
                dh
            );
            assert!(csize > 0);
        }
    }

    #[test]
    fn chroma_is_averaged_over_2x2_blocks() {
        let w = 4u32;
        let h = 4u32;
        let quads: [[u8; 3]; 4] = [
            [255, 0, 0],
            [0, 255, 0],
            [0, 0, 255],
            [255, 255, 255],
        ];
        let mut src = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let q = ((y / 2) * 2 + (x / 2)) as usize;
                src.extend_from_slice(&[quads[q][0], quads[q][1], quads[q][2], 255]);
            }
        }
        let mut dst = vec![0u8; (w * h * 3 / 2) as usize];
        assert!(bgra_to_i420(&src, w, h, w * 4, w, h, &mut dst));
        let y_size = (w * h) as usize;
        let c = (w * h / 4) as usize;
        let u_plane = &dst[y_size..y_size + c];
        let v_plane = &dst[y_size + c..];
        assert_eq!(u_plane, &[240, 54, 90, 128]);
        assert_eq!(v_plane, &[110, 34, 240, 128]);
    }

    #[test]
    fn fast_path_matches_bt601_reference() {
        let (y, u, v) = yuv_of(0, 0, 0);
        close(y, 16);
        close(u, 128);
        close(v, 128);

        let (y, u, v) = yuv_of(255, 255, 255);
        close(y, 235);
        close(u, 128);
        close(v, 128);

        let (y, u, v) = yuv_of(0, 0, 255);
        close(y, 81);
        close(u, 90);
        close(v, 240);

        let (y, u, v) = yuv_of(0, 255, 0);
        close(y, 145);
        close(u, 54);
        close(v, 34);

        let (y, u, v) = yuv_of(255, 0, 0);
        close(y, 41);
        close(u, 240);
        close(v, 110);
    }

    #[test]
    fn fast_path_agrees_with_bilinear_path_at_1to1() {
        let (w, h) = (64usize, 64usize);
        let mut src = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let o = (y * w + x) * 4;
                src[o] = (x * 4) as u8;
                src[o + 1] = (y * 4) as u8;
                src[o + 2] = ((x * 7 + y * 3) % 256) as u8;
                src[o + 3] = 255;
            }
        }
        let n = w * h * 3 / 2;

        let mut fast = vec![0u8; n];
        assert!(bgra_to_i420(&src, w as u32, h as u32, (w * 4) as u32, w as u32, h as u32, &mut fast));

        let mut scaled = vec![0u8; n];
        assert!(bgra_to_i420_scaled(&src, w, h, (w * 4) as u32, w, h, &mut scaled));

        let mut worst_y = 0i32;
        let mut worst_c = 0i32;
        for i in 0..n {
            let d = (fast[i] as i32 - scaled[i] as i32).abs();
            if i < w * h {
                worst_y = worst_y.max(d);
            } else {
                worst_c = worst_c.max(d);
            }
        }
        assert!(worst_y <= 1, "luma diverged by {worst_y} levels (limit 1)");
        assert!(worst_c <= 1, "chroma diverged by {worst_c} levels (limit 1)");
    }

    #[test]
    fn handles_odd_and_undersized() {
        let src = solid(8, 8, 10, 20, 30);
        let mut dst = vec![0u8; 8 * 8 * 3 / 2];
        assert!(bgra_to_i420(&src, 8, 8, 8 * 4, 7, 7, &mut dst));
        let mut tiny = vec![0u8; 8 * 8 * 3 / 2 - 1];
        assert!(!bgra_to_i420(&src, 8, 8, 8 * 4, 8, 8, &mut tiny));
        assert!(!bgra_to_i420(&src, 1, 8, 4, 8, 8, &mut dst));
    }
}