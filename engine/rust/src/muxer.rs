use std::io::Write;
use std::path::Path;

pub const SECOND_100NS: i64 = 10_000_000;

#[derive(Clone)]
pub struct VideoSample {
    pub data: Vec<u8>,
    pub keyframe: bool,
    pub pts100ns: i64,
}

#[derive(Clone)]
pub struct AudioSample {
    pub data: Vec<u8>,
    pub frames: u32,
    pub pts100ns: i64,
}

pub const IR_OK: i32 = 0;
pub const IR_FAIL: i32 = -1;
pub const IR_NOT_ENOUGH_DATA: i32 = -30;

struct BeWriter {
    v: Vec<u8>,
}

impl BeWriter {
    fn new() -> Self {
        BeWriter { v: Vec::new() }
    }
    fn u8(&mut self, b: u8) {
        self.v.push(b);
    }
    fn u16(&mut self, x: u16) {
        self.v.push((x >> 8) as u8);
        self.v.push(x as u8);
    }
    fn u32(&mut self, x: u32) {
        self.v.push((x >> 24) as u8);
        self.v.push((x >> 16) as u8);
        self.v.push((x >> 8) as u8);
        self.v.push(x as u8);
    }
    fn u64(&mut self, x: u64) {
        for i in (0..8).rev() {
            self.v.push((x >> (i * 8)) as u8);
        }
    }
    fn bytes(&mut self, b: &[u8]) {
        self.v.extend_from_slice(b);
    }
    fn fourcc(&mut self, cc: &[u8; 4]) {
        self.v.extend_from_slice(cc);
    }
    #[allow(dead_code)]
    fn box_start(&mut self, cc: &[u8; 4]) {
        self.u32(0);
        self.fourcc(cc);
    }
    #[allow(dead_code)]
    fn box_end(&mut self, start: usize) {
        let size = (self.v.len() - start) as u32;
        self.v[start..start + 4].copy_from_slice(&size.to_be_bytes());
    }
}

fn be_box(w: &mut BeWriter, size: u32, cc: &[u8; 4]) {
    w.u32(size);
    w.fourcc(cc);
}

fn write_mvhd(w: &mut BeWriter, timescale: u32, duration: u32) {
    be_box(w, 108, b"mvhd");
    w.u32(0);
    w.u32(0);
    w.u32(0);
    w.u32(timescale);
    w.u32(duration);
    w.u32(0x00010000);
    w.u16(0x0100);
    w.u16(0);
    w.u32(0);
    w.u32(0);
    w.u32(0x00010000);
    w.u32(0);
    w.u32(0);
    w.u32(0);
    w.u32(0x00010000);
    w.u32(0);
    w.u32(0);
    w.u32(0);
    w.u32(0x40000000);
    w.u64(0);
    w.u64(0);
    w.u64(0);
    w.u32(2);
}

fn write_mdhd(w: &mut BeWriter, timescale: u32, duration: u32) {
    be_box(w, 32, b"mdhd");
    w.u32(0);
    w.u32(0);
    w.u32(0);
    w.u32(timescale);
    w.u32(duration);
    w.u16(0);
    w.u16(0);
}

fn write_hdlr(w: &mut BeWriter, handler: &[u8; 4], name: &[u8]) {
    let content = 4 + 4 + 4 + 12 + name.len() + 1;
    be_box(w, 8 + content as u32, b"hdlr");
    w.u32(0);
    w.u32(0);
    w.fourcc(handler);
    w.u32(0);
    w.u32(0);
    w.u32(0);
    w.bytes(name);
    w.u8(0);
}

fn write_dinf(w: &mut BeWriter) {
    be_box(w, 36, b"dinf");
    be_box(w, 28, b"dref");
    w.u32(0);
    w.u32(1);
    be_box(w, 12, b"url ");
    w.u32(1);
}

fn write_stsc(w: &mut BeWriter) {
    be_box(w, 28, b"stsc");
    w.u32(0);
    w.u32(1);
    w.u32(1);
    w.u32(1);
    w.u32(1);
}

