#![allow(non_snake_case, dead_code, clippy::missing_safety_doc)]

use std::ffi::{c_char, c_void};
use std::mem::{transmute, ManuallyDrop};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, TryLockError};

use windows::core::{Interface, PCWSTR};
use windows::Win32::Foundation::{HANDLE, HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Direct3D9::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;

const MAGIC: u32 = 0x4952_4652;
const CONTROL_VERSION: u32 = 2;
const VERSION: u32 = 3;
const SLOTS: usize = 3;
const HEADER_BYTES: usize = 4096;
const MAX_W: usize = 3840;
const MAX_H: usize = 2160;
const MAX_SLOT_BYTES: usize = MAX_W * MAX_H * 4;
const RING_BYTES: usize = HEADER_BYTES + SLOTS * MAX_SLOT_BYTES;

const OFF_MAGIC: usize = 0;
const OFF_VERSION: usize = 4;
const OFF_SLOTS: usize = 8;
const OFF_W: usize = 12;
const OFF_H: usize = 16;
const OFF_STRIDE: usize = 20;
const OFF_ACTIVE: usize = 24;
const OFF_PID: usize = 28;
const OFF_LATEST: usize = 32;
const OFF_SEQ: usize = 36;
const OFF_SLOT_SEQ: usize = 40;
const OFF_SLOT_PTS: usize = 56;
const OFF_PUBLISHED: usize = 88;

const OFF_TRANSPORT: usize = 96;
const OFF_GPU_GENERATION: usize = 100;
const OFF_GPU_LATEST: usize = 104;
const OFF_GPU_SEQ: usize = 108;
const OFF_GPU_W: usize = 112;
const OFF_GPU_H: usize = 116;
const OFF_GPU_FORMAT: usize = 120;
const OFF_GPU_SLOT_SEQ: usize = 128;
const OFF_GPU_SLOT_PTS: usize = 144;
const OFF_GPU_ADAPTER_LUID: usize = 176;
const OFF_GPU_NAMES: usize = 192;
const GPU_NAME_CHARS: usize = 96;

const OFF_READ_SEQ: usize = 800;
const OFF_FRAME_NO: usize = 808;
const OFF_SLOT_FRAME: usize = 816;
const OFF_DROPPED: usize = 792;
const OFF_SLOT_GEOMETRY: usize = 848;
const SLOT_GEOMETRY_BYTES: usize = 12;

const CTL_MAGIC: usize = 0;
const CTL_VERSION: usize = 4;
const CTL_CAPTURE_ACTIVE: usize = 8;
const CTL_CAPTURE_FPS: usize = 12;
const CTL_DEBUG_LOG: usize = 36;
const CTL_LOG_PATH: usize = 944;

const PAGE_READWRITE: u32 = 0x04;
const FILE_MAP_WRITE: u32 = 0x0002;
const FILE_MAP_READ: u32 = 0x0004;
const FILE_MAP_ALL_ACCESS: u32 = 0x000F001F;
const WAIT_OBJECT_0: u32 = 0;
const FILE_APPEND_DATA: u32 = 0x0004;
const FILE_SHARE_ALL: u32 = 0x0000_0007;
const OPEN_ALWAYS: u32 = 4;
const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
const INVALID_HANDLE: *mut c_void = -1isize as *mut c_void;
const DXGI_SHARED_RESOURCE_READ_RAW: u32 = 0x8000_0000;
const DXGI_SHARED_RESOURCE_WRITE_RAW: u32 = 1;

const GL_VIEWPORT: u32 = 0x0BA2;
const GL_BACK: u32 = 0x0405;
const GL_BGRA: u32 = 0x80E1;
const GL_UNSIGNED_BYTE: u32 = 0x1401;
const GL_PACK_ALIGNMENT: u32 = 0x0D05;
const GL_READ_BUFFER: u32 = 0x0C02;
const GL_TEXTURE_2D: u32 = 0x0DE1;
const GL_TEXTURE_BINDING_2D: u32 = 0x8069;
const GL_RGBA8: i32 = 0x8058;
const GL_RGBA: u32 = 0x1908;
const GL_FRAMEBUFFER: u32 = 0x8D40;
const GL_READ_FRAMEBUFFER: u32 = 0x8CA8;
const GL_DRAW_FRAMEBUFFER: u32 = 0x8CA9;
const GL_COLOR_ATTACHMENT0: u32 = 0x8CE0;
const GL_FRAMEBUFFER_BINDING: u32 = 0x8CA6;
const GL_READ_FRAMEBUFFER_BINDING: u32 = 0x8CAA;
const GL_DRAW_FRAMEBUFFER_BINDING: u32 = 0x8CA6;
const GL_COLOR_BUFFER_BIT: u32 = 0x0000_4000;
const GL_NEAREST: u32 = 0x2600;
const GL_PIXEL_PACK_BUFFER: u32 = 0x88EB;
const GL_PIXEL_PACK_BUFFER_BINDING: u32 = 0x88ED;
const GL_STREAM_READ: u32 = 0x88E1;
const GL_READ_ONLY: u32 = 0x88B8;
const GL_SYNC_GPU_COMMANDS_COMPLETE: u32 = 0x9117;
const GL_SYNC_FLUSH_COMMANDS_BIT: u32 = 0x0000_0001;
const GL_ALREADY_SIGNALED: u32 = 0x911A;
const GL_CONDITION_SATISFIED: u32 = 0x911C;
const WGL_ACCESS_WRITE_DISCARD_NV: u32 = 0x0002;

#[repr(C)]
struct LargeInteger {
    quad: i64,
}

type ThreadProc = unsafe extern "system" fn(*mut c_void) -> u32;

#[link(name = "kernel32")]
extern "system" {
    fn QueryPerformanceCounter(value: *mut LargeInteger) -> i32;
    fn QueryPerformanceFrequency(value: *mut LargeInteger) -> i32;
    fn GetCurrentProcessId() -> u32;
    fn OpenFileMappingW(access: u32, inherit: i32, name: *const u16) -> *mut c_void;
    fn CreateFileMappingW(
        file: *mut c_void,
        attrs: *const c_void,
        protect: u32,
        high: u32,
        low: u32,
        name: *const u16,
    ) -> *mut c_void;
    fn MapViewOfFile(
        mapping: *mut c_void,
        access: u32,
        high: u32,
        low: u32,
        bytes: usize,
    ) -> *mut c_void;
    fn UnmapViewOfFile(base: *const c_void) -> i32;
    fn CloseHandle(handle: *mut c_void) -> i32;
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    fn CreateThread(
        attrs: *const c_void,
        stack: usize,
        start: ThreadProc,
        arg: *mut c_void,
        flags: u32,
        id: *mut u32,
    ) -> *mut c_void;
    fn Sleep(ms: u32);
    fn FreeLibraryAndExitThread(module: *mut c_void, code: u32) -> !;
    fn DisableThreadLibraryCalls(module: *mut c_void) -> i32;
    fn CreateEventW(
        attrs: *const c_void,
        manual: i32,
        initial: i32,
        name: *const u16,
    ) -> *mut c_void;
    fn SetEvent(event: *mut c_void) -> i32;
    fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
    fn OutputDebugStringW(text: *const u16);
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *const c_void,
        creation: u32,
        flags: u32,
        template: *mut c_void,
    ) -> *mut c_void;
    fn WriteFile(
        file: *mut c_void,
        data: *const c_void,
        bytes: u32,
        written: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
}

#[link(name = "user32")]
extern "system" {
    fn CreateWindowExW(
        exstyle: u32,
        class: *const u16,
        title: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: *mut c_void,
        menu: *mut c_void,
        instance: *mut c_void,
        param: *mut c_void,
    ) -> *mut c_void;
    fn DestroyWindow(window: *mut c_void) -> i32;
    fn CallNextHookEx(hook: *mut c_void, code: i32, wparam: usize, lparam: isize) -> isize;
    fn UnhookWindowsHookEx(hook: *mut c_void) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleExW(flags: u32, address: *const u16, module: *mut *mut c_void) -> i32;
}

#[repr(C)]
struct HookMsg {
    hwnd: *mut c_void,
    message: u32,
    wparam: usize,
    lparam: isize,
}
const WM_USER_SAFE_INJECT: u32 = 0x0400 + 432;
const GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS: u32 = 0x4;
static SAFE_HOOK_ARMED: AtomicBool = AtomicBool::new(true);

#[no_mangle]
pub unsafe extern "system" fn dummy_debug_proc(code: i32, wparam: usize, lparam: isize) -> isize {
    let msg = lparam as *const HookMsg;
    if code >= 0
        && !msg.is_null()
        && (*msg).message == WM_USER_SAFE_INJECT
        && SAFE_HOOK_ARMED.swap(false, Ordering::AcqRel)
    {
        let mut pinned: *mut c_void = null_mut();
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
            dummy_debug_proc as *const () as usize as *const u16,
            &mut pinned,
        );
        UnhookWindowsHookEx((*msg).lparam as *mut c_void);
    }
    CallNextHookEx(null_mut(), code, wparam, lparam)
}

#[link(name = "opengl32")]
extern "system" {
    fn wglGetCurrentContext() -> *mut c_void;
    fn wglGetProcAddress(name: *const c_char) -> *mut c_void;
    fn glGetIntegerv(name: u32, value: *mut i32);
    fn glGetError() -> u32;
    fn glReadBuffer(buffer: u32);
    fn glDrawBuffer(buffer: u32);
    fn glPixelStorei(name: u32, value: i32);
    fn glReadPixels(x: i32, y: i32, w: i32, h: i32, format: u32, ty: u32, data: *mut c_void);
    fn glGenTextures(count: i32, textures: *mut u32);
    fn glDeleteTextures(count: i32, textures: *const u32);
    fn glBindTexture(target: u32, texture: u32);
    fn glTexImage2D(
        target: u32,
        level: i32,
        internal: i32,
        w: i32,
        h: i32,
        border: i32,
        format: u32,
        ty: u32,
        data: *const c_void,
    );
    fn glGetTexImage(target: u32, level: i32, format: u32, ty: u32, data: *mut c_void);
}

#[link(name = "minhook", kind = "static")]
extern "system" {
    fn MH_Initialize() -> i32;
    fn MH_Uninitialize() -> i32;
    fn MH_CreateHook(target: *mut c_void, detour: *mut c_void, original: *mut *mut c_void) -> i32;
    fn MH_EnableHook(target: *mut c_void) -> i32;
    fn MH_DisableHook(target: *mut c_void) -> i32;
}

static QPC_FREQ: AtomicU64 = AtomicU64::new(1);
static SELF_MODULE: AtomicU64 = AtomicU64::new(0);
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);
static SHUTDOWN_STARTED: AtomicBool = AtomicBool::new(false);
static ACTIVE_DETOURS: AtomicI32 = AtomicI32::new(0);
static IN_GL_PRESENT: AtomicBool = AtomicBool::new(false);
static GL_CLEANED: AtomicBool = AtomicBool::new(true);

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn qpc_ticks() -> i64 {
    let mut value = LargeInteger { quad: 0 };
    unsafe { QueryPerformanceCounter(&mut value) };
    value.quad
}

fn qpc_100ns() -> i64 {
    qpc_ticks().saturating_mul(10_000_000) / QPC_FREQ.load(Ordering::Relaxed).max(1) as i64
}

