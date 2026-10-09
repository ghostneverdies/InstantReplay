#![allow(non_snake_case)]

pub mod audio;
pub mod capture;
pub mod capture_dxgi;
pub mod encoder;

pub mod muxer;

use std::collections::VecDeque;
use std::ffi::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

pub const STAGE_CREATE_DEVICE: i64 = 1;
pub const STAGE_CREATE_ITEM: i64 = 2;
pub const STAGE_FRAME_POOL: i64 = 3;
pub const STAGE_START_CAPTURE: i64 = 4;
pub const STAGE_ENCODER: i64 = 5;
pub const STAGE_AUDIO: i64 = 6;
pub const STAGE_MUX: i64 = 7;

const SECOND_100NS: i64 = 10_000_000;
const RING_BUDGET_BYTES: i64 = 512 * 1024 * 1024;
pub(crate) const MAX_RAW_QUEUE: usize = 8;
const AUDIO_PTS_RESYNC: i64 = 5 * SECOND_100NS / 100;
const MIC_READY_WAIT_MS: u64 = 2500;

pub const AUDIO_RATE: u32 = 48_000;
pub const AUDIO_CHANNELS: u32 = 2;

pub(crate) fn qpc100ns() -> i64 {
    unsafe {
        let mut t = 0i64;
        let mut f = 0i64;
        if QueryPerformanceFrequency(&mut f).is_err() || f == 0 {
            return 0;
        }
        if QueryPerformanceCounter(&mut t).is_err() {
            return 0;
        }
        t * SECOND_100NS / f
    }
}

pub struct RawFrame {
    pub bgra: Vec<u8>,
    pub w: u32,
    pub h: u32,
    pub pts100ns: i64,
}

#[derive(Clone)]
pub struct RingVideo {
    pub data: Vec<u8>,
    pub keyframe: bool,
    pub pts100ns: i64,
}

#[derive(Clone)]
pub struct RingAudio {
    pub data: Vec<u8>,
    pub frames: u32,
    pub pts100ns: i64,
}

pub struct SharedState {
    pub running: AtomicBool,
    pub has_error: AtomicBool,
    pub ring_seconds: AtomicI32,
    pub fps: AtomicI32,
    pub res_w: AtomicI32,
    pub res_h: AtomicI32,
    pub start_qpc: AtomicI64,
    pub frames_encoded: AtomicI64,
    pub frames_dropped: AtomicI64,
    pub audio_frames: AtomicI64,
    pub audio_events: AtomicI64,
    pub audio_got_frames: AtomicI64,
    pub audio_fail_hr: AtomicI64,
    pub health_heartbeat: AtomicI64,

    pub mic_frames: AtomicI64,
    pub mic_events: AtomicI64,
    pub mic_got_frames: AtomicI64,
    pub mic_fail_hr: AtomicI64,
    pub mic_active: Mutex<String>,

    pub sys_peak: AtomicI32,
    pub mic_peak: AtomicI32,

    pub vring: Mutex<Vec<RingVideo>>,
    pub vring_bytes: AtomicI64,
    pub aring: Mutex<Vec<RingAudio>>,
    pub aring_bytes: AtomicI64,
    pub mring: Mutex<Vec<RingAudio>>,
    pub mring_bytes: AtomicI64,

    pub raw: Mutex<VecDeque<RawFrame>>,
    pub raw_wait: Condvar,

    pub active_encoder: AtomicI32,
    pub hw_init_hr: AtomicI32,
    pub gpu_mode: AtomicI32,
    pub gpu: Mutex<Option<Arc<encoder::GpuInput>>>,
    pub gpu_gen: AtomicI64,
    pub gpu_frames: AtomicI64,
    pub enc_resolved: AtomicBool,
    pub enc_fail_hr: AtomicI32,
    pub keyframe_pending: Mutex<bool>,
    pub audio_next_pts: Mutex<i64>,
    pub mic_next_pts: Mutex<i64>,
    pub min_gap_100ns: AtomicI64,
    pub last_capture_pts: AtomicI64,

    pub t_capture_ns: AtomicI64,
    pub n_capture: AtomicI64,
    pub n_wgc: AtomicI64,
    pub t_wgc_gap_ns: AtomicI64,
    pub t_buf_ns: AtomicI64,
    pub t_copy_ns: AtomicI64,
    pub n_frame_diff: AtomicI64,
    pub n_frame_same: AtomicI64,
    pub n_gap: [AtomicI64; capture::GAP_BUCKETS],
    pub n_gap_max_ns: AtomicI64,
    pub n_staging_alloc: AtomicI64,
    pub capture_method: AtomicI32,
    pub n_dxgi_timeout: AtomicI64,
    pub n_dxgi_access_lost: AtomicI64,
    pub n_dxgi_fatal: AtomicI64,
    pub t_wait_ns: AtomicI64,
    pub t_convert_ns: AtomicI64,
    pub t_encode_ns: AtomicI64,
    pub n_encode_calls: AtomicI64,
    pub n_queue_drop: AtomicI64,
    pub n_superseded: AtomicI64,
    pub n_slot_resync: AtomicI64,
    pub bgra_pool: Mutex<Vec<Vec<u8>>>,
    pub n_capture_drop: AtomicI64,
}

impl SharedState {
    pub fn new() -> Arc<Self> {
        Arc::new(SharedState {
            running: AtomicBool::new(false),
            has_error: AtomicBool::new(false),
            ring_seconds: AtomicI32::new(60),
            fps: AtomicI32::new(30),
            res_w: AtomicI32::new(1280),
            res_h: AtomicI32::new(720),
            start_qpc: AtomicI64::new(0),
            frames_encoded: AtomicI64::new(0),
            frames_dropped: AtomicI64::new(0),
            audio_frames: AtomicI64::new(0),
            audio_events: AtomicI64::new(0),
            audio_got_frames: AtomicI64::new(0),
            audio_fail_hr: AtomicI64::new(0),
            health_heartbeat: AtomicI64::new(0),
            mic_frames: AtomicI64::new(0),
            mic_events: AtomicI64::new(0),
            mic_got_frames: AtomicI64::new(0),
            mic_fail_hr: AtomicI64::new(0),
            mic_active: Mutex::new(String::new()),
            sys_peak: AtomicI32::new(0),
            mic_peak: AtomicI32::new(0),
            vring: Mutex::new(Vec::new()),
            vring_bytes: AtomicI64::new(0),
            aring: Mutex::new(Vec::new()),
            aring_bytes: AtomicI64::new(0),
            mring: Mutex::new(Vec::new()),
            mring_bytes: AtomicI64::new(0),
            raw: Mutex::new(VecDeque::new()),
            raw_wait: Condvar::new(),
            active_encoder: AtomicI32::new(0),
            hw_init_hr: AtomicI32::new(0),
            gpu_mode: AtomicI32::new(0),
            gpu: Mutex::new(None),
            gpu_gen: AtomicI64::new(0),
            gpu_frames: AtomicI64::new(0),
            enc_resolved: AtomicBool::new(false),
            enc_fail_hr: AtomicI32::new(0),
            keyframe_pending: Mutex::new(true),
            audio_next_pts: Mutex::new(0),
            mic_next_pts: Mutex::new(0),
            min_gap_100ns: AtomicI64::new(0),
            last_capture_pts: AtomicI64::new(0),
            t_capture_ns: AtomicI64::new(0),
            n_capture: AtomicI64::new(0),
            n_wgc: AtomicI64::new(0),
            t_wgc_gap_ns: AtomicI64::new(0),
            t_buf_ns: AtomicI64::new(0),
            t_copy_ns: AtomicI64::new(0),
            n_frame_diff: AtomicI64::new(0),
            n_frame_same: AtomicI64::new(0),
            n_gap: std::array::from_fn(|_| AtomicI64::new(0)),
            n_gap_max_ns: AtomicI64::new(0),
            n_staging_alloc: AtomicI64::new(0),
            capture_method: AtomicI32::new(0),
            n_dxgi_timeout: AtomicI64::new(0),
            n_dxgi_access_lost: AtomicI64::new(0),
            n_dxgi_fatal: AtomicI64::new(0),
            t_wait_ns: AtomicI64::new(0),
            t_convert_ns: AtomicI64::new(0),
            t_encode_ns: AtomicI64::new(0),
            n_encode_calls: AtomicI64::new(0),
            n_queue_drop: AtomicI64::new(0),
            n_superseded: AtomicI64::new(0),
            n_slot_resync: AtomicI64::new(0),
            bgra_pool: Mutex::new(Vec::new()),
            n_capture_drop: AtomicI64::new(0),
        })
    }