fn write_stco(w: &mut BeWriter, offsets: &[u32]) {
    be_box(w, 16 + offsets.len() as u32 * 4, b"stco");
    w.u32(0);
    w.u32(offsets.len() as u32);
    for o in offsets {
        w.u32(*o);
    }
}

fn rle_deltas(deltas: &[u32]) -> Vec<(u32, u32)> {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for &d in deltas {
        if let Some(last) = runs.last_mut() {
            if last.1 == d {
                last.0 += 1;
                continue;
            }
        }
        runs.push((1, d));
    }
    runs
}

fn write_stts(w: &mut BeWriter, runs: &[(u32, u32)]) {
    be_box(w, 16 + runs.len() as u32 * 8, b"stts");
    w.u32(0);
    w.u32(runs.len() as u32);
    for &(count, delta) in runs {
        w.u32(count);
        w.u32(delta);
    }
}

fn write_stss(w: &mut BeWriter, v: &[VideoSample]) {
    let keys: Vec<u32> = v
        .iter()
        .enumerate()
        .filter(|(_, s)| s.keyframe)
        .map(|(i, _)| (i + 1) as u32)
        .collect();
    be_box(w, 16 + keys.len() as u32 * 4, b"stss");
    w.u32(0);
    w.u32(keys.len() as u32);
    for k in keys {
        w.u32(k);
    }
}

fn write_stsz(w: &mut BeWriter, sizes: impl Iterator<Item = usize>, count: u32) {
    let sizes: Vec<u32> = sizes.map(|s| s as u32).collect();
    be_box(w, 20 + sizes.len() as u32 * 4, b"stsz");
    w.u32(0);
    w.u32(0);
    w.u32(count);
    for s in sizes {
        w.u32(s);
    }
}

fn write_stts_audio(w: &mut BeWriter, a: &[AudioSample]) {
    let frames: Vec<u32> = a.iter().map(|s| s.frames).collect();
    let runs = rle_deltas(&frames);
    write_stts(w, &runs);
}

fn find_start_code(data: &[u8], from: usize) -> Option<(usize, usize)> {
    let size = data.len();
    let mut i = from;
    while i + 3 <= size {
        if data[i] == 0 && data[i + 1] == 0 {
            if data[i + 2] == 1 {
                return Some((i, 3));
            }
            if i + 3 < size && data[i + 2] == 0 && data[i + 3] == 1 {
                return Some((i, 4));
            }
        }
        i += 1;
    }
    None
}

fn annexb_to_avcc(src: &[u8], out: &mut Vec<u8>) -> bool {
    out.clear();
    if src.len() < 4 {
        return false;
    }
    struct Part {
        start: usize,
        len: usize,
    }
    let mut parts: Vec<Part> = Vec::new();
    let mut total = 0usize;
    let mut i = 0usize;
    while i + 3 <= src.len() {
        let (sc, start_code_len) = match find_start_code(src, i) {
            Some(x) => x,
            None => break,
        };
        if sc + start_code_len >= src.len() {
            break;
        }
        let nal_end = find_start_code(src, sc + start_code_len)
            .map(|(p, _)| p)
            .unwrap_or(src.len());
        let len = nal_end - (sc + start_code_len);
        if len == 0 {
            i = nal_end;
            continue;
        }
        parts.push(Part {
            start: sc + start_code_len,
            len,
        });
        total += len + 4;
        i = nal_end;
    }
    if parts.is_empty() {
        return false;
    }
    out.reserve(total);
    for p in &parts {
        out.extend_from_slice(&(p.len as u32).to_be_bytes());
        out.extend_from_slice(&src[p.start..p.start + p.len]);
    }
    true
}