struct LogFile {
    handle: *mut c_void,
    path: Vec<u16>,
}
unsafe impl Send for LogFile {}
static LOG_FILE: Mutex<LogFile> = Mutex::new(LogFile {
    handle: INVALID_HANDLE,
    path: Vec::new(),
});

fn log(message: &str) {
    let line = format!("[hook:capture:rust] {message}\r\n");
    let text = wide(&line);
    unsafe {
        OutputDebugStringW(text.as_ptr());
        let bytes = line.as_bytes();
        if let Ok(file) = LOG_FILE.try_lock() {
            if file.handle != INVALID_HANDLE && !file.handle.is_null() {
                let mut written = 0;
                WriteFile(
                    file.handle,
                    bytes.as_ptr().cast(),
                    bytes.len().min(u32::MAX as usize) as u32,
                    &mut written,
                    null_mut(),
                );
            }
        }
    }
}

unsafe fn configure_log(base: *const u8) {
    let enabled = read_u32(base, CTL_DEBUG_LOG) != 0;
    let source = base.add(CTL_LOG_PATH).cast::<u16>();
    let mut path = Vec::with_capacity(260);
    for index in 0..260 {
        let value = std::ptr::read_volatile(source.add(index));
        path.push(value);
        if value == 0 {
            break;
        }
    }
    let mut file = LOG_FILE.lock().unwrap_or_else(|e| e.into_inner());
    if !enabled || path.len() <= 1 {
        if file.handle != INVALID_HANDLE && !file.handle.is_null() {
            CloseHandle(file.handle);
        }
        file.handle = INVALID_HANDLE;
        file.path.clear();
        return;
    }
    if file.handle != INVALID_HANDLE && file.path == path {
        return;
    }
    if file.handle != INVALID_HANDLE && !file.handle.is_null() {
        CloseHandle(file.handle);
    }
    let handle = CreateFileW(
        path.as_ptr(),
        FILE_APPEND_DATA,
        FILE_SHARE_ALL,
        null(),
        OPEN_ALWAYS,
        FILE_ATTRIBUTE_NORMAL,
        null_mut(),
    );
    file.handle = handle;
    file.path = path;
}

#[derive(Clone, Copy)]
struct Control {
    active: bool,
    fps: u32,
}

struct Mapping {
    handle: *mut c_void,
    base: *mut u8,
}

unsafe impl Send for Mapping {}

impl Drop for Mapping {
    fn drop(&mut self) {
        unsafe {
            if !self.base.is_null() {
                UnmapViewOfFile(self.base.cast());
            }
            if !self.handle.is_null() {
                CloseHandle(self.handle);
            }
        }
    }
}

struct Protocol {
    control: Option<Mapping>,
    ring: Option<Mapping>,
    retry_at: i64,
    last_size: (u32, u32),
    clock_start: i64,
    last_frame_no: u64,
    last_fps: u32,
}

unsafe impl Send for Protocol {}

impl Protocol {
    const fn new() -> Self {
        Self {
            control: None,
            ring: None,
            retry_at: 0,
            last_size: (0, 0),
            clock_start: 0,
            last_frame_no: 0,
            last_fps: 0,
        }
    }

    unsafe fn ensure_control(&mut self) -> bool {
        if self.control.is_some() {
            return true;
        }
        let now = qpc_100ns();
        if now < self.retry_at {
            return false;
        }
        self.retry_at = now + 10_000_000;
        let name = wide("Local\\InstantReplay.Control.v1");
        let handle = OpenFileMappingW(FILE_MAP_READ | FILE_MAP_WRITE, 0, name.as_ptr());
        if handle.is_null() {
            return false;
        }
        let base = MapViewOfFile(handle, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, 0).cast::<u8>();
        if base.is_null() {
            CloseHandle(handle);
            return false;
        }
        self.control = Some(Mapping { handle, base });
        true
    }

    unsafe fn read_control(&mut self) -> Option<Control> {
        if !self.ensure_control() {
            return None;
        }
        let base = self.control.as_ref()?.base;
        if read_u32(base, CTL_MAGIC) != MAGIC || read_u32(base, CTL_VERSION) != CONTROL_VERSION {
            return None;
        }
        configure_log(base);
        Some(Control {
            active: read_u32(base, CTL_CAPTURE_ACTIVE) != 0,
            fps: read_u32(base, CTL_CAPTURE_FPS).clamp(15, 240),
        })
    }

    unsafe fn ensure_ring(&mut self) -> bool {
        if self.ring.is_some() {
            return true;
        }
        let name = wide("Local\\InstantReplay.Frames.v1");
        let handle = CreateFileMappingW(
            INVALID_HANDLE,
            null(),
            PAGE_READWRITE,
            0,
            RING_BYTES as u32,
            name.as_ptr(),
        );
        if handle.is_null() {
            return false;
        }
        let base = MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, RING_BYTES).cast::<u8>();
        if base.is_null() {
            CloseHandle(handle);
            return false;
        }
        std::ptr::write_bytes(base, 0, HEADER_BYTES);
        write_u32(base, OFF_MAGIC, MAGIC);
        write_u32(base, OFF_VERSION, VERSION);
        write_u32(base, OFF_SLOTS, SLOTS as u32);
        write_u32(base, OFF_PID, GetCurrentProcessId());
        write_u32(base, OFF_ACTIVE, 1);
        self.ring = Some(Mapping { handle, base });
        log("frame transport created");
        true
    }

    unsafe fn base(&mut self) -> Option<*mut u8> {
        self.ensure_ring().then(|| self.ring.as_ref().unwrap().base)
    }

    unsafe fn next_frame(&mut self, control: Control) -> Option<u64> {
        if !control.active {
            self.clock_start = 0;
            self.last_frame_no = 0;
            self.last_fps = 0;
            return None;
        }
        let now = qpc_100ns();
        if self.clock_start == 0 || self.last_fps != control.fps {
            self.clock_start = now;
            self.last_frame_no = 0;
            self.last_fps = control.fps;
        }
        let elapsed = (now - self.clock_start).max(0) as u64;
        let frame_no = elapsed.saturating_mul(control.fps as u64) / 10_000_000;
        if frame_no <= self.last_frame_no {
            return None;
        }
        self.last_frame_no = frame_no;
        Some(frame_no)
    }

    unsafe fn publish_cpu(
        &mut self,
        pixels: &[u8],
        w: u32,
        h: u32,
        bottom_up: bool,
        frame_no: u64,
        generation: u64,
    ) {
        let row = w as usize * 4;
        let bytes = row.saturating_mul(h as usize);
        if bytes == 0 || bytes > MAX_SLOT_BYTES || pixels.len() < bytes {
            return;
        }
        let Some(base) = self.base() else { return };
        let deadline = qpc_100ns() + 1_000_000;
        loop {
            let seq = read_u32(base, OFF_SEQ);
            let read = read_u32(base, OFF_READ_SEQ);
            if seq.wrapping_sub(read) < SLOTS as u32 {
                break;
            }
            if qpc_100ns() >= deadline {
                atomic_u32(base, OFF_DROPPED).fetch_add(1, Ordering::Relaxed);
                return;
            }
            Sleep(1);
        }
        let seq = read_u32(base, OFF_SEQ);
        let slot = seq as usize % SLOTS;
        let dst = base.add(HEADER_BYTES + slot * MAX_SLOT_BYTES);
        if bottom_up {
            for y in 0..h as usize {
                std::ptr::copy_nonoverlapping(
                    pixels.as_ptr().add((h as usize - 1 - y) * row),
                    dst.add(y * row),
                    row,
                );
            }
        } else {
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), dst, bytes);
        }
        if generation != CPU_FRAME_GENERATION.load(Ordering::Acquire) {
            return;
        }
        write_u32(base, OFF_W, w);
        write_u32(base, OFF_H, h);
        write_u32(base, OFF_STRIDE, row as u32);
        let geometry = OFF_SLOT_GEOMETRY + slot * SLOT_GEOMETRY_BYTES;
        write_u32(base, geometry, w);
        write_u32(base, geometry + 4, h);
        write_u32(base, geometry + 8, row as u32);
        write_i64(base, OFF_SLOT_PTS + slot * 8, qpc_ticks());
        write_u64(base, OFF_SLOT_FRAME + slot * 8, frame_no);
        write_u64(base, OFF_FRAME_NO, frame_no);
        fence();
        atomic_u32(base, OFF_SLOT_SEQ + slot * 4).fetch_add(1, Ordering::Release);
        write_u32(base, OFF_LATEST, slot as u32);
        atomic_u32(base, OFF_SEQ).fetch_add(1, Ordering::Release);
        atomic_u32(base, OFF_PUBLISHED).fetch_add(1, Ordering::Relaxed);
        write_u32(base, OFF_TRANSPORT, 0);
    }

    unsafe fn configure_gpu(
        &mut self,
        w: u32,
        h: u32,
        adapter_luid: i64,
        names: &[Vec<u16>],
    ) -> Option<u32> {
        let base = self.base()?;
        let generation = read_u32(base, OFF_GPU_GENERATION).wrapping_add(1).max(1);
        write_u32(base, OFF_W, w);
        write_u32(base, OFF_H, h);
        write_u32(base, OFF_STRIDE, w.saturating_mul(4));
        write_u32(base, OFF_GPU_W, w);
        write_u32(base, OFF_GPU_H, h);
        write_u32(base, OFF_GPU_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM.0 as u32);
        write_i64(base, OFF_GPU_ADAPTER_LUID, adapter_luid);
        for (slot, name) in names.iter().enumerate().take(SLOTS) {
            let out = base
                .add(OFF_GPU_NAMES + slot * GPU_NAME_CHARS * 2)
                .cast::<u16>();
            std::ptr::write_bytes(out, 0, GPU_NAME_CHARS);
            let count = name.len().saturating_sub(1).min(GPU_NAME_CHARS - 1);
            std::ptr::copy_nonoverlapping(name.as_ptr(), out, count);
        }
        fence();
        write_u32(base, OFF_GPU_GENERATION, generation);
        write_u32(base, OFF_TRANSPORT, 1);
        Some(generation)
    }

    unsafe fn publish_gpu(&mut self, slot: usize, pts: i64) {
        let Some(base) = self.base() else { return };
        write_i64(base, OFF_GPU_SLOT_PTS + slot * 8, pts);
        atomic_u32(base, OFF_GPU_SLOT_SEQ + slot * 4).fetch_add(1, Ordering::Release);
        atomic_u32(base, OFF_GPU_LATEST).store(slot as u32, Ordering::Release);
        atomic_u32(base, OFF_GPU_SEQ).fetch_add(1, Ordering::Release);
        atomic_u32(base, OFF_PUBLISHED).fetch_add(1, Ordering::Relaxed);
        write_u32(base, OFF_TRANSPORT, 1);
    }

    unsafe fn shutdown(&mut self) {
        if let Some(ring) = &self.ring {
            write_u32(ring.base, OFF_ACTIVE, 0);
        }
        self.ring = None;
        self.control = None;
    }
}

