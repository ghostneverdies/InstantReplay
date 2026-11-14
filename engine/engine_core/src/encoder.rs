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
    HardwareNvenc = 8,
    Ffmpeg = 16,
}

const HW_FAIL_LIMIT: u32 = 8;

#[derive(Clone, Copy, PartialEq, Eq)]
enum MfFlavour {
    Hardware,
    Software,
}

enum Inner {
    Mf(mf::MfEncoder),
    Ff {
        video: crate::ffmpeg::FfVideo,
        nv12: Vec<u8>,
        tmp: Vec<u8>,
        idr: bool,
    },
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

    pub fn create_ffmpeg(
        video: crate::ffmpeg::FfVideo,
        w: i32,
        h: i32,
        fps: i32,
        bps: u32,
    ) -> Self {
        let dw = (w.max(2) as usize) & !1;
        let dh = (h.max(2) as usize) & !1;
        VideoEncoder {
            inner: Inner::Ff {
                video,
                nv12: vec![0u8; dw * dh * 3 / 2],
                tmp: Vec::new(),
                idr: true,
            },
            w,
            h,
            fps,
            bps,
            consecutive_err: 0,
            frames_since_key: 0,
            convert_ns: 0,
            hw_init_hr: 0,
        }
    }

    pub fn kind(&self) -> EncoderKind {
        match &self.inner {
            Inner::Mf(m) => match m.flavour() {
                MfFlavour::Hardware => EncoderKind::Hardware,
                MfFlavour::Software => EncoderKind::Software,
            },
            Inner::Ff { .. } => EncoderKind::Ffmpeg,
        }
    }

    pub fn kind_code(&self) -> i32 {
        match &self.inner {
            Inner::Mf(_) => self.kind() as i32,
            Inner::Ff { video, .. } => video.kind_code(),
        }
    }

    pub fn flush(&mut self, out: &mut Vec<EncodedFrame>) {
        match &mut self.inner {
            Inner::Mf(mf) => {
                let _ = mf.finish(out);
            }
            Inner::Ff { video, .. } => {
                let _ = video.flush(out);
            }
        }
    }

    pub fn force_idr(&mut self) {
        match &mut self.inner {
            Inner::Mf(m) => m.force_idr(),
            Inner::Ff { idr, .. } => *idr = true,
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
            Inner::Ff {
                video,
                nv12,
                tmp,
                idr,
            } => {
                let dw = (self.w.max(2) as u32) & !1;
                let dh = (self.h.max(2) as u32) & !1;
                let needed = (dw as usize) * (dh as usize) * 3 / 2;
                if nv12.len() < needed {
                    nv12.resize(needed, 0);
                }
                let start = Instant::now();
                let ok = bgra_to_nv12(bgra, src_w, src_h, pitch, dw, dh, &mut nv12[..needed], tmp);
                if !ok {
                    Err(-3)
                } else {
                    let ys = (dw as usize) * (dh as usize);
                    let (y, uv) = nv12[..needed].split_at(ys);
                    let force = std::mem::take(idr);
                    let r =
                        video.encode_nv12(y, dw as usize, uv, dw as usize, pts100ns, force, out);
                    self.convert_ns += start.elapsed().as_nanos() as i64;
                    r.map_err(|message| {
                        crate::ffmpeg::set_last_error(message);
                        -2030
                    })
                }
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
                codec_u32(
                    c,
                    &CODECAPI_AVEncCommonRateControlMode,
                    eAVEncCommonRateControlMode_CBR.0 as u32,
                );
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
            let hd = w >= 1280 || h > 576;
            let _ = out_t.SetUINT32(&MF_MT_VIDEO_CHROMA_SITING, 5);
            let _ = out_t.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, 1);
            let _ = out_t.SetUINT32(&MF_MT_VIDEO_PRIMARIES, if hd { 2 } else { 3 });
            let _ = out_t.SetUINT32(&MF_MT_YUV_MATRIX, if hd { 1 } else { 2 });
            let _ = out_t.SetUINT32(&MF_MT_TRANSFER_FUNCTION, 5);
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
            let _ = in_t.SetUINT32(&MF_MT_VIDEO_CHROMA_SITING, 5);
            let _ = in_t.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, 1);
            let _ = in_t.SetUINT32(&MF_MT_VIDEO_PRIMARIES, if hd { 2 } else { 3 });
            let _ = in_t.SetUINT32(&MF_MT_YUV_MATRIX, if hd { 1 } else { 2 });
            let _ = in_t.SetUINT32(&MF_MT_TRANSFER_FUNCTION, 5);
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

    pub fn submit_at(
        &self,
        frame: &windows::Win32::Graphics::Direct3D11::ID3D11Texture2D,
        pts: i64,
        dx: u32,
        dy: u32,
        w: u32,
        h: u32,
        clear: bool,
    ) {
        let mut stamp = self.stamp.lock().unwrap_or_else(|e| e.into_inner());
        gpu::submit_at(self, frame, dx, dy, w, h, clear);
        stamp.0 = stamp.0.wrapping_add(1);
        stamp.1 = pts;
    }
}

mod ffgpu {
    use super::EncodedFrame;
    use crate::ffmpeg::FfVideo;
    use std::collections::VecDeque;
    use windows::Win32::Graphics::Direct3D11::*;
    use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_NV12, DXGI_SAMPLE_DESC};

    const STAGES: usize = 4;

    pub struct FfBackend {
        pub video: FfVideo,
        pub context: ID3D11DeviceContext,
        pub stages: Vec<ID3D11Texture2D>,
        pub free: Vec<usize>,
        pub pending: VecDeque<(usize, i64)>,
        h: usize,
        idr: bool,
    }

    impl FfBackend {
        pub fn new(
            video: FfVideo,
            device: &ID3D11Device,
            context: &ID3D11DeviceContext,
            w: u32,
            h: u32,
        ) -> Result<Self, i32> {
            let desc = D3D11_TEXTURE2D_DESC {
                Width: w,
                Height: h,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_NV12,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_STAGING,
                BindFlags: 0,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                MiscFlags: 0,
            };
            let mut stages = Vec::with_capacity(STAGES);
            for _ in 0..STAGES {
                let mut tex = None;
                unsafe { device.CreateTexture2D(&desc, None, Some(&mut tex)) }
                    .map_err(|e| e.code().0)?;
                stages.push(tex.ok_or(-2010)?);
            }
            Ok(FfBackend {
                video,
                context: context.clone(),
                stages,
                free: (0..STAGES).rev().collect(),
                pending: VecDeque::new(),
                h: h as usize,
                idr: true,
            })
        }

        pub fn force_idr(&mut self) {
            self.idr = true;
        }

        pub fn encode_oldest(&mut self, out: &mut Vec<EncodedFrame>) -> Result<(), i32> {
            let Some((index, pts)) = self.pending.pop_front() else {
                return Ok(());
            };
            let stage = &self.stages[index];
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            unsafe {
                self.context
                    .Map(stage, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
            }
            .map_err(|e| e.code().0)?;
            let pitch = mapped.RowPitch as usize;
            let h = self.h;
            let result = unsafe {
                let base = mapped.pData as *const u8;
                let y = std::slice::from_raw_parts(base, pitch * h);
                let uv = std::slice::from_raw_parts(base.add(pitch * h), pitch * (h / 2));
                self.video
                    .encode_nv12(y, pitch, uv, pitch, pts, std::mem::take(&mut self.idr), out)
            };
            unsafe { self.context.Unmap(stage, 0) };
            self.free.push(index);
            result.map_err(|message| {
                crate::ffmpeg::set_last_error(message);
                -2030
            })
        }
    }
}

enum Backend {
    Mf(mf::MfEncoder),
    Nv(nvenc::NvEncoder),
    Ff(ffgpu::FfBackend),
}

pub struct GpuEncoder {
    samples: Option<tracked::SamplePool>,
    conv: gpu::Converter,
    backend: Backend,
    input: std::sync::Arc<GpuInput>,
    last_pts: i64,
    convert_ns: i64,
    frame_dur: i64,
    nvenc_hr: i32,
}

fn is_nvidia(device: &windows::Win32::Graphics::Direct3D11::ID3D11Device) -> bool {
    use windows::core::Interface;
    if std::env::var("IR_NVENC").is_ok_and(|v| v == "0") {
        return false;
    }
    device
        .cast::<windows::Win32::Graphics::Dxgi::IDXGIDevice>()
        .and_then(|d| unsafe { d.GetAdapter() })
        .and_then(|a| unsafe { a.GetDesc() })
        .is_ok_and(|desc| desc.VendorId == 0x10DE)
}

impl GpuEncoder {
    pub fn create(
        input: std::sync::Arc<GpuInput>,
        out_w: i32,
        out_h: i32,
        fps: i32,
        bps: u32,
        improved: bool,
        ffmpeg: Option<crate::ffmpeg::FfVideo>,
    ) -> Result<Self, i32> {
        let (w, h) = ((out_w as u32) & !1, (out_h as u32) & !1);
        let fps = fps.max(1);
        let frame_dur = 10_000_000 / fps as i64;
        let conv = gpu::Converter::create(&input, w, h, improved)?;
        if let Some(video) = ffmpeg {
            let backend = ffgpu::FfBackend::new(video, &input.device, &input.context, w, h)?;
            return Ok(GpuEncoder {
                samples: None,
                conv,
                backend: Backend::Ff(backend),
                input,
                last_pts: -1,
                convert_ns: 0,
                frame_dur,
                nvenc_hr: 0,
            });
        }
        let mut nvenc_hr = 0;
        if is_nvidia(&input.device) {
            let textures: Vec<_> = conv.outputs().iter().map(|o| o.tex.clone()).collect();
            match nvenc::NvEncoder::create(
                &input.device,
                &input.context,
                &textures,
                w,
                h,
                fps as u32,
                bps,
            ) {
                Ok(nv) => {
                    return Ok(GpuEncoder {
                        samples: None,
                        conv,
                        backend: Backend::Nv(nv),
                        input,
                        last_pts: -1,
                        convert_ns: 0,
                        frame_dur,
                        nvenc_hr: 0,
                    });
                }
                Err(hr) => {
                    eprintln!(
                        "[encoder] NVENC unavailable (0x{:08X}), using Media Foundation",
                        hr as u32
                    );
                    nvenc_hr = hr;
                }
            }
        }
        let mf = mf::MfEncoder::create_gpu(&input.device, w, h, fps as u32, bps)?;
        let mut samples = Vec::with_capacity(GPU_RING);
        for o in conv.outputs() {
            samples.push(mf::surface_sample(&o.tex)?);
        }
        Ok(GpuEncoder {
            samples: Some(tracked::SamplePool::new(samples)),
            conv,
            backend: Backend::Mf(mf),
            input,
            last_pts: -1,
            convert_ns: 0,
            frame_dur,
            nvenc_hr,
        })
    }

