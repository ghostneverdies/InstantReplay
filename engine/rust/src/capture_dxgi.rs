use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM};
use windows::Win32::Graphics::Dxgi::{
    IDXGIOutputDuplication, DXGI_OUTDUPL_FRAME_INFO, DXGI_OUTDUPL_POINTER_SHAPE_INFO,
    DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR, DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR,
    DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME,
};
use windows_capture::dxgi_duplication_api::{
    DxgiDuplicationApi, DxgiDuplicationFormat, Error as DxgiError,
};
use windows_capture::monitor::Monitor;

use crate::capture::{gap_bucket, sampled_sum, CapError, GAP_BUCKETS};
use crate::{qpc100ns, RawFrame, SharedState};

const WANTED_FORMATS: [DxgiDuplicationFormat; 1] = [DxgiDuplicationFormat::Bgra8];

const ACQUIRE_TIMEOUT_MS: u32 = 10;

const RETRY_DELAY: Duration = Duration::from_millis(400);

enum Failure {
    Create(String),
    AccessLost,
    Fatal(String),
}

pub struct DxgiSession {
    stop: Arc<std::sync::atomic::AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl DxgiSession {
    pub fn start(shared: Arc<SharedState>, monitor: Monitor) -> Result<Self, CapError> {
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (tx, rx) = mpsc::channel::<Result<(), String>>();
        let stop_c = stop.clone();
        let shared_c = shared.clone();
        let handle = std::thread::Builder::new()
            .name("ir-dxgi-capture".into())
            .spawn(move || run(shared_c, monitor, stop_c, tx))?;
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(DxgiSession { stop, handle: Some(handle) }),
            Ok(Err(msg)) => {
                let _ = handle.join();
                Err(msg.into())
            }
            Err(_) => {
                let _ = handle.join();
                Err("DXGI capture thread did not report readiness".into())
            }
        }
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for DxgiSession {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run(
    shared: Arc<SharedState>,
    monitor: Monitor,
    stop: Arc<std::sync::atomic::AtomicBool>,
    ready: mpsc::Sender<Result<(), String>>,
) {
    let mut reported = false;
    loop {
        if stop.load(Ordering::Relaxed) {
            let _ = ready.send(Ok(()));
            return;
        }
        match capture_once(&shared, monitor, &stop, &mut reported, &ready) {
            Ok(()) => return,
            Err(Failure::AccessLost) => {
                shared.n_dxgi_access_lost.fetch_add(1, Ordering::Relaxed);
                if !reported {
                    let _ = ready.send(Err("DXGI duplication access was lost during startup".into()));
                    return;
                }
                std::thread::sleep(RETRY_DELAY);
            }
            Err(Failure::Create(msg)) | Err(Failure::Fatal(msg)) => {
                if !reported {
                    let _ = ready.send(Err(format!("DXGI desktop duplication unavailable: {msg}")));
                    return;
                }
                shared.n_dxgi_fatal.fetch_add(1, Ordering::Relaxed);
                return;
            }
        }
        reported = true;
    }
}

fn capture_once(
    shared: &Arc<SharedState>,
    monitor: Monitor,
    stop: &std::sync::atomic::AtomicBool,
    reported: &mut bool,
    ready: &mpsc::Sender<Result<(), String>>,
) -> Result<(), Failure> {
    let mut api = match DxgiDuplicationApi::new_options(monitor, &WANTED_FORMATS) {
        Ok(a) => a,
        Err(e) => {
            let msg = describe_dxgi(&e);
            if !*reported {
                let _ = ready.send(Err(msg.clone()));
            }
            return Err(Failure::Create(msg));
        }
    };

    if !*reported {
        *reported = true;
        let _ = ready.send(Ok(()));
        let (num, den) = api.refresh_rate();
        eprintln!(
            "[dxgi] duplication up: {}x{} @ {}/{}Hz bgra8",
            api.width(),
            api.height(),
            num,
            if den == 0 { 1 } else { den }
        );
    }

    let mut staging: Option<windows_capture::d3d11::StagingTexture> = None;
    let mut scratch: Vec<u8> = Vec::new();
    let mut last_arrival: Option<Instant> = None;
    let mut last_sum: Option<u64> = None;

    let origin = output_origin(monitor);

    let mut pointer = PointerShape::default();
    let mut last_ptr: Option<(bool, (i32, i32), u64)> = None;

    loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }

        let mut frame = match api.acquire_next_frame(ACQUIRE_TIMEOUT_MS) {
            Ok(f) => f,
            Err(DxgiError::Timeout) => {
                shared.n_dxgi_timeout.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            Err(DxgiError::AccessLost) => return Err(Failure::AccessLost),
            Err(e) => {
                let msg = describe_dxgi(&e);
                shared.n_dxgi_fatal.fetch_add(1, Ordering::Relaxed);
                eprintln!("[dxgi] AcquireNextFrame failed: {msg}");
                return Err(Failure::Fatal(msg));
            }
        };

        let t_arrival = Instant::now();
        shared.n_wgc.fetch_add(1, Ordering::Relaxed);
        if let Some(prev) = last_arrival.replace(t_arrival) {
            let gap = t_arrival.duration_since(prev);
            shared
                .t_wgc_gap_ns
                .fetch_add(gap.as_nanos() as i64, Ordering::Relaxed);
            let ms = gap.as_secs_f64() * 1000.0;
            shared.n_gap[GAP_BUCKETS.min(gap_bucket(ms))].fetch_add(1, Ordering::Relaxed);
            shared.n_gap_max_ns.fetch_max(gap.as_nanos() as i64, Ordering::Relaxed);
        }

        let w = frame.width();
        let h = frame.height();
        if w < 2 || h < 2 {
            continue;
        }

        let key: (u32, u32, DXGI_FORMAT) = (w, h, DXGI_FORMAT_B8G8R8A8_UNORM);
        let current = staging
            .as_ref()
            .map(|s| {
                let d = s.desc();
                (d.Width, d.Height, d.Format)
            });
        if current != Some(key) {
            match windows_capture::d3d11::StagingTexture::new(frame.device(), w, h, key.2) {
                Ok(t) => {
                    staging = Some(t);
                    shared.n_staging_alloc.fetch_add(1, Ordering::Relaxed);
                }
                Err(e) => {
                    let msg = describe(&e);
                    eprintln!("[dxgi] staging texture creation failed: {msg}");
                    return Err(Failure::Fatal(msg));
                }
            }
        }

        let mut bgra = shared.take_bgra();
        bgra.clear();
        let t0 = Instant::now();
        let t1;
        let t2;
        {
            let fb = match frame.buffer_with(staging.as_mut().expect("staging set above")) {
                Ok(fb) => fb,
                Err(e) => {
                    let msg = describe_dxgi(&e);
                    eprintln!("[dxgi] buffer_with failed: {msg}");
                    shared.recycle_bgra(bgra);
                    return Err(Failure::Fatal(msg));
                }
            };
            t1 = Instant::now();
            let src = fb.as_nopadding_buffer(&mut scratch);
            bgra.extend_from_slice(src);
            t2 = Instant::now();
        }

        shared
            .t_buf_ns
            .fetch_add(t1.duration_since(t0).as_nanos() as i64, Ordering::Relaxed);
        shared
            .t_copy_ns
            .fetch_add(t2.duration_since(t1).as_nanos() as i64, Ordering::Relaxed);
        shared
            .t_capture_ns
            .fetch_add(t0.elapsed().as_nanos() as i64, Ordering::Relaxed);
        shared.n_capture.fetch_add(1, Ordering::Relaxed);

        if bgra.len() < (w as usize) * (h as usize) * 4 {
            continue;
        }

        pointer.draw(frame.duplication(), frame.frame_info(), &mut bgra, w, h, origin);

        let sum = sampled_sum(&bgra, w, h);
        let ptr_sig = pointer.signature();
        let is_dup = match last_sum {
            Some(prev) if prev == sum && last_ptr == Some(ptr_sig) => {
                shared.n_frame_same.fetch_add(1, Ordering::Relaxed);
                true
            }
            _ => {
                shared.n_frame_diff.fetch_add(1, Ordering::Relaxed);
                false
            }
        };
        if !is_dup {
            last_sum = Some(sum);
            last_ptr = Some(ptr_sig);
        } else {
            shared.recycle_bgra(bgra);
            continue;
        }

        let pts = qpc100ns() - shared.start_qpc.load(Ordering::Relaxed);
        shared.push_frame(RawFrame { bgra, w, h, pts100ns: pts });
    }
}

fn describe<E: std::fmt::Display>(e: &E) -> String {
    e.to_string()
}

fn describe_dxgi(e: &DxgiError) -> String {
    let base = e.to_string();
    if let DxgiError::WindowsError(w) = e {
        return format!("{base} (HRESULT 0x{:08X})", w.code().0 as u32);
    }
    base
}

pub fn probe_available() -> bool {
    let Some(mon) = crate::capture::find_monitor(-1) else {
        return false;
    };
    DxgiDuplicationApi::new_options(mon.monitor, &WANTED_FORMATS).is_ok()
}

fn output_origin(monitor: Monitor) -> (i32, i32) {
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, HMONITOR, MONITORINFO};
    unsafe {
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(HMONITOR(monitor.as_raw_hmonitor()), &mut mi).as_bool() {
            (mi.rcMonitor.left, mi.rcMonitor.top)
        } else {
            (0, 0)
        }
    }
}

#[derive(Default)]
struct PointerShape {
    buf: Vec<u8>,
    len: usize,
    shape: Option<DXGI_OUTDUPL_POINTER_SHAPE_INFO>,
    offered: u32,
    shape_gen: u64,
    at: (i32, i32),
    visible: bool,
    failed: bool,
}

impl PointerShape {
    fn draw(
        &mut self,
        duplication: &IDXGIOutputDuplication,
        frame_info: &DXGI_OUTDUPL_FRAME_INFO,
        bgra: &mut [u8],
        w: u32,
        h: u32,
        origin: (i32, i32),
    ) {
        self.advance(
            frame_info.PointerPosition.Visible.as_bool(),
            frame_info.LastMouseUpdateTime != 0,
            (
                frame_info.PointerPosition.Position.x - origin.0,
                frame_info.PointerPosition.Position.y - origin.1,
            ),
        );

        let offered = frame_info.PointerShapeBufferSize;
        if offered != 0 {
            self.offered = offered;
            if self.buf.len() < offered as usize {
                self.buf.resize(offered as usize, 0);
            }
            let mut needed = 0u32;
            let mut info = DXGI_OUTDUPL_POINTER_SHAPE_INFO::default();
            let fetched = unsafe {
                duplication.GetFramePointerShape(
                    self.buf.len() as u32,
                    self.buf.as_mut_ptr() as *mut std::ffi::c_void,
                    &mut needed,
                    &mut info,
                )
            };
            match fetched {
                Ok(()) => {
                    self.len = needed as usize;
                    self.shape = Some(info);
                    self.failed = false;
                    self.shape_gen += 1;
                    if std::env::var_os("IR_DEBUG_POINTER").is_some() {
                        eprintln!(
                            "[ptr] fetch {}x{} pitch={} hot=({},{}) ty={} len={} at=({},{})",
                            info.Width, info.Height, info.Pitch,
                            info.HotSpot.x, info.HotSpot.y, info.Type, needed,
                            self.at.0, self.at.1
                        );
                    }
                }
                Err(e) => {
                    self.offered = 0;
                    self.failed = self.shape.is_none();
                    eprintln!("[dxgi] GetFramePointerShape failed: {e:?}");
                    return;
                }
            }
        }

        if !self.visible || self.failed {
            return;
        }

        let Some(shape) = self.shape else { return };

        match shape.Type {
            t if t == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR.0 as u32 => {
                self.blend_color(bgra, w, h, self.len);
            }
            t if t == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR.0 as u32 => {
                self.blend_masked_color(bgra, w, h, self.len);
            }
            t if t == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME.0 as u32 => {
                self.blend_monochrome(bgra, w, h, self.len);
            }
            other => {
                self.failed = true;
                eprintln!("[dxgi] unsupported pointer shape type {other}");
            }
        }
    }