unsafe fn read_u32(base: *const u8, offset: usize) -> u32 {
    std::ptr::read_volatile(base.add(offset).cast::<u32>())
}
unsafe fn write_u32(base: *mut u8, offset: usize, value: u32) {
    std::ptr::write_volatile(base.add(offset).cast::<u32>(), value)
}
unsafe fn write_i64(base: *mut u8, offset: usize, value: i64) {
    std::ptr::write_volatile(base.add(offset).cast::<i64>(), value)
}
unsafe fn write_u64(base: *mut u8, offset: usize, value: u64) {
    std::ptr::write_volatile(base.add(offset).cast::<u64>(), value)
}
unsafe fn atomic_u32<'a>(base: *mut u8, offset: usize) -> &'a AtomicU32 {
    &*base.add(offset).cast::<AtomicU32>()
}
fn fence() {
    std::sync::atomic::fence(Ordering::SeqCst)
}

static PROTOCOL: Mutex<Protocol> = Mutex::new(Protocol::new());
static CPU_FRAME_GENERATION: AtomicU64 = AtomicU64::new(1);

struct CpuFrame {
    pixels: Vec<u8>,
    w: u32,
    h: u32,
    bottom_up: bool,
    frame_no: u64,
    generation: u64,
}

static CPU_PUBLISHER: Mutex<Option<std::sync::mpsc::SyncSender<CpuFrame>>> = Mutex::new(None);
static CPU_PUBLISH_THREAD: Mutex<Option<std::thread::JoinHandle<()>>> = Mutex::new(None);