    pub fn take_bgra(&self) -> Vec<u8> {
        self.bgra_pool
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pop()
            .unwrap_or_default()
    }

    pub fn recycle_bgra(&self, mut v: Vec<u8>) {
        const MAX_POOLED: usize = 3;
        if v.capacity() == 0 {
            return;
        }
        let mut pool = self.bgra_pool.lock().unwrap_or_else(|e| e.into_inner());
        if pool.len() < MAX_POOLED {
            v.clear();
            pool.push(v);
        }
    }

    pub fn push_frame(&self, frame: RawFrame) {
        let mut q = self.raw.lock().unwrap_or_else(|e| e.into_inner());
        if q.len() >= MAX_RAW_QUEUE {
            q.pop_front();
            self.n_queue_drop.fetch_add(1, Ordering::Relaxed);
            self.frames_dropped.fetch_add(1, Ordering::Relaxed);
        }
        q.push_back(frame);
        self.raw_wait.notify_one();
    }

    pub fn push_audio(&self, data: &[u8], frames: u32) {
        if data.is_empty() || frames == 0 {
            return;
        }
        let dur = (frames as i64 * SECOND_100NS) / AUDIO_RATE as i64;
        let pts = {
            let mut next = self.audio_next_pts.lock().unwrap_or_else(|e| e.into_inner());
            let cursor = *next;
            let elapsed = qpc100ns() - self.start_qpc.load(Ordering::Relaxed);
            let pts = if cursor == 0 {
                elapsed.max(0)
            } else if (elapsed - cursor).abs() > AUDIO_PTS_RESYNC {
                elapsed.max(cursor)
            } else {
                cursor
            };
            *next = pts + dur;
            pts
        };
        let mut ring = self.aring.lock().unwrap_or_else(|e| e.into_inner());
        ring.push(RingAudio {
            data: data.to_vec(),
            frames,
            pts100ns: pts,
        });
        self.aring_bytes
            .fetch_add(data.len() as i64, Ordering::Relaxed);
        self.audio_frames
            .fetch_add(frames as i64, Ordering::Relaxed);
        self.audio_got_frames.store(1, Ordering::Relaxed);
        self.sys_peak.fetch_max(peak_i16(data, 4), Ordering::Relaxed);
        let secs = self.ring_seconds.load(Ordering::Relaxed);
        trim_audio_ring(&mut ring, &self.aring_bytes, secs);
    }

    pub fn push_mic(&self, data: &[u8], frames: u32) {
        if data.is_empty() || frames == 0 {
            return;
        }
        let dur = (frames as i64 * SECOND_100NS) / AUDIO_RATE as i64;
        let pts = {
            let mut next = self.mic_next_pts.lock().unwrap_or_else(|e| e.into_inner());
            let cursor = *next;
            let elapsed = qpc100ns() - self.start_qpc.load(Ordering::Relaxed);
            let pts = if cursor == 0 {
                elapsed.max(0)
            } else if (elapsed - cursor).abs() > AUDIO_PTS_RESYNC {
                elapsed.max(cursor)
            } else {
                cursor
            };
            *next = pts + dur;
            pts
        };
        let mut ring = self.mring.lock().unwrap_or_else(|e| e.into_inner());
        ring.push(RingAudio {
            data: data.to_vec(),
            frames,
            pts100ns: pts,
        });
        self.mring_bytes
            .fetch_add(data.len() as i64, Ordering::Relaxed);
        self.mic_frames
            .fetch_add(frames as i64, Ordering::Relaxed);
        self.mic_got_frames.store(1, Ordering::Relaxed);
        self.mic_peak.fetch_max(peak_i16(data, 2), Ordering::Relaxed);
        let secs = self.ring_seconds.load(Ordering::Relaxed);
        trim_audio_ring(&mut ring, &self.mring_bytes, secs);
    }

    pub fn reset_for_start(&self, ring_seconds: i32, fps: i32, res_w: i32, res_h: i32, start_qpc: i64) {
        self.ring_seconds.store(ring_seconds, Ordering::Relaxed);
        self.fps.store(fps, Ordering::Relaxed);
        self.min_gap_100ns.store(
            if fps > 0 { SECOND_100NS / fps as i64 } else { 0 },
            Ordering::Relaxed,
        );
        self.last_capture_pts.store(0, Ordering::Relaxed);
        self.res_w.store(res_w, Ordering::Relaxed);
        self.res_h.store(res_h, Ordering::Relaxed);
        self.start_qpc.store(start_qpc, Ordering::Relaxed);
        self.frames_encoded.store(0, Ordering::Relaxed);
        self.frames_dropped.store(0, Ordering::Relaxed);
        self.audio_frames.store(0, Ordering::Relaxed);
        self.audio_events.store(0, Ordering::Relaxed);
        self.audio_got_frames.store(0, Ordering::Relaxed);
        self.audio_fail_hr.store(0, Ordering::Relaxed);
        self.mic_frames.store(0, Ordering::Relaxed);
        self.mic_events.store(0, Ordering::Relaxed);
        self.mic_got_frames.store(0, Ordering::Relaxed);
        self.mic_fail_hr.store(0, Ordering::Relaxed);
        self.mic_active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.sys_peak.store(0, Ordering::Relaxed);
        self.mic_peak.store(0, Ordering::Relaxed);
        self.health_heartbeat.store(0, Ordering::Relaxed);
        {
            let mut v = self.vring.lock().unwrap_or_else(|e| e.into_inner());
            v.clear();
        }
        self.vring_bytes.store(0, Ordering::Relaxed);
        {
            let mut a = self.aring.lock().unwrap_or_else(|e| e.into_inner());
            a.clear();
        }
        self.aring_bytes.store(0, Ordering::Relaxed);
        {
            let mut m = self.mring.lock().unwrap_or_else(|e| e.into_inner());
            m.clear();
        }
        self.mring_bytes.store(0, Ordering::Relaxed);
        {
            let mut r = self.raw.lock().unwrap_or_else(|e| e.into_inner());
            r.clear();
        }
        *self.gpu.lock().unwrap_or_else(|e| e.into_inner()) = None;
        self.gpu_gen.store(0, Ordering::Relaxed);
        self.gpu_frames.store(0, Ordering::Relaxed);
        self.enc_resolved.store(false, Ordering::Relaxed);
        self.enc_fail_hr.store(0, Ordering::Relaxed);
        *self.keyframe_pending.lock().unwrap_or_else(|e| e.into_inner()) = true;
        *self.audio_next_pts.lock().unwrap_or_else(|e| e.into_inner()) = 0;
        *self.mic_next_pts.lock().unwrap_or_else(|e| e.into_inner()) = 0;
    }
}

