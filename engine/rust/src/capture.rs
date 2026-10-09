use std::sync::atomic::Ordering;
use std::sync::Arc;

use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM};
use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::d3d11::StagingTexture;
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};

use crate::encoder::GpuInput;
use crate::{qpc100ns, RawFrame, SharedState};

pub type CapError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Clone, Copy, Debug)]
pub struct MonInfo {
    pub monitor: Monitor,
    pub w: i32,
    pub h: i32,
    pub primary: bool,
}

pub fn enumerate_monitors() -> Vec<MonInfo> {
    let Ok(list) = Monitor::enumerate() else {
        return Vec::new();
    };
    let primary = Monitor::primary().ok().map(|m| m.as_raw_hmonitor());
    let mut out: Vec<MonInfo> = list
        .into_iter()
        .filter_map(|m| {
            let w = m.width().unwrap_or(0) as i32;
            let h = m.height().unwrap_or(0) as i32;
            if w <= 0 || h <= 0 {
                return None;
            }
            Some(MonInfo {
                primary: Some(m.as_raw_hmonitor()) == primary,
                monitor: m,
                w,
                h,
            })
        })
        .collect();
    out.sort_by_key(|m| if m.primary { 0 } else { 1 });
    out
}

pub fn find_monitor(index: i32) -> Option<MonInfo> {
    let mons = enumerate_monitors();
    if mons.is_empty() {
        return None;
    }
    if index < 0 {
        mons.iter()
            .find(|m| m.primary)
            .copied()
            .or_else(|| mons.first().copied())
    } else {
        mons.get(index as usize).copied()
    }
}

struct Handler {
    shared: Arc<SharedState>,
    last_arrival: Option<std::time::Instant>,
    last_sum: Option<u64>,
    staging: Option<StagingTexture>,
    staging_key: Option<(u32, u32, DXGI_FORMAT)>,
    scratch: Vec<u8>,
    gpu: Option<Arc<GpuInput>>,
    gpu_published: bool,
}

pub(crate) fn sampled_sum(bgra: &[u8], w: u32, h: u32) -> u64 {
    let mut acc: u64 = (w as u64) << 32 | (h as u64);
    let mut i = 0usize;
    while i < bgra.len() {
        acc = acc.wrapping_add(bgra[i] as u64);
        i += 61;
    }
    acc
}

pub const GAP_BUCKETS: usize = 8;

pub fn gap_bucket(ms: f64) -> usize {
    match ms {
        m if m < 5.0 => 0,
        m if m < 10.0 => 1,
        m if m < 14.0 => 2,
        m if m < 18.0 => 3,
        m if m < 25.0 => 4,
        m if m < 35.0 => 5,
        m if m < 50.0 => 6,
        _ => 7,
    }
}

fn skip_readback() -> bool {
    static ONCE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ONCE.get_or_init(|| std::env::var("IR_SKIP_READBACK").is_ok_and(|v| v != "0"))
}

