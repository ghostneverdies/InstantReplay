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

    pub fn flush(&mut self, out: &mut Vec<EncodedFrame>) {
        let Inner::Mf(mf) = &mut self.inner;
        let _ = mf.finish(out);
    }

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
    use windows::core::{Interface, GUID};
    use windows::Win32::Graphics::Direct3D11::ID3D11Device;
    use windows::Win32::Graphics::Dxgi::IDXGIDevice;
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Com::{
        CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Variant::VARIANT;

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

    pub fn surface_sample(
        tex: &windows::Win32::Graphics::Direct3D11::ID3D11Texture2D,
    ) -> R<Sample> {
        unsafe {
            let buf = MFCreateDXGISurfaceBuffer(
                &windows::Win32::Graphics::Direct3D11::ID3D11Texture2D::IID,
                tex,
                0,
                false,
            )
            .map_err(e)?;
            let max = buf.GetMaxLength().map_err(e)?;
            buf.SetCurrentLength(max).map_err(e)?;
            let s: IMFSample = MFCreateTrackedSample().map_err(e)?.cast().map_err(e)?;
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
                let hw = if want_hardware {
                    MFT_ENUM_FLAG_HARDWARE
                } else {
                    MFT_ENUM_FLAG_SYNCMFT
                };
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
                        mgr.as_ref()
                            .ok_or(-122)?
                            .ResetDevice(dev, token)
                            .map_err(e)?;
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
            let low_latency = 1;
            let _ = attrs.SetUINT32(&MF_LOW_LATENCY, low_latency);

            if let Some(m) = mgr {
                transform
                    .ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, m.as_raw() as usize)
                    .map_err(e)?;
            }

            let codec = transform.cast::<ICodecAPI>().ok();
            if let Some(c) = &codec {
                codec_u32(c, &CODECAPI_AVLowLatencyMode, low_latency);
                codec_u32(c, &CODECAPI_AVEncCommonRateControlMode, eAVEncCommonRateControlMode_UnconstrainedVBR.0 as u32);
                codec_u32(c, &CODECAPI_AVEncCommonMeanBitRate, bps);
                codec_u32(c, &CODECAPI_AVEncMPVGOPSize, fps.max(1));
                codec_u32(c, &CODECAPI_AVEncMPVDefaultBPictureCount, 0);
            }

            let out_t = MFCreateMediaType().map_err(e)?;
            out_t
                .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                .map_err(e)?;
            out_t
                .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)
                .map_err(e)?;
            out_t.SetUINT32(&MF_MT_AVG_BITRATE, bps).map_err(e)?;
            out_t.SetUINT64(&MF_MT_FRAME_SIZE, pack(w, h)).map_err(e)?;
            out_t
                .SetUINT64(&MF_MT_FRAME_RATE, pack(fps.max(1), 1))
                .map_err(e)?;
            out_t
                .SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1))
                .map_err(e)?;
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
            in_t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                .map_err(e)?;
            in_t.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)
                .map_err(e)?;
            in_t.SetUINT64(&MF_MT_FRAME_SIZE, pack(w, h)).map_err(e)?;
            in_t.SetUINT64(&MF_MT_FRAME_RATE, pack(fps.max(1), 1))
                .map_err(e)?;
            in_t.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1))
                .map_err(e)?;
            in_t.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
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
                _rt: MfRuntime {
                    com: false,
                    mf: false,
                },
            })
        }

        unsafe fn read_seq_header(t: &IMFTransform) -> Vec<u8> {
            if let Ok(ty) = t.GetOutputCurrentType(0) {
                if let Ok(n) = ty.GetBlobSize(&MF_MT_MPEG_SEQUENCE_HEADER) {
                    let mut b = vec![0u8; n as usize];
                    if n > 4
                        && ty
                            .GetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &mut b, None)
                            .is_ok()
                    {
                        if b.starts_with(&[0, 0, 0, 1]) || b.starts_with(&[0, 0, 1]) {
                            return b;
                        }
                    }
                }
            }
            Vec::new()
        }

        pub fn finish(&mut self, out: &mut Vec<EncodedFrame>) -> R<()> {
            unsafe {
                self.transform
                    .ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0)
                    .map_err(e)?;
                self.transform
                    .ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0)
                    .map_err(e)?;
                let deadline = Instant::now() + Duration::from_secs(2);
                loop {
                    if self.events.is_some() {
                        self.pump(out)?;
                    } else {
                        self.drain_sync(out)?;
                    }
                    if self.received >= self.submitted || Instant::now() >= deadline {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                Ok(())
            }
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
                let ok = bgra_to_nv12(
                    bgra,
                    src_w,
                    src_h,
                    pitch,
                    self.width,
                    self.height,
                    dst,
                    &mut self.tmp_i420,
                );
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

        pub fn encode_surface(
            &mut self,
            sample: &Sample,
            pts100ns: i64,
            out: &mut Vec<EncodedFrame>,
        ) -> R<()> {
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
                let ev = match self
                    .events
                    .as_ref()
                    .expect("checked above")
                    .GetEvent(MF_EVENT_FLAG_NO_WAIT)
                {
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
            mb.Lock(&mut p, None, Some(&mut len as *mut u32))
                .map_err(e)?;
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
                let _ = self
                    .transform
                    .ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
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
    stamp: std::sync::Mutex<(u64, i64)>,
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

    pub fn submit(&self, frame: &windows::Win32::Graphics::Direct3D11::ID3D11Texture2D, pts: i64) {
        let mut stamp = self.stamp.lock().unwrap_or_else(|e| e.into_inner());
        gpu::submit(self, frame);
        stamp.0 = stamp.0.wrapping_add(1);
        stamp.1 = pts;
    }
}

pub struct GpuEncoder {
    samples: tracked::SamplePool,
    conv: gpu::Converter,
    mf: mf::MfEncoder,
    input: std::sync::Arc<GpuInput>,
    last_pts: i64,
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
        Ok(GpuEncoder {
            samples: tracked::SamplePool::new(samples),
            conv,
            mf,
            input,
            last_pts: -1,
            convert_ns: 0,
        })
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

    pub fn encode_latest(&mut self, out: &mut Vec<EncodedFrame>) -> Result<Option<i64>, i32> {
        self.mf.wait_room(GPU_RING as u64 - 1, out)?;
        let Some(lease) = self.samples.try_take() else {
            return Ok(None);
        };
        let input = self.input.clone();
        let started = Instant::now();
        let stamp = input.stamp.lock().unwrap_or_else(|e| e.into_inner());
        if stamp.1 <= self.last_pts {
            return Ok(None);
        }
        let pts = stamp.1;
        self.conv.run(lease.index)?;
        self.convert_ns += started.elapsed().as_nanos() as i64;
        drop(stamp);
        unsafe {
            lease.submit_transform(|sample| self.mf.encode_surface(sample, pts, out))?;
        }
        self.last_pts = pts;
        Ok(Some(pts))
    }
    pub fn flush(&mut self, out: &mut Vec<EncodedFrame>) {
        let _ = self.mf.finish(out);
    }
}

mod gpu {
    use super::{GpuInput, GPU_RING};
    use std::ffi::c_void;
    use windows::core::Interface;
    use windows::Win32::Graphics::Direct3D::Fxc::D3DDecompressShaders;
    use windows::Win32::Graphics::Direct3D::{ID3DBlob, D3D_SRV_DIMENSION_TEXTURE2D};
    use windows::Win32::Graphics::Direct3D11::*;
    use windows::Win32::Graphics::Dxgi::Common::*;

    type R<T> = Result<T, i32>;

    fn e(er: windows::core::Error) -> i32 {
        er.code().0
    }

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

    unsafe fn shader(dev: &ID3D11Device, bytes: &[u8]) -> R<ID3D11ComputeShader> {
        let mut blob: Option<ID3DBlob> = None;
        D3DDecompressShaders(
            bytes.as_ptr() as _,
            bytes.len(),
            1,
            0,
            None,
            0,
            &mut blob,
            None,
        )
        .map_err(e)?;
        let blob = blob.ok_or(-130)?;
        let bytes =
            std::slice::from_raw_parts(blob.GetBufferPointer() as *const u8, blob.GetBufferSize());
        let mut shader = None;
        dev.CreateComputeShader(bytes, None, Some(&mut shader))
            .map_err(e)?;
        shader.ok_or(-131)
    }

    unsafe fn texture(
        dev: &ID3D11Device,
        w: u32,
        h: u32,
        fmt: DXGI_FORMAT,
        bind: u32,
    ) -> R<ID3D11Texture2D> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: w,
            Height: h,
            MipLevels: 1,
            ArraySize: 1,
            Format: fmt,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
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

    unsafe fn srv(
        dev: &ID3D11Device,
        tex: &ID3D11Texture2D,
        fmt: DXGI_FORMAT,
    ) -> R<ID3D11ShaderResourceView> {
        let desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
            Format: fmt,
            ViewDimension: D3D_SRV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_SRV {
                    MostDetailedMip: 0,
                    MipLevels: 1,
                },
            },
        };
        let mut v: Option<ID3D11ShaderResourceView> = None;
        dev.CreateShaderResourceView(
            tex,
            Some(&desc),
            Some(&mut v as *mut Option<ID3D11ShaderResourceView>),
        )
        .map_err(e)?;
        v.ok_or(-133)
    }

    unsafe fn uav(
        dev: &ID3D11Device,
        tex: &ID3D11Texture2D,
        fmt: DXGI_FORMAT,
    ) -> R<ID3D11UnorderedAccessView> {
        let desc = D3D11_UNORDERED_ACCESS_VIEW_DESC {
            Format: fmt,
            ViewDimension: D3D11_UAV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_UNORDERED_ACCESS_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_UAV { MipSlice: 0 },
            },
        };
        let mut v: Option<ID3D11UnorderedAccessView> = None;
        dev.CreateUnorderedAccessView(
            tex,
            Some(&desc),
            Some(&mut v as *mut Option<ID3D11UnorderedAccessView>),
        )
        .map_err(e)?;
        v.ok_or(-134)
    }

    pub fn create_input(
        dev: &ID3D11Device,
        ctx: &ID3D11DeviceContext,
        w: u32,
        h: u32,
    ) -> R<GpuInput> {
        unsafe {
            let mt: ID3D11Multithread = ctx.cast().map_err(e)?;
            let _ = mt.SetMultithreadProtected(true);
            let latest = texture(
                dev,
                w,
                h,
                DXGI_FORMAT_B8G8R8A8_TYPELESS,
                D3D11_BIND_SHADER_RESOURCE.0 as u32,
            )?;
            Ok(GpuInput {
                device: dev.clone(),
                context: ctx.clone(),
                mt,
                latest,
                w,
                h,
                stamp: std::sync::Mutex::new((0, -1)),
            })
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
                dev.CreateBuffer(
                    &cb_desc,
                    Some(&cb_init),
                    Some(&mut cb as *mut Option<ID3D11Buffer>),
                )
                .map_err(e)?;
                let cb = cb.ok_or(-135)?;

                let (single, pass1, pass2) = if improved {
                    (
                        None,
                        Some(shader(
                            dev,
                            super::embedded_shaders::CONVERT_PASS1_SHADER_BYTES,
                        )?),
                        Some(shader(
                            dev,
                            super::embedded_shaders::CONVERT_PASS2_SHADER_BYTES,
                        )?),
                    )
                } else {
                    (
                        Some(shader(
                            dev,
                            super::embedded_shaders::CONVERT_SINGLE_PASS_SHADER_BYTES,
                        )?),
                        None,
                        None,
                    )
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
                    self.resize_h = Some(shader(
                        &self.dev,
                        super::embedded_shaders::RESIZE_PASS_H_SHADER_BYTES,
                    )?);
                    self.resize_v = Some(shader(
                        &self.dev,
                        super::embedded_shaders::RESIZE_PASS_V_SHADER_BYTES,
                    )?);
                }
                let bind = (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_UNORDERED_ACCESS.0) as u32;
                let mid = texture(
                    &self.dev,
                    self.out_w,
                    self.in_h,
                    DXGI_FORMAT_B8G8R8A8_TYPELESS,
                    bind,
                )?;
                let out = texture(
                    &self.dev,
                    self.out_w,
                    self.out_h,
                    DXGI_FORMAT_B8G8R8A8_TYPELESS,
                    bind,
                )?;
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
                    ctx.CSSetShaderResources(
                        0,
                        Some(&[Some(conv_in.clone()), Some(out.srv_uv.clone())]),
                    );
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
            assert!(bgra_to_nv12(
                &bgra,
                w,
                h,
                w * 4,
                dw,
                dh,
                &mut nv12,
                &mut tmp
            ));
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
        .chunks_mut(dw)
        .zip(cb_full.chunks_mut(dw))
        .zip(cr_full.chunks_mut(dw))
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

                y_row[x] =
                    clamp_u8(16.0 + 65.738 / 256.0 * r + 129.057 / 256.0 * g + 25.064 / 256.0 * b);
                cb_row[x] = clamp_u8(
                    -37.945 / 256.0 * r - 74.494 / 256.0 * g + 112.439 / 256.0 * b + 128.0,
                );
                cr_row[x] =
                    clamp_u8(112.439 / 256.0 * r - 94.154 / 256.0 * g - 18.285 / 256.0 * b + 128.0);
            }
        });

    u_plane
        .chunks_mut(cw)
        .zip(v_plane.chunks_mut(cw))
        .enumerate()
        .for_each(|(c, (u_row, v_row))| {
            let top = (c * 2) * dw;
            let bot = top + dw;
            let (cb_top, cb_bot) = (&cb_full[top..top + dw], &cb_full[bot..bot + dw]);
            let (cr_top, cr_bot) = (&cr_full[top..top + dw], &cr_full[bot..bot + dw]);
            for x in 0..cw {
                let x2 = x * 2;
                u_row[x] = ((cb_top[x2] as u32
                    + cb_top[x2 + 1] as u32
                    + cb_bot[x2] as u32
                    + cb_bot[x2 + 1] as u32
                    + 2)
                    >> 2) as u8;
                v_row[x] = ((cr_top[x2] as u32
                    + cr_top[x2 + 1] as u32
                    + cr_bot[x2] as u32
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
        .chunks_mut(2 * w)
        .zip(u_plane.chunks_mut(cw))
        .zip(v_plane.chunks_mut(cw))
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
            .chunks_mut(2 * dw)
            .zip(uv_plane.chunks_mut(dw))
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
                    let b =
                        (p00[0] as i32 + p01[0] as i32 + p10[0] as i32 + p11[0] as i32 + 2) >> 2;
                    let g =
                        (p00[1] as i32 + p01[1] as i32 + p10[1] as i32 + p11[1] as i32 + 2) >> 2;
                    let r =
                        (p00[2] as i32 + p01[2] as i32 + p10[2] as i32 + p11[2] as i32 + 2) >> 2;
                    uv_row[2 * x] = u601(r, g, b);
                    uv_row[2 * x + 1] = v601(r, g, b);
                }
            });
        return true;
    }

    if tmp.len() < needed {
        tmp.resize(needed, 0);
    }
    if !bgra_to_i420_scaled(
        bgra,
        src_w as usize,
        src_h as usize,
        row_pitch,
        dw,
        dh,
        &mut tmp[..needed],
    ) {
        return false;
    }
    let ys = dw * dh;
    let cw = dw / 2;
    let cs = cw * (dh / 2);
    dst[..ys].copy_from_slice(&tmp[..ys]);
    let (u, v) = (&tmp[ys..ys + cs], &tmp[ys + cs..needed]);
    dst[ys..needed]
        .chunks_mut(dw)
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
        assert!((a as i32 - b).abs() <= 1, "expected ~{}, got {}", b, a);
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
        let quads: [[u8; 3]; 4] = [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 255]];
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
        assert!(bgra_to_i420(
            &src,
            w as u32,
            h as u32,
            (w * 4) as u32,
            w as u32,
            h as u32,
            &mut fast
        ));

        let mut scaled = vec![0u8; n];
        assert!(bgra_to_i420_scaled(
            &src,
            w,
            h,
            (w * 4) as u32,
            w,
            h,
            &mut scaled
        ));

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
        assert!(
            worst_c <= 1,
            "chroma diverged by {worst_c} levels (limit 1)"
        );
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

