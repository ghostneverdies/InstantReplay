use crate::encoder::EncodedFrame;
use crate::ffmpeg_sys as sys;
use crate::muxer::{
    self, AudioSample, VideoSample, IR_FAIL, IR_NOT_ENOUGH_DATA, IR_OK, SECOND_100NS,
};
use libloading::Library;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::{Arc, Mutex};

const AVUTIL_MAJOR: u32 = 60;
const SWRESAMPLE_MAJOR: u32 = 6;
const AVCODEC_MAJOR: u32 = 62;
const AVFORMAT_MAJOR: u32 = 62;

const AVERROR_EAGAIN: c_int = -11;
const AVERROR_EOF: c_int = -541_478_725;
const AV_PKT_FLAG_KEY: c_int = 1;
const AV_CODEC_FLAG_GLOBAL_HEADER: c_int = 1 << 22;
const AVIO_FLAG_WRITE: c_int = 2;
const AV_INPUT_BUFFER_PADDING_SIZE: usize = 64;
const AV_NOPTS_VALUE: i64 = i64::MIN;
const AV_PICTURE_TYPE_NONE: c_int = 0;
const AV_PICTURE_TYPE_I: c_int = 1;

const TB_100NS: sys::AVRational = sys::AVRational {
    num: 1,
    den: 10_000_000,
};

static LAST_ERROR: Mutex<String> = Mutex::new(String::new());

pub fn set_last_error(message: impl Into<String>) {
    *LAST_ERROR.lock().unwrap_or_else(|e| e.into_inner()) = message.into();
}

pub fn last_error() -> String {
    LAST_ERROR.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

macro_rules! ff_api {
    ($( $name:ident : fn($($arg:ty),*) $(-> $ret:ty)? ; )*) => {
        pub struct Api {
            $( pub $name: unsafe extern "C" fn($($arg),*) $(-> $ret)?, )*
        }
        impl Api {
            unsafe fn load(libs: &[Library]) -> Result<Api, String> {
                Ok(Api {
                    $( $name: {
                        let symbol = concat!(stringify!($name), "\0").as_bytes();
                        let mut found: Option<unsafe extern "C" fn($($arg),*) $(-> $ret)?> = None;
                        for lib in libs {
                            if let Ok(s) = lib.get::<unsafe extern "C" fn($($arg),*) $(-> $ret)?>(symbol) {
                                found = Some(*s);
                                break;
                            }
                        }
                        found.ok_or_else(|| format!("FFmpeg is missing the function {}", stringify!($name)))?
                    }, )*
                })
            }
        }
    };
}

ff_api! {
    avutil_version: fn() -> u32;
    avcodec_version: fn() -> u32;
    avformat_version: fn() -> u32;
    swresample_version: fn() -> u32;

    av_frame_alloc: fn() -> *mut sys::AVFrame;
    av_frame_free: fn(*mut *mut sys::AVFrame);
    av_frame_get_buffer: fn(*mut sys::AVFrame, c_int) -> c_int;
    av_frame_make_writable: fn(*mut sys::AVFrame) -> c_int;
    av_malloc: fn(usize) -> *mut c_void;
    av_free: fn(*mut c_void);
    av_dict_set: fn(*mut *mut sys::AVDictionary, *const c_char, *const c_char, c_int) -> c_int;
    av_dict_free: fn(*mut *mut sys::AVDictionary);
    av_channel_layout_default: fn(*mut sys::AVChannelLayout, c_int);
    av_channel_layout_copy: fn(*mut sys::AVChannelLayout, *const sys::AVChannelLayout) -> c_int;
    av_strerror: fn(c_int, *mut c_char, usize) -> c_int;
    av_rescale_q: fn(i64, sys::AVRational, sys::AVRational) -> i64;

    avcodec_find_encoder_by_name: fn(*const c_char) -> *const sys::AVCodec;
    avcodec_alloc_context3: fn(*const sys::AVCodec) -> *mut sys::AVCodecContext;
    avcodec_free_context: fn(*mut *mut sys::AVCodecContext);
    avcodec_open2: fn(*mut sys::AVCodecContext, *const sys::AVCodec, *mut *mut sys::AVDictionary) -> c_int;
    avcodec_send_frame: fn(*mut sys::AVCodecContext, *const sys::AVFrame) -> c_int;
    avcodec_receive_packet: fn(*mut sys::AVCodecContext, *mut sys::AVPacket) -> c_int;
    avcodec_parameters_from_context: fn(*mut sys::AVCodecParameters, *const sys::AVCodecContext) -> c_int;
    av_packet_alloc: fn() -> *mut sys::AVPacket;
    av_packet_free: fn(*mut *mut sys::AVPacket);
    av_packet_unref: fn(*mut sys::AVPacket);

    avformat_alloc_output_context2: fn(*mut *mut sys::AVFormatContext, *const sys::AVOutputFormat, *const c_char, *const c_char) -> c_int;
    avformat_new_stream: fn(*mut sys::AVFormatContext, *const sys::AVCodec) -> *mut sys::AVStream;
    avformat_write_header: fn(*mut sys::AVFormatContext, *mut *mut sys::AVDictionary) -> c_int;
    av_interleaved_write_frame: fn(*mut sys::AVFormatContext, *mut sys::AVPacket) -> c_int;
    av_write_trailer: fn(*mut sys::AVFormatContext) -> c_int;
    avio_open: fn(*mut *mut sys::AVIOContext, *const c_char, c_int) -> c_int;
    avio_closep: fn(*mut *mut sys::AVIOContext) -> c_int;
    avformat_free_context: fn(*mut sys::AVFormatContext);
}

pub struct Ff {
    api: Api,
    pub dir: PathBuf,
    _libs: Vec<Library>,
}

impl std::ops::Deref for Ff {
    type Target = Api;
    fn deref(&self) -> &Api {
        &self.api
    }
}

fn lib_file(name: &str, major: u32) -> String {
    if cfg!(windows) {
        format!("{name}-{major}.dll")
    } else {
        format!("lib{name}.so.{major}")
    }
}

fn encoder_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("IR_FFMPEG_DIR") {
        return PathBuf::from(dir);
    }
    #[cfg(windows)]
    if let Some(dir) = module_dir() {
        if let Some(sibling) = dir.parent().map(|root| root.join("encoder")) {
            if sibling.is_dir() {
                return sibling;
            }
        }
        let flat = dir.join("encoder");
        if flat.is_dir() {
            return flat;
        }
        if let Some(sibling) = dir.parent().map(|root| root.join("encoder")) {
            return sibling;
        }
        return flat;
    }
    PathBuf::from("encoder")
}

#[cfg(windows)]
fn module_dir() -> Option<PathBuf> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HMODULE;
    use windows::Win32::System::LibraryLoader::{
        GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
        GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    };
    let mut module = HMODULE::default();
    unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            PCWSTR(module_dir as *const () as *const u16),
            &mut module,
        )
        .ok()?;
        let mut buffer = [0u16; 1024];
        let len = GetModuleFileNameW(Some(module), &mut buffer) as usize;
        if len == 0 || len >= buffer.len() {
            return None;
        }
        PathBuf::from(String::from_utf16_lossy(&buffer[..len]))
            .parent()
            .map(Path::to_path_buf)
    }
}

static LOADED: Mutex<Option<Arc<Ff>>> = Mutex::new(None);

pub fn load() -> Result<Arc<Ff>, String> {
    let mut slot = LOADED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(ff) = slot.as_ref() {
        return Ok(ff.clone());
    }
    let ff = Arc::new(load_from(&encoder_dir())?);
    *slot = Some(ff.clone());
    Ok(ff)
}