impl GraphicsCaptureApiHandler for Handler {
    type Flags = Arc<SharedState>;
    type Error = CapError;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Handler {
            shared: ctx.flags,
            last_arrival: None,
            last_sum: None,
            staging: None,
            staging_key: None,
            scratch: Vec::new(),
            gpu: None,
            gpu_published: false,
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        _control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        let t_arrival = std::time::Instant::now();
        self.shared.n_wgc.fetch_add(1, Ordering::Relaxed);
        if let Some(prev) = self.last_arrival.replace(t_arrival) {
            let gap = t_arrival.duration_since(prev);
            self.shared
                .t_wgc_gap_ns
                .fetch_add(gap.as_nanos() as i64, Ordering::Relaxed);
            let ms = gap.as_secs_f64() * 1000.0;
            self.shared.n_gap[GAP_BUCKETS.min(gap_bucket(ms))].fetch_add(1, Ordering::Relaxed);
            self.shared.n_gap_max_ns.fetch_max(gap.as_nanos() as i64, Ordering::Relaxed);
        }

        let w = frame.width();
        let h = frame.height();
        if w < 2 || h < 2 {
            return Ok(());
        }

        let pts = qpc100ns() - self.shared.start_qpc.load(Ordering::Relaxed);

        if skip_readback() {
            self.shared.n_capture.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }

        if self.shared.gpu_mode.load(Ordering::Relaxed) == 1 {
            let stale = self.gpu.as_ref().map_or(true, |g| g.w != w || g.h != h);
            if stale {
                match GpuInput::create(frame.d3d_device(), frame.device_context(), w, h) {
                    Ok(g) => {
                        self.gpu = Some(Arc::new(g));
                        self.gpu_published = false;
                    }
                    Err(_) => {
                        self.shared.gpu_mode.store(0, Ordering::Relaxed);
                        self.gpu = None;
                    }
                }
            }
            if self.shared.gpu_mode.load(Ordering::Relaxed) == 1 {
                if let Some(g) = &self.gpu {
                    let t0 = std::time::Instant::now();
                    g.submit(frame.as_raw_texture());
                    self.shared
                        .t_capture_ns
                        .fetch_add(t0.elapsed().as_nanos() as i64, Ordering::Relaxed);
                    if !self.gpu_published {
                        *self.shared.gpu.lock().unwrap_or_else(|e| e.into_inner()) = Some(g.clone());
                        self.shared.gpu_gen.fetch_add(1, Ordering::Relaxed);
                        self.gpu_published = true;
                    }
                    self.shared.gpu_frames.fetch_add(1, Ordering::Relaxed);
                    self.shared.n_capture.fetch_add(1, Ordering::Relaxed);
                    self.shared.raw_wait.notify_all();
                    return Ok(());
                }
            }
        }

        let t0 = std::time::Instant::now();
        let desc_key = (w, h, DXGI_FORMAT_B8G8R8A8_UNORM);
        if self.staging_key != Some(desc_key) {
            self.staging = Some(StagingTexture::new(frame.d3d_device(), w, h, desc_key.2)?);
            self.staging_key = Some(desc_key);
            self.shared.n_staging_alloc.fetch_add(1, Ordering::Relaxed);
        }
        let staging = self.staging.as_mut().expect("staging just set");
        let fb = frame.buffer_into(staging)?;
        let t1 = std::time::Instant::now();
        let src = fb.as_nopadding_buffer(&mut self.scratch);
        let mut bgra = self.shared.take_bgra();
        bgra.clear();
        bgra.extend_from_slice(src);
        let t2 = std::time::Instant::now();
        self.shared
            .t_buf_ns
            .fetch_add(t1.duration_since(t0).as_nanos() as i64, Ordering::Relaxed);
        self.shared
            .t_copy_ns
            .fetch_add(t2.duration_since(t1).as_nanos() as i64, Ordering::Relaxed);
        self.shared
            .t_capture_ns
            .fetch_add(t0.elapsed().as_nanos() as i64, Ordering::Relaxed);
        self.shared.n_capture.fetch_add(1, Ordering::Relaxed);
        if bgra.len() < (w as usize) * (h as usize) * 4 {
            return Ok(());
        }
        let sum = sampled_sum(&bgra, w, h);
        let is_dup = match self.last_sum {
            Some(prev) if prev == sum => {
                self.shared.n_frame_same.fetch_add(1, Ordering::Relaxed);
                true
            }
            _ => {
                self.shared.n_frame_diff.fetch_add(1, Ordering::Relaxed);
                false
            }
        };
        if !is_dup {
            self.last_sum = Some(sum);
        } else {
            self.shared.recycle_bgra(bgra);
            return Ok(());
        }
        self.shared
            .push_frame(RawFrame { bgra, w, h, pts100ns: pts });
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureMethod {
    Wgc = 0,
    Dxgi = 1,
}

impl CaptureMethod {
    pub fn from_i32(v: i32) -> Self {
        match v {
            1 => CaptureMethod::Dxgi,
            _ => CaptureMethod::Wgc,
        }
    }

    pub fn as_i32(self) -> i32 {
        self as i32
    }

    pub fn label(self) -> &'static str {
        match self {
            CaptureMethod::Wgc => "WGC",
            CaptureMethod::Dxgi => "DXGI duplication",
        }
    }
}

enum SessionImpl {
    Wgc(CaptureControl<Handler, CapError>),
    Dxgi(crate::capture_dxgi::DxgiSession),
}

pub struct CaptureSession {
    inner: Option<SessionImpl>,
}

impl CaptureSession {
    pub fn start(
        shared: Arc<SharedState>,
        index: i32,
        method: CaptureMethod,
    ) -> Result<Self, CapError> {
        match method {
            CaptureMethod::Dxgi => {
                let mon = find_monitor(index).ok_or("no monitor found")?;
                shared.capture_method.store(method.as_i32(), Ordering::Relaxed);
                let session = crate::capture_dxgi::DxgiSession::start(shared, mon.monitor)?;
                Ok(CaptureSession { inner: Some(SessionImpl::Dxgi(session)) })
            }
            CaptureMethod::Wgc => Self::start_wgc(shared, index),
        }
    }

    fn start_wgc(shared: Arc<SharedState>, index: i32) -> Result<Self, CapError> {
        let mon = find_monitor(index).ok_or("no monitor found")?;
        shared.capture_method.store(CaptureMethod::Wgc.as_i32(), Ordering::Relaxed);
        let settings = Settings::new(
            mon.monitor,
            CursorCaptureSettings::WithCursor,
            DrawBorderSettings::WithoutBorder,
            SecondaryWindowSettings::Default,
            MinimumUpdateIntervalSettings::Custom(std::time::Duration::from_millis(1)),
            DirtyRegionSettings::Default,
            ColorFormat::Bgra8,
            shared,
        );
        let control = <Handler as GraphicsCaptureApiHandler>::start_free_threaded(settings)
            .map_err(|e| -> CapError { Box::new(e) })?;
        Ok(CaptureSession {
            inner: Some(SessionImpl::Wgc(control)),
        })
    }

    pub fn stop(&mut self) {
        match self.inner.take() {
            Some(SessionImpl::Wgc(c)) => {
                let _ = c.stop();
            }
            Some(SessionImpl::Dxgi(mut d)) => d.stop(),
            None => {}
        }
    }
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        self.stop();
    }
}