//wcap shaders bytecode start
mod embedded_shaders {
    pub const RESIZE_PASS_H_SHADER_BYTES: &[u8] = &[
        0x42, 0x53, 0x43, 0x44, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x4d, 0x53, 0x43, 0x46, 0x00, 0x00, 0x00, 0x00, 0xc3, 0x02, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00,
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x43, 0x00, 0x00, 0x00, 0x01, 0x00, 0x03,
        0x15, 0x80, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x53, 0x53, 0x48, 0x45, 0x58, 0x21, 0x00, 0xa9, 0x74, 0x9d, 0xdf, 0x78, 0x02,
        0x80, 0x07, 0x5b, 0x80, 0x80, 0x8d, 0x00, 0x20, 0x09, 0x78, 0xb4, 0x10, 0x00, 0xc1, 0x00,
        0x00, 0x44, 0x02, 0x00, 0x56, 0x46, 0x00, 0xbe, 0x0f, 0xfb, 0xf6, 0xef, 0xdb, 0xb6, 0x67,
        0x96, 0xbd, 0x26, 0xc5, 0x26, 0x55, 0x50, 0xe6, 0x15, 0xd9, 0x5b, 0xe1, 0x96, 0x61, 0x6e,
        0xe6, 0xdc, 0x63, 0x37, 0x88, 0x0a, 0x57, 0x93, 0x28, 0x18, 0x38, 0x65, 0xce, 0x9c, 0x6d,
        0x2e, 0x28, 0x70, 0x75, 0xd5, 0x24, 0x55, 0xf8, 0x42, 0x6f, 0x6f, 0xea, 0xe2, 0x02, 0x17,
        0x42, 0x10, 0x1a, 0x00, 0x00, 0x33, 0x00, 0x60, 0x34, 0x06, 0x00, 0xfb, 0x50, 0x96, 0xb5,
        0x38, 0x77, 0x40, 0x77, 0x9a, 0x9a, 0x5f, 0xb4, 0x16, 0xc9, 0x95, 0x10, 0x89, 0xa5, 0x89,
        0x08, 0xd4, 0x6d, 0x9d, 0x53, 0x3b, 0x71, 0xf1, 0xb2, 0x24, 0x35, 0x89, 0x49, 0x64, 0x57,
        0xe8, 0xac, 0xc1, 0x16, 0xb8, 0x49, 0x98, 0xa5, 0x6f, 0x22, 0x27, 0xc5, 0x8d, 0xe0, 0x27,
        0xf8, 0xef, 0xbf, 0xfe, 0xfb, 0x82, 0xff, 0x00, 0x20, 0x00, 0x00, 0x04, 0x00, 0x20, 0x34,
        0x20, 0x00, 0x4e, 0x15, 0x38, 0x77, 0x30, 0x3c, 0xdf, 0x07, 0x7e, 0xbf, 0x13, 0xfc, 0x02,
        0x88, 0x18, 0x01, 0x05, 0x20, 0x40, 0xd9, 0x17, 0x0f, 0x01, 0x68, 0x79, 0xc1, 0x04, 0xb0,
        0x30, 0x78, 0x6e, 0x7c, 0x3d, 0x8e, 0xdd, 0x19, 0xd5, 0xfd, 0xb8, 0xd0, 0x71, 0xd8, 0x10,
        0x8d, 0x5e, 0x39, 0xcf, 0x69, 0xd0, 0xb5, 0x76, 0x44, 0x88, 0x59, 0xf6, 0x09, 0xe7, 0xc2,
        0x14, 0x45, 0x98, 0x3b, 0x20, 0x3b, 0x97, 0x1b, 0x75, 0x99, 0x30, 0xb8, 0xe5, 0x8e, 0xdb,
        0xe4, 0x39, 0x5c, 0x55, 0x11, 0x1e, 0x39, 0x3f, 0x5d, 0x76, 0x38, 0xfc, 0xee, 0x2e, 0x28,
        0x31, 0xd5, 0x89, 0x18, 0x38, 0x3f, 0xe6, 0x72, 0xa8, 0x2c, 0xf8, 0xb9, 0xf5, 0x6e, 0x91,
        0xbf, 0x8d, 0x29, 0x43, 0x08, 0x8f, 0xc5, 0x86, 0x9f, 0x5e, 0x8e, 0x10, 0x55, 0x0c, 0xca,
        0x4f, 0xbc, 0x14, 0xf1, 0x2f, 0x73, 0x4b, 0x76, 0x4a, 0x50, 0xa4, 0x2b, 0x92, 0x96, 0xdc,
        0xa1, 0xeb, 0x1f, 0x59, 0x99, 0x85, 0xe1, 0x5e, 0x0b, 0x1f, 0xa7, 0x5a, 0xe3, 0x4e, 0x87,
        0x71, 0x9b, 0x71, 0x9b, 0x1a, 0xf4, 0xa9, 0x23, 0x8e, 0x74, 0xaf, 0xec, 0x69, 0x55, 0x06,
        0x4b, 0xdd, 0x0c, 0x4c, 0x22, 0x82, 0x5a, 0xc5, 0xb0, 0xac, 0xa7, 0x76, 0x22, 0x20, 0x3f,
        0xa0, 0x5f, 0x2d, 0xc3, 0x94, 0x5f, 0xf6, 0x9a, 0xb4, 0x0c, 0xc4, 0xce, 0x3c, 0xd0, 0xb2,
        0xc0, 0xcc, 0xf3, 0xca, 0x1b, 0x3d, 0x05, 0x96, 0x4a, 0x55, 0xe6, 0x92, 0x06, 0x49, 0xa5,
        0x61, 0x3d, 0x29, 0x55, 0xfa, 0x28, 0xd2, 0xb8, 0xdd, 0x9d, 0xd0, 0x89, 0x08, 0x3b, 0xbb,
        0xb7, 0x2c, 0x64, 0xd7, 0xd1, 0xe2, 0x77, 0x55, 0x19, 0xc0, 0xbc, 0xf7, 0xdb, 0x45, 0x00,
        0x86, 0x32, 0xd7, 0xab, 0x57, 0x97, 0xb8, 0xa7, 0xc4, 0x43, 0x54, 0x56, 0x8f, 0xb2, 0x2d,
        0x1b, 0x7c, 0x27, 0xd2, 0xdf, 0x4d, 0xdf, 0xdc, 0x6e, 0xdf, 0x44, 0x15, 0x68, 0xb5, 0xaa,
        0x38, 0x47, 0xee, 0x00, 0x55, 0x2d, 0x9b, 0x1b, 0xc3, 0xd9, 0x1d, 0x56, 0xbc, 0x16, 0xdf,
        0xf7, 0x46, 0x3e, 0x55, 0xc3, 0xaf, 0x83, 0xbf, 0xb9, 0xf6, 0x06, 0xdc, 0x2b, 0xf1, 0x14,
        0x5c, 0x0a, 0xed, 0xc3, 0x87, 0x6f, 0x34, 0x4d, 0x68, 0x60, 0x02, 0x9a, 0x15, 0x5e, 0xa9,
        0x83, 0x5d, 0x70, 0x25, 0x37, 0x96, 0x24, 0xd4, 0x33, 0x63, 0x1e, 0x96, 0xef, 0x0b, 0xc7,
        0xc8, 0x9f, 0xdf, 0xb6, 0xd1, 0x89, 0x90, 0xa7, 0x72, 0x7f, 0xa6, 0xc5, 0xe8, 0xa9, 0xeb,
        0x6f, 0xc6, 0xa2, 0x57, 0xe9, 0x94, 0xda, 0xb3, 0x6e, 0x6e, 0x5e, 0xd7, 0x3d, 0xd3, 0xf8,
        0xf2, 0x1a, 0x76, 0x04, 0xa1, 0xb4, 0x93, 0x60, 0xf2, 0x01, 0xd6, 0xd4, 0xa5, 0x82, 0xd3,
        0xb2, 0x07, 0x17, 0x29, 0xeb, 0xcc, 0x05, 0xf4, 0x7d, 0xbd, 0xc5, 0x77, 0xae, 0x55, 0xaf,
        0x60, 0x1c, 0x4f, 0x86, 0xdf, 0x4e, 0x35, 0xa8, 0x2e, 0x09, 0x94, 0xe4, 0x36, 0xbc, 0xfc,
        0x4d, 0x38, 0x78, 0xa2, 0x01, 0xcc, 0x3f, 0x74, 0xbf, 0x58, 0x1d, 0x14, 0xf4, 0x64, 0x13,
        0xe0, 0x13, 0xf8, 0xa5, 0x67, 0xfb, 0x4c, 0xe9, 0xe6, 0xdc, 0x13, 0x2d, 0xa3, 0x49, 0x88,
        0xd0, 0xa9, 0x9b, 0x8a, 0xc5, 0xfd, 0xc1, 0xb9, 0x4f, 0x3e, 0x97, 0xc4, 0x54, 0xf2, 0x97,
        0xdb, 0x12, 0x46, 0x22, 0xac, 0x74, 0x6d, 0xf5, 0x92, 0x5e, 0x3d, 0xfa, 0x9d, 0xe0, 0xf5,
        0xf8, 0x35, 0x05, 0xf4, 0xc9, 0xe7, 0xc4, 0xff, 0xb7, 0x38, 0xa6, 0x80, 0x81, 0x5b, 0xe5,
        0x20, 0x86, 0x00, 0x80,
    ];