fn start_cpu_publisher() {
    let (sender, receiver) = std::sync::mpsc::sync_channel::<CpuFrame>(3);
    if let Ok(mut slot) = CPU_PUBLISHER.lock() {
        *slot = Some(sender);
    }
    let worker = std::thread::Builder::new()
        .name("ir-hook-publish".into())
        .spawn(move || {
            let mut protocol = Protocol::new();
            while !SHUTTING_DOWN.load(Ordering::Acquire) {
                match receiver.recv_timeout(std::time::Duration::from_millis(20)) {
                    Ok(frame) => unsafe {
                        if frame.generation == CPU_FRAME_GENERATION.load(Ordering::Acquire) {
                            protocol.publish_cpu(
                                &frame.pixels,
                                frame.w,
                                frame.h,
                                frame.bottom_up,
                                frame.frame_no,
                                frame.generation,
                            );
                        }
                    },
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
            unsafe { protocol.shutdown() };
        });
    if let Ok(mut slot) = CPU_PUBLISH_THREAD.lock() {
        *slot = worker.ok();
    }
}

fn queue_cpu(pixels: &[u8], w: u32, h: u32, bottom_up: bool, frame_no: u64) {
    let frame = CpuFrame {
        pixels: pixels.to_vec(),
        w,
        h,
        bottom_up,
        frame_no,
        generation: CPU_FRAME_GENERATION.load(Ordering::Acquire),
    };
    if let Ok(sender) = CPU_PUBLISHER.try_lock() {
        if let Some(sender) = sender.as_ref() {
            let _ = sender.try_send(frame);
        }
    }
}

struct DetourGuard;
impl DetourGuard {
    fn enter() -> Self {
        ACTIVE_DETOURS.fetch_add(1, Ordering::AcqRel);
        Self
    }
}
impl Drop for DetourGuard {
    fn drop(&mut self) {
        ACTIVE_DETOURS.fetch_sub(1, Ordering::AcqRel);
    }
}

fn with_protocol<R>(f: impl FnOnce(&mut Protocol) -> R) -> Option<R> {
    match PROTOCOL.try_lock() {
        Ok(mut value) => Some(f(&mut value)),
        Err(TryLockError::Poisoned(value)) => Some(f(&mut value.into_inner())),
        Err(TryLockError::WouldBlock) => None,
    }
}

#[allow(dead_code)]
fn capture_due(last: &mut i64, control: Control) -> bool {
    if !control.active {
        *last = 0;
        return false;
    }
    let now = qpc_100ns();
    let interval = 10_000_000 / control.fps.max(1) as i64;
    if *last != 0 && now - *last < interval {
        return false;
    }
    *last = now;
    true
}

type OverlayGl = unsafe extern "system" fn();
type OverlayD3D9 = unsafe extern "system" fn(*mut c_void);
type OverlayD3D11 = unsafe extern "system" fn(*mut c_void);
type OverlayD3D12 = unsafe extern "system" fn(*mut c_void, *mut c_void);

static OVERLAY_GL: Mutex<Option<OverlayGl>> = Mutex::new(None);
static OVERLAY_D3D9: Mutex<Option<OverlayD3D9>> = Mutex::new(None);
static OVERLAY_D3D11: Mutex<Option<OverlayD3D11>> = Mutex::new(None);
static OVERLAY_D3D12: Mutex<Option<OverlayD3D12>> = Mutex::new(None);
static OVERLAY_RETRY: AtomicU64 = AtomicU64::new(0);

unsafe fn resolve_overlay() {
    let now = qpc_100ns() as u64;
    if now < OVERLAY_RETRY.load(Ordering::Relaxed) {
        return;
    }
    OVERLAY_RETRY.store(now + 10_000_000, Ordering::Relaxed);
    let module = GetModuleHandleW(wide("overlay_hook.dll").as_ptr());
    if module.is_null() {
        return;
    }
    let gl = GetProcAddress(module, b"IR_OverlayPresent\0".as_ptr().cast());
    let d9 = GetProcAddress(module, b"IR_OverlayPresentD3D9\0".as_ptr().cast());
    let d11 = GetProcAddress(module, b"IR_OverlayPresentD3D11\0".as_ptr().cast());
    let d12 = GetProcAddress(module, b"IR_OverlayPresentD3D12\0".as_ptr().cast());
    if !gl.is_null() {
        *OVERLAY_GL.lock().unwrap_or_else(|e| e.into_inner()) = Some(transmute(gl));
    }
    if !d9.is_null() {
        *OVERLAY_D3D9.lock().unwrap_or_else(|e| e.into_inner()) = Some(transmute(d9));
    }
    if !d11.is_null() {
        *OVERLAY_D3D11.lock().unwrap_or_else(|e| e.into_inner()) = Some(transmute(d11));
    }
    if !d12.is_null() {
        *OVERLAY_D3D12.lock().unwrap_or_else(|e| e.into_inner()) = Some(transmute(d12));
    }
}

unsafe fn call_overlay_gl() {
    resolve_overlay();
    if let Some(function) = *OVERLAY_GL.lock().unwrap_or_else(|e| e.into_inner()) {
        function();
    }
}

unsafe fn call_overlay_d3d11(swap: *mut c_void) {
    resolve_overlay();
    if let Some(function) = *OVERLAY_D3D11.lock().unwrap_or_else(|e| e.into_inner()) {
        function(swap);
    }
}

unsafe fn call_overlay_d3d9(device: *mut c_void) {
    resolve_overlay();
    if let Some(function) = *OVERLAY_D3D9.lock().unwrap_or_else(|e| e.into_inner()) {
        function(device);
    }
}

unsafe fn call_overlay_d3d12(swap: *mut c_void, queue: *mut c_void) {
    resolve_overlay();
    if let Some(function) = *OVERLAY_D3D12.lock().unwrap_or_else(|e| e.into_inner()) {
        function(swap, queue);
    }
}

struct SharedSlot {
    texture: ID3D11Texture2D,
    mutex: IDXGIKeyedMutex,
    handle: HANDLE,
    name: Vec<u16>,
}

unsafe impl Send for SharedSlot {}

impl Drop for SharedSlot {
    fn drop(&mut self) {
        unsafe {
            if !self.handle.is_invalid() {
                CloseHandle(self.handle.0);
            }
        }
    }
}

struct SharedTextures {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    slots: Vec<SharedSlot>,
    w: u32,
    h: u32,
    next: usize,
    generation: u32,
}

unsafe impl Send for SharedTextures {}

#[allow(dead_code)]
impl SharedTextures {
    unsafe fn create(device: &ID3D11Device, w: u32, h: u32) -> Result<Self, String> {
        let context = device
            .GetImmediateContext()
            .map_err(|e| format!("D3D11 immediate context unavailable: {:#x}", e.code().0))?;
        let pid = GetCurrentProcessId();
        let nonce = qpc_ticks() as u64;
        let desc = D3D11_TEXTURE2D_DESC {
            Width: w,
            Height: h,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
            CPUAccessFlags: 0,
            MiscFlags: (D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX.0
                | D3D11_RESOURCE_MISC_SHARED_NTHANDLE.0) as u32,
        };
        let mut slots = Vec::with_capacity(SLOTS);
        for index in 0..SLOTS {
            let mut texture = None;
            device
                .CreateTexture2D(&desc, None, Some(&mut texture))
                .map_err(|e| format!("CreateTexture2D failed: {:#x}", e.code().0))?;
            let texture = texture.ok_or("CreateTexture2D returned null")?;
            let keyed: IDXGIKeyedMutex = texture
                .cast()
                .map_err(|e| format!("IDXGIKeyedMutex unavailable: {:#x}", e.code().0))?;
            let resource: IDXGIResource1 = texture
                .cast()
                .map_err(|e| format!("IDXGIResource1 unavailable: {:#x}", e.code().0))?;
            let name = wide(&format!(
                "Local\\InstantReplay.Capture.{}.{}.{}",
                pid, nonce, index
            ));
            let handle = resource
                .CreateSharedHandle(
                    None,
                    DXGI_SHARED_RESOURCE_READ_RAW | DXGI_SHARED_RESOURCE_WRITE_RAW,
                    PCWSTR(name.as_ptr()),
                )
                .map_err(|e| format!("CreateSharedHandle failed: {:#x}", e.code().0))?;
            slots.push(SharedSlot {
                texture,
                mutex: keyed,
                handle,
                name,
            });
        }
        let names: Vec<Vec<u16>> = slots.iter().map(|s| s.name.clone()).collect();
        let adapter_luid = device
            .cast::<IDXGIDevice>()
            .and_then(|dxgi| dxgi.GetAdapter())
            .and_then(|adapter| adapter.GetDesc())
            .map(|desc| {
                std::ptr::read_unaligned(
                    (&desc.AdapterLuid as *const windows::Win32::Foundation::LUID).cast::<i64>(),
                )
            })
            .unwrap_or(0);
        let generation =
            with_protocol(|protocol| protocol.configure_gpu(w, h, adapter_luid, &names))
                .flatten()
                .ok_or("frame protocol unavailable")?;
        log(&format!(
            "shared D3D11 transport initialized ({w}x{h}, generation {generation})"
        ));
        Ok(Self {
            device: device.clone(),
            context,
            slots,
            w,
            h,
            next: 0,
            generation,
        })
    }

    unsafe fn copy_from(&mut self, source: &ID3D11Texture2D) -> bool {
        for _ in 0..self.slots.len() {
            let index = self.next;
            self.next = (self.next + 1) % self.slots.len();
            let slot = &self.slots[index];
            if slot.mutex.AcquireSync(0, 0).is_err() {
                continue;
            }
            self.context.CopyResource(&slot.texture, source);
            let _ = slot.mutex.ReleaseSync(1);
            with_protocol(|protocol| protocol.publish_gpu(index, qpc_ticks()));
            return true;
        }
        false
    }
}

struct D3D11Capture {
    swap: usize,
    format: DXGI_FORMAT,
    w: u32,
    h: u32,
    stage: ID3D11Texture2D,
    context: ID3D11DeviceContext,
    pixels: Vec<u8>,
}

unsafe impl Send for D3D11Capture {}
static D3D11_CAPTURE: Mutex<Option<D3D11Capture>> = Mutex::new(None);

unsafe fn capture_d3d11(swap_raw: *mut c_void) {
    let control = with_protocol(|protocol| protocol.read_control()).flatten();
    let Some(control) = control else { return };
    let swap = ManuallyDrop::new(IDXGISwapChain::from_raw(swap_raw));
    let Ok(device) = swap.GetDevice::<ID3D11Device>() else {
        return;
    };
    let Ok(back) = swap.GetBuffer::<ID3D11Texture2D>(0) else {
        return;
    };
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    back.GetDesc(&mut desc);
    if desc.Width < 16
        || desc.Height < 16
        || desc.Width as usize > MAX_W
        || desc.Height as usize > MAX_H
        || desc.SampleDesc.Count != 1
        || !matches!(
            desc.Format,
            DXGI_FORMAT_B8G8R8A8_UNORM
                | DXGI_FORMAT_B8G8R8A8_UNORM_SRGB
                | DXGI_FORMAT_R8G8B8A8_UNORM
                | DXGI_FORMAT_R8G8B8A8_UNORM_SRGB
        )
    {
        return;
    }
    let mut state = match D3D11_CAPTURE.try_lock() {
        Ok(value) => value,
        Err(_) => return,
    };
    let rebuild = state.as_ref().is_none_or(|s| {
        s.swap != swap_raw as usize
            || s.w != desc.Width
            || s.h != desc.Height
            || s.format != desc.Format
    });
    if rebuild {
        let context = match device.GetImmediateContext() {
            Ok(context) => context,
            Err(_) => return,
        };
        let staged = D3D11_TEXTURE2D_DESC {
            Width: desc.Width,
            Height: desc.Height,
            MipLevels: 1,
            ArraySize: 1,
            Format: desc.Format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        let mut stage = None;
        if device
            .CreateTexture2D(&staged, None, Some(&mut stage))
            .is_err()
        {
            return;
        }
        let Some(stage) = stage else { return };
        log(&format!(
            "D3D11 CPU readback initialized ({}x{})",
            desc.Width, desc.Height
        ));
        *state = Some(D3D11Capture {
            swap: swap_raw as usize,
            format: desc.Format,
            w: desc.Width,
            h: desc.Height,
            stage,
            context,
            pixels: Vec::new(),
        });
    }
    let Some(state) = state.as_mut() else { return };
    let Some(frame_no) = with_protocol(|protocol| protocol.next_frame(control)).flatten() else {
        return;
    };
    state.context.CopyResource(&state.stage, &back);
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    if state
        .context
        .Map(&state.stage, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
        .is_err()
    {
        return;
    }
    let row = state.w as usize * 4;
    let pitch = mapped.RowPitch as usize;
    state.pixels.resize(row * state.h as usize, 0);
    if !mapped.pData.is_null() && pitch >= row {
        for y in 0..state.h as usize {
            std::ptr::copy_nonoverlapping(
                (mapped.pData as *const u8).add(y * pitch),
                state.pixels.as_mut_ptr().add(y * row),
                row,
            );
        }
    }
    state.context.Unmap(&state.stage, 0);
    if matches!(
        state.format,
        DXGI_FORMAT_R8G8B8A8_UNORM | DXGI_FORMAT_R8G8B8A8_UNORM_SRGB
    ) {
        for pixel in state.pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
    }
    queue_cpu(&state.pixels, state.w, state.h, false, frame_no);
}

static D3D12_QUEUE: Mutex<Option<ID3D12CommandQueue>> = Mutex::new(None);
type D3D12Execute = unsafe extern "system" fn(*mut c_void, u32, *const *mut c_void);
static mut D3D12_EXECUTE_ORIGINAL: Option<D3D12Execute> = None;

unsafe extern "system" fn d3d12_execute_detour(
    queue_raw: *mut c_void,
    count: u32,
    lists: *const *mut c_void,
) {
    let _guard = DetourGuard::enter();
    if let Some(original) = D3D12_EXECUTE_ORIGINAL {
        original(queue_raw, count, lists);
    }
    let queue = ManuallyDrop::new(ID3D12CommandQueue::from_raw(queue_raw));
    if queue.GetDesc().Type != D3D12_COMMAND_LIST_TYPE_DIRECT {
        return;
    }
    if let Ok(mut current) = D3D12_QUEUE.try_lock() {
        *current = Some((*queue).clone());
    }
}

unsafe fn install_d3d12_queue_hook() -> bool {
    if GetModuleHandleW(wide("d3d12.dll").as_ptr()).is_null() {
        return false;
    }
    let mut device = None;
    if D3D12CreateDevice::<_, ID3D12Device>(None, D3D_FEATURE_LEVEL(0xb000), &mut device).is_err() {
        return false;
    }
    let Some(device) = device else { return false };
    let desc = D3D12_COMMAND_QUEUE_DESC {
        Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
        Priority: 0,
        Flags: D3D12_COMMAND_QUEUE_FLAG_NONE,
        NodeMask: 0,
    };
    let Ok(queue) = device.CreateCommandQueue::<ID3D12CommandQueue>(&desc) else {
        return false;
    };
    let target = vtable_entry(queue.as_raw(), 10);
    let mut original = null_mut();
    if MH_CreateHook(
        target,
        d3d12_execute_detour as *const () as *mut c_void,
        &mut original,
    ) != 0
    {
        return false;
    }
    D3D12_EXECUTE_ORIGINAL = Some(transmute(original));
    true
}

struct D3D12Capture {
    swap: usize,
    queue: usize,
    w: u32,
    h: u32,
    format: DXGI_FORMAT,
    allocator: ID3D12CommandAllocator,
    list: ID3D12GraphicsCommandList,
    readback: ID3D12Resource,
    fence: ID3D12Fence,
    event: HANDLE,
    fence_value: u64,
    footprint: D3D12_PLACED_SUBRESOURCE_FOOTPRINT,
    bytes: u64,
    pixels: Vec<u8>,
}

unsafe impl Send for D3D12Capture {}
impl Drop for D3D12Capture {
    fn drop(&mut self) {
        unsafe {
            if !self.event.is_invalid() {
                CloseHandle(self.event.0);
            }
        }
    }
}
static D3D12_CAPTURE: Mutex<Option<D3D12Capture>> = Mutex::new(None);

unsafe fn d3d12_queue_for(device: &ID3D12Device) -> Option<ID3D12CommandQueue> {
    let queue = D3D12_QUEUE.lock().ok()?.as_ref()?.clone();
    let mut queue_device: Option<ID3D12Device> = None;
    queue.GetDevice(&mut queue_device).ok()?;
    (queue_device?.as_raw() == device.as_raw()).then_some(queue)
}

unsafe fn create_d3d12_capture(
    swap: *mut c_void,
    device: &ID3D12Device,
    queue: &ID3D12CommandQueue,
    desc: &D3D12_RESOURCE_DESC,
) -> Result<D3D12Capture, String> {
    let mut footprint = D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default();
    let mut rows = 0;
    let mut row_bytes = 0;
    let mut bytes = 0;
    device.GetCopyableFootprints(
        desc,
        0,
        1,
        0,
        Some(&mut footprint),
        Some(&mut rows),
        Some(&mut row_bytes),
        Some(&mut bytes),
    );
    let heap = D3D12_HEAP_PROPERTIES {
        Type: D3D12_HEAP_TYPE_READBACK,
        CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
        MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
        CreationNodeMask: 1,
        VisibleNodeMask: 1,
    };
    let buffer_desc = D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
        Alignment: 0,
        Width: bytes,
        Height: 1,
        DepthOrArraySize: 1,
        MipLevels: 1,
        Format: DXGI_FORMAT_UNKNOWN,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        Flags: D3D12_RESOURCE_FLAG_NONE,
    };
    let mut readback = None;
    device
        .CreateCommittedResource::<ID3D12Resource>(
            &heap,
            D3D12_HEAP_FLAG_NONE,
            &buffer_desc,
            D3D12_RESOURCE_STATE_COPY_DEST,
            None,
            &mut readback,
        )
        .map_err(|e| format!("D3D12 readback resource failed: {:#x}", e.code().0))?;
    let allocator: ID3D12CommandAllocator = device
        .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
        .map_err(|e| format!("D3D12 allocator failed: {:#x}", e.code().0))?;
    let list: ID3D12GraphicsCommandList = device
        .CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)
        .map_err(|e| format!("D3D12 command list failed: {:#x}", e.code().0))?;
    list.Close().ok();
    let fence: ID3D12Fence = device
        .CreateFence(0, D3D12_FENCE_FLAG_NONE)
        .map_err(|e| format!("D3D12 fence failed: {:#x}", e.code().0))?;
    let event = HANDLE(CreateEventW(null(), 0, 0, null()));
    if event.is_invalid() {
        return Err("D3D12 fence event failed".into());
    }
    log(&format!(
        "D3D12 CPU fallback initialized ({}x{})",
        desc.Width, desc.Height
    ));
    Ok(D3D12Capture {
        swap: swap as usize,
        queue: queue.as_raw() as usize,
        w: desc.Width as u32,
        h: desc.Height,
        format: desc.Format,
        allocator,
        list,
        readback: readback.ok_or("D3D12 readback resource missing")?,
        fence,
        event,
        fence_value: 0,
        footprint,
        bytes,
        pixels: Vec::new(),
    })
}

unsafe fn capture_d3d12(swap_raw: *mut c_void, device: &ID3D12Device, queue: &ID3D12CommandQueue) {
    let control = with_protocol(|protocol| protocol.read_control()).flatten();
    let Some(control) = control else { return };
    let swap = ManuallyDrop::new(IDXGISwapChain::from_raw(swap_raw));
    let Ok(swap3) = swap.cast::<IDXGISwapChain3>() else {
        return;
    };
    let Ok(back) = swap3.GetBuffer::<ID3D12Resource>(swap3.GetCurrentBackBufferIndex()) else {
        return;
    };
    let desc = back.GetDesc();
    if desc.Dimension != D3D12_RESOURCE_DIMENSION_TEXTURE2D
        || desc.SampleDesc.Count != 1
        || desc.Width as usize > MAX_W
        || desc.Height as usize > MAX_H
        || !matches!(
            desc.Format,
            DXGI_FORMAT_B8G8R8A8_UNORM
                | DXGI_FORMAT_B8G8R8A8_UNORM_SRGB
                | DXGI_FORMAT_R8G8B8A8_UNORM
                | DXGI_FORMAT_R8G8B8A8_UNORM_SRGB
        )
    {
        return;
    }
    let mut capture = match D3D12_CAPTURE.try_lock() {
        Ok(value) => value,
        Err(_) => return,
    };
    let rebuild = capture.as_ref().is_none_or(|state| {
        state.swap != swap_raw as usize
            || state.queue != queue.as_raw() as usize
            || state.w != desc.Width as u32
            || state.h != desc.Height
            || state.format != desc.Format
    });
    if rebuild {
        match create_d3d12_capture(swap_raw, device, queue, &desc) {
            Ok(value) => *capture = Some(value),
            Err(error) => {
                log(&error);
                return;
            }
        }
    }
    let Some(state) = capture.as_mut() else {
        return;
    };
    let Some(frame_no) = with_protocol(|protocol| protocol.next_frame(control)).flatten() else {
        return;
    };
    if state.allocator.Reset().is_err() || state.list.Reset(&state.allocator, None).is_err() {
        return;
    }
    let borrowed = ManuallyDrop::new(ID3D12Resource::from_raw(back.as_raw()));
    let transition = |before, after| D3D12_RESOURCE_BARRIER {
        Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
        Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        Anonymous: D3D12_RESOURCE_BARRIER_0 {
            Transition: ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                pResource: ManuallyDrop::new(Some(ID3D12Resource::from_raw(borrowed.as_raw()))),
                Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                StateBefore: before,
                StateAfter: after,
            }),
        },
    };
    let before = transition(
        D3D12_RESOURCE_STATE_PRESENT,
        D3D12_RESOURCE_STATE_COPY_SOURCE,
    );
    let after = transition(
        D3D12_RESOURCE_STATE_COPY_SOURCE,
        D3D12_RESOURCE_STATE_PRESENT,
    );
    state.list.ResourceBarrier(&[before]);
    let destination = D3D12_TEXTURE_COPY_LOCATION {
        pResource: ManuallyDrop::new(Some(ID3D12Resource::from_raw(state.readback.as_raw()))),
        Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
        Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
            PlacedFootprint: state.footprint,
        },
    };
    let source = D3D12_TEXTURE_COPY_LOCATION {
        pResource: ManuallyDrop::new(Some(ID3D12Resource::from_raw(back.as_raw()))),
        Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
        Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
            SubresourceIndex: 0,
        },
    };
    state
        .list
        .CopyTextureRegion(&destination, 0, 0, 0, &source, None);
    state.list.ResourceBarrier(&[after]);
    if state.list.Close().is_err() {
        return;
    }
    let base: ID3D12CommandList = state.list.cast().unwrap();
    queue.ExecuteCommandLists(&[Some(base)]);
    state.fence_value += 1;
    if queue.Signal(&state.fence, state.fence_value).is_err() {
        return;
    }
    if state.fence.GetCompletedValue() < state.fence_value {
        if state
            .fence
            .SetEventOnCompletion(state.fence_value, state.event)
            .is_err()
            || WaitForSingleObject(state.event.0, 2000) != WAIT_OBJECT_0
        {
            return;
        }
    }
    let mut mapped = null_mut();
    let range = D3D12_RANGE {
        Begin: 0,
        End: state.bytes as usize,
    };
    if state
        .readback
        .Map(0, Some(&range), Some(&mut mapped))
        .is_err()
        || mapped.is_null()
    {
        return;
    }
    let row = state.w as usize * 4;
    let pitch = state.footprint.Footprint.RowPitch as usize;
    state.pixels.resize(row * state.h as usize, 0);
    for y in 0..state.h as usize {
        std::ptr::copy_nonoverlapping(
            (mapped as *const u8).add(state.footprint.Offset as usize + y * pitch),
            state.pixels.as_mut_ptr().add(y * row),
            row,
        );
    }
    if matches!(
        state.format,
        DXGI_FORMAT_R8G8B8A8_UNORM | DXGI_FORMAT_R8G8B8A8_UNORM_SRGB
    ) {
        for pixel in state.pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
    }
    state
        .readback
        .Unmap(0, Some(&D3D12_RANGE { Begin: 0, End: 0 }));
    queue_cpu(&state.pixels, state.w, state.h, false, frame_no);
}