fn load_from(dir: &Path) -> Result<Ff, String> {
    let wanted = [
        lib_file("avutil", AVUTIL_MAJOR),
        lib_file("swresample", SWRESAMPLE_MAJOR),
        lib_file("avcodec", AVCODEC_MAJOR),
        lib_file("avformat", AVFORMAT_MAJOR),
    ];
    let missing: Vec<&str> = wanted
        .iter()
        .filter(|f| !dir.join(f).is_file())
        .map(String::as_str)
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "FFmpeg libraries not found in '{}': missing {}. Copy the FFmpeg 8.1 shared build DLLs into that folder.",
            dir.display(),
            missing.join(", ")
        ));
    }
    let mut libs = Vec::with_capacity(wanted.len());
    for file in &wanted {
        let path = dir.join(file);
        let lib = unsafe { Library::new(&path) }
            .map_err(|e| format!("Could not load '{}': {e}", path.display()))?;
        libs.push(lib);
    }
    let api = unsafe { Api::load(&libs)? };
    let (util, codec, format) = unsafe {
        (
            (api.avutil_version)() >> 16,
            (api.avcodec_version)() >> 16,
            (api.avformat_version)() >> 16,
        )
    };
    if util != AVUTIL_MAJOR || codec != AVCODEC_MAJOR || format != AVFORMAT_MAJOR {
        return Err(format!(
            "Unsupported FFmpeg build in '{}': found avutil {util} / avcodec {codec} / avformat {format}, \
             need {AVUTIL_MAJOR} / {AVCODEC_MAJOR} / {AVFORMAT_MAJOR} (FFmpeg 8.x).",
            dir.display()
        ));
    }
    Ok(Ff {
        api,
        dir: dir.to_path_buf(),
        _libs: libs,
    })
}

impl Ff {
    fn err(&self, code: c_int) -> String {
        let mut buffer = [0 as c_char; 128];
        unsafe {
            (self.av_strerror)(code, buffer.as_mut_ptr(), buffer.len());
            format!(
                "{} ({code})",
                CStr::from_ptr(buffer.as_ptr()).to_string_lossy()
            )
        }
    }

    unsafe fn dict(&self, pairs: &[(&str, &str)]) -> *mut sys::AVDictionary {
        let mut dict: *mut sys::AVDictionary = null_mut();
        for (key, value) in pairs {
            if let (Ok(k), Ok(v)) = (CString::new(*key), CString::new(*value)) {
                (self.av_dict_set)(&mut dict, k.as_ptr(), v.as_ptr(), 0);
            }
        }
        dict
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Enc {
    Auto,
    Nvenc,
    Amf,
    Qsv,
    X264,
    Mf,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Rate {
    Cbr,
    Vbr,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Codec {
    Auto,
    Hevc,
    H264,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Preset {
    Speed,
    Balanced,
    Quality,
}

#[derive(Clone, Copy, Debug)]
pub struct VideoOptions {
    pub enc: Enc,
    pub codec: Codec,
    pub rate: Rate,
    pub preset: Preset,
    pub w: i32,
    pub h: i32,
    pub fps: i32,
    pub bps: u32,
}

impl VideoOptions {
    pub fn from_pref(pref: i32, w: i32, h: i32, fps: i32, bps: u32) -> Self {
        let enc = match pref & 0xFF {
            1 => Enc::Nvenc,
            2 => Enc::Amf,
            3 => Enc::Qsv,
            4 => Enc::X264,
            5 => Enc::Mf,
            _ => Enc::Auto,
        };
        let rate = if (pref >> 8) & 1 == 1 {
            Rate::Vbr
        } else {
            Rate::Cbr
        };
        let codec = match (pref >> 12) & 3 {
            1 => Codec::Hevc,
            2 => Codec::H264,
            _ => Codec::Auto,
        };
        let preset = match (pref >> 10) & 3 {
            0 => Preset::Speed,
            2 => Preset::Quality,
            _ => Preset::Balanced,
        };
        VideoOptions {
            enc,
            codec,
            rate,
            preset,
            w: w & !1,
            h: h & !1,
            fps: fps.max(1),
            bps,
        }
    }
}

fn candidates(enc: Enc, codec: Codec) -> &'static [&'static str] {
    match (enc, codec) {
        (Enc::Auto, Codec::Auto) => &[
            "hevc_nvenc",
            "hevc_amf",
            "hevc_qsv",
            "h264_nvenc",
            "h264_amf",
            "h264_qsv",
            "libx264",
        ],
        (Enc::Auto, Codec::Hevc) => &["hevc_nvenc", "hevc_amf", "hevc_qsv"],
        (Enc::Auto, Codec::H264) => &["h264_nvenc", "h264_amf", "h264_qsv", "libx264"],
        (Enc::Nvenc, Codec::H264) => &["h264_nvenc"],
        (Enc::Nvenc, _) => &["hevc_nvenc"],
        (Enc::Amf, Codec::H264) => &["h264_amf"],
        (Enc::Amf, _) => &["hevc_amf"],
        (Enc::Qsv, Codec::H264) => &["h264_qsv"],
        (Enc::Qsv, _) => &["hevc_qsv"],
        (Enc::X264, Codec::Hevc) => &[],
        (Enc::X264, _) => &["libx264"],
        (Enc::Mf, Codec::H264) => &["h264_mf"],
        (Enc::Mf, _) => &["hevc_mf"],
    }
}

fn nvenc_preset(p: Preset) -> &'static str {
    match p {
        Preset::Speed => "p2",
        Preset::Balanced => "p4",
        Preset::Quality => "p6",
    }
}

fn nvenc_rc(r: Rate) -> &'static str {
    match r {
        Rate::Cbr => "cbr",
        Rate::Vbr => "vbr",
    }
}

fn amf_preset(p: Preset) -> &'static str {
    match p {
        Preset::Speed => "speed",
        Preset::Balanced => "balanced",
        Preset::Quality => "quality",
    }
}

fn amf_usage(p: Preset) -> &'static str {
    match p {
        Preset::Speed => "ultralowlatency",
        Preset::Balanced => "transcoding",
        Preset::Quality => "high_quality",
    }
}

fn amf_rc(r: Rate) -> &'static str {
    match r {
        Rate::Cbr => "cbr",
        Rate::Vbr => "vbr_peak",
    }
}

fn qsv_preset(p: Preset) -> &'static str {
    match p {
        Preset::Speed => "veryfast",
        Preset::Balanced => "medium",
        Preset::Quality => "slow",
    }
}

fn x264_preset(_p: Preset) -> &'static str {
    "veryfast"
}

fn mf_quality(p: Preset) -> &'static str {
    match p {
        Preset::Speed => "40",
        Preset::Balanced => "60",
        Preset::Quality => "85",
    }
}

fn mf_rc(r: Rate) -> &'static str {
    match r {
        Rate::Cbr => "cbr",
        Rate::Vbr => "pc_vbr",
    }
}

fn mf_scenario(p: Preset) -> &'static str {
    match p {
        Preset::Speed => "display_remoting",
        Preset::Balanced => "camera_record",
        Preset::Quality => "archive",
    }
}