fn trim_video_ring(ring: &mut Vec<RingVideo>, shared: &SharedState, ring_seconds: i32) {
    if ring.is_empty() {
        return;
    }
    if ring_seconds > 0 {
        let window = ring_seconds as i64 * SECOND_100NS;
        let newest = ring.last().unwrap().pts100ns;
        let cutoff = newest - window;
        while ring.len() > 1 && ring[0].pts100ns < cutoff {
            let n = ring.remove(0).data.len() as i64;
            shared.vring_bytes.fetch_sub(n, Ordering::Relaxed);
        }
    }
    while ring.len() > 1 && shared.vring_bytes.load(Ordering::Relaxed) > RING_BUDGET_BYTES {
        let n = ring.remove(0).data.len() as i64;
        shared.vring_bytes.fetch_sub(n, Ordering::Relaxed);
    }
}

fn peak_i16(data: &[u8], stride: usize) -> i32 {
    let mut peak = 0i32;
    let mut i = 0usize;
    while i + 1 < data.len() {
        let v = i16::from_le_bytes([data[i], data[i + 1]]) as i32;
        let m = v.abs();
        if m > peak {
            peak = m;
        }
        i += stride;
    }
    peak
}

fn trim_audio_ring(ring: &mut Vec<RingAudio>, bytes: &AtomicI64, ring_seconds: i32) {
    if ring.is_empty() || ring_seconds <= 0 {
        return;
    }
    let window = ring_seconds as i64 * SECOND_100NS;
    let last = ring.last().unwrap();
    let last_dur = (last.frames as i64 * SECOND_100NS) / AUDIO_RATE as i64;
    let cutoff = last.pts100ns + last_dur - window;
    while ring.len() > 1 {
        let f = &ring[0];
        let dur = (f.frames as i64 * SECOND_100NS) / AUDIO_RATE as i64;
        if f.pts100ns + dur < cutoff {
            let n = ring.remove(0).data.len() as i64;
            bytes.fetch_sub(n, Ordering::Relaxed);
        } else {
            break;
        }
    }
}

struct EncoderConfig {
    pref: encoder::EncoderPref,
    w: i32,
    h: i32,
    fps: i32,
    bps: u32,
    gpu: bool,
}

enum GpuExit {
    Stop,
    Fallback(i32),
    Rebuild(Arc<encoder::GpuInput>),
}

fn wait_gpu_input(shared: &Arc<SharedState>, timeout: std::time::Duration) -> Option<Arc<encoder::GpuInput>> {
    let deadline = Instant::now() + timeout;
    loop {
        if !shared.running.load(Ordering::Relaxed) {
            return None;
        }
        if let Some(g) = shared.gpu.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            return Some(g);
        }
        if Instant::now() >= deadline {
            return None;
        }
        let q = shared.raw.lock().unwrap_or_else(|e| e.into_inner());
        let _ = shared
            .raw_wait
            .wait_timeout(q, std::time::Duration::from_millis(20))
            .unwrap_or_else(|e| e.into_inner());
    }
}