fn collect_sps_pps(src: &[u8], sps: &mut Vec<u8>, pps: &mut Vec<u8>) {
    if sps.len() >= 4 && !pps.is_empty() {
        return;
    }
    let mut i = 0usize;
    while i < src.len() && (sps.len() < 4 || pps.is_empty()) {
        let (sc, start_code_len) = match find_start_code(src, i) {
            Some(x) => x,
            None => break,
        };
        let nal_end = find_start_code(src, sc + start_code_len)
            .map(|(p, _)| p)
            .unwrap_or(src.len());
        let len = nal_end - (sc + start_code_len);
        if len > 0 {
            let nal_type = src[sc + start_code_len] & 0x1F;
            if nal_type == 7 && sps.len() < 4 {
                sps.clear();
                sps.extend_from_slice(&src[sc + start_code_len..sc + start_code_len + len]);
            } else if nal_type == 8 && pps.is_empty() {
                pps.clear();
                pps.extend_from_slice(&src[sc + start_code_len..sc + start_code_len + len]);
            }
        }
        i = nal_end;
    }
}

fn build_avcc(sps: &[u8], pps: &[u8], out: &mut Vec<u8>) -> bool {
    out.clear();
    if sps.len() < 4 || pps.is_empty() {
        return false;
    }
    out.reserve(11 + sps.len() + pps.len());
    out.push(1);
    out.push(sps[1]);
    out.push(sps[2]);
    out.push(sps[3]);
    out.push(0xFF);
    out.push(0xE1);
    out.extend_from_slice(&(sps.len() as u16).to_be_bytes());
    out.extend_from_slice(sps);
    out.push(1);
    out.extend_from_slice(&(pps.len() as u16).to_be_bytes());
    out.extend_from_slice(pps);
    true
}

fn write_avc1(w: &mut BeWriter, w_: u32, h_: u32, avcc: &[u8]) {
    be_box(w, 86 + 8 + avcc.len() as u32, b"avc1");
    for _ in 0..6 {
        w.u8(0);
    }
    w.u16(1);
    w.u16(0);
    w.u16(0);
    w.u32(0);
    w.u32(0);
    w.u32(0);
    w.u16(w_ as u16);
    w.u16(h_ as u16);
    w.u32(0x00480000);
    w.u32(0x00480000);
    w.u32(0);
    w.u16(1);
    for _ in 0..32 {
        w.u8(0);
    }
    w.u16(0x0018);
    w.u16(0xFFFF);
    be_box(w, 8 + avcc.len() as u32, b"avcC");
    w.bytes(avcc);
}

fn write_qt_sound(w: &mut BeWriter, rate: u32, ch: u32) {
    be_box(w, 36, b"sowt");
    for _ in 0..6 {
        w.u8(0);
    }
    w.u16(1);
    w.u16(0);
    w.u16(0);
    w.u32(0);
    w.u16(ch as u16);
    w.u16(16);
    w.u16(0);
    w.u16(0);
    w.u32(rate << 16);
}

fn write_stsd_video(w: &mut BeWriter, w_: u32, h_: u32, config: &[u8]) {
    let mut tmp = BeWriter::new();
    write_avc1(&mut tmp, w_, h_, config);
    be_box(w, 16 + tmp.v.len() as u32, b"stsd");
    w.u32(0);
    w.u32(1);
    w.bytes(&tmp.v);
}

fn write_stsd_audio(w: &mut BeWriter, rate: u32, ch: u32) {
    let mut tmp = BeWriter::new();
    write_qt_sound(&mut tmp, rate, ch);
    be_box(w, 16 + tmp.v.len() as u32, b"stsd");
    w.u32(0);
    w.u32(1);
    w.bytes(&tmp.v);
}