fn codec_options(name: &str, o: &VideoOptions) -> Vec<(&'static str, &'static str)> {
    let cbr = o.rate == Rate::Cbr;
    match name {
        "hevc_nvenc" => vec![
            ("preset", nvenc_preset(o.preset)),
            ("tune", "hq"),
            ("rc", nvenc_rc(o.rate)),
            ("profile", "main"),
            ("forced-idr", "1"),
            ("spatial-aq", "1"),
            (
                "temporal-aq",
                if o.preset == Preset::Quality {
                    "1"
                } else {
                    "0"
                },
            ),
            (
                "zerolatency",
                if o.preset == Preset::Speed { "1" } else { "0" },
            ),
            (
                "multipass",
                if o.preset == Preset::Quality {
                    "qres"
                } else {
                    "disabled"
                },
            ),
            ("rc-lookahead", if cbr { "0" } else { "20" }),
        ],
        "hevc_amf" => vec![
            ("usage", amf_usage(o.preset)),
            ("quality", amf_preset(o.preset)),
            ("rc", amf_rc(o.rate)),
            ("profile", "main"),
            ("forced_idr", "1"),
            ("header_spacing", "1"),
            ("latency", if o.preset == Preset::Speed { "1" } else { "0" }),
            (
                "preanalysis",
                if o.preset == Preset::Quality {
                    "1"
                } else {
                    "0"
                },
            ),
            ("vbaq", "1"),
            ("enforce_hrd", if cbr { "1" } else { "0" }),
        ],
        "hevc_qsv" => vec![
            ("preset", qsv_preset(o.preset)),
            ("profile", "main"),
            ("forced_idr", "1"),
            ("look_ahead", if cbr { "0" } else { "1" }),
            ("low_delay_brc", if cbr { "1" } else { "0" }),
        ],
        "libx265" => {
            let mut v = vec![
                ("preset", x264_preset(o.preset)),
                ("profile", "main"),
                ("forced-idr", "1"),
            ];
            if cbr {
                v.push(("x265-params", "hrd=1:repeat-headers=1"));
            }
            v
        }
        "hevc_mf" => vec![
            ("rate_control", mf_rc(o.rate)),
            ("scenario", mf_scenario(o.preset)),
            ("quality", mf_quality(o.preset)),
        ],
        "h264_nvenc" => vec![
            ("preset", nvenc_preset(o.preset)),
            ("tune", "hq"),
            ("rc", nvenc_rc(o.rate)),
            ("profile", "high"),
            ("forced-idr", "1"),
            ("spatial-aq", "1"),
            (
                "temporal-aq",
                if o.preset == Preset::Quality {
                    "1"
                } else {
                    "0"
                },
            ),
            (
                "multipass",
                if o.preset == Preset::Quality {
                    "qres"
                } else {
                    "disabled"
                },
            ),
            ("rc-lookahead", if cbr { "0" } else { "20" }),
        ],
        "h264_amf" => vec![
            ("usage", amf_usage(o.preset)),
            ("quality", amf_preset(o.preset)),
            ("rc", amf_rc(o.rate)),
            ("profile", "high"),
            ("forced_idr", "1"),
            ("header_spacing", "1"),
            (
                "preanalysis",
                if o.preset == Preset::Quality {
                    "1"
                } else {
                    "0"
                },
            ),
            ("vbaq", "1"),
            ("enforce_hrd", if cbr { "1" } else { "0" }),
        ],
        "h264_qsv" => vec![
            ("preset", qsv_preset(o.preset)),
            ("profile", "high"),
            ("forced_idr", "1"),
            ("look_ahead", if cbr { "0" } else { "1" }),
            ("low_delay_brc", if cbr { "1" } else { "0" }),
        ],
        "libx264" => vec![
            ("preset", x264_preset(o.preset)),
            ("profile", "high"),
            ("forced-idr", "1"),
        ],
        "h264_mf" => vec![
            ("rate_control", mf_rc(o.rate)),
            ("scenario", mf_scenario(o.preset)),
            ("quality", mf_quality(o.preset)),
        ],
        _ => Vec::new(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Container {
    Mp4,
    Mov,
    Mkv,
    Ts,
    Flv,
}

impl Container {
    pub const ALL: [Container; 5] = [
        Container::Mp4,
        Container::Mov,
        Container::Mkv,
        Container::Ts,
        Container::Flv,
    ];

    pub fn from_path(path: &str) -> Container {
        match Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref()
        {
            Some("mkv") | Some("mka") => Container::Mkv,
            Some("mov") => Container::Mov,
            Some("ts") | Some("m2ts") | Some("mts") => Container::Ts,
            Some("flv") => Container::Flv,
            _ => Container::Mp4,
        }
    }

    pub fn muxer(self) -> &'static str {
        match self {
            Container::Mp4 => "mp4",
            Container::Mov => "mov",
            Container::Mkv => "matroska",
            Container::Ts => "mpegts",
            Container::Flv => "flv",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Container::Mp4 => "mp4",
            Container::Mov => "mov",
            Container::Mkv => "mkv",
            Container::Ts => "ts",
            Container::Flv => "flv",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Container::Mp4 => "MP4",
            Container::Mov => "MOV",
            Container::Mkv => "MKV",
            Container::Ts => "MPEG-TS",
            Container::Flv => "FLV",
        }
    }

    pub fn supports_faststart(self) -> bool {
        matches!(self, Container::Mp4 | Container::Mov)
    }
}

fn has_param_sets(data: &[u8]) -> bool {
    let (mut vps, mut hevc_sps, mut hevc_pps) = (false, false, false);
    let (mut avc_sps, mut avc_pps) = (false, false);
    for (start, _) in nal_units(data).take(6) {
        match (data[start] >> 1) & 0x3F {
            32 => vps = true,
            33 => hevc_sps = true,
            34 => hevc_pps = true,
            _ => {}
        }
        match data[start] & 0x1F {
            7 => avc_sps = true,
            8 => avc_pps = true,
            _ => {}
        }
    }
    (vps && hevc_sps && hevc_pps) || (avc_sps && avc_pps)
}

fn nal_units(data: &[u8]) -> impl Iterator<Item = (usize, usize)> + '_ {
    let mut starts: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            starts.push((i, i + 3));
            i += 3;
        } else {
            i += 1;
        }
    }
    (0..starts.len()).filter_map(move |n| {
        let payload = starts[n].1;
        let mut end = if n + 1 < starts.len() {
            starts[n + 1].0
        } else {
            data.len()
        };
        while end > payload && data[end - 1] == 0 && n + 1 < starts.len() {
            end -= 1;
        }
        (payload < end).then_some((payload, end))
    })
}

fn extract_param_sets(data: &[u8]) -> Vec<u8> {
    let mut vps = Vec::new();
    let mut sps = Vec::new();
    let mut pps = Vec::new();
    let mut avc_sps = Vec::new();
    let mut avc_pps = Vec::new();
    for (start, end) in nal_units(data) {
        match (data[start] >> 1) & 0x3F {
            32 if vps.is_empty() => vps = data[start..end].to_vec(),
            33 if sps.is_empty() => sps = data[start..end].to_vec(),
            34 if pps.is_empty() => pps = data[start..end].to_vec(),
            _ => {}
        }
        match data[start] & 0x1F {
            7 if avc_sps.is_empty() => avc_sps = data[start..end].to_vec(),
            8 if avc_pps.is_empty() => avc_pps = data[start..end].to_vec(),
            _ => {}
        }
    }
    if vps.is_empty() || sps.is_empty() || pps.is_empty() {
        if avc_sps.is_empty() || avc_pps.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(avc_sps.len() + avc_pps.len() + 8);
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&avc_sps);
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&avc_pps);
        return out;
    }
    let mut out = Vec::with_capacity(vps.len() + sps.len() + pps.len() + 12);
    out.extend_from_slice(&[0, 0, 0, 1]);
    out.extend_from_slice(&vps);
    out.extend_from_slice(&[0, 0, 0, 1]);
    out.extend_from_slice(&sps);
    out.extend_from_slice(&[0, 0, 0, 1]);
    out.extend_from_slice(&pps);
    out
}

fn parameter_sets_are_hevc(data: &[u8]) -> bool {
    nal_units(data).any(|(start, _)| ((data[start] >> 1) & 0x3F) == 32)
}