fn encoder_thread(
    shared: Arc<SharedState>,
    cfg: EncoderConfig,
    init_tx: std::sync::mpsc::Sender<Result<(encoder::EncoderKind, i32), i32>>,
) {
    let improved = std::env::var("IR_IMPROVED_CONVERT").is_ok_and(|v| v == "1");
    let mut init_tx = Some(init_tx);
    let mut gpu_enc: Option<encoder::GpuEncoder> = None;

    if cfg.gpu {
        if let Some(tx) = init_tx.take() {
            let _ = tx.send(Ok((encoder::EncoderKind::HardwareGpu, 0)));
        }
        match wait_gpu_input(&shared, std::time::Duration::from_secs(4)) {
            Some(input) => match encoder::GpuEncoder::create(input, cfg.w, cfg.h, cfg.fps, cfg.bps, improved) {
                Ok(g) => gpu_enc = Some(g),
                Err(hr) => shared.hw_init_hr.store(hr, Ordering::Relaxed),
            },
            None => shared.hw_init_hr.store(-110, Ordering::Relaxed),
        }
        if gpu_enc.is_none() {
            shared.gpu_mode.store(0, Ordering::Relaxed);
            *shared.gpu.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
    }

    let mut cpu_enc: Option<encoder::VideoEncoder> = None;
    if gpu_enc.is_none() {
        match encoder::VideoEncoder::create(cfg.pref, cfg.w, cfg.h, cfg.fps, cfg.bps) {
            Ok(e) => {
                if let Some(tx) = init_tx.take() {
                    let _ = tx.send(Ok((e.kind(), e.hw_init_hr)));
                } else if e.hw_init_hr != 0 && shared.hw_init_hr.load(Ordering::Relaxed) == 0 {
                    shared.hw_init_hr.store(e.hw_init_hr, Ordering::Relaxed);
                }
                shared.active_encoder.store(e.kind() as i32, Ordering::Relaxed);
                cpu_enc = Some(e);
            }
            Err(hr) => {
                if let Some(tx) = init_tx.take() {
                    let _ = tx.send(Err(hr));
                } else {
                    shared.enc_fail_hr.store(hr, Ordering::Relaxed);
                }
                shared.enc_resolved.store(true, Ordering::Release);
                return;
            }
        }
    } else {
        shared
            .active_encoder
            .store(encoder::EncoderKind::HardwareGpu as i32, Ordering::Relaxed);
    }
    drop(init_tx);
    shared.enc_resolved.store(true, Ordering::Release);

    loop {
        if let Some(g) = gpu_enc.as_mut() {
            match gpu_loop(&shared, &cfg, g) {
                GpuExit::Stop => return,
                GpuExit::Rebuild(input) => {
                    drop(gpu_enc.take());
                    match encoder::GpuEncoder::create(input, cfg.w, cfg.h, cfg.fps, cfg.bps, improved) {
                        Ok(n) => {
                            gpu_enc = Some(n);
                            continue;
                        }
                        Err(hr) => shared.hw_init_hr.store(hr, Ordering::Relaxed),
                    }
                }
                GpuExit::Fallback(hr) => {
                    shared.hw_init_hr.store(hr, Ordering::Relaxed);
                    drop(gpu_enc.take());
                }
            }
            shared.gpu_mode.store(0, Ordering::Relaxed);
            *shared.gpu.lock().unwrap_or_else(|e| e.into_inner()) = None;
            match encoder::VideoEncoder::create(cfg.pref, cfg.w, cfg.h, cfg.fps, cfg.bps) {
                Ok(e) => {
                    shared.active_encoder.store(e.kind() as i32, Ordering::Relaxed);
                    cpu_enc = Some(e);
                    *shared.keyframe_pending.lock().unwrap_or_else(|e| e.into_inner()) = true;
                }
                Err(_) => return,
            }
        } else if let Some(c) = cpu_enc.as_mut() {
            cpu_loop(&shared, c);
            return;
        } else {
            return;
        }
    }
}

fn gpu_loop(shared: &Arc<SharedState>, cfg: &EncoderConfig, enc: &mut encoder::GpuEncoder) -> GpuExit {
    let gap = {
        let g = shared.min_gap_100ns.load(Ordering::Relaxed);
        if g > 0 { g } else { SECOND_100NS / cfg.fps.max(1) as i64 }
    };
    let start_qpc = shared.start_qpc.load(Ordering::Relaxed);
    let mut out: Vec<encoder::EncodedFrame> = Vec::new();
    let mut next_slot: i64 = qpc100ns() - start_qpc;
    let mut seen = shared.gpu_frames.load(Ordering::Relaxed);
    let mut seen_gen = shared.gpu_gen.load(Ordering::Relaxed);
    let mut fails = 0u32;

    loop {
        if !shared.running.load(Ordering::Relaxed) {
            return GpuExit::Stop;
        }
        let now = qpc100ns() - start_qpc;
        if now < next_slot {
            let remaining_ns = ((next_slot - now) * 100).max(0) as u64;
            let t0 = Instant::now();
            let q = shared.raw.lock().unwrap_or_else(|e| e.into_inner());
            let _ = shared
                .raw_wait
                .wait_timeout(q, std::time::Duration::from_nanos(remaining_ns))
                .unwrap_or_else(|e| e.into_inner());
            shared
                .t_wait_ns
                .fetch_add(t0.elapsed().as_nanos() as i64, Ordering::Relaxed);
            continue;
        }

        let frames = shared.gpu_frames.load(Ordering::Relaxed);
        if frames > seen + 1 {
            shared.n_superseded.fetch_add(frames - seen - 1, Ordering::Relaxed);
        }
        seen = frames;

        let pts = next_slot;
        next_slot += gap;
        let after = qpc100ns() - start_qpc;
        if after > next_slot {
            shared.n_slot_resync.fetch_add(1, Ordering::Relaxed);
            next_slot = after + gap;
        }

        let gen = shared.gpu_gen.load(Ordering::Relaxed);
        if gen != seen_gen {
            seen_gen = gen;
            let Some(new_in) = shared.gpu.lock().unwrap_or_else(|e| e.into_inner()).clone() else {
                return GpuExit::Fallback(-111);
            };
            if enc.device_differs(&new_in) {
                return GpuExit::Rebuild(new_in);
            }
            if enc.rebind(new_in).is_err() {
                return GpuExit::Fallback(-112);
            }
        }

        let pending = *shared.keyframe_pending.lock().unwrap_or_else(|e| e.into_inner());
        if pending {
            enc.force_idr();
            *shared.keyframe_pending.lock().unwrap_or_else(|e| e.into_inner()) = false;
        }

        out.clear();
        let t_enc = Instant::now();
        let result = enc.encode(pts, &mut out);
        let total_ns = t_enc.elapsed().as_nanos() as i64;
        let conv_ns = enc.take_convert_ns();
        shared.t_convert_ns.fetch_add(conv_ns, Ordering::Relaxed);
        shared
            .t_encode_ns
            .fetch_add((total_ns - conv_ns).max(0), Ordering::Relaxed);
        shared.n_encode_calls.fetch_add(1, Ordering::Relaxed);

        match result {
            Ok(()) => fails = 0,
            Err(code) => {
                shared.frames_dropped.fetch_add(1, Ordering::Relaxed);
                fails += 1;
                if fails >= 8 {
                    return GpuExit::Fallback(code);
                }
            }
        }
        for encoded in out.drain(..) {
            push_encoded(shared, encoded);
        }
    }
}

fn cpu_loop(shared: &Arc<SharedState>, enc: &mut encoder::VideoEncoder) {
    let mut out: Vec<encoder::EncodedFrame> = Vec::new();

    let min_gap = shared.min_gap_100ns.load(Ordering::Relaxed).max(0);
    let paced = min_gap > 0;
    let start_qpc = shared.start_qpc.load(Ordering::Relaxed);
    let mut next_slot: i64 = 0;
    let mut primed = false;
    let mut last: Option<RawFrame> = None;

    'slots: loop {
        let mut got: Option<RawFrame> = None;
        {
            let mut q = shared.raw.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if !shared.running.load(Ordering::Relaxed) {
                    break 'slots;
                }
                if primed {
                    let now = qpc100ns() - start_qpc;
                    if now >= next_slot {
                        while let Some(f) = q.pop_front() {
                            if got.is_some() {
                                shared.n_superseded.fetch_add(1, Ordering::Relaxed);
                            }
                            got = Some(f);
                        }
                        if got.is_some() {
                            break;
                        }
                        break;
                    }
                    let remaining_ns = ((next_slot - now) * 100).max(0) as u64;
                    let t0 = Instant::now();
                    let (g, _) = shared
                        .raw_wait
                        .wait_timeout(q, std::time::Duration::from_nanos(remaining_ns))
                        .unwrap_or_else(|e| e.into_inner());
                    shared
                        .t_wait_ns
                        .fetch_add(t0.elapsed().as_nanos() as i64, Ordering::Relaxed);
                    q = g;
                    continue;
                }
                if let Some(f) = q.pop_front() {
                    got = Some(f);
                    break;
                }
                let t0 = Instant::now();
                q = shared.raw_wait.wait(q).unwrap_or_else(|e| e.into_inner());
                shared
                    .t_wait_ns
                    .fetch_add(t0.elapsed().as_nanos() as i64, Ordering::Relaxed);
            }
        }

        if !shared.running.load(Ordering::Relaxed) {
            break 'slots;
        }

        let pts = if let Some(f) = got.as_ref() {
            if paced {
                let p = if primed { next_slot } else { f.pts100ns };
                if primed {
                    next_slot += min_gap;
                } else {
                    next_slot = f.pts100ns + min_gap;
                    primed = true;
                }
                let now = qpc100ns() - start_qpc;
                if now > next_slot {
                    shared.n_slot_resync.fetch_add(1, Ordering::Relaxed);
                    next_slot = now + min_gap;
                }
                p
            } else {
                f.pts100ns
            }
        } else if paced {
            let p = next_slot;
            next_slot += min_gap;
            p
        } else {
            continue;
        };

        let pending = *shared.keyframe_pending.lock().unwrap_or_else(|e| e.into_inner());
        if pending {
            enc.force_idr();
            *shared.keyframe_pending.lock().unwrap_or_else(|e| e.into_inner()) = false;
        }

        if let Some(f) = got.take() {
            if let Some(old) = last.replace(f) {
                shared.recycle_bgra(old.bgra);
            }
        }
        let src = last.as_ref().expect("a slot always has a frame to encode");

        out.clear();
        let t_enc = Instant::now();
        let result = enc.encode_bgra(&src.bgra, src.w, src.h, src.w * 4, pts, &mut out);
        let total_ns = t_enc.elapsed().as_nanos() as i64;
        let conv_ns = enc.take_convert_ns();
        shared.t_convert_ns.fetch_add(conv_ns, Ordering::Relaxed);
        shared
            .t_encode_ns
            .fetch_add((total_ns - conv_ns).max(0), Ordering::Relaxed);
        shared.n_encode_calls.fetch_add(1, Ordering::Relaxed);
        shared.active_encoder.store(enc.kind() as i32, Ordering::Relaxed);

        if result.is_err() {
            shared.frames_dropped.fetch_add(1, Ordering::Relaxed);
        }

        for encoded in out.drain(..) {
            push_encoded(&shared, encoded);
        }
    }

    out.clear();
    enc.flush(&mut out);
    for encoded in out.drain(..) {
        push_encoded(&shared, encoded);
    }
}