fn build_video_trak(w: &mut BeWriter, v: &[VideoSample], duration_ms: u32, w_: u32, h_: u32,
                    config: &[u8], offsets: &[u32], deltas_ms: &[u32]) {
    let mut inner = BeWriter::new();

    be_box(&mut inner, 92, b"tkhd");
    inner.u32(0x00000001);
    inner.u32(0);
    inner.u32(0);
    inner.u32(1);
    inner.u32(0);
    inner.u32(duration_ms);
    inner.u32(0);
    inner.u32(0);
    inner.u16(0);
    inner.u16(0);
    inner.u16(0);
    inner.u16(0);
    inner.u32(0x00010000);
    inner.u32(0);
    inner.u32(0);
    inner.u32(0);
    inner.u32(0x00010000);
    inner.u32(0);
    inner.u32(0);
    inner.u32(0);
    inner.u32(0x40000000);
    inner.u32(w_ << 16);
    inner.u32(h_ << 16);

    let mut mdia = BeWriter::new();
    write_mdhd(&mut mdia, 1000, duration_ms);
    write_hdlr(&mut mdia, b"vide", b"VideoHandler");
    {
        let mut minf = BeWriter::new();
        be_box(&mut minf, 20, b"vmhd");
        minf.u32(1);
        minf.u16(0);
        minf.u16(0);
        minf.u16(0);
        minf.u16(0);
        write_dinf(&mut minf);
        {
            let mut stbl = BeWriter::new();
            write_stsd_video(&mut stbl, w_, h_, config);
            write_stts(&mut stbl, &rle_deltas(deltas_ms));
            write_stss(&mut stbl, v);
            write_stsc(&mut stbl);
            write_stsz(&mut stbl, v.iter().map(|s| s.data.len()), v.len() as u32);
            write_stco(&mut stbl, offsets);
            be_box(&mut minf, 8 + stbl.v.len() as u32, b"stbl");
            minf.bytes(&stbl.v);
        }
        be_box(&mut mdia, 8 + minf.v.len() as u32, b"minf");
        mdia.bytes(&minf.v);
    }
    be_box(&mut inner, 8 + mdia.v.len() as u32, b"mdia");
    inner.bytes(&mdia.v);

    be_box(w, 8 + inner.v.len() as u32, b"trak");
    w.bytes(&inner.v);
}

fn build_audio_trak(w: &mut BeWriter, a: &[AudioSample], rate: u32, ch: u32, offsets: &[u32]) {
    let duration: u64 = a.iter().map(|s| s.frames as u64).sum();

    let mut inner = BeWriter::new();

    be_box(&mut inner, 92, b"tkhd");
    inner.u32(0x00000001);
    inner.u32(0);
    inner.u32(0);
    inner.u32(2);
    inner.u32(0);
    inner.u32(duration as u32);
    inner.u32(0);
    inner.u32(0);
    inner.u16(0);
    inner.u16(0);
    inner.u16(0x0100);
    inner.u16(0);
    inner.u32(0x00010000);
    inner.u32(0);
    inner.u32(0);
    inner.u32(0);
    inner.u32(0x00010000);
    inner.u32(0);
    inner.u32(0);
    inner.u32(0);
    inner.u32(0x40000000);
    inner.u32(0);
    inner.u32(0);

    let mut mdia = BeWriter::new();
    write_mdhd(&mut mdia, rate, duration as u32);
    write_hdlr(&mut mdia, b"soun", b"SoundHandler");
    {
        let mut minf = BeWriter::new();
        be_box(&mut minf, 16, b"smhd");
        minf.u32(0);
        minf.u16(0);
        minf.u16(0);
        write_dinf(&mut minf);
        {
            let mut stbl = BeWriter::new();
            write_stsd_audio(&mut stbl, rate, ch);
            write_stts_audio(&mut stbl, a);
            write_stsc(&mut stbl);
            write_stsz(&mut stbl, a.iter().map(|s| s.data.len()), a.len() as u32);
            write_stco(&mut stbl, offsets);
            be_box(&mut minf, 8 + stbl.v.len() as u32, b"stbl");
            minf.bytes(&stbl.v);
        }
        be_box(&mut mdia, 8 + minf.v.len() as u32, b"minf");
        mdia.bytes(&minf.v);
    }
    be_box(&mut inner, 8 + mdia.v.len() as u32, b"mdia");
    inner.bytes(&mdia.v);

    be_box(w, 8 + inner.v.len() as u32, b"trak");
    w.bytes(&inner.v);
}