    pub fn nvenc_hr(&self) -> i32 {
        self.nvenc_hr
    }

    pub fn kind(&self) -> EncoderKind {
        match self.backend {
            Backend::Nv(_) => EncoderKind::HardwareNvenc,
            Backend::Mf(_) => EncoderKind::HardwareGpu,
            Backend::Ff(_) => EncoderKind::Ffmpeg,
        }
    }

    pub fn kind_code(&self) -> i32 {
        match &self.backend {
            Backend::Ff(ff) => ff.video.kind_code(),
            _ => self.kind() as i32,
        }
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
        match &mut self.backend {
            Backend::Mf(mf) => mf.force_idr(),
            Backend::Nv(nv) => nv.force_idr(),
            Backend::Ff(ff) => ff.force_idr(),
        }
    }

    pub fn take_convert_ns(&mut self) -> i64 {
        std::mem::take(&mut self.convert_ns)
    }

    fn pick_pts(latest: i64, last: i64, dur: i64, now: i64) -> Option<i64> {
        if latest > last {
            Some(latest)
        } else if last >= 0 && dur > 0 && now >= last.saturating_add(dur * 3 / 2) {
            Some((last + dur).max(now - 2_000_000))
        } else {
            None
        }
    }

    pub fn encode_latest(
        &mut self,
        out: &mut Vec<EncodedFrame>,
        now: i64,
    ) -> Result<Option<i64>, i32> {
        let input = self.input.clone();
        match &mut self.backend {
            Backend::Mf(mf) => {
                mf.wait_room(GPU_RING as u64 - 1, out)?;
                let Some(pool) = self.samples.as_ref() else {
                    return Ok(None);
                };
                let Some(lease) = pool.try_take() else {
                    return Ok(None);
                };
                let started = Instant::now();
                let stamp = input.stamp.lock().unwrap_or_else(|e| e.into_inner());
                let Some(pts) = Self::pick_pts(stamp.1, self.last_pts, self.frame_dur, now) else {
                    return Ok(None);
                };
                self.conv.run(lease.index)?;
                self.convert_ns += started.elapsed().as_nanos() as i64;
                drop(stamp);
                unsafe {
                    lease.submit_transform(|sample| mf.encode_surface(sample, pts, out))?;
                }
                self.last_pts = pts;
                Ok(Some(pts))
            }
            Backend::Ff(ff) => {
                if ff.free.is_empty() {
                    ff.encode_oldest(out)?;
                }
                let Some(&slot) = ff.free.last() else {
                    return Ok(None);
                };
                let started = Instant::now();
                let stamp = input.stamp.lock().unwrap_or_else(|e| e.into_inner());
                let Some(pts) = Self::pick_pts(stamp.1, self.last_pts, self.frame_dur, now) else {
                    drop(stamp);
                    while !ff.pending.is_empty() {
                        ff.encode_oldest(out)?;
                    }
                    return Ok(None);
                };
                self.conv.run(slot)?;
                self.convert_ns += started.elapsed().as_nanos() as i64;
                drop(stamp);
                unsafe {
                    ff.context
                        .CopyResource(&ff.stages[slot], &self.conv.outputs()[slot].tex);
                }
                ff.free.pop();
                ff.pending.push_back((slot, pts));
                self.last_pts = pts;
                while ff.pending.len() > FF_PIPELINE {
                    ff.encode_oldest(out)?;
                }
                Ok(Some(pts))
            }
            Backend::Nv(nv) => {
                if nv.free_slot().is_none() {
                    let keep = nv.in_flight().saturating_sub(1);
                    nv.drain_to(keep, out)?;
                }
                let Some(slot) = nv.free_slot() else {
                    return Ok(None);
                };
                let started = Instant::now();
                let stamp = input.stamp.lock().unwrap_or_else(|e| e.into_inner());
                let Some(pts) = Self::pick_pts(stamp.1, self.last_pts, self.frame_dur, now) else {
                    drop(stamp);
                    nv.drain_to(0, out)?;
                    return Ok(None);
                };
                self.conv.run(slot)?;
                self.convert_ns += started.elapsed().as_nanos() as i64;
                drop(stamp);
                unsafe {
                    nv.submit(slot, pts)?;
                }
                self.last_pts = pts;
                nv.drain_to(NV_PIPELINE, out)?;
                Ok(Some(pts))
            }
        }
    }

    pub fn flush(&mut self, out: &mut Vec<EncodedFrame>) {
        match &mut self.backend {
            Backend::Mf(mf) => {
                let _ = mf.finish(out);
            }
            Backend::Nv(nv) => {
                let _ = nv.drain_to(0, out);
            }
            Backend::Ff(ff) => {
                while !ff.pending.is_empty() {
                    if ff.encode_oldest(out).is_err() {
                        break;
                    }
                }
                let _ = ff.video.flush(out);
            }
        }
    }
}

const NV_PIPELINE: usize = 2;

const FF_PIPELINE: usize = 2;

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