    pub const RESIZE_PASS_V_SHADER_BYTES: &[u8] = &[
        0x42, 0x53, 0x43, 0x44, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x4d, 0x53, 0x43, 0x46, 0x00, 0x00, 0x00, 0x00, 0xcf, 0x02, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00,
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x43, 0x00, 0x00, 0x00, 0x01, 0x00, 0x03,
        0x15, 0x80, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x53, 0x53, 0x48, 0x45, 0x58, 0x21, 0x00, 0x76, 0x47, 0x4b, 0x83, 0x84, 0x02,
        0x80, 0x07, 0x5b, 0x80, 0x80, 0x8d, 0x00, 0x20, 0x0b, 0x78, 0xb0, 0x10, 0x00, 0xc1, 0x00,
        0x00, 0x44, 0x02, 0x00, 0x56, 0x46, 0x00, 0xbd, 0x0f, 0xf7, 0xed, 0xbe, 0xbd, 0x6e, 0xcf,
        0x73, 0x37, 0x93, 0x62, 0xc4, 0x6a, 0xca, 0xdc, 0x22, 0x1b, 0x2b, 0xbc, 0x32, 0x6d, 0xcf,
        0xdc, 0x37, 0x66, 0x23, 0x91, 0x70, 0x75, 0x89, 0xa2, 0x06, 0x2e, 0x99, 0x33, 0x5c, 0x5b,
        0x21, 0x28, 0xab, 0x71, 0x31, 0x89, 0x15, 0x7e, 0x2e, 0x5a, 0xd4, 0x45, 0x05, 0xde, 0x84,
        0xc4, 0x8d, 0x33, 0x00, 0x08, 0x00, 0x00, 0x9a, 0x19, 0x00, 0x30, 0xb0, 0x02, 0xdb, 0x7d,
        0xdf, 0x58, 0x97, 0x34, 0x81, 0xb7, 0x4b, 0x2c, 0x4b, 0x93, 0x46, 0xe2, 0x31, 0x84, 0xc4,
        0xd2, 0x56, 0x44, 0x6c, 0x8b, 0x73, 0x2a, 0x23, 0xaf, 0x4e, 0x76, 0xa5, 0x2d, 0xb0, 0x5d,
        0xaa, 0x89, 0x6b, 0x66, 0x69, 0xe0, 0x76, 0xe2, 0x6e, 0x21, 0xbb, 0xf0, 0xa2, 0xc6, 0xff,
        0x0d, 0xff, 0xff, 0xff, 0xff, 0x9c, 0xff, 0x00, 0x20, 0x00, 0x00, 0x08, 0x00, 0x6a, 0xa6,
        0x61, 0x00, 0xfc, 0xe5, 0xa4, 0x96, 0xe0, 0x10, 0xfd, 0xed, 0xfd, 0xfd, 0x17, 0xfc, 0x04,
        0x20, 0x18, 0x82, 0x07, 0xc0, 0xb0, 0x40, 0xca, 0x0d, 0x80, 0xdc, 0x60, 0x80, 0x02, 0xdc,
        0xf7, 0x23, 0xd7, 0xfe, 0xeb, 0x67, 0x6f, 0x43, 0x03, 0x17, 0x3c, 0x00, 0x98, 0xbc, 0x84,
        0xe6, 0xf9, 0x1c, 0x53, 0x91, 0x81, 0x5d, 0xb3, 0x10, 0xc3, 0xd2, 0x78, 0x1f, 0xac, 0x1e,
        0xc1, 0x20, 0x7f, 0xbf, 0x01, 0xb7, 0xfe, 0xb9, 0xaf, 0xcf, 0x25, 0x41, 0x47, 0xf7, 0xdd,
        0x32, 0xc9, 0x64, 0x90, 0x84, 0x11, 0xa2, 0xb3, 0xb4, 0xed, 0x26, 0x8d, 0xe2, 0xf1, 0x18,
        0x13, 0x55, 0x33, 0x51, 0x1f, 0xa3, 0x04, 0x7e, 0x86, 0x64, 0x61, 0x69, 0x6f, 0xf1, 0xcf,
        0x7d, 0xba, 0x5a, 0x48, 0x00, 0x13, 0x85, 0x87, 0xf5, 0x66, 0x88, 0x86, 0x75, 0x95, 0xd6,
        0xa7, 0x14, 0xd4, 0xb2, 0xcf, 0xf2, 0x4c, 0xf8, 0x49, 0x90, 0x93, 0x2f, 0x0d, 0x46, 0x06,
        0x71, 0x3f, 0xf8, 0x65, 0x49, 0xc9, 0x24, 0x43, 0xfe, 0x14, 0xd3, 0x32, 0x36, 0x4f, 0xe7,
        0xe7, 0x9a, 0x8f, 0x46, 0xd2, 0x86, 0xe8, 0x65, 0x7a, 0xf2, 0xe5, 0xa3, 0x7a, 0x57, 0x3d,
        0xdc, 0x7c, 0x93, 0x7d, 0x26, 0xfc, 0x75, 0x54, 0x5e, 0x92, 0xb2, 0x6d, 0xcf, 0x38, 0x40,
        0xa8, 0xf8, 0xd8, 0x74, 0x36, 0xcb, 0xda, 0x93, 0xf4, 0x9e, 0xbd, 0xcb, 0x55, 0x3e, 0xef,
        0x80, 0xaf, 0x2b, 0xa9, 0x84, 0xa4, 0xc0, 0xc8, 0x93, 0xdf, 0xfa, 0x95, 0xe5, 0x21, 0x21,
        0x84, 0xf6, 0x0d, 0x35, 0x0a, 0xe6, 0x95, 0xc8, 0x6e, 0x81, 0xb6, 0xe4, 0x95, 0xc8, 0x0a,
        0xd0, 0xef, 0xf3, 0xd0, 0xc2, 0x01, 0x1e, 0xea, 0x26, 0xf3, 0x25, 0x5d, 0xa5, 0xde, 0x0a,
        0xb6, 0xc7, 0x3e, 0xb7, 0x15, 0xce, 0x2d, 0xaa, 0x78, 0xb1, 0x2e, 0x82, 0x35, 0x10, 0xa5,
        0xda, 0x58, 0x4e, 0xb2, 0x47, 0xf1, 0x8d, 0x52, 0x2c, 0xcc, 0xef, 0x8e, 0xb3, 0x26, 0x6c,
        0x3a, 0x90, 0x97, 0xc3, 0x46, 0x09, 0x02, 0x6f, 0xa9, 0xcf, 0x27, 0xfb, 0x79, 0x5f, 0x4d,
        0x77, 0xc9, 0x10, 0x43, 0xe2, 0x3c, 0x7f, 0xb3, 0x9e, 0x01, 0x1d, 0xe9, 0xc4, 0x8c, 0xe6,
        0x56, 0xec, 0xe6, 0xdf, 0x98, 0x11, 0xd3, 0x83, 0x6c, 0xd4, 0x28, 0xb7, 0xe6, 0x16, 0x99,
        0x36, 0x77, 0x9a, 0xf1, 0xb1, 0x20, 0x08, 0xaa, 0x23, 0xec, 0x18, 0x09, 0x67, 0xb7, 0xd3,
        0x65, 0x7c, 0x79, 0x57, 0x08, 0x98, 0xb3, 0x88, 0xa0, 0xcd, 0x69, 0xd7, 0x8d, 0xf3, 0xe6,
        0xd6, 0x70, 0xde, 0xd0, 0xc2, 0xc9, 0xd3, 0x11, 0x3f, 0x66, 0xb5, 0xd0, 0x5f, 0xda, 0x5e,
        0x1b, 0x0b, 0xb5, 0x5e, 0xa1, 0xf0, 0x6b, 0xf7, 0x77, 0x79, 0x07, 0x97, 0x2a, 0x7a, 0x04,
        0xbb, 0x95, 0xe8, 0x21, 0xb5, 0x64, 0x2a, 0xed, 0x1c, 0x56, 0x3d, 0xd2, 0xd0, 0xc5, 0x35,
        0x71, 0x71, 0x92, 0x7f, 0x33, 0xc7, 0xfd, 0x35, 0xb7, 0xd2, 0xd2, 0x06, 0xc2, 0xde, 0xf9,
        0x48, 0x47, 0xa8, 0xbd, 0x42, 0x83, 0x98, 0xb2, 0x11, 0x38, 0x37, 0xd0, 0xb9, 0x6a, 0xf3,
        0x73, 0xe2, 0x90, 0x69, 0x2e, 0xa6, 0xfa, 0x7f, 0x44, 0x28, 0xac, 0x6d, 0x65, 0x3b, 0x40,
        0x8c, 0x8e, 0x8d, 0x71, 0xf7, 0x5a, 0xf2, 0x74, 0xef, 0x43, 0x15, 0x53, 0x64, 0x04, 0xe8,
        0xaa, 0xa9, 0x64, 0xc3, 0x69, 0xd3, 0xe9, 0x3e, 0x83, 0x5f, 0xb5, 0xde, 0x48, 0x78, 0x76,
        0x03, 0x69, 0x01, 0xeb, 0xca, 0xc0, 0x62, 0x84, 0x63, 0x10, 0x06, 0xe0, 0xa7, 0x30, 0xea,
        0x90, 0xa3, 0x04, 0xed, 0x1c, 0xaf, 0xfa, 0x36, 0x91, 0xb8, 0x52, 0xc0, 0x1d, 0x57, 0x00,
        0x47,
    ];

