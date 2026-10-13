use crate::{encoder::GpuInput, qpc100ns, RawFrame, SharedState};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};
use std::time::Duration;
use windows::{
    core::*,
    Foundation::TypedEventHandler,
    Graphics::{Capture::*, DirectX::DirectXPixelFormat, SizeInt32},
    Win32::{
        Foundation::*,
        Graphics::{Direct3D::*, Direct3D11::*, Dwm::*, Dxgi::*, Gdi::*},
        System::WinRT::{Direct3D11::*, Graphics::Capture::*, *},
        UI::{HiDpi::*, WindowsAndMessaging::*},
    },
};
pub type CapError = Box<dyn std::error::Error + Send + Sync>;
pub const GAP_BUCKETS: usize = 8;
fn empty_frame_queue(error: &windows::core::Error) -> bool {
    error.code().is_ok()
}
pub fn gap_bucket(ms: f64) -> usize {
    [5., 10., 14., 18., 25., 35., 50.]
        .iter()
        .position(|&v| ms < v)
        .unwrap_or(7)
}
#[derive(Clone, Copy, Debug)]
pub struct Monitor(pub isize);
impl Monitor {
    pub fn as_raw_hmonitor(self) -> *mut std::ffi::c_void {
        self.0 as *mut _
    }
}
#[derive(Clone, Copy, Debug)]
pub struct MonInfo {
    pub monitor: Monitor,
    pub w: i32,
    pub h: i32,
    pub primary: bool,
}
unsafe extern "system" fn monitor_enum(m: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    let mut info = MONITORINFO::default();
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(m, &mut info).as_bool() {
        (&mut *(data.0 as *mut Vec<MonInfo>)).push(MonInfo {
            monitor: Monitor(m.0 as isize),
            w: info.rcMonitor.right - info.rcMonitor.left,
            h: info.rcMonitor.bottom - info.rcMonitor.top,
            primary: info.dwFlags & MONITORINFOF_PRIMARY != 0,
        });
    }
    TRUE
}
pub fn enumerate_monitors() -> Vec<MonInfo> {
    let mut list: Vec<MonInfo> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(monitor_enum),
            LPARAM(&mut list as *mut _ as isize),
        );
    }
    list.sort_by_key(|m| !m.primary);
    list
}
pub fn find_monitor(index: i32) -> Option<MonInfo> {
    let list = enumerate_monitors();
    list.get(index.max(0) as usize).copied()
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureMethod {
    Wgc = 0,
    Dxgi = 1,
}
impl CaptureMethod {
    pub fn from_i32(v: i32) -> Self {
        if v == 1 {
            Self::Dxgi
        } else {
            Self::Wgc
        }
    }
    pub fn as_i32(self) -> i32 {
        self as i32
    }
    pub fn label(self) -> &'static str {
        if self == Self::Dxgi {
            "DXGI duplication"
        } else {
            "WGC"
        }
    }
}
#[derive(Default, Debug)]
pub struct FramePacer {
    last_seen: i64,
    next: u128,
    tolerance: i64,
}
impl FramePacer {
    pub fn with_tolerance(tolerance: i64) -> Self {
        Self { tolerance: tolerance.max(0), ..Self::default() }
    }
    pub fn accept(&mut self, time: i64, fps: u32) -> bool {
        if time <= 0 || time <= self.last_seen {
            return false;
        }
        self.last_seen = time;
        if fps == 0 {
            return true;
        }
        let scaled = time as u128 * fps as u128;
        let tolerance = (self.tolerance as u128).min(1_250_000 / fps as u128);
        if scaled + tolerance * (fps as u128) < self.next {
            return false;
        }
        if self.next == 0 {
            self.next = scaled;
        }
        if self.tolerance > 0 && scaled.saturating_sub(self.next) >= 10_000_000 {
            self.next = scaled + 10_000_000;
        } else {
            self.next += ((scaled.saturating_sub(self.next) / 10_000_000) + 1) * 10_000_000;
        }
        true
    }
}
pub unsafe fn create_device(
    adapter: Option<&IDXGIAdapter>,
) -> windows::core::Result<(ID3D11Device, ID3D11DeviceContext)> {
    let mut device = None;
    let mut context = None;
    D3D11CreateDevice(
        adapter,
        if adapter.is_some() {
            D3D_DRIVER_TYPE_UNKNOWN
        } else {
            D3D_DRIVER_TYPE_HARDWARE
        },
        HMODULE::default(),
        D3D11_CREATE_DEVICE_BGRA_SUPPORT,
        None,
        D3D11_SDK_VERSION,
        Some(&mut device),
        None,
        Some(&mut context),
    )?;
    let device = device.ok_or_else(|| Error::from_hresult(E_FAIL))?;
    let context = context.ok_or_else(|| Error::from_hresult(E_FAIL))?;
    let mt: ID3D11Multithread = context.cast()?;
    let _ = mt.SetMultithreadProtected(true);
    Ok((device, context))
}
pub unsafe fn read_texture(
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    texture: &ID3D11Texture2D,
    staging: &mut Option<ID3D11Texture2D>,
    out: &mut Vec<u8>,
) -> windows::core::Result<(u32, u32)> {
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    texture.GetDesc(&mut desc);
    let stale = staging.as_ref().is_none_or(|t| {
        let mut d = D3D11_TEXTURE2D_DESC::default();
        t.GetDesc(&mut d);
        d.Width != desc.Width || d.Height != desc.Height || d.Format != desc.Format
    });
    if stale {
        desc.Usage = D3D11_USAGE_STAGING;
        desc.BindFlags = 0;
        desc.MiscFlags = 0;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        device.CreateTexture2D(&desc, None, Some(staging))?;
    }
    let staging = staging
        .as_ref()
        .ok_or_else(|| Error::from_hresult(E_FAIL))?;
    context.CopyResource(staging, texture);
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    context.Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
    let row = desc.Width as usize * 4;
    if mapped.pData.is_null() || (mapped.RowPitch as usize) < row {
        context.Unmap(staging, 0);
        return Err(Error::from_hresult(E_FAIL));
    }
    out.resize(row * desc.Height as usize, 0);
    for y in 0..desc.Height as usize {
        out[y * row..(y + 1) * row].copy_from_slice(std::slice::from_raw_parts(
            (mapped.pData as *const u8).add(y * mapped.RowPitch as usize),
            row,
        ));
    }
    context.Unmap(staging, 0);
    Ok((desc.Width, desc.Height))
}
pub struct FramePublisher {
    pub shared: Arc<SharedState>,
    pub device: ID3D11Device,
    pub context: ID3D11DeviceContext,
    pacer: FramePacer,
    staging: Option<ID3D11Texture2D>,
    gpu: Option<Arc<GpuInput>>,
    crop: Option<ID3D11Texture2D>,
}
impl FramePublisher {
    pub fn set_pacing_tolerance(&mut self, tolerance: i64) {
        self.pacer = FramePacer::with_tolerance(tolerance);
    }
    pub fn new(
        shared: Arc<SharedState>,
        device: ID3D11Device,
        context: ID3D11DeviceContext,
    ) -> Self {
        Self {
            shared,
            device,
            context,
            pacer: FramePacer::default(),
            staging: None,
            gpu: None,
            crop: None,
        }
    }
    pub fn accept(&mut self, source_time: i64) -> bool {
        if source_time < self.shared.start_qpc.load(Ordering::Acquire) {
            return false;
        }
        let accepted = self.pacer.accept(
            source_time,
            self.shared.fps.load(Ordering::Relaxed).max(0) as u32,
        );
        if !accepted {
            self.shared.n_capture_drop.fetch_add(1, Ordering::Relaxed);
        }
        accepted
    }
    pub unsafe fn publish(
        &mut self,
        texture: &ID3D11Texture2D,
        source_time: i64,
        content: Option<SizeInt32>,
    ) -> windows::core::Result<()> {
        let Some(pts) = self.shared.capture_time(source_time) else {
            return Ok(());
        };
        if !self.accept(source_time) {
            return Ok(());
        }
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        texture.GetDesc(&mut desc);
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: desc.Width as i32,
            bottom: desc.Height as i32,
        };
        if let Some(size) = content {
            rect.right = rect.right.min(size.Width);
            rect.bottom = rect.bottom.min(size.Height);
        }
        let window = HWND(self.shared.target_window.load(Ordering::Relaxed) as *mut _);
        if !window.0.is_null() && self.shared.capture_options.load(Ordering::Relaxed) & 1 != 0 {
            let mut frame = RECT::default();
            let mut client = RECT::default();
            if DwmGetWindowAttribute(
                window,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut frame as *mut _ as _,
                std::mem::size_of::<RECT>() as u32,
            )
            .is_ok()
                && GetClientRect(window, &mut client).is_ok()
            {
                let mut points = [
                    POINT {
                        x: client.left,
                        y: client.top,
                    },
                    POINT {
                        x: client.right,
                        y: client.bottom,
                    },
                ];
                let _ = MapWindowPoints(Some(window), None, &mut points);
                rect.left = (points[0].x - frame.left).clamp(0, rect.right);
                rect.top = (points[0].y - frame.top).clamp(0, rect.bottom);
                rect.right = (points[1].x - frame.left).clamp(rect.left, rect.right);
                rect.bottom = (points[1].y - frame.top).clamp(rect.top, rect.bottom);
            }
        }
        let (w, h) = (
            (rect.right - rect.left) as u32,
            (rect.bottom - rect.top) as u32,
        );
        if w < 2 || h < 2 {
            return Ok(());
        }
        let needs_crop = rect.left != 0 || rect.top != 0 || w != desc.Width || h != desc.Height;
        let cropped = if needs_crop {
            let stale = self.crop.as_ref().is_none_or(|t| {
                let mut d = D3D11_TEXTURE2D_DESC::default();
                t.GetDesc(&mut d);
                d.Width != w || d.Height != h
            });
            if stale {
                desc.Width = w;
                desc.Height = h;
                desc.BindFlags = D3D11_BIND_SHADER_RESOURCE.0 as u32;
                desc.Usage = D3D11_USAGE_DEFAULT;
                desc.CPUAccessFlags = 0;
                desc.MiscFlags = 0;
                self.device
                    .CreateTexture2D(&desc, None, Some(&mut self.crop))?;
            }
            let cropped = self.crop.as_ref().unwrap();
            let bounds = D3D11_BOX {
                left: rect.left as u32,
                top: rect.top as u32,
                front: 0,
                right: rect.right as u32,
                bottom: rect.bottom as u32,
                back: 1,
            };
            self.context
                .CopySubresourceRegion(cropped, 0, 0, 0, 0, texture, 0, Some(&bounds));
            cropped.clone()
        } else {
            texture.clone()
        };
        if self.shared.gpu_mode.load(Ordering::Acquire) == 1 {
            if self.gpu.as_ref().is_none_or(|g| g.w != w || g.h != h) {
                match GpuInput::create(&self.device, &self.context, w, h) {
                    Ok(g) => {
                        let g = Arc::new(g);
                        self.gpu = Some(g.clone());
                        *self.shared.gpu.lock().unwrap_or_else(|e| e.into_inner()) = Some(g);
                        self.shared.gpu_gen.fetch_add(1, Ordering::Release);
                    }
                    Err(_) => {
                        self.shared.gpu_mode.store(0, Ordering::Release);
                        self.gpu = None;
                    }
                }
            }
            if let Some(g) = &self.gpu {
                g.submit(&cropped, pts);
                self.shared.last_capture_pts.store(pts, Ordering::Release);
                self.shared.gpu_frames.fetch_add(1, Ordering::Release);
                self.shared.n_capture.fetch_add(1, Ordering::Relaxed);
                let _queue = self.shared.raw.lock().unwrap_or_else(|e| e.into_inner());
                self.shared.raw_wait.notify_all();
                return Ok(());
            }
        }
        let mut bgra = self.shared.take_bgra();
        let (w, h) = read_texture(
            &self.device,
            &self.context,
            &cropped,
            &mut self.staging,
            &mut bgra,
        )?;
        self.shared.n_capture.fetch_add(1, Ordering::Relaxed);
        self.shared.last_capture_pts.store(pts, Ordering::Release);
        self.shared.push_frame(RawFrame {
            bgra,
            w,
            h,
            pts100ns: pts,
        });
        Ok(())
    }
}
struct WgcSession {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}
impl WgcSession {
    fn start(shared: Arc<SharedState>, monitor: Monitor) -> std::result::Result<Self, CapError> {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let (tx, rx) = mpsc::channel();
        let handle = std::thread::Builder::new()
            .name("ir-wgc".into())
            .spawn(move || {
                let result = unsafe { run_wgc(shared, monitor, flag.clone(), &tx) };
                if let Err(error) = result {
                    let _ = tx.send(Err(error.to_string()));
                }
            })?;
        match rx.recv_timeout(Duration::from_secs(8)) {
            Ok(Ok(())) => Ok(Self {
                stop,
                handle: Some(handle),
            }),
            result => {
                stop.store(true, Ordering::Release);
                let _ = handle.join();
                Err(format!("WGC startup failed: {result:?}").into())
            }
        }
    }
    fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
unsafe fn run_wgc(
    shared: Arc<SharedState>,
    monitor: Monitor,
    stop: Arc<AtomicBool>,
    ready: &mpsc::Sender<std::result::Result<(), String>>,
) -> windows::core::Result<()> {
    if shared.capture_options.load(Ordering::Acquire) & 32 == 0 {
        return wgc_worker(shared, monitor, stop, ready, None);
    }
    let device_context = create_device(None)?;
    let _ = ready.send(Ok(()));
    while shared.running.load(Ordering::Acquire) && !stop.load(Ordering::Acquire) {
        shared.health_heartbeat.store(
            qpc100ns() - shared.start_qpc.load(Ordering::Acquire),
            Ordering::Release,
        );
        if let Some(window) = active_capture_window() {
            shared.target_window.store(window, Ordering::Release);
            // Each session owns its callbacks; closing it before switching prevents stale frames.
            *shared
                .keyframe_pending
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = true;
            let _ = wgc_worker(
                shared.clone(),
                monitor,
                stop.clone(),
                ready,
                Some(device_context.clone()),
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}
unsafe fn wgc_worker(
    shared: Arc<SharedState>,
    monitor: Monitor,
    stop: Arc<AtomicBool>,
    ready: &mpsc::Sender<std::result::Result<(), String>>,
    device_context: Option<(ID3D11Device, ID3D11DeviceContext)>,
) -> windows::core::Result<()> {
    RoInitialize(RO_INIT_MULTITHREADED)?;
    struct Runtime;
    impl Drop for Runtime {
        fn drop(&mut self) {
            unsafe {
                RoUninitialize();
            }
        }
    }
    let _runtime = Runtime;
    let _ = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    let (device, context) = match device_context {
        Some(pair) => pair,
        None => create_device(None)?,
    };
    let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
    let window = HWND(shared.target_window.load(Ordering::Acquire) as *mut _);
    let item: GraphicsCaptureItem = if window.0.is_null() {
        interop.CreateForMonitor(HMONITOR(monitor.as_raw_hmonitor()))?
    } else {
        interop.CreateForWindow(window)?
    };
    let size = item.Size()?;
    if size.Width < 2 || size.Height < 2 {
        return Err(Error::from_hresult(E_INVALIDARG));
    }
    let dxgi: IDXGIDevice = device.cast()?;
    let winrt = CreateDirect3D11DeviceFromDXGIDevice(&dxgi)?
        .cast::<windows::Graphics::DirectX::Direct3D11::IDirect3DDevice>()?;
    let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
        &winrt,
        DirectXPixelFormat::B8G8R8A8UIntNormalized,
        4,
        size,
    )?;
    let session = pool.CreateCaptureSession(&item)?;
    let _ = session.SetMinUpdateInterval(windows::Foundation::TimeSpan { Duration: 10_000 });
    let options = shared.capture_options.load(Ordering::Relaxed);
    let _ = session.SetIsCursorCaptureEnabled(options & 2 != 0);
    let _ = session.SetIsBorderRequired(options & 4 != 0);
    let _ = session.SetIncludeSecondaryWindows(options & 16 != 0);
    let previous_corner = if !window.0.is_null() && options & 8 == 0 {
        let mut previous = DWM_WINDOW_CORNER_PREFERENCE::default();
        if DwmGetWindowAttribute(
            window,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &mut previous as *mut _ as _,
            4,
        )
        .is_ok()
        {
            let value = DWMWCP_DONOTROUND;
            let _ = DwmSetWindowAttribute(
                window,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &value as *const _ as _,
                4,
            );
            Some(previous)
        } else {
            None
        }
    } else {
        None
    };
    let mut frame_publisher = FramePublisher::new(
        shared.clone(),
        device,
        context,
    );
    frame_publisher.set_pacing_tolerance(20_000);
    let publisher = Arc::new(Mutex::new(frame_publisher));
    let callback_state = publisher.clone();
    let callback_device = AgileReference::new(&winrt)?;
    let current_size = Arc::new(Mutex::new(size));
    let callback_size = current_size.clone();
    let frame_token = pool.FrameArrived(&TypedEventHandler::new(
        move |pool: Ref<'_, Direct3D11CaptureFramePool>, _| {
            let pool = pool.ok()?;
            let mut publisher = callback_state.lock().unwrap_or_else(|e| e.into_inner());
            if !publisher.shared.running.load(Ordering::Acquire) {
                return Ok(());
            }
            let mut frame = match pool.TryGetNextFrame() {
                Ok(frame) => frame,
                Err(error) if empty_frame_queue(&error) => return Ok(()),
                Err(error) => {
                    publisher.shared.has_error.store(true, Ordering::Release);
                    return Err(error);
                }
            };
            for _ in 0..3 {
                let next = match pool.TryGetNextFrame() {
                    Ok(next) => next,
                    Err(error) if empty_frame_queue(&error) => break,
                    Err(error) => {
                        let _ = frame.Close();
                        publisher.shared.has_error.store(true, Ordering::Release);
                        return Err(error);
                    }
                };
                let _ = frame.Close();
                frame = next;
                publisher.shared.n_capture_drop.fetch_add(1, Ordering::Relaxed);
            }
            let content = frame.ContentSize()?;
            let time = frame.SystemRelativeTime()?.Duration;
            let previous = *callback_size.lock().unwrap_or_else(|e| e.into_inner());
            let resized = previous.Width != content.Width || previous.Height != content.Height;
            let result = (|| -> windows::core::Result<()> {
                if resized || content.Width < 2 || content.Height < 2 {
                    return Ok(());
                }
                let access: IDirect3DDxgiInterfaceAccess = frame.Surface()?.cast()?;
                let texture: ID3D11Texture2D = access.GetInterface()?;
                publisher.shared.n_wgc.fetch_add(1, Ordering::Relaxed);
                publisher.publish(&texture, time, Some(content))
            })();
            let _ = frame.Close();
            let mut previous = callback_size.lock().unwrap_or_else(|e| e.into_inner());
            if content.Width > 0
                && content.Height > 0
                && (previous.Width != content.Width || previous.Height != content.Height)
            {
                let recreated = pool.Recreate(
                    &callback_device.resolve()?,
                    DirectXPixelFormat::B8G8R8A8UIntNormalized,
                    4,
                    content,
                );
                if recreated.is_err() {
                    publisher.shared.has_error.store(true, Ordering::Release);
                }
                recreated?;
                *previous = content;
            }
            if result.is_err() {
                publisher.shared.has_error.store(true, Ordering::Release);
            }
            result
        },
    ))?;
    let closed = Arc::new(AtomicBool::new(false));
    let closed_flag = closed.clone();
    let close_token = item.Closed(&TypedEventHandler::new(move |_, _| {
        closed_flag.store(true, Ordering::Release);
        Ok(())
    }))?;
    session.StartCapture()?;
    let mut selection_check = std::time::Instant::now();
    let _ = ready.send(Ok(()));
    while shared.running.load(Ordering::Acquire)
        && !stop.load(Ordering::Acquire)
        && !closed.load(Ordering::Acquire)
    {
        if options & 32 != 0 && selection_check.elapsed() >= Duration::from_millis(200) {
            selection_check = std::time::Instant::now();
            if active_capture_window() != Some(window.0 as isize) {
                break;
            }
        }
        shared.health_heartbeat.store(
            qpc100ns() - shared.start_qpc.load(Ordering::Acquire),
            Ordering::Release,
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    if closed.load(Ordering::Acquire) && options & 32 == 0 {
        shared.has_error.store(true, Ordering::Release);
        shared.running.store(false, Ordering::Release);
        shared.raw_wait.notify_all();
    }
    let _ = pool.RemoveFrameArrived(frame_token);
    let _ = item.RemoveClosed(close_token);
    let _ = session.Close();
    let _ = pool.Close();
    if let Some(previous) = previous_corner {
        let _ = DwmSetWindowAttribute(
            window,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &previous as *const _ as _,
            4,
        );
    }
    Ok(())
}
enum SessionImpl {
    Wgc(WgcSession),
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
    ) -> std::result::Result<Self, CapError> {
        let monitor = find_monitor(index).ok_or("no monitor found")?;
        let window = shared.target_window.load(Ordering::Acquire) != 0
            || shared.capture_options.load(Ordering::Acquire) & 32 != 0;
        let method = if window { CaptureMethod::Wgc } else { method };
        shared
            .capture_method
            .store(method.as_i32(), Ordering::Release);
        let inner = match method {
            CaptureMethod::Wgc => SessionImpl::Wgc(WgcSession::start(shared, monitor.monitor)?),
            CaptureMethod::Dxgi => SessionImpl::Dxgi(crate::capture_dxgi::DxgiSession::start(
                shared,
                monitor.monitor,
            )?),
        };
        Ok(Self { inner: Some(inner) })
    }
    pub fn stop(&mut self) {
        match self.inner.take() {
            Some(SessionImpl::Wgc(mut s)) => s.stop(),
            Some(SessionImpl::Dxgi(mut s)) => s.stop(),
            None => {}
        }
    }
}
impl Drop for CaptureSession {
    fn drop(&mut self) {
        self.stop();
    }
}
#[derive(Clone, Debug)]
pub struct WindowInfo {
    pub handle: isize,
    pub title: String,
    pub w: i32,
    pub h: i32,
}
unsafe extern "system" fn window_enum(window: HWND, param: LPARAM) -> BOOL {
    if !IsWindowVisible(window).as_bool()
        || GetAncestor(window, GA_ROOTOWNER) != window
        || GetWindowLongPtrW(window, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0
    {
        return TRUE;
    }
    let mut cloaked = 0u32;
    let _ = DwmGetWindowAttribute(window, DWMWA_CLOAKED, &mut cloaked as *mut _ as _, 4);
    if cloaked != 0 {
        return TRUE;
    }
    let mut class = [0u16; 256];
    let length = GetClassNameW(window, &mut class);
    let class = String::from_utf16_lossy(&class[..length.max(0) as usize]);
    if [
        "Progman",
        "WorkerW",
        "Shell_TrayWnd",
        "Shell_SecondaryTrayWnd",
        "Windows.UI.Core.CoreWindow",
        "CabinetWClass",
        "ExploreWClass",
    ]
    .contains(&class.as_str())
    {
        return TRUE;
    }
    let mut pid = 0;
    GetWindowThreadProcessId(window, Some(&mut pid));
    if pid == std::process::id() || excluded_process(&process_name(pid)) {
        return TRUE;
    }
    let mut title = [0u16; 512];
    let length = GetWindowTextW(window, &mut title);
    if length <= 0 {
        return TRUE;
    }
    let title = String::from_utf16_lossy(&title[..length as usize]);
    if [
        "Program Manager",
        "Windows Input Experience",
        "Microsoft Text Input Application",
    ]
    .contains(&title.as_str())
    {
        return TRUE;
    }
    if let Some((w, h)) = window_size(window, false) {
        (&mut *(param.0 as *mut Vec<WindowInfo>)).push(WindowInfo {
            handle: window.0 as isize,
            title,
            w,
            h,
        });
    }
    TRUE
}
pub fn enumerate_windows() -> Vec<WindowInfo> {
    let mut list: Vec<WindowInfo> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(window_enum), LPARAM(&mut list as *mut _ as isize));
    }
    list.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    list
}
fn process_name(pid: u32) -> String {
    use windows::Win32::System::Threading::*;
    unsafe {
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return String::new();
        };
        let mut path = [0u16; 32768];
        let mut length = path.len() as u32;
        let ok = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut length,
        )
        .is_ok();
        let _ = CloseHandle(process);
        if !ok {
            return String::new();
        }
        String::from_utf16_lossy(&path[..length as usize])
            .rsplit('\\')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
    }
}
fn excluded_process(name: &str) -> bool {
    name.is_empty()
        || [
            "explorer.exe",
            "shellexperiencehost.exe",
            "startmenuexperiencehost.exe",
            "searchhost.exe",
            "searchapp.exe",
            "textinputhost.exe",
            "applicationframehost.exe",
            "lockapp.exe",
            "dwm.exe",
        ]
        .contains(&name)
}
fn known_game(name: &str, class: &str) -> bool {
    [
        "cs2.exe",
        "csgo.exe",
        "valorant-win64-shipping.exe",
        "fortniteclient-win64-shipping.exe",
        "r5apex.exe",
        "overwatch.exe",
        "rocketleague.exe",
        "gta5.exe",
        "gta5_enhanced.exe",
        "rdr2.exe",
        "cod.exe",
        "pubg.exe",
        "tslgame.exe",
        "dota2.exe",
        "league of legends.exe",
        "eldenring.exe",
        "cyberpunk2077.exe",
        "robloxplayerbeta.exe",
        "minecraft.windows.exe",
        "terraria.exe",
        "witcher3.exe",
        "destiny2.exe",
        "rainbowsix.exe",
        "rainbowsix_dx11.exe",
    ]
    .contains(&name)
        || (["javaw.exe", "java.exe"].contains(&name) && ["GLFW30", "LWJGL"].contains(&class))
}
fn active_capture_window() -> Option<isize> {
    let windows = enumerate_windows();
    let foreground = unsafe { GetAncestor(GetForegroundWindow(), GA_ROOT) };
    if let Some(window) = windows.iter().find(|w| w.handle == foreground.0 as isize) {
        return Some(window.handle);
    }
    // Launchers, desktop and our settings must not replace a visible game target.
    windows
        .iter()
        .find(|w| unsafe {
            let hwnd = HWND(w.handle as *mut _);
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            let mut class = [0u16; 256];
            let length = GetClassNameW(hwnd, &mut class).max(0) as usize;
            known_game(
                &process_name(pid),
                &String::from_utf16_lossy(&class[..length]),
            )
        })
        .map(|w| w.handle)
}
pub unsafe fn window_size(window: HWND, client: bool) -> Option<(i32, i32)> {
    if !IsWindow(Some(window)).as_bool() || IsIconic(window).as_bool() {
        return None;
    }
    let mut rect = RECT::default();
    let result = if client {
        GetClientRect(window, &mut rect)
    } else {
        DwmGetWindowAttribute(
            window,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut rect as *mut _ as _,
            std::mem::size_of::<RECT>() as u32,
        )
    };
    if result.is_err() {
        return None;
    }
    let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
    (w >= 2 && h >= 2).then_some((w, h))
}
#[cfg(test)]
mod tests {
    #[test]
    fn empty_wgc_queue_is_not_a_capture_failure() {
        let empty = unsafe {
            <windows::Graphics::Capture::Direct3D11CaptureFrame as windows::core::Type<_>>::from_abi(std::ptr::null_mut())
        }.err().expect("null frame must be returned as an empty binding error");
        assert!(super::empty_frame_queue(&empty));
        assert!(!super::empty_frame_queue(&windows::core::Error::from_hresult(windows::core::HRESULT(0x80004005u32 as i32))));
        assert!(!super::empty_frame_queue(&windows::core::Error::from_hresult(windows::core::HRESULT(0x887A0005u32 as i32))));
    }