    fn blend_color(&mut self, bgra: &mut [u8], w: u32, h: u32, len: usize) {
        let Some(shape) = self.shape else { return };
        let (pw, ph, pitch) = (shape.Width as usize, shape.Height as usize, shape.Pitch as usize);
        for py in 0..ph {
            let Some(dy) = self.row_of(py, h) else { continue };
            let Some(row) = self.row(py, pw * 4, pitch, len) else { continue };
            for px in 0..pw {
                let a = row[px * 4 + 3];
                if a == 0 {
                    continue;
                }
                let Some(di) = self.offset(dy, px, w, bgra.len()) else { continue };
                let s = &row[px * 4..px * 4 + 3];
                if a == 255 {
                    bgra[di..di + 3].copy_from_slice(s);
                } else {
                    let ia = 255 - a as u32;
                    for c in 0..3 {
                        let sc = s[c] as u32 * a as u32;
                        bgra[di + c] = ((sc + bgra[di + c] as u32 * ia) / 255) as u8;
                    }
                }
            }
        }
    }

    fn blend_masked_color(&mut self, bgra: &mut [u8], w: u32, h: u32, len: usize) {
        let Some(shape) = self.shape else { return };
        let (pw, ph, pitch) = (shape.Width as usize, shape.Height as usize, shape.Pitch as usize);
        for py in 0..ph {
            let Some(dy) = self.row_of(py, h) else { continue };
            let Some(row) = self.row(py, pw * 3, pitch, len) else { continue };
            for px in 0..pw {
                let Some(di) = self.offset(dy, px, w, bgra.len()) else { continue };
                let s = &row[px * 3..px * 3 + 3];
                for c in 0..3 {
                    bgra[di + c] ^= s[c];
                }
            }
        }
    }