fn packet_has_picture(data: &[u8], hevc: bool) -> bool {
    let mut found_nal = false;
    for (start, _) in nal_units(data) {
        found_nal = true;
        let nal_type = if hevc {
            (data[start] >> 1) & 0x3F
        } else {
            data[start] & 0x1F
        };
        if (hevc && nal_type <= 31) || (!hevc && (1..=5).contains(&nal_type)) {
            return true;
        }
    }
    !found_nal
}

pub struct FfVideo {
    ff: Arc<Ff>,
    ctx: *mut sys::AVCodecContext,
    frame: *mut sys::AVFrame,
    pkt: *mut sys::AVPacket,
    pub name: &'static str,
    extradata: Vec<u8>,
    w: usize,
    h: usize,
    flushed: bool,
}

unsafe impl Send for FfVideo {}

impl FfVideo {
    pub fn open(opts: &VideoOptions) -> Result<FfVideo, String> {
        let ff = load()?;
        let mut failures = Vec::new();
        for name in candidates(opts.enc, opts.codec) {
            match unsafe { Self::try_open(&ff, name, opts) } {
                Ok(video) => return Ok(video),
                Err(e) => failures.push(format!("{name}: {e}")),
            }
        }
        Err(format!(
            "No permitted video encoder could be opened ({}).",
            failures.join("; ")
        ))
    }

    unsafe fn try_open(
        ff: &Arc<Ff>,
        name: &'static str,
        o: &VideoOptions,
    ) -> Result<FfVideo, String> {
        let cname = CString::new(name).map_err(|e| e.to_string())?;
        let codec = (ff.avcodec_find_encoder_by_name)(cname.as_ptr());
        if codec.is_null() {
            return Err("not included in this FFmpeg build".into());
        }
        let mut ctx = (ff.avcodec_alloc_context3)(codec);
        if ctx.is_null() {
            return Err("out of memory".into());
        }
        let c = &mut *ctx;
        let hevc = name.starts_with("hevc_");
        let h264 = name.starts_with("h264_") || name == "libx264";
        let bps = if h264 {
            match o.preset {
                Preset::Quality => o.bps as i64 * 4 / 3,
                Preset::Balanced => o.bps as i64 * 7 / 5,
                Preset::Speed => o.bps as i64 * 3 / 2,
            }
        } else {
            o.bps as i64
        };
        c.width = o.w;
        c.height = o.h;
        c.time_base = TB_100NS;
        c.framerate = sys::AVRational { num: o.fps, den: 1 };
        let planar = name == "libx264";
        c.pix_fmt = if planar {
            sys::AVPixelFormat_AV_PIX_FMT_YUV420P
        } else {
            sys::AVPixelFormat_AV_PIX_FMT_NV12
        };
        c.gop_size = o.fps.saturating_mul(2);
        c.max_b_frames = 0;
        c.profile = if hevc { 1 } else { 100 };
        c.bit_rate = bps;
        match o.rate {
            Rate::Cbr => {
                c.rc_max_rate = bps;
                c.rc_buffer_size = bps.min(i32::MAX as i64) as c_int;
            }
            Rate::Vbr => {
                c.rc_max_rate = bps * 3 / 2;
                c.rc_buffer_size = (bps as u64 * 2).min(i32::MAX as u64) as c_int;
            }
        }
        c.flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
        let hd = o.w >= 1280 || o.h > 576;
        let code = if hd { 1 } else { 6 };
        c.color_range = 2;
        c.colorspace = code;
        c.color_primaries = code;
        c.color_trc = code;

        let pairs = codec_options(name, o);
        let mut dict = ff.dict(&pairs);
        let rc = (ff.avcodec_open2)(ctx, codec, &mut dict);
        (ff.av_dict_free)(&mut dict);
        if rc < 0 {
            (ff.avcodec_free_context)(&mut ctx);
            return Err(ff.err(rc));
        }
        let extradata = if (*ctx).extradata.is_null() || (*ctx).extradata_size <= 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts((*ctx).extradata, (*ctx).extradata_size as usize).to_vec()
        };

        let frame = (ff.av_frame_alloc)();
        let pkt = (ff.av_packet_alloc)();
        if frame.is_null() || pkt.is_null() {
            let (mut f, mut p) = (frame, pkt);
            if !f.is_null() {
                (ff.av_frame_free)(&mut f);
            }
            if !p.is_null() {
                (ff.av_packet_free)(&mut p);
            }
            (ff.avcodec_free_context)(&mut ctx);
            return Err("out of memory".into());
        }
        (*frame).width = o.w;
        (*frame).height = o.h;
        (*frame).format = c.pix_fmt;
        let rc = (ff.av_frame_get_buffer)(frame, 0);
        if rc < 0 {
            let (mut f, mut p) = (frame, pkt);
            (ff.av_frame_free)(&mut f);
            (ff.av_packet_free)(&mut p);
            (ff.avcodec_free_context)(&mut ctx);
            return Err(ff.err(rc));
        }
        Ok(FfVideo {
            ff: ff.clone(),
            ctx,
            frame,
            pkt,
            name,
            extradata,
            w: o.w as usize,
            h: o.h as usize,
            flushed: false,
        })
    }

    pub fn kind_code(&self) -> i32 {
        match self.name {
            "hevc_nvenc" => 16,
            "hevc_amf" => 17,
            "hevc_qsv" => 18,
            "libx265" => 19,
            "hevc_mf" => 20,
            "h264_nvenc" => 26,
            "h264_amf" => 27,
            "h264_qsv" => 28,
            "libx264" => 29,
            _ => 30,
        }
    }

    pub fn encode_nv12(
        &mut self,
        y: &[u8],
        y_stride: usize,
        uv: &[u8],
        uv_stride: usize,
        pts: i64,
        idr: bool,
        out: &mut Vec<EncodedFrame>,
    ) -> Result<(), String> {
        if self.flushed {
            return Err("encoder was already flushed".into());
        }
        let (w, h) = (self.w, self.h);
        if y_stride < w
            || uv_stride < w
            || y.len() < y_stride * (h - 1) + w
            || uv.len() < uv_stride * (h / 2 - 1) + w
        {
            return Err("NV12 frame is smaller than the encoder size".into());
        }
        let ff = self.ff.clone();
        unsafe {
            let rc = (ff.av_frame_make_writable)(self.frame);
            if rc < 0 {
                return Err(ff.err(rc));
            }
            let f = &mut *self.frame;
            for row in 0..h {
                std::ptr::copy_nonoverlapping(
                    y.as_ptr().add(row * y_stride),
                    f.data[0].add(row * f.linesize[0] as usize),
                    w,
                );
            }
            if self.name == "libx264" {
                for row in 0..h / 2 {
                    let source = uv.as_ptr().add(row * uv_stride);
                    let u = f.data[1].add(row * f.linesize[1] as usize);
                    let v = f.data[2].add(row * f.linesize[2] as usize);
                    for x in 0..w / 2 {
                        *u.add(x) = *source.add(x * 2);
                        *v.add(x) = *source.add(x * 2 + 1);
                    }
                }
            } else {
                for row in 0..h / 2 {
                    std::ptr::copy_nonoverlapping(
                        uv.as_ptr().add(row * uv_stride),
                        f.data[1].add(row * f.linesize[1] as usize),
                        w,
                    );
                }
            }
            f.pts = pts;
            f.pict_type = if idr {
                AV_PICTURE_TYPE_I
            } else {
                AV_PICTURE_TYPE_NONE
            };
            let mut rc = (ff.avcodec_send_frame)(self.ctx, self.frame);
            if rc == AVERROR_EAGAIN {
                self.receive(out)?;
                rc = (ff.avcodec_send_frame)(self.ctx, self.frame);
            }
            if rc < 0 {
                return Err(ff.err(rc));
            }
        }
        self.receive(out)
    }

    fn receive(&mut self, out: &mut Vec<EncodedFrame>) -> Result<(), String> {
        let ff = self.ff.clone();
        unsafe {
            loop {
                let rc = (ff.avcodec_receive_packet)(self.ctx, self.pkt);
                if rc == AVERROR_EAGAIN || rc == AVERROR_EOF {
                    return Ok(());
                }
                if rc < 0 {
                    return Err(ff.err(rc));
                }
                let p = &*self.pkt;
                if !p.data.is_null() && p.size > 0 {
                    let data = std::slice::from_raw_parts(p.data, p.size as usize);
                    let hevc = self.name.starts_with("hevc_");
                    if !packet_has_picture(data, hevc) {
                        (ff.av_packet_unref)(self.pkt);
                        continue;
                    }
                    let keyframe = p.flags & AV_PKT_FLAG_KEY != 0;
                    let pts = if p.pts != AV_NOPTS_VALUE {
                        p.pts
                    } else {
                        p.dts
                    };
                    let bytes = if keyframe && !self.extradata.is_empty() && !has_param_sets(data) {
                        let mut joined = Vec::with_capacity(self.extradata.len() + data.len());
                        joined.extend_from_slice(&self.extradata);
                        joined.extend_from_slice(data);
                        joined
                    } else {
                        data.to_vec()
                    };
                    out.push(EncodedFrame {
                        data: bytes,
                        keyframe,
                        pts100ns: pts,
                    });
                }
                (ff.av_packet_unref)(self.pkt);
            }
        }
    }

    pub fn flush(&mut self, out: &mut Vec<EncodedFrame>) -> Result<(), String> {
        if self.flushed {
            return Ok(());
        }
        self.flushed = true;
        let rc = unsafe { (self.ff.avcodec_send_frame)(self.ctx, null()) };
        if rc < 0 && rc != AVERROR_EOF {
            return Err(self.ff.err(rc));
        }
        self.receive(out)
    }
}