type DxgiPresent = unsafe extern "system" fn(*mut c_void, u32, u32) -> i32;
type DxgiResize = unsafe extern "system" fn(*mut c_void, u32, u32, u32, i32, u32) -> i32;
static mut DXGI_PRESENT_ORIGINAL: Option<DxgiPresent> = None;
static mut DXGI_RESIZE_ORIGINAL: Option<DxgiResize> = None;

unsafe extern "system" fn dxgi_present_detour(swap: *mut c_void, sync: u32, flags: u32) -> i32 {
    let _guard = DetourGuard::enter();
    if !SHUTTING_DOWN.load(Ordering::Acquire) {
        let _ = std::panic::catch_unwind(|| unsafe {
            let borrowed = ManuallyDrop::new(IDXGISwapChain::from_raw(swap));
            if let Ok(device) = borrowed.GetDevice::<ID3D12Device>() {
                if let Some(queue) = d3d12_queue_for(&device) {
                    capture_d3d12(swap, &device, &queue);
                    call_overlay_d3d12(swap, queue.as_raw());
                }
            } else {
                capture_d3d11(swap);
                call_overlay_d3d11(swap);
            }
        });
    }
    match DXGI_PRESENT_ORIGINAL {
        Some(original) => original(swap, sync, flags),
        None => 0x8000_4005u32 as i32,
    }
}

unsafe extern "system" fn dxgi_resize_detour(
    swap: *mut c_void,
    count: u32,
    w: u32,
    h: u32,
    format: i32,
    flags: u32,
) -> i32 {
    let _guard = DetourGuard::enter();
    if let Ok(mut state) = D3D11_CAPTURE.try_lock() {
        *state = None;
    }
    if let Ok(mut state) = D3D12_CAPTURE.try_lock() {
        *state = None;
    }
    match DXGI_RESIZE_ORIGINAL {
        Some(original) => original(swap, count, w, h, format, flags),
        None => 0x8000_4005u32 as i32,
    }
}

unsafe fn vtable_entry(object: *mut c_void, index: usize) -> *mut c_void {
    let table = *(object.cast::<*const *mut c_void>());
    *table.add(index)
}

unsafe fn install_dxgi_hooks() -> bool {
    let class = wide("STATIC");
    let title = wide("InstantReplayHookProbe");
    let window = CreateWindowExW(
        0,
        class.as_ptr(),
        title.as_ptr(),
        0x8000_0000,
        0,
        0,
        8,
        8,
        null_mut(),
        null_mut(),
        GetModuleHandleW(null()),
        null_mut(),
    );
    if window.is_null() {
        return false;
    }
    let desc = DXGI_SWAP_CHAIN_DESC {
        BufferDesc: DXGI_MODE_DESC {
            Width: 8,
            Height: 8,
            RefreshRate: DXGI_RATIONAL::default(),
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            ScanlineOrdering: DXGI_MODE_SCANLINE_ORDER_UNSPECIFIED,
            Scaling: DXGI_MODE_SCALING_UNSPECIFIED,
        },
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
        BufferCount: 2,
        OutputWindow: HWND(window),
        Windowed: true.into(),
        SwapEffect: DXGI_SWAP_EFFECT_DISCARD,
        Flags: 0,
    };
    let mut swap = None;
    let mut device = None;
    let mut context = None;
    let result = D3D11CreateDeviceAndSwapChain(
        None,
        D3D_DRIVER_TYPE_HARDWARE,
        HMODULE::default(),
        D3D11_CREATE_DEVICE_BGRA_SUPPORT,
        None,
        D3D11_SDK_VERSION,
        Some(&desc),
        Some(&mut swap),
        Some(&mut device),
        None::<*mut D3D_FEATURE_LEVEL>,
        Some(&mut context),
    );
    let mut hooked = false;
    if result.is_ok() {
        if let Some(swap) = swap {
            let present = vtable_entry(swap.as_raw(), 8);
            let resize = vtable_entry(swap.as_raw(), 13);
            let mut original_present = null_mut();
            let mut original_resize = null_mut();
            if MH_CreateHook(
                present,
                dxgi_present_detour as *const () as *mut c_void,
                &mut original_present,
            ) == 0
                && MH_CreateHook(
                    resize,
                    dxgi_resize_detour as *const () as *mut c_void,
                    &mut original_resize,
                ) == 0
            {
                DXGI_PRESENT_ORIGINAL = Some(transmute(original_present));
                DXGI_RESIZE_ORIGINAL = Some(transmute(original_resize));
                hooked = true;
            }
        }
    }
    DestroyWindow(window);
    hooked
}

type GlGenFramebuffers = unsafe extern "system" fn(i32, *mut u32);
type GlDeleteFramebuffers = unsafe extern "system" fn(i32, *const u32);
type GlBindFramebuffer = unsafe extern "system" fn(u32, u32);
type GlFramebufferTexture2D = unsafe extern "system" fn(u32, u32, u32, u32, i32);
type GlBlitFramebuffer =
    unsafe extern "system" fn(i32, i32, i32, i32, i32, i32, i32, i32, u32, u32);
type GlGenBuffers = unsafe extern "system" fn(i32, *mut u32);
type GlDeleteBuffers = unsafe extern "system" fn(i32, *const u32);
type GlBindBuffer = unsafe extern "system" fn(u32, u32);
type GlBufferData = unsafe extern "system" fn(u32, isize, *const c_void, u32);
type GlMapBuffer = unsafe extern "system" fn(u32, u32) -> *mut c_void;
type GlUnmapBuffer = unsafe extern "system" fn(u32) -> u8;
type GlFenceSync = unsafe extern "system" fn(u32, u32) -> *mut c_void;
type GlClientWaitSync = unsafe extern "system" fn(*mut c_void, u32, u64) -> u32;
type GlDeleteSync = unsafe extern "system" fn(*mut c_void);
type WglDxOpenDevice = unsafe extern "system" fn(*mut c_void) -> *mut c_void;
type WglDxCloseDevice = unsafe extern "system" fn(*mut c_void) -> i32;
type WglDxRegisterObject =
    unsafe extern "system" fn(*mut c_void, *mut c_void, u32, u32, u32) -> *mut c_void;
type WglDxUnregisterObject = unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32;
type WglDxLockObjects = unsafe extern "system" fn(*mut c_void, i32, *const *mut c_void) -> i32;
type WglDxUnlockObjects = unsafe extern "system" fn(*mut c_void, i32, *const *mut c_void) -> i32;

unsafe fn gl_proc<T>(name: &'static [u8]) -> Option<T> {
    let value = wglGetProcAddress(name.as_ptr().cast());
    if value.is_null() || value as usize <= 4 || value as isize == -1 {
        None
    } else {
        Some(transmute_copy_ptr(value))
    }
}

unsafe fn transmute_copy_ptr<T>(value: *mut c_void) -> T {
    std::mem::transmute_copy(&value)
}

struct GlFunctions {
    gen_fbo: GlGenFramebuffers,
    delete_fbo: GlDeleteFramebuffers,
    bind_fbo: GlBindFramebuffer,
    attach: GlFramebufferTexture2D,
    blit: GlBlitFramebuffer,
    dx_open: WglDxOpenDevice,
    dx_close: WglDxCloseDevice,
    dx_register: WglDxRegisterObject,
    dx_unregister: WglDxUnregisterObject,
    dx_lock: WglDxLockObjects,
    dx_unlock: WglDxUnlockObjects,
}

impl GlFunctions {
    unsafe fn load() -> Option<Self> {
        Some(Self {
            gen_fbo: gl_proc(b"glGenFramebuffers\0")?,
            delete_fbo: gl_proc(b"glDeleteFramebuffers\0")?,
            bind_fbo: gl_proc(b"glBindFramebuffer\0")?,
            attach: gl_proc(b"glFramebufferTexture2D\0")?,
            blit: gl_proc(b"glBlitFramebuffer\0")?,
            dx_open: gl_proc(b"wglDXOpenDeviceNV\0")?,
            dx_close: gl_proc(b"wglDXCloseDeviceNV\0")?,
            dx_register: gl_proc(b"wglDXRegisterObjectNV\0")?,
            dx_unregister: gl_proc(b"wglDXUnregisterObjectNV\0")?,
            dx_lock: gl_proc(b"wglDXLockObjectsNV\0")?,
            dx_unlock: gl_proc(b"wglDXUnlockObjectsNV\0")?,
        })
    }
}