    fn blend_monochrome(&mut self, bgra: &mut [u8], w: u32, h: u32, len: usize) {
        let Some(shape) = self.shape else { return };
        let (pw, pitch) = (shape.Width as usize, shape.Pitch as usize);
        let ph = shape.Height as usize / 2;
        let stride = (pw + 31) / 32 * 4;
        for py in 0..ph {
            let Some(dy) = self.row_of(py, h) else { continue };
            let (Some(and_row), Some(xor_row)) = (
                self.row(py, stride, pitch, len),
                self.row(py + ph, stride, pitch, len),
            ) else {
                continue;
            };
            for px in 0..pw {
                let bit = 0x80u8 >> (px & 7);
                let and = and_row[px / 8] & bit != 0;
                let xor = xor_row[px / 8] & bit != 0;
                if and && xor {
                    continue;
                }
                let Some(di) = self.offset(dy, px, w, bgra.len()) else { continue };
                match (and, xor) {
                    (false, false) => bgra[di..di + 3].copy_from_slice(&[0, 0, 0]),
                    (false, true) => bgra[di..di + 3].copy_from_slice(&[255, 255, 255]),
                    (true, false) => {
                        for c in 0..3 {
                            bgra[di + c] = !bgra[di + c];
                        }
                    }
                    (true, true) => unreachable!("handled above"),
                }
            }
        }
    }

