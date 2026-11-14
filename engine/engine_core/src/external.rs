use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows::core::{Interface, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Graphics::Direct3D11::{ID3D11Device1, ID3D11Texture2D};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIAdapter, IDXGIFactory1, IDXGIKeyedMutex,
};
use windows::Win32::System::Memory::{
    MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_READ, FILE_MAP_WRITE,
    MEMORY_MAPPED_VIEW_ADDRESS,
};

use crate::{qpc_ticks_to_100ns, RawFrame, SharedState};

pub const FRAME_RING_NAME: &str = "Local\\InstantReplay.Frames.v1";

const MAGIC: u32 = 0x4952_4652;
const VERSION: u32 = 3;
const SLOTS: usize = 3;
const HEADER_BYTES: usize = 4096;
const MAX_SLOT_BYTES: usize = 3840 * 2160 * 4;

const OFF_MAGIC: usize = 0;
const OFF_VERSION: usize = 4;
const OFF_W: usize = 12;
const OFF_H: usize = 16;
const OFF_STRIDE: usize = 20;
const OFF_ACTIVE: usize = 24;
const OFF_SEQ: usize = 36;
const OFF_SLOT_SEQ: usize = 40;
const OFF_SLOT_PTS: usize = 56;
const OFF_TRANSPORT: usize = 96;
const OFF_GPU_GENERATION: usize = 100;
const OFF_GPU_LATEST: usize = 104;
const OFF_GPU_SEQ: usize = 108;
const OFF_GPU_W: usize = 112;
const OFF_GPU_H: usize = 116;
const OFF_GPU_SLOT_SEQ: usize = 128;
const OFF_GPU_SLOT_PTS: usize = 144;
const OFF_GPU_ADAPTER_LUID: usize = 176;
const OFF_GPU_NAMES: usize = 192;
const GPU_NAME_CHARS: usize = 96;
const GPU_ACCESS: u32 = 0x8000_0001;

const OFF_CPU_GENERATION: usize = 100;
const OFF_CPU_SLOT_BYTES: usize = 120;
const OFF_CPU_NAME: usize = 1024;
const CPU_NAME_CHARS: usize = 128;

const OFF_READ_SEQ: usize = 800;
const OFF_SLOT_GEOMETRY: usize = 848;
const SLOT_GEOMETRY_BYTES: usize = 12;

struct View {
    handle: HANDLE,
    base: *mut u8,
}

unsafe impl Send for View {}

impl View {
    fn open(name: &str) -> Option<Self> {
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let handle = unsafe {
            OpenFileMappingW(
                FILE_MAP_READ.0 | FILE_MAP_WRITE.0,
                false,
                PCWSTR(wide.as_ptr()),
            )
        }
        .ok()?;
        let addr = unsafe { MapViewOfFile(handle, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, 0) };
        if addr.Value.is_null() {
            unsafe {
                let _ = CloseHandle(handle);
            }
            return None;
        }
        Some(Self {
            handle,
            base: addr.Value as *mut u8,
        })
    }
}

impl Drop for View {
    fn drop(&mut self) {
        unsafe {
            let _ = UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                Value: self.base as *mut core::ffi::c_void,
            });
            let _ = CloseHandle(self.handle);
        }
    }
}

unsafe fn read_u32(base: *const u8, offset: usize) -> u32 {
    std::ptr::read_volatile(base.add(offset) as *const u32)
}

unsafe fn read_i64(base: *const u8, offset: usize) -> i64 {
    std::ptr::read_volatile(base.add(offset) as *const i64)
}

unsafe fn write_u32(base: *mut u8, offset: usize, value: u32) {
    std::ptr::write_volatile(base.add(offset).cast::<u32>(), value)
}

unsafe fn read_name(base: *const u8, slot: usize) -> Option<Vec<u16>> {
    let source = base
        .add(OFF_GPU_NAMES + slot * GPU_NAME_CHARS * 2)
        .cast::<u16>();
    let mut value = Vec::with_capacity(GPU_NAME_CHARS);
    for index in 0..GPU_NAME_CHARS {
        let ch = std::ptr::read_volatile(source.add(index));
        value.push(ch);
        if ch == 0 {
            return (value.len() > 1).then_some(value);
        }
    }
    None
}

unsafe fn read_cpu_name(base: *const u8) -> Option<Vec<u16>> {
    let source = base.add(OFF_CPU_NAME).cast::<u16>();
    let mut value = Vec::with_capacity(CPU_NAME_CHARS);
    for index in 0..CPU_NAME_CHARS {
        let ch = std::ptr::read_volatile(source.add(index));
        value.push(ch);
        if ch == 0 {
            return (value.len() > 1).then_some(value);
        }
    }
    None
}