struct GlInterop {
    context: usize,
    functions: GlFunctions,
    shared: SharedTextures,
    local: Vec<ID3D11Texture2D>,
    interop_device: *mut c_void,
    objects: Vec<*mut c_void>,
    textures: Vec<u32>,
    fbo: u32,
    last: i64,
}

unsafe impl Send for GlInterop {}

#[allow(dead_code)]
impl GlInterop {
    unsafe fn create(context: *mut c_void, w: u32, h: u32) -> Result<Self, String> {
        let functions =
            GlFunctions::load().ok_or("WGL_NV_DX_interop2 or framebuffer blit unavailable")?;
        let mut device = None;
        let mut immediate = None;
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut immediate),
        )
        .map_err(|e| format!("OpenGL bridge D3D11CreateDevice failed: {:#x}", e.code().0))?;
        let device = device.ok_or("OpenGL bridge D3D11 device missing")?;
        let shared = SharedTextures::create(&device, w, h)?;
        let local_desc = D3D11_TEXTURE2D_DESC {
            Width: w,
            Height: h,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let mut local = Vec::with_capacity(SLOTS);
        for _ in 0..SLOTS {
            let mut texture = None;
            device
                .CreateTexture2D(&local_desc, None, Some(&mut texture))
                .map_err(|e| format!("OpenGL local texture failed: {:#x}", e.code().0))?;
            local.push(texture.ok_or("OpenGL local texture missing")?);
        }
        let interop_device = (functions.dx_open)(device.as_raw());
        if interop_device.is_null() {
            return Err("wglDXOpenDeviceNV failed".into());
        }
        let mut textures = vec![0u32; SLOTS];
        glGenTextures(SLOTS as i32, textures.as_mut_ptr());
        let mut objects = Vec::with_capacity(SLOTS);
        for (index, texture) in local.iter().enumerate() {
            let object = (functions.dx_register)(
                interop_device,
                texture.as_raw(),
                textures[index],
                GL_TEXTURE_2D,
                WGL_ACCESS_WRITE_DISCARD_NV,
            );
            if object.is_null() {
                for old in &objects {
                    (functions.dx_unregister)(interop_device, *old);
                }
                glDeleteTextures(textures.len() as i32, textures.as_ptr());
                (functions.dx_close)(interop_device);
                return Err(format!("wglDXRegisterObjectNV failed for slot {index}"));
            }
            objects.push(object);
        }
        let mut fbo = 0;
        (functions.gen_fbo)(1, &mut fbo);
        if fbo == 0 {
            return Err("glGenFramebuffers returned zero".into());
        }
        log(&format!("OpenGL zero-copy bridge initialized ({w}x{h})"));
        GL_CLEANED.store(false, Ordering::Release);
        Ok(Self {
            context: context as usize,
            functions,
            shared,
            local,
            interop_device,
            objects,
            textures,
            fbo,
            last: 0,
        })
    }

    unsafe fn release_gl(&mut self) {
        for object in self.objects.drain(..) {
            (self.functions.dx_unregister)(self.interop_device, object);
        }
        if !self.textures.is_empty() {
            glDeleteTextures(self.textures.len() as i32, self.textures.as_ptr());
            self.textures.clear();
        }
        if self.fbo != 0 {
            (self.functions.delete_fbo)(1, &self.fbo);
            self.fbo = 0;
        }
        if !self.interop_device.is_null() {
            (self.functions.dx_close)(self.interop_device);
            self.interop_device = null_mut();
        }
    }

    unsafe fn capture(&mut self, x: i32, y: i32, w: i32, h: i32) -> bool {
        for _ in 0..self.shared.slots.len() {
            let index = self.shared.next;
            self.shared.next = (self.shared.next + 1) % self.shared.slots.len();
            let slot = &self.shared.slots[index];
            if slot.mutex.AcquireSync(0, 0).is_err() {
                continue;
            }
            let object = self.objects[index];
            if (self.functions.dx_lock)(self.interop_device, 1, &object) == 0 {
                let _ = slot.mutex.ReleaseSync(0);
                continue;
            }
            let mut previous_fbo = 0;
            let mut previous_read = 0;
            glGetIntegerv(GL_FRAMEBUFFER_BINDING, &mut previous_fbo);
            glGetIntegerv(GL_READ_BUFFER, &mut previous_read);
            (self.functions.bind_fbo)(GL_DRAW_FRAMEBUFFER, self.fbo);
            (self.functions.attach)(
                GL_DRAW_FRAMEBUFFER,
                GL_COLOR_ATTACHMENT0,
                GL_TEXTURE_2D,
                self.textures[index],
                0,
            );
            (self.functions.bind_fbo)(GL_READ_FRAMEBUFFER, 0);
            glReadBuffer(GL_BACK);
            (self.functions.blit)(
                x,
                y,
                x + w,
                y + h,
                0,
                h,
                w,
                0,
                GL_COLOR_BUFFER_BIT,
                GL_NEAREST,
            );
            (self.functions.bind_fbo)(GL_FRAMEBUFFER, previous_fbo as u32);
            glReadBuffer(previous_read as u32);
            (self.functions.dx_unlock)(self.interop_device, 1, &object);
            self.shared
                .context
                .CopyResource(&slot.texture, &self.local[index]);
            let _ = slot.mutex.ReleaseSync(1);
            with_protocol(|protocol| protocol.publish_gpu(index, qpc_ticks()));
            return true;
        }
        false
    }
}

struct GlFallback {
    pixels: Vec<u8>,
    logged: bool,
    context: usize,
    w: u32,
    h: u32,
    pbos: [u32; SLOTS],
    textures: [u32; SLOTS],
    ready: [bool; SLOTS],
    fbo: u32,
    frame_no: [u64; SLOTS],
    cur_tex: usize,
    copy_wait: usize,
}

impl GlFallback {
    const fn new() -> Self {
        Self {
            pixels: Vec::new(),
            logged: false,
            context: 0,
            w: 0,
            h: 0,
            pbos: [0; SLOTS],
            textures: [0; SLOTS],
            ready: [false; SLOTS],
            fbo: 0,
            frame_no: [0; SLOTS],
            cur_tex: 0,
            copy_wait: 0,
        }
    }
}

static GL_INTEROP: Mutex<Option<GlInterop>> = Mutex::new(None);
static GL_INTEROP_FAILURE: Mutex<Option<(usize, u32, u32)>> = Mutex::new(None);
static GL_FALLBACK: Mutex<GlFallback> = Mutex::new(GlFallback::new());