fn build_moov(w: &mut BeWriter, v: &[VideoSample], a: &[AudioSample], a_rate: u32, a_ch: u32,
              w_: u32, h_: u32, config: &[u8], v_offsets: &[u32], a_offsets: &[u32],
              duration_ms: u32, deltas_ms: &[u32]) {
    let mut inner = BeWriter::new();
    write_mvhd(&mut inner, 1000, duration_ms);
    build_video_trak(&mut inner, v, duration_ms, w_, h_, config, v_offsets, deltas_ms);
    if !a.is_empty() {
        build_audio_trak(&mut inner, a, a_rate, a_ch, a_offsets);
    }
    be_box(w, 8 + inner.v.len() as u32, b"moov");
    w.bytes(&inner.v);
}

fn clip_audio_to_window(
    a: &[AudioSample],
    first_pts: i64,
    last_end: i64,
    a_sample_rate: u32,
    a_channels: u32,
) -> Vec<AudioSample> {
    let bpf = (a_channels.max(1) as usize) * 2;
    let arate = a_sample_rate.max(1) as i64;
    let mut out: Vec<AudioSample> = Vec::new();
    for c in a {
        if c.frames == 0 || c.data.is_empty() {
            continue;
        }
        let skip = if c.pts100ns < first_pts {
            (((first_pts - c.pts100ns) * arate + SECOND_100NS / 2) / SECOND_100NS)
                .clamp(0, c.frames as i64) as u32
        } else {
            0
        };
        let c_end = c.pts100ns + (c.frames as i64 * SECOND_100NS) / arate;
        let mut frames = c.frames - skip;
        if c_end > last_end {
            let over = ((c_end - last_end) * arate + SECOND_100NS / 2) / SECOND_100NS;
            frames = frames.saturating_sub(over.clamp(0, frames as i64) as u32);
        }
        if frames == 0 {
            continue;
        }
        let byte_off = (skip as usize).saturating_mul(bpf);
        let take = (frames as usize).saturating_mul(bpf);
        if byte_off >= c.data.len() {
            continue;
        }
        let data = &c.data[byte_off..(byte_off + take).min(c.data.len())];
        let frames = (data.len() / bpf) as u32;
        if frames == 0 {
            continue;
        }
        out.push(AudioSample {
            data: data.to_vec(),
            frames,
            pts100ns: c.pts100ns + (skip as i64 * SECOND_100NS) / arate,
        });
    }
    out
}

fn video_deltas_ms(v: &[VideoSample], fps: u32) -> Vec<u32> {
    let fallback = (1000u32).max(1) / fps.max(1);
    let n = v.len();
    if n == 0 {
        return vec![fallback.max(1)];
    }
    let t0 = v[0].pts100ns;
    let cum_ms = |i: usize| -> i64 { (v[i].pts100ns - t0).div_euclid(10_000) };

    let mut deltas: Vec<u32> = (0..n)
        .map(|i| {
            let d = if i + 1 < n { cum_ms(i + 1) - cum_ms(i) } else { 0 };
            if d >= 1 {
                d as u32
            } else {
                fallback.max(1)
            }
        })
        .collect();

    if let Some(prev) = deltas.iter().rev().skip(1).next().copied() {
        if let Some(last) = deltas.last_mut() {
            *last = prev;
        }
    }
    deltas
}