impl Drop for FfVideo {
    fn drop(&mut self) {
        unsafe {
            (self.ff.av_frame_free)(&mut self.frame);
            (self.ff.av_packet_free)(&mut self.pkt);
            (self.ff.avcodec_free_context)(&mut self.ctx);
        }
    }
}

struct Output<'a> {
    ff: &'a Ff,
    ctx: *mut sys::AVFormatContext,
    io_open: bool,
}

impl Drop for Output<'_> {
    fn drop(&mut self) {
        unsafe {
            if self.io_open {
                (self.ff.avio_closep)(&mut (*self.ctx).pb);
            }
            if !self.ctx.is_null() {
                (self.ff.avformat_free_context)(self.ctx);
            }
        }
    }
}

struct AudioPacket<'a> {
    ff: &'a Ff,
    pkt: *mut sys::AVPacket,
}

impl Drop for AudioPacket<'_> {
    fn drop(&mut self) {
        unsafe { (self.ff.av_packet_free)(&mut self.pkt) }
    }
}

unsafe fn encode_aac<'a>(
    ff: &'a Ff,
    slices: &[muxer::AudioSlice<'_>],
    rate: u32,
    channels: u32,
    delay_samples: i64,
    ctx: *mut sys::AVCodecContext,
) -> Result<Vec<AudioPacket<'a>>, String> {
    let ch = channels.max(1) as usize;
    let frame_size = ((*ctx).frame_size.max(256)) as usize;
    let mut packets: Vec<AudioPacket> = Vec::new();
    let mut planes: Vec<Vec<f32>> = vec![Vec::new(); ch];
    let mut next_pts = delay_samples;

    let mut send = |count: usize,
                    planes: &mut Vec<Vec<f32>>,
                    packets: &mut Vec<AudioPacket<'a>>|
     -> Result<(), String> {
        let mut frame = (ff.av_frame_alloc)();
        if frame.is_null() {
            return Err("out of memory".into());
        }
        let result = (|| -> Result<(), String> {
            let f = &mut *frame;
            f.nb_samples = count as c_int;
            f.format = sys::AVSampleFormat_AV_SAMPLE_FMT_FLTP;
            f.sample_rate = rate as c_int;
            let rc = (ff.av_channel_layout_copy)(&mut f.ch_layout, &(*ctx).ch_layout);
            if rc < 0 {
                return Err(ff.err(rc));
            }
            let rc = (ff.av_frame_get_buffer)(frame, 0);
            if rc < 0 {
                return Err(ff.err(rc));
            }
            for (c, plane) in planes.iter().enumerate() {
                std::ptr::copy_nonoverlapping(plane.as_ptr(), f.data[c] as *mut f32, count);
            }
            f.pts = next_pts;
            next_pts += count as i64;
            let rc = (ff.avcodec_send_frame)(ctx, frame);
            if rc < 0 {
                return Err(ff.err(rc));
            }
            drain_audio(ff, ctx, packets)
        })();
        (ff.av_frame_free)(&mut frame);
        for plane in planes.iter_mut() {
            plane.drain(..count);
        }
        result
    };

    for slice in slices {
        let frames = slice.data.len() / (ch * 2);
        for n in 0..frames {
            for (c, plane) in planes.iter_mut().enumerate() {
                let o = (n * ch + c) * 2;
                plane.push(i16::from_le_bytes([slice.data[o], slice.data[o + 1]]) as f32 / 32768.0);
            }
            if planes[0].len() >= frame_size {
                send(frame_size, &mut planes, &mut packets)?;
            }
        }
    }
    let rest = planes[0].len();
    if rest > 0 {
        send(rest, &mut planes, &mut packets)?;
    }
    let rc = (ff.avcodec_send_frame)(ctx, null());
    if rc < 0 && rc != AVERROR_EOF {
        return Err(ff.err(rc));
    }
    drain_audio(ff, ctx, &mut packets)?;
    Ok(packets)
}

unsafe fn drain_audio<'a>(
    ff: &'a Ff,
    ctx: *mut sys::AVCodecContext,
    packets: &mut Vec<AudioPacket<'a>>,
) -> Result<(), String> {
    loop {
        let pkt = (ff.av_packet_alloc)();
        if pkt.is_null() {
            return Err("out of memory".into());
        }
        let rc = (ff.avcodec_receive_packet)(ctx, pkt);
        if rc < 0 {
            let mut p = pkt;
            (ff.av_packet_free)(&mut p);
            return if rc == AVERROR_EAGAIN || rc == AVERROR_EOF {
                Ok(())
            } else {
                Err(ff.err(rc))
            };
        }
        packets.push(AudioPacket { ff, pkt });
    }
}