    pub const CONVERT_SINGLE_PASS_SHADER_BYTES: &[u8] = &[
        0x42, 0x53, 0x43, 0x44, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x4d, 0x53, 0x43, 0x46, 0x00, 0x00, 0x00, 0x00, 0x07, 0x02, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00,
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x43, 0x00, 0x00, 0x00, 0x01, 0x00, 0x03,
        0x15, 0xf4, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x53, 0x53, 0x48, 0x45, 0x58, 0x21, 0x00, 0x31, 0x3d, 0x20, 0xbd, 0xbc, 0x01,
        0xf4, 0x04, 0x5b, 0x80, 0x80, 0x8d, 0x00, 0x20, 0x47, 0x4f, 0xb1, 0x48, 0x00, 0x01, 0x00,
        0x00, 0x23, 0x00, 0x60, 0x56, 0x40, 0x00, 0xfd, 0x0f, 0xf7, 0xed, 0x5e, 0xab, 0xde, 0x66,
        0x25, 0xa2, 0xcd, 0xc0, 0x28, 0x70, 0xc2, 0x83, 0xaf, 0x11, 0x19, 0x55, 0x20, 0x71, 0x54,
        0xd9, 0xc0, 0x2d, 0x70, 0xc9, 0xcc, 0x25, 0xb7, 0x9e, 0x06, 0x0b, 0x57, 0xe7, 0x5c, 0x12,
        0xa9, 0xcb, 0xde, 0x0d, 0xa0, 0xa1, 0xcd, 0xdc, 0x10, 0xd0, 0x00, 0x00, 0x62, 0x00, 0x00,
        0x35, 0x05, 0x00, 0xdc, 0x56, 0xb7, 0xfd, 0x64, 0x39, 0x89, 0xdb, 0x8a, 0xad, 0x74, 0x33,
        0x2d, 0xd8, 0xd3, 0xb5, 0x5a, 0x50, 0xb0, 0xb0, 0x97, 0xa4, 0x48, 0x30, 0x6d, 0xd3, 0x37,
        0x83, 0x34, 0xc1, 0x19, 0x23, 0xe8, 0x16, 0xd0, 0x25, 0x41, 0x57, 0xf7, 0x3e, 0xff, 0xfd,
        0xdf, 0x7f, 0x02, 0xf4, 0x00, 0x00, 0x00, 0x00, 0x06, 0x00, 0xa0, 0xa6, 0x1e, 0x86, 0xa5,
        0x5f, 0xe5, 0x55, 0x09, 0x7a, 0xd6, 0xbf, 0xfb, 0x9b, 0xec, 0xfb, 0x20, 0xa0, 0x8c, 0x16,
        0xa2, 0x01, 0x06, 0x9c, 0x94, 0xcc, 0x73, 0x89, 0x61, 0x55, 0x34, 0x01, 0x6f, 0x86, 0x12,
        0x0b, 0x99, 0xba, 0x80, 0x77, 0xd2, 0x20, 0x97, 0xe8, 0x43, 0xaf, 0xdb, 0x5d, 0x97, 0x7b,
        0x28, 0x97, 0xc1, 0xc5, 0x67, 0x9f, 0x39, 0xa2, 0xe1, 0x71, 0x2e, 0xa1, 0xa6, 0x23, 0x40,
        0xd6, 0x86, 0x5e, 0xf0, 0x90, 0x50, 0x4b, 0xb6, 0x69, 0xdc, 0xe8, 0xf7, 0x85, 0x5c, 0x60,
        0x39, 0xed, 0x1c, 0x8a, 0xb5, 0x8b, 0xd7, 0x3a, 0xea, 0xd1, 0xf6, 0x5c, 0x4c, 0x43, 0x38,
        0x84, 0xde, 0x0f, 0x24, 0x4a, 0x92, 0xb7, 0x38, 0xa2, 0x3a, 0x98, 0x6e, 0x23, 0x5b, 0x95,
        0x88, 0x0a, 0x8d, 0x52, 0x64, 0xbd, 0x92, 0xe0, 0xcd, 0x9e, 0xa0, 0x64, 0x65, 0xd4, 0x6d,
        0x80, 0xd3, 0x74, 0x02, 0x3a, 0x4f, 0x44, 0xab, 0x81, 0x9c, 0xd7, 0xfe, 0xac, 0xfb, 0x01,
        0x3f, 0x19, 0x08, 0xfb, 0x8d, 0x72, 0x99, 0xc6, 0x7b, 0xfe, 0x2d, 0xae, 0x14, 0x05, 0x25,
        0x9a, 0x33, 0x8a, 0xdb, 0x95, 0xcb, 0x71, 0xdd, 0x13, 0x8c, 0x53, 0xd5, 0xd5, 0xec, 0x69,
        0x7b, 0x58, 0xfd, 0x1d, 0xa9, 0x58, 0xf9, 0xcb, 0x1d, 0x74, 0xec, 0xe6, 0x92, 0x4c, 0x5a,
        0x26, 0x81, 0xa6, 0x8d, 0x90, 0x39, 0x8b, 0x7e, 0xce, 0xb8, 0xbc, 0xd1, 0x81, 0x45, 0x53,
        0x20, 0x61, 0x3f, 0x46, 0x69, 0xd9, 0xd5, 0xb6, 0xa2, 0x64, 0x0a, 0x1e, 0x2d, 0x0d, 0x12,
        0x4f, 0x9f, 0x7a, 0x63, 0x52, 0x1e, 0x9b, 0xf3, 0xa9, 0x84, 0x33, 0x1e, 0x76, 0x8e, 0xfd,
        0x28, 0xae, 0xf9, 0x20, 0xdb, 0x33, 0x8c, 0x84, 0x65, 0x0f, 0xdd, 0x6d, 0xe3, 0xcc, 0xf7,
        0x06, 0xab, 0xb1, 0x6e, 0x7e, 0xa5, 0x1a, 0xdf, 0x21, 0x13, 0x24, 0x45, 0xcd, 0x52, 0xb6,
        0x76, 0xb7, 0x3f, 0x0f, 0xf1, 0x26, 0x2a, 0xdf, 0xec, 0x11, 0xb3, 0xb9, 0x61, 0x22, 0x8f,
        0x68, 0x9f, 0xf0, 0x86, 0xfe, 0xd2, 0xb5, 0x2f, 0x92, 0x7d, 0x61, 0x10, 0x91, 0x5f, 0xe2,
        0x05, 0xd0, 0x01, 0xd6, 0xf7, 0xfd, 0x74, 0xff, 0x80, 0x6a, 0xab, 0x6c, 0xb6, 0xd6, 0x2b,
        0x49, 0x4e, 0xba, 0x80, 0x0e, 0x8e, 0xc0, 0x4a, 0x0c, 0x80, 0x0f,
    ];