    fn row_of(&self, py: usize, h: u32) -> Option<usize> {
        let dy = self.at.1 + py as i32 - self.hot_y();
        if dy < 0 || dy >= h as i32 {
            return None;
        }
        Some(dy as usize)
    }

    fn hot_x(&self) -> i32 {
        self.shape.map_or(0, |s| s.HotSpot.x)
    }

    fn hot_y(&self) -> i32 {
        self.shape.map_or(0, |s| s.HotSpot.y)
    }

    fn row(&self, py: usize, row_bytes: usize, pitch: usize, len: usize) -> Option<&[u8]> {
        let start = py.checked_mul(pitch)?;
        let end = start.checked_add(row_bytes)?;
        (end <= len).then(|| &self.buf[start..end])
    }

    fn offset(&self, dy: usize, px: usize, w: u32, len: usize) -> Option<usize> {
        let dx = self.at.0 + px as i32 - self.hot_x();
        if dx < 0 || dx >= w as i32 {
            return None;
        }
        let di = (dy * w as usize + dx as usize) * 4;
        (di + 3 <= len).then_some(di)
    }

    fn signature(&self) -> (bool, (i32, i32), u64) {
        (self.visible, self.at, self.shape_gen)
    }

    fn advance(&mut self, visible: bool, fresh_mouse_update: bool, at: (i32, i32)) {
        if visible {
            self.at = at;
            self.visible = true;
        } else if fresh_mouse_update {
            self.visible = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(w: u32, h: u32, b: u8, g: u8, r: u8) -> Vec<u8> {
        let mut v = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..(w * h) {
            v.extend_from_slice(&[b, g, r, 255]);
        }
        v
    }

    fn color_shape(w: usize, h: usize, buf: &mut [u8], fill: impl Fn(usize, usize) -> [u8; 4]) {
        let mut i = 0;
        for y in 0..h {
            for x in 0..w {
                let p = fill(x, y);
                buf[i..i + 4].copy_from_slice(&p);
                i += 4;
            }
        }
    }

    fn ptr(shape: Option<DXGI_OUTDUPL_POINTER_SHAPE_INFO>, at: (i32, i32)) -> PointerShape {
        PointerShape {
            shape,
            at,
            visible: true,
            ..Default::default()
        }
    }

    fn shape_info(ty: u32, w: u32, h: u32, pitch: u32, hot: (i32, i32)) -> DXGI_OUTDUPL_POINTER_SHAPE_INFO {
        DXGI_OUTDUPL_POINTER_SHAPE_INFO {
            Type: ty,
            Width: w,
            Height: h,
            Pitch: pitch,
            HotSpot: windows::Win32::Foundation::POINT { x: hot.0, y: hot.1 },
        }
    }

    #[test]
    fn color_blend_respects_alpha() {
        let (w, h) = (8u32, 8u32);
        let info = shape_info(DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR.0 as u32, 4, 4, 4 * 4, (0, 0));
        let mut p = ptr(Some(info), (2, 2));

        let mut buf = vec![0u8; 4 * 4 * 4];
        color_shape(4, 4, &mut buf, |x, y| {
            if x >= 2 {
                [0, 0, 0, 0]
            } else if y == 3 {
                [200, 100, 50, 128]
            } else {
                [10, 20, 30, 255]
            }
        });
        p.buf = buf;

        let mut f = flat(w, h, 90, 90, 90);
        p.blend_color(&mut f, w, h, 4 * 4 * 4);

        let px = |x: u32, y: u32| -> (u8, u8, u8) {
            let i = ((y * w + x) * 4) as usize;
            (f[i], f[i + 1], f[i + 2])
        };
        assert_eq!(px(2, 2), (10, 20, 30));
        assert_eq!(px(3, 2), (10, 20, 30));
        assert_eq!(px(4, 2), (90, 90, 90));
        assert_eq!(px(5, 2), (90, 90, 90));
        let over = |s: u8, d: u8, a: u32| ((s as u32 * a + d as u32 * (255 - a)) / 255) as u8;
        assert_eq!(px(2, 5), (over(200, 90, 128), over(100, 90, 128), over(50, 90, 128)));
        assert_eq!(px(7, 7), (90, 90, 90));
        assert_eq!(f[3], 255);
    }

    #[test]
    fn hotspot_offsets_the_bitmap() {
        let (w, h) = (16u32, 16u32);
        let info = shape_info(DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR.0 as u32, 4, 4, 4 * 4, (1, 1));
        let mut p = ptr(Some(info), (8, 8));
        let mut buf = vec![0u8; 4 * 4 * 4];
        color_shape(4, 4, &mut buf, |x, y| {
            if (x, y) == (1, 1) {
                [255, 255, 255, 255]
            } else {
                [0, 0, 0, 0]
            }
        });
        p.buf = buf;

        let mut f = flat(w, h, 0, 0, 0);
        p.blend_color(&mut f, w, h, 4 * 4 * 4);

        let i = ((8 * w + 8) * 4) as usize;
        assert_eq!(&f[i..i + 3], &[255, 255, 255], "hotspot pixel not drawn");
        assert!(f.iter().all(|&b| b == 0 || b == 255));
        let changed = f.chunks(4).filter(|c| c[0] == 255).count();
        assert_eq!(changed, 1, "{changed} pixels changed");
    }

    #[test]
    fn edges_are_clipped() {
        let info = shape_info(DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR.0 as u32, 4, 4, 4 * 4, (0, 0));
        for at in [(0, 0), (7, 7), (3, 0), (0, 3), (-2, -2), (99, 99)] {
            let mut p = ptr(Some(info), at);
            let mut buf = vec![0u8; 4 * 4 * 4];
            color_shape(4, 4, &mut buf, |_, _| [7, 8, 9, 255]);
            p.buf = buf;
            let mut f = flat(8, 8, 1, 1, 1);
            p.blend_color(&mut f, 8, 8, 4 * 4 * 4);
            for c in f.chunks(4) {
                assert!(c == [7, 8, 9, 255] || c == [1, 1, 1, 255], "bad pixel {c:?} at {at:?}");
            }
        }
    }

    #[test]
    fn monochrome_planes_are_read_in_the_right_order() {
        let (w, h) = (8u32, 8u32);
        let info = shape_info(DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME.0 as u32, 4, 8, 4, (0, 0));
        let mut p = ptr(Some(info), (0, 0));

        let mut buf = vec![0u8; 32];
        for row in 0..4 {
            buf[row * 4] = 0b1010_0000;
            buf[(row + 4) * 4] = 0b0011_0000;
        }
        p.buf = buf;

        let mut f = flat(w, h, 100, 100, 100);
        p.blend_monochrome(&mut f, w, h, 32);

        let px = |x: u32, y: u32| -> (u8, u8, u8) {
            let i = ((y * w + x) * 4) as usize;
            (f[i], f[i + 1], f[i + 2])
        };
        assert_eq!(px(0, 0), (155, 155, 155), "and=1 xor=0 should be inverted");
        assert_eq!(px(1, 0), (0, 0, 0), "and=0 xor=0 should be black");
        assert_eq!(px(2, 0), (100, 100, 100), "and=1 xor=1 should be untouched");
        assert_eq!(px(3, 0), (255, 255, 255), "and=0 xor=1 should be white");
        assert_eq!(px(1, 3), (0, 0, 0), "only one AND row was read");
        assert_eq!(px(0, 3), (155, 155, 155));
        assert_eq!(px(3, 3), (255, 255, 255));
        assert_eq!(px(2, 4), (100, 100, 100), "drew past the pointer height");
    }

    #[test]
    fn masked_color_xors_the_destination() {
        let (w, h) = (4u32, 4u32);
        let info = shape_info(DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR.0 as u32, 2, 2, 2 * 3, (0, 0));
        let mut p = ptr(Some(info), (1, 1));
        let mut buf = vec![0u8; 2 * 2 * 3];
        buf[0..3].copy_from_slice(&[0xFF, 0, 0]);
        buf[3..6].copy_from_slice(&[0, 0xFF, 0]);
        buf[6..9].copy_from_slice(&[0, 0, 0xFF]);
        buf[9..12].copy_from_slice(&[0x0F, 0xF0, 0x0F]);
        p.buf = buf;

        let mut f = flat(w, h, 0x10, 0x20, 0x30);
        p.blend_masked_color(&mut f, w, h, 12);

        let px = |x: u32, y: u32| -> (u8, u8, u8) {
            let i = ((y * w + x) * 4) as usize;
            (f[i], f[i + 1], f[i + 2])
        };
        assert_eq!(px(1, 1), (0xEF, 0x20, 0x30));
        assert_eq!(px(2, 1), (0x10, 0xDF, 0x30));
        assert_eq!(px(1, 2), (0x10, 0x20, 0xCF));
        assert_eq!(px(2, 2), (0x1F, 0xD0, 0x3F));
        assert_eq!(px(0, 0), (0x10, 0x20, 0x30), "wrote outside the pointer");
    }

    #[test]
    fn an_unchanged_shape_still_blends() {
        let mut p = PointerShape::default();
        p.offered = 64;
        p.len = 64;
        p.buf = vec![0u8; 64];
        color_shape(4, 4, &mut p.buf, |_, _| [10, 20, 30, 255]);
        p.shape = Some(shape_info(
            DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR.0 as u32,
            4,
            4,
            16,
            (0, 0),
        ));

        p.advance(true, true, (6, 6));
        let mut f = flat(16, 16, 90, 90, 90);
        p.blend_color(&mut f, 16, 16, p.len);
        let i = ((6 * 16 + 6) * 4) as usize;
        assert_eq!(&f[i..i + 3], &[10, 20, 30], "cached shape was not reused");
    }

    #[test]
    fn visibility_follows_the_mouse_update() {
        let mut p = ptr(None, (5, 5));

        p.advance(true, true, (11, 12));
        assert!(p.visible);
        assert_eq!(p.at, (11, 12));

        p.advance(false, false, (999, 999));
        assert!(p.visible, "a desktop-only frame blanked the pointer");
        assert_eq!(p.at, (11, 12), "position was overwritten by a stale frame");

        p.advance(false, true, (999, 999));
        assert!(!p.visible, "a real hide was ignored");
    }

    #[test]
    fn signature_tracks_position_visibility_and_shape() {
        let mut p = ptr(None, (0, 0));
        p.advance(true, true, (1, 2));
        let a = p.signature();
        p.advance(true, true, (3, 4));
        assert_ne!(a, p.signature(), "a moved pointer looked unchanged");
        let b = p.signature();
        p.shape_gen += 1;
        assert_ne!(b, p.signature(), "a new shape looked unchanged");
    }

    #[test]
    fn pitch_and_short_writes_are_respected() {
        let (w, h) = (16u32, 16u32);
        let info = shape_info(DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR.0 as u32, 2, 2, 16, (0, 0));
        let mut p = ptr(Some(info), (5, 5));
        let mut buf = vec![0u8; 32];
        buf[0..4].copy_from_slice(&[1, 2, 3, 255]);
        buf[16..20].copy_from_slice(&[4, 5, 6, 255]);
        p.buf = buf;

        let mut f = flat(w, h, 0, 0, 0);
        p.blend_color(&mut f, w, h, 20);

        let px = |x: u32, y: u32| -> (u8, u8, u8) {
            let i = ((y * w + x) * 4) as usize;
            (f[i], f[i + 1], f[i + 2])
        };
        assert_eq!(px(5, 5), (1, 2, 3));
        assert_eq!(px(5, 6), (0, 0, 0), "read a row past the reported size");
        assert_eq!(px(6, 5), (0, 0, 0), "read past the shape width");
    }
}