fn copy_top_down(source: &[u8], width: usize, height: usize, stride: usize, out: &mut Vec<u8>) {
    let row = width * 4;
    out.resize(row * height, 0);
    for y in 0..height {
        let start = y * stride;
        out[y * row..(y + 1) * row].copy_from_slice(&source[start..start + row]);
    }
}

pub fn dimensions() -> Option<(i32, i32)> {
    let deadline = Instant::now() + Duration::from_millis(1500);
    loop {
        if let Some(view) = View::open(FRAME_RING_NAME) {
            let magic = unsafe { read_u32(view.base, OFF_MAGIC) };
            let version = unsafe { read_u32(view.base, OFF_VERSION) };
            let transport = unsafe { read_u32(view.base, OFF_TRANSPORT) };
            let gpu = transport == 1;
            let dynamic_cpu = transport == 2;
            let w = unsafe { read_u32(view.base, if gpu { OFF_GPU_W } else { OFF_W }) };
            let h = unsafe { read_u32(view.base, if gpu { OFF_GPU_H } else { OFF_H }) };
            let stride = unsafe { read_u32(view.base, OFF_STRIDE) };
            let plausible = magic == MAGIC
                && version == VERSION
                && w >= 16
                && h >= 16
                && (gpu
                    || dynamic_cpu
                    || (stride >= w * 4 && (stride as usize) * (h as usize) <= MAX_SLOT_BYTES));
            if plausible {
                return Some((w as i32, h as i32));
            }
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub fn gpu_transport_available() -> bool {
    View::open(FRAME_RING_NAME).is_some_and(|view| unsafe {
        read_u32(view.base, OFF_MAGIC) == MAGIC
            && read_u32(view.base, OFF_VERSION) == VERSION
            && read_u32(view.base, OFF_TRANSPORT) == 1
            && read_u32(view.base, OFF_GPU_GENERATION) != 0
    })
}

struct GpuShared {
    generation: u32,
    input: Arc<crate::encoder::GpuInput>,
    slots: Vec<(ID3D11Texture2D, IDXGIKeyedMutex)>,
}

struct CpuShared {
    generation: u32,
    slot_bytes: usize,
    view: View,
}

impl CpuShared {
    unsafe fn open(control: &View) -> Option<Self> {
        let generation = read_u32(control.base, OFF_CPU_GENERATION);
        let slot_bytes = read_u32(control.base, OFF_CPU_SLOT_BYTES) as usize;
        if generation == 0 || slot_bytes == 0 {
            return None;
        }
        let name = read_cpu_name(control.base)?;
        let name = String::from_utf16(&name[..name.len().saturating_sub(1)]).ok()?;
        Some(Self {
            generation,
            slot_bytes,
            view: View::open(&name)?,
        })
    }
}

fn luid_value(luid: &windows::Win32::Foundation::LUID) -> i64 {
    unsafe { std::ptr::read_unaligned((luid as *const windows::Win32::Foundation::LUID).cast()) }
}

unsafe fn adapter_for_luid(value: i64) -> Option<IDXGIAdapter> {
    if value == 0 {
        return None;
    }
    let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
    for index in 0..32 {
        let Ok(adapter) = factory.EnumAdapters1(index) else {
            break;
        };
        let Ok(desc) = adapter.GetDesc1() else {
            continue;
        };
        if luid_value(&desc.AdapterLuid) == value {
            return adapter.cast().ok();
        }
    }
    None
}

impl GpuShared {
    unsafe fn open(view: &View) -> Option<Self> {
        let generation = read_u32(view.base, OFF_GPU_GENERATION);
        let w = read_u32(view.base, OFF_GPU_W);
        let h = read_u32(view.base, OFF_GPU_H);
        if generation == 0 || !(16..=8192).contains(&w) || !(16..=8192).contains(&h) {
            return None;
        }
        let luid = read_i64(view.base, OFF_GPU_ADAPTER_LUID);
        let adapter = adapter_for_luid(luid);
        let (device, context) = crate::capture::create_device(adapter.as_ref()).ok()?;
        let device1: ID3D11Device1 = device.cast().ok()?;
        let mut slots = Vec::with_capacity(SLOTS);
        for index in 0..SLOTS {
            let name = read_name(view.base, index)?;
            let texture: ID3D11Texture2D = device1
                .OpenSharedResourceByName(PCWSTR(name.as_ptr()), GPU_ACCESS)
                .ok()?;
            let keyed: IDXGIKeyedMutex = texture.cast().ok()?;
            slots.push((texture, keyed));
        }
        let input = Arc::new(crate::encoder::GpuInput::create(&device, &context, w, h).ok()?);
        Some(Self {
            generation,
            input,
            slots,
        })
    }
}

pub fn spawn(shared: Arc<SharedState>) -> Option<JoinHandle<()>> {
    std::thread::Builder::new()
        .name("ir-hook-capture".into())
        .spawn(move || run(shared))
        .ok()
}

fn run(shared: Arc<SharedState>) {
    let mut view: Option<View> = None;
    let mut last_gpu_seq = 0u32;
    let mut gpu: Option<GpuShared> = None;
    let mut cpu: Option<CpuShared> = None;
    let mut next_open = Instant::now();

    while shared.running.load(Ordering::Acquire) {
        if view.is_none() {
            if Instant::now() >= next_open {
                if let Some(v) = View::open(FRAME_RING_NAME) {
                    let seq = unsafe { read_u32(v.base, OFF_SEQ) };
                    unsafe { write_u32(v.base, OFF_READ_SEQ, seq) };
                    view = Some(v);
                }
                next_open = Instant::now() + Duration::from_millis(250);
            }
            if view.is_none() {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
        }

        let v = view.as_ref().unwrap();
        let active = unsafe { read_u32(v.base, OFF_ACTIVE) };
        if active == 0 {
            view = None;
            gpu = None;
            cpu = None;
            *shared.gpu.lock().unwrap_or_else(|e| e.into_inner()) = None;
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }

        let transport = unsafe { read_u32(v.base, OFF_TRANSPORT) };
        if transport == 1 {
            let generation = unsafe { read_u32(v.base, OFF_GPU_GENERATION) };
            if gpu
                .as_ref()
                .is_none_or(|state| state.generation != generation)
            {
                gpu = unsafe { GpuShared::open(v) };
                if let Some(state) = &gpu {
                    for (_, keyed) in &state.slots {
                        if unsafe { keyed.AcquireSync(1, 0) }.is_ok() {
                            let _ = unsafe { keyed.ReleaseSync(0) };
                        }
                    }
                    last_gpu_seq = unsafe { read_u32(v.base, OFF_GPU_SEQ) };
                    *shared.gpu.lock().unwrap_or_else(|e| e.into_inner()) =
                        Some(state.input.clone());
                    shared.gpu_mode.store(1, Ordering::Release);
                    shared.gpu_gen.fetch_add(1, Ordering::Release);
                    shared.raw_wait.notify_all();
                } else {
                    std::thread::sleep(Duration::from_millis(20));
                    continue;
                }
            }
            let seq = unsafe { read_u32(v.base, OFF_GPU_SEQ) };
            if seq == last_gpu_seq {
                std::thread::sleep(Duration::from_millis(1));
                continue;
            }
            let latest = unsafe { read_u32(v.base, OFF_GPU_LATEST) } as usize;
            let Some(state) = &gpu else {
                continue;
            };
            if latest >= state.slots.len() {
                last_gpu_seq = seq;
                continue;
            }
            let slot_seq = unsafe { read_u32(v.base, OFF_GPU_SLOT_SEQ + latest * 4) };
            let pts_ticks = unsafe { read_i64(v.base, OFF_GPU_SLOT_PTS + latest * 8) };
            let (texture, keyed) = &state.slots[latest];
            if unsafe { keyed.AcquireSync(1, 0) }.is_err() {
                std::thread::yield_now();
                continue;
            }
            let pts = if pts_ticks > 0 {
                (qpc_ticks_to_100ns(pts_ticks) - shared.start_qpc.load(Ordering::Relaxed)).max(0)
            } else {
                (crate::qpc100ns() - shared.start_qpc.load(Ordering::Relaxed)).max(0)
            };
            state.input.submit(texture, pts);
            let _ = unsafe { keyed.ReleaseSync(0) };
            if unsafe { read_u32(v.base, OFF_GPU_SLOT_SEQ + latest * 4) } == slot_seq {
                shared.last_capture_pts.store(pts, Ordering::Relaxed);
                shared.n_capture.fetch_add(1, Ordering::Relaxed);
                shared.gpu_frames.fetch_add(1, Ordering::Relaxed);
            }
            last_gpu_seq = seq;
            continue;
        }
        if gpu.take().is_some() {
            *shared.gpu.lock().unwrap_or_else(|e| e.into_inner()) = None;
            shared.gpu_mode.store(0, Ordering::Release);
            shared.gpu_gen.fetch_add(1, Ordering::Release);
        }

        if transport == 2 {
            let generation = unsafe { read_u32(v.base, OFF_CPU_GENERATION) };
            if cpu
                .as_ref()
                .is_none_or(|state| state.generation != generation)
            {
                cpu = unsafe { CpuShared::open(v) };
                if cpu.is_none() {
                    std::thread::sleep(Duration::from_millis(20));
                    continue;
                }
                let seq = unsafe { read_u32(v.base, OFF_SEQ) };
                unsafe { write_u32(v.base, OFF_READ_SEQ, seq) };
            }
        } else {
            cpu = None;
        }

        let seq = unsafe { read_u32(v.base, OFF_SEQ) };
        let mut read = unsafe { read_u32(v.base, OFF_READ_SEQ) };
        if seq.wrapping_sub(read) > SLOTS as u32 {
            read = seq.wrapping_sub(SLOTS as u32);
            unsafe { write_u32(v.base, OFF_READ_SEQ, read) };
        }
        if read == seq {
            std::thread::sleep(Duration::from_millis(1));
            continue;
        }

        let slot_index = read as usize % SLOTS;
        let geometry = OFF_SLOT_GEOMETRY + slot_index * SLOT_GEOMETRY_BYTES;
        let width = unsafe { read_u32(v.base, geometry) } as usize;
        let height = unsafe { read_u32(v.base, geometry + 4) } as usize;
        let stride = unsafe { read_u32(v.base, geometry + 8) } as usize;
        let slot_capacity = cpu
            .as_ref()
            .map_or(MAX_SLOT_BYTES, |state| state.slot_bytes);
        let valid = width >= 16
            && height >= 16
            && stride >= width * 4
            && stride
                .checked_mul(height)
                .is_some_and(|bytes| bytes <= slot_capacity);
        if !valid {
            unsafe { write_u32(v.base, OFF_READ_SEQ, read.wrapping_add(1)) };
            std::thread::sleep(Duration::from_millis(1));
            continue;
        }

        let pts_ticks = unsafe { read_i64(v.base, OFF_SLOT_PTS + slot_index * 8) };
        let raw_pts = if pts_ticks > 0 {
            qpc_ticks_to_100ns(pts_ticks)
        } else {
            crate::qpc100ns()
        };
        let Some(pts) = shared.capture_time(raw_pts) else {
            unsafe { write_u32(v.base, OFF_READ_SEQ, read.wrapping_add(1)) };
            continue;
        };

        let started = Instant::now();
        let slot_seq_before = unsafe { read_u32(v.base, OFF_SLOT_SEQ + slot_index * 4) };
        let slot = if let Some(state) = &cpu {
            unsafe { state.view.base.add(slot_index * state.slot_bytes) }
        } else {
            unsafe { v.base.add(HEADER_BYTES + slot_index * MAX_SLOT_BYTES) }
        };

        let mut bgra = shared.take_bgra();
        let source = unsafe { std::slice::from_raw_parts(slot, stride * height) };
        copy_top_down(source, width, height, stride, &mut bgra);

        let slot_seq_after = unsafe { read_u32(v.base, OFF_SLOT_SEQ + slot_index * 4) };
        if slot_seq_before != slot_seq_after {
            shared.recycle_bgra(bgra);
            unsafe { write_u32(v.base, OFF_READ_SEQ, read.wrapping_add(1)) };
            continue;
        }

        shared.last_capture_pts.store(pts, Ordering::Relaxed);
        shared.n_capture.fetch_add(1, Ordering::Relaxed);
        shared
            .t_capture_ns
            .fetch_add(started.elapsed().as_nanos() as i64, Ordering::Relaxed);
        shared.push_frame(RawFrame {
            bgra,
            w: width as u32,
            h: height as u32,
            pts100ns: pts,
            frame_no: 0,
        });
        unsafe { write_u32(v.base, OFF_READ_SEQ, read.wrapping_add(1)) };
    }
}

#[cfg(test)]
mod tests {
    use super::copy_top_down;

    #[test]
    fn copies_top_down_rows_and_ignores_pitch_padding() {
        let source = [
            1, 2, 3, 4, 5, 6, 7, 8, 90, 91, 92, 93, 9, 10, 11, 12, 13, 14, 15, 16, 94, 95, 96, 97,
        ];
        let mut out = Vec::new();
        copy_top_down(&source, 2, 2, 12, &mut out);
        assert_eq!(out, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
    }

    #[test]
    fn dimension_changes_reuse_the_consumer_without_stale_rows() {
        let mut out = vec![0; 64];
        copy_top_down(&[1; 32], 4, 2, 16, &mut out);
        assert_eq!(out.len(), 32);
        copy_top_down(&[2; 16], 2, 2, 8, &mut out);
        assert_eq!(out, vec![2; 16]);
    }
}
