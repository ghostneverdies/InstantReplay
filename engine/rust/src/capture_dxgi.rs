use crate::{
    capture::{CapError, FramePacer, Monitor},
    qpc100ns, RawFrame, SharedState,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::time::Duration;
use windows::{
    core::*,
    Win32::Graphics::{Direct3D11::*, Dxgi::*},
};
const RETRY_DELAY: Duration = Duration::from_millis(100);
pub struct DxgiSession {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}
impl DxgiSession {
    pub fn start(
        shared: Arc<SharedState>,
        monitor: Monitor,
    ) -> std::result::Result<Self, CapError> {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let (tx, rx) = mpsc::channel();
        let handle = std::thread::Builder::new()
            .name("ir-dxgi".into())
            .spawn(move || run(shared, monitor, flag, tx))?;
        match rx.recv_timeout(Duration::from_secs(8)) {
            Ok(Ok(())) => Ok(Self {
                stop,
                handle: Some(handle),
            }),
            result => {
                stop.store(true, Ordering::Release);
                let _ = handle.join();
                Err(format!("DXGI startup failed: {result:?}").into())
            }
        }
    }
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
impl Drop for DxgiSession {
    fn drop(&mut self) {
        self.stop();
    }
}
unsafe fn duplication(
    monitor: Monitor,
) -> windows::core::Result<(ID3D11Device, ID3D11DeviceContext, IDXGIOutputDuplication)> {
    let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
    for adapter_index in 0.. {
        let adapter = match factory.EnumAdapters1(adapter_index) {
            Ok(a) => a,
            Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => break,
            Err(e) => return Err(e),
        };
        for output_index in 0.. {
            let output = match adapter.EnumOutputs(output_index) {
                Ok(o) => o,
                Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(e) => return Err(e),
            };
            let desc = output.GetDesc()?;
            if desc.Monitor.0 as isize != monitor.0 {
                continue;
            }
            let adapter: IDXGIAdapter = adapter.cast()?;
            let (device, context) = crate::capture::create_device(Some(&adapter))?;
            let output: IDXGIOutput1 = output.cast()?;
            let duplicate = output.DuplicateOutput(&device)?;
            return Ok((device, context, duplicate));
        }
    }
    Err(Error::from_hresult(DXGI_ERROR_NOT_FOUND))
}
struct AcquiredFrame<'a>(&'a IDXGIOutputDuplication);
impl Drop for AcquiredFrame<'_> {
    fn drop(&mut self) {
        unsafe {
            let _ = self.0.ReleaseFrame();
        }
    }
}
fn run(
    shared: Arc<SharedState>,
    monitor: Monitor,
    stop: Arc<AtomicBool>,
    ready: mpsc::Sender<std::result::Result<(), String>>,
) {
    let mut reported = false;
    let mut pacer = FramePacer::with_tolerance(20_000);
    while shared.running.load(Ordering::Acquire) && !stop.load(Ordering::Acquire) {
        let resources = unsafe { duplication(monitor) };
        let (device, context, dup) = match resources {
            Ok(resources) => resources,
            Err(e) => {
                if !reported {
                    let _ = ready.send(Err(e.to_string()));
                    return;
                }
                shared.n_dxgi_access_lost.fetch_add(1, Ordering::Relaxed);
                std::thread::sleep(RETRY_DELAY);
                continue;
            }
        };
        if !reported {
            reported = true;
            let _ = ready.send(Ok(()));
        }
        let mut staging = None;
        let mut desktop = Vec::new();
        let mut desktop_size = (0, 0);
        let mut composition: Option<ID3D11Texture2D> = None;
        let (encode_device, encode_context) = if shared.gpu_mode.load(Ordering::Acquire) == 1 {
            unsafe { crate::capture::create_device(None) }
                .unwrap_or_else(|_| (device.clone(), context.clone()))
        } else {
            (device.clone(), context.clone())
        };
        if std::env::var_os("IR_DEBUG_ADAPTER").is_some() {
            for (name, d) in [("capture", &device), ("encoder", &encode_device)] {
                unsafe {
                    if let Ok(desc) = d.cast::<IDXGIDevice>().and_then(|d| d.GetAdapter()).and_then(|a| a.GetDesc()) {
                        eprintln!("[dxgi] {name} adapter={} luid={}:{} vendor={}", String::from_utf16_lossy(&desc.Description).trim_end_matches('\0'), desc.AdapterLuid.HighPart, desc.AdapterLuid.LowPart, desc.VendorId);
                    }
                }
            }
        }
        let mut publisher =
            crate::capture::FramePublisher::new(shared.clone(), encode_device.clone(), encode_context.clone());
        publisher.set_pacing_tolerance(20_000);
        let mut pointer = PointerShape::default();
        let mut recreate = false;
        while shared.running.load(Ordering::Acquire) && !stop.load(Ordering::Acquire) {
            shared.health_heartbeat.store(
                qpc100ns() - shared.start_qpc.load(Ordering::Acquire),
                Ordering::Release,
            );
            let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
            let mut resource = None;
            match unsafe { dup.AcquireNextFrame(10, &mut info, &mut resource) } {
                Ok(()) => {}
                Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => {
                    shared.n_dxgi_timeout.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
                Err(e)
                    if e.code() == DXGI_ERROR_ACCESS_LOST
                        || e.code() == DXGI_ERROR_DEVICE_REMOVED
                        || e.code() == DXGI_ERROR_DEVICE_RESET =>
                {
                    shared.n_dxgi_access_lost.fetch_add(1, Ordering::Relaxed);
                    recreate = true;
                    break;
                }
                Err(_) => {
                    shared.n_dxgi_fatal.fetch_add(1, Ordering::Relaxed);
                    shared.has_error.store(true, Ordering::Release);
                    shared.running.store(false, Ordering::Release);
                    shared.raw_wait.notify_all();
                    return;
                }
            }
            let _frame = AcquiredFrame(&dup);
            shared.n_wgc.fetch_add(1, Ordering::Relaxed);
            let cursor = shared.capture_options.load(Ordering::Relaxed) & 2 != 0;
            if cursor {
                pointer.update(&dup, &info);
            }
            let tick = if cursor {
                info.LastPresentTime.max(info.LastMouseUpdateTime)
            } else {
                info.LastPresentTime
            };
            if tick <= 0 {
                continue;
            }
            let time = crate::qpc_ticks_to_100ns(tick);
            let source = match resource.and_then(|r| r.cast::<ID3D11Texture2D>().ok()) {
                Some(t) => t,
                None => continue,
            };
            if info.LastPresentTime != 0 {
                let started = std::time::Instant::now();
                shared.n_frame_diff.fetch_add(1, Ordering::Relaxed);
                match unsafe {
                    crate::capture::read_texture(
                        &device,
                        &context,
                        &source,
                        &mut staging,
                        &mut desktop,
                    )
                } {
                    Ok(size) => desktop_size = size,
                    Err(_) => {
                        recreate = true;
                        break;
                    }
                }
                shared.t_capture_ns.fetch_add(started.elapsed().as_nanos() as i64, Ordering::Relaxed);
            }
            drop(_frame);
            if desktop.is_empty() {
                continue;
            }
            let (w, h) = desktop_size;
            let copied = std::time::Instant::now();
            let mut bgra = shared.take_bgra();
            bgra.extend_from_slice(&desktop);
            if cursor {
                pointer.draw(&mut bgra, w, h);
            }
            shared.t_copy_ns.fetch_add(copied.elapsed().as_nanos() as i64, Ordering::Relaxed);
            if shared.gpu_mode.load(Ordering::Acquire) == 1 {
                let uploaded = std::time::Instant::now();
                let result = unsafe {
                    (|| -> windows::core::Result<()> {
                        let stale = composition.as_ref().is_none_or(|texture| {
                            let mut desc = D3D11_TEXTURE2D_DESC::default();
                            texture.GetDesc(&mut desc);
                            desc.Width != w || desc.Height != h
                        });
                        if stale {
                            let mut desc = D3D11_TEXTURE2D_DESC::default();
                            source.GetDesc(&mut desc);
                            desc.BindFlags = D3D11_BIND_SHADER_RESOURCE.0 as u32;
                            desc.Usage = D3D11_USAGE_DEFAULT;
                            desc.CPUAccessFlags = 0;
                            desc.MiscFlags = 0;
                            encode_device.CreateTexture2D(&desc, None, Some(&mut composition))?;
                        }
                        let texture = composition.as_ref().unwrap();
                        encode_context.UpdateSubresource(texture, 0, None, bgra.as_ptr() as _, w * 4, 0);
                        publisher.publish(texture, time, None)
                    })()
                };
                shared.t_buf_ns.fetch_add(uploaded.elapsed().as_nanos() as i64, Ordering::Relaxed);
                shared.recycle_bgra(bgra);
                if result.is_err() {
                    recreate = true;
                    break;
                }
                continue;
            }
            if time < shared.start_qpc.load(Ordering::Acquire)
                || !pacer.accept(time, shared.fps.load(Ordering::Relaxed).max(0) as u32)
            {
                shared.n_capture_drop.fetch_add(1, Ordering::Relaxed);
                shared.recycle_bgra(bgra);
                continue;
            }
            shared.n_capture.fetch_add(1, Ordering::Relaxed);
            let Some(pts) = shared.capture_time(time) else {
                shared.recycle_bgra(bgra);
                continue;
            };
            shared.last_capture_pts.store(pts, Ordering::Release);
            shared.push_frame(RawFrame {
                bgra,
                w,
                h,
                pts100ns: pts,
            });
        }
        if !recreate {
            return;
        }
        std::thread::sleep(RETRY_DELAY);
    }
}
pub fn probe_available() -> bool {
    crate::capture::find_monitor(-1).is_some_and(|mon| unsafe { duplication(mon.monitor).is_ok() })
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
    fn update(
        &mut self,
        duplication: &IDXGIOutputDuplication,
        frame_info: &DXGI_OUTDUPL_FRAME_INFO,
    ) {
        self.advance(
            frame_info.PointerPosition.Visible.as_bool(),
            frame_info.LastMouseUpdateTime != 0,
            (
                frame_info.PointerPosition.Position.x,
                frame_info.PointerPosition.Position.y,
            ),
        );

        let offered = frame_info.PointerShapeBufferSize;
        if offered != 0 || self.shape.is_none() {
            self.offered = offered;
            let capacity = (offered as usize).max(4096);
            if self.buf.len() < capacity {
                self.buf.resize(capacity, 0);
            }
            let mut needed = 0u32;
            let mut info = DXGI_OUTDUPL_POINTER_SHAPE_INFO::default();
            let mut fetched = unsafe {
                duplication.GetFramePointerShape(
                    self.buf.len() as u32,
                    self.buf.as_mut_ptr() as *mut std::ffi::c_void,
                    &mut needed,
                    &mut info,
                )
            };
            if fetched.is_err() && needed as usize > self.buf.len() {
                self.buf.resize(needed as usize, 0);
                fetched = unsafe {
                    duplication.GetFramePointerShape(
                        self.buf.len() as u32,
                        self.buf.as_mut_ptr() as *mut std::ffi::c_void,
                        &mut needed,
                        &mut info,
                    )
                };
            }
            match fetched {
                Ok(()) => {
                    self.len = needed as usize;
                    self.shape = Some(info);
                    self.failed = false;
                    self.shape_gen += 1;
                    if std::env::var_os("IR_DEBUG_POINTER").is_some() {
                        eprintln!(
                            "[ptr] fetch {}x{} pitch={} hot=({},{}) ty={} len={} at=({},{})",
                            info.Width,
                            info.Height,
                            info.Pitch,
                            info.HotSpot.x,
                            info.HotSpot.y,
                            info.Type,
                            needed,
                            self.at.0,
                            self.at.1
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
    }

    fn draw(&mut self, bgra: &mut [u8], w: u32, h: u32) {
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
        let (pw, ph, pitch) = (
            shape.Width as usize,
            shape.Height as usize,
            shape.Pitch as usize,
        );
        for py in 0..ph {
            let Some(dy) = self.row_of(py, h) else {
                continue;
            };
            let Some(row) = self.row(py, pw * 4, pitch, len) else {
                continue;
            };
            for px in 0..pw {
                let a = row[px * 4 + 3];
                if a == 0 {
                    continue;
                }
                let Some(di) = self.offset(dy, px, w, bgra.len()) else {
                    continue;
                };
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
        let (pw, ph, pitch) = (
            shape.Width as usize,
            shape.Height as usize,
            shape.Pitch as usize,
        );
        for py in 0..ph {
            let Some(dy) = self.row_of(py, h) else {
                continue;
            };
            let Some(row) = self.row(py, pw * 4, pitch, len) else {
                continue;
            };
            for px in 0..pw {
                let Some(di) = self.offset(dy, px, w, bgra.len()) else {
                    continue;
                };
                let s = &row[px * 4..px * 4 + 3];
                for c in 0..3 {
                    bgra[di + c] = if row[px * 4 + 3] == 0 {
                        s[c]
                    } else {
                        bgra[di + c] ^ s[c]
                    };
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
            let Some(dy) = self.row_of(py, h) else {
                continue;
            };
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
                if and && !xor {
                    continue;
                }
                let Some(di) = self.offset(dy, px, w, bgra.len()) else {
                    continue;
                };
                match (and, xor) {
                    (false, false) => bgra[di..di + 3].copy_from_slice(&[0, 0, 0]),
                    (false, true) => bgra[di..di + 3].copy_from_slice(&[255, 255, 255]),
                    (true, true) => {
                        for c in 0..3 {
                            bgra[di + c] = !bgra[di + c];
                        }
                    }
                    (true, false) => unreachable!("handled above"),
                }
            }
        }
    }

    fn row_of(&self, py: usize, h: u32) -> Option<usize> {
        let dy = self.at.1 + py as i32;
        if dy < 0 || dy >= h as i32 {
            return None;
        }
        Some(dy as usize)
    }

    fn row(&self, py: usize, row_bytes: usize, pitch: usize, len: usize) -> Option<&[u8]> {
        let start = py.checked_mul(pitch)?;
        let end = start.checked_add(row_bytes)?;
        (end <= len).then(|| &self.buf[start..end])
    }

    fn offset(&self, dy: usize, px: usize, w: u32, len: usize) -> Option<usize> {
        let dx = self.at.0 + px as i32;
        if dx < 0 || dx >= w as i32 {
            return None;
        }
        let di = (dy * w as usize + dx as usize) * 4;
        (di + 3 <= len).then_some(di)
    }

    #[cfg(test)]
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

    fn shape_info(
        ty: u32,
        w: u32,
        h: u32,
        pitch: u32,
        hot: (i32, i32),
    ) -> DXGI_OUTDUPL_POINTER_SHAPE_INFO {
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
        let info = shape_info(
            DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR.0 as u32,
            4,
            4,
            4 * 4,
            (0, 0),
        );
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
        assert_eq!(
            px(2, 5),
            (over(200, 90, 128), over(100, 90, 128), over(50, 90, 128))
        );
        assert_eq!(px(7, 7), (90, 90, 90));
        assert_eq!(f[3], 255);
    }

    #[test]
    fn dxgi_position_is_shape_origin_not_hotspot() {
        let (w, h) = (16u32, 16u32);
        let info = shape_info(
            DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR.0 as u32,
            4,
            4,
            4 * 4,
            (1, 1),
        );
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

        let i = ((9 * w + 9) * 4) as usize;
        assert_eq!(
            &f[i..i + 3],
            &[255, 255, 255],
            "DXGI shape origin must not be offset by the hotspot"
        );
        assert!(f.iter().all(|&b| b == 0 || b == 255));
        let changed = f.chunks(4).filter(|c| c[0] == 255).count();
        assert_eq!(changed, 1, "{changed} pixels changed");
    }

    #[test]
    fn edges_are_clipped() {
        let info = shape_info(
            DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR.0 as u32,
            4,
            4,
            4 * 4,
            (0, 0),
        );
        for at in [(0, 0), (7, 7), (3, 0), (0, 3), (-2, -2), (99, 99)] {
            let mut p = ptr(Some(info), at);
            let mut buf = vec![0u8; 4 * 4 * 4];
            color_shape(4, 4, &mut buf, |_, _| [7, 8, 9, 255]);
            p.buf = buf;
            let mut f = flat(8, 8, 1, 1, 1);
            p.blend_color(&mut f, 8, 8, 4 * 4 * 4);
            for c in f.chunks(4) {
                assert!(
                    c == [7, 8, 9, 255] || c == [1, 1, 1, 255],
                    "bad pixel {c:?} at {at:?}"
                );
            }
        }
    }

    #[test]
    fn monochrome_planes_are_read_in_the_right_order() {
        let (w, h) = (8u32, 8u32);
        let info = shape_info(
            DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME.0 as u32,
            4,
            8,
            4,
            (0, 0),
        );
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
        assert_eq!(px(0, 0), (100, 100, 100), "and=1 xor=0 should be unchanged");
        assert_eq!(px(1, 0), (0, 0, 0), "and=0 xor=0 should be black");
        assert_eq!(px(2, 0), (155, 155, 155), "and=1 xor=1 should be inverted");
        assert_eq!(px(3, 0), (255, 255, 255), "and=0 xor=1 should be white");
        assert_eq!(px(1, 3), (0, 0, 0), "only one AND row was read");
        assert_eq!(px(0, 3), (100, 100, 100));
        assert_eq!(px(3, 3), (255, 255, 255));
        assert_eq!(px(2, 4), (100, 100, 100), "drew past the pointer height");
    }

    #[test]
    fn masked_color_xors_the_destination() {
        let (w, h) = (4u32, 4u32);
        let info = shape_info(
            DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR.0 as u32,
            2,
            2,
            2 * 4,
            (0, 0),
        );
        let mut p = ptr(Some(info), (1, 1));
        let mut buf = vec![0u8; 2 * 2 * 4];
        buf[0..4].copy_from_slice(&[0xFF, 0, 0, 255]);
        buf[4..8].copy_from_slice(&[0, 0xFF, 0, 255]);
        buf[8..12].copy_from_slice(&[0, 0, 0xFF, 255]);
        buf[12..16].copy_from_slice(&[0x0F, 0xF0, 0x0F, 0]);
        p.buf = buf;

        let mut f = flat(w, h, 0x10, 0x20, 0x30);
        p.blend_masked_color(&mut f, w, h, 16);

        let px = |x: u32, y: u32| -> (u8, u8, u8) {
            let i = ((y * w + x) * 4) as usize;
            (f[i], f[i + 1], f[i + 2])
        };
        assert_eq!(px(1, 1), (0xEF, 0x20, 0x30));
        assert_eq!(px(2, 1), (0x10, 0xDF, 0x30));
        assert_eq!(px(1, 2), (0x10, 0x20, 0xCF));
        assert_eq!(px(2, 2), (0x0F, 0xF0, 0x0F));
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
        let info = shape_info(
            DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR.0 as u32,
            2,
            2,
            16,
            (0, 0),
        );
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