unsafe fn present_gl() {
    if IN_GL_PRESENT.swap(true, Ordering::AcqRel) {
        return;
    }
    struct Clear;
    impl Drop for Clear {
        fn drop(&mut self) {
            IN_GL_PRESENT.store(false, Ordering::Release);
        }
    }
    let _clear = Clear;
    let context = wglGetCurrentContext();
    if context.is_null() {
        return;
    }
    if SHUTTING_DOWN.load(Ordering::Acquire) {
        if let Ok(mut interop) = GL_INTEROP.try_lock() {
            if let Some(mut state) = interop.take() {
                if state.context == context as usize {
                    state.release_gl();
                    GL_CLEANED.store(true, Ordering::Release);
                } else {
                    *interop = Some(state);
                }
            } else {
                GL_CLEANED.store(true, Ordering::Release);
            }
        }
        return;
    }
    let control = with_protocol(|protocol| protocol.read_control()).flatten();
    let Some(control) = control else { return };
    if !control.active {
        call_overlay_gl();
        return;
    }
    let Some(frame_no) = with_protocol(|protocol| protocol.next_frame(control)).flatten() else {
        call_overlay_gl();
        return;
    };
    let mut viewport = [0i32; 4];
    glGetIntegerv(GL_VIEWPORT, viewport.as_mut_ptr());
    let (x, y, w, h) = (viewport[0], viewport[1], viewport[2], viewport[3]);
    if w < 16 || h < 16 || w as usize > MAX_W || h as usize > MAX_H {
        call_overlay_gl();
        return;
    }

    let signature = (context as usize, w as u32, h as u32);
    if let Ok(mut interop) = GL_INTEROP.try_lock() {
        let must_rebuild = interop.as_ref().is_some_and(|state| {
            state.context != signature.0
                || state.shared.w != signature.1
                || state.shared.h != signature.2
        });
        if must_rebuild {
            if let Some(state) = interop.as_mut() {
                if state.context == signature.0 {
                    state.release_gl();
                }
            }
            *interop = None;
        }
        if interop.is_none() {
            let already_failed = GL_INTEROP_FAILURE
                .try_lock()
                .ok()
                .and_then(|failed| *failed)
                == Some(signature);
            if !already_failed {
                match GlInterop::create(context, signature.1, signature.2) {
                    Ok(state) => {
                        *interop = Some(state);
                        if let Ok(mut failed) = GL_INTEROP_FAILURE.try_lock() {
                            *failed = None;
                        }
                    }
                    Err(error) => {
                        log(&format!(
                            "OpenGL shared-texture bridge unavailable: {error}; using PBO fallback"
                        ));
                        if let Ok(mut failed) = GL_INTEROP_FAILURE.try_lock() {
                            *failed = Some(signature);
                        }
                    }
                }
            }
        }
        if let Some(state) = interop.as_mut() {
            let _ = state.capture(x, y, w, h);
            drop(interop);
            call_overlay_gl();
            return;
        }
    }

    let mut fallback = match GL_FALLBACK.try_lock() {
        Ok(value) => value,
        Err(_) => {
            call_overlay_gl();
            return;
        }
    };
    let bytes = w as usize * h as usize * 4;
    fallback.pixels.resize(bytes, 0);
    let bind_fbo = gl_proc::<GlBindFramebuffer>(b"glBindFramebuffer\0")
        .or_else(|| gl_proc::<GlBindFramebuffer>(b"glBindFramebufferEXT\0"))
        .or_else(|| gl_proc::<GlBindFramebuffer>(b"glBindFramebufferARB\0"));
    let mut previous_read = 0;
    let mut previous_alignment = 4;
    glGetIntegerv(GL_READ_BUFFER, &mut previous_read);
    glGetIntegerv(GL_PACK_ALIGNMENT, &mut previous_alignment);
    let mut previous_read_fbo = 0;
    if let Some(bind) = bind_fbo {
        glGetIntegerv(GL_READ_FRAMEBUFFER_BINDING, &mut previous_read_fbo);
        bind(GL_READ_FRAMEBUFFER, 0);
    }
    glReadBuffer(GL_BACK);
    glPixelStorei(GL_PACK_ALIGNMENT, 4);

    let pbo = (
        gl_proc::<GlGenBuffers>(b"glGenBuffers\0"),
        gl_proc::<GlDeleteBuffers>(b"glDeleteBuffers\0"),
        gl_proc::<GlBindBuffer>(b"glBindBuffer\0"),
        gl_proc::<GlBufferData>(b"glBufferData\0"),
        gl_proc::<GlMapBuffer>(b"glMapBuffer\0"),
        gl_proc::<GlUnmapBuffer>(b"glUnmapBuffer\0"),
        gl_proc::<GlGenFramebuffers>(b"glGenFramebuffers\0"),
        gl_proc::<GlDeleteFramebuffers>(b"glDeleteFramebuffers\0"),
        gl_proc::<GlBindFramebuffer>(b"glBindFramebuffer\0"),
        gl_proc::<GlFramebufferTexture2D>(b"glFramebufferTexture2D\0"),
        gl_proc::<GlBlitFramebuffer>(b"glBlitFramebuffer\0"),
    );
    let mut published_frame = None;
    if let (
        Some(gen),
        Some(delete),
        Some(bind),
        Some(allocate),
        Some(map),
        Some(unmap),
        Some(gen_fbo),
        Some(delete_fbo),
        Some(bind_fbo),
        Some(attach),
        Some(blit),
    ) = pbo
    {
        let context_changed = fallback.context != context as usize;
        let size_changed = fallback.w != w as u32 || fallback.h != h as u32;
        if context_changed || size_changed || fallback.pbos[0] == 0 {
            let old_context = fallback.context;
            let old_w = fallback.w;
            let old_h = fallback.h;
            let discarded = fallback.ready.iter().filter(|ready| **ready).count();
            let generation = CPU_FRAME_GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
            log(&format!(
                "OpenGL surface transition: context 0x{old_context:X}->0x{:X}, {}x{}->{}x{}, generation {generation}, discarded {discarded} staged frame(s)",
                context as usize, old_w, old_h, w, h
            ));
            if !context_changed && fallback.pbos[0] != 0 {
                delete(SLOTS as i32, fallback.pbos.as_ptr());
                glDeleteTextures(SLOTS as i32, fallback.textures.as_ptr());
                if fallback.fbo != 0 {
                    delete_fbo(1, &fallback.fbo);
                }
            }
            fallback.pbos = [0; SLOTS];
            fallback.textures = [0; SLOTS];
            fallback.ready = [false; SLOTS];
            fallback.fbo = 0;
            fallback.frame_no = [0; SLOTS];
            fallback.cur_tex = 0;
            fallback.copy_wait = 0;
            fallback.context = context as usize;
            fallback.w = w as u32;
            fallback.h = h as u32;
            let mut previous_pack_buffer = 0;
            let mut previous_texture = 0;
            glGetIntegerv(GL_PIXEL_PACK_BUFFER_BINDING, &mut previous_pack_buffer);
            glGetIntegerv(GL_TEXTURE_BINDING_2D, &mut previous_texture);
            gen(SLOTS as i32, fallback.pbos.as_mut_ptr());
            glGenTextures(SLOTS as i32, fallback.textures.as_mut_ptr());
            gen_fbo(1, &mut fallback.fbo);
            for slot in 0..SLOTS {
                bind(GL_PIXEL_PACK_BUFFER, fallback.pbos[slot]);
                allocate(GL_PIXEL_PACK_BUFFER, bytes as isize, null(), GL_STREAM_READ);
                glBindTexture(GL_TEXTURE_2D, fallback.textures[slot]);
                glTexImage2D(
                    GL_TEXTURE_2D,
                    0,
                    GL_RGBA8,
                    w,
                    h,
                    0,
                    GL_BGRA,
                    GL_UNSIGNED_BYTE,
                    null(),
                );
            }
            bind(GL_PIXEL_PACK_BUFFER, previous_pack_buffer as u32);
            glBindTexture(GL_TEXTURE_2D, previous_texture as u32);
            if !fallback.logged {
                fallback.logged = true;
                log("OpenGL OBS-style texture/PBO capture active");
            }
        }

        let mut previous_pack_buffer = 0;
        let mut previous_texture = 0;
        let mut previous_draw_fbo = 0;
        glGetIntegerv(GL_PIXEL_PACK_BUFFER_BINDING, &mut previous_pack_buffer);
        glGetIntegerv(GL_TEXTURE_BINDING_2D, &mut previous_texture);
        glGetIntegerv(GL_DRAW_FRAMEBUFFER_BINDING, &mut previous_draw_fbo);

        let next_tex = (fallback.cur_tex + 1) % SLOTS;
        if fallback.ready[next_tex] {
            fallback.ready[next_tex] = false;
            bind(GL_PIXEL_PACK_BUFFER, fallback.pbos[next_tex]);
            let source = map(GL_PIXEL_PACK_BUFFER, GL_READ_ONLY);
            if !source.is_null() {
                std::ptr::copy_nonoverlapping(
                    source.cast::<u8>(),
                    fallback.pixels.as_mut_ptr(),
                    bytes,
                );
                if unmap(GL_PIXEL_PACK_BUFFER) != 0 {
                    published_frame = Some(fallback.frame_no[next_tex]);
                }
            }
        }

        if fallback.copy_wait < SLOTS - 1 {
            fallback.copy_wait += 1;
        } else {
            let slot = fallback.cur_tex;
            while glGetError() != 0 {}
            bind_fbo(GL_READ_FRAMEBUFFER, 0);
            bind_fbo(GL_DRAW_FRAMEBUFFER, fallback.fbo);
            glBindTexture(GL_TEXTURE_2D, fallback.textures[slot]);
            attach(
                GL_DRAW_FRAMEBUFFER,
                GL_COLOR_ATTACHMENT0,
                GL_TEXTURE_2D,
                fallback.textures[slot],
                0,
            );
            glReadBuffer(GL_BACK);
            glDrawBuffer(GL_COLOR_ATTACHMENT0);
            blit(
                x,
                y,
                x + w,
                y + h,
                0,
                0,
                w,
                h,
                GL_COLOR_BUFFER_BIT,
                GL_NEAREST,
            );
            glBindTexture(GL_TEXTURE_2D, fallback.textures[slot]);
            bind(GL_PIXEL_PACK_BUFFER, fallback.pbos[slot]);
            glGetTexImage(GL_TEXTURE_2D, 0, GL_BGRA, GL_UNSIGNED_BYTE, null_mut());
            if glGetError() == 0 {
                fallback.ready[slot] = true;
                fallback.frame_no[slot] = frame_no;
            }
        }
        fallback.cur_tex = next_tex;

        bind(GL_PIXEL_PACK_BUFFER, previous_pack_buffer as u32);
        glBindTexture(GL_TEXTURE_2D, previous_texture as u32);
        bind_fbo(GL_DRAW_FRAMEBUFFER, previous_draw_fbo as u32);
    } else {
        if !fallback.logged {
            fallback.logged = true;
            log("OpenGL texture/PBO capture unavailable; waiting for DXGI fallback");
        }
    }
    if let Some(bind) = bind_fbo {
        bind(GL_READ_FRAMEBUFFER, previous_read_fbo as u32);
    }
    glReadBuffer(previous_read as u32);
    glPixelStorei(GL_PACK_ALIGNMENT, previous_alignment);
    if let Some(frame_no) = published_frame {
        queue_cpu(&fallback.pixels, w as u32, h as u32, true, frame_no);
    }
    drop(fallback);
    call_overlay_gl();
}

type SwapBuffersFn = unsafe extern "system" fn(*mut c_void) -> i32;
static mut SWAP_ORIGINAL: Option<SwapBuffersFn> = None;
static mut WGL_SWAP_ORIGINAL: Option<SwapBuffersFn> = None;
thread_local! { static SWAP_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) }; }

unsafe extern "system" fn swap_detour(dc: *mut c_void) -> i32 {
    let _guard = DetourGuard::enter();
    let outer = SWAP_DEPTH.with(|depth| {
        let value = depth.get();
        depth.set(value + 1);
        value == 0
    });
    if outer {
        let _ = std::panic::catch_unwind(|| unsafe { present_gl() });
    }
    let result = SWAP_ORIGINAL.map_or(0, |function| function(dc));
    SWAP_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    result
}

unsafe extern "system" fn wgl_swap_detour(dc: *mut c_void) -> i32 {
    let _guard = DetourGuard::enter();
    let outer = SWAP_DEPTH.with(|depth| {
        let value = depth.get();
        depth.set(value + 1);
        value == 0
    });
    if outer {
        let _ = std::panic::catch_unwind(|| unsafe { present_gl() });
    }
    let result = WGL_SWAP_ORIGINAL.map_or(0, |function| function(dc));
    SWAP_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    result
}

unsafe fn install_gl_hooks() -> bool {
    let gdi = GetModuleHandleW(wide("gdi32.dll").as_ptr());
    let opengl = GetModuleHandleW(wide("opengl32.dll").as_ptr());
    if opengl.is_null() {
        return false;
    }
    let mut hooked = false;
    if !gdi.is_null() {
        let target = GetProcAddress(gdi, b"SwapBuffers\0".as_ptr().cast());
        let mut original = null_mut();
        if !target.is_null()
            && MH_CreateHook(
                target,
                swap_detour as *const () as *mut c_void,
                &mut original,
            ) == 0
        {
            SWAP_ORIGINAL = Some(transmute(original));
            hooked = true;
        }
    }
    let target = GetProcAddress(opengl, b"wglSwapBuffers\0".as_ptr().cast());
    let mut original = null_mut();
    if !target.is_null()
        && MH_CreateHook(
            target,
            wgl_swap_detour as *const () as *mut c_void,
            &mut original,
        ) == 0
    {
        WGL_SWAP_ORIGINAL = Some(transmute(original));
        hooked = true;
    }
    hooked
}

struct D3D9Capture {
    stage: Option<IDirect3DSurface9>,
    resolve: Option<IDirect3DSurface9>,
    w: u32,
    h: u32,
    format: D3DFORMAT,
    multisampled: bool,
    pixels: Vec<u8>,
}

unsafe impl Send for D3D9Capture {}
static D3D9_CAPTURE: Mutex<D3D9Capture> = Mutex::new(D3D9Capture {
    stage: None,
    resolve: None,
    w: 0,
    h: 0,
    format: D3DFMT_UNKNOWN,
    multisampled: false,
    pixels: Vec::new(),
});

unsafe fn capture_d3d9(device_raw: *mut c_void) {
    let control = with_protocol(|protocol| protocol.read_control()).flatten();
    let Some(control) = control else { return };
    let Some(frame_no) = with_protocol(|protocol| protocol.next_frame(control)).flatten() else {
        return;
    };
    let mut state = match D3D9_CAPTURE.try_lock() {
        Ok(value) => value,
        Err(_) => return,
    };
    let device = ManuallyDrop::new(IDirect3DDevice9::from_raw(device_raw));
    let Ok(back) = device.GetRenderTarget(0) else {
        return;
    };
    let mut desc = D3DSURFACE_DESC::default();
    if back.GetDesc(&mut desc).is_err()
        || !matches!(desc.Format, D3DFMT_A8R8G8B8 | D3DFMT_X8R8G8B8)
        || desc.Width as usize > MAX_W
        || desc.Height as usize > MAX_H
    {
        return;
    }
    let multisampled = desc.MultiSampleType != D3DMULTISAMPLE_NONE;
    if state.stage.is_none()
        || state.w != desc.Width
        || state.h != desc.Height
        || state.format != desc.Format
        || state.multisampled != multisampled
    {
        let mut stage = None;
        if device
            .CreateOffscreenPlainSurface(
                desc.Width,
                desc.Height,
                desc.Format,
                D3DPOOL_SYSTEMMEM,
                &mut stage,
                null_mut(),
            )
            .is_err()
        {
            return;
        }
        let mut resolve = None;
        if multisampled
            && device
                .CreateRenderTarget(
                    desc.Width,
                    desc.Height,
                    desc.Format,
                    D3DMULTISAMPLE_NONE,
                    0,
                    false,
                    &mut resolve,
                    null_mut(),
                )
                .is_err()
        {
            return;
        }
        state.stage = stage;
        state.resolve = resolve;
        state.w = desc.Width;
        state.h = desc.Height;
        state.format = desc.Format;
        state.multisampled = multisampled;
        log(&format!(
            "D3D9 CPU capture initialized ({}x{}, msaa={multisampled})",
            desc.Width, desc.Height
        ));
    }
    let Some(stage) = state.stage.as_ref().cloned() else {
        return;
    };
    let readable = match state.resolve.as_ref() {
        Some(resolve) => {
            if device
                .StretchRect(&back, null(), resolve, null(), D3DTEXF_NONE)
                .is_err()
            {
                return;
            }
            device.GetRenderTargetData(resolve, &stage)
        }
        None => device.GetRenderTargetData(&back, &stage),
    };
    if readable.is_err() {
        return;
    }
    let mut locked = D3DLOCKED_RECT::default();
    if stage
        .LockRect(&mut locked, null(), D3DLOCK_READONLY as u32)
        .is_err()
    {
        return;
    }
    let row = desc.Width as usize * 4;
    let pitch = locked.Pitch.max(0) as usize;
    let bytes = row * desc.Height as usize;
    state.pixels.resize(bytes, 0);
    if !locked.pBits.is_null() && pitch >= row {
        for y in 0..desc.Height as usize {
            std::ptr::copy_nonoverlapping(
                (locked.pBits as *const u8).add(y * pitch),
                state.pixels.as_mut_ptr().add(y * row),
                row,
            );
        }
        queue_cpu(&state.pixels, desc.Width, desc.Height, false, frame_no);
    }
    let _ = stage.UnlockRect();
}