fn push_encoded(shared: &Arc<SharedState>, encoded: encoder::EncodedFrame) {
    let bytes = encoded.data.len() as i64;
    let mut ring = shared.vring.lock().unwrap_or_else(|e| e.into_inner());
    ring.push(RingVideo {
        data: encoded.data,
        keyframe: encoded.keyframe,
        pts100ns: encoded.pts100ns,
    });
    shared.vring_bytes.fetch_add(bytes, Ordering::Relaxed);
    let secs = shared.ring_seconds.load(Ordering::Relaxed);
    trim_video_ring(&mut ring, shared, secs);
    drop(ring);
    shared.frames_encoded.fetch_add(1, Ordering::Relaxed);
    shared.health_heartbeat.fetch_add(1, Ordering::Relaxed);
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DebugStats {
    pub t_capture_ns: i64,
    pub n_capture: i64,
    pub n_wgc: i64,
    pub t_wgc_gap_ns: i64,
    pub t_buf_ns: i64,
    pub t_copy_ns: i64,
    pub n_frame_diff: i64,
    pub n_frame_same: i64,
    pub n_gap: [i64; capture::GAP_BUCKETS],
    pub n_gap_max_ns: i64,
    pub n_staging_alloc: i64,
    pub capture_method: i32,
    pub n_dxgi_timeout: i64,
    pub n_dxgi_access_lost: i64,
    pub n_dxgi_fatal: i64,
    pub t_wait_ns: i64,
    pub t_convert_ns: i64,
    pub t_encode_ns: i64,
    pub n_encode_calls: i64,
    pub n_queue_drop: i64,
    pub n_superseded: i64,
    pub n_slot_resync: i64,
    pub n_capture_drop: i64,
}

pub struct Engine {
    op: Mutex<()>,
    shared: Arc<SharedState>,
    last_hr: AtomicI64,
    fail_stage: AtomicI64,
    fail_hr: AtomicI64,
    capture: Mutex<Option<capture::CaptureSession>>,
    enc_thread: Mutex<Option<JoinHandle<()>>>,
    audio_thread: Mutex<Option<JoinHandle<()>>>,
    mic_thread: Mutex<Option<JoinHandle<()>>>,
}

impl Engine {
    pub fn new() -> Self {
        Engine {
            op: Mutex::new(()),
            shared: SharedState::new(),
            last_hr: AtomicI64::new(0),
            fail_stage: AtomicI64::new(0),
            fail_hr: AtomicI64::new(0),
            capture: Mutex::new(None),
            enc_thread: Mutex::new(None),
            audio_thread: Mutex::new(None),
            mic_thread: Mutex::new(None),
        }
    }

    pub fn is_running(&self) -> bool {
        self.shared.running.load(Ordering::Relaxed)
    }

    fn note_fail(&self, stage: i64, hr: i64) {
        self.fail_stage.store(stage, Ordering::Relaxed);
        self.fail_hr.store(hr, Ordering::Relaxed);
        self.last_hr.store(hr, Ordering::Relaxed);
    }

    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        &self,
        monitor_index: i32,
        ring_seconds: i32,
        fps: i32,
        bitrate_kbps: i32,
        cap_w: i32,
        cap_h: i32,
        capture_audio: i32,
        capture_mic: i32,
        mic_device: Option<&str>,
        encoder_pref: i32,
        capture_method: i32,
        start_qpc_out: &mut i64,
    ) -> i32 {
        let _guard = self.op.lock().unwrap_or_else(|e| e.into_inner());
        if self.shared.running.load(Ordering::Relaxed) {
            *start_qpc_out = self.shared.start_qpc.load(Ordering::Relaxed);
            return IR_OK;
        }

        let Some(mon) = capture::find_monitor(monitor_index) else {
            self.note_fail(STAGE_CREATE_ITEM, 0);
            return IR_FAIL;
        };
        let (res_w, res_h) = fit_resolution(mon.w, mon.h, cap_w, cap_h);
        let fps = fps.clamp(15, 240);
        let pref = encoder::EncoderPref::from_i32(encoder_pref).resolved();
        let gpu_path = pref != encoder::EncoderPref::Software
            && capture::CaptureMethod::from_i32(capture_method) == capture::CaptureMethod::Wgc
            && std::env::var("IR_GPU").map_or(true, |v| v != "0");
        let bps = (bitrate_kbps.max(1) as i32).max(1) as u32 * 1000;

        let start_qpc = qpc100ns();
        *start_qpc_out = start_qpc;
        self.shared
            .reset_for_start(ring_seconds.max(1), fps, res_w, res_h, start_qpc);
        self.shared.gpu_mode.store(gpu_path as i32, Ordering::Relaxed);
        self.shared.running.store(true, Ordering::Relaxed);
        self.fail_stage.store(0, Ordering::Relaxed);
        self.fail_hr.store(0, Ordering::Relaxed);
        self.last_hr.store(0, Ordering::Relaxed);

        {
            let mut slot = self.enc_thread.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(t) = slot.take() {
                let _ = t.join();
            }
            let shared = self.shared.clone();
            let cfg = EncoderConfig {
                pref,
                gpu: gpu_path,
                w: res_w,
                h: res_h,
                fps,
                bps,
            };
            let (tx, rx) = std::sync::mpsc::channel();
            let spawned = std::thread::Builder::new()
                .name("ir-encode".into())
                .spawn(move || encoder_thread(shared, cfg, tx));
            match spawned {
                Ok(t) => *slot = Some(t),
                Err(_) => {
                    self.shared.running.store(false, Ordering::Relaxed);
                    self.note_fail(STAGE_ENCODER, -1);
                    return IR_FAIL;
                }
            }
            match rx.recv_timeout(std::time::Duration::from_secs(15)) {
                Ok(Ok((kind, hw_hr))) => {
                    self.shared.active_encoder.store(kind as i32, Ordering::Relaxed);
                    self.shared.hw_init_hr.store(hw_hr, Ordering::Relaxed);
                }
                Ok(Err(hr)) => {
                    self.shared.running.store(false, Ordering::Relaxed);
                    self.shared.raw_wait.notify_all();
                    if let Some(t) = slot.take() {
                        let _ = t.join();
                    }
                    self.note_fail(STAGE_ENCODER, hr as i64);
                    return IR_FAIL;
                }
                Err(_) => {
                    self.shared.running.store(false, Ordering::Relaxed);
                    self.shared.raw_wait.notify_all();
                    self.note_fail(STAGE_ENCODER, -2);
                    return IR_FAIL;
                }
            }
        }

        {
            let mut slot = self.capture.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(mut old) = slot.take() {
                old.stop();
            }
            match capture::CaptureSession::start(
                self.shared.clone(),
                monitor_index,
                capture::CaptureMethod::from_i32(capture_method),
            ) {
                Ok(s) => *slot = Some(s),
                Err(e) => {
                    self.shared.running.store(false, Ordering::Relaxed);
                    self.note_fail(STAGE_START_CAPTURE, -1);
                    let _ = e;
                    return IR_FAIL;
                }
            }
        }

        if gpu_path {
            let deadline = Instant::now() + std::time::Duration::from_secs(6);
            while !self.shared.enc_resolved.load(Ordering::Acquire) && Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            let hr = self.shared.enc_fail_hr.load(Ordering::Relaxed);
            if hr != 0 || !self.shared.enc_resolved.load(Ordering::Acquire) {
                self.shared.running.store(false, Ordering::Relaxed);
                self.shared.raw_wait.notify_all();
                if let Some(mut cap) = self.capture.lock().unwrap_or_else(|e| e.into_inner()).take() {
                    cap.stop();
                }
                self.note_fail(STAGE_ENCODER, hr as i64);
                return IR_FAIL;
            }
        }

        if capture_audio != 0 {
            let mut slot = self.audio_thread.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(t) = slot.take() {
                let _ = t.join();
            }
            *slot = Some(audio::spawn(self.shared.clone()));
        }

        if capture_mic != 0 && mic_device.map_or(false, |s| !s.trim().is_empty()) {
            let mut slot = self.mic_thread.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(t) = slot.take() {
                let _ = t.join();
            }
            *slot = Some(audio::spawn_mic(
                self.shared.clone(),
                mic_device.map(str::to_string),
            ));
            drop(slot);
            self.await_mic_ready();
        } else if capture_mic != 0 {
            self.shared.mic_fail_hr.store(-10, Ordering::Relaxed);
        }

        IR_OK
    }

    fn await_mic_ready(&self) {
        let deadline = Instant::now() + Duration::from_millis(MIC_READY_WAIT_MS);
        loop {
            let settled = self.shared.mic_fail_hr.load(Ordering::Relaxed) != 0
                || self.shared.mic_got_frames.load(Ordering::Relaxed) != 0;
            if settled
                || Instant::now() >= deadline
                || !self.shared.running.load(Ordering::Relaxed)
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    pub fn stop(&self) -> i32 {
        let _guard = self.op.lock().unwrap_or_else(|e| e.into_inner());
        if !self.shared.running.load(Ordering::Relaxed) {
            return IR_OK;
        }
        self.shared.running.store(false, Ordering::Relaxed);

        if let Some(mut cap) = self.capture.lock().unwrap_or_else(|e| e.into_inner()).take() {
            cap.stop();
        }
        if let Some(t) = self.audio_thread.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = t.join();
        }
        if let Some(t) = self.mic_thread.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = t.join();
        }
        self.shared.raw_wait.notify_all();
        if let Some(t) = self.enc_thread.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = t.join();
        }
        self.shared.active_encoder.store(0, Ordering::Relaxed);
        IR_OK
    }

    pub fn save(&self, path: &str, seconds_back: i32) -> i32 {
        let _guard = self.op.lock().unwrap_or_else(|e| e.into_inner());

        let video: Vec<muxer::VideoSample> = {
            let v = self.shared.vring.lock().unwrap_or_else(|e| e.into_inner());
            if v.is_empty() {
                return IR_NOT_ENOUGH_DATA;
            }
            v.iter()
                .map(|f| muxer::VideoSample {
                    data: f.data.clone(),
                    keyframe: f.keyframe,
                    pts100ns: f.pts100ns,
                })
                .collect()
        };
        let (sys, mic) = {
            let a = self.shared.aring.lock().unwrap_or_else(|e| e.into_inner());
            let m = self.shared.mring.lock().unwrap_or_else(|e| e.into_inner());
            let sys = a.clone();
            let mic = m.clone();
            (sys, mic)
        };
        let max_frames = (self.shared.ring_seconds.load(Ordering::Relaxed).max(1) as i64 + 1)
            * AUDIO_RATE as i64;
        let mixed = audio::mix(&sys, &mic, max_frames);
        let audio: Vec<muxer::AudioSample> = mixed
            .iter()
            .map(|f| muxer::AudioSample {
                data: f.data.clone(),
                frames: f.frames,
                pts100ns: f.pts100ns,
            })
            .collect();

        let fps = self.shared.fps.load(Ordering::Relaxed).max(1);
        let res_w = self.shared.res_w.load(Ordering::Relaxed).max(2) as u32;
        let res_h = self.shared.res_h.load(Ordering::Relaxed).max(2) as u32;
        let rc = muxer::mux_to_mp4(
            path,
            &video,
            &audio,
            AUDIO_RATE,
            AUDIO_CHANNELS,
            fps,
            res_w,
            res_h,
            seconds_back,
        );
        if rc != IR_OK {
            self.note_fail(STAGE_MUX, 0);
        }
        rc
    }

    #[allow(clippy::too_many_arguments)]
    pub fn status(
        &self,
        is_buffering: *mut i32,
        ring_seconds: *mut i32,
        frames_encoded: *mut i64,
        frames_dropped: *mut i64,
        has_error: *mut i32,
        audio_frames: *mut i64,
        audio_events: *mut i64,
        audio_got_frames: *mut i64,
        health_heartbeat: *mut i64,
    ) {
        let s = &self.shared;
        unsafe {
            if !is_buffering.is_null() {
                *is_buffering = s.running.load(Ordering::Relaxed) as i32;
            }
            if !ring_seconds.is_null() {
                *ring_seconds = s.ring_seconds.load(Ordering::Relaxed);
            }
            if !frames_encoded.is_null() {
                *frames_encoded = s.frames_encoded.load(Ordering::Relaxed);
            }
            if !frames_dropped.is_null() {
                *frames_dropped = s.frames_dropped.load(Ordering::Relaxed);
            }
            if !has_error.is_null() {
                *has_error = s.has_error.load(Ordering::Relaxed) as i32;
            }
            if !audio_frames.is_null() {
                *audio_frames = s.audio_frames.load(Ordering::Relaxed);
            }
            if !audio_events.is_null() {
                *audio_events = s.audio_events.load(Ordering::Relaxed);
            }
            if !audio_got_frames.is_null() {
                *audio_got_frames = s.audio_got_frames.load(Ordering::Relaxed);
            }
            if !health_heartbeat.is_null() {
                *health_heartbeat = s.health_heartbeat.load(Ordering::Relaxed);
            }
        }
    }

    pub fn last_hresult(&self) -> i32 {
        self.last_hr.load(Ordering::Relaxed) as i32
    }

    pub fn debug_stats(&self) -> DebugStats {
        let s = &self.shared;
        DebugStats {
            t_capture_ns: s.t_capture_ns.load(Ordering::Relaxed),
            n_capture: s.n_capture.load(Ordering::Relaxed),
            n_wgc: s.n_wgc.load(Ordering::Relaxed),
            t_wgc_gap_ns: s.t_wgc_gap_ns.load(Ordering::Relaxed),
            t_buf_ns: s.t_buf_ns.load(Ordering::Relaxed),
            t_copy_ns: s.t_copy_ns.load(Ordering::Relaxed),
            n_frame_diff: s.n_frame_diff.load(Ordering::Relaxed),
            n_frame_same: s.n_frame_same.load(Ordering::Relaxed),
            n_gap: std::array::from_fn(|i| s.n_gap[i].load(Ordering::Relaxed)),
            n_gap_max_ns: s.n_gap_max_ns.load(Ordering::Relaxed),
            n_staging_alloc: s.n_staging_alloc.load(Ordering::Relaxed),
            capture_method: s.capture_method.load(Ordering::Relaxed),
            n_dxgi_timeout: s.n_dxgi_timeout.load(Ordering::Relaxed),
            n_dxgi_access_lost: s.n_dxgi_access_lost.load(Ordering::Relaxed),
            n_dxgi_fatal: s.n_dxgi_fatal.load(Ordering::Relaxed),
            t_wait_ns: s.t_wait_ns.load(Ordering::Relaxed),
            t_convert_ns: s.t_convert_ns.load(Ordering::Relaxed),
            t_encode_ns: s.t_encode_ns.load(Ordering::Relaxed),
            n_encode_calls: s.n_encode_calls.load(Ordering::Relaxed),
            n_queue_drop: s.n_queue_drop.load(Ordering::Relaxed),
            n_superseded: s.n_superseded.load(Ordering::Relaxed),
            n_slot_resync: s.n_slot_resync.load(Ordering::Relaxed),
            n_capture_drop: s.n_capture_drop.load(Ordering::Relaxed),
        }
    }
    pub fn last_fail_stage(&self) -> i32 {
        self.fail_stage.load(Ordering::Relaxed) as i32
    }
    pub fn last_fail_hr(&self) -> i32 {
        self.fail_hr.load(Ordering::Relaxed) as i32
    }
}