    use super::*;
    #[test]
    fn duplicates_and_stalls_do_not_synthesize_frames() {
        let mut p = FramePacer::default();
        assert!(p.accept(10_000_000, 60));
        assert!(!p.accept(10_000_000, 60));
        assert!(!p.accept(9_000_000, 60));
        assert!(p.accept(110_000_000, 60));
        assert!(!p.accept(110_000_001, 60));
        assert!(p.accept(110_166_667, 60));
    }
    #[test]
    fn fractional_source_rate_has_no_catch_up() {
        let mut p = FramePacer::default();
        let mut kept = 0;
        for i in 0..1440 {
            if p.accept(10_000_000 + i * 10_000_000 / 144, 60) {
                kept += 1;
            }
        }
        assert!((599..=600).contains(&kept));
    }

    #[test]
    fn tolerant_pacing_keeps_jittered_refresh_frames_and_rejects_duplicates() {
        let mut p = FramePacer::with_tolerance(20_000);
        let mut kept = 0;
        for i in 0..600 {
            let time = 10_000_000 + i * 10_000_000 / 60 + if i % 2 == 0 { 9_000 } else { -9_000 };
            kept += p.accept(time, 60) as i32;
            assert!(!p.accept(time, 60));
            assert!(!p.accept(time - 1, 60));
        }
        assert_eq!(kept, 600);
        assert!(p.accept(200_000_000, 60));
        assert!(!p.accept(200_000_001, 60));
    }