    pub const CONVERT_PASS1_SHADER_BYTES: &[u8] = &[
        0x42, 0x53, 0x43, 0x44, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x4d, 0x53, 0x43, 0x46, 0x00, 0x00, 0x00, 0x00, 0x77, 0x01, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00,
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x43, 0x00, 0x00, 0x00, 0x01, 0x00, 0x03,
        0x15, 0x38, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x53, 0x53, 0x48, 0x45, 0x58, 0x21, 0x00, 0x08, 0x98, 0xd2, 0x44, 0x2c, 0x01,
        0x38, 0x02, 0x5b, 0x80, 0x80, 0x8d, 0x00, 0x10, 0x81, 0x23, 0x00, 0x00, 0x00, 0x00, 0x45,
        0x33, 0x00, 0x06, 0x0f, 0x36, 0xe9, 0xbd, 0xce, 0x3a, 0x56, 0x91, 0x42, 0xf7, 0x13, 0x13,
        0x0a, 0x97, 0x5c, 0x63, 0x7b, 0xab, 0x9c, 0xaa, 0x90, 0xb8, 0x24, 0x3a, 0x2e, 0x81, 0x4c,
        0x24, 0x89, 0x08, 0x1d, 0x91, 0x15, 0x6e, 0xce, 0x0e, 0x12, 0xb7, 0x85, 0xfe, 0x0e, 0xfd,
        0x73, 0xa1, 0x80, 0x2e, 0x00, 0x80, 0x00, 0x00, 0x22, 0x12, 0x00, 0x80, 0x07, 0x2a, 0xc6,
        0xb4, 0xbe, 0x6f, 0xc6, 0x0a, 0x84, 0x5a, 0xbf, 0x31, 0x85, 0x36, 0x2f, 0x36, 0x0a, 0x8a,
        0x44, 0xb4, 0x46, 0x4a, 0x31, 0x61, 0x8c, 0x98, 0xc1, 0x6f, 0xf6, 0x85, 0xfe, 0x5d, 0x7f,
        0xff, 0xdf, 0xbf, 0x08, 0xe0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xc0, 0x08, 0x30, 0xc8,
        0x9f, 0x23, 0x7e, 0x03, 0xfb, 0xfd, 0x8d, 0xf0, 0x80, 0xd0, 0x1b, 0x68, 0x10, 0x91, 0x00,
        0x4e, 0x57, 0x35, 0x40, 0x69, 0xd4, 0x1c, 0xc3, 0x81, 0xd3, 0xe0, 0xd5, 0x4b, 0xed, 0x6f,
        0x61, 0xfb, 0xe5, 0xcb, 0xc1, 0x30, 0x61, 0xec, 0x30, 0x6d, 0x13, 0x47, 0xdc, 0x4c, 0xcb,
        0x4f, 0xc3, 0x34, 0x18, 0x8e, 0x7a, 0xbe, 0xeb, 0xbf, 0x02, 0xbf, 0xdb, 0x07, 0x1f, 0x6e,
        0xe4, 0x5f, 0x09, 0xb4, 0xaa, 0x7f, 0xde, 0x2a, 0xaf, 0xeb, 0xf6, 0x16, 0x32, 0x04, 0xe3,
        0x0d, 0x3d, 0x4e, 0x46, 0x6b, 0x8b, 0x66, 0x40, 0x1b, 0x62, 0xf1, 0x94, 0x2d, 0x73, 0xae,
        0xa5, 0xc4, 0x5b, 0x1f, 0xab, 0x3e, 0x0f, 0x9b, 0x3b, 0x52, 0x98, 0x44, 0x8c, 0x19, 0xce,
        0xdc, 0x1d, 0x13, 0xe8, 0x61, 0xff, 0xbd, 0x7e, 0x54, 0x4a, 0xcd, 0x43, 0x3b, 0x12, 0xbf,
        0xc1, 0x3f, 0x93, 0x8a, 0x0c, 0x34, 0x62, 0xf1, 0x5f, 0xd3, 0x8d, 0xc9, 0x85, 0xdd, 0xca,
        0xcb, 0x3e, 0xf7, 0x6c, 0x37, 0x3b, 0x22, 0xca, 0x31, 0x9a, 0xaa, 0x91, 0x5d, 0x8e, 0x29,
        0xbf, 0xa5, 0x52, 0xa6, 0xdf, 0xee, 0xe9, 0x86, 0xdd, 0xb6, 0x7b, 0x35, 0xd3, 0xe3, 0x64,
        0x54, 0xd5, 0x2b, 0x81, 0x2c, 0xeb, 0x22, 0xbc, 0x69, 0x70, 0x81, 0xd0, 0x4a, 0xe7, 0xa9,
        0x00, 0x20,
    ];