fn fit_resolution(desk_w: i32, desk_h: i32, cap_w: i32, cap_h: i32) -> (i32, i32) {
    let mut w = desk_w;
    let mut h = desk_h;
    if cap_w > 0 && cap_h > 0 {
        let scale = (cap_w as f64 / w.max(1) as f64).min(cap_h as f64 / h.max(1) as f64);
        if scale < 1.0 {
            w = (w as f64 * scale) as i32;
            h = (h as f64 * scale) as i32;
        }
    } else if cap_w > 0 {
        let scale = cap_w as f64 / w.max(1) as f64;
        if scale < 1.0 {
            h = (h as f64 * scale) as i32;
            w = cap_w;
        }
    } else if cap_h > 0 {
        let scale = cap_h as f64 / h.max(1) as f64;
        if scale < 1.0 {
            w = (w as f64 * scale) as i32;
            h = cap_h;
        }
    }
    let w = (w.max(2) / 2) * 2;
    let h = (h.max(2) / 2) * 2;
    (w, h)
}

pub use muxer::{IR_FAIL, IR_NOT_ENOUGH_DATA, IR_OK};

macro_rules! guard {
    ($body:expr) => {
        match catch_unwind(AssertUnwindSafe(|| $body)) {
            Ok(v) => v,
            Err(_) => IR_FAIL,
        }
    };
}