    #[test]
    fn tolerant_pacing_still_caps_fast_sources() {
        let mut p = FramePacer::with_tolerance(20_000);
        let mut kept = 0;
        for i in 0..1440 {
            kept += p.accept(10_000_000 + i * 10_000_000 / 144, 60) as i32;
        }
        assert!((599..=600).contains(&kept));
    }

    #[test]
    fn tolerant_pacing_matches_jittered_capture_rates() {
        for fps in [30, 60, 120, 240] {
            let mut p = FramePacer::with_tolerance(20_000);
            let period = 10_000_000 / fps;
            let jitter = (period / 32).min(9_000);
            for i in 0..fps * 10 {
                let time = 10_000_000 + i * 10_000_000 / fps
                    + if i % 2 == 0 { jitter } else { -jitter };
                assert!(p.accept(time, fps as u32), "fps={fps}, frame={i}");
                assert!(!p.accept(time, fps as u32));
            }
        }
    }

    #[test]
    fn tolerant_pacing_resynchronizes_after_capture_stalls() {
        let mut p = FramePacer::with_tolerance(20_000);
        assert!(p.accept(10_000_000, 60));
        assert!(p.accept(100_000_000, 60));
        assert!(!p.accept(100_020_000, 60));
        assert!(!p.accept(99_000_000, 60));
        assert!(p.accept(100_166_666, 60));
    }
}