    pub const CONVERT_PASS2_SHADER_BYTES: &[u8] = &[
        0x42, 0x53, 0x43, 0x44, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x4d, 0x53, 0x43, 0x46, 0x00, 0x00, 0x00, 0x00, 0xf7, 0x01, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00,
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x43, 0x00, 0x00, 0x00, 0x01, 0x00, 0x03,
        0x15, 0xf8, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x53, 0x53, 0x48, 0x45, 0x58, 0x21, 0x00, 0x86, 0xfe, 0x9e, 0xc7, 0xac, 0x01,
        0xf8, 0x03, 0x5b, 0x80, 0x80, 0x8d, 0x00, 0x20, 0x86, 0x3f, 0x0d, 0x08, 0x00, 0x01, 0x00,
        0x00, 0x23, 0x06, 0x60, 0x50, 0x40, 0x00, 0xfd, 0x0f, 0xbd, 0xed, 0xf6, 0xbe, 0x6b, 0xc3,
        0x52, 0x62, 0x66, 0xe0, 0x24, 0x2e, 0x54, 0x18, 0xde, 0xc6, 0x1e, 0xd5, 0x08, 0x52, 0xb5,
        0x34, 0xe2, 0x0a, 0x81, 0x4b, 0xe4, 0x32, 0xa8, 0x67, 0x28, 0x84, 0x1f, 0x9f, 0xb8, 0x22,
        0xe7, 0x92, 0xc6, 0x45, 0xd0, 0x8d, 0x5a, 0xb9, 0x81, 0x8b, 0x97, 0xb9, 0x08, 0x32, 0x00,
        0x00, 0x39, 0x00, 0xb0, 0x1b, 0x02, 0x00, 0x7c, 0x28, 0xbe, 0xc5, 0x5a, 0xff, 0x2c, 0x6b,
        0x85, 0xcb, 0x8d, 0xab, 0x02, 0xb7, 0xaa, 0xd4, 0x2a, 0x14, 0x28, 0x59, 0x4d, 0x96, 0x86,
        0xbf, 0x9b, 0x52, 0x13, 0x6c, 0x29, 0x44, 0x83, 0x88, 0x02, 0x37, 0x6e, 0x22, 0xef, 0x37,
        0xdf, 0x7f, 0xfd, 0xf7, 0x44, 0xff, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x86, 0x84, 0x61,
        0x06, 0x03, 0xdd, 0xd0, 0xe2, 0x22, 0x1a, 0xef, 0x00, 0xef, 0xef, 0xdd, 0xef, 0x00, 0x2d,
        0xd3, 0xa0, 0x5e, 0x01, 0x04, 0x16, 0xdd, 0x33, 0x9b, 0x51, 0xac, 0x88, 0x92, 0x35, 0x5b,
        0x41, 0x10, 0x89, 0x52, 0x5d, 0x2d, 0x74, 0xfa, 0xa9, 0xbd, 0xa0, 0x99, 0xe4, 0xda, 0x32,
        0x1a, 0xf9, 0x0e, 0x99, 0x54, 0x74, 0x8a, 0xf7, 0xb7, 0x61, 0x25, 0x22, 0xe4, 0x6e, 0xe1,
        0x2e, 0xf3, 0xc4, 0xfe, 0x58, 0xe6, 0x5d, 0x29, 0x68, 0xf4, 0x08, 0xc4, 0xbd, 0x6c, 0x98,
        0x96, 0x58, 0x87, 0x3d, 0x2d, 0xd0, 0x94, 0x5e, 0x96, 0x87, 0xed, 0xfc, 0x6e, 0x2b, 0x22,
        0x59, 0x14, 0x39, 0x62, 0x09, 0x4a, 0xb0, 0xac, 0x23, 0xee, 0xa4, 0x50, 0xe3, 0x31, 0x0e,
        0x13, 0xeb, 0x67, 0x3a, 0xa7, 0x9a, 0x87, 0x30, 0x26, 0x7f, 0x84, 0xd3, 0xb3, 0xde, 0x8b,
        0x97, 0x40, 0x08, 0x83, 0x75, 0x50, 0x2a, 0x08, 0x9a, 0x4e, 0x94, 0xa7, 0x2f, 0x9e, 0x8c,
        0x0d, 0x60, 0xab, 0x98, 0xf0, 0x13, 0x21, 0xdd, 0xd6, 0x48, 0xf4, 0xba, 0x2f, 0x39, 0x9c,
        0x2d, 0x1c, 0x64, 0xbe, 0xed, 0x29, 0x66, 0x9b, 0x90, 0x2a, 0x31, 0x25, 0x9b, 0xb0, 0x59,
        0x14, 0x8f, 0x5a, 0x1e, 0x93, 0xa0, 0xd8, 0xbf, 0x76, 0x3f, 0x29, 0x45, 0x16, 0x47, 0x46,
        0x0d, 0xbc, 0x36, 0x43, 0x69, 0x67, 0x2b, 0xa3, 0x59, 0x5f, 0xa2, 0xdd, 0x70, 0x36, 0x79,
        0xc1, 0x07, 0xd9, 0x99, 0xd8, 0x6c, 0xa6, 0x33, 0x33, 0xac, 0x43, 0x1b, 0xbc, 0xfa, 0x65,
        0x10, 0xd9, 0x43, 0xd8, 0xe2, 0x71, 0x17, 0x61, 0x32, 0x5c, 0x1d, 0xae, 0xfe, 0xeb, 0x70,
        0xb5, 0x7d, 0x6e, 0x45, 0x6d, 0x71, 0xfa, 0x9c, 0xbb, 0x0d, 0xfc, 0xaa, 0x69, 0x2a, 0x27,
        0xe1, 0x9a, 0xe0, 0x4a, 0xfd, 0x78, 0x20, 0x7d, 0xb0, 0xb3, 0xfa, 0x8f, 0xca, 0x7f, 0x7e,
        0x22, 0x66, 0x68, 0xbb, 0x2b, 0x90, 0xdd, 0x02, 0xf9, 0x2e, 0xe5, 0x88, 0x25, 0x02, 0x8a,
        0xf1, 0xac, 0x89, 0xf7, 0xf6, 0x6f, 0x06, 0xe7, 0x57, 0xbf, 0x1c, 0xf4, 0x24, 0x5f, 0x9a,
        0x46, 0xf9, 0x4f, 0x94, 0x1a, 0x1f, 0x17, 0x61, 0x00, 0xc0,
    ];
}
//wcap shaders bytecode end