#[no_mangle]
pub unsafe extern "C" fn IR_Create(out_engine: *mut *mut Engine) -> i32 {
    guard!({
        if out_engine.is_null() {
            return IR_FAIL;
        }
        *out_engine = Box::into_raw(Box::new(Engine::new()));
        IR_OK
    })
}

#[no_mangle]
pub unsafe extern "C" fn IR_Destroy(engine: *mut Engine) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if engine.is_null() {
            return;
        }
        let e = Box::from_raw(engine);
        e.stop();
    }));
}

#[allow(clippy::too_many_arguments)]
#[no_mangle]
pub unsafe extern "C" fn IR_Start(
    engine: *mut Engine,
    monitor_index: i32,
    ring_seconds: i32,
    fps: i32,
    bitrate_kbps: i32,
    cap_w: i32,
    cap_h: i32,
    capture_audio: i32,
    capture_mic: i32,
    mic_device: *const u16,
    encoder_pref: i32,
    capture_method: i32,
    start_qpc_100ns: *mut i64,
) -> i32 {
    guard!({
        if engine.is_null() {
            return IR_FAIL;
        }
        let mut len = 0usize;
        if !mic_device.is_null() {
            while *mic_device.add(len) != 0 {
                len += 1;
                if len > 32_767 {
                    break;
                }
            }
        }
        let device: Option<String> = if len == 0 {
            None
        } else {
            Some(String::from_utf16_lossy(std::slice::from_raw_parts(mic_device, len)))
        };
        let mut origin = 0i64;
        let rc = (*engine).start(
            monitor_index,
            ring_seconds,
            fps,
            bitrate_kbps,
            cap_w,
            cap_h,
            capture_audio,
            capture_mic,
            device.as_deref(),
            encoder_pref,
            capture_method,
            &mut origin,
        );
        if !start_qpc_100ns.is_null() {
            *start_qpc_100ns = origin;
        }
        rc
    })
}

#[no_mangle]
pub unsafe extern "C" fn IR_Stop(engine: *mut Engine) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !engine.is_null() {
            (*engine).stop();
        }
    }));
}