#[allow(clippy::too_many_arguments)]
pub fn mux_until(
    path: &str,
    v: &[VideoSample],
    a: &[AudioSample],
    a_sample_rate: u32,
    a_channels: u32,
    fps: i32,
    w: u32,
    h: u32,
    seconds_back: i32,
    end_time: i64,
) -> i32 {
    match mux_inner(
        path,
        v,
        a,
        a_sample_rate,
        a_channels,
        fps,
        w,
        h,
        seconds_back,
        end_time,
    ) {
        Ok(()) => IR_OK,
        Err(code) if code == IR_NOT_ENOUGH_DATA => IR_NOT_ENOUGH_DATA,
        Err(code) => {
            let _ = std::fs::remove_file(path);
            code
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn mux_inner(
    path: &str,
    v: &[VideoSample],
    a: &[AudioSample],
    rate: u32,
    channels: u32,
    fps: i32,
    w: u32,
    h: u32,
    seconds_back: i32,
    end_time: i64,
) -> Result<(), i32> {
    if v.is_empty() {
        return Err(IR_NOT_ENOUGH_DATA);
    }
    let fail = |message: String| -> i32 {
        set_last_error(message);
        IR_FAIL
    };
    let ff = load().map_err(&fail)?;
    let fps = fps.max(1);
    let (w, h) = (w.max(2) as i32, h.max(2) as i32);

    let trim_start = if seconds_back > 0 {
        end_time - seconds_back as i64 * SECOND_100NS
    } else {
        i64::MIN
    };
    let mut start = 0usize;
    if seconds_back > 0 {
        let mut before = None;
        for (i, s) in v.iter().enumerate() {
            if s.pts100ns > trim_start {
                break;
            }
            if s.keyframe {
                before = Some(i);
            }
        }
        start = before.unwrap_or_else(|| v.iter().position(|s| s.keyframe).unwrap_or(0));
    }
    let samples = &v[start..];
    let first_pts = if samples.len() == 1 && trim_start != i64::MIN {
        samples[0].pts100ns.max(trim_start)
    } else {
        samples[0].pts100ns
    };
    let frame_dur = SECOND_100NS / fps as i64;
    let last_end = end_time.max(samples.last().map_or(0, |s| s.pts100ns) + frame_dur);
    if last_end - first_pts < SECOND_100NS / 2 {
        return Err(IR_NOT_ENOUGH_DATA);
    }
    let extradata = samples
        .iter()
        .find(|s| s.keyframe)
        .map(|s| extract_param_sets(s.data))
        .unwrap_or_default();
    if extradata.is_empty() {
        return Err(fail(
            "The buffered video has no HEVC VPS/SPS/PPS or AVC SPS/PPS parameter sets.".into(),
        ));
    }

    let slices = muxer::clip_audio_to_window(a, first_pts, last_end, rate, channels);
    let audio_delay = slices.first().map_or(0, |s| {
        ((s.pts100ns - first_pts).max(0) as i128 * rate as i128 / SECOND_100NS as i128) as i64
    });

    unsafe {
        let ffr: &Ff = &ff;

        let mut aac_ctx: *mut sys::AVCodecContext = null_mut();
        let mut audio_packets: Vec<AudioPacket> = Vec::new();
        let has_audio = !slices.is_empty();
        if has_audio {
            let name = CString::new("aac").map_err(|e| fail(e.to_string()))?;
            let codec = (ffr.avcodec_find_encoder_by_name)(name.as_ptr());
            if codec.is_null() {
                return Err(fail("This FFmpeg build has no AAC encoder.".into()));
            }
            aac_ctx = (ffr.avcodec_alloc_context3)(codec);
            if aac_ctx.is_null() {
                return Err(fail("out of memory".into()));
            }
            let c = &mut *aac_ctx;
            c.sample_fmt = sys::AVSampleFormat_AV_SAMPLE_FMT_FLTP;
            c.sample_rate = rate as c_int;
            (ffr.av_channel_layout_default)(&mut c.ch_layout, channels.max(1) as c_int);
            c.bit_rate = if channels >= 2 { 192_000 } else { 128_000 };
            c.time_base = sys::AVRational {
                num: 1,
                den: rate as c_int,
            };
            c.flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
            let rc = (ffr.avcodec_open2)(aac_ctx, codec, null_mut());
            if rc < 0 {
                (ffr.avcodec_free_context)(&mut aac_ctx);
                return Err(fail(format!("AAC encoder: {}", ffr.err(rc))));
            }
            match encode_aac(ffr, &slices, rate, channels, audio_delay, aac_ctx) {
                Ok(p) => audio_packets = p,
                Err(e) => {
                    (ffr.avcodec_free_context)(&mut aac_ctx);
                    return Err(fail(format!("AAC encoding failed: {e}")));
                }
            }
        }
        let result = write_file(
            ffr,
            path,
            samples,
            first_pts,
            last_end,
            &extradata,
            w,
            h,
            fps,
            &audio_packets,
            aac_ctx,
            rate,
            hd_codes(w, h),
        );
        drop(audio_packets);
        if !aac_ctx.is_null() {
            (ffr.avcodec_free_context)(&mut aac_ctx);
        }
        result.map_err(&fail)
    }
}

fn hd_codes(w: i32, h: i32) -> i32 {
    if w >= 1280 || h > 576 {
        1
    } else {
        6
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn write_file(
    ff: &Ff,
    path: &str,
    samples: &[VideoSample],
    first_pts: i64,
    last_end: i64,
    extradata: &[u8],
    w: i32,
    h: i32,
    fps: i32,
    audio: &[AudioPacket],
    aac_ctx: *mut sys::AVCodecContext,
    rate: u32,
    color_code: i32,
) -> Result<(), String> {
    let container = Container::from_path(path);
    let format = CString::new(container.muxer()).map_err(|e| e.to_string())?;
    let cpath = CString::new(path)
        .map_err(|_| "The output path contains an invalid character.".to_string())?;

    let mut ctx: *mut sys::AVFormatContext = null_mut();
    let rc = (ff.avformat_alloc_output_context2)(&mut ctx, null(), format.as_ptr(), cpath.as_ptr());
    if rc < 0 || ctx.is_null() {
        return Err(format!(
            "Could not create the {} muxer: {}",
            container.name(),
            ff.err(rc)
        ));
    }
    let mut out = Output {
        ff,
        ctx,
        io_open: false,
    };

    let vst = (ff.avformat_new_stream)(ctx, null());
    if vst.is_null() {
        return Err("out of memory".into());
    }
    {
        let st = &mut *vst;
        st.time_base = sys::AVRational {
            num: 1,
            den: 90_000,
        };
        st.avg_frame_rate = sys::AVRational { num: fps, den: 1 };
        let par = &mut *st.codecpar;
        par.codec_type = sys::AVMediaType_AVMEDIA_TYPE_VIDEO;
        par.codec_id = if parameter_sets_are_hevc(extradata) {
            sys::AVCodecID_AV_CODEC_ID_HEVC
        } else {
            sys::AVCodecID_AV_CODEC_ID_H264
        };
        par.format = sys::AVPixelFormat_AV_PIX_FMT_YUV420P;
        par.width = w;
        par.height = h;
        par.color_range = 2;
        par.color_space = color_code;
        par.color_primaries = color_code;
        par.color_trc = color_code;
        let buffer = (ff.av_malloc)(extradata.len() + AV_INPUT_BUFFER_PADDING_SIZE) as *mut u8;
        if buffer.is_null() {
            return Err("out of memory".into());
        }
        std::ptr::copy_nonoverlapping(extradata.as_ptr(), buffer, extradata.len());
        std::ptr::write_bytes(buffer.add(extradata.len()), 0, AV_INPUT_BUFFER_PADDING_SIZE);
        par.extradata = buffer;
        par.extradata_size = extradata.len() as c_int;
    }

    let mut ast: *mut sys::AVStream = null_mut();
    if !aac_ctx.is_null() {
        ast = (ff.avformat_new_stream)(ctx, null());
        if ast.is_null() {
            return Err("out of memory".into());
        }
        (*ast).time_base = sys::AVRational {
            num: 1,
            den: rate as c_int,
        };
        let rc = (ff.avcodec_parameters_from_context)((*ast).codecpar, aac_ctx);
        if rc < 0 {
            return Err(ff.err(rc));
        }
    }

    let rc = (ff.avio_open)(&mut (*ctx).pb, cpath.as_ptr(), AVIO_FLAG_WRITE);
    if rc < 0 {
        return Err(format!("Could not create '{path}': {}", ff.err(rc)));
    }
    out.io_open = true;

    let mut options = if container.supports_faststart() {
        ff.dict(&[("movflags", "faststart")])
    } else {
        null_mut()
    };
    let rc = (ff.avformat_write_header)(ctx, &mut options);
    (ff.av_dict_free)(&mut options);
    if rc < 0 {
        return Err(format!("Could not write the file header: {}", ff.err(rc)));
    }

    let vtb = (*vst).time_base;
    let atb = if ast.is_null() {
        TB_100NS
    } else {
        (*ast).time_base
    };
    let vpkt = (ff.av_packet_alloc)();
    if vpkt.is_null() {
        return Err("out of memory".into());
    }
    struct PktGuard<'a>(&'a Ff, *mut sys::AVPacket);
    impl Drop for PktGuard<'_> {
        fn drop(&mut self) {
            unsafe { (self.0.av_packet_free)(&mut self.1) }
        }
    }
    let _vguard = PktGuard(ff, vpkt);

    let n = samples.len();
    let mut video_ts: Vec<(i64, i64)> = Vec::with_capacity(n);
    let mut previous = i64::MIN;
    for (i, s) in samples.iter().enumerate() {
        let rel = (s.pts100ns.max(first_pts) - first_pts).max(0);
        let mut pts = (ff.av_rescale_q)(rel, TB_100NS, vtb);
        if pts <= previous {
            pts = previous + 1;
        }
        let next_rel = if i + 1 < n {
            (samples[i + 1].pts100ns.max(first_pts) - first_pts).max(0)
        } else {
            (last_end - first_pts).max(rel + 1)
        };
        let duration = ((ff.av_rescale_q)(next_rel, TB_100NS, vtb) - pts).max(1);
        previous = pts;
        video_ts.push((pts, duration));
    }

    let ctb = if aac_ctx.is_null() {
        TB_100NS
    } else {
        (*aac_ctx).time_base
    };
    let audio_time = |p: &AudioPacket| -> i64 {
        let pk = &*p.pkt;
        let t = if pk.dts != AV_NOPTS_VALUE {
            pk.dts
        } else {
            pk.pts
        };
        (ff.av_rescale_q)(t, ctb, TB_100NS)
    };
    let (mut vi, mut ai) = (0usize, 0usize);
    while vi < n || ai < audio.len() {
        let take_video = if vi >= n {
            false
        } else if ai >= audio.len() {
            true
        } else {
            let v_time = (samples[vi].pts100ns.max(first_pts) - first_pts).max(0);
            v_time <= audio_time(&audio[ai])
        };
        if take_video {
            let s = &samples[vi];
            let (pts, duration) = video_ts[vi];
            let p = &mut *vpkt;
            p.data = s.data.as_ptr() as *mut u8;
            p.size = s.data.len() as c_int;
            p.stream_index = (*vst).index;
            p.pts = pts;
            p.dts = pts;
            p.duration = duration;
            p.flags = if s.keyframe { AV_PKT_FLAG_KEY } else { 0 };
            let rc = (ff.av_interleaved_write_frame)(ctx, vpkt);
            p.data = null_mut();
            p.size = 0;
            if rc < 0 {
                return Err(format!("Could not write video: {}", ff.err(rc)));
            }
            vi += 1;
            if vi == n || vi % 4 == 0 {
                crate::SAVE_PROGRESS.store(
                    (vi * 100 / n.max(1)) as i32,
                    std::sync::atomic::Ordering::Relaxed,
                );
            }
        } else {
            let pk = audio[ai].pkt;
            {
                let p = &mut *pk;
                p.stream_index = (*ast).index;
                if p.pts != AV_NOPTS_VALUE {
                    p.pts = (ff.av_rescale_q)(p.pts, ctb, atb);
                }
                if p.dts != AV_NOPTS_VALUE {
                    p.dts = (ff.av_rescale_q)(p.dts, ctb, atb);
                }
                p.duration = (ff.av_rescale_q)(p.duration, ctb, atb);
            }
            let rc = (ff.av_interleaved_write_frame)(ctx, pk);
            if rc < 0 {
                return Err(format!("Could not write audio: {}", ff.err(rc)));
            }
            ai += 1;
        }
    }
    let rc = (ff.av_write_trailer)(ctx);
    if rc < 0 {
        return Err(format!("Could not finish the file: {}", ff.err(rc)));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameter_sets_are_detected_and_extracted() {
        let keyframe = [
            0, 0, 0, 1, 0x40, 1, 2, 3, 0, 0, 0, 1, 0x42, 4, 5, 0, 0, 1, 0x44, 6, 7, 0, 0, 1, 0x26,
            9, 9, 9,
        ];
        assert!(has_param_sets(&keyframe));
        assert_eq!(
            extract_param_sets(&keyframe),
            vec![0, 0, 0, 1, 0x40, 1, 2, 3, 0, 0, 0, 1, 0x42, 4, 5, 0, 0, 0, 1, 0x44, 6, 7,]
        );
        assert!(parameter_sets_are_hevc(&extract_param_sets(&keyframe)));

        let avc_keyframe = [
            0, 0, 0, 1, 0x67, 1, 2, 3, 0, 0, 0, 1, 0x68, 4, 5, 0, 0, 1, 0x65, 9,
        ];
        assert!(has_param_sets(&avc_keyframe));
        let avc_sets = extract_param_sets(&avc_keyframe);
        assert_eq!(
            avc_sets,
            vec![0, 0, 0, 1, 0x67, 1, 2, 3, 0, 0, 0, 1, 0x68, 4, 5]
        );
        assert!(!parameter_sets_are_hevc(&avc_sets));
        assert!(packet_has_picture(&avc_keyframe, false));
        assert!(!packet_has_picture(&avc_sets, false));
        assert!(packet_has_picture(&keyframe, true));

        let p_frame = [0, 0, 0, 1, 0x02, 1, 2, 3];
        assert!(!has_param_sets(&p_frame));
        assert!(extract_param_sets(&p_frame).is_empty());
        assert!(extract_param_sets(&[]).is_empty());
    }

    #[test]
    fn nal_units_exclude_start_codes() {
        let data = [0, 0, 0, 1, 0x67, 1, 0, 0, 1, 0x68, 2];
        let units: Vec<&[u8]> = nal_units(&data).map(|(s, e)| &data[s..e]).collect();
        assert_eq!(units, vec![&[0x67u8, 1][..], &[0x68u8, 2][..]]);
    }

    #[test]
    fn packed_preferences_decode() {
        let o = VideoOptions::from_pref(4 | (1 << 8) | (2 << 10), 1921, 1081, 0, 5000);
        assert_eq!(
            (o.enc, o.rate, o.preset),
            (Enc::X264, Rate::Vbr, Preset::Quality)
        );
        assert_eq!((o.w, o.h, o.fps), (1920, 1080, 1));
        let d = VideoOptions::from_pref(0, 1280, 720, 60, 9_000_000);
        assert_eq!(
            (d.enc, d.rate, d.preset),
            (Enc::Auto, Rate::Cbr, Preset::Speed)
        );
        let b = VideoOptions::from_pref(1 << 10, 1280, 720, 60, 9_000_000);
        assert_eq!(b.preset, Preset::Balanced);
        assert_eq!(
            candidates(Enc::Auto, Codec::Auto),
            &[
                "hevc_nvenc",
                "hevc_amf",
                "hevc_qsv",
                "h264_nvenc",
                "h264_amf",
                "h264_qsv",
                "libx264",
            ]
        );
        assert_eq!(candidates(Enc::Nvenc, Codec::Auto), &["hevc_nvenc"]);
        assert_eq!(candidates(Enc::Amf, Codec::Auto), &["hevc_amf"]);
        assert_eq!(candidates(Enc::Qsv, Codec::Auto), &["hevc_qsv"]);
        assert_eq!(candidates(Enc::X264, Codec::Auto), &["libx264"]);
        assert_eq!(candidates(Enc::Mf, Codec::Auto), &["hevc_mf"]);
        assert_eq!(
            candidates(Enc::Auto, Codec::H264),
            &["h264_nvenc", "h264_amf", "h264_qsv", "libx264"]
        );
    }

    #[test]
    fn each_encoder_gets_the_option_names_it_accepts() {
        let o = VideoOptions {
            enc: Enc::Amf,
            codec: Codec::Hevc,
            rate: Rate::Cbr,
            preset: Preset::Quality,
            w: 1920,
            h: 1080,
            fps: 60,
            bps: 9_000_000,
        };

        let amf = codec_options("hevc_amf", &o);
        assert!(amf.contains(&("forced_idr", "1")));
        assert!(amf.contains(&("usage", "high_quality")));
        assert!(amf.contains(&("quality", "quality")));
        assert!(amf.contains(&("rc", "cbr")));
        assert!(!amf.iter().any(|(k, _)| *k == "header_insertion_mode"));

        let nvenc = codec_options("hevc_nvenc", &o);
        assert!(nvenc.contains(&("forced-idr", "1")));
        assert!(nvenc.contains(&("preset", "p6")));
        assert!(nvenc.contains(&("multipass", "qres")));

        assert_eq!(nvenc_preset(Preset::Speed), "p2");
        assert_eq!(nvenc_preset(Preset::Balanced), "p4");
        assert_eq!(nvenc_preset(Preset::Quality), "p6");
        assert_eq!(x264_preset(Preset::Speed), "veryfast");
        assert_eq!(x264_preset(Preset::Balanced), "veryfast");
        assert_eq!(x264_preset(Preset::Quality), "veryfast");
        assert_eq!(qsv_preset(Preset::Speed), "veryfast");
        assert_eq!(qsv_preset(Preset::Balanced), "medium");
        assert_eq!(qsv_preset(Preset::Quality), "slow");
        assert_eq!(amf_preset(Preset::Speed), "speed");
        assert_eq!(amf_preset(Preset::Balanced), "balanced");
        assert_eq!(amf_preset(Preset::Quality), "quality");

        let mf = codec_options("hevc_mf", &o);
        assert!(mf.contains(&("quality", "85")));
        assert!(mf.contains(&("scenario", "archive")));

        let vbr = VideoOptions {
            rate: Rate::Vbr,
            ..o
        };
        assert!(codec_options("hevc_amf", &vbr).contains(&("rc", "vbr_peak")));
        assert!(codec_options("hevc_nvenc", &vbr).contains(&("rc-lookahead", "20")));
        assert!(codec_options("libx265", &vbr)
            .iter()
            .all(|(k, _)| *k != "nal-hrd"));
    }

    #[test]
    fn container_is_chosen_from_the_extension() {
        assert_eq!(Container::from_path("replay.mp4"), Container::Mp4);
        assert_eq!(Container::from_path("replay.MKV"), Container::Mkv);
        assert_eq!(Container::from_path(r"C:\clips\replay.mov"), Container::Mov);
        assert_eq!(Container::from_path("replay.m2ts"), Container::Ts);
        assert_eq!(Container::from_path("replay.flv"), Container::Flv);
        assert_eq!(Container::from_path("no-extension"), Container::Mp4);

        assert_eq!(Container::Mkv.muxer(), "matroska");
        assert_eq!(Container::Ts.muxer(), "mpegts");
        assert_eq!(Container::Flv.extension(), "flv");
        assert!(Container::Mp4.supports_faststart());
        assert!(Container::Mov.supports_faststart());
        assert!(!Container::Mkv.supports_faststart());
        assert!(!Container::Ts.supports_faststart());
    }

    #[test]
    fn opens_a_real_encoder_when_the_bundled_ffmpeg_is_present() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../encoder");
        let Ok(ff) = load_from(&dir) else {
            return;
        };
        let ff = Arc::new(ff);
        let o = VideoOptions {
            enc: Enc::X264,
            codec: Codec::H264,
            rate: Rate::Cbr,
            preset: Preset::Balanced,
            w: 640,
            h: 360,
            fps: 30,
            bps: 2_000_000,
        };
        let video = unsafe { FfVideo::try_open(&ff, "libx264", &o) }
            .expect("libx264 should open from the bundled FFmpeg build");
        assert_eq!(video.kind_code(), 29);
    }

    #[test]
    fn muxes_a_real_clip_into_every_container() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../encoder");
        let Ok(ff) = load_from(&dir) else {
            return;
        };
        std::env::set_var("IR_FFMPEG_DIR", &dir);
        let ff = Arc::new(ff);
        let (w, h, fps) = (320usize, 180usize, 30i32);
        let o = VideoOptions {
            enc: Enc::X264,
            codec: Codec::H264,
            rate: Rate::Cbr,
            preset: Preset::Speed,
            w: w as i32,
            h: h as i32,
            fps,
            bps: 1_000_000,
        };
        let mut enc = unsafe { FfVideo::try_open(&ff, "libx264", &o) }.expect("libx264");
        let frame_dur = SECOND_100NS / fps as i64;

        let y = vec![16u8; w * h];
        let uv = vec![128u8; w * h / 2];
        let mut frames = Vec::new();
        for i in 0..(fps as usize * 2) {
            let idr = i % fps as usize == 0;
            enc.encode_nv12(&y, w, &uv, w, i as i64 * frame_dur, idr, &mut frames)
                .expect("encode");
        }

        let tmp = std::env::temp_dir().join("instantreplay-ffmpeg-mux-test");
        std::fs::create_dir_all(&tmp).expect("temp dir");
        let end = frames.last().expect("frames").pts100ns + frame_dur;

        for container in Container::ALL {
            let samples: Vec<VideoSample> = frames
                .iter()
                .map(|f| VideoSample {
                    data: &f.data,
                    keyframe: f.keyframe,
                    pts100ns: f.pts100ns,
                })
                .collect();
            let path = tmp.join(format!("clip.{}", container.extension()));
            let path = path.to_str().expect("utf-8 path");

            let rc = mux_until(
                path,
                &samples,
                &[],
                48_000,
                2,
                fps,
                w as u32,
                h as u32,
                0,
                end,
            );
            assert_eq!(
                rc,
                IR_OK,
                "{} muxing failed: {}",
                container.name(),
                last_error()
            );
            let len = std::fs::metadata(path).expect("output file").len();
            assert!(
                len > 1_000,
                "{} produced only {len} bytes",
                container.name()
            );
            let _ = std::fs::remove_file(path);
        }

        let mut avc = unsafe { FfVideo::try_open(&ff, "libx264", &o) }.expect("libx264");
        let mut avc_frames = Vec::new();
        for i in 0..(fps as usize * 2) {
            avc.encode_nv12(&y, w, &uv, w, i as i64 * frame_dur, i == 0, &mut avc_frames)
                .expect("encode AVC fallback");
        }
        let avc_samples: Vec<VideoSample> = avc_frames
            .iter()
            .map(|f| VideoSample {
                data: &f.data,
                keyframe: f.keyframe,
                pts100ns: f.pts100ns,
            })
            .collect();
        let avc_path = tmp.join("avc-fallback.mp4");
        let avc_path_str = avc_path.to_str().expect("utf-8 path");
        let avc_end = avc_frames.last().expect("AVC frames").pts100ns + frame_dur;
        assert_eq!(
            mux_until(
                avc_path_str,
                &avc_samples,
                &[],
                48_000,
                2,
                fps,
                w as u32,
                h as u32,
                0,
                avc_end
            ),
            IR_OK,
            "AVC fallback MP4 muxing failed: {}",
            last_error()
        );
        assert!(std::fs::metadata(&avc_path).expect("AVC output").len() > 1_000);
        let _ = std::fs::remove_file(avc_path);
        let _ = std::fs::remove_dir(&tmp);
    }
}