pub fn mux_to_mp4(path: &str, v: &[VideoSample], a: &[AudioSample], a_sample_rate: u32,
                  a_channels: u32, fps: i32, w_: u32, h_: u32, seconds_back: i32) -> i32 {
    if v.is_empty() {
        return IR_NOT_ENOUGH_DATA;
    }
    let w_ = w_.max(2);
    let h_ = h_.max(2);
    let fps = fps.max(1) as u32;

    let mut trim_start = i64::MIN;
    if seconds_back > 0 {
        let newest_end = v.last().map(|f| f.pts100ns).unwrap_or(0);
        trim_start = newest_end - seconds_back as i64 * SECOND_100NS;
    }

    let mut start = 0usize;
    if seconds_back > 0 {
        let mut before: Option<usize> = None;
        for (i, s) in v.iter().enumerate() {
            if s.pts100ns >= trim_start {
                break;
            }
            if s.keyframe {
                before = Some(i);
            }
        }
        start = match before {
            Some(i) => i,
            None => v.iter().position(|s| s.keyframe).unwrap_or(0),
        };
    }

    if v.len() - start < 2 {
        return IR_NOT_ENOUGH_DATA;
    }
    let first_pts = v[start].pts100ns;
    let last_end = v.last().unwrap().pts100ns;
    if last_end - first_pts < SECOND_100NS / 2 {
        return IR_NOT_ENOUGH_DATA;
    }

    let mut vsamples: Vec<VideoSample> = Vec::with_capacity(v.len() - start);
    let mut avcc: Vec<u8> = Vec::new();
    let mut sps: Vec<u8> = Vec::new();
    let mut pps: Vec<u8> = Vec::new();
    for s in &v[start..] {
        let mut data = Vec::new();
        if !annexb_to_avcc(&s.data, &mut data) {
            continue;
        }
        if avcc.is_empty() {
            collect_sps_pps(&s.data, &mut sps, &mut pps);
            if !sps.is_empty() && !pps.is_empty() {
                build_avcc(&sps, &pps, &mut avcc);
            }
        }
        vsamples.push(VideoSample {
            data,
            keyframe: s.keyframe,
            pts100ns: s.pts100ns,
        });
    }
    if avcc.is_empty() || vsamples.is_empty() {
        return IR_FAIL;
    }
    let config: &[u8] = &avcc;

    let asamples = clip_audio_to_window(a, first_pts, last_end, a_sample_rate, a_channels);

    let deltas_ms = video_deltas_ms(&vsamples, fps);
    let duration_ms: u32 = deltas_ms.iter().sum();

    let mut items: Vec<(bool, usize, i64)> = Vec::with_capacity(vsamples.len() + asamples.len());
    for i in 0..vsamples.len() {
        items.push((true, i, vsamples[i].pts100ns));
    }
    for (j, s) in asamples.iter().enumerate() {
        items.push((false, j, s.pts100ns));
    }
    items.sort_by(|x, y| {
        if x.2 != y.2 {
            x.2.cmp(&y.2)
        } else {
            x.0.cmp(&y.0).reverse()
        }
    });

    let ftyp_size: u32 = 8 + 4 + 4 + 4 * 4;
    let zero_v = vec![0u32; vsamples.len()];
    let zero_a = vec![0u32; asamples.len()];
    let mut moov0 = BeWriter::new();
    build_moov(&mut moov0, &vsamples, &asamples, a_sample_rate, a_channels, w_, h_, config,
               &zero_v, &zero_a, duration_ms, &deltas_ms);
    let mdat_start: u64 = ftyp_size as u64 + moov0.v.len() as u64;

    let mut v_offsets = vec![0u32; vsamples.len()];
    let mut a_offsets = vec![0u32; asamples.len()];
    let mut running: u64 = mdat_start + 8;
    for &(is_video, idx, _) in &items {
        if is_video {
            v_offsets[idx] = running as u32;
            running += vsamples[idx].data.len() as u64;
        } else {
            a_offsets[idx] = running as u32;
            running += asamples[idx].data.len() as u64;
        }
    }

    let mut moov = BeWriter::new();
    build_moov(&mut moov, &vsamples, &asamples, a_sample_rate, a_channels, w_, h_, config,
               &v_offsets, &a_offsets, duration_ms, &deltas_ms);
    if moov.v.len() != moov0.v.len() {
        return IR_FAIL;
    }

    let result = (|| -> std::io::Result<()> {
        let mut out = std::io::BufWriter::new(std::fs::File::create(Path::new(path))?);
        let mut ftyp = BeWriter::new();
        be_box(&mut ftyp, ftyp_size, b"ftyp");
        ftyp.fourcc(b"isom");
        ftyp.u32(512);
        ftyp.fourcc(b"isom");
        ftyp.fourcc(b"iso2");
        ftyp.fourcc(b"avc1");
        ftyp.fourcc(b"mp41");
        out.write_all(&ftyp.v)?;
        out.write_all(&moov.v)?;

        let mut mdath = BeWriter::new();
        be_box(&mut mdath, (running - mdat_start) as u32, b"mdat");
        out.write_all(&mdath.v)?;

        for &(is_video, idx, _) in &items {
            if is_video {
                out.write_all(&vsamples[idx].data)?;
            } else {
                out.write_all(&asamples[idx].data)?;
            }
        }
        out.flush()?;
        Ok(())
    })();

    match result {
        Ok(()) => IR_OK,
        Err(_) => {
            let _ = std::fs::remove_file(Path::new(path));
            IR_FAIL
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;
    const CH: u32 = 2;
    const BPF: usize = 4;

    fn pkt(pts_ms: i64, frames: u32, tag: u8) -> AudioSample {
        AudioSample {
            data: vec![tag; frames as usize * BPF],
            frames,
            pts100ns: pts_ms * 10_000,
        }
    }

    fn ms(v: i64) -> i64 {
        v * 10_000
    }

    #[test]
    fn keeps_packet_inside_window() {
        let a = vec![pkt(100, 480, 7)];
        let out = clip_audio_to_window(&a, ms(50), ms(200), RATE, CH);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].frames, 480);
        assert_eq!(out[0].pts100ns, ms(100));
    }

    #[test]
    fn trims_packet_straddling_start() {
        let a = vec![pkt(0, 4800, 3)];
        let out = clip_audio_to_window(&a, ms(50), ms(200), RATE, CH);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].frames, 2400, "should keep exactly the 50 ms inside");
        assert_eq!(out[0].pts100ns, ms(50), "must start at the window edge");
        assert!(out[0].data.iter().all(|&b| b == 3));
    }

    #[test]
    fn trims_packet_straddling_end() {
        let a = vec![pkt(100, 4800, 5)];
        let out = clip_audio_to_window(&a, ms(0), ms(150), RATE, CH);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].frames, 2400);
        assert_eq!(out[0].pts100ns, ms(100));
    }

    #[test]
    fn drops_packets_outside_window() {
        let a = vec![pkt(0, 480, 1), pkt(100, 480, 2), pkt(500, 480, 3)];
        let out = clip_audio_to_window(&a, ms(50), ms(200), RATE, CH);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].pts100ns, ms(100));
    }

    #[test]
    fn output_is_monotonic_and_contiguous() {
        let a: Vec<AudioSample> = (0..10).map(|i| pkt(i * 10, 480, 9)).collect();
        let out = clip_audio_to_window(&a, ms(25), ms(75), RATE, CH);
        assert!(!out.is_empty());
        for w in out.windows(2) {
            assert!(
                w[0].pts100ns < w[1].pts100ns,
                "pts must strictly increase: {} then {}",
                w[0].pts100ns,
                w[1].pts100ns
            );
        }
        assert!(out.first().unwrap().pts100ns >= ms(25));
        let total_frames: u32 = out.iter().map(|s| s.frames).sum();
        assert!(total_frames <= 4800, "clipped audio cannot exceed the window");
        for s in &out {
            assert_eq!(s.data.len(), s.frames as usize * BPF);
        }
    }

    #[test]
    fn byte_offset_matches_skipped_frames() {
        let frames = 4800u32;
        let mut data = Vec::with_capacity(frames as usize * BPF);
        for f in 0..frames {
            let tag = (f % 251) as u8;
            for _ in 0..BPF {
                data.push(tag);
            }
        }
        let a = vec![AudioSample {
            data,
            frames,
            pts100ns: ms(0),
        }];
        let out = clip_audio_to_window(&a, ms(20), ms(100), RATE, CH);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].frames, 3840, "4800 frames minus the 960 skipped");
        assert_eq!(out[0].data[0], (960 % 251) as u8, "starts at frame 960");
        let last = *out[0].data.last().unwrap();
        assert_eq!(last, ((frames - 1) % 251) as u8, "ends on the final frame");
    }
}