#[no_mangle]
pub unsafe extern "C" fn IR_Save(
    engine: *mut Engine,
    out_path: *const u16,
    seconds_back: i32,
) -> i32 {
    guard!({
        if engine.is_null() || out_path.is_null() {
            return IR_FAIL;
        }
        let mut len = 0usize;
        while *out_path.add(len) != 0 {
            len += 1;
            if len > 32_767 {
                return IR_FAIL;
            }
        }
        let path = String::from_utf16_lossy(std::slice::from_raw_parts(out_path, len));
        (*engine).save(&path, seconds_back)
    })
}

#[allow(clippy::too_many_arguments)]
#[no_mangle]
pub unsafe extern "C" fn IR_GetStatus(
    engine: *mut Engine,
    is_buffering: *mut i32,
    ring_seconds: *mut i32,
    frames_encoded: *mut i64,
    frames_dropped: *mut i64,
    has_error: *mut i32,
    audio_frames: *mut i64,
    audio_events: *mut i64,
    audio_got_frames: *mut i64,
    health_heartbeat: *mut i64,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if engine.is_null() {
            return;
        }
        (*engine).status(
            is_buffering,
            ring_seconds,
            frames_encoded,
            frames_dropped,
            has_error,
            audio_frames,
            audio_events,
            audio_got_frames,
            health_heartbeat,
        );
    }));
}

#[no_mangle]
pub unsafe extern "C" fn IR_GetMonitorCount() -> i32 {
    guard!(capture::enumerate_monitors().len() as i32)
}

#[allow(clippy::too_many_arguments)]
#[no_mangle]
pub unsafe extern "C" fn IR_GetMonitorInfo(
    index: i32,
    name: *mut u16,
    name_cap: i32,
    width: *mut i32,
    height: *mut i32,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let mons = capture::enumerate_monitors();
        if mons.is_empty() {
            return;
        }
        let idx = if index < 0 || index as usize >= mons.len() {
            0
        } else {
            index as usize
        };
        let m = mons[idx];
        if !width.is_null() {
            *width = m.w;
        }
        if !height.is_null() {
            *height = m.h;
        }
        if !name.is_null() && name_cap > 0 {
            let text: Vec<u16> = if m.primary {
                format!("Primary - {}x{}", m.w, m.h)
            } else {
                format!("{}x{}", m.w, m.h)
            }
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
            let n = text.len().min(name_cap as usize);
            std::ptr::copy_nonoverlapping(text.as_ptr(), name, n);
            if n < name_cap as usize {
                *name.add(n - 1) = 0;
            }
        }
    }));
}

#[no_mangle]
pub unsafe extern "C" fn IR_IsDxgiSupported() -> i32 {
    guard!({
        if capture::enumerate_monitors().is_empty() {
            0
        } else {
            1
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn IR_DxgiDuplicationSupported() -> i32 {
    guard!({ i32::from(capture_dxgi::probe_available()) })
}

#[no_mangle]
pub unsafe extern "C" fn IR_LastHresult(engine: *mut Engine) -> i32 {
    guard!({
        if engine.is_null() {
            return 0;
        }
        (*engine).last_hresult()
    })
}

#[no_mangle]
pub unsafe extern "C" fn IR_LastFailStage(engine: *mut Engine) -> i32 {
    guard!({
        if engine.is_null() {
            return 0;
        }
        (*engine).last_fail_stage()
    })
}

#[no_mangle]
pub unsafe extern "C" fn IR_LastFailHr(engine: *mut Engine) -> i32 {
    guard!({
        if engine.is_null() {
            return 0;
        }
        (*engine).last_fail_hr()
    })
}

#[no_mangle]
pub unsafe extern "C" fn IR_ActiveEncoder(engine: *mut Engine) -> i32 {
    guard!({
        if engine.is_null() {
            return 0;
        }
        let e = &*engine;
        e.shared.active_encoder.load(Ordering::Relaxed)
    })
}

#[no_mangle]
pub unsafe extern "C" fn IR_ActiveCaptureMethod(engine: *mut Engine) -> i32 {
    guard!({
        if engine.is_null() {
            return 0;
        }
        let e = &*engine;
        e.shared.capture_method.load(Ordering::Relaxed)
    })
}

#[no_mangle]
pub unsafe extern "C" fn IR_HwInitHr(engine: *mut Engine) -> i32 {
    guard!({
        if engine.is_null() {
            return 0;
        }
        let e = &*engine;
        e.shared.hw_init_hr.load(Ordering::Relaxed)
    })
}

#[no_mangle]
pub unsafe extern "C" fn IR_Version() -> *const c_char {
    b"rust-1.1\0".as_ptr() as *const c_char
}

unsafe fn copy_wide(text: &str, out: *mut u16, cap: i32) {
    if out.is_null() || cap <= 0 {
        return;
    }
    let cap = cap as usize;
    let units: Vec<u16> = text.encode_utf16().take(cap - 1).collect();
    for (i, u) in units.iter().enumerate() {
        *out.add(i) = *u;
    }
    *out.add(units.len()) = 0;
}

#[no_mangle]
pub unsafe extern "C" fn IR_GetMicDeviceCount() -> i32 {
    guard!(audio::mic_devices().len() as i32)
}

#[no_mangle]
pub unsafe extern "C" fn IR_GetMicDeviceName(index: i32, name: *mut u16, name_cap: i32) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if index < 0 {
            return;
        }
        let devices = audio::mic_devices();
        if let Some(text) = devices.get(index as usize) {
            copy_wide(text, name, name_cap);
        }
    }));
}

#[allow(clippy::too_many_arguments)]
#[no_mangle]
pub unsafe extern "C" fn IR_MicStatus(
    engine: *mut Engine,
    mic_frames: *mut i64,
    mic_events: *mut i64,
    mic_got_frames: *mut i64,
    mic_fail_hr: *mut i64,
    sys_peak: *mut i32,
    mic_peak: *mut i32,
    active_name: *mut u16,
    active_cap: i32,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if engine.is_null() {
            return;
        }
        let s = &(*engine).shared;
        if !sys_peak.is_null() {
            *sys_peak = s.sys_peak.load(Ordering::Relaxed);
        }
        if !mic_peak.is_null() {
            *mic_peak = s.mic_peak.load(Ordering::Relaxed);
        }
        if !mic_frames.is_null() {
            *mic_frames = s.mic_frames.load(Ordering::Relaxed);
        }
        if !mic_events.is_null() {
            *mic_events = s.mic_events.load(Ordering::Relaxed);
        }
        if !mic_got_frames.is_null() {
            *mic_got_frames = s.mic_got_frames.load(Ordering::Relaxed);
        }
        if !mic_fail_hr.is_null() {
            *mic_fail_hr = s.mic_fail_hr.load(Ordering::Relaxed);
        }
        if !active_name.is_null() && active_cap > 0 {
            let text = s
                .mic_active
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            copy_wide(&text, active_name, active_cap);
        }
    }));
}