mod tracked {
    use std::sync::{Arc, Condvar, Mutex, Weak};
    use windows::{
        core::*,
        Win32::{Foundation::*, Media::MediaFoundation::*},
    };

    #[derive(Default)]
    pub struct SamplePool {
        state: Arc<PoolState>,
    }
    #[derive(Default)]
    struct PoolState {
        slots: Mutex<Vec<Option<IMFSample>>>,
        ready: Condvar,
    }
    impl SamplePool {
        pub fn new(samples: Vec<IMFSample>) -> Self {
            Self {
                state: Arc::new(PoolState {
                    slots: Mutex::new(samples.into_iter().map(Some).collect()),
                    ready: Condvar::new(),
                }),
            }
        }
        pub fn try_take(&self) -> Option<SampleLease> {
            let mut slots = self.state.slots.lock().unwrap_or_else(|p| p.into_inner());
            let index = slots.iter().position(Option::is_some)?;
            Some(SampleLease {
                sample: slots[index].take(),
                state: self.state.clone(),
                index,
            })
        }
    }

    pub struct SampleLease {
        sample: Option<IMFSample>,
        state: Arc<PoolState>,
        pub index: usize,
    }
    impl SampleLease {
        pub fn sample(&self) -> &IMFSample {
            self.sample.as_ref().unwrap()
        }
        pub unsafe fn submit_transform(
            mut self,
            submit: impl FnOnce(&IMFSample) -> std::result::Result<(), i32>,
        ) -> std::result::Result<(), i32> {
            let callback: IMFAsyncCallback = ReturnSample {
                state: Arc::downgrade(&self.state),
                index: self.index,
            }
            .into();
            let tracked: IMFTrackedSample = self.sample().cast().map_err(|e| e.code().0)?;
            tracked
                .SetAllocator(&callback, None)
                .map_err(|e| e.code().0)?;
            let sample = self.sample.take().unwrap();
            let result = submit(&sample);
            drop(tracked);
            drop(sample);
            result
        }
    }
    impl Drop for SampleLease {
        fn drop(&mut self) {
            if let Some(sample) = self.sample.take() {
                self.state.slots.lock().unwrap_or_else(|p| p.into_inner())[self.index] =
                    Some(sample);
                self.state.ready.notify_one();
            }
        }
    }
    #[implement(IMFAsyncCallback)]
    struct ReturnSample {
        state: Weak<PoolState>,
        index: usize,
    }
    impl IMFAsyncCallback_Impl for ReturnSample_Impl {
        fn GetParameters(&self, _: *mut u32, _: *mut u32) -> Result<()> {
            Err(Error::from_hresult(E_NOTIMPL))
        }
        fn Invoke(&self, result: Ref<'_, IMFAsyncResult>) -> Result<()> {
            let Some(state) = self.state.upgrade() else {
                return Ok(());
            };
            let sample: IMFSample = unsafe { result.ok()?.GetObject()?.cast()? };
            state.slots.lock().unwrap_or_else(|p| p.into_inner())[self.index] = Some(sample);
            state.ready.notify_one();
            Ok(())
        }
    }
}