    pub fn submit_at(
        g: &GpuInput,
        frame: &ID3D11Texture2D,
        dx: u32,
        dy: u32,
        w: u32,
        h: u32,
        clear: bool,
    ) {
        unsafe {
            let zeros = if clear {
                vec![0u8; g.w as usize * g.h as usize * 4]
            } else {
                Vec::new()
            };
            g.mt.Enter();
            if clear {
                g.context.UpdateSubresource(
                    &g.latest,
                    0,
                    None,
                    zeros.as_ptr() as *const _,
                    g.w * 4,
                    0,
                );
            }
            let region = D3D11_BOX {
                left: 0,
                top: 0,
                front: 0,
                right: w,
                bottom: h,
                back: 1,
            };
            g.context
                .CopySubresourceRegion(&g.latest, 0, dx, dy, 0, frame, 0, Some(&region));
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

mod nvenc {
    use super::EncodedFrame;
    use std::collections::VecDeque;
    use std::ffi::c_void;
    use std::mem::{offset_of, size_of, zeroed};
    use std::ptr::null_mut;
    use windows::core::{s, w, Interface, GUID};
    use windows::Win32::Graphics::Direct3D11::*;
    use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_NV12, DXGI_SAMPLE_DESC};
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

    type Ptr = *mut c_void;

    const API_VERSION: u32 = 12;
    const fn sv(v: u32) -> u32 {
        API_VERSION | (v << 16) | (0x7 << 28)
    }
    const FUNCTION_LIST_VER: u32 = sv(2);
    const OPEN_SESSION_VER: u32 = sv(1);
    const CONFIG_VER: u32 = sv(8) | (1 << 31);
    const INIT_VER: u32 = sv(5) | (1 << 31);
    const PRESET_VER: u32 = sv(4) | (1 << 31);
    const PIC_VER: u32 = sv(6) | (1 << 31);
    const LOCK_VER: u32 = sv(2);
    const MAP_VER: u32 = sv(4);
    const REGISTER_VER: u32 = sv(4);
    const BITSTREAM_VER: u32 = sv(1);

    const CODEC_H264: GUID = GUID::from_values(
        0x6bc82762,
        0x4e63,
        0x4ca4,
        [0xaa, 0x85, 0x1e, 0x50, 0xf3, 0x21, 0xf6, 0xbf],
    );
    const PROFILE_H264_HIGH: GUID = GUID::from_values(
        0xe7cbc309,
        0x4f7a,
        0x4b89,
        [0xaf, 0x2a, 0xd5, 0x37, 0xc9, 0x2b, 0xe3, 0x10],
    );
    const PRESETS: [GUID; 7] = [
        GUID::from_values(
            0xfc0a8d3e,
            0x45f8,
            0x4cf8,
            [0x80, 0xc7, 0x29, 0x88, 0x71, 0x59, 0x0e, 0xbf],
        ),
        GUID::from_values(
            0xf581cfb8,
            0x88d6,
            0x4381,
            [0x93, 0xf0, 0xdf, 0x13, 0xf9, 0xc2, 0x7d, 0xab],
        ),
        GUID::from_values(
            0x36850110,
            0x3a07,
            0x441f,
            [0x94, 0xd5, 0x36, 0x70, 0x63, 0x1f, 0x91, 0xf6],
        ),
        GUID::from_values(
            0x90a7b826,
            0xdf06,
            0x4862,
            [0xb9, 0xd2, 0xcd, 0x6d, 0x73, 0xa0, 0x86, 0x81],
        ),
        GUID::from_values(
            0x21c6e6b4,
            0x297a,
            0x4cba,
            [0x99, 0x8f, 0xb6, 0xcb, 0xde, 0x72, 0xad, 0xe3],
        ),
        GUID::from_values(
            0x8e75c279,
            0x6299,
            0x4ab6,
            [0x83, 0x02, 0x0b, 0x21, 0x5a, 0x33, 0x5c, 0xf5],
        ),
        GUID::from_values(
            0x84848c12,
            0x6f71,
            0x4c13,
            [0x93, 0x1b, 0x53, 0xe2, 0x83, 0xf5, 0x79, 0x74],
        ),
    ];

    const RC_CBR: u32 = 2;
    const TUNING_HIGH_QUALITY: u32 = 1;
    const BUFFER_NV12: u32 = 1;
    const PIC_STRUCT_FRAME: u32 = 1;
    const PIC_TYPE_IDR: u32 = 3;
    const FLAG_FORCE_IDR: u32 = 0x2;
    const FLAG_OUTPUT_SPSPPS: u32 = 0x4;

    const H_FLAGS: usize = 0;
    const H_IDR_PERIOD: usize = 2;
    const H_VUI: usize = 18;
    const H_CHROMA_FORMAT_IDC: usize = 48;
    const H_FLAG_REPEAT_SPSPPS: u32 = 1 << 12;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Qp {
        inter_p: u32,
        inter_b: u32,
        intra: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct RcParams {
        version: u32,
        mode: u32,
        const_qp: Qp,
        average_bit_rate: u32,
        max_bit_rate: u32,
        vbv_buffer_size: u32,
        vbv_initial_delay: u32,
        flags: u32,
        min_qp: Qp,
        max_qp: Qp,
        initial_rc_qp: Qp,
        temporal_layer_idx_mask: u32,
        temporal_layer_qp: [u8; 8],
        target_quality: u8,
        target_quality_lsb: u8,
        lookahead_depth: u16,
        low_delay_key_frame_scale: u8,
        y_dc_qp_index_offset: i8,
        u_dc_qp_index_offset: i8,
        v_dc_qp_index_offset: i8,
        qp_map_mode: u32,
        multi_pass: u32,
        alpha_layer_bitrate_ratio: u32,
        cb_qp_index_offset: i8,
        cr_qp_index_offset: i8,
        reserved2: u16,
        reserved: [u32; 4],
    }
    const RC_FLAG_ENABLE_AQ: u32 = 1 << 3;
    const RC_FLAG_ZERO_REORDER_DELAY: u32 = 1 << 9;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Config {
        version: u32,
        profile_guid: GUID,
        gop_length: u32,
        frame_interval_p: i32,
        mono_chrome_encoding: u32,
        frame_field_mode: u32,
        mv_precision: u32,
        rc: RcParams,
        codec: [u64; 224],
        reserved: [u32; 278],
        reserved2: [Ptr; 64],
    }

    #[repr(C)]
    struct PresetConfig {
        version: u32,
        cfg: Config,
        reserved1: [u32; 255],
        reserved2: [Ptr; 64],
    }

    #[repr(C)]
    struct InitParams {
        version: u32,
        encode_guid: GUID,
        preset_guid: GUID,
        encode_width: u32,
        encode_height: u32,
        dar_width: u32,
        dar_height: u32,
        frame_rate_num: u32,
        frame_rate_den: u32,
        enable_encode_async: u32,
        enable_ptd: u32,
        flags: u32,
        priv_data_size: u32,
        priv_data: Ptr,
        encode_config: *mut Config,
        max_encode_width: u32,
        max_encode_height: u32,
        max_me_hint_counts_per_block: [u32; 8],
        tuning_info: u32,
        buffer_format: u32,
        reserved: [u32; 287],
        reserved2: [Ptr; 64],
    }

    #[repr(C)]
    struct OpenSessionEx {
        version: u32,
        device_type: u32,
        device: Ptr,
        reserved: Ptr,
        api_version: u32,
        reserved1: [u32; 253],
        reserved2: [Ptr; 64],
    }

    #[repr(C)]
    struct RegisterResource {
        version: u32,
        resource_type: u32,
        width: u32,
        height: u32,
        pitch: u32,
        sub_resource_index: u32,
        resource_to_register: Ptr,
        registered_resource: Ptr,
        buffer_format: u32,
        buffer_usage: u32,
        input_fence_point: Ptr,
        reserved1: [u32; 247],
        reserved2: [Ptr; 61],
    }

    #[repr(C)]
    struct MapInput {
        version: u32,
        sub_resource_index: u32,
        input_resource: Ptr,
        registered_resource: Ptr,
        mapped_resource: Ptr,
        mapped_buffer_fmt: u32,
        reserved1: [u32; 251],
        reserved2: [Ptr; 63],
    }

    #[repr(C)]
    struct CreateBitstream {
        version: u32,
        size: u32,
        memory_heap: u32,
        reserved: u32,
        bitstream_buffer: Ptr,
        bitstream_buffer_ptr: Ptr,
        reserved1: [u32; 58],
        reserved2: [Ptr; 64],
    }

    #[repr(C)]
    struct PicParams {
        version: u32,
        input_width: u32,
        input_height: u32,
        input_pitch: u32,
        encode_pic_flags: u32,
        frame_idx: u32,
        input_time_stamp: u64,
        input_duration: u64,
        input_buffer: Ptr,
        output_bitstream: Ptr,
        completion_event: Ptr,
        buffer_fmt: u32,
        picture_struct: u32,
        picture_type: u32,
        codec_pic_params: [u64; 194],
        me_hint_counts_per_block: [u32; 8],
        me_external_hints: Ptr,
        reserved1: [u32; 6],
        reserved2: [Ptr; 2],
        qp_delta_map: Ptr,
        qp_delta_map_size: u32,
        reserved_bit_fields: u32,
        me_hint_ref_pic_dist: [u16; 2],
        alpha_buffer: Ptr,
        me_external_sb_hints: Ptr,
        me_sb_hints_count: u32,
        reserved3: [u32; 285],
        reserved4: [Ptr; 58],
    }

    #[repr(C)]
    struct LockBitstream {
        version: u32,
        flags: u32,
        output_bitstream: Ptr,
        slice_offsets: *mut u32,
        frame_idx: u32,
        hw_encode_status: u32,
        num_slices: u32,
        bitstream_size_in_bytes: u32,
        output_time_stamp: u64,
        output_duration: u64,
        bitstream_buffer_ptr: Ptr,
        picture_type: u32,
        picture_struct: u32,
        frame_avg_qp: u32,
        frame_satd: u32,
        ltr_frame_idx: u32,
        ltr_frame_bitmap: u32,
        temporal_id: u32,
        reserved: [u32; 12],
        intra_mb_count: u32,
        inter_mb_count: u32,
        average_mvx: i32,
        average_mvy: i32,
        alpha_layer_size_in_bytes: u32,
        reserved1: [u32; 218],
        reserved2: [Ptr; 64],
    }

    #[repr(C)]
    struct FunctionList {
        version: u32,
        reserved: u32,
        slots: [*const c_void; 41],
        reserved2: [*const c_void; 277],
    }
    const SLOT_CREATE_BITSTREAM: usize = 14;
    const SLOT_DESTROY_BITSTREAM: usize = 15;
    const SLOT_ENCODE: usize = 16;
    const SLOT_LOCK: usize = 17;
    const SLOT_UNLOCK: usize = 18;
    const SLOT_INIT: usize = 11;
    const SLOT_MAP: usize = 25;
    const SLOT_UNMAP: usize = 26;
    const SLOT_DESTROY: usize = 27;
    const SLOT_OPEN_EX: usize = 29;
    const SLOT_REGISTER: usize = 30;
    const SLOT_UNREGISTER: usize = 31;
    const SLOT_PRESET_EX: usize = 39;

    const _: () = {
        assert!(size_of::<FunctionList>() == 2552);
        assert!(size_of::<OpenSessionEx>() == 1552);
        assert!(size_of::<InitParams>() == 1808);
        assert!(size_of::<Config>() == 3584);
        assert!(size_of::<RcParams>() == 128);
        assert!(size_of::<PresetConfig>() == 5128);
        assert!(size_of::<RegisterResource>() == 1536);
        assert!(size_of::<MapInput>() == 1544);
        assert!(size_of::<CreateBitstream>() == 776);
        assert!(size_of::<PicParams>() == 3360);
        assert!(size_of::<LockBitstream>() == 1544);
        assert!(offset_of!(InitParams, encode_config) == 88);
        assert!(offset_of!(InitParams, tuning_info) == 136);
        assert!(offset_of!(Config, rc) == 40);
        assert!(offset_of!(Config, codec) == 168);
        assert!(offset_of!(RcParams, average_bit_rate) == 20);
        assert!(offset_of!(RcParams, vbv_buffer_size) == 28);
        assert!(offset_of!(RcParams, multi_pass) == 100);
        assert!(offset_of!(PicParams, input_time_stamp) == 24);
        assert!(offset_of!(PicParams, picture_type) == 72);
        assert!(offset_of!(PicParams, codec_pic_params) == 80);
        assert!(offset_of!(LockBitstream, bitstream_size_in_bytes) == 36);
        assert!(offset_of!(LockBitstream, output_time_stamp) == 40);
        assert!(offset_of!(LockBitstream, bitstream_buffer_ptr) == 56);
        assert!(offset_of!(LockBitstream, picture_type) == 64);
        assert!(offset_of!(RegisterResource, resource_to_register) == 24);
        assert!(offset_of!(RegisterResource, registered_resource) == 32);
        assert!(offset_of!(RegisterResource, buffer_format) == 40);
        assert!(offset_of!(MapInput, mapped_resource) == 24);
        assert!(offset_of!(MapInput, mapped_buffer_fmt) == 32);
        assert!(offset_of!(CreateBitstream, bitstream_buffer) == 16);
    };

    type FnOpenEx = unsafe extern "system" fn(*mut OpenSessionEx, *mut Ptr) -> i32;
    type FnInit = unsafe extern "system" fn(Ptr, *mut InitParams) -> i32;
    type FnPresetEx = unsafe extern "system" fn(Ptr, GUID, GUID, u32, *mut PresetConfig) -> i32;
    type FnCreateBitstream = unsafe extern "system" fn(Ptr, *mut CreateBitstream) -> i32;
    type FnPtr = unsafe extern "system" fn(Ptr, Ptr) -> i32;
    type FnEncode = unsafe extern "system" fn(Ptr, *mut PicParams) -> i32;
    type FnLock = unsafe extern "system" fn(Ptr, *mut LockBitstream) -> i32;
    type FnRegister = unsafe extern "system" fn(Ptr, *mut RegisterResource) -> i32;
    type FnMap = unsafe extern "system" fn(Ptr, *mut MapInput) -> i32;
    type FnDestroy = unsafe extern "system" fn(Ptr) -> i32;

    struct Api {
        open_ex: FnOpenEx,
        init: FnInit,
        preset_ex: FnPresetEx,
        create_bitstream: FnCreateBitstream,
        destroy_bitstream: FnPtr,
        encode: FnEncode,
        lock: FnLock,
        unlock: FnPtr,
        register: FnRegister,
        unregister: FnPtr,
        map: FnMap,
        unmap: FnPtr,
        destroy: FnDestroy,
    }

    fn fail(status: i32) -> i32 {
        (0x8E56_0000u32 | (status as u32 & 0xFFFF)) as i32
    }

    unsafe fn load() -> Result<Api, i32> {
        let lib = LoadLibraryW(w!("nvEncodeAPI64.dll")).map_err(|_| -2001)?;
        let max_version =
            GetProcAddress(lib, s!("NvEncodeAPIGetMaxSupportedVersion")).ok_or(-2002)?;
        let max_version: unsafe extern "system" fn(*mut u32) -> i32 =
            std::mem::transmute(max_version);
        let mut supported = 0u32;
        if max_version(&mut supported) != 0 || supported < (API_VERSION << 4) {
            return Err(-2003);
        }
        let create = GetProcAddress(lib, s!("NvEncodeAPICreateInstance")).ok_or(-2002)?;
        let create: unsafe extern "system" fn(*mut FunctionList) -> i32 =
            std::mem::transmute(create);
        let mut list: FunctionList = zeroed();
        list.version = FUNCTION_LIST_VER;
        let status = create(&mut list);
        if status != 0 {
            return Err(fail(status));
        }
        macro_rules! slot {
            ($i:expr) => {{
                let p = list.slots[$i];
                if p.is_null() {
                    return Err(-2004);
                }
                std::mem::transmute(p)
            }};
        }
        Ok(Api {
            open_ex: slot!(SLOT_OPEN_EX),
            init: slot!(SLOT_INIT),
            preset_ex: slot!(SLOT_PRESET_EX),
            create_bitstream: slot!(SLOT_CREATE_BITSTREAM),
            destroy_bitstream: slot!(SLOT_DESTROY_BITSTREAM),
            encode: slot!(SLOT_ENCODE),
            lock: slot!(SLOT_LOCK),
            unlock: slot!(SLOT_UNLOCK),
            register: slot!(SLOT_REGISTER),
            unregister: slot!(SLOT_UNREGISTER),
            map: slot!(SLOT_MAP),
            unmap: slot!(SLOT_UNMAP),
            destroy: slot!(SLOT_DESTROY),
        })
    }

    unsafe fn put(words: *mut u32, i: usize, v: u32) {
        *words.add(i) = v;
    }

    fn preset_guid() -> GUID {
        let n = std::env::var("IR_NVENC_PRESET")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|n| (1..=7).contains(n))
            .unwrap_or(5);
        PRESETS[n - 1]
    }

    struct Slot {
        src: ID3D11Texture2D,
        copy: Option<ID3D11Texture2D>,
        reg: Ptr,
        out: Ptr,
        mapped: Ptr,
    }

    pub struct NvEncoder {
        api: Api,
        enc: Ptr,
        context: ID3D11DeviceContext,
        w: u32,
        h: u32,
        slots: Vec<Slot>,
        pending: VecDeque<(usize, i64)>,
        force_idr: bool,
        submitted: u64,
        _device: ID3D11Device,
    }

    impl NvEncoder {
        pub fn create(
            device: &ID3D11Device,
            context: &ID3D11DeviceContext,
            textures: &[ID3D11Texture2D],
            w: u32,
            h: u32,
            fps: u32,
            bps: u32,
        ) -> Result<Self, i32> {
            unsafe { Self::create_inner(device, context, textures, w, h, fps.max(1), bps) }
        }

        unsafe fn create_inner(
            device: &ID3D11Device,
            context: &ID3D11DeviceContext,
            textures: &[ID3D11Texture2D],
            w: u32,
            h: u32,
            fps: u32,
            bps: u32,
        ) -> Result<Self, i32> {
            let api = load()?;
            let mut open: OpenSessionEx = zeroed();
            open.version = OPEN_SESSION_VER;
            open.device_type = 0;
            open.device = device.as_raw();
            open.api_version = API_VERSION;
            let mut enc: Ptr = null_mut();
            let status = (api.open_ex)(&mut open, &mut enc);
            if status != 0 || enc.is_null() {
                return Err(fail(status));
            }
            let mut me = NvEncoder {
                api,
                enc,
                context: context.clone(),
                w,
                h,
                slots: Vec::with_capacity(textures.len()),
                pending: VecDeque::new(),
                force_idr: true,
                submitted: 0,
                _device: device.clone(),
            };

            let preset = preset_guid();
            let mut pc: PresetConfig = zeroed();
            pc.version = PRESET_VER;
            pc.cfg.version = CONFIG_VER;
            let status = (me.api.preset_ex)(enc, CODEC_H264, preset, TUNING_HIGH_QUALITY, &mut pc);
            if status != 0 {
                return Err(fail(status));
            }
            let mut cfg = pc.cfg;
            cfg.profile_guid = PROFILE_H264_HIGH;
            cfg.gop_length = fps;
            cfg.frame_interval_p = 1;
            cfg.rc.mode = RC_CBR;
            cfg.rc.average_bit_rate = bps;
            cfg.rc.max_bit_rate = bps;
            cfg.rc.vbv_buffer_size = bps;
            cfg.rc.vbv_initial_delay = 0;
            cfg.rc.flags |= RC_FLAG_ENABLE_AQ | RC_FLAG_ZERO_REORDER_DELAY;
            let hd = w >= 1280 || h > 576;
            let (primaries, transfer, matrix) = if hd { (1, 1, 1) } else { (6, 6, 6) };
            let words = cfg.codec.as_mut_ptr() as *mut u32;
            *words.add(H_FLAGS) |= H_FLAG_REPEAT_SPSPPS;
            put(words, H_IDR_PERIOD, fps);
            put(words, H_CHROMA_FORMAT_IDC, 1);
            put(words, H_VUI + 2, 1);
            put(words, H_VUI + 3, 5);
            put(words, H_VUI + 4, 1);
            put(words, H_VUI + 5, 1);
            put(words, H_VUI + 6, primaries);
            put(words, H_VUI + 7, transfer);
            put(words, H_VUI + 8, matrix);

            let mut init: InitParams = zeroed();
            init.version = INIT_VER;
            init.encode_guid = CODEC_H264;
            init.preset_guid = preset;
            init.encode_width = w;
            init.encode_height = h;
            init.dar_width = w;
            init.dar_height = h;
            init.frame_rate_num = fps;
            init.frame_rate_den = 1;
            init.enable_encode_async = 0;
            init.enable_ptd = 1;
            init.encode_config = &mut cfg;
            init.max_encode_width = w;
            init.max_encode_height = h;
            init.tuning_info = TUNING_HIGH_QUALITY;
            init.buffer_format = BUFFER_NV12;
            let status = (me.api.init)(enc, &mut init);
            if status != 0 {
                return Err(fail(status));
            }

            let mut direct = Vec::with_capacity(textures.len());
            let mut rejected = false;
            for tex in textures {
                match me.register(tex) {
                    Ok(reg) => direct.push(reg),
                    Err(_) => {
                        rejected = true;
                        break;
                    }
                }
            }
            if rejected {
                for reg in direct.drain(..) {
                    (me.api.unregister)(enc, reg);
                }
            }
            for (i, tex) in textures.iter().enumerate() {
                let (copy, reg) = if rejected {
                    let copy = make_copy(device, w, h)?;
                    let reg = me.register(&copy)?;
                    (Some(copy), reg)
                } else {
                    (None, direct[i])
                };
                let mut bs: CreateBitstream = zeroed();
                bs.version = BITSTREAM_VER;
                me.slots.push(Slot {
                    src: tex.clone(),
                    copy,
                    reg,
                    out: null_mut(),
                    mapped: null_mut(),
                });
                let status = (me.api.create_bitstream)(enc, &mut bs);
                if status != 0 || bs.bitstream_buffer.is_null() {
                    return Err(fail(status));
                }
                me.slots[i].out = bs.bitstream_buffer;
            }
            Ok(me)
        }

        unsafe fn register(&self, tex: &ID3D11Texture2D) -> Result<Ptr, i32> {
            let mut r: RegisterResource = zeroed();
            r.version = REGISTER_VER;
            r.resource_type = 0;
            r.width = self.w;
            r.height = self.h;
            r.resource_to_register = tex.as_raw();
            r.buffer_format = BUFFER_NV12;
            r.buffer_usage = 0;
            let status = (self.api.register)(self.enc, &mut r);
            if status != 0 || r.registered_resource.is_null() {
                return Err(fail(status));
            }
            Ok(r.registered_resource)
        }

        pub fn force_idr(&mut self) {
            self.force_idr = true;
        }

        pub fn free_slot(&self) -> Option<usize> {
            self.slots.iter().position(|s| s.mapped.is_null())
        }

        pub fn in_flight(&self) -> usize {
            self.pending.len()
        }

        pub unsafe fn submit(&mut self, slot: usize, pts: i64) -> Result<(), i32> {
            if let Some(copy) = &self.slots[slot].copy {
                self.context.CopyResource(copy, &self.slots[slot].src);
            }
            self.context.Flush();
            let mut map: MapInput = zeroed();
            map.version = MAP_VER;
            map.registered_resource = self.slots[slot].reg;
            let status = (self.api.map)(self.enc, &mut map);
            if status != 0 {
                return Err(fail(status));
            }
            self.slots[slot].mapped = map.mapped_resource;
            let mut pic: PicParams = zeroed();
            pic.version = PIC_VER;
            pic.input_width = self.w;
            pic.input_height = self.h;
            pic.input_pitch = self.w;
            if std::mem::take(&mut self.force_idr) {
                pic.encode_pic_flags = FLAG_FORCE_IDR | FLAG_OUTPUT_SPSPPS;
            }
            pic.input_buffer = map.mapped_resource;
            pic.output_bitstream = self.slots[slot].out;
            pic.buffer_fmt = map.mapped_buffer_fmt;
            pic.picture_struct = PIC_STRUCT_FRAME;
            pic.input_time_stamp = pts.max(0) as u64;
            let status = (self.api.encode)(self.enc, &mut pic);
            if status != 0 {
                (self.api.unmap)(self.enc, map.mapped_resource);
                self.slots[slot].mapped = null_mut();
                return Err(fail(status));
            }
            self.submitted += 1;
            self.pending.push_back((slot, pts));
            Ok(())
        }

        pub fn drain_to(&mut self, keep: usize, out: &mut Vec<EncodedFrame>) -> Result<(), i32> {
            while self.pending.len() > keep {
                let (slot, pts) = self.pending[0];
                unsafe {
                    let mut lock: LockBitstream = zeroed();
                    lock.version = LOCK_VER;
                    lock.output_bitstream = self.slots[slot].out;
                    let status = (self.api.lock)(self.enc, &mut lock);
                    if status != 0 {
                        return Err(fail(status));
                    }
                    let data = if lock.bitstream_buffer_ptr.is_null() {
                        Vec::new()
                    } else {
                        std::slice::from_raw_parts(
                            lock.bitstream_buffer_ptr as *const u8,
                            lock.bitstream_size_in_bytes as usize,
                        )
                        .to_vec()
                    };
                    let keyframe = lock.picture_type == PIC_TYPE_IDR;
                    (self.api.unlock)(self.enc, self.slots[slot].out);
                    (self.api.unmap)(self.enc, self.slots[slot].mapped);
                    self.slots[slot].mapped = null_mut();
                    self.pending.pop_front();
                    if !data.is_empty() {
                        out.push(EncodedFrame {
                            data,
                            keyframe,
                            pts100ns: pts,
                        });
                    }
                }
            }
            Ok(())
        }
    }

    impl Drop for NvEncoder {
        fn drop(&mut self) {
            unsafe {
                let mut sink = Vec::new();
                let _ = self.drain_to(0, &mut sink);
                for s in &mut self.slots {
                    if !s.mapped.is_null() {
                        (self.api.unmap)(self.enc, s.mapped);
                    }
                    if !s.reg.is_null() {
                        (self.api.unregister)(self.enc, s.reg);
                    }
                    if !s.out.is_null() {
                        (self.api.destroy_bitstream)(self.enc, s.out);
                    }
                }
                (self.api.destroy)(self.enc);
            }
        }
    }

    unsafe fn make_copy(device: &ID3D11Device, w: u32, h: u32) -> Result<ID3D11Texture2D, i32> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: w,
            Height: h,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_NV12,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let mut tex = None;
        device
            .CreateTexture2D(&desc, None, Some(&mut tex))
            .map_err(|e| e.code().0)?;
        tex.ok_or(-2010)
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
        return bgra_to_i420_1to1(bgra, dw, dh, row_pitch, dst, dst_w >= 1280 || dst_h > 576);
    }
    bgra_to_i420_scaled(
        bgra,
        sw,
        sh,
        row_pitch,
        dw,
        dh,
        dst,
        dst_w >= 1280 || dst_h > 576,
    )
}

fn bgra_to_i420_scaled(
    bgra: &[u8],
    sw: usize,
    sh: usize,
    row_pitch: u32,
    dw: usize,
    dh: usize,
    dst: &mut [u8],
    hd: bool,
) -> bool {
    let cw = dw / 2;
    let ch = dh / 2;

    let (y_plane, uv) = dst.split_at_mut(dw * dh);
    let (u_plane, v_plane) = uv.split_at_mut(cw * ch);

    y_plane.fill(0);
    u_plane.fill(128);
    v_plane.fill(128);
    let scale = (dw as f64 / sw as f64).min(dh as f64 / sh as f64);
    let content_w = ((sw as f64 * scale).round() as usize).clamp(2, dw) & !1;
    let content_h = ((sh as f64 * scale).round() as usize).clamp(2, dh) & !1;
    let offset_x = ((dw - content_w) / 2) & !1;
    let offset_y = ((dh - content_h) / 2) & !1;

    let scale_x = sw as f64 / content_w as f64;
    let scale_y = sh as f64 / content_h as f64;
    let mut col_x0 = vec![0usize; content_w];
    let mut col_x1 = vec![0usize; content_w];
    let mut col_fx = vec![0f32; content_w];
    for x in 0..content_w {
        let sxv = ((x as f64 + 0.5) * scale_x) - 0.5;
        let x0 = (sxv.floor() as i64).clamp(0, sw as i64 - 2) as usize;
        col_x0[x] = x0;
        col_x1[x] = x0 + 1;
        col_fx[x] = (sxv - x0 as f64) as f32;
    }
    let mut row_y0 = vec![0usize; content_h];
    let mut row_y1 = vec![0usize; content_h];
    let mut row_fy = vec![0f32; content_h];
    for y in 0..content_h {
        let syv = ((y as f64 + 0.5) * scale_y) - 0.5;
        let y0 = (syv.floor() as i64).clamp(0, sh as i64 - 2) as usize;
        row_y0[y] = y0;
        row_y1[y] = y0 + 1;
        row_fy[y] = (syv - y0 as f64) as f32;
    }

    let mut cb_full = vec![128u8; dw * dh];
    let mut cr_full = vec![128u8; dw * dh];

    y_plane
        .chunks_mut(dw)
        .zip(cb_full.chunks_mut(dw))
        .zip(cr_full.chunks_mut(dw))
        .enumerate()
        .for_each(|(y, ((y_row, cb_row), cr_row))| {
            if y < offset_y || y >= offset_y + content_h {
                return;
            }
            let sy = y - offset_y;
            let row0 = &bgra[row_y0[sy] * row_pitch as usize..];
            let row1 = &bgra[row_y1[sy] * row_pitch as usize..];
            let fy = row_fy[sy];
            for x in 0..content_w {
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

                let (yy, uu, vv) = rgb_to_yuv_full(r, g, b, hd);
                let dx = offset_x + x;
                y_row[dx] = yy;
                cb_row[dx] = uu;
                cr_row[dx] = vv;
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

fn bgra_to_i420_1to1(
    bgra: &[u8],
    w: usize,
    h: usize,
    row_pitch: u32,
    dst: &mut [u8],
    hd: bool,
) -> bool {
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
                y_top[x] = y_full(p[2] as i32, p[1] as i32, p[0] as i32, hd);
            }
            for x in 0..w {
                let p = &rb[x * 4..];
                y_bot[x] = y_full(p[2] as i32, p[1] as i32, p[0] as i32, hd);
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
                u_row[x] = u_full(r, g, b, hd);
                v_row[x] = v_full(r, g, b, hd);
            }
        });
    true
}

#[inline(always)]
fn y_full(r: i32, g: i32, b: i32, hd: bool) -> u8 {
    let (cr, cg, cb) = if hd {
        (13933, 46871, 4732)
    } else {
        (19595, 38470, 7471)
    };
    clamp_i32((cr * r + cg * g + cb * b + 32768) >> 16)
}

#[inline(always)]
fn u_full(r: i32, g: i32, b: i32, hd: bool) -> u8 {
    let (cr, cg) = if hd {
        (-7508, -25260)
    } else {
        (-11059, -21709)
    };
    clamp_i32(128 + ((cr * r + cg * g + 32768 * b + 32768) >> 16))
}

#[inline(always)]
fn v_full(r: i32, g: i32, b: i32, hd: bool) -> u8 {
    let (cg, cb) = if hd { (-29763, -3005) } else { (-27439, -5329) };
    clamp_i32(128 + ((32768 * r + cg * g + cb * b + 32768) >> 16))
}

#[inline(always)]
fn rgb_to_yuv_full(r: f32, g: f32, b: f32, hd: bool) -> (u8, u8, u8) {
    if hd {
        (
            clamp_u8(0.2126 * r + 0.7152 * g + 0.0722 * b),
            clamp_u8(-0.114572 * r - 0.385428 * g + 0.5 * b + 128.0),
            clamp_u8(0.5 * r - 0.454153 * g - 0.045847 * b + 128.0),
        )
    } else {
        (
            clamp_u8(0.299 * r + 0.587 * g + 0.114 * b),
            clamp_u8(-0.168736 * r - 0.331264 * g + 0.5 * b + 128.0),
            clamp_u8(0.5 * r - 0.418688 * g - 0.081312 * b + 128.0),
        )
    }
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
                    y_top[x] = y_full(
                        p[2] as i32,
                        p[1] as i32,
                        p[0] as i32,
                        dw >= 1280 || dh > 576,
                    );
                }
                for x in 0..dw {
                    let p = &rb[x * 4..];
                    y_bot[x] = y_full(
                        p[2] as i32,
                        p[1] as i32,
                        p[0] as i32,
                        dw >= 1280 || dh > 576,
                    );
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
                    uv_row[2 * x] = u_full(r, g, b, dw >= 1280 || dh > 576);
                    uv_row[2 * x + 1] = v_full(r, g, b, dw >= 1280 || dh > 576);
                }
            });
        return true;
    }

    let sw = src_w as usize;
    let sh = src_h as usize;
    if sw <= dw && sh <= dh {
        let scale = (dw as f64 / sw as f64).min(dh as f64 / sh as f64);
        if scale >= 1.0 && scale < 1.001 {
            let ox = ((dw - sw) / 2) & !1;
            let oy = ((dh - sh) / 2) & !1;
            let (y_plane, uv_plane) = dst[..needed].split_at_mut(dw * dh);
            y_plane.fill(0);
            uv_plane.fill(128);
            let hd = dw >= 1280 || dh > 576;
            for y in 0..sh {
                let src = &bgra[y * rp..];
                let row = &mut y_plane[(oy + y) * dw + ox..];
                for x in 0..sw {
                    let p = &src[x * 4..];
                    row[x] = y_full(p[2] as i32, p[1] as i32, p[0] as i32, hd);
                }
            }
            for y in 0..sh / 2 {
                let top = &bgra[(y * 2) * rp..];
                let bottom = &bgra[(y * 2 + 1) * rp..];
                let row = &mut uv_plane[((oy / 2) + y) * dw + ox..];
                for x in 0..sw / 2 {
                    let o = x * 8;
                    let (p00, p01, p10, p11) =
                        (&top[o..], &top[o + 4..], &bottom[o..], &bottom[o + 4..]);
                    let b =
                        (p00[0] as i32 + p01[0] as i32 + p10[0] as i32 + p11[0] as i32 + 2) >> 2;
                    let g =
                        (p00[1] as i32 + p01[1] as i32 + p10[1] as i32 + p11[1] as i32 + 2) >> 2;
                    let r =
                        (p00[2] as i32 + p01[2] as i32 + p10[2] as i32 + p11[2] as i32 + 2) >> 2;
                    row[x * 2] = u_full(r, g, b, hd);
                    row[x * 2 + 1] = v_full(r, g, b, hd);
                }
            }
            return true;
        }
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
        dw >= 1280 || dh > 576,
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
    fn matches_bt601_full_range() {
        let (y, u, v) = yuv_of(0, 0, 0);
        close(y, 0);
        close(u, 128);
        close(v, 128);

        let (y, u, v) = yuv_of(255, 255, 255);
        close(y, 255);
        close(u, 128);
        close(v, 128);

        let (y, u, v) = yuv_of(0, 0, 255);
        close(y, 76);
        close(u, 85);
        close(v, 255);

        let (y, u, v) = yuv_of(0, 255, 0);
        close(y, 150);
        close(u, 44);
        close(v, 21);

        let (y, u, v) = yuv_of(255, 0, 0);
        close(y, 29);
        close(u, 255);
        close(v, 107);
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
        assert_eq!(u_plane, &[255, 44, 85, 128]);
        assert_eq!(v_plane, &[107, 21, 255, 128]);
    }

    #[test]
    fn fast_path_matches_bt601_reference() {
        let (y, u, v) = yuv_of(0, 0, 0);
        close(y, 0);
        close(u, 128);
        close(v, 128);

        let (y, u, v) = yuv_of(255, 255, 255);
        close(y, 255);
        close(u, 128);
        close(v, 128);

        let (y, u, v) = yuv_of(0, 0, 255);
        close(y, 76);
        close(u, 85);
        close(v, 255);

        let (y, u, v) = yuv_of(0, 255, 0);
        close(y, 150);
        close(u, 44);
        close(v, 21);

        let (y, u, v) = yuv_of(255, 0, 0);
        close(y, 29);
        close(u, 255);
        close(v, 107);
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
            &mut scaled,
            false
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
        0x00, 0x00, 0x4d, 0x53, 0x43, 0x46, 0x00, 0x00, 0x00, 0x00, 0xd7, 0x01, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00,
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x43, 0x00, 0x00, 0x00, 0x01, 0x00, 0x03,
        0x15, 0xec, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x53, 0x53, 0x48, 0x45, 0x58, 0x21, 0x00, 0x40, 0xac, 0x84, 0x53, 0x8c, 0x01,
        0xec, 0x04, 0x5b, 0x80, 0x80, 0x8d, 0x00, 0x20, 0xc7, 0x4e, 0x80, 0x10, 0x00, 0x81, 0x00,
        0x00, 0x23, 0x00, 0x60, 0x57, 0x47, 0x00, 0xbf, 0x0f, 0xbd, 0x7d, 0xb5, 0xed, 0xf5, 0xe4,
        0x45, 0xb4, 0xca, 0xc4, 0x42, 0xe0, 0x0f, 0x0f, 0xab, 0x04, 0x79, 0x54, 0x20, 0x71, 0xa0,
        0xd2, 0x24, 0xae, 0x12, 0xb8, 0x4f, 0xe6, 0x0b, 0x37, 0xe7, 0x06, 0x12, 0x57, 0xcc, 0x5f,
        0xd0, 0xdf, 0x17, 0xb8, 0x74, 0x33, 0x00, 0x04, 0x00, 0x00, 0x8d, 0x00, 0x00, 0x18, 0x58,
        0x01, 0xd9, 0x3a, 0xb8, 0xcf, 0x6c, 0xa3, 0x2a, 0x1a, 0x77, 0xed, 0x68, 0x89, 0x85, 0x55,
        0x58, 0xe0, 0xb7, 0x2e, 0x86, 0x81, 0xc4, 0x0d, 0xe3, 0x1a, 0xa0, 0x61, 0xf0, 0xc6, 0xbc,
        0x2e, 0xc8, 0x4c, 0x85, 0x77, 0x59, 0x78, 0x97, 0x78, 0xff, 0xff, 0xff, 0xff, 0xfe, 0xff,
        0x00, 0xf1, 0x00, 0x00, 0x00, 0x00, 0x21, 0x01, 0x21, 0x00, 0x8d, 0x82, 0x20, 0x44, 0x22,
        0x5e, 0x17, 0xfd, 0xdb, 0xb1, 0xfb, 0xfb, 0x06, 0xfb, 0x1a, 0xa0, 0x84, 0x19, 0xdc, 0x00,
        0x04, 0x7e, 0x00, 0xd7, 0x28, 0x31, 0x82, 0xf0, 0xca, 0x53, 0x80, 0x2d, 0xed, 0xa4, 0xf6,
        0x40, 0xb1, 0xb5, 0xc0, 0xc0, 0xa7, 0xdd, 0x88, 0x44, 0x0c, 0xb7, 0x48, 0xae, 0xe0, 0x32,
        0x9e, 0xce, 0x76, 0x45, 0x8a, 0x66, 0x43, 0x0c, 0x94, 0x6e, 0xab, 0x67, 0xce, 0x82, 0x6a,
        0xb4, 0xe6, 0x2a, 0x97, 0x6f, 0x42, 0x1a, 0xe9, 0xc9, 0xc6, 0x44, 0x2a, 0x04, 0xb4, 0xea,
        0xeb, 0x30, 0x7e, 0x4b, 0xaa, 0xa5, 0xda, 0xc4, 0xbc, 0xc4, 0xa9, 0xec, 0x4c, 0xb8, 0x2c,
        0x89, 0x43, 0x58, 0x5d, 0xb5, 0xc6, 0xee, 0x85, 0xf4, 0x98, 0x5e, 0x7a, 0xd4, 0xad, 0xcf,
        0x53, 0xae, 0x4c, 0xbe, 0x52, 0xd3, 0x32, 0x39, 0x37, 0x01, 0xbd, 0x15, 0x2d, 0xcc, 0x83,
        0x29, 0x4e, 0xf0, 0xbc, 0x59, 0x20, 0x05, 0x50, 0x5f, 0x77, 0x7f, 0x60, 0x40, 0xa0, 0x27,
        0xfa, 0xcf, 0x08, 0x55, 0x87, 0x2d, 0x4f, 0x6d, 0x16, 0x1b, 0x82, 0x7f, 0xaa, 0xb3, 0xfe,
        0x90, 0x6a, 0x2b, 0x2b, 0x2c, 0xdd, 0x8b, 0x52, 0x2c, 0x55, 0xa1, 0x90, 0x73, 0x52, 0xfa,
        0x8c, 0x38, 0xaa, 0xae, 0x1b, 0x5d, 0x7e, 0x2d, 0x15, 0xe7, 0xf8, 0xda, 0xf7, 0xaf, 0xc0,
        0x49, 0x46, 0x02, 0x70, 0x8f, 0xa6, 0xf6, 0x0b, 0xf1, 0xd4, 0x32, 0xeb, 0x8c, 0xf6, 0x81,
        0xd7, 0x12, 0x2f, 0xa0, 0x73, 0xcf, 0x94, 0x3e, 0xec, 0x12, 0x7e, 0x4f, 0x82, 0x13, 0xcc,
        0x02, 0x0f, 0x95, 0xfa, 0x93, 0x83, 0x51, 0x10, 0x5c, 0x0c, 0xf1, 0xa8, 0x0d, 0x91, 0x53,
        0xfd, 0x9d, 0x06, 0xc5, 0x49, 0xf7, 0xa0, 0xe2, 0xa2, 0x55, 0xf0, 0x58, 0xa3, 0xaf, 0xe9,
        0x8c, 0x5d, 0x84, 0x12, 0xb2, 0x96, 0xe0, 0xee, 0xff, 0x82, 0xc9, 0x49, 0x87, 0x9e, 0xfb,
        0x6b, 0x17, 0x5f, 0xc2, 0x28, 0xe7, 0x00, 0x40,
    ];

    pub const CONVERT_PASS1_SHADER_BYTES: &[u8] = &[
        0x42, 0x53, 0x43, 0x44, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x4d, 0x53, 0x43, 0x46, 0x00, 0x00, 0x00, 0x00, 0x83, 0x01, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00,
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x43, 0x00, 0x00, 0x00, 0x01, 0x00, 0x03,
        0x15, 0x58, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x53, 0x53, 0x48, 0x45, 0x58, 0x21, 0x00, 0xcb, 0x08, 0xa5, 0x8f, 0x38, 0x01,
        0x58, 0x02, 0x5b, 0x80, 0x80, 0x8d, 0x00, 0x10, 0x81, 0x25, 0x00, 0x00, 0x00, 0x00, 0x45,
        0x33, 0x00, 0x70, 0x7f, 0x36, 0xf4, 0xde, 0x67, 0x9d, 0xae, 0x4a, 0xd0, 0xbd, 0xc4, 0x84,
        0x85, 0xa9, 0xb8, 0x31, 0x17, 0x76, 0xaa, 0xf5, 0x62, 0x72, 0xe8, 0x40, 0x04, 0x92, 0x91,
        0x98, 0x22, 0x30, 0x45, 0x2e, 0xb0, 0x73, 0x76, 0xa8, 0x0b, 0x25, 0x2a, 0x71, 0x67, 0xfc,
        0x85, 0x7c, 0xb9, 0xc0, 0x40, 0x97, 0x00, 0x40, 0x00, 0x00, 0xd1, 0x08, 0x00, 0x80, 0x43,
        0x19, 0xd9, 0xb8, 0x24, 0xdf, 0x44, 0x93, 0x57, 0x23, 0x4b, 0x7e, 0x76, 0x82, 0xf0, 0x2b,
        0xb5, 0x50, 0x88, 0x61, 0xbe, 0xc1, 0xce, 0x38, 0x89, 0x71, 0x4b, 0xb0, 0x0b, 0x7e, 0x5e,
        0xf0, 0xf7, 0x22, 0xff, 0xfd, 0xdf, 0x7f, 0x44, 0xf6, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x40, 0x06, 0x08, 0x64, 0x5f, 0x06, 0xbf, 0x06, 0xfd, 0x7e, 0x46, 0xf8, 0x18, 0xe0, 0x81,
        0x14, 0x43, 0x9e, 0x5b, 0x40, 0x3a, 0xef, 0xaf, 0x69, 0x39, 0x20, 0x9d, 0xf4, 0x5c, 0x70,
        0x3f, 0xe2, 0x6f, 0xe1, 0x61, 0x6c, 0xcb, 0xc1, 0x28, 0xeb, 0x2a, 0xa1, 0xae, 0x5c, 0x09,
        0x80, 0x10, 0xa4, 0x85, 0x67, 0xd1, 0xc8, 0x38, 0x64, 0x43, 0xef, 0xa9, 0xbf, 0x76, 0xe5,
        0x75, 0x40, 0xb6, 0x8d, 0xaa, 0x89, 0x7d, 0x88, 0x30, 0x09, 0x89, 0x1b, 0x2b, 0x9f, 0xa7,
        0xaf, 0xc9, 0x75, 0x8d, 0x8f, 0x49, 0xe7, 0xa2, 0x79, 0x2a, 0xc3, 0x53, 0xce, 0x09, 0x5c,
        0x0a, 0x83, 0x05, 0x64, 0x7f, 0x62, 0x86, 0xb2, 0x6a, 0xb1, 0x74, 0xc7, 0xe7, 0x1c, 0x3c,
        0x53, 0x11, 0xeb, 0xad, 0x7c, 0xa5, 0xbe, 0xb2, 0xc5, 0xe6, 0x0a, 0x33, 0xd7, 0x2d, 0x42,
        0x54, 0x40, 0x7f, 0x2d, 0xf9, 0x7d, 0x68, 0x28, 0xc0, 0x1b, 0xee, 0xdf, 0x40, 0x37, 0x9e,
        0x61, 0xdf, 0xac, 0xe9, 0x79, 0xc8, 0x60, 0x36, 0x92, 0x76, 0x67, 0x0e, 0x0e, 0xe2, 0x97,
        0xbb, 0xc5, 0x06, 0x6f, 0x09, 0xad, 0x23, 0x2d, 0xcf, 0xed, 0xac, 0xa5, 0x3b, 0xda, 0x62,
        0x77, 0xfa, 0xa3, 0x51, 0x92, 0xed, 0xef, 0xae, 0x14, 0x95, 0xa5, 0x16, 0x4c, 0x5d, 0x8a,
        0x52, 0xed, 0xb3, 0x44, 0x0c, 0xfe, 0x83, 0x99, 0x1c, 0xf0, 0x94, 0x5e, 0x00, 0x41,
    ];

    pub const CONVERT_PASS2_SHADER_BYTES: &[u8] = &[
        0x42, 0x53, 0x43, 0x44, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x4d, 0x53, 0x43, 0x46, 0x00, 0x00, 0x00, 0x00, 0xdb, 0x01, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00,
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x43, 0x00, 0x00, 0x00, 0x01, 0x00, 0x03,
        0x15, 0xf0, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x53, 0x53, 0x48, 0x45, 0x58, 0x21, 0x00, 0xf1, 0x2e, 0x41, 0xd1, 0x90, 0x01,
        0xf0, 0x03, 0x5b, 0x80, 0x80, 0x8d, 0x00, 0x20, 0x06, 0x3f, 0x01, 0x08, 0x00, 0x31, 0x00,
        0x00, 0x23, 0x06, 0x70, 0x57, 0x40, 0x00, 0xff, 0x0f, 0xbd, 0xbd, 0x7d, 0xef, 0xc3, 0xeb,
        0x6c, 0xdb, 0x5c, 0x4a, 0xdc, 0x0c, 0x3c, 0xa8, 0x91, 0x96, 0xd5, 0xb6, 0x44, 0x1e, 0x7a,
        0x10, 0x0a, 0x5a, 0x4b, 0xe2, 0x32, 0x81, 0x67, 0xe4, 0x08, 0x91, 0x44, 0x50, 0xee, 0x8b,
        0xb9, 0x24, 0x32, 0xd7, 0x42, 0x37, 0x45, 0xe7, 0xdc, 0xc0, 0x91, 0xcb, 0x00, 0x01, 0x00,
        0x00, 0x23, 0x00, 0x00, 0x66, 0x45, 0x00, 0xf6, 0x0f, 0xbf, 0x6d, 0x2b, 0xfd, 0x1f, 0x6b,
        0x86, 0x60, 0x1a, 0xd7, 0x0b, 0xdc, 0x41, 0x15, 0x0c, 0x05, 0xa4, 0x22, 0xfb, 0xc6, 0x62,
        0x81, 0x98, 0x94, 0xc4, 0x35, 0x97, 0x90, 0x22, 0x13, 0x57, 0x50, 0xf7, 0x12, 0xff, 0xfd,
        0xdf, 0x7f, 0x42, 0xf4, 0x00, 0x00, 0x00, 0x00, 0x06, 0x00, 0x80, 0x68, 0x0e, 0x06, 0xff,
        0xe2, 0x24, 0x55, 0x12, 0x44, 0xef, 0x6f, 0xef, 0xef, 0x2f, 0xe0, 0xe0, 0x40, 0x01, 0xd8,
        0x20, 0xe2, 0x7c, 0x02, 0x6a, 0x00, 0x6d, 0x99, 0x21, 0xc2, 0xec, 0xfb, 0xb2, 0x01, 0x26,
        0x0c, 0xb3, 0x81, 0x62, 0x7f, 0x86, 0x06, 0x2c, 0x2f, 0xb4, 0xaa, 0x14, 0x03, 0xfc, 0x6f,
        0x66, 0x4d, 0x46, 0xb4, 0xdc, 0xb3, 0x98, 0x11, 0xf4, 0x9e, 0xe7, 0x30, 0x73, 0xc6, 0xfd,
        0x59, 0xde, 0xdd, 0xc1, 0x69, 0xae, 0x45, 0x17, 0x4c, 0xb2, 0x53, 0xef, 0x31, 0x6a, 0x52,
        0xb8, 0x73, 0x84, 0xcd, 0x69, 0xb8, 0x96, 0x52, 0x57, 0x78, 0x17, 0xaf, 0x04, 0xa1, 0x9d,
        0x35, 0x56, 0xc7, 0x83, 0xa0, 0xf3, 0x18, 0xcc, 0x62, 0xaf, 0xaa, 0x75, 0x86, 0xa2, 0x25,
        0x65, 0x91, 0xbc, 0xff, 0x42, 0xe5, 0x77, 0x45, 0x0e, 0x5c, 0x1a, 0x4c, 0xba, 0x87, 0xbc,
        0x56, 0xc2, 0x29, 0xd0, 0xf1, 0x9a, 0xb1, 0x72, 0x79, 0x27, 0x92, 0xcb, 0x6e, 0x21, 0x95,
        0x94, 0x88, 0xb5, 0x81, 0x15, 0xf0, 0x98, 0x56, 0x6d, 0x8d, 0x34, 0x18, 0x24, 0xe7, 0x2a,
        0x33, 0x83, 0x0d, 0xb3, 0xc0, 0x3e, 0x72, 0xcb, 0x6c, 0x52, 0xb5, 0x10, 0x1c, 0x85, 0x34,
        0x03, 0xc9, 0xed, 0x88, 0x59, 0xbd, 0x73, 0xfc, 0xdc, 0x28, 0x46, 0x77, 0x1f, 0x47, 0xbb,
        0xd1, 0x49, 0x53, 0xb7, 0xf3, 0x0e, 0xd6, 0x67, 0x5e, 0xe6, 0x6a, 0x06, 0x3a, 0xb1, 0x09,
        0xba, 0x3d, 0x97, 0x9c, 0xc8, 0x6f, 0x8d, 0x48, 0x80, 0x6b, 0xd4, 0x5e, 0xa7, 0xa6, 0x5f,
        0x0c, 0x73, 0x8b, 0x83, 0x90, 0xa0, 0x1a, 0x76, 0x9b, 0xf6, 0x61, 0x7f, 0xaa, 0x4b, 0x4e,
        0x81, 0x8a, 0x54, 0xef, 0x6d, 0x22, 0x3f, 0xc1, 0xb4, 0xfa, 0x76, 0x84, 0xa5, 0xec, 0x75,
        0xda, 0xf5, 0xe5, 0xe0, 0xb5, 0x93, 0xce, 0xb8, 0x01, 0x59, 0x21, 0xed, 0x0d, 0x19, 0xe6,
        0x53, 0xa0, 0xff, 0xf5, 0x28, 0xa3, 0x80, 0x71, 0x49, 0x7f, 0xc4, 0xd8,
    ];
}

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
                assert!((y as i32 - 76).abs() <= 3, "Y={y}, improved={improved}");
                assert!((u as i32 - 85).abs() <= 3, "U={u}, improved={improved}");
                assert!((v as i32 - 255).abs() <= 3, "V={v}, improved={improved}");
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