type D3D9Present = unsafe extern "system" fn(
    *mut c_void,
    *const c_void,
    *const c_void,
    *mut c_void,
    *const c_void,
) -> i32;
static mut D3D9_PRESENT_ORIGINAL: Option<D3D9Present> = None;

unsafe extern "system" fn d3d9_present_detour(
    device: *mut c_void,
    source: *const c_void,
    destination: *const c_void,
    window: *mut c_void,
    dirty: *const c_void,
) -> i32 {
    let _guard = DetourGuard::enter();
    if !SHUTTING_DOWN.load(Ordering::Acquire) {
        let _ = std::panic::catch_unwind(|| unsafe {
            capture_d3d9(device);
            call_overlay_d3d9(device);
        });
    }
    D3D9_PRESENT_ORIGINAL.map_or(0x8876_086Cu32 as i32, |function| {
        function(device, source, destination, window, dirty)
    })
}

unsafe fn install_d3d9_hook() -> bool {
    if GetModuleHandleW(wide("d3d9.dll").as_ptr()).is_null() {
        return false;
    }
    let Some(api) = Direct3DCreate9(D3D_SDK_VERSION) else {
        return false;
    };
    let class = wide("STATIC");
    let title = wide("InstantReplayD3D9Probe");
    let window = CreateWindowExW(
        0,
        class.as_ptr(),
        title.as_ptr(),
        0x8000_0000,
        0,
        0,
        8,
        8,
        null_mut(),
        null_mut(),
        GetModuleHandleW(null()),
        null_mut(),
    );
    if window.is_null() {
        return false;
    }
    let mut params = D3DPRESENT_PARAMETERS {
        BackBufferWidth: 8,
        BackBufferHeight: 8,
        BackBufferFormat: D3DFMT_A8R8G8B8,
        BackBufferCount: 1,
        MultiSampleType: D3DMULTISAMPLE_NONE,
        MultiSampleQuality: 0,
        SwapEffect: D3DSWAPEFFECT_DISCARD,
        hDeviceWindow: HWND(window),
        Windowed: true.into(),
        EnableAutoDepthStencil: false.into(),
        AutoDepthStencilFormat: D3DFMT_UNKNOWN,
        Flags: 0,
        FullScreen_RefreshRateInHz: 0,
        PresentationInterval: 0,
    };
    let mut device = None;
    let result = api.CreateDevice(
        D3DADAPTER_DEFAULT,
        D3DDEVTYPE_HAL,
        HWND(window),
        D3DCREATE_SOFTWARE_VERTEXPROCESSING as u32,
        &mut params,
        &mut device,
    );
    let mut hooked = false;
    if result.is_ok() {
        if let Some(device) = device {
            let target = vtable_entry(device.as_raw(), 17);
            let mut original = null_mut();
            if MH_CreateHook(
                target,
                d3d9_present_detour as *const () as *mut c_void,
                &mut original,
            ) == 0
            {
                D3D9_PRESENT_ORIGINAL = Some(transmute(original));
                hooked = true;
            }
        }
    }
    DestroyWindow(window);
    hooked
}

type VulkanPresent = unsafe extern "system" fn(*mut c_void, *const c_void) -> i32;
static mut VULKAN_PRESENT_ORIGINAL: Option<VulkanPresent> = None;

unsafe extern "system" fn vulkan_present_detour(
    queue: *mut c_void,
    present_info: *const c_void,
) -> i32 {
    let _guard = DetourGuard::enter();
    VULKAN_PRESENT_ORIGINAL.map_or(-1, |function| function(queue, present_info))
}

unsafe fn install_vulkan_hook() -> bool {
    let module = GetModuleHandleW(wide("vulkan-1.dll").as_ptr());
    if module.is_null() {
        return false;
    }
    let target = GetProcAddress(module, b"vkQueuePresentKHR\0".as_ptr().cast());
    let mut original = null_mut();
    if target.is_null()
        || MH_CreateHook(
            target,
            vulkan_present_detour as *const () as *mut c_void,
            &mut original,
        ) != 0
    {
        return false;
    }
    VULKAN_PRESENT_ORIGINAL = Some(transmute(original));
    true
}

static READY_EVENT: AtomicU64 = AtomicU64::new(0);

unsafe extern "system" fn init_thread(_: *mut c_void) -> u32 {
    let mut frequency = LargeInteger { quad: 0 };
    if QueryPerformanceFrequency(&mut frequency) == 0 || frequency.quad <= 0 {
        return 0;
    }
    QPC_FREQ.store(frequency.quad as u64, Ordering::Release);
    start_cpu_publisher();
    with_protocol(|protocol| protocol.ensure_control());
    log(&format!(
        "capture_hook loaded (pid {}, v{})",
        GetCurrentProcessId(),
        VERSION
    ));
    let mh_status = MH_Initialize();
    if mh_status != 0 {
        log("MH_Initialize failed");
        return 0;
    }
    let dxgi = install_dxgi_hooks();
    let d3d9 = install_d3d9_hook();
    let d3d12_queue = install_d3d12_queue_hook();
    let gl = install_gl_hooks();
    let vulkan = install_vulkan_hook();
    if !dxgi && !d3d9 && !gl && !vulkan {
        log("no supported graphics present entry points hooked");
        MH_Uninitialize();
        return 0;
    }
    if MH_EnableHook(null_mut()) != 0 {
        log("MH_EnableHook failed");
        MH_DisableHook(null_mut());
        MH_Uninitialize();
        return 0;
    }
    log(&format!(
        "hooks enabled (D3D9={}, DXGI={}, D3D12 queue={}, OpenGL={}, Vulkan={})",
        d3d9, dxgi, d3d12_queue, gl, vulkan
    ));
    let event = CreateEventW(
        null(),
        1,
        0,
        wide("Local\\InstantReplay.CaptureHook.Ready.v1").as_ptr(),
    );
    if !event.is_null() {
        SetEvent(event);
        READY_EVENT.store(event as u64, Ordering::Release);
    }
    0
}

unsafe fn shutdown() {
    SHUTTING_DOWN.store(true, Ordering::Release);
    MH_DisableHook(null_mut());
    MH_Uninitialize();
    if let Ok(mut sender) = CPU_PUBLISHER.lock() {
        sender.take();
    }
    if let Ok(mut worker) = CPU_PUBLISH_THREAD.lock() {
        if let Some(worker) = worker.take() {
            let _ = worker.join();
        }
    }
    if let Ok(mut capture) = D3D11_CAPTURE.lock() {
        *capture = None;
    }
    if let Ok(mut capture) = D3D12_CAPTURE.lock() {
        *capture = None;
    }
    if let Ok(mut queue) = D3D12_QUEUE.lock() {
        *queue = None;
    }
    if let Ok(mut capture) = D3D9_CAPTURE.lock() {
        capture.stage = None;
        capture.resolve = None;
        capture.pixels.clear();
    }
    if let Ok(mut protocol) = PROTOCOL.lock() {
        protocol.shutdown();
    }
    if let Ok(mut file) = LOG_FILE.lock() {
        if file.handle != INVALID_HANDLE && !file.handle.is_null() {
            CloseHandle(file.handle);
        }
        file.handle = INVALID_HANDLE;
        file.path.clear();
    }
    let event = READY_EVENT.swap(0, Ordering::AcqRel) as *mut c_void;
    if !event.is_null() {
        CloseHandle(event);
    }
}

unsafe extern "system" fn shutdown_thread(_: *mut c_void) -> u32 {
    SHUTTING_DOWN.store(true, Ordering::Release);
    log("cooperative shutdown requested");
    for _ in 0..50 {
        if GL_CLEANED.load(Ordering::Acquire) {
            break;
        }
        Sleep(10);
    }
    MH_DisableHook(null_mut());
    for _ in 0..500 {
        if ACTIVE_DETOURS.load(Ordering::Acquire) == 0 {
            break;
        }
        Sleep(10);
    }
    shutdown();
    FreeLibraryAndExitThread(SELF_MODULE.load(Ordering::Acquire) as *mut c_void, 0)
}

#[no_mangle]
pub unsafe extern "system" fn IR_RequestShutdown(_: *mut c_void) -> i32 {
    if SHUTDOWN_STARTED.swap(true, Ordering::AcqRel) {
        return 1;
    }
    let thread = CreateThread(null(), 0, shutdown_thread, null_mut(), 0, null_mut());
    if thread.is_null() {
        SHUTDOWN_STARTED.store(false, Ordering::Release);
        return 0;
    }
    CloseHandle(thread);
    1
}

#[no_mangle]
pub unsafe extern "system" fn DllMain(
    module: *mut c_void,
    reason: u32,
    _reserved: *mut c_void,
) -> i32 {
    match reason {
        1 => {
            process_attach(module);
        }
        0 => {
            SHUTTING_DOWN.store(true, Ordering::Release);
            if let Ok(protocol) = PROTOCOL.try_lock() {
                if let Some(ring) = &protocol.ring {
                    write_u32(ring.base, OFF_ACTIVE, 0);
                }
            }
        }
        _ => {}
    }
    1
}

unsafe fn process_attach(module: *mut c_void) {
    if SELF_MODULE
        .compare_exchange(0, module as u64, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    DisableThreadLibraryCalls(module);
    let thread = CreateThread(null(), 0, init_thread, null_mut(), 0, null_mut());
    if !thread.is_null() {
        CloseHandle(thread);
    }
}

unsafe extern "system" fn tls_callback(module: *mut c_void, reason: u32, _: *mut c_void) {
    if reason == 1 {
        process_attach(module);
    }
}

#[used]
#[cfg_attr(target_env = "msvc", unsafe(link_section = ".CRT$XLB"))]
#[no_mangle]
pub static TLS_CALLBACK: unsafe extern "system" fn(*mut c_void, u32, *mut c_void) = tls_callback;