#[cfg(test)]
mod native_regression_tests {
    use super::*;
    use windows::Win32::{
        Foundation::*,
        Graphics::{Direct3D::*, Direct3D11::*, Dxgi::Common::*},
        System::Com::*,
    };
    #[test]
    fn software_mf_encodes_sparse_timestamps_without_filler() {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok().unwrap();
        }
        let mut encoder = VideoEncoder::create(EncoderPref::Software, 64, 48, 30, 500_000).unwrap();
        assert_eq!(encoder.kind(), EncoderKind::Software);
        let mut out = Vec::new();
        for pts in [0, 333_333, 20_000_000] {
            let pixels = [20u8, 40, 200, 255].repeat(64 * 48);
            encoder
                .encode_bgra(&pixels, 64, 48, 256, pts, &mut out)
                .unwrap();
        }
        encoder.flush(&mut out);
        assert_eq!(
            out.iter().map(|f| f.pts100ns).collect::<Vec<_>>(),
            vec![0, 333_333, 20_000_000]
        );
        assert!(out[0].keyframe);
        assert!(out.iter().all(|f| !f.data.is_empty()));
        let path =
            std::env::temp_dir().join(format!("ir-static-regression-{}.mp4", std::process::id()));
        let path = path.to_string_lossy().into_owned();
        let video: Vec<crate::muxer::VideoSample> = out
            .iter()
            .map(|f| crate::muxer::VideoSample {
                data: &f.data,
                keyframe: f.keyframe,
                pts100ns: f.pts100ns,
            })
            .collect();
        assert_eq!(
            crate::muxer::mux_to_mp4_until(
                &path,
                &video,
                &[],
                48000,
                2,
                30,
                64,
                48,
                30,
                1_200_000_000
            ),
            crate::IR_OK
        );
        let bytes = std::fs::read(&path).unwrap();
        let mvhd = bytes.windows(4).position(|tag| tag == b"mvhd").unwrap();
        assert_eq!(
            u32::from_be_bytes(bytes[mvhd + 20..mvhd + 24].try_into().unwrap()),
            30_000
        );
        let elst = bytes.windows(4).position(|tag| tag == b"elst").unwrap();
        assert_eq!(
            u32::from_be_bytes(bytes[elst + 16..elst + 20].try_into().unwrap()),
            90_000
        );
        assert_eq!(
            crate::muxer::mux_to_mp4_until(
                &path,
                &video,
                &[],
                48000,
                2,
                30,
                64,
                48,
                30,
                1_200_000_000
            ),
            crate::IR_FAIL
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            bytes,
            "A failed save must preserve an existing file"
        );
        std::fs::remove_file(path).unwrap();

        drop(encoder);
        unsafe {
            CoUninitialize();
        }
    }
    #[test]
    fn embedded_shaders_resize_and_convert_on_warp() {
        unsafe {
            let mut device = None;
            let mut context = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_WARP,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .unwrap();
            let device = device.unwrap();
            let context = context.unwrap();
            let mt: ID3D11Multithread = windows::core::Interface::cast(&context).unwrap();
            let _ = mt.SetMultithreadProtected(true);
            let input = GpuInput::create(&device, &context, 64, 48).unwrap();
            let pixels = [0u8, 0, 255, 255].repeat(64 * 48);
            let desc = D3D11_TEXTURE2D_DESC {
                Width: 64,
                Height: 48,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_DEFAULT,
                ..Default::default()
            };
            let initial = D3D11_SUBRESOURCE_DATA {
                pSysMem: pixels.as_ptr() as _,
                SysMemPitch: 256,
                SysMemSlicePitch: 0,
            };
            let mut texture = None;
            device
                .CreateTexture2D(&desc, Some(&initial), Some(&mut texture))
                .unwrap();
            input.submit(&texture.unwrap(), 123);
            for improved in [false, true] {
                let converter = gpu::Converter::create(&input, 32, 24, improved).unwrap();
                converter.run(0).unwrap();
                let out = &converter.outputs()[0].tex;
                let mut desc = D3D11_TEXTURE2D_DESC::default();
                out.GetDesc(&mut desc);
                desc.Usage = D3D11_USAGE_STAGING;
                desc.BindFlags = 0;
                desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
                desc.MiscFlags = 0;
                let mut staging = None;
                device
                    .CreateTexture2D(&desc, None, Some(&mut staging))
                    .unwrap();
                let staging = staging.unwrap();
                context.CopyResource(&staging, out);
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                context
                    .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                    .unwrap();
                let ptr = mapped.pData as *const u8;
                let y = *ptr.add(12 * mapped.RowPitch as usize + 16);
                let u = *ptr.add(24 * mapped.RowPitch as usize + 6 * mapped.RowPitch as usize + 16);
                let v = *ptr.add(24 * mapped.RowPitch as usize + 6 * mapped.RowPitch as usize + 17);
                context.Unmap(&staging, 0);
                assert!((y as i32 - 81).abs() <= 3, "Y={y}, improved={improved}");
                assert!((u as i32 - 90).abs() <= 3, "U={u}, improved={improved}");
                assert!((v as i32 - 240).abs() <= 3, "V={v}, improved={improved}");
            }
        }
    }
    #[test]
    fn software_and_auto_encode_real_4k_frames() {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok().unwrap();
        }
        let pixels = [20u8, 40, 200, 255].repeat(3840 * 2160);
        for preference in [EncoderPref::Software, EncoderPref::Auto] {
            let mut encoder = VideoEncoder::create(preference, 3840, 2160, 30, 4_500_000).unwrap();
            let mut out = Vec::new();
            for pts in [0, 333_333] {
                encoder
                    .encode_bgra(&pixels, 3840, 2160, 3840 * 4, pts, &mut out)
                    .unwrap();
            }
            encoder.flush(&mut out);
            assert_eq!(
                out.iter().map(|f| f.pts100ns).collect::<Vec<_>>(),
                vec![0, 333_333]
            );
            assert!(out[0].keyframe);
            assert!(out.iter().all(|f| !f.data.is_empty()));
            let video: Vec<crate::muxer::VideoSample> = out
                .iter()
                .map(|f| crate::muxer::VideoSample {
                    data: &f.data,
                    keyframe: f.keyframe,
                    pts100ns: f.pts100ns,
                })
                .collect();
            let path = std::env::temp_dir().join(format!(
                "ir-4k-{:?}-{}.mp4",
                preference,
                std::process::id()
            ));
            assert_eq!(
                crate::muxer::mux_to_mp4_until(
                    &path.to_string_lossy(),
                    &video,
                    &[],
                    48000,
                    2,
                    30,
                    3840,
                    2160,
                    30,
                    10_000_000,
                ),
                crate::IR_OK
            );
            eprintln!(
                "4K {:?}: {:?}, {}",
                preference,
                encoder.kind(),
                path.display()
            );
        }
        unsafe {
            CoUninitialize();
        }
    }
}
