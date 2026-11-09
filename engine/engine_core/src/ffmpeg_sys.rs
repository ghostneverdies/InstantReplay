pub type va_list = *mut ::std::os::raw::c_char;
pub const AVSampleFormat_AV_SAMPLE_FMT_NONE: AVSampleFormat = -1;
pub const AVSampleFormat_AV_SAMPLE_FMT_U8: AVSampleFormat = 0;
pub const AVSampleFormat_AV_SAMPLE_FMT_S16: AVSampleFormat = 1;
pub const AVSampleFormat_AV_SAMPLE_FMT_S32: AVSampleFormat = 2;
pub const AVSampleFormat_AV_SAMPLE_FMT_FLT: AVSampleFormat = 3;
pub const AVSampleFormat_AV_SAMPLE_FMT_DBL: AVSampleFormat = 4;
pub const AVSampleFormat_AV_SAMPLE_FMT_U8P: AVSampleFormat = 5;
pub const AVSampleFormat_AV_SAMPLE_FMT_S16P: AVSampleFormat = 6;
pub const AVSampleFormat_AV_SAMPLE_FMT_S32P: AVSampleFormat = 7;
pub const AVSampleFormat_AV_SAMPLE_FMT_FLTP: AVSampleFormat = 8;
pub const AVSampleFormat_AV_SAMPLE_FMT_DBLP: AVSampleFormat = 9;
pub const AVSampleFormat_AV_SAMPLE_FMT_S64: AVSampleFormat = 10;
pub const AVSampleFormat_AV_SAMPLE_FMT_S64P: AVSampleFormat = 11;
pub const AVSampleFormat_AV_SAMPLE_FMT_NB: AVSampleFormat = 12;
pub type AVSampleFormat = ::std::os::raw::c_int;
unsafe extern "C" {
    pub fn av_get_sample_fmt_name(sample_fmt: AVSampleFormat) -> *const ::std::os::raw::c_char;
}
unsafe extern "C" {
    pub fn av_get_sample_fmt(name: *const ::std::os::raw::c_char) -> AVSampleFormat;
}
unsafe extern "C" {
    pub fn av_get_alt_sample_fmt(
        sample_fmt: AVSampleFormat,
        planar: ::std::os::raw::c_int,
    ) -> AVSampleFormat;
}
unsafe extern "C" {
    pub fn av_get_packed_sample_fmt(sample_fmt: AVSampleFormat) -> AVSampleFormat;
}
unsafe extern "C" {
    pub fn av_get_planar_sample_fmt(sample_fmt: AVSampleFormat) -> AVSampleFormat;
}
unsafe extern "C" {
    pub fn av_get_sample_fmt_string(
        buf: *mut ::std::os::raw::c_char,
        buf_size: ::std::os::raw::c_int,
        sample_fmt: AVSampleFormat,
    ) -> *mut ::std::os::raw::c_char;
}
unsafe extern "C" {
    pub fn av_get_bytes_per_sample(sample_fmt: AVSampleFormat) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_sample_fmt_is_planar(sample_fmt: AVSampleFormat) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_samples_get_buffer_size(
        linesize: *mut ::std::os::raw::c_int,
        nb_channels: ::std::os::raw::c_int,
        nb_samples: ::std::os::raw::c_int,
        sample_fmt: AVSampleFormat,
        align: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_samples_fill_arrays(
        audio_data: *mut *mut u8,
        linesize: *mut ::std::os::raw::c_int,
        buf: *const u8,
        nb_channels: ::std::os::raw::c_int,
        nb_samples: ::std::os::raw::c_int,
        sample_fmt: AVSampleFormat,
        align: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_samples_alloc(
        audio_data: *mut *mut u8,
        linesize: *mut ::std::os::raw::c_int,
        nb_channels: ::std::os::raw::c_int,
        nb_samples: ::std::os::raw::c_int,
        sample_fmt: AVSampleFormat,
        align: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_samples_alloc_array_and_samples(
        audio_data: *mut *mut *mut u8,
        linesize: *mut ::std::os::raw::c_int,
        nb_channels: ::std::os::raw::c_int,
        nb_samples: ::std::os::raw::c_int,
        sample_fmt: AVSampleFormat,
        align: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_samples_copy(
        dst: *const *mut u8,
        src: *const *mut u8,
        dst_offset: ::std::os::raw::c_int,
        src_offset: ::std::os::raw::c_int,
        nb_samples: ::std::os::raw::c_int,
        nb_channels: ::std::os::raw::c_int,
        sample_fmt: AVSampleFormat,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_samples_set_silence(
        audio_data: *const *mut u8,
        offset: ::std::os::raw::c_int,
        nb_samples: ::std::os::raw::c_int,
        nb_channels: ::std::os::raw::c_int,
        sample_fmt: AVSampleFormat,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_version_info() -> *const ::std::os::raw::c_char;
}
pub const AVMediaType_AVMEDIA_TYPE_UNKNOWN: AVMediaType = -1;
pub const AVMediaType_AVMEDIA_TYPE_VIDEO: AVMediaType = 0;
pub const AVMediaType_AVMEDIA_TYPE_AUDIO: AVMediaType = 1;
pub const AVMediaType_AVMEDIA_TYPE_DATA: AVMediaType = 2;
pub const AVMediaType_AVMEDIA_TYPE_SUBTITLE: AVMediaType = 3;
pub const AVMediaType_AVMEDIA_TYPE_ATTACHMENT: AVMediaType = 4;
pub const AVMediaType_AVMEDIA_TYPE_NB: AVMediaType = 5;
pub type AVMediaType = ::std::os::raw::c_int;
unsafe extern "C" {
    pub fn av_get_media_type_string(media_type: AVMediaType) -> *const ::std::os::raw::c_char;
}
pub const AVPictureType_AV_PICTURE_TYPE_NONE: AVPictureType = 0;
pub const AVPictureType_AV_PICTURE_TYPE_I: AVPictureType = 1;
pub const AVPictureType_AV_PICTURE_TYPE_P: AVPictureType = 2;
pub const AVPictureType_AV_PICTURE_TYPE_B: AVPictureType = 3;
pub const AVPictureType_AV_PICTURE_TYPE_S: AVPictureType = 4;
pub const AVPictureType_AV_PICTURE_TYPE_SI: AVPictureType = 5;
pub const AVPictureType_AV_PICTURE_TYPE_SP: AVPictureType = 6;
pub const AVPictureType_AV_PICTURE_TYPE_BI: AVPictureType = 7;
pub type AVPictureType = ::std::os::raw::c_int;
unsafe extern "C" {
    pub fn av_get_picture_type_char(pict_type: AVPictureType) -> ::std::os::raw::c_char;
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct _iobuf {
    pub _Placeholder: *mut ::std::os::raw::c_void,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of _iobuf"][::std::mem::size_of::<_iobuf>() - 8usize];
    ["Alignment of _iobuf"][::std::mem::align_of::<_iobuf>() - 8usize];
    ["Offset of field: _iobuf::_Placeholder"]
        [::std::mem::offset_of!(_iobuf, _Placeholder) - 0usize];
};
pub type FILE = _iobuf;
unsafe extern "C" {
    pub fn av_strerror(
        errnum: ::std::os::raw::c_int,
        errbuf: *mut ::std::os::raw::c_char,
        errbuf_size: usize,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_malloc(size: usize) -> *mut ::std::os::raw::c_void;
}
unsafe extern "C" {
    pub fn av_mallocz(size: usize) -> *mut ::std::os::raw::c_void;
}
unsafe extern "C" {
    pub fn av_malloc_array(nmemb: usize, size: usize) -> *mut ::std::os::raw::c_void;
}
unsafe extern "C" {
    pub fn av_calloc(nmemb: usize, size: usize) -> *mut ::std::os::raw::c_void;
}
unsafe extern "C" {
    pub fn av_realloc(ptr: *mut ::std::os::raw::c_void, size: usize)
        -> *mut ::std::os::raw::c_void;
}
unsafe extern "C" {
    pub fn av_reallocp(ptr: *mut ::std::os::raw::c_void, size: usize) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_realloc_f(
        ptr: *mut ::std::os::raw::c_void,
        nelem: usize,
        elsize: usize,
    ) -> *mut ::std::os::raw::c_void;
}
unsafe extern "C" {
    pub fn av_realloc_array(
        ptr: *mut ::std::os::raw::c_void,
        nmemb: usize,
        size: usize,
    ) -> *mut ::std::os::raw::c_void;
}
unsafe extern "C" {
    pub fn av_reallocp_array(
        ptr: *mut ::std::os::raw::c_void,
        nmemb: usize,
        size: usize,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_fast_realloc(
        ptr: *mut ::std::os::raw::c_void,
        size: *mut ::std::os::raw::c_uint,
        min_size: usize,
    ) -> *mut ::std::os::raw::c_void;
}
unsafe extern "C" {
    pub fn av_fast_malloc(
        ptr: *mut ::std::os::raw::c_void,
        size: *mut ::std::os::raw::c_uint,
        min_size: usize,
    );
}
unsafe extern "C" {
    pub fn av_fast_mallocz(
        ptr: *mut ::std::os::raw::c_void,
        size: *mut ::std::os::raw::c_uint,
        min_size: usize,
    );
}
unsafe extern "C" {
    pub fn av_free(ptr: *mut ::std::os::raw::c_void);
}
unsafe extern "C" {
    pub fn av_freep(ptr: *mut ::std::os::raw::c_void);
}
unsafe extern "C" {
    pub fn av_strdup(s: *const ::std::os::raw::c_char) -> *mut ::std::os::raw::c_char;
}
unsafe extern "C" {
    pub fn av_strndup(s: *const ::std::os::raw::c_char, len: usize) -> *mut ::std::os::raw::c_char;
}
unsafe extern "C" {
    pub fn av_memdup(p: *const ::std::os::raw::c_void, size: usize) -> *mut ::std::os::raw::c_void;
}
unsafe extern "C" {
    pub fn av_memcpy_backptr(dst: *mut u8, back: ::std::os::raw::c_int, cnt: ::std::os::raw::c_int);
}
unsafe extern "C" {
    pub fn av_dynarray_add(
        tab_ptr: *mut ::std::os::raw::c_void,
        nb_ptr: *mut ::std::os::raw::c_int,
        elem: *mut ::std::os::raw::c_void,
    );
}
unsafe extern "C" {
    pub fn av_dynarray_add_nofree(
        tab_ptr: *mut ::std::os::raw::c_void,
        nb_ptr: *mut ::std::os::raw::c_int,
        elem: *mut ::std::os::raw::c_void,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_dynarray2_add(
        tab_ptr: *mut *mut ::std::os::raw::c_void,
        nb_ptr: *mut ::std::os::raw::c_int,
        elem_size: usize,
        elem_data: *const u8,
    ) -> *mut ::std::os::raw::c_void;
}
unsafe extern "C" {
    pub fn av_size_mult(a: usize, b: usize, r: *mut usize) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_max_alloc(max: usize);
}
unsafe extern "C" {
    pub fn av_log2(v: ::std::os::raw::c_uint) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_log2_16bit(v: ::std::os::raw::c_uint) -> ::std::os::raw::c_int;
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVRational {
    pub num: ::std::os::raw::c_int,
    pub den: ::std::os::raw::c_int,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVRational"][::std::mem::size_of::<AVRational>() - 8usize];
    ["Alignment of AVRational"][::std::mem::align_of::<AVRational>() - 4usize];
    ["Offset of field: AVRational::num"][::std::mem::offset_of!(AVRational, num) - 0usize];
    ["Offset of field: AVRational::den"][::std::mem::offset_of!(AVRational, den) - 4usize];
};
unsafe extern "C" {
    pub fn av_reduce(
        dst_num: *mut ::std::os::raw::c_int,
        dst_den: *mut ::std::os::raw::c_int,
        num: i64,
        den: i64,
        max: i64,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_mul_q(b: AVRational, c: AVRational) -> AVRational;
}
unsafe extern "C" {
    pub fn av_div_q(b: AVRational, c: AVRational) -> AVRational;
}
unsafe extern "C" {
    pub fn av_add_q(b: AVRational, c: AVRational) -> AVRational;
}
unsafe extern "C" {
    pub fn av_sub_q(b: AVRational, c: AVRational) -> AVRational;
}
unsafe extern "C" {
    pub fn av_d2q(d: f64, max: ::std::os::raw::c_int) -> AVRational;
}
unsafe extern "C" {
    pub fn av_nearer_q(q: AVRational, q1: AVRational, q2: AVRational) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_find_nearest_q_idx(q: AVRational, q_list: *const AVRational)
        -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_q2intfloat(q: AVRational) -> u32;
}
unsafe extern "C" {
    pub fn av_gcd_q(
        a: AVRational,
        b: AVRational,
        max_den: ::std::os::raw::c_int,
        def: AVRational,
    ) -> AVRational;
}
pub const AVRounding_AV_ROUND_ZERO: AVRounding = 0;
pub const AVRounding_AV_ROUND_INF: AVRounding = 1;
pub const AVRounding_AV_ROUND_DOWN: AVRounding = 2;
pub const AVRounding_AV_ROUND_UP: AVRounding = 3;
pub const AVRounding_AV_ROUND_NEAR_INF: AVRounding = 5;
pub const AVRounding_AV_ROUND_PASS_MINMAX: AVRounding = 8192;
pub type AVRounding = ::std::os::raw::c_int;
unsafe extern "C" {
    pub fn av_gcd(a: i64, b: i64) -> i64;
}
unsafe extern "C" {
    pub fn av_rescale(a: i64, b: i64, c: i64) -> i64;
}
unsafe extern "C" {
    pub fn av_rescale_rnd(a: i64, b: i64, c: i64, rnd: AVRounding) -> i64;
}
unsafe extern "C" {
    pub fn av_rescale_q(a: i64, bq: AVRational, cq: AVRational) -> i64;
}
unsafe extern "C" {
    pub fn av_rescale_q_rnd(a: i64, bq: AVRational, cq: AVRational, rnd: AVRounding) -> i64;
}
unsafe extern "C" {
    pub fn av_compare_ts(
        ts_a: i64,
        tb_a: AVRational,
        ts_b: i64,
        tb_b: AVRational,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_compare_mod(a: u64, b: u64, mod_: u64) -> i64;
}
unsafe extern "C" {
    pub fn av_rescale_delta(
        in_tb: AVRational,
        in_ts: i64,
        fs_tb: AVRational,
        duration: ::std::os::raw::c_int,
        last: *mut i64,
        out_tb: AVRational,
    ) -> i64;
}
unsafe extern "C" {
    pub fn av_add_stable(ts_tb: AVRational, ts: i64, inc_tb: AVRational, inc: i64) -> i64;
}
unsafe extern "C" {
    pub fn av_bessel_i0(x: f64) -> f64;
}
pub const AVClassCategory_AV_CLASS_CATEGORY_NA: AVClassCategory = 0;
pub const AVClassCategory_AV_CLASS_CATEGORY_INPUT: AVClassCategory = 1;
pub const AVClassCategory_AV_CLASS_CATEGORY_OUTPUT: AVClassCategory = 2;
pub const AVClassCategory_AV_CLASS_CATEGORY_MUXER: AVClassCategory = 3;
pub const AVClassCategory_AV_CLASS_CATEGORY_DEMUXER: AVClassCategory = 4;
pub const AVClassCategory_AV_CLASS_CATEGORY_ENCODER: AVClassCategory = 5;
pub const AVClassCategory_AV_CLASS_CATEGORY_DECODER: AVClassCategory = 6;
pub const AVClassCategory_AV_CLASS_CATEGORY_FILTER: AVClassCategory = 7;
pub const AVClassCategory_AV_CLASS_CATEGORY_BITSTREAM_FILTER: AVClassCategory = 8;
pub const AVClassCategory_AV_CLASS_CATEGORY_SWSCALER: AVClassCategory = 9;
pub const AVClassCategory_AV_CLASS_CATEGORY_SWRESAMPLER: AVClassCategory = 10;
pub const AVClassCategory_AV_CLASS_CATEGORY_HWDEVICE: AVClassCategory = 11;
pub const AVClassCategory_AV_CLASS_CATEGORY_DEVICE_VIDEO_OUTPUT: AVClassCategory = 40;
pub const AVClassCategory_AV_CLASS_CATEGORY_DEVICE_VIDEO_INPUT: AVClassCategory = 41;
pub const AVClassCategory_AV_CLASS_CATEGORY_DEVICE_AUDIO_OUTPUT: AVClassCategory = 42;
pub const AVClassCategory_AV_CLASS_CATEGORY_DEVICE_AUDIO_INPUT: AVClassCategory = 43;
pub const AVClassCategory_AV_CLASS_CATEGORY_DEVICE_OUTPUT: AVClassCategory = 44;
pub const AVClassCategory_AV_CLASS_CATEGORY_DEVICE_INPUT: AVClassCategory = 45;
pub const AVClassCategory_AV_CLASS_CATEGORY_NB: AVClassCategory = 46;
pub type AVClassCategory = ::std::os::raw::c_int;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVClass {
    pub class_name: *const ::std::os::raw::c_char,
    pub item_name: ::std::option::Option<
        unsafe extern "C" fn(ctx: *mut ::std::os::raw::c_void) -> *const ::std::os::raw::c_char,
    >,
    pub option: *const AVOption,
    pub version: ::std::os::raw::c_int,
    pub log_level_offset_offset: ::std::os::raw::c_int,
    pub parent_log_context_offset: ::std::os::raw::c_int,
    pub category: AVClassCategory,
    pub get_category: ::std::option::Option<
        unsafe extern "C" fn(ctx: *mut ::std::os::raw::c_void) -> AVClassCategory,
    >,
    pub query_ranges: ::std::option::Option<
        unsafe extern "C" fn(
            arg1: *mut *mut AVOptionRanges,
            obj: *mut ::std::os::raw::c_void,
            key: *const ::std::os::raw::c_char,
            flags: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int,
    >,
    pub child_next: ::std::option::Option<
        unsafe extern "C" fn(
            obj: *mut ::std::os::raw::c_void,
            prev: *mut ::std::os::raw::c_void,
        ) -> *mut ::std::os::raw::c_void,
    >,
    pub child_class_iterate: ::std::option::Option<
        unsafe extern "C" fn(iter: *mut *mut ::std::os::raw::c_void) -> *const AVClass,
    >,
    pub state_flags_offset: ::std::os::raw::c_int,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVClass"][::std::mem::size_of::<AVClass>() - 80usize];
    ["Alignment of AVClass"][::std::mem::align_of::<AVClass>() - 8usize];
    ["Offset of field: AVClass::class_name"][::std::mem::offset_of!(AVClass, class_name) - 0usize];
    ["Offset of field: AVClass::item_name"][::std::mem::offset_of!(AVClass, item_name) - 8usize];
    ["Offset of field: AVClass::option"][::std::mem::offset_of!(AVClass, option) - 16usize];
    ["Offset of field: AVClass::version"][::std::mem::offset_of!(AVClass, version) - 24usize];
    ["Offset of field: AVClass::log_level_offset_offset"]
        [::std::mem::offset_of!(AVClass, log_level_offset_offset) - 28usize];
    ["Offset of field: AVClass::parent_log_context_offset"]
        [::std::mem::offset_of!(AVClass, parent_log_context_offset) - 32usize];
    ["Offset of field: AVClass::category"][::std::mem::offset_of!(AVClass, category) - 36usize];
    ["Offset of field: AVClass::get_category"]
        [::std::mem::offset_of!(AVClass, get_category) - 40usize];
    ["Offset of field: AVClass::query_ranges"]
        [::std::mem::offset_of!(AVClass, query_ranges) - 48usize];
    ["Offset of field: AVClass::child_next"][::std::mem::offset_of!(AVClass, child_next) - 56usize];
    ["Offset of field: AVClass::child_class_iterate"]
        [::std::mem::offset_of!(AVClass, child_class_iterate) - 64usize];
    ["Offset of field: AVClass::state_flags_offset"]
        [::std::mem::offset_of!(AVClass, state_flags_offset) - 72usize];
};
unsafe extern "C" {
    pub fn av_log(
        avcl: *mut ::std::os::raw::c_void,
        level: ::std::os::raw::c_int,
        fmt: *const ::std::os::raw::c_char,
        ...
    );
}
unsafe extern "C" {
    pub fn av_log_once(
        avcl: *mut ::std::os::raw::c_void,
        initial_level: ::std::os::raw::c_int,
        subsequent_level: ::std::os::raw::c_int,
        state: *mut ::std::os::raw::c_int,
        fmt: *const ::std::os::raw::c_char,
        ...
    );
}
unsafe extern "C" {
    pub fn av_vlog(
        avcl: *mut ::std::os::raw::c_void,
        level: ::std::os::raw::c_int,
        fmt: *const ::std::os::raw::c_char,
        vl: va_list,
    );
}
unsafe extern "C" {
    pub fn av_log_get_level() -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_log_set_level(level: ::std::os::raw::c_int);
}
unsafe extern "C" {
    pub fn av_log_set_callback(
        callback: ::std::option::Option<
            unsafe extern "C" fn(
                arg1: *mut ::std::os::raw::c_void,
                arg2: ::std::os::raw::c_int,
                arg3: *const ::std::os::raw::c_char,
                arg4: va_list,
            ),
        >,
    );
}
unsafe extern "C" {
    pub fn av_log_default_callback(
        avcl: *mut ::std::os::raw::c_void,
        level: ::std::os::raw::c_int,
        fmt: *const ::std::os::raw::c_char,
        vl: va_list,
    );
}
unsafe extern "C" {
    pub fn av_default_item_name(ctx: *mut ::std::os::raw::c_void) -> *const ::std::os::raw::c_char;
}
unsafe extern "C" {
    pub fn av_default_get_category(ptr: *mut ::std::os::raw::c_void) -> AVClassCategory;
}
unsafe extern "C" {
    pub fn av_log_format_line(
        ptr: *mut ::std::os::raw::c_void,
        level: ::std::os::raw::c_int,
        fmt: *const ::std::os::raw::c_char,
        vl: va_list,
        line: *mut ::std::os::raw::c_char,
        line_size: ::std::os::raw::c_int,
        print_prefix: *mut ::std::os::raw::c_int,
    );
}
unsafe extern "C" {
    pub fn av_log_format_line2(
        ptr: *mut ::std::os::raw::c_void,
        level: ::std::os::raw::c_int,
        fmt: *const ::std::os::raw::c_char,
        vl: va_list,
        line: *mut ::std::os::raw::c_char,
        line_size: ::std::os::raw::c_int,
        print_prefix: *mut ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_log_set_flags(arg: ::std::os::raw::c_int);
}
unsafe extern "C" {
    pub fn av_log_get_flags() -> ::std::os::raw::c_int;
}
pub const AVPixelFormat_AV_PIX_FMT_NONE: AVPixelFormat = -1;
pub const AVPixelFormat_AV_PIX_FMT_YUV420P: AVPixelFormat = 0;
pub const AVPixelFormat_AV_PIX_FMT_YUYV422: AVPixelFormat = 1;
pub const AVPixelFormat_AV_PIX_FMT_RGB24: AVPixelFormat = 2;
pub const AVPixelFormat_AV_PIX_FMT_BGR24: AVPixelFormat = 3;
pub const AVPixelFormat_AV_PIX_FMT_YUV422P: AVPixelFormat = 4;
pub const AVPixelFormat_AV_PIX_FMT_YUV444P: AVPixelFormat = 5;
pub const AVPixelFormat_AV_PIX_FMT_YUV410P: AVPixelFormat = 6;
pub const AVPixelFormat_AV_PIX_FMT_YUV411P: AVPixelFormat = 7;
pub const AVPixelFormat_AV_PIX_FMT_GRAY8: AVPixelFormat = 8;
pub const AVPixelFormat_AV_PIX_FMT_MONOWHITE: AVPixelFormat = 9;
pub const AVPixelFormat_AV_PIX_FMT_MONOBLACK: AVPixelFormat = 10;
pub const AVPixelFormat_AV_PIX_FMT_PAL8: AVPixelFormat = 11;
pub const AVPixelFormat_AV_PIX_FMT_YUVJ420P: AVPixelFormat = 12;
pub const AVPixelFormat_AV_PIX_FMT_YUVJ422P: AVPixelFormat = 13;
pub const AVPixelFormat_AV_PIX_FMT_YUVJ444P: AVPixelFormat = 14;
pub const AVPixelFormat_AV_PIX_FMT_UYVY422: AVPixelFormat = 15;
pub const AVPixelFormat_AV_PIX_FMT_UYYVYY411: AVPixelFormat = 16;
pub const AVPixelFormat_AV_PIX_FMT_BGR8: AVPixelFormat = 17;
pub const AVPixelFormat_AV_PIX_FMT_BGR4: AVPixelFormat = 18;
pub const AVPixelFormat_AV_PIX_FMT_BGR4_BYTE: AVPixelFormat = 19;
pub const AVPixelFormat_AV_PIX_FMT_RGB8: AVPixelFormat = 20;
pub const AVPixelFormat_AV_PIX_FMT_RGB4: AVPixelFormat = 21;
pub const AVPixelFormat_AV_PIX_FMT_RGB4_BYTE: AVPixelFormat = 22;
pub const AVPixelFormat_AV_PIX_FMT_NV12: AVPixelFormat = 23;
pub const AVPixelFormat_AV_PIX_FMT_NV21: AVPixelFormat = 24;
pub const AVPixelFormat_AV_PIX_FMT_ARGB: AVPixelFormat = 25;
pub const AVPixelFormat_AV_PIX_FMT_RGBA: AVPixelFormat = 26;
pub const AVPixelFormat_AV_PIX_FMT_ABGR: AVPixelFormat = 27;
pub const AVPixelFormat_AV_PIX_FMT_BGRA: AVPixelFormat = 28;
pub const AVPixelFormat_AV_PIX_FMT_GRAY16BE: AVPixelFormat = 29;
pub const AVPixelFormat_AV_PIX_FMT_GRAY16LE: AVPixelFormat = 30;
pub const AVPixelFormat_AV_PIX_FMT_YUV440P: AVPixelFormat = 31;
pub const AVPixelFormat_AV_PIX_FMT_YUVJ440P: AVPixelFormat = 32;
pub const AVPixelFormat_AV_PIX_FMT_YUVA420P: AVPixelFormat = 33;
pub const AVPixelFormat_AV_PIX_FMT_RGB48BE: AVPixelFormat = 34;
pub const AVPixelFormat_AV_PIX_FMT_RGB48LE: AVPixelFormat = 35;
pub const AVPixelFormat_AV_PIX_FMT_RGB565BE: AVPixelFormat = 36;
pub const AVPixelFormat_AV_PIX_FMT_RGB565LE: AVPixelFormat = 37;
pub const AVPixelFormat_AV_PIX_FMT_RGB555BE: AVPixelFormat = 38;
pub const AVPixelFormat_AV_PIX_FMT_RGB555LE: AVPixelFormat = 39;
pub const AVPixelFormat_AV_PIX_FMT_BGR565BE: AVPixelFormat = 40;
pub const AVPixelFormat_AV_PIX_FMT_BGR565LE: AVPixelFormat = 41;
pub const AVPixelFormat_AV_PIX_FMT_BGR555BE: AVPixelFormat = 42;
pub const AVPixelFormat_AV_PIX_FMT_BGR555LE: AVPixelFormat = 43;
pub const AVPixelFormat_AV_PIX_FMT_VAAPI: AVPixelFormat = 44;
pub const AVPixelFormat_AV_PIX_FMT_YUV420P16LE: AVPixelFormat = 45;
pub const AVPixelFormat_AV_PIX_FMT_YUV420P16BE: AVPixelFormat = 46;
pub const AVPixelFormat_AV_PIX_FMT_YUV422P16LE: AVPixelFormat = 47;
pub const AVPixelFormat_AV_PIX_FMT_YUV422P16BE: AVPixelFormat = 48;
pub const AVPixelFormat_AV_PIX_FMT_YUV444P16LE: AVPixelFormat = 49;
pub const AVPixelFormat_AV_PIX_FMT_YUV444P16BE: AVPixelFormat = 50;
pub const AVPixelFormat_AV_PIX_FMT_DXVA2_VLD: AVPixelFormat = 51;
pub const AVPixelFormat_AV_PIX_FMT_RGB444LE: AVPixelFormat = 52;
pub const AVPixelFormat_AV_PIX_FMT_RGB444BE: AVPixelFormat = 53;
pub const AVPixelFormat_AV_PIX_FMT_BGR444LE: AVPixelFormat = 54;
pub const AVPixelFormat_AV_PIX_FMT_BGR444BE: AVPixelFormat = 55;
pub const AVPixelFormat_AV_PIX_FMT_YA8: AVPixelFormat = 56;
pub const AVPixelFormat_AV_PIX_FMT_Y400A: AVPixelFormat = 56;
pub const AVPixelFormat_AV_PIX_FMT_GRAY8A: AVPixelFormat = 56;
pub const AVPixelFormat_AV_PIX_FMT_BGR48BE: AVPixelFormat = 57;
pub const AVPixelFormat_AV_PIX_FMT_BGR48LE: AVPixelFormat = 58;
pub const AVPixelFormat_AV_PIX_FMT_YUV420P9BE: AVPixelFormat = 59;
pub const AVPixelFormat_AV_PIX_FMT_YUV420P9LE: AVPixelFormat = 60;
pub const AVPixelFormat_AV_PIX_FMT_YUV420P10BE: AVPixelFormat = 61;
pub const AVPixelFormat_AV_PIX_FMT_YUV420P10LE: AVPixelFormat = 62;
pub const AVPixelFormat_AV_PIX_FMT_YUV422P10BE: AVPixelFormat = 63;
pub const AVPixelFormat_AV_PIX_FMT_YUV422P10LE: AVPixelFormat = 64;
pub const AVPixelFormat_AV_PIX_FMT_YUV444P9BE: AVPixelFormat = 65;
pub const AVPixelFormat_AV_PIX_FMT_YUV444P9LE: AVPixelFormat = 66;
pub const AVPixelFormat_AV_PIX_FMT_YUV444P10BE: AVPixelFormat = 67;
pub const AVPixelFormat_AV_PIX_FMT_YUV444P10LE: AVPixelFormat = 68;
pub const AVPixelFormat_AV_PIX_FMT_YUV422P9BE: AVPixelFormat = 69;
pub const AVPixelFormat_AV_PIX_FMT_YUV422P9LE: AVPixelFormat = 70;
pub const AVPixelFormat_AV_PIX_FMT_GBRP: AVPixelFormat = 71;
pub const AVPixelFormat_AV_PIX_FMT_GBR24P: AVPixelFormat = 71;
pub const AVPixelFormat_AV_PIX_FMT_GBRP9BE: AVPixelFormat = 72;
pub const AVPixelFormat_AV_PIX_FMT_GBRP9LE: AVPixelFormat = 73;
pub const AVPixelFormat_AV_PIX_FMT_GBRP10BE: AVPixelFormat = 74;
pub const AVPixelFormat_AV_PIX_FMT_GBRP10LE: AVPixelFormat = 75;
pub const AVPixelFormat_AV_PIX_FMT_GBRP16BE: AVPixelFormat = 76;
pub const AVPixelFormat_AV_PIX_FMT_GBRP16LE: AVPixelFormat = 77;
pub const AVPixelFormat_AV_PIX_FMT_YUVA422P: AVPixelFormat = 78;
pub const AVPixelFormat_AV_PIX_FMT_YUVA444P: AVPixelFormat = 79;
pub const AVPixelFormat_AV_PIX_FMT_YUVA420P9BE: AVPixelFormat = 80;
pub const AVPixelFormat_AV_PIX_FMT_YUVA420P9LE: AVPixelFormat = 81;
pub const AVPixelFormat_AV_PIX_FMT_YUVA422P9BE: AVPixelFormat = 82;
pub const AVPixelFormat_AV_PIX_FMT_YUVA422P9LE: AVPixelFormat = 83;
pub const AVPixelFormat_AV_PIX_FMT_YUVA444P9BE: AVPixelFormat = 84;
pub const AVPixelFormat_AV_PIX_FMT_YUVA444P9LE: AVPixelFormat = 85;
pub const AVPixelFormat_AV_PIX_FMT_YUVA420P10BE: AVPixelFormat = 86;
pub const AVPixelFormat_AV_PIX_FMT_YUVA420P10LE: AVPixelFormat = 87;
pub const AVPixelFormat_AV_PIX_FMT_YUVA422P10BE: AVPixelFormat = 88;
pub const AVPixelFormat_AV_PIX_FMT_YUVA422P10LE: AVPixelFormat = 89;
pub const AVPixelFormat_AV_PIX_FMT_YUVA444P10BE: AVPixelFormat = 90;
pub const AVPixelFormat_AV_PIX_FMT_YUVA444P10LE: AVPixelFormat = 91;
pub const AVPixelFormat_AV_PIX_FMT_YUVA420P16BE: AVPixelFormat = 92;
pub const AVPixelFormat_AV_PIX_FMT_YUVA420P16LE: AVPixelFormat = 93;
pub const AVPixelFormat_AV_PIX_FMT_YUVA422P16BE: AVPixelFormat = 94;
pub const AVPixelFormat_AV_PIX_FMT_YUVA422P16LE: AVPixelFormat = 95;
pub const AVPixelFormat_AV_PIX_FMT_YUVA444P16BE: AVPixelFormat = 96;
pub const AVPixelFormat_AV_PIX_FMT_YUVA444P16LE: AVPixelFormat = 97;
pub const AVPixelFormat_AV_PIX_FMT_VDPAU: AVPixelFormat = 98;
pub const AVPixelFormat_AV_PIX_FMT_XYZ12LE: AVPixelFormat = 99;
pub const AVPixelFormat_AV_PIX_FMT_XYZ12BE: AVPixelFormat = 100;
pub const AVPixelFormat_AV_PIX_FMT_NV16: AVPixelFormat = 101;
pub const AVPixelFormat_AV_PIX_FMT_NV20LE: AVPixelFormat = 102;
pub const AVPixelFormat_AV_PIX_FMT_NV20BE: AVPixelFormat = 103;
pub const AVPixelFormat_AV_PIX_FMT_RGBA64BE: AVPixelFormat = 104;
pub const AVPixelFormat_AV_PIX_FMT_RGBA64LE: AVPixelFormat = 105;
pub const AVPixelFormat_AV_PIX_FMT_BGRA64BE: AVPixelFormat = 106;
pub const AVPixelFormat_AV_PIX_FMT_BGRA64LE: AVPixelFormat = 107;
pub const AVPixelFormat_AV_PIX_FMT_YVYU422: AVPixelFormat = 108;
pub const AVPixelFormat_AV_PIX_FMT_YA16BE: AVPixelFormat = 109;
pub const AVPixelFormat_AV_PIX_FMT_YA16LE: AVPixelFormat = 110;
pub const AVPixelFormat_AV_PIX_FMT_GBRAP: AVPixelFormat = 111;
pub const AVPixelFormat_AV_PIX_FMT_GBRAP16BE: AVPixelFormat = 112;
pub const AVPixelFormat_AV_PIX_FMT_GBRAP16LE: AVPixelFormat = 113;
pub const AVPixelFormat_AV_PIX_FMT_QSV: AVPixelFormat = 114;
pub const AVPixelFormat_AV_PIX_FMT_MMAL: AVPixelFormat = 115;
pub const AVPixelFormat_AV_PIX_FMT_D3D11VA_VLD: AVPixelFormat = 116;
pub const AVPixelFormat_AV_PIX_FMT_CUDA: AVPixelFormat = 117;
pub const AVPixelFormat_AV_PIX_FMT_0RGB: AVPixelFormat = 118;
pub const AVPixelFormat_AV_PIX_FMT_RGB0: AVPixelFormat = 119;
pub const AVPixelFormat_AV_PIX_FMT_0BGR: AVPixelFormat = 120;
pub const AVPixelFormat_AV_PIX_FMT_BGR0: AVPixelFormat = 121;
pub const AVPixelFormat_AV_PIX_FMT_YUV420P12BE: AVPixelFormat = 122;
pub const AVPixelFormat_AV_PIX_FMT_YUV420P12LE: AVPixelFormat = 123;
pub const AVPixelFormat_AV_PIX_FMT_YUV420P14BE: AVPixelFormat = 124;
pub const AVPixelFormat_AV_PIX_FMT_YUV420P14LE: AVPixelFormat = 125;
pub const AVPixelFormat_AV_PIX_FMT_YUV422P12BE: AVPixelFormat = 126;
pub const AVPixelFormat_AV_PIX_FMT_YUV422P12LE: AVPixelFormat = 127;
pub const AVPixelFormat_AV_PIX_FMT_YUV422P14BE: AVPixelFormat = 128;
pub const AVPixelFormat_AV_PIX_FMT_YUV422P14LE: AVPixelFormat = 129;
pub const AVPixelFormat_AV_PIX_FMT_YUV444P12BE: AVPixelFormat = 130;
pub const AVPixelFormat_AV_PIX_FMT_YUV444P12LE: AVPixelFormat = 131;
pub const AVPixelFormat_AV_PIX_FMT_YUV444P14BE: AVPixelFormat = 132;
pub const AVPixelFormat_AV_PIX_FMT_YUV444P14LE: AVPixelFormat = 133;
pub const AVPixelFormat_AV_PIX_FMT_GBRP12BE: AVPixelFormat = 134;
pub const AVPixelFormat_AV_PIX_FMT_GBRP12LE: AVPixelFormat = 135;
pub const AVPixelFormat_AV_PIX_FMT_GBRP14BE: AVPixelFormat = 136;
pub const AVPixelFormat_AV_PIX_FMT_GBRP14LE: AVPixelFormat = 137;
pub const AVPixelFormat_AV_PIX_FMT_YUVJ411P: AVPixelFormat = 138;
pub const AVPixelFormat_AV_PIX_FMT_BAYER_BGGR8: AVPixelFormat = 139;
pub const AVPixelFormat_AV_PIX_FMT_BAYER_RGGB8: AVPixelFormat = 140;
pub const AVPixelFormat_AV_PIX_FMT_BAYER_GBRG8: AVPixelFormat = 141;
pub const AVPixelFormat_AV_PIX_FMT_BAYER_GRBG8: AVPixelFormat = 142;
pub const AVPixelFormat_AV_PIX_FMT_BAYER_BGGR16LE: AVPixelFormat = 143;
pub const AVPixelFormat_AV_PIX_FMT_BAYER_BGGR16BE: AVPixelFormat = 144;
pub const AVPixelFormat_AV_PIX_FMT_BAYER_RGGB16LE: AVPixelFormat = 145;
pub const AVPixelFormat_AV_PIX_FMT_BAYER_RGGB16BE: AVPixelFormat = 146;
pub const AVPixelFormat_AV_PIX_FMT_BAYER_GBRG16LE: AVPixelFormat = 147;
pub const AVPixelFormat_AV_PIX_FMT_BAYER_GBRG16BE: AVPixelFormat = 148;
pub const AVPixelFormat_AV_PIX_FMT_BAYER_GRBG16LE: AVPixelFormat = 149;
pub const AVPixelFormat_AV_PIX_FMT_BAYER_GRBG16BE: AVPixelFormat = 150;
pub const AVPixelFormat_AV_PIX_FMT_YUV440P10LE: AVPixelFormat = 151;
pub const AVPixelFormat_AV_PIX_FMT_YUV440P10BE: AVPixelFormat = 152;
pub const AVPixelFormat_AV_PIX_FMT_YUV440P12LE: AVPixelFormat = 153;
pub const AVPixelFormat_AV_PIX_FMT_YUV440P12BE: AVPixelFormat = 154;
pub const AVPixelFormat_AV_PIX_FMT_AYUV64LE: AVPixelFormat = 155;
pub const AVPixelFormat_AV_PIX_FMT_AYUV64BE: AVPixelFormat = 156;
pub const AVPixelFormat_AV_PIX_FMT_VIDEOTOOLBOX: AVPixelFormat = 157;
pub const AVPixelFormat_AV_PIX_FMT_P010LE: AVPixelFormat = 158;
pub const AVPixelFormat_AV_PIX_FMT_P010BE: AVPixelFormat = 159;
pub const AVPixelFormat_AV_PIX_FMT_GBRAP12BE: AVPixelFormat = 160;
pub const AVPixelFormat_AV_PIX_FMT_GBRAP12LE: AVPixelFormat = 161;
pub const AVPixelFormat_AV_PIX_FMT_GBRAP10BE: AVPixelFormat = 162;
pub const AVPixelFormat_AV_PIX_FMT_GBRAP10LE: AVPixelFormat = 163;
pub const AVPixelFormat_AV_PIX_FMT_MEDIACODEC: AVPixelFormat = 164;
pub const AVPixelFormat_AV_PIX_FMT_GRAY12BE: AVPixelFormat = 165;
pub const AVPixelFormat_AV_PIX_FMT_GRAY12LE: AVPixelFormat = 166;
pub const AVPixelFormat_AV_PIX_FMT_GRAY10BE: AVPixelFormat = 167;
pub const AVPixelFormat_AV_PIX_FMT_GRAY10LE: AVPixelFormat = 168;
pub const AVPixelFormat_AV_PIX_FMT_P016LE: AVPixelFormat = 169;
pub const AVPixelFormat_AV_PIX_FMT_P016BE: AVPixelFormat = 170;
pub const AVPixelFormat_AV_PIX_FMT_D3D11: AVPixelFormat = 171;
pub const AVPixelFormat_AV_PIX_FMT_GRAY9BE: AVPixelFormat = 172;
pub const AVPixelFormat_AV_PIX_FMT_GRAY9LE: AVPixelFormat = 173;
pub const AVPixelFormat_AV_PIX_FMT_GBRPF32BE: AVPixelFormat = 174;
pub const AVPixelFormat_AV_PIX_FMT_GBRPF32LE: AVPixelFormat = 175;
pub const AVPixelFormat_AV_PIX_FMT_GBRAPF32BE: AVPixelFormat = 176;
pub const AVPixelFormat_AV_PIX_FMT_GBRAPF32LE: AVPixelFormat = 177;
pub const AVPixelFormat_AV_PIX_FMT_DRM_PRIME: AVPixelFormat = 178;
pub const AVPixelFormat_AV_PIX_FMT_OPENCL: AVPixelFormat = 179;
pub const AVPixelFormat_AV_PIX_FMT_GRAY14BE: AVPixelFormat = 180;
pub const AVPixelFormat_AV_PIX_FMT_GRAY14LE: AVPixelFormat = 181;
pub const AVPixelFormat_AV_PIX_FMT_GRAYF32BE: AVPixelFormat = 182;
pub const AVPixelFormat_AV_PIX_FMT_GRAYF32LE: AVPixelFormat = 183;
pub const AVPixelFormat_AV_PIX_FMT_YUVA422P12BE: AVPixelFormat = 184;
pub const AVPixelFormat_AV_PIX_FMT_YUVA422P12LE: AVPixelFormat = 185;
pub const AVPixelFormat_AV_PIX_FMT_YUVA444P12BE: AVPixelFormat = 186;
pub const AVPixelFormat_AV_PIX_FMT_YUVA444P12LE: AVPixelFormat = 187;
pub const AVPixelFormat_AV_PIX_FMT_NV24: AVPixelFormat = 188;
pub const AVPixelFormat_AV_PIX_FMT_NV42: AVPixelFormat = 189;
pub const AVPixelFormat_AV_PIX_FMT_VULKAN: AVPixelFormat = 190;
pub const AVPixelFormat_AV_PIX_FMT_Y210BE: AVPixelFormat = 191;
pub const AVPixelFormat_AV_PIX_FMT_Y210LE: AVPixelFormat = 192;
pub const AVPixelFormat_AV_PIX_FMT_X2RGB10LE: AVPixelFormat = 193;
pub const AVPixelFormat_AV_PIX_FMT_X2RGB10BE: AVPixelFormat = 194;
pub const AVPixelFormat_AV_PIX_FMT_X2BGR10LE: AVPixelFormat = 195;
pub const AVPixelFormat_AV_PIX_FMT_X2BGR10BE: AVPixelFormat = 196;
pub const AVPixelFormat_AV_PIX_FMT_P210BE: AVPixelFormat = 197;
pub const AVPixelFormat_AV_PIX_FMT_P210LE: AVPixelFormat = 198;
pub const AVPixelFormat_AV_PIX_FMT_P410BE: AVPixelFormat = 199;
pub const AVPixelFormat_AV_PIX_FMT_P410LE: AVPixelFormat = 200;
pub const AVPixelFormat_AV_PIX_FMT_P216BE: AVPixelFormat = 201;
pub const AVPixelFormat_AV_PIX_FMT_P216LE: AVPixelFormat = 202;
pub const AVPixelFormat_AV_PIX_FMT_P416BE: AVPixelFormat = 203;
pub const AVPixelFormat_AV_PIX_FMT_P416LE: AVPixelFormat = 204;
pub const AVPixelFormat_AV_PIX_FMT_VUYA: AVPixelFormat = 205;
pub const AVPixelFormat_AV_PIX_FMT_RGBAF16BE: AVPixelFormat = 206;
pub const AVPixelFormat_AV_PIX_FMT_RGBAF16LE: AVPixelFormat = 207;
pub const AVPixelFormat_AV_PIX_FMT_VUYX: AVPixelFormat = 208;
pub const AVPixelFormat_AV_PIX_FMT_P012LE: AVPixelFormat = 209;
pub const AVPixelFormat_AV_PIX_FMT_P012BE: AVPixelFormat = 210;
pub const AVPixelFormat_AV_PIX_FMT_Y212BE: AVPixelFormat = 211;
pub const AVPixelFormat_AV_PIX_FMT_Y212LE: AVPixelFormat = 212;
pub const AVPixelFormat_AV_PIX_FMT_XV30BE: AVPixelFormat = 213;
pub const AVPixelFormat_AV_PIX_FMT_XV30LE: AVPixelFormat = 214;
pub const AVPixelFormat_AV_PIX_FMT_XV36BE: AVPixelFormat = 215;
pub const AVPixelFormat_AV_PIX_FMT_XV36LE: AVPixelFormat = 216;
pub const AVPixelFormat_AV_PIX_FMT_RGBF32BE: AVPixelFormat = 217;
pub const AVPixelFormat_AV_PIX_FMT_RGBF32LE: AVPixelFormat = 218;
pub const AVPixelFormat_AV_PIX_FMT_RGBAF32BE: AVPixelFormat = 219;
pub const AVPixelFormat_AV_PIX_FMT_RGBAF32LE: AVPixelFormat = 220;
pub const AVPixelFormat_AV_PIX_FMT_P212BE: AVPixelFormat = 221;
pub const AVPixelFormat_AV_PIX_FMT_P212LE: AVPixelFormat = 222;
pub const AVPixelFormat_AV_PIX_FMT_P412BE: AVPixelFormat = 223;
pub const AVPixelFormat_AV_PIX_FMT_P412LE: AVPixelFormat = 224;
pub const AVPixelFormat_AV_PIX_FMT_GBRAP14BE: AVPixelFormat = 225;
pub const AVPixelFormat_AV_PIX_FMT_GBRAP14LE: AVPixelFormat = 226;
pub const AVPixelFormat_AV_PIX_FMT_D3D12: AVPixelFormat = 227;
pub const AVPixelFormat_AV_PIX_FMT_AYUV: AVPixelFormat = 228;
pub const AVPixelFormat_AV_PIX_FMT_UYVA: AVPixelFormat = 229;
pub const AVPixelFormat_AV_PIX_FMT_VYU444: AVPixelFormat = 230;
pub const AVPixelFormat_AV_PIX_FMT_V30XBE: AVPixelFormat = 231;
pub const AVPixelFormat_AV_PIX_FMT_V30XLE: AVPixelFormat = 232;
pub const AVPixelFormat_AV_PIX_FMT_RGBF16BE: AVPixelFormat = 233;
pub const AVPixelFormat_AV_PIX_FMT_RGBF16LE: AVPixelFormat = 234;
pub const AVPixelFormat_AV_PIX_FMT_RGBA128BE: AVPixelFormat = 235;
pub const AVPixelFormat_AV_PIX_FMT_RGBA128LE: AVPixelFormat = 236;
pub const AVPixelFormat_AV_PIX_FMT_RGB96BE: AVPixelFormat = 237;
pub const AVPixelFormat_AV_PIX_FMT_RGB96LE: AVPixelFormat = 238;
pub const AVPixelFormat_AV_PIX_FMT_Y216BE: AVPixelFormat = 239;
pub const AVPixelFormat_AV_PIX_FMT_Y216LE: AVPixelFormat = 240;
pub const AVPixelFormat_AV_PIX_FMT_XV48BE: AVPixelFormat = 241;
pub const AVPixelFormat_AV_PIX_FMT_XV48LE: AVPixelFormat = 242;
pub const AVPixelFormat_AV_PIX_FMT_GBRPF16BE: AVPixelFormat = 243;
pub const AVPixelFormat_AV_PIX_FMT_GBRPF16LE: AVPixelFormat = 244;
pub const AVPixelFormat_AV_PIX_FMT_GBRAPF16BE: AVPixelFormat = 245;
pub const AVPixelFormat_AV_PIX_FMT_GBRAPF16LE: AVPixelFormat = 246;
pub const AVPixelFormat_AV_PIX_FMT_GRAYF16BE: AVPixelFormat = 247;
pub const AVPixelFormat_AV_PIX_FMT_GRAYF16LE: AVPixelFormat = 248;
pub const AVPixelFormat_AV_PIX_FMT_AMF_SURFACE: AVPixelFormat = 249;
pub const AVPixelFormat_AV_PIX_FMT_GRAY32BE: AVPixelFormat = 250;
pub const AVPixelFormat_AV_PIX_FMT_GRAY32LE: AVPixelFormat = 251;
pub const AVPixelFormat_AV_PIX_FMT_YAF32BE: AVPixelFormat = 252;
pub const AVPixelFormat_AV_PIX_FMT_YAF32LE: AVPixelFormat = 253;
pub const AVPixelFormat_AV_PIX_FMT_YAF16BE: AVPixelFormat = 254;
pub const AVPixelFormat_AV_PIX_FMT_YAF16LE: AVPixelFormat = 255;
pub const AVPixelFormat_AV_PIX_FMT_GBRAP32BE: AVPixelFormat = 256;
pub const AVPixelFormat_AV_PIX_FMT_GBRAP32LE: AVPixelFormat = 257;
pub const AVPixelFormat_AV_PIX_FMT_YUV444P10MSBBE: AVPixelFormat = 258;
pub const AVPixelFormat_AV_PIX_FMT_YUV444P10MSBLE: AVPixelFormat = 259;
pub const AVPixelFormat_AV_PIX_FMT_YUV444P12MSBBE: AVPixelFormat = 260;
pub const AVPixelFormat_AV_PIX_FMT_YUV444P12MSBLE: AVPixelFormat = 261;
pub const AVPixelFormat_AV_PIX_FMT_GBRP10MSBBE: AVPixelFormat = 262;
pub const AVPixelFormat_AV_PIX_FMT_GBRP10MSBLE: AVPixelFormat = 263;
pub const AVPixelFormat_AV_PIX_FMT_GBRP12MSBBE: AVPixelFormat = 264;
pub const AVPixelFormat_AV_PIX_FMT_GBRP12MSBLE: AVPixelFormat = 265;
pub const AVPixelFormat_AV_PIX_FMT_OHCODEC: AVPixelFormat = 266;
pub const AVPixelFormat_AV_PIX_FMT_NB: AVPixelFormat = 267;
pub type AVPixelFormat = ::std::os::raw::c_int;
pub const AVColorPrimaries_AVCOL_PRI_RESERVED0: AVColorPrimaries = 0;
pub const AVColorPrimaries_AVCOL_PRI_BT709: AVColorPrimaries = 1;
pub const AVColorPrimaries_AVCOL_PRI_UNSPECIFIED: AVColorPrimaries = 2;
pub const AVColorPrimaries_AVCOL_PRI_RESERVED: AVColorPrimaries = 3;
pub const AVColorPrimaries_AVCOL_PRI_BT470M: AVColorPrimaries = 4;
pub const AVColorPrimaries_AVCOL_PRI_BT470BG: AVColorPrimaries = 5;
pub const AVColorPrimaries_AVCOL_PRI_SMPTE170M: AVColorPrimaries = 6;
pub const AVColorPrimaries_AVCOL_PRI_SMPTE240M: AVColorPrimaries = 7;
pub const AVColorPrimaries_AVCOL_PRI_FILM: AVColorPrimaries = 8;
pub const AVColorPrimaries_AVCOL_PRI_BT2020: AVColorPrimaries = 9;
pub const AVColorPrimaries_AVCOL_PRI_SMPTE428: AVColorPrimaries = 10;
pub const AVColorPrimaries_AVCOL_PRI_SMPTEST428_1: AVColorPrimaries = 10;
pub const AVColorPrimaries_AVCOL_PRI_SMPTE431: AVColorPrimaries = 11;
pub const AVColorPrimaries_AVCOL_PRI_SMPTE432: AVColorPrimaries = 12;
pub const AVColorPrimaries_AVCOL_PRI_EBU3213: AVColorPrimaries = 22;
pub const AVColorPrimaries_AVCOL_PRI_JEDEC_P22: AVColorPrimaries = 22;
pub const AVColorPrimaries_AVCOL_PRI_NB: AVColorPrimaries = 23;
pub const AVColorPrimaries_AVCOL_PRI_EXT_BASE: AVColorPrimaries = 256;
pub const AVColorPrimaries_AVCOL_PRI_V_GAMUT: AVColorPrimaries = 256;
pub const AVColorPrimaries_AVCOL_PRI_EXT_NB: AVColorPrimaries = 257;
pub type AVColorPrimaries = ::std::os::raw::c_int;
pub const AVColorTransferCharacteristic_AVCOL_TRC_RESERVED0: AVColorTransferCharacteristic = 0;
pub const AVColorTransferCharacteristic_AVCOL_TRC_BT709: AVColorTransferCharacteristic = 1;
pub const AVColorTransferCharacteristic_AVCOL_TRC_UNSPECIFIED: AVColorTransferCharacteristic = 2;
pub const AVColorTransferCharacteristic_AVCOL_TRC_RESERVED: AVColorTransferCharacteristic = 3;
pub const AVColorTransferCharacteristic_AVCOL_TRC_GAMMA22: AVColorTransferCharacteristic = 4;
pub const AVColorTransferCharacteristic_AVCOL_TRC_GAMMA28: AVColorTransferCharacteristic = 5;
pub const AVColorTransferCharacteristic_AVCOL_TRC_SMPTE170M: AVColorTransferCharacteristic = 6;
pub const AVColorTransferCharacteristic_AVCOL_TRC_SMPTE240M: AVColorTransferCharacteristic = 7;
pub const AVColorTransferCharacteristic_AVCOL_TRC_LINEAR: AVColorTransferCharacteristic = 8;
pub const AVColorTransferCharacteristic_AVCOL_TRC_LOG: AVColorTransferCharacteristic = 9;
pub const AVColorTransferCharacteristic_AVCOL_TRC_LOG_SQRT: AVColorTransferCharacteristic = 10;
pub const AVColorTransferCharacteristic_AVCOL_TRC_IEC61966_2_4: AVColorTransferCharacteristic = 11;
pub const AVColorTransferCharacteristic_AVCOL_TRC_BT1361_ECG: AVColorTransferCharacteristic = 12;
pub const AVColorTransferCharacteristic_AVCOL_TRC_IEC61966_2_1: AVColorTransferCharacteristic = 13;
pub const AVColorTransferCharacteristic_AVCOL_TRC_BT2020_10: AVColorTransferCharacteristic = 14;
pub const AVColorTransferCharacteristic_AVCOL_TRC_BT2020_12: AVColorTransferCharacteristic = 15;
pub const AVColorTransferCharacteristic_AVCOL_TRC_SMPTE2084: AVColorTransferCharacteristic = 16;
pub const AVColorTransferCharacteristic_AVCOL_TRC_SMPTEST2084: AVColorTransferCharacteristic = 16;
pub const AVColorTransferCharacteristic_AVCOL_TRC_SMPTE428: AVColorTransferCharacteristic = 17;
pub const AVColorTransferCharacteristic_AVCOL_TRC_SMPTEST428_1: AVColorTransferCharacteristic = 17;
pub const AVColorTransferCharacteristic_AVCOL_TRC_ARIB_STD_B67: AVColorTransferCharacteristic = 18;
pub const AVColorTransferCharacteristic_AVCOL_TRC_NB: AVColorTransferCharacteristic = 19;
pub const AVColorTransferCharacteristic_AVCOL_TRC_EXT_BASE: AVColorTransferCharacteristic = 256;
pub const AVColorTransferCharacteristic_AVCOL_TRC_V_LOG: AVColorTransferCharacteristic = 256;
pub const AVColorTransferCharacteristic_AVCOL_TRC_EXT_NB: AVColorTransferCharacteristic = 257;
pub type AVColorTransferCharacteristic = ::std::os::raw::c_int;
pub const AVColorSpace_AVCOL_SPC_RGB: AVColorSpace = 0;
pub const AVColorSpace_AVCOL_SPC_BT709: AVColorSpace = 1;
pub const AVColorSpace_AVCOL_SPC_UNSPECIFIED: AVColorSpace = 2;
pub const AVColorSpace_AVCOL_SPC_RESERVED: AVColorSpace = 3;
pub const AVColorSpace_AVCOL_SPC_FCC: AVColorSpace = 4;
pub const AVColorSpace_AVCOL_SPC_BT470BG: AVColorSpace = 5;
pub const AVColorSpace_AVCOL_SPC_SMPTE170M: AVColorSpace = 6;
pub const AVColorSpace_AVCOL_SPC_SMPTE240M: AVColorSpace = 7;
pub const AVColorSpace_AVCOL_SPC_YCGCO: AVColorSpace = 8;
pub const AVColorSpace_AVCOL_SPC_YCOCG: AVColorSpace = 8;
pub const AVColorSpace_AVCOL_SPC_BT2020_NCL: AVColorSpace = 9;
pub const AVColorSpace_AVCOL_SPC_BT2020_CL: AVColorSpace = 10;
pub const AVColorSpace_AVCOL_SPC_SMPTE2085: AVColorSpace = 11;
pub const AVColorSpace_AVCOL_SPC_CHROMA_DERIVED_NCL: AVColorSpace = 12;
pub const AVColorSpace_AVCOL_SPC_CHROMA_DERIVED_CL: AVColorSpace = 13;
pub const AVColorSpace_AVCOL_SPC_ICTCP: AVColorSpace = 14;
pub const AVColorSpace_AVCOL_SPC_IPT_C2: AVColorSpace = 15;
pub const AVColorSpace_AVCOL_SPC_YCGCO_RE: AVColorSpace = 16;
pub const AVColorSpace_AVCOL_SPC_YCGCO_RO: AVColorSpace = 17;
pub const AVColorSpace_AVCOL_SPC_NB: AVColorSpace = 18;
pub type AVColorSpace = ::std::os::raw::c_int;
pub const AVColorRange_AVCOL_RANGE_UNSPECIFIED: AVColorRange = 0;
pub const AVColorRange_AVCOL_RANGE_MPEG: AVColorRange = 1;
pub const AVColorRange_AVCOL_RANGE_JPEG: AVColorRange = 2;
pub const AVColorRange_AVCOL_RANGE_NB: AVColorRange = 3;
pub type AVColorRange = ::std::os::raw::c_int;
pub const AVChromaLocation_AVCHROMA_LOC_UNSPECIFIED: AVChromaLocation = 0;
pub const AVChromaLocation_AVCHROMA_LOC_LEFT: AVChromaLocation = 1;
pub const AVChromaLocation_AVCHROMA_LOC_CENTER: AVChromaLocation = 2;
pub const AVChromaLocation_AVCHROMA_LOC_TOPLEFT: AVChromaLocation = 3;
pub const AVChromaLocation_AVCHROMA_LOC_TOP: AVChromaLocation = 4;
pub const AVChromaLocation_AVCHROMA_LOC_BOTTOMLEFT: AVChromaLocation = 5;
pub const AVChromaLocation_AVCHROMA_LOC_BOTTOM: AVChromaLocation = 6;
pub const AVChromaLocation_AVCHROMA_LOC_NB: AVChromaLocation = 7;
pub type AVChromaLocation = ::std::os::raw::c_int;
pub const AVAlphaMode_AVALPHA_MODE_UNSPECIFIED: AVAlphaMode = 0;
pub const AVAlphaMode_AVALPHA_MODE_PREMULTIPLIED: AVAlphaMode = 1;
pub const AVAlphaMode_AVALPHA_MODE_STRAIGHT: AVAlphaMode = 2;
pub const AVAlphaMode_AVALPHA_MODE_NB: AVAlphaMode = 3;
pub type AVAlphaMode = ::std::os::raw::c_int;
unsafe extern "C" {
    pub fn av_int_list_length_for_size(
        elsize: ::std::os::raw::c_uint,
        list: *const ::std::os::raw::c_void,
        term: u64,
    ) -> ::std::os::raw::c_uint;
}
unsafe extern "C" {
    pub fn av_get_time_base_q() -> AVRational;
}
unsafe extern "C" {
    pub fn av_fourcc_make_string(
        buf: *mut ::std::os::raw::c_char,
        fourcc: u32,
    ) -> *mut ::std::os::raw::c_char;
}
#[repr(C)]
#[derive(Debug)]
pub struct AVBuffer {
    _unused: [u8; 0],
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVBufferRef {
    pub buffer: *mut AVBuffer,
    pub data: *mut u8,
    pub size: usize,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVBufferRef"][::std::mem::size_of::<AVBufferRef>() - 24usize];
    ["Alignment of AVBufferRef"][::std::mem::align_of::<AVBufferRef>() - 8usize];
    ["Offset of field: AVBufferRef::buffer"][::std::mem::offset_of!(AVBufferRef, buffer) - 0usize];
    ["Offset of field: AVBufferRef::data"][::std::mem::offset_of!(AVBufferRef, data) - 8usize];
    ["Offset of field: AVBufferRef::size"][::std::mem::offset_of!(AVBufferRef, size) - 16usize];
};
unsafe extern "C" {
    pub fn av_buffer_alloc(size: usize) -> *mut AVBufferRef;
}
unsafe extern "C" {
    pub fn av_buffer_allocz(size: usize) -> *mut AVBufferRef;
}
unsafe extern "C" {
    pub fn av_buffer_create(
        data: *mut u8,
        size: usize,
        free: ::std::option::Option<
            unsafe extern "C" fn(opaque: *mut ::std::os::raw::c_void, data: *mut u8),
        >,
        opaque: *mut ::std::os::raw::c_void,
        flags: ::std::os::raw::c_int,
    ) -> *mut AVBufferRef;
}
unsafe extern "C" {
    pub fn av_buffer_default_free(opaque: *mut ::std::os::raw::c_void, data: *mut u8);
}
unsafe extern "C" {
    pub fn av_buffer_ref(buf: *const AVBufferRef) -> *mut AVBufferRef;
}
unsafe extern "C" {
    pub fn av_buffer_unref(buf: *mut *mut AVBufferRef);
}
unsafe extern "C" {
    pub fn av_buffer_is_writable(buf: *const AVBufferRef) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_buffer_get_opaque(buf: *const AVBufferRef) -> *mut ::std::os::raw::c_void;
}
unsafe extern "C" {
    pub fn av_buffer_get_ref_count(buf: *const AVBufferRef) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_buffer_make_writable(buf: *mut *mut AVBufferRef) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_buffer_realloc(buf: *mut *mut AVBufferRef, size: usize) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_buffer_replace(
        dst: *mut *mut AVBufferRef,
        src: *const AVBufferRef,
    ) -> ::std::os::raw::c_int;
}
#[repr(C)]
#[derive(Debug)]
pub struct AVBufferPool {
    _unused: [u8; 0],
}
unsafe extern "C" {
    pub fn av_buffer_pool_init(
        size: usize,
        alloc: ::std::option::Option<unsafe extern "C" fn(size: usize) -> *mut AVBufferRef>,
    ) -> *mut AVBufferPool;
}
unsafe extern "C" {
    pub fn av_buffer_pool_init2(
        size: usize,
        opaque: *mut ::std::os::raw::c_void,
        alloc: ::std::option::Option<
            unsafe extern "C" fn(
                opaque: *mut ::std::os::raw::c_void,
                size: usize,
            ) -> *mut AVBufferRef,
        >,
        pool_free: ::std::option::Option<unsafe extern "C" fn(opaque: *mut ::std::os::raw::c_void)>,
    ) -> *mut AVBufferPool;
}
unsafe extern "C" {
    pub fn av_buffer_pool_uninit(pool: *mut *mut AVBufferPool);
}
unsafe extern "C" {
    pub fn av_buffer_pool_get(pool: *mut AVBufferPool) -> *mut AVBufferRef;
}
unsafe extern "C" {
    pub fn av_buffer_pool_buffer_get_opaque(
        ref_: *const AVBufferRef,
    ) -> *mut ::std::os::raw::c_void;
}
pub const AVChannel_AV_CHAN_NONE: AVChannel = -1;
pub const AVChannel_AV_CHAN_FRONT_LEFT: AVChannel = 0;
pub const AVChannel_AV_CHAN_FRONT_RIGHT: AVChannel = 1;
pub const AVChannel_AV_CHAN_FRONT_CENTER: AVChannel = 2;
pub const AVChannel_AV_CHAN_LOW_FREQUENCY: AVChannel = 3;
pub const AVChannel_AV_CHAN_BACK_LEFT: AVChannel = 4;
pub const AVChannel_AV_CHAN_BACK_RIGHT: AVChannel = 5;
pub const AVChannel_AV_CHAN_FRONT_LEFT_OF_CENTER: AVChannel = 6;
pub const AVChannel_AV_CHAN_FRONT_RIGHT_OF_CENTER: AVChannel = 7;
pub const AVChannel_AV_CHAN_BACK_CENTER: AVChannel = 8;
pub const AVChannel_AV_CHAN_SIDE_LEFT: AVChannel = 9;
pub const AVChannel_AV_CHAN_SIDE_RIGHT: AVChannel = 10;
pub const AVChannel_AV_CHAN_TOP_CENTER: AVChannel = 11;
pub const AVChannel_AV_CHAN_TOP_FRONT_LEFT: AVChannel = 12;
pub const AVChannel_AV_CHAN_TOP_FRONT_CENTER: AVChannel = 13;
pub const AVChannel_AV_CHAN_TOP_FRONT_RIGHT: AVChannel = 14;
pub const AVChannel_AV_CHAN_TOP_BACK_LEFT: AVChannel = 15;
pub const AVChannel_AV_CHAN_TOP_BACK_CENTER: AVChannel = 16;
pub const AVChannel_AV_CHAN_TOP_BACK_RIGHT: AVChannel = 17;
pub const AVChannel_AV_CHAN_STEREO_LEFT: AVChannel = 29;
pub const AVChannel_AV_CHAN_STEREO_RIGHT: AVChannel = 30;
pub const AVChannel_AV_CHAN_WIDE_LEFT: AVChannel = 31;
pub const AVChannel_AV_CHAN_WIDE_RIGHT: AVChannel = 32;
pub const AVChannel_AV_CHAN_SURROUND_DIRECT_LEFT: AVChannel = 33;
pub const AVChannel_AV_CHAN_SURROUND_DIRECT_RIGHT: AVChannel = 34;
pub const AVChannel_AV_CHAN_LOW_FREQUENCY_2: AVChannel = 35;
pub const AVChannel_AV_CHAN_TOP_SIDE_LEFT: AVChannel = 36;
pub const AVChannel_AV_CHAN_TOP_SIDE_RIGHT: AVChannel = 37;
pub const AVChannel_AV_CHAN_BOTTOM_FRONT_CENTER: AVChannel = 38;
pub const AVChannel_AV_CHAN_BOTTOM_FRONT_LEFT: AVChannel = 39;
pub const AVChannel_AV_CHAN_BOTTOM_FRONT_RIGHT: AVChannel = 40;
pub const AVChannel_AV_CHAN_SIDE_SURROUND_LEFT: AVChannel = 41;
pub const AVChannel_AV_CHAN_SIDE_SURROUND_RIGHT: AVChannel = 42;
pub const AVChannel_AV_CHAN_TOP_SURROUND_LEFT: AVChannel = 43;
pub const AVChannel_AV_CHAN_TOP_SURROUND_RIGHT: AVChannel = 44;
pub const AVChannel_AV_CHAN_BINAURAL_LEFT: AVChannel = 61;
pub const AVChannel_AV_CHAN_BINAURAL_RIGHT: AVChannel = 62;
pub const AVChannel_AV_CHAN_UNUSED: AVChannel = 512;
pub const AVChannel_AV_CHAN_UNKNOWN: AVChannel = 768;
pub const AVChannel_AV_CHAN_AMBISONIC_BASE: AVChannel = 1024;
pub const AVChannel_AV_CHAN_AMBISONIC_END: AVChannel = 2047;
pub type AVChannel = ::std::os::raw::c_int;
pub const AVChannelOrder_AV_CHANNEL_ORDER_UNSPEC: AVChannelOrder = 0;
pub const AVChannelOrder_AV_CHANNEL_ORDER_NATIVE: AVChannelOrder = 1;
pub const AVChannelOrder_AV_CHANNEL_ORDER_CUSTOM: AVChannelOrder = 2;
pub const AVChannelOrder_AV_CHANNEL_ORDER_AMBISONIC: AVChannelOrder = 3;
pub const AVChannelOrder_FF_CHANNEL_ORDER_NB: AVChannelOrder = 4;
pub type AVChannelOrder = ::std::os::raw::c_int;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVChannelCustom {
    pub id: AVChannel,
    pub name: [::std::os::raw::c_char; 16usize],
    pub opaque: *mut ::std::os::raw::c_void,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVChannelCustom"][::std::mem::size_of::<AVChannelCustom>() - 32usize];
    ["Alignment of AVChannelCustom"][::std::mem::align_of::<AVChannelCustom>() - 8usize];
    ["Offset of field: AVChannelCustom::id"][::std::mem::offset_of!(AVChannelCustom, id) - 0usize];
    ["Offset of field: AVChannelCustom::name"]
        [::std::mem::offset_of!(AVChannelCustom, name) - 4usize];
    ["Offset of field: AVChannelCustom::opaque"]
        [::std::mem::offset_of!(AVChannelCustom, opaque) - 24usize];
};
#[repr(C)]
#[derive(Copy, Clone)]
pub struct AVChannelLayout {
    pub order: AVChannelOrder,
    pub nb_channels: ::std::os::raw::c_int,
    pub u: AVChannelLayout__bindgen_ty_1,
    pub opaque: *mut ::std::os::raw::c_void,
}
#[repr(C)]
#[derive(Copy, Clone)]
pub union AVChannelLayout__bindgen_ty_1 {
    pub mask: u64,
    pub map: *mut AVChannelCustom,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVChannelLayout__bindgen_ty_1"]
        [::std::mem::size_of::<AVChannelLayout__bindgen_ty_1>() - 8usize];
    ["Alignment of AVChannelLayout__bindgen_ty_1"]
        [::std::mem::align_of::<AVChannelLayout__bindgen_ty_1>() - 8usize];
    ["Offset of field: AVChannelLayout__bindgen_ty_1::mask"]
        [::std::mem::offset_of!(AVChannelLayout__bindgen_ty_1, mask) - 0usize];
    ["Offset of field: AVChannelLayout__bindgen_ty_1::map"]
        [::std::mem::offset_of!(AVChannelLayout__bindgen_ty_1, map) - 0usize];
};
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVChannelLayout"][::std::mem::size_of::<AVChannelLayout>() - 24usize];
    ["Alignment of AVChannelLayout"][::std::mem::align_of::<AVChannelLayout>() - 8usize];
    ["Offset of field: AVChannelLayout::order"]
        [::std::mem::offset_of!(AVChannelLayout, order) - 0usize];
    ["Offset of field: AVChannelLayout::nb_channels"]
        [::std::mem::offset_of!(AVChannelLayout, nb_channels) - 4usize];
    ["Offset of field: AVChannelLayout::u"][::std::mem::offset_of!(AVChannelLayout, u) - 8usize];
    ["Offset of field: AVChannelLayout::opaque"]
        [::std::mem::offset_of!(AVChannelLayout, opaque) - 16usize];
};
#[repr(C)]
#[derive(Debug)]
pub struct AVBPrint {
    _unused: [u8; 0],
}
unsafe extern "C" {
    pub fn av_channel_name(
        buf: *mut ::std::os::raw::c_char,
        buf_size: usize,
        channel: AVChannel,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_channel_name_bprint(bp: *mut AVBPrint, channel_id: AVChannel);
}
unsafe extern "C" {
    pub fn av_channel_description(
        buf: *mut ::std::os::raw::c_char,
        buf_size: usize,
        channel: AVChannel,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_channel_description_bprint(bp: *mut AVBPrint, channel_id: AVChannel);
}
unsafe extern "C" {
    pub fn av_channel_from_string(name: *const ::std::os::raw::c_char) -> AVChannel;
}
unsafe extern "C" {
    pub fn av_channel_layout_custom_init(
        channel_layout: *mut AVChannelLayout,
        nb_channels: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_channel_layout_from_mask(
        channel_layout: *mut AVChannelLayout,
        mask: u64,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_channel_layout_from_string(
        channel_layout: *mut AVChannelLayout,
        str_: *const ::std::os::raw::c_char,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_channel_layout_default(
        ch_layout: *mut AVChannelLayout,
        nb_channels: ::std::os::raw::c_int,
    );
}
unsafe extern "C" {
    pub fn av_channel_layout_standard(
        opaque: *mut *mut ::std::os::raw::c_void,
    ) -> *const AVChannelLayout;
}
unsafe extern "C" {
    pub fn av_channel_layout_uninit(channel_layout: *mut AVChannelLayout);
}
unsafe extern "C" {
    pub fn av_channel_layout_copy(
        dst: *mut AVChannelLayout,
        src: *const AVChannelLayout,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_channel_layout_describe(
        channel_layout: *const AVChannelLayout,
        buf: *mut ::std::os::raw::c_char,
        buf_size: usize,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_channel_layout_describe_bprint(
        channel_layout: *const AVChannelLayout,
        bp: *mut AVBPrint,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_channel_layout_channel_from_index(
        channel_layout: *const AVChannelLayout,
        idx: ::std::os::raw::c_uint,
    ) -> AVChannel;
}
unsafe extern "C" {
    pub fn av_channel_layout_index_from_channel(
        channel_layout: *const AVChannelLayout,
        channel: AVChannel,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_channel_layout_index_from_string(
        channel_layout: *const AVChannelLayout,
        name: *const ::std::os::raw::c_char,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_channel_layout_channel_from_string(
        channel_layout: *const AVChannelLayout,
        name: *const ::std::os::raw::c_char,
    ) -> AVChannel;
}
unsafe extern "C" {
    pub fn av_channel_layout_subset(channel_layout: *const AVChannelLayout, mask: u64) -> u64;
}
unsafe extern "C" {
    pub fn av_channel_layout_check(channel_layout: *const AVChannelLayout)
        -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_channel_layout_compare(
        chl: *const AVChannelLayout,
        chl1: *const AVChannelLayout,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_channel_layout_ambisonic_order(
        channel_layout: *const AVChannelLayout,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_channel_layout_retype(
        channel_layout: *mut AVChannelLayout,
        order: AVChannelOrder,
        flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVDictionaryEntry {
    pub key: *mut ::std::os::raw::c_char,
    pub value: *mut ::std::os::raw::c_char,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVDictionaryEntry"][::std::mem::size_of::<AVDictionaryEntry>() - 16usize];
    ["Alignment of AVDictionaryEntry"][::std::mem::align_of::<AVDictionaryEntry>() - 8usize];
    ["Offset of field: AVDictionaryEntry::key"]
        [::std::mem::offset_of!(AVDictionaryEntry, key) - 0usize];
    ["Offset of field: AVDictionaryEntry::value"]
        [::std::mem::offset_of!(AVDictionaryEntry, value) - 8usize];
};
#[repr(C)]
#[derive(Debug)]
pub struct AVDictionary {
    _unused: [u8; 0],
}
unsafe extern "C" {
    pub fn av_dict_get(
        m: *const AVDictionary,
        key: *const ::std::os::raw::c_char,
        prev: *const AVDictionaryEntry,
        flags: ::std::os::raw::c_int,
    ) -> *mut AVDictionaryEntry;
}
unsafe extern "C" {
    pub fn av_dict_iterate(
        m: *const AVDictionary,
        prev: *const AVDictionaryEntry,
    ) -> *const AVDictionaryEntry;
}
unsafe extern "C" {
    pub fn av_dict_count(m: *const AVDictionary) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_dict_set(
        pm: *mut *mut AVDictionary,
        key: *const ::std::os::raw::c_char,
        value: *const ::std::os::raw::c_char,
        flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_dict_set_int(
        pm: *mut *mut AVDictionary,
        key: *const ::std::os::raw::c_char,
        value: i64,
        flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_dict_parse_string(
        pm: *mut *mut AVDictionary,
        str_: *const ::std::os::raw::c_char,
        key_val_sep: *const ::std::os::raw::c_char,
        pairs_sep: *const ::std::os::raw::c_char,
        flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_dict_copy(
        dst: *mut *mut AVDictionary,
        src: *const AVDictionary,
        flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_dict_free(m: *mut *mut AVDictionary);
}
unsafe extern "C" {
    pub fn av_dict_get_string(
        m: *const AVDictionary,
        buffer: *mut *mut ::std::os::raw::c_char,
        key_val_sep: ::std::os::raw::c_char,
        pairs_sep: ::std::os::raw::c_char,
    ) -> ::std::os::raw::c_int;
}
pub const AVFrameSideDataType_AV_FRAME_DATA_PANSCAN: AVFrameSideDataType = 0;
pub const AVFrameSideDataType_AV_FRAME_DATA_A53_CC: AVFrameSideDataType = 1;
pub const AVFrameSideDataType_AV_FRAME_DATA_STEREO3D: AVFrameSideDataType = 2;
pub const AVFrameSideDataType_AV_FRAME_DATA_MATRIXENCODING: AVFrameSideDataType = 3;
pub const AVFrameSideDataType_AV_FRAME_DATA_DOWNMIX_INFO: AVFrameSideDataType = 4;
pub const AVFrameSideDataType_AV_FRAME_DATA_REPLAYGAIN: AVFrameSideDataType = 5;
pub const AVFrameSideDataType_AV_FRAME_DATA_DISPLAYMATRIX: AVFrameSideDataType = 6;
pub const AVFrameSideDataType_AV_FRAME_DATA_AFD: AVFrameSideDataType = 7;
pub const AVFrameSideDataType_AV_FRAME_DATA_MOTION_VECTORS: AVFrameSideDataType = 8;
pub const AVFrameSideDataType_AV_FRAME_DATA_SKIP_SAMPLES: AVFrameSideDataType = 9;
pub const AVFrameSideDataType_AV_FRAME_DATA_AUDIO_SERVICE_TYPE: AVFrameSideDataType = 10;
pub const AVFrameSideDataType_AV_FRAME_DATA_MASTERING_DISPLAY_METADATA: AVFrameSideDataType = 11;
pub const AVFrameSideDataType_AV_FRAME_DATA_GOP_TIMECODE: AVFrameSideDataType = 12;
pub const AVFrameSideDataType_AV_FRAME_DATA_SPHERICAL: AVFrameSideDataType = 13;
pub const AVFrameSideDataType_AV_FRAME_DATA_CONTENT_LIGHT_LEVEL: AVFrameSideDataType = 14;
pub const AVFrameSideDataType_AV_FRAME_DATA_ICC_PROFILE: AVFrameSideDataType = 15;
pub const AVFrameSideDataType_AV_FRAME_DATA_S12M_TIMECODE: AVFrameSideDataType = 16;
pub const AVFrameSideDataType_AV_FRAME_DATA_DYNAMIC_HDR_PLUS: AVFrameSideDataType = 17;
pub const AVFrameSideDataType_AV_FRAME_DATA_REGIONS_OF_INTEREST: AVFrameSideDataType = 18;
pub const AVFrameSideDataType_AV_FRAME_DATA_VIDEO_ENC_PARAMS: AVFrameSideDataType = 19;
pub const AVFrameSideDataType_AV_FRAME_DATA_SEI_UNREGISTERED: AVFrameSideDataType = 20;
pub const AVFrameSideDataType_AV_FRAME_DATA_FILM_GRAIN_PARAMS: AVFrameSideDataType = 21;
pub const AVFrameSideDataType_AV_FRAME_DATA_DETECTION_BBOXES: AVFrameSideDataType = 22;
pub const AVFrameSideDataType_AV_FRAME_DATA_DOVI_RPU_BUFFER: AVFrameSideDataType = 23;
pub const AVFrameSideDataType_AV_FRAME_DATA_DOVI_METADATA: AVFrameSideDataType = 24;
pub const AVFrameSideDataType_AV_FRAME_DATA_DYNAMIC_HDR_VIVID: AVFrameSideDataType = 25;
pub const AVFrameSideDataType_AV_FRAME_DATA_AMBIENT_VIEWING_ENVIRONMENT: AVFrameSideDataType = 26;
pub const AVFrameSideDataType_AV_FRAME_DATA_VIDEO_HINT: AVFrameSideDataType = 27;
pub const AVFrameSideDataType_AV_FRAME_DATA_LCEVC: AVFrameSideDataType = 28;
pub const AVFrameSideDataType_AV_FRAME_DATA_VIEW_ID: AVFrameSideDataType = 29;
pub const AVFrameSideDataType_AV_FRAME_DATA_3D_REFERENCE_DISPLAYS: AVFrameSideDataType = 30;
pub const AVFrameSideDataType_AV_FRAME_DATA_EXIF: AVFrameSideDataType = 31;
pub type AVFrameSideDataType = ::std::os::raw::c_int;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVFrameSideData {
    pub type_: AVFrameSideDataType,
    pub data: *mut u8,
    pub size: usize,
    pub metadata: *mut AVDictionary,
    pub buf: *mut AVBufferRef,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVFrameSideData"][::std::mem::size_of::<AVFrameSideData>() - 40usize];
    ["Alignment of AVFrameSideData"][::std::mem::align_of::<AVFrameSideData>() - 8usize];
    ["Offset of field: AVFrameSideData::type_"]
        [::std::mem::offset_of!(AVFrameSideData, type_) - 0usize];
    ["Offset of field: AVFrameSideData::data"]
        [::std::mem::offset_of!(AVFrameSideData, data) - 8usize];
    ["Offset of field: AVFrameSideData::size"]
        [::std::mem::offset_of!(AVFrameSideData, size) - 16usize];
    ["Offset of field: AVFrameSideData::metadata"]
        [::std::mem::offset_of!(AVFrameSideData, metadata) - 24usize];
    ["Offset of field: AVFrameSideData::buf"]
        [::std::mem::offset_of!(AVFrameSideData, buf) - 32usize];
};
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVSideDataDescriptor {
    pub name: *const ::std::os::raw::c_char,
    pub props: ::std::os::raw::c_uint,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVSideDataDescriptor"][::std::mem::size_of::<AVSideDataDescriptor>() - 16usize];
    ["Alignment of AVSideDataDescriptor"][::std::mem::align_of::<AVSideDataDescriptor>() - 8usize];
    ["Offset of field: AVSideDataDescriptor::name"]
        [::std::mem::offset_of!(AVSideDataDescriptor, name) - 0usize];
    ["Offset of field: AVSideDataDescriptor::props"]
        [::std::mem::offset_of!(AVSideDataDescriptor, props) - 8usize];
};
#[repr(C)]
#[derive(Copy, Clone)]
pub struct AVFrame {
    pub data: [*mut u8; 8usize],
    pub linesize: [::std::os::raw::c_int; 8usize],
    pub extended_data: *mut *mut u8,
    pub width: ::std::os::raw::c_int,
    pub height: ::std::os::raw::c_int,
    pub nb_samples: ::std::os::raw::c_int,
    pub format: ::std::os::raw::c_int,
    pub pict_type: AVPictureType,
    pub sample_aspect_ratio: AVRational,
    pub pts: i64,
    pub pkt_dts: i64,
    pub time_base: AVRational,
    pub quality: ::std::os::raw::c_int,
    pub opaque: *mut ::std::os::raw::c_void,
    pub repeat_pict: ::std::os::raw::c_int,
    pub sample_rate: ::std::os::raw::c_int,
    pub buf: [*mut AVBufferRef; 8usize],
    pub extended_buf: *mut *mut AVBufferRef,
    pub nb_extended_buf: ::std::os::raw::c_int,
    pub side_data: *mut *mut AVFrameSideData,
    pub nb_side_data: ::std::os::raw::c_int,
    pub flags: ::std::os::raw::c_int,
    pub color_range: AVColorRange,
    pub color_primaries: AVColorPrimaries,
    pub color_trc: AVColorTransferCharacteristic,
    pub colorspace: AVColorSpace,
    pub chroma_location: AVChromaLocation,
    pub best_effort_timestamp: i64,
    pub metadata: *mut AVDictionary,
    pub decode_error_flags: ::std::os::raw::c_int,
    pub hw_frames_ctx: *mut AVBufferRef,
    pub opaque_ref: *mut AVBufferRef,
    pub crop_top: usize,
    pub crop_bottom: usize,
    pub crop_left: usize,
    pub crop_right: usize,
    pub private_ref: *mut ::std::os::raw::c_void,
    pub ch_layout: AVChannelLayout,
    pub duration: i64,
    pub alpha_mode: AVAlphaMode,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVFrame"][::std::mem::size_of::<AVFrame>() - 424usize];
    ["Alignment of AVFrame"][::std::mem::align_of::<AVFrame>() - 8usize];
    ["Offset of field: AVFrame::data"][::std::mem::offset_of!(AVFrame, data) - 0usize];
    ["Offset of field: AVFrame::linesize"][::std::mem::offset_of!(AVFrame, linesize) - 64usize];
    ["Offset of field: AVFrame::extended_data"]
        [::std::mem::offset_of!(AVFrame, extended_data) - 96usize];
    ["Offset of field: AVFrame::width"][::std::mem::offset_of!(AVFrame, width) - 104usize];
    ["Offset of field: AVFrame::height"][::std::mem::offset_of!(AVFrame, height) - 108usize];
    ["Offset of field: AVFrame::nb_samples"]
        [::std::mem::offset_of!(AVFrame, nb_samples) - 112usize];
    ["Offset of field: AVFrame::format"][::std::mem::offset_of!(AVFrame, format) - 116usize];
    ["Offset of field: AVFrame::pict_type"][::std::mem::offset_of!(AVFrame, pict_type) - 120usize];
    ["Offset of field: AVFrame::sample_aspect_ratio"]
        [::std::mem::offset_of!(AVFrame, sample_aspect_ratio) - 124usize];
    ["Offset of field: AVFrame::pts"][::std::mem::offset_of!(AVFrame, pts) - 136usize];
    ["Offset of field: AVFrame::pkt_dts"][::std::mem::offset_of!(AVFrame, pkt_dts) - 144usize];
    ["Offset of field: AVFrame::time_base"][::std::mem::offset_of!(AVFrame, time_base) - 152usize];
    ["Offset of field: AVFrame::quality"][::std::mem::offset_of!(AVFrame, quality) - 160usize];
    ["Offset of field: AVFrame::opaque"][::std::mem::offset_of!(AVFrame, opaque) - 168usize];
    ["Offset of field: AVFrame::repeat_pict"]
        [::std::mem::offset_of!(AVFrame, repeat_pict) - 176usize];
    ["Offset of field: AVFrame::sample_rate"]
        [::std::mem::offset_of!(AVFrame, sample_rate) - 180usize];
    ["Offset of field: AVFrame::buf"][::std::mem::offset_of!(AVFrame, buf) - 184usize];
    ["Offset of field: AVFrame::extended_buf"]
        [::std::mem::offset_of!(AVFrame, extended_buf) - 248usize];
    ["Offset of field: AVFrame::nb_extended_buf"]
        [::std::mem::offset_of!(AVFrame, nb_extended_buf) - 256usize];
    ["Offset of field: AVFrame::side_data"][::std::mem::offset_of!(AVFrame, side_data) - 264usize];
    ["Offset of field: AVFrame::nb_side_data"]
        [::std::mem::offset_of!(AVFrame, nb_side_data) - 272usize];
    ["Offset of field: AVFrame::flags"][::std::mem::offset_of!(AVFrame, flags) - 276usize];
    ["Offset of field: AVFrame::color_range"]
        [::std::mem::offset_of!(AVFrame, color_range) - 280usize];
    ["Offset of field: AVFrame::color_primaries"]
        [::std::mem::offset_of!(AVFrame, color_primaries) - 284usize];
    ["Offset of field: AVFrame::color_trc"][::std::mem::offset_of!(AVFrame, color_trc) - 288usize];
    ["Offset of field: AVFrame::colorspace"]
        [::std::mem::offset_of!(AVFrame, colorspace) - 292usize];
    ["Offset of field: AVFrame::chroma_location"]
        [::std::mem::offset_of!(AVFrame, chroma_location) - 296usize];
    ["Offset of field: AVFrame::best_effort_timestamp"]
        [::std::mem::offset_of!(AVFrame, best_effort_timestamp) - 304usize];
    ["Offset of field: AVFrame::metadata"][::std::mem::offset_of!(AVFrame, metadata) - 312usize];
    ["Offset of field: AVFrame::decode_error_flags"]
        [::std::mem::offset_of!(AVFrame, decode_error_flags) - 320usize];
    ["Offset of field: AVFrame::hw_frames_ctx"]
        [::std::mem::offset_of!(AVFrame, hw_frames_ctx) - 328usize];
    ["Offset of field: AVFrame::opaque_ref"]
        [::std::mem::offset_of!(AVFrame, opaque_ref) - 336usize];
    ["Offset of field: AVFrame::crop_top"][::std::mem::offset_of!(AVFrame, crop_top) - 344usize];
    ["Offset of field: AVFrame::crop_bottom"]
        [::std::mem::offset_of!(AVFrame, crop_bottom) - 352usize];
    ["Offset of field: AVFrame::crop_left"][::std::mem::offset_of!(AVFrame, crop_left) - 360usize];
    ["Offset of field: AVFrame::crop_right"]
        [::std::mem::offset_of!(AVFrame, crop_right) - 368usize];
    ["Offset of field: AVFrame::private_ref"]
        [::std::mem::offset_of!(AVFrame, private_ref) - 376usize];
    ["Offset of field: AVFrame::ch_layout"][::std::mem::offset_of!(AVFrame, ch_layout) - 384usize];
    ["Offset of field: AVFrame::duration"][::std::mem::offset_of!(AVFrame, duration) - 408usize];
    ["Offset of field: AVFrame::alpha_mode"]
        [::std::mem::offset_of!(AVFrame, alpha_mode) - 416usize];
};
unsafe extern "C" {
    pub fn av_frame_alloc() -> *mut AVFrame;
}
unsafe extern "C" {
    pub fn av_frame_free(frame: *mut *mut AVFrame);
}
unsafe extern "C" {
    pub fn av_frame_ref(dst: *mut AVFrame, src: *const AVFrame) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_frame_replace(dst: *mut AVFrame, src: *const AVFrame) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_frame_clone(src: *const AVFrame) -> *mut AVFrame;
}
unsafe extern "C" {
    pub fn av_frame_unref(frame: *mut AVFrame);
}
unsafe extern "C" {
    pub fn av_frame_move_ref(dst: *mut AVFrame, src: *mut AVFrame);
}
unsafe extern "C" {
    pub fn av_frame_get_buffer(
        frame: *mut AVFrame,
        align: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_frame_is_writable(frame: *mut AVFrame) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_frame_make_writable(frame: *mut AVFrame) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_frame_copy(dst: *mut AVFrame, src: *const AVFrame) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_frame_copy_props(dst: *mut AVFrame, src: *const AVFrame) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_frame_get_plane_buffer(
        frame: *const AVFrame,
        plane: ::std::os::raw::c_int,
    ) -> *mut AVBufferRef;
}
unsafe extern "C" {
    pub fn av_frame_new_side_data(
        frame: *mut AVFrame,
        type_: AVFrameSideDataType,
        size: usize,
    ) -> *mut AVFrameSideData;
}
unsafe extern "C" {
    pub fn av_frame_new_side_data_from_buf(
        frame: *mut AVFrame,
        type_: AVFrameSideDataType,
        buf: *mut AVBufferRef,
    ) -> *mut AVFrameSideData;
}
unsafe extern "C" {
    pub fn av_frame_get_side_data(
        frame: *const AVFrame,
        type_: AVFrameSideDataType,
    ) -> *mut AVFrameSideData;
}
unsafe extern "C" {
    pub fn av_frame_remove_side_data(frame: *mut AVFrame, type_: AVFrameSideDataType);
}
unsafe extern "C" {
    pub fn av_frame_apply_cropping(
        frame: *mut AVFrame,
        flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_frame_side_data_name(type_: AVFrameSideDataType) -> *const ::std::os::raw::c_char;
}
unsafe extern "C" {
    pub fn av_frame_side_data_desc(type_: AVFrameSideDataType) -> *const AVSideDataDescriptor;
}
unsafe extern "C" {
    pub fn av_frame_side_data_free(
        sd: *mut *mut *mut AVFrameSideData,
        nb_sd: *mut ::std::os::raw::c_int,
    );
}
unsafe extern "C" {
    pub fn av_frame_side_data_new(
        sd: *mut *mut *mut AVFrameSideData,
        nb_sd: *mut ::std::os::raw::c_int,
        type_: AVFrameSideDataType,
        size: usize,
        flags: ::std::os::raw::c_uint,
    ) -> *mut AVFrameSideData;
}
unsafe extern "C" {
    pub fn av_frame_side_data_add(
        sd: *mut *mut *mut AVFrameSideData,
        nb_sd: *mut ::std::os::raw::c_int,
        type_: AVFrameSideDataType,
        buf: *mut *mut AVBufferRef,
        flags: ::std::os::raw::c_uint,
    ) -> *mut AVFrameSideData;
}
unsafe extern "C" {
    pub fn av_frame_side_data_clone(
        sd: *mut *mut *mut AVFrameSideData,
        nb_sd: *mut ::std::os::raw::c_int,
        src: *const AVFrameSideData,
        flags: ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_frame_side_data_get_c(
        sd: *const *const AVFrameSideData,
        nb_sd: ::std::os::raw::c_int,
        type_: AVFrameSideDataType,
    ) -> *const AVFrameSideData;
}
unsafe extern "C" {
    pub fn av_frame_side_data_remove(
        sd: *mut *mut *mut AVFrameSideData,
        nb_sd: *mut ::std::os::raw::c_int,
        type_: AVFrameSideDataType,
    );
}
unsafe extern "C" {
    pub fn av_frame_side_data_remove_by_props(
        sd: *mut *mut *mut AVFrameSideData,
        nb_sd: *mut ::std::os::raw::c_int,
        props: ::std::os::raw::c_int,
    );
}
pub const AVHWDeviceType_AV_HWDEVICE_TYPE_NONE: AVHWDeviceType = 0;
pub const AVHWDeviceType_AV_HWDEVICE_TYPE_VDPAU: AVHWDeviceType = 1;
pub const AVHWDeviceType_AV_HWDEVICE_TYPE_CUDA: AVHWDeviceType = 2;
pub const AVHWDeviceType_AV_HWDEVICE_TYPE_VAAPI: AVHWDeviceType = 3;
pub const AVHWDeviceType_AV_HWDEVICE_TYPE_DXVA2: AVHWDeviceType = 4;
pub const AVHWDeviceType_AV_HWDEVICE_TYPE_QSV: AVHWDeviceType = 5;
pub const AVHWDeviceType_AV_HWDEVICE_TYPE_VIDEOTOOLBOX: AVHWDeviceType = 6;
pub const AVHWDeviceType_AV_HWDEVICE_TYPE_D3D11VA: AVHWDeviceType = 7;
pub const AVHWDeviceType_AV_HWDEVICE_TYPE_DRM: AVHWDeviceType = 8;
pub const AVHWDeviceType_AV_HWDEVICE_TYPE_OPENCL: AVHWDeviceType = 9;
pub const AVHWDeviceType_AV_HWDEVICE_TYPE_MEDIACODEC: AVHWDeviceType = 10;
pub const AVHWDeviceType_AV_HWDEVICE_TYPE_VULKAN: AVHWDeviceType = 11;
pub const AVHWDeviceType_AV_HWDEVICE_TYPE_D3D12VA: AVHWDeviceType = 12;
pub const AVHWDeviceType_AV_HWDEVICE_TYPE_AMF: AVHWDeviceType = 13;
pub const AVHWDeviceType_AV_HWDEVICE_TYPE_OHCODEC: AVHWDeviceType = 14;
pub type AVHWDeviceType = ::std::os::raw::c_int;
unsafe extern "C" {
    pub fn av_hwdevice_find_type_by_name(name: *const ::std::os::raw::c_char) -> AVHWDeviceType;
}
unsafe extern "C" {
    pub fn av_hwdevice_get_type_name(type_: AVHWDeviceType) -> *const ::std::os::raw::c_char;
}
unsafe extern "C" {
    pub fn av_hwdevice_iterate_types(prev: AVHWDeviceType) -> AVHWDeviceType;
}
unsafe extern "C" {
    pub fn av_hwdevice_ctx_alloc(type_: AVHWDeviceType) -> *mut AVBufferRef;
}
unsafe extern "C" {
    pub fn av_hwdevice_ctx_init(ref_: *mut AVBufferRef) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_hwdevice_ctx_create(
        device_ctx: *mut *mut AVBufferRef,
        type_: AVHWDeviceType,
        device: *const ::std::os::raw::c_char,
        opts: *mut AVDictionary,
        flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_hwdevice_ctx_create_derived(
        dst_ctx: *mut *mut AVBufferRef,
        type_: AVHWDeviceType,
        src_ctx: *mut AVBufferRef,
        flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_hwdevice_ctx_create_derived_opts(
        dst_ctx: *mut *mut AVBufferRef,
        type_: AVHWDeviceType,
        src_ctx: *mut AVBufferRef,
        options: *mut AVDictionary,
        flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_hwframe_ctx_alloc(device_ctx: *mut AVBufferRef) -> *mut AVBufferRef;
}
unsafe extern "C" {
    pub fn av_hwframe_ctx_init(ref_: *mut AVBufferRef) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_hwframe_get_buffer(
        hwframe_ctx: *mut AVBufferRef,
        frame: *mut AVFrame,
        flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_hwframe_transfer_data(
        dst: *mut AVFrame,
        src: *const AVFrame,
        flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
pub const AVHWFrameTransferDirection_AV_HWFRAME_TRANSFER_DIRECTION_FROM:
    AVHWFrameTransferDirection = 0;
pub const AVHWFrameTransferDirection_AV_HWFRAME_TRANSFER_DIRECTION_TO: AVHWFrameTransferDirection =
    1;
pub type AVHWFrameTransferDirection = ::std::os::raw::c_int;
unsafe extern "C" {
    pub fn av_hwframe_transfer_get_formats(
        hwframe_ctx: *mut AVBufferRef,
        dir: AVHWFrameTransferDirection,
        formats: *mut *mut AVPixelFormat,
        flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVHWFramesConstraints {
    pub valid_hw_formats: *mut AVPixelFormat,
    pub valid_sw_formats: *mut AVPixelFormat,
    pub min_width: ::std::os::raw::c_int,
    pub min_height: ::std::os::raw::c_int,
    pub max_width: ::std::os::raw::c_int,
    pub max_height: ::std::os::raw::c_int,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVHWFramesConstraints"][::std::mem::size_of::<AVHWFramesConstraints>() - 32usize];
    ["Alignment of AVHWFramesConstraints"]
        [::std::mem::align_of::<AVHWFramesConstraints>() - 8usize];
    ["Offset of field: AVHWFramesConstraints::valid_hw_formats"]
        [::std::mem::offset_of!(AVHWFramesConstraints, valid_hw_formats) - 0usize];
    ["Offset of field: AVHWFramesConstraints::valid_sw_formats"]
        [::std::mem::offset_of!(AVHWFramesConstraints, valid_sw_formats) - 8usize];
    ["Offset of field: AVHWFramesConstraints::min_width"]
        [::std::mem::offset_of!(AVHWFramesConstraints, min_width) - 16usize];
    ["Offset of field: AVHWFramesConstraints::min_height"]
        [::std::mem::offset_of!(AVHWFramesConstraints, min_height) - 20usize];
    ["Offset of field: AVHWFramesConstraints::max_width"]
        [::std::mem::offset_of!(AVHWFramesConstraints, max_width) - 24usize];
    ["Offset of field: AVHWFramesConstraints::max_height"]
        [::std::mem::offset_of!(AVHWFramesConstraints, max_height) - 28usize];
};
unsafe extern "C" {
    pub fn av_hwdevice_hwconfig_alloc(device_ctx: *mut AVBufferRef) -> *mut ::std::os::raw::c_void;
}
unsafe extern "C" {
    pub fn av_hwdevice_get_hwframe_constraints(
        ref_: *mut AVBufferRef,
        hwconfig: *const ::std::os::raw::c_void,
    ) -> *mut AVHWFramesConstraints;
}
unsafe extern "C" {
    pub fn av_hwframe_constraints_free(constraints: *mut *mut AVHWFramesConstraints);
}
unsafe extern "C" {
    pub fn av_hwframe_map(
        dst: *mut AVFrame,
        src: *const AVFrame,
        flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_hwframe_ctx_create_derived(
        derived_frame_ctx: *mut *mut AVBufferRef,
        format: AVPixelFormat,
        derived_device_ctx: *mut AVBufferRef,
        source_frame_ctx: *mut AVBufferRef,
        flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
pub const AVCodecID_AV_CODEC_ID_NONE: AVCodecID = 0;
pub const AVCodecID_AV_CODEC_ID_MPEG1VIDEO: AVCodecID = 1;
pub const AVCodecID_AV_CODEC_ID_MPEG2VIDEO: AVCodecID = 2;
pub const AVCodecID_AV_CODEC_ID_H261: AVCodecID = 3;
pub const AVCodecID_AV_CODEC_ID_H263: AVCodecID = 4;
pub const AVCodecID_AV_CODEC_ID_RV10: AVCodecID = 5;
pub const AVCodecID_AV_CODEC_ID_RV20: AVCodecID = 6;
pub const AVCodecID_AV_CODEC_ID_MJPEG: AVCodecID = 7;
pub const AVCodecID_AV_CODEC_ID_MJPEGB: AVCodecID = 8;
pub const AVCodecID_AV_CODEC_ID_LJPEG: AVCodecID = 9;
pub const AVCodecID_AV_CODEC_ID_SP5X: AVCodecID = 10;
pub const AVCodecID_AV_CODEC_ID_JPEGLS: AVCodecID = 11;
pub const AVCodecID_AV_CODEC_ID_MPEG4: AVCodecID = 12;
pub const AVCodecID_AV_CODEC_ID_RAWVIDEO: AVCodecID = 13;
pub const AVCodecID_AV_CODEC_ID_MSMPEG4V1: AVCodecID = 14;
pub const AVCodecID_AV_CODEC_ID_MSMPEG4V2: AVCodecID = 15;
pub const AVCodecID_AV_CODEC_ID_MSMPEG4V3: AVCodecID = 16;
pub const AVCodecID_AV_CODEC_ID_WMV1: AVCodecID = 17;
pub const AVCodecID_AV_CODEC_ID_WMV2: AVCodecID = 18;
pub const AVCodecID_AV_CODEC_ID_H263P: AVCodecID = 19;
pub const AVCodecID_AV_CODEC_ID_H263I: AVCodecID = 20;
pub const AVCodecID_AV_CODEC_ID_FLV1: AVCodecID = 21;
pub const AVCodecID_AV_CODEC_ID_SVQ1: AVCodecID = 22;
pub const AVCodecID_AV_CODEC_ID_SVQ3: AVCodecID = 23;
pub const AVCodecID_AV_CODEC_ID_DVVIDEO: AVCodecID = 24;
pub const AVCodecID_AV_CODEC_ID_HUFFYUV: AVCodecID = 25;
pub const AVCodecID_AV_CODEC_ID_CYUV: AVCodecID = 26;
pub const AVCodecID_AV_CODEC_ID_H264: AVCodecID = 27;
pub const AVCodecID_AV_CODEC_ID_INDEO3: AVCodecID = 28;
pub const AVCodecID_AV_CODEC_ID_VP3: AVCodecID = 29;
pub const AVCodecID_AV_CODEC_ID_THEORA: AVCodecID = 30;
pub const AVCodecID_AV_CODEC_ID_ASV1: AVCodecID = 31;
pub const AVCodecID_AV_CODEC_ID_ASV2: AVCodecID = 32;
pub const AVCodecID_AV_CODEC_ID_FFV1: AVCodecID = 33;
pub const AVCodecID_AV_CODEC_ID_4XM: AVCodecID = 34;
pub const AVCodecID_AV_CODEC_ID_VCR1: AVCodecID = 35;
pub const AVCodecID_AV_CODEC_ID_CLJR: AVCodecID = 36;
pub const AVCodecID_AV_CODEC_ID_MDEC: AVCodecID = 37;
pub const AVCodecID_AV_CODEC_ID_ROQ: AVCodecID = 38;
pub const AVCodecID_AV_CODEC_ID_INTERPLAY_VIDEO: AVCodecID = 39;
pub const AVCodecID_AV_CODEC_ID_XAN_WC3: AVCodecID = 40;
pub const AVCodecID_AV_CODEC_ID_XAN_WC4: AVCodecID = 41;
pub const AVCodecID_AV_CODEC_ID_RPZA: AVCodecID = 42;
pub const AVCodecID_AV_CODEC_ID_CINEPAK: AVCodecID = 43;
pub const AVCodecID_AV_CODEC_ID_WS_VQA: AVCodecID = 44;
pub const AVCodecID_AV_CODEC_ID_MSRLE: AVCodecID = 45;
pub const AVCodecID_AV_CODEC_ID_MSVIDEO1: AVCodecID = 46;
pub const AVCodecID_AV_CODEC_ID_IDCIN: AVCodecID = 47;
pub const AVCodecID_AV_CODEC_ID_8BPS: AVCodecID = 48;
pub const AVCodecID_AV_CODEC_ID_SMC: AVCodecID = 49;
pub const AVCodecID_AV_CODEC_ID_FLIC: AVCodecID = 50;
pub const AVCodecID_AV_CODEC_ID_TRUEMOTION1: AVCodecID = 51;
pub const AVCodecID_AV_CODEC_ID_VMDVIDEO: AVCodecID = 52;
pub const AVCodecID_AV_CODEC_ID_MSZH: AVCodecID = 53;
pub const AVCodecID_AV_CODEC_ID_ZLIB: AVCodecID = 54;
pub const AVCodecID_AV_CODEC_ID_QTRLE: AVCodecID = 55;
pub const AVCodecID_AV_CODEC_ID_TSCC: AVCodecID = 56;
pub const AVCodecID_AV_CODEC_ID_ULTI: AVCodecID = 57;
pub const AVCodecID_AV_CODEC_ID_QDRAW: AVCodecID = 58;
pub const AVCodecID_AV_CODEC_ID_VIXL: AVCodecID = 59;
pub const AVCodecID_AV_CODEC_ID_QPEG: AVCodecID = 60;
pub const AVCodecID_AV_CODEC_ID_PNG: AVCodecID = 61;
pub const AVCodecID_AV_CODEC_ID_PPM: AVCodecID = 62;
pub const AVCodecID_AV_CODEC_ID_PBM: AVCodecID = 63;
pub const AVCodecID_AV_CODEC_ID_PGM: AVCodecID = 64;
pub const AVCodecID_AV_CODEC_ID_PGMYUV: AVCodecID = 65;
pub const AVCodecID_AV_CODEC_ID_PAM: AVCodecID = 66;
pub const AVCodecID_AV_CODEC_ID_FFVHUFF: AVCodecID = 67;
pub const AVCodecID_AV_CODEC_ID_RV30: AVCodecID = 68;
pub const AVCodecID_AV_CODEC_ID_RV40: AVCodecID = 69;
pub const AVCodecID_AV_CODEC_ID_VC1: AVCodecID = 70;
pub const AVCodecID_AV_CODEC_ID_WMV3: AVCodecID = 71;
pub const AVCodecID_AV_CODEC_ID_LOCO: AVCodecID = 72;
pub const AVCodecID_AV_CODEC_ID_WNV1: AVCodecID = 73;
pub const AVCodecID_AV_CODEC_ID_AASC: AVCodecID = 74;
pub const AVCodecID_AV_CODEC_ID_INDEO2: AVCodecID = 75;
pub const AVCodecID_AV_CODEC_ID_FRAPS: AVCodecID = 76;
pub const AVCodecID_AV_CODEC_ID_TRUEMOTION2: AVCodecID = 77;
pub const AVCodecID_AV_CODEC_ID_BMP: AVCodecID = 78;
pub const AVCodecID_AV_CODEC_ID_CSCD: AVCodecID = 79;
pub const AVCodecID_AV_CODEC_ID_MMVIDEO: AVCodecID = 80;
pub const AVCodecID_AV_CODEC_ID_ZMBV: AVCodecID = 81;
pub const AVCodecID_AV_CODEC_ID_AVS: AVCodecID = 82;
pub const AVCodecID_AV_CODEC_ID_SMACKVIDEO: AVCodecID = 83;
pub const AVCodecID_AV_CODEC_ID_NUV: AVCodecID = 84;
pub const AVCodecID_AV_CODEC_ID_KMVC: AVCodecID = 85;
pub const AVCodecID_AV_CODEC_ID_FLASHSV: AVCodecID = 86;
pub const AVCodecID_AV_CODEC_ID_CAVS: AVCodecID = 87;
pub const AVCodecID_AV_CODEC_ID_JPEG2000: AVCodecID = 88;
pub const AVCodecID_AV_CODEC_ID_VMNC: AVCodecID = 89;
pub const AVCodecID_AV_CODEC_ID_VP5: AVCodecID = 90;
pub const AVCodecID_AV_CODEC_ID_VP6: AVCodecID = 91;
pub const AVCodecID_AV_CODEC_ID_VP6F: AVCodecID = 92;
pub const AVCodecID_AV_CODEC_ID_TARGA: AVCodecID = 93;
pub const AVCodecID_AV_CODEC_ID_DSICINVIDEO: AVCodecID = 94;
pub const AVCodecID_AV_CODEC_ID_TIERTEXSEQVIDEO: AVCodecID = 95;
pub const AVCodecID_AV_CODEC_ID_TIFF: AVCodecID = 96;
pub const AVCodecID_AV_CODEC_ID_GIF: AVCodecID = 97;
pub const AVCodecID_AV_CODEC_ID_DXA: AVCodecID = 98;
pub const AVCodecID_AV_CODEC_ID_DNXHD: AVCodecID = 99;
pub const AVCodecID_AV_CODEC_ID_THP: AVCodecID = 100;
pub const AVCodecID_AV_CODEC_ID_SGI: AVCodecID = 101;
pub const AVCodecID_AV_CODEC_ID_C93: AVCodecID = 102;
pub const AVCodecID_AV_CODEC_ID_BETHSOFTVID: AVCodecID = 103;
pub const AVCodecID_AV_CODEC_ID_PTX: AVCodecID = 104;
pub const AVCodecID_AV_CODEC_ID_TXD: AVCodecID = 105;
pub const AVCodecID_AV_CODEC_ID_VP6A: AVCodecID = 106;
pub const AVCodecID_AV_CODEC_ID_AMV: AVCodecID = 107;
pub const AVCodecID_AV_CODEC_ID_VB: AVCodecID = 108;
pub const AVCodecID_AV_CODEC_ID_PCX: AVCodecID = 109;
pub const AVCodecID_AV_CODEC_ID_SUNRAST: AVCodecID = 110;
pub const AVCodecID_AV_CODEC_ID_INDEO4: AVCodecID = 111;
pub const AVCodecID_AV_CODEC_ID_INDEO5: AVCodecID = 112;
pub const AVCodecID_AV_CODEC_ID_MIMIC: AVCodecID = 113;
pub const AVCodecID_AV_CODEC_ID_RL2: AVCodecID = 114;
pub const AVCodecID_AV_CODEC_ID_ESCAPE124: AVCodecID = 115;
pub const AVCodecID_AV_CODEC_ID_DIRAC: AVCodecID = 116;
pub const AVCodecID_AV_CODEC_ID_BFI: AVCodecID = 117;
pub const AVCodecID_AV_CODEC_ID_CMV: AVCodecID = 118;
pub const AVCodecID_AV_CODEC_ID_MOTIONPIXELS: AVCodecID = 119;
pub const AVCodecID_AV_CODEC_ID_TGV: AVCodecID = 120;
pub const AVCodecID_AV_CODEC_ID_TGQ: AVCodecID = 121;
pub const AVCodecID_AV_CODEC_ID_TQI: AVCodecID = 122;
pub const AVCodecID_AV_CODEC_ID_AURA: AVCodecID = 123;
pub const AVCodecID_AV_CODEC_ID_AURA2: AVCodecID = 124;
pub const AVCodecID_AV_CODEC_ID_V210X: AVCodecID = 125;
pub const AVCodecID_AV_CODEC_ID_TMV: AVCodecID = 126;
pub const AVCodecID_AV_CODEC_ID_V210: AVCodecID = 127;
pub const AVCodecID_AV_CODEC_ID_DPX: AVCodecID = 128;
pub const AVCodecID_AV_CODEC_ID_MAD: AVCodecID = 129;
pub const AVCodecID_AV_CODEC_ID_FRWU: AVCodecID = 130;
pub const AVCodecID_AV_CODEC_ID_FLASHSV2: AVCodecID = 131;
pub const AVCodecID_AV_CODEC_ID_CDGRAPHICS: AVCodecID = 132;
pub const AVCodecID_AV_CODEC_ID_R210: AVCodecID = 133;
pub const AVCodecID_AV_CODEC_ID_ANM: AVCodecID = 134;
pub const AVCodecID_AV_CODEC_ID_BINKVIDEO: AVCodecID = 135;
pub const AVCodecID_AV_CODEC_ID_IFF_ILBM: AVCodecID = 136;
pub const AVCodecID_AV_CODEC_ID_KGV1: AVCodecID = 137;
pub const AVCodecID_AV_CODEC_ID_YOP: AVCodecID = 138;
pub const AVCodecID_AV_CODEC_ID_VP8: AVCodecID = 139;
pub const AVCodecID_AV_CODEC_ID_PICTOR: AVCodecID = 140;
pub const AVCodecID_AV_CODEC_ID_ANSI: AVCodecID = 141;
pub const AVCodecID_AV_CODEC_ID_A64_MULTI: AVCodecID = 142;
pub const AVCodecID_AV_CODEC_ID_A64_MULTI5: AVCodecID = 143;
pub const AVCodecID_AV_CODEC_ID_R10K: AVCodecID = 144;
pub const AVCodecID_AV_CODEC_ID_MXPEG: AVCodecID = 145;
pub const AVCodecID_AV_CODEC_ID_LAGARITH: AVCodecID = 146;
pub const AVCodecID_AV_CODEC_ID_PRORES: AVCodecID = 147;
pub const AVCodecID_AV_CODEC_ID_JV: AVCodecID = 148;
pub const AVCodecID_AV_CODEC_ID_DFA: AVCodecID = 149;
pub const AVCodecID_AV_CODEC_ID_WMV3IMAGE: AVCodecID = 150;
pub const AVCodecID_AV_CODEC_ID_VC1IMAGE: AVCodecID = 151;
pub const AVCodecID_AV_CODEC_ID_UTVIDEO: AVCodecID = 152;
pub const AVCodecID_AV_CODEC_ID_BMV_VIDEO: AVCodecID = 153;
pub const AVCodecID_AV_CODEC_ID_VBLE: AVCodecID = 154;
pub const AVCodecID_AV_CODEC_ID_DXTORY: AVCodecID = 155;
pub const AVCodecID_AV_CODEC_ID_V410: AVCodecID = 156;
pub const AVCodecID_AV_CODEC_ID_XWD: AVCodecID = 157;
pub const AVCodecID_AV_CODEC_ID_CDXL: AVCodecID = 158;
pub const AVCodecID_AV_CODEC_ID_XBM: AVCodecID = 159;
pub const AVCodecID_AV_CODEC_ID_ZEROCODEC: AVCodecID = 160;
pub const AVCodecID_AV_CODEC_ID_MSS1: AVCodecID = 161;
pub const AVCodecID_AV_CODEC_ID_MSA1: AVCodecID = 162;
pub const AVCodecID_AV_CODEC_ID_TSCC2: AVCodecID = 163;
pub const AVCodecID_AV_CODEC_ID_MTS2: AVCodecID = 164;
pub const AVCodecID_AV_CODEC_ID_CLLC: AVCodecID = 165;
pub const AVCodecID_AV_CODEC_ID_MSS2: AVCodecID = 166;
pub const AVCodecID_AV_CODEC_ID_VP9: AVCodecID = 167;
pub const AVCodecID_AV_CODEC_ID_AIC: AVCodecID = 168;
pub const AVCodecID_AV_CODEC_ID_ESCAPE130: AVCodecID = 169;
pub const AVCodecID_AV_CODEC_ID_G2M: AVCodecID = 170;
pub const AVCodecID_AV_CODEC_ID_WEBP: AVCodecID = 171;
pub const AVCodecID_AV_CODEC_ID_HNM4_VIDEO: AVCodecID = 172;
pub const AVCodecID_AV_CODEC_ID_HEVC: AVCodecID = 173;
pub const AVCodecID_AV_CODEC_ID_FIC: AVCodecID = 174;
pub const AVCodecID_AV_CODEC_ID_ALIAS_PIX: AVCodecID = 175;
pub const AVCodecID_AV_CODEC_ID_BRENDER_PIX: AVCodecID = 176;
pub const AVCodecID_AV_CODEC_ID_PAF_VIDEO: AVCodecID = 177;
pub const AVCodecID_AV_CODEC_ID_EXR: AVCodecID = 178;
pub const AVCodecID_AV_CODEC_ID_VP7: AVCodecID = 179;
pub const AVCodecID_AV_CODEC_ID_SANM: AVCodecID = 180;
pub const AVCodecID_AV_CODEC_ID_SGIRLE: AVCodecID = 181;
pub const AVCodecID_AV_CODEC_ID_MVC1: AVCodecID = 182;
pub const AVCodecID_AV_CODEC_ID_MVC2: AVCodecID = 183;
pub const AVCodecID_AV_CODEC_ID_HQX: AVCodecID = 184;
pub const AVCodecID_AV_CODEC_ID_TDSC: AVCodecID = 185;
pub const AVCodecID_AV_CODEC_ID_HQ_HQA: AVCodecID = 186;
pub const AVCodecID_AV_CODEC_ID_HAP: AVCodecID = 187;
pub const AVCodecID_AV_CODEC_ID_DDS: AVCodecID = 188;
pub const AVCodecID_AV_CODEC_ID_DXV: AVCodecID = 189;
pub const AVCodecID_AV_CODEC_ID_SCREENPRESSO: AVCodecID = 190;
pub const AVCodecID_AV_CODEC_ID_RSCC: AVCodecID = 191;
pub const AVCodecID_AV_CODEC_ID_AVS2: AVCodecID = 192;
pub const AVCodecID_AV_CODEC_ID_PGX: AVCodecID = 193;
pub const AVCodecID_AV_CODEC_ID_AVS3: AVCodecID = 194;
pub const AVCodecID_AV_CODEC_ID_MSP2: AVCodecID = 195;
pub const AVCodecID_AV_CODEC_ID_VVC: AVCodecID = 196;
pub const AVCodecID_AV_CODEC_ID_Y41P: AVCodecID = 197;
pub const AVCodecID_AV_CODEC_ID_AVRP: AVCodecID = 198;
pub const AVCodecID_AV_CODEC_ID_012V: AVCodecID = 199;
pub const AVCodecID_AV_CODEC_ID_AVUI: AVCodecID = 200;
pub const AVCodecID_AV_CODEC_ID_TARGA_Y216: AVCodecID = 201;
pub const AVCodecID_AV_CODEC_ID_V308: AVCodecID = 202;
pub const AVCodecID_AV_CODEC_ID_V408: AVCodecID = 203;
pub const AVCodecID_AV_CODEC_ID_YUV4: AVCodecID = 204;
pub const AVCodecID_AV_CODEC_ID_AVRN: AVCodecID = 205;
pub const AVCodecID_AV_CODEC_ID_CPIA: AVCodecID = 206;
pub const AVCodecID_AV_CODEC_ID_XFACE: AVCodecID = 207;
pub const AVCodecID_AV_CODEC_ID_SNOW: AVCodecID = 208;
pub const AVCodecID_AV_CODEC_ID_SMVJPEG: AVCodecID = 209;
pub const AVCodecID_AV_CODEC_ID_APNG: AVCodecID = 210;
pub const AVCodecID_AV_CODEC_ID_DAALA: AVCodecID = 211;
pub const AVCodecID_AV_CODEC_ID_CFHD: AVCodecID = 212;
pub const AVCodecID_AV_CODEC_ID_TRUEMOTION2RT: AVCodecID = 213;
pub const AVCodecID_AV_CODEC_ID_M101: AVCodecID = 214;
pub const AVCodecID_AV_CODEC_ID_MAGICYUV: AVCodecID = 215;
pub const AVCodecID_AV_CODEC_ID_SHEERVIDEO: AVCodecID = 216;
pub const AVCodecID_AV_CODEC_ID_YLC: AVCodecID = 217;
pub const AVCodecID_AV_CODEC_ID_PSD: AVCodecID = 218;
pub const AVCodecID_AV_CODEC_ID_PIXLET: AVCodecID = 219;
pub const AVCodecID_AV_CODEC_ID_SPEEDHQ: AVCodecID = 220;
pub const AVCodecID_AV_CODEC_ID_FMVC: AVCodecID = 221;
pub const AVCodecID_AV_CODEC_ID_SCPR: AVCodecID = 222;
pub const AVCodecID_AV_CODEC_ID_CLEARVIDEO: AVCodecID = 223;
pub const AVCodecID_AV_CODEC_ID_XPM: AVCodecID = 224;
pub const AVCodecID_AV_CODEC_ID_AV1: AVCodecID = 225;
pub const AVCodecID_AV_CODEC_ID_BITPACKED: AVCodecID = 226;
pub const AVCodecID_AV_CODEC_ID_MSCC: AVCodecID = 227;
pub const AVCodecID_AV_CODEC_ID_SRGC: AVCodecID = 228;
pub const AVCodecID_AV_CODEC_ID_SVG: AVCodecID = 229;
pub const AVCodecID_AV_CODEC_ID_GDV: AVCodecID = 230;
pub const AVCodecID_AV_CODEC_ID_FITS: AVCodecID = 231;
pub const AVCodecID_AV_CODEC_ID_IMM4: AVCodecID = 232;
pub const AVCodecID_AV_CODEC_ID_PROSUMER: AVCodecID = 233;
pub const AVCodecID_AV_CODEC_ID_MWSC: AVCodecID = 234;
pub const AVCodecID_AV_CODEC_ID_WCMV: AVCodecID = 235;
pub const AVCodecID_AV_CODEC_ID_RASC: AVCodecID = 236;
pub const AVCodecID_AV_CODEC_ID_HYMT: AVCodecID = 237;
pub const AVCodecID_AV_CODEC_ID_ARBC: AVCodecID = 238;
pub const AVCodecID_AV_CODEC_ID_AGM: AVCodecID = 239;
pub const AVCodecID_AV_CODEC_ID_LSCR: AVCodecID = 240;
pub const AVCodecID_AV_CODEC_ID_VP4: AVCodecID = 241;
pub const AVCodecID_AV_CODEC_ID_IMM5: AVCodecID = 242;
pub const AVCodecID_AV_CODEC_ID_MVDV: AVCodecID = 243;
pub const AVCodecID_AV_CODEC_ID_MVHA: AVCodecID = 244;
pub const AVCodecID_AV_CODEC_ID_CDTOONS: AVCodecID = 245;
pub const AVCodecID_AV_CODEC_ID_MV30: AVCodecID = 246;
pub const AVCodecID_AV_CODEC_ID_NOTCHLC: AVCodecID = 247;
pub const AVCodecID_AV_CODEC_ID_PFM: AVCodecID = 248;
pub const AVCodecID_AV_CODEC_ID_MOBICLIP: AVCodecID = 249;
pub const AVCodecID_AV_CODEC_ID_PHOTOCD: AVCodecID = 250;
pub const AVCodecID_AV_CODEC_ID_IPU: AVCodecID = 251;
pub const AVCodecID_AV_CODEC_ID_ARGO: AVCodecID = 252;
pub const AVCodecID_AV_CODEC_ID_CRI: AVCodecID = 253;
pub const AVCodecID_AV_CODEC_ID_SIMBIOSIS_IMX: AVCodecID = 254;
pub const AVCodecID_AV_CODEC_ID_SGA_VIDEO: AVCodecID = 255;
pub const AVCodecID_AV_CODEC_ID_GEM: AVCodecID = 256;
pub const AVCodecID_AV_CODEC_ID_VBN: AVCodecID = 257;
pub const AVCodecID_AV_CODEC_ID_JPEGXL: AVCodecID = 258;
pub const AVCodecID_AV_CODEC_ID_QOI: AVCodecID = 259;
pub const AVCodecID_AV_CODEC_ID_PHM: AVCodecID = 260;
pub const AVCodecID_AV_CODEC_ID_RADIANCE_HDR: AVCodecID = 261;
pub const AVCodecID_AV_CODEC_ID_WBMP: AVCodecID = 262;
pub const AVCodecID_AV_CODEC_ID_MEDIA100: AVCodecID = 263;
pub const AVCodecID_AV_CODEC_ID_VQC: AVCodecID = 264;
pub const AVCodecID_AV_CODEC_ID_PDV: AVCodecID = 265;
pub const AVCodecID_AV_CODEC_ID_EVC: AVCodecID = 266;
pub const AVCodecID_AV_CODEC_ID_RTV1: AVCodecID = 267;
pub const AVCodecID_AV_CODEC_ID_VMIX: AVCodecID = 268;
pub const AVCodecID_AV_CODEC_ID_LEAD: AVCodecID = 269;
pub const AVCodecID_AV_CODEC_ID_DNXUC: AVCodecID = 270;
pub const AVCodecID_AV_CODEC_ID_RV60: AVCodecID = 271;
pub const AVCodecID_AV_CODEC_ID_JPEGXL_ANIM: AVCodecID = 272;
pub const AVCodecID_AV_CODEC_ID_APV: AVCodecID = 273;
pub const AVCodecID_AV_CODEC_ID_PRORES_RAW: AVCodecID = 274;
pub const AVCodecID_AV_CODEC_ID_JPEGXS: AVCodecID = 275;
pub const AVCodecID_AV_CODEC_ID_FIRST_AUDIO: AVCodecID = 65536;
pub const AVCodecID_AV_CODEC_ID_PCM_S16LE: AVCodecID = 65536;
pub const AVCodecID_AV_CODEC_ID_PCM_S16BE: AVCodecID = 65537;
pub const AVCodecID_AV_CODEC_ID_PCM_U16LE: AVCodecID = 65538;
pub const AVCodecID_AV_CODEC_ID_PCM_U16BE: AVCodecID = 65539;
pub const AVCodecID_AV_CODEC_ID_PCM_S8: AVCodecID = 65540;
pub const AVCodecID_AV_CODEC_ID_PCM_U8: AVCodecID = 65541;
pub const AVCodecID_AV_CODEC_ID_PCM_MULAW: AVCodecID = 65542;
pub const AVCodecID_AV_CODEC_ID_PCM_ALAW: AVCodecID = 65543;
pub const AVCodecID_AV_CODEC_ID_PCM_S32LE: AVCodecID = 65544;
pub const AVCodecID_AV_CODEC_ID_PCM_S32BE: AVCodecID = 65545;
pub const AVCodecID_AV_CODEC_ID_PCM_U32LE: AVCodecID = 65546;
pub const AVCodecID_AV_CODEC_ID_PCM_U32BE: AVCodecID = 65547;
pub const AVCodecID_AV_CODEC_ID_PCM_S24LE: AVCodecID = 65548;
pub const AVCodecID_AV_CODEC_ID_PCM_S24BE: AVCodecID = 65549;
pub const AVCodecID_AV_CODEC_ID_PCM_U24LE: AVCodecID = 65550;
pub const AVCodecID_AV_CODEC_ID_PCM_U24BE: AVCodecID = 65551;
pub const AVCodecID_AV_CODEC_ID_PCM_S24DAUD: AVCodecID = 65552;
pub const AVCodecID_AV_CODEC_ID_PCM_ZORK: AVCodecID = 65553;
pub const AVCodecID_AV_CODEC_ID_PCM_S16LE_PLANAR: AVCodecID = 65554;
pub const AVCodecID_AV_CODEC_ID_PCM_DVD: AVCodecID = 65555;
pub const AVCodecID_AV_CODEC_ID_PCM_F32BE: AVCodecID = 65556;
pub const AVCodecID_AV_CODEC_ID_PCM_F32LE: AVCodecID = 65557;
pub const AVCodecID_AV_CODEC_ID_PCM_F64BE: AVCodecID = 65558;
pub const AVCodecID_AV_CODEC_ID_PCM_F64LE: AVCodecID = 65559;
pub const AVCodecID_AV_CODEC_ID_PCM_BLURAY: AVCodecID = 65560;
pub const AVCodecID_AV_CODEC_ID_PCM_LXF: AVCodecID = 65561;
pub const AVCodecID_AV_CODEC_ID_S302M: AVCodecID = 65562;
pub const AVCodecID_AV_CODEC_ID_PCM_S8_PLANAR: AVCodecID = 65563;
pub const AVCodecID_AV_CODEC_ID_PCM_S24LE_PLANAR: AVCodecID = 65564;
pub const AVCodecID_AV_CODEC_ID_PCM_S32LE_PLANAR: AVCodecID = 65565;
pub const AVCodecID_AV_CODEC_ID_PCM_S16BE_PLANAR: AVCodecID = 65566;
pub const AVCodecID_AV_CODEC_ID_PCM_S64LE: AVCodecID = 65567;
pub const AVCodecID_AV_CODEC_ID_PCM_S64BE: AVCodecID = 65568;
pub const AVCodecID_AV_CODEC_ID_PCM_F16LE: AVCodecID = 65569;
pub const AVCodecID_AV_CODEC_ID_PCM_F24LE: AVCodecID = 65570;
pub const AVCodecID_AV_CODEC_ID_PCM_VIDC: AVCodecID = 65571;
pub const AVCodecID_AV_CODEC_ID_PCM_SGA: AVCodecID = 65572;
pub const AVCodecID_AV_CODEC_ID_ADPCM_IMA_QT: AVCodecID = 69632;
pub const AVCodecID_AV_CODEC_ID_ADPCM_IMA_WAV: AVCodecID = 69633;
pub const AVCodecID_AV_CODEC_ID_ADPCM_IMA_DK3: AVCodecID = 69634;
pub const AVCodecID_AV_CODEC_ID_ADPCM_IMA_DK4: AVCodecID = 69635;
pub const AVCodecID_AV_CODEC_ID_ADPCM_IMA_WS: AVCodecID = 69636;
pub const AVCodecID_AV_CODEC_ID_ADPCM_IMA_SMJPEG: AVCodecID = 69637;
pub const AVCodecID_AV_CODEC_ID_ADPCM_MS: AVCodecID = 69638;
pub const AVCodecID_AV_CODEC_ID_ADPCM_4XM: AVCodecID = 69639;
pub const AVCodecID_AV_CODEC_ID_ADPCM_XA: AVCodecID = 69640;
pub const AVCodecID_AV_CODEC_ID_ADPCM_ADX: AVCodecID = 69641;
pub const AVCodecID_AV_CODEC_ID_ADPCM_EA: AVCodecID = 69642;
pub const AVCodecID_AV_CODEC_ID_ADPCM_G726: AVCodecID = 69643;
pub const AVCodecID_AV_CODEC_ID_ADPCM_CT: AVCodecID = 69644;
pub const AVCodecID_AV_CODEC_ID_ADPCM_SWF: AVCodecID = 69645;
pub const AVCodecID_AV_CODEC_ID_ADPCM_YAMAHA: AVCodecID = 69646;
pub const AVCodecID_AV_CODEC_ID_ADPCM_SBPRO_4: AVCodecID = 69647;
pub const AVCodecID_AV_CODEC_ID_ADPCM_SBPRO_3: AVCodecID = 69648;
pub const AVCodecID_AV_CODEC_ID_ADPCM_SBPRO_2: AVCodecID = 69649;
pub const AVCodecID_AV_CODEC_ID_ADPCM_THP: AVCodecID = 69650;
pub const AVCodecID_AV_CODEC_ID_ADPCM_IMA_AMV: AVCodecID = 69651;
pub const AVCodecID_AV_CODEC_ID_ADPCM_EA_R1: AVCodecID = 69652;
pub const AVCodecID_AV_CODEC_ID_ADPCM_EA_R3: AVCodecID = 69653;
pub const AVCodecID_AV_CODEC_ID_ADPCM_EA_R2: AVCodecID = 69654;
pub const AVCodecID_AV_CODEC_ID_ADPCM_IMA_EA_SEAD: AVCodecID = 69655;
pub const AVCodecID_AV_CODEC_ID_ADPCM_IMA_EA_EACS: AVCodecID = 69656;
pub const AVCodecID_AV_CODEC_ID_ADPCM_EA_XAS: AVCodecID = 69657;
pub const AVCodecID_AV_CODEC_ID_ADPCM_EA_MAXIS_XA: AVCodecID = 69658;
pub const AVCodecID_AV_CODEC_ID_ADPCM_IMA_ISS: AVCodecID = 69659;
pub const AVCodecID_AV_CODEC_ID_ADPCM_G722: AVCodecID = 69660;
pub const AVCodecID_AV_CODEC_ID_ADPCM_IMA_APC: AVCodecID = 69661;
pub const AVCodecID_AV_CODEC_ID_ADPCM_VIMA: AVCodecID = 69662;
pub const AVCodecID_AV_CODEC_ID_ADPCM_AFC: AVCodecID = 69663;
pub const AVCodecID_AV_CODEC_ID_ADPCM_IMA_OKI: AVCodecID = 69664;
pub const AVCodecID_AV_CODEC_ID_ADPCM_DTK: AVCodecID = 69665;
pub const AVCodecID_AV_CODEC_ID_ADPCM_IMA_RAD: AVCodecID = 69666;
pub const AVCodecID_AV_CODEC_ID_ADPCM_G726LE: AVCodecID = 69667;
pub const AVCodecID_AV_CODEC_ID_ADPCM_THP_LE: AVCodecID = 69668;
pub const AVCodecID_AV_CODEC_ID_ADPCM_PSX: AVCodecID = 69669;
pub const AVCodecID_AV_CODEC_ID_ADPCM_AICA: AVCodecID = 69670;
pub const AVCodecID_AV_CODEC_ID_ADPCM_IMA_DAT4: AVCodecID = 69671;
pub const AVCodecID_AV_CODEC_ID_ADPCM_MTAF: AVCodecID = 69672;
pub const AVCodecID_AV_CODEC_ID_ADPCM_AGM: AVCodecID = 69673;
pub const AVCodecID_AV_CODEC_ID_ADPCM_ARGO: AVCodecID = 69674;
pub const AVCodecID_AV_CODEC_ID_ADPCM_IMA_SSI: AVCodecID = 69675;
pub const AVCodecID_AV_CODEC_ID_ADPCM_ZORK: AVCodecID = 69676;
pub const AVCodecID_AV_CODEC_ID_ADPCM_IMA_APM: AVCodecID = 69677;
pub const AVCodecID_AV_CODEC_ID_ADPCM_IMA_ALP: AVCodecID = 69678;
pub const AVCodecID_AV_CODEC_ID_ADPCM_IMA_MTF: AVCodecID = 69679;
pub const AVCodecID_AV_CODEC_ID_ADPCM_IMA_CUNNING: AVCodecID = 69680;
pub const AVCodecID_AV_CODEC_ID_ADPCM_IMA_MOFLEX: AVCodecID = 69681;
pub const AVCodecID_AV_CODEC_ID_ADPCM_IMA_ACORN: AVCodecID = 69682;
pub const AVCodecID_AV_CODEC_ID_ADPCM_XMD: AVCodecID = 69683;
pub const AVCodecID_AV_CODEC_ID_ADPCM_IMA_XBOX: AVCodecID = 69684;
pub const AVCodecID_AV_CODEC_ID_ADPCM_SANYO: AVCodecID = 69685;
pub const AVCodecID_AV_CODEC_ID_ADPCM_IMA_HVQM4: AVCodecID = 69686;
pub const AVCodecID_AV_CODEC_ID_ADPCM_IMA_PDA: AVCodecID = 69687;
pub const AVCodecID_AV_CODEC_ID_ADPCM_N64: AVCodecID = 69688;
pub const AVCodecID_AV_CODEC_ID_ADPCM_IMA_HVQM2: AVCodecID = 69689;
pub const AVCodecID_AV_CODEC_ID_ADPCM_IMA_MAGIX: AVCodecID = 69690;
pub const AVCodecID_AV_CODEC_ID_ADPCM_PSXC: AVCodecID = 69691;
pub const AVCodecID_AV_CODEC_ID_ADPCM_CIRCUS: AVCodecID = 69692;
pub const AVCodecID_AV_CODEC_ID_ADPCM_IMA_ESCAPE: AVCodecID = 69693;
pub const AVCodecID_AV_CODEC_ID_AMR_NB: AVCodecID = 73728;
pub const AVCodecID_AV_CODEC_ID_AMR_WB: AVCodecID = 73729;
pub const AVCodecID_AV_CODEC_ID_RA_144: AVCodecID = 77824;
pub const AVCodecID_AV_CODEC_ID_RA_288: AVCodecID = 77825;
pub const AVCodecID_AV_CODEC_ID_ROQ_DPCM: AVCodecID = 81920;
pub const AVCodecID_AV_CODEC_ID_INTERPLAY_DPCM: AVCodecID = 81921;
pub const AVCodecID_AV_CODEC_ID_XAN_DPCM: AVCodecID = 81922;
pub const AVCodecID_AV_CODEC_ID_SOL_DPCM: AVCodecID = 81923;
pub const AVCodecID_AV_CODEC_ID_SDX2_DPCM: AVCodecID = 81924;
pub const AVCodecID_AV_CODEC_ID_GREMLIN_DPCM: AVCodecID = 81925;
pub const AVCodecID_AV_CODEC_ID_DERF_DPCM: AVCodecID = 81926;
pub const AVCodecID_AV_CODEC_ID_WADY_DPCM: AVCodecID = 81927;
pub const AVCodecID_AV_CODEC_ID_CBD2_DPCM: AVCodecID = 81928;
pub const AVCodecID_AV_CODEC_ID_MP2: AVCodecID = 86016;
pub const AVCodecID_AV_CODEC_ID_MP3: AVCodecID = 86017;
pub const AVCodecID_AV_CODEC_ID_AAC: AVCodecID = 86018;
pub const AVCodecID_AV_CODEC_ID_AC3: AVCodecID = 86019;
pub const AVCodecID_AV_CODEC_ID_DTS: AVCodecID = 86020;
pub const AVCodecID_AV_CODEC_ID_VORBIS: AVCodecID = 86021;
pub const AVCodecID_AV_CODEC_ID_DVAUDIO: AVCodecID = 86022;
pub const AVCodecID_AV_CODEC_ID_WMAV1: AVCodecID = 86023;
pub const AVCodecID_AV_CODEC_ID_WMAV2: AVCodecID = 86024;
pub const AVCodecID_AV_CODEC_ID_MACE3: AVCodecID = 86025;
pub const AVCodecID_AV_CODEC_ID_MACE6: AVCodecID = 86026;
pub const AVCodecID_AV_CODEC_ID_VMDAUDIO: AVCodecID = 86027;
pub const AVCodecID_AV_CODEC_ID_FLAC: AVCodecID = 86028;
pub const AVCodecID_AV_CODEC_ID_MP3ADU: AVCodecID = 86029;
pub const AVCodecID_AV_CODEC_ID_MP3ON4: AVCodecID = 86030;
pub const AVCodecID_AV_CODEC_ID_SHORTEN: AVCodecID = 86031;
pub const AVCodecID_AV_CODEC_ID_ALAC: AVCodecID = 86032;
pub const AVCodecID_AV_CODEC_ID_WESTWOOD_SND1: AVCodecID = 86033;
pub const AVCodecID_AV_CODEC_ID_GSM: AVCodecID = 86034;
pub const AVCodecID_AV_CODEC_ID_QDM2: AVCodecID = 86035;
pub const AVCodecID_AV_CODEC_ID_COOK: AVCodecID = 86036;
pub const AVCodecID_AV_CODEC_ID_TRUESPEECH: AVCodecID = 86037;
pub const AVCodecID_AV_CODEC_ID_TTA: AVCodecID = 86038;
pub const AVCodecID_AV_CODEC_ID_SMACKAUDIO: AVCodecID = 86039;
pub const AVCodecID_AV_CODEC_ID_QCELP: AVCodecID = 86040;
pub const AVCodecID_AV_CODEC_ID_WAVPACK: AVCodecID = 86041;
pub const AVCodecID_AV_CODEC_ID_DSICINAUDIO: AVCodecID = 86042;
pub const AVCodecID_AV_CODEC_ID_IMC: AVCodecID = 86043;
pub const AVCodecID_AV_CODEC_ID_MUSEPACK7: AVCodecID = 86044;
pub const AVCodecID_AV_CODEC_ID_MLP: AVCodecID = 86045;
pub const AVCodecID_AV_CODEC_ID_GSM_MS: AVCodecID = 86046;
pub const AVCodecID_AV_CODEC_ID_ATRAC3: AVCodecID = 86047;
pub const AVCodecID_AV_CODEC_ID_APE: AVCodecID = 86048;
pub const AVCodecID_AV_CODEC_ID_NELLYMOSER: AVCodecID = 86049;
pub const AVCodecID_AV_CODEC_ID_MUSEPACK8: AVCodecID = 86050;
pub const AVCodecID_AV_CODEC_ID_SPEEX: AVCodecID = 86051;
pub const AVCodecID_AV_CODEC_ID_WMAVOICE: AVCodecID = 86052;
pub const AVCodecID_AV_CODEC_ID_WMAPRO: AVCodecID = 86053;
pub const AVCodecID_AV_CODEC_ID_WMALOSSLESS: AVCodecID = 86054;
pub const AVCodecID_AV_CODEC_ID_ATRAC3P: AVCodecID = 86055;
pub const AVCodecID_AV_CODEC_ID_EAC3: AVCodecID = 86056;
pub const AVCodecID_AV_CODEC_ID_SIPR: AVCodecID = 86057;
pub const AVCodecID_AV_CODEC_ID_MP1: AVCodecID = 86058;
pub const AVCodecID_AV_CODEC_ID_TWINVQ: AVCodecID = 86059;
pub const AVCodecID_AV_CODEC_ID_TRUEHD: AVCodecID = 86060;
pub const AVCodecID_AV_CODEC_ID_MP4ALS: AVCodecID = 86061;
pub const AVCodecID_AV_CODEC_ID_ATRAC1: AVCodecID = 86062;
pub const AVCodecID_AV_CODEC_ID_BINKAUDIO_RDFT: AVCodecID = 86063;
pub const AVCodecID_AV_CODEC_ID_BINKAUDIO_DCT: AVCodecID = 86064;
pub const AVCodecID_AV_CODEC_ID_AAC_LATM: AVCodecID = 86065;
pub const AVCodecID_AV_CODEC_ID_QDMC: AVCodecID = 86066;
pub const AVCodecID_AV_CODEC_ID_CELT: AVCodecID = 86067;
pub const AVCodecID_AV_CODEC_ID_G723_1: AVCodecID = 86068;
pub const AVCodecID_AV_CODEC_ID_G729: AVCodecID = 86069;
pub const AVCodecID_AV_CODEC_ID_8SVX_EXP: AVCodecID = 86070;
pub const AVCodecID_AV_CODEC_ID_8SVX_FIB: AVCodecID = 86071;
pub const AVCodecID_AV_CODEC_ID_BMV_AUDIO: AVCodecID = 86072;
pub const AVCodecID_AV_CODEC_ID_RALF: AVCodecID = 86073;
pub const AVCodecID_AV_CODEC_ID_IAC: AVCodecID = 86074;
pub const AVCodecID_AV_CODEC_ID_ILBC: AVCodecID = 86075;
pub const AVCodecID_AV_CODEC_ID_OPUS: AVCodecID = 86076;
pub const AVCodecID_AV_CODEC_ID_COMFORT_NOISE: AVCodecID = 86077;
pub const AVCodecID_AV_CODEC_ID_TAK: AVCodecID = 86078;
pub const AVCodecID_AV_CODEC_ID_METASOUND: AVCodecID = 86079;
pub const AVCodecID_AV_CODEC_ID_PAF_AUDIO: AVCodecID = 86080;
pub const AVCodecID_AV_CODEC_ID_ON2AVC: AVCodecID = 86081;
pub const AVCodecID_AV_CODEC_ID_DSS_SP: AVCodecID = 86082;
pub const AVCodecID_AV_CODEC_ID_CODEC2: AVCodecID = 86083;
pub const AVCodecID_AV_CODEC_ID_FFWAVESYNTH: AVCodecID = 86084;
pub const AVCodecID_AV_CODEC_ID_SONIC: AVCodecID = 86085;
pub const AVCodecID_AV_CODEC_ID_SONIC_LS: AVCodecID = 86086;
pub const AVCodecID_AV_CODEC_ID_EVRC: AVCodecID = 86087;
pub const AVCodecID_AV_CODEC_ID_SMV: AVCodecID = 86088;
pub const AVCodecID_AV_CODEC_ID_DSD_LSBF: AVCodecID = 86089;
pub const AVCodecID_AV_CODEC_ID_DSD_MSBF: AVCodecID = 86090;
pub const AVCodecID_AV_CODEC_ID_DSD_LSBF_PLANAR: AVCodecID = 86091;
pub const AVCodecID_AV_CODEC_ID_DSD_MSBF_PLANAR: AVCodecID = 86092;
pub const AVCodecID_AV_CODEC_ID_4GV: AVCodecID = 86093;
pub const AVCodecID_AV_CODEC_ID_INTERPLAY_ACM: AVCodecID = 86094;
pub const AVCodecID_AV_CODEC_ID_XMA1: AVCodecID = 86095;
pub const AVCodecID_AV_CODEC_ID_XMA2: AVCodecID = 86096;
pub const AVCodecID_AV_CODEC_ID_DST: AVCodecID = 86097;
pub const AVCodecID_AV_CODEC_ID_ATRAC3AL: AVCodecID = 86098;
pub const AVCodecID_AV_CODEC_ID_ATRAC3PAL: AVCodecID = 86099;
pub const AVCodecID_AV_CODEC_ID_DOLBY_E: AVCodecID = 86100;
pub const AVCodecID_AV_CODEC_ID_APTX: AVCodecID = 86101;
pub const AVCodecID_AV_CODEC_ID_APTX_HD: AVCodecID = 86102;
pub const AVCodecID_AV_CODEC_ID_SBC: AVCodecID = 86103;
pub const AVCodecID_AV_CODEC_ID_ATRAC9: AVCodecID = 86104;
pub const AVCodecID_AV_CODEC_ID_HCOM: AVCodecID = 86105;
pub const AVCodecID_AV_CODEC_ID_ACELP_KELVIN: AVCodecID = 86106;
pub const AVCodecID_AV_CODEC_ID_MPEGH_3D_AUDIO: AVCodecID = 86107;
pub const AVCodecID_AV_CODEC_ID_SIREN: AVCodecID = 86108;
pub const AVCodecID_AV_CODEC_ID_HCA: AVCodecID = 86109;
pub const AVCodecID_AV_CODEC_ID_FASTAUDIO: AVCodecID = 86110;
pub const AVCodecID_AV_CODEC_ID_MSNSIREN: AVCodecID = 86111;
pub const AVCodecID_AV_CODEC_ID_DFPWM: AVCodecID = 86112;
pub const AVCodecID_AV_CODEC_ID_BONK: AVCodecID = 86113;
pub const AVCodecID_AV_CODEC_ID_MISC4: AVCodecID = 86114;
pub const AVCodecID_AV_CODEC_ID_APAC: AVCodecID = 86115;
pub const AVCodecID_AV_CODEC_ID_FTR: AVCodecID = 86116;
pub const AVCodecID_AV_CODEC_ID_WAVARC: AVCodecID = 86117;
pub const AVCodecID_AV_CODEC_ID_RKA: AVCodecID = 86118;
pub const AVCodecID_AV_CODEC_ID_AC4: AVCodecID = 86119;
pub const AVCodecID_AV_CODEC_ID_OSQ: AVCodecID = 86120;
pub const AVCodecID_AV_CODEC_ID_QOA: AVCodecID = 86121;
pub const AVCodecID_AV_CODEC_ID_LC3: AVCodecID = 86122;
pub const AVCodecID_AV_CODEC_ID_G728: AVCodecID = 86123;
pub const AVCodecID_AV_CODEC_ID_AHX: AVCodecID = 86124;
pub const AVCodecID_AV_CODEC_ID_FIRST_SUBTITLE: AVCodecID = 94208;
pub const AVCodecID_AV_CODEC_ID_DVD_SUBTITLE: AVCodecID = 94208;
pub const AVCodecID_AV_CODEC_ID_DVB_SUBTITLE: AVCodecID = 94209;
pub const AVCodecID_AV_CODEC_ID_TEXT: AVCodecID = 94210;
pub const AVCodecID_AV_CODEC_ID_XSUB: AVCodecID = 94211;
pub const AVCodecID_AV_CODEC_ID_SSA: AVCodecID = 94212;
pub const AVCodecID_AV_CODEC_ID_MOV_TEXT: AVCodecID = 94213;
pub const AVCodecID_AV_CODEC_ID_HDMV_PGS_SUBTITLE: AVCodecID = 94214;
pub const AVCodecID_AV_CODEC_ID_DVB_TELETEXT: AVCodecID = 94215;
pub const AVCodecID_AV_CODEC_ID_SRT: AVCodecID = 94216;
pub const AVCodecID_AV_CODEC_ID_MICRODVD: AVCodecID = 94217;
pub const AVCodecID_AV_CODEC_ID_EIA_608: AVCodecID = 94218;
pub const AVCodecID_AV_CODEC_ID_JACOSUB: AVCodecID = 94219;
pub const AVCodecID_AV_CODEC_ID_SAMI: AVCodecID = 94220;
pub const AVCodecID_AV_CODEC_ID_REALTEXT: AVCodecID = 94221;
pub const AVCodecID_AV_CODEC_ID_STL: AVCodecID = 94222;
pub const AVCodecID_AV_CODEC_ID_SUBVIEWER1: AVCodecID = 94223;
pub const AVCodecID_AV_CODEC_ID_SUBVIEWER: AVCodecID = 94224;
pub const AVCodecID_AV_CODEC_ID_SUBRIP: AVCodecID = 94225;
pub const AVCodecID_AV_CODEC_ID_WEBVTT: AVCodecID = 94226;
pub const AVCodecID_AV_CODEC_ID_MPL2: AVCodecID = 94227;
pub const AVCodecID_AV_CODEC_ID_VPLAYER: AVCodecID = 94228;
pub const AVCodecID_AV_CODEC_ID_PJS: AVCodecID = 94229;
pub const AVCodecID_AV_CODEC_ID_ASS: AVCodecID = 94230;
pub const AVCodecID_AV_CODEC_ID_HDMV_TEXT_SUBTITLE: AVCodecID = 94231;
pub const AVCodecID_AV_CODEC_ID_TTML: AVCodecID = 94232;
pub const AVCodecID_AV_CODEC_ID_ARIB_CAPTION: AVCodecID = 94233;
pub const AVCodecID_AV_CODEC_ID_IVTV_VBI: AVCodecID = 94234;
pub const AVCodecID_AV_CODEC_ID_FIRST_UNKNOWN: AVCodecID = 98304;
pub const AVCodecID_AV_CODEC_ID_TTF: AVCodecID = 98304;
pub const AVCodecID_AV_CODEC_ID_SCTE_35: AVCodecID = 98305;
pub const AVCodecID_AV_CODEC_ID_EPG: AVCodecID = 98306;
pub const AVCodecID_AV_CODEC_ID_BINTEXT: AVCodecID = 98307;
pub const AVCodecID_AV_CODEC_ID_XBIN: AVCodecID = 98308;
pub const AVCodecID_AV_CODEC_ID_IDF: AVCodecID = 98309;
pub const AVCodecID_AV_CODEC_ID_OTF: AVCodecID = 98310;
pub const AVCodecID_AV_CODEC_ID_SMPTE_KLV: AVCodecID = 98311;
pub const AVCodecID_AV_CODEC_ID_DVD_NAV: AVCodecID = 98312;
pub const AVCodecID_AV_CODEC_ID_TIMED_ID3: AVCodecID = 98313;
pub const AVCodecID_AV_CODEC_ID_BIN_DATA: AVCodecID = 98314;
pub const AVCodecID_AV_CODEC_ID_SMPTE_2038: AVCodecID = 98315;
pub const AVCodecID_AV_CODEC_ID_LCEVC: AVCodecID = 98316;
pub const AVCodecID_AV_CODEC_ID_SMPTE_436M_ANC: AVCodecID = 98317;
pub const AVCodecID_AV_CODEC_ID_PROBE: AVCodecID = 102400;
pub const AVCodecID_AV_CODEC_ID_MPEG2TS: AVCodecID = 131072;
pub const AVCodecID_AV_CODEC_ID_MPEG4SYSTEMS: AVCodecID = 131073;
pub const AVCodecID_AV_CODEC_ID_FFMETADATA: AVCodecID = 135168;
pub const AVCodecID_AV_CODEC_ID_WRAPPED_AVFRAME: AVCodecID = 135169;
pub const AVCodecID_AV_CODEC_ID_VNULL: AVCodecID = 135170;
pub const AVCodecID_AV_CODEC_ID_ANULL: AVCodecID = 135171;
pub type AVCodecID = ::std::os::raw::c_int;
unsafe extern "C" {
    pub fn av_get_bits_per_sample(codec_id: AVCodecID) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_get_exact_bits_per_sample(codec_id: AVCodecID) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_get_pcm_codec(fmt: AVSampleFormat, be: ::std::os::raw::c_int) -> AVCodecID;
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVProfile {
    pub profile: ::std::os::raw::c_int,
    pub name: *const ::std::os::raw::c_char,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVProfile"][::std::mem::size_of::<AVProfile>() - 16usize];
    ["Alignment of AVProfile"][::std::mem::align_of::<AVProfile>() - 8usize];
    ["Offset of field: AVProfile::profile"][::std::mem::offset_of!(AVProfile, profile) - 0usize];
    ["Offset of field: AVProfile::name"][::std::mem::offset_of!(AVProfile, name) - 8usize];
};
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVCodec {
    pub name: *const ::std::os::raw::c_char,
    pub long_name: *const ::std::os::raw::c_char,
    pub type_: AVMediaType,
    pub id: AVCodecID,
    pub capabilities: ::std::os::raw::c_int,
    pub max_lowres: u8,
    pub supported_framerates: *const AVRational,
    pub pix_fmts: *const AVPixelFormat,
    pub supported_samplerates: *const ::std::os::raw::c_int,
    pub sample_fmts: *const AVSampleFormat,
    pub priv_class: *const AVClass,
    pub profiles: *const AVProfile,
    pub wrapper_name: *const ::std::os::raw::c_char,
    pub ch_layouts: *const AVChannelLayout,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVCodec"][::std::mem::size_of::<AVCodec>() - 96usize];
    ["Alignment of AVCodec"][::std::mem::align_of::<AVCodec>() - 8usize];
    ["Offset of field: AVCodec::name"][::std::mem::offset_of!(AVCodec, name) - 0usize];
    ["Offset of field: AVCodec::long_name"][::std::mem::offset_of!(AVCodec, long_name) - 8usize];
    ["Offset of field: AVCodec::type_"][::std::mem::offset_of!(AVCodec, type_) - 16usize];
    ["Offset of field: AVCodec::id"][::std::mem::offset_of!(AVCodec, id) - 20usize];
    ["Offset of field: AVCodec::capabilities"]
        [::std::mem::offset_of!(AVCodec, capabilities) - 24usize];
    ["Offset of field: AVCodec::max_lowres"][::std::mem::offset_of!(AVCodec, max_lowres) - 28usize];
    ["Offset of field: AVCodec::supported_framerates"]
        [::std::mem::offset_of!(AVCodec, supported_framerates) - 32usize];
    ["Offset of field: AVCodec::pix_fmts"][::std::mem::offset_of!(AVCodec, pix_fmts) - 40usize];
    ["Offset of field: AVCodec::supported_samplerates"]
        [::std::mem::offset_of!(AVCodec, supported_samplerates) - 48usize];
    ["Offset of field: AVCodec::sample_fmts"]
        [::std::mem::offset_of!(AVCodec, sample_fmts) - 56usize];
    ["Offset of field: AVCodec::priv_class"][::std::mem::offset_of!(AVCodec, priv_class) - 64usize];
    ["Offset of field: AVCodec::profiles"][::std::mem::offset_of!(AVCodec, profiles) - 72usize];
    ["Offset of field: AVCodec::wrapper_name"]
        [::std::mem::offset_of!(AVCodec, wrapper_name) - 80usize];
    ["Offset of field: AVCodec::ch_layouts"][::std::mem::offset_of!(AVCodec, ch_layouts) - 88usize];
};
unsafe extern "C" {
    pub fn av_codec_iterate(opaque: *mut *mut ::std::os::raw::c_void) -> *const AVCodec;
}
unsafe extern "C" {
    pub fn av_codec_is_encoder(codec: *const AVCodec) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_codec_is_decoder(codec: *const AVCodec) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_get_profile_name(
        codec: *const AVCodec,
        profile: ::std::os::raw::c_int,
    ) -> *const ::std::os::raw::c_char;
}
pub const AVFieldOrder_AV_FIELD_UNKNOWN: AVFieldOrder = 0;
pub const AVFieldOrder_AV_FIELD_PROGRESSIVE: AVFieldOrder = 1;
pub const AVFieldOrder_AV_FIELD_TT: AVFieldOrder = 2;
pub const AVFieldOrder_AV_FIELD_BB: AVFieldOrder = 3;
pub const AVFieldOrder_AV_FIELD_TB: AVFieldOrder = 4;
pub const AVFieldOrder_AV_FIELD_BT: AVFieldOrder = 5;
pub type AVFieldOrder = ::std::os::raw::c_int;
pub const AVDiscard_AVDISCARD_NONE: AVDiscard = -16;
pub const AVDiscard_AVDISCARD_DEFAULT: AVDiscard = 0;
pub const AVDiscard_AVDISCARD_NONREF: AVDiscard = 8;
pub const AVDiscard_AVDISCARD_BIDIR: AVDiscard = 16;
pub const AVDiscard_AVDISCARD_NONINTRA: AVDiscard = 24;
pub const AVDiscard_AVDISCARD_NONKEY: AVDiscard = 32;
pub const AVDiscard_AVDISCARD_ALL: AVDiscard = 48;
pub type AVDiscard = ::std::os::raw::c_int;
pub const AVAudioServiceType_AV_AUDIO_SERVICE_TYPE_MAIN: AVAudioServiceType = 0;
pub const AVAudioServiceType_AV_AUDIO_SERVICE_TYPE_EFFECTS: AVAudioServiceType = 1;
pub const AVAudioServiceType_AV_AUDIO_SERVICE_TYPE_VISUALLY_IMPAIRED: AVAudioServiceType = 2;
pub const AVAudioServiceType_AV_AUDIO_SERVICE_TYPE_HEARING_IMPAIRED: AVAudioServiceType = 3;
pub const AVAudioServiceType_AV_AUDIO_SERVICE_TYPE_DIALOGUE: AVAudioServiceType = 4;
pub const AVAudioServiceType_AV_AUDIO_SERVICE_TYPE_COMMENTARY: AVAudioServiceType = 5;
pub const AVAudioServiceType_AV_AUDIO_SERVICE_TYPE_EMERGENCY: AVAudioServiceType = 6;
pub const AVAudioServiceType_AV_AUDIO_SERVICE_TYPE_VOICE_OVER: AVAudioServiceType = 7;
pub const AVAudioServiceType_AV_AUDIO_SERVICE_TYPE_KARAOKE: AVAudioServiceType = 8;
pub const AVAudioServiceType_AV_AUDIO_SERVICE_TYPE_NB: AVAudioServiceType = 9;
pub type AVAudioServiceType = ::std::os::raw::c_int;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVCPBProperties {
    pub max_bitrate: i64,
    pub min_bitrate: i64,
    pub avg_bitrate: i64,
    pub buffer_size: i64,
    pub vbv_delay: u64,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVCPBProperties"][::std::mem::size_of::<AVCPBProperties>() - 40usize];
    ["Alignment of AVCPBProperties"][::std::mem::align_of::<AVCPBProperties>() - 8usize];
    ["Offset of field: AVCPBProperties::max_bitrate"]
        [::std::mem::offset_of!(AVCPBProperties, max_bitrate) - 0usize];
    ["Offset of field: AVCPBProperties::min_bitrate"]
        [::std::mem::offset_of!(AVCPBProperties, min_bitrate) - 8usize];
    ["Offset of field: AVCPBProperties::avg_bitrate"]
        [::std::mem::offset_of!(AVCPBProperties, avg_bitrate) - 16usize];
    ["Offset of field: AVCPBProperties::buffer_size"]
        [::std::mem::offset_of!(AVCPBProperties, buffer_size) - 24usize];
    ["Offset of field: AVCPBProperties::vbv_delay"]
        [::std::mem::offset_of!(AVCPBProperties, vbv_delay) - 32usize];
};
unsafe extern "C" {
    pub fn av_cpb_properties_alloc(size: *mut usize) -> *mut AVCPBProperties;
}
unsafe extern "C" {
    pub fn av_xiphlacing(
        s: *mut ::std::os::raw::c_uchar,
        v: ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_uint;
}
pub const AVPacketSideDataType_AV_PKT_DATA_PALETTE: AVPacketSideDataType = 0;
pub const AVPacketSideDataType_AV_PKT_DATA_NEW_EXTRADATA: AVPacketSideDataType = 1;
pub const AVPacketSideDataType_AV_PKT_DATA_PARAM_CHANGE: AVPacketSideDataType = 2;
pub const AVPacketSideDataType_AV_PKT_DATA_H263_MB_INFO: AVPacketSideDataType = 3;
pub const AVPacketSideDataType_AV_PKT_DATA_REPLAYGAIN: AVPacketSideDataType = 4;
pub const AVPacketSideDataType_AV_PKT_DATA_DISPLAYMATRIX: AVPacketSideDataType = 5;
pub const AVPacketSideDataType_AV_PKT_DATA_STEREO3D: AVPacketSideDataType = 6;
pub const AVPacketSideDataType_AV_PKT_DATA_AUDIO_SERVICE_TYPE: AVPacketSideDataType = 7;
pub const AVPacketSideDataType_AV_PKT_DATA_QUALITY_STATS: AVPacketSideDataType = 8;
pub const AVPacketSideDataType_AV_PKT_DATA_FALLBACK_TRACK: AVPacketSideDataType = 9;
pub const AVPacketSideDataType_AV_PKT_DATA_CPB_PROPERTIES: AVPacketSideDataType = 10;
pub const AVPacketSideDataType_AV_PKT_DATA_SKIP_SAMPLES: AVPacketSideDataType = 11;
pub const AVPacketSideDataType_AV_PKT_DATA_JP_DUALMONO: AVPacketSideDataType = 12;
pub const AVPacketSideDataType_AV_PKT_DATA_STRINGS_METADATA: AVPacketSideDataType = 13;
pub const AVPacketSideDataType_AV_PKT_DATA_SUBTITLE_POSITION: AVPacketSideDataType = 14;
pub const AVPacketSideDataType_AV_PKT_DATA_MATROSKA_BLOCKADDITIONAL: AVPacketSideDataType = 15;
pub const AVPacketSideDataType_AV_PKT_DATA_WEBVTT_IDENTIFIER: AVPacketSideDataType = 16;
pub const AVPacketSideDataType_AV_PKT_DATA_WEBVTT_SETTINGS: AVPacketSideDataType = 17;
pub const AVPacketSideDataType_AV_PKT_DATA_METADATA_UPDATE: AVPacketSideDataType = 18;
pub const AVPacketSideDataType_AV_PKT_DATA_MPEGTS_STREAM_ID: AVPacketSideDataType = 19;
pub const AVPacketSideDataType_AV_PKT_DATA_MASTERING_DISPLAY_METADATA: AVPacketSideDataType = 20;
pub const AVPacketSideDataType_AV_PKT_DATA_SPHERICAL: AVPacketSideDataType = 21;
pub const AVPacketSideDataType_AV_PKT_DATA_CONTENT_LIGHT_LEVEL: AVPacketSideDataType = 22;
pub const AVPacketSideDataType_AV_PKT_DATA_A53_CC: AVPacketSideDataType = 23;
pub const AVPacketSideDataType_AV_PKT_DATA_ENCRYPTION_INIT_INFO: AVPacketSideDataType = 24;
pub const AVPacketSideDataType_AV_PKT_DATA_ENCRYPTION_INFO: AVPacketSideDataType = 25;
pub const AVPacketSideDataType_AV_PKT_DATA_AFD: AVPacketSideDataType = 26;
pub const AVPacketSideDataType_AV_PKT_DATA_PRFT: AVPacketSideDataType = 27;
pub const AVPacketSideDataType_AV_PKT_DATA_ICC_PROFILE: AVPacketSideDataType = 28;
pub const AVPacketSideDataType_AV_PKT_DATA_DOVI_CONF: AVPacketSideDataType = 29;
pub const AVPacketSideDataType_AV_PKT_DATA_S12M_TIMECODE: AVPacketSideDataType = 30;
pub const AVPacketSideDataType_AV_PKT_DATA_DYNAMIC_HDR10_PLUS: AVPacketSideDataType = 31;
pub const AVPacketSideDataType_AV_PKT_DATA_IAMF_MIX_GAIN_PARAM: AVPacketSideDataType = 32;
pub const AVPacketSideDataType_AV_PKT_DATA_IAMF_DEMIXING_INFO_PARAM: AVPacketSideDataType = 33;
pub const AVPacketSideDataType_AV_PKT_DATA_IAMF_RECON_GAIN_INFO_PARAM: AVPacketSideDataType = 34;
pub const AVPacketSideDataType_AV_PKT_DATA_AMBIENT_VIEWING_ENVIRONMENT: AVPacketSideDataType = 35;
pub const AVPacketSideDataType_AV_PKT_DATA_FRAME_CROPPING: AVPacketSideDataType = 36;
pub const AVPacketSideDataType_AV_PKT_DATA_LCEVC: AVPacketSideDataType = 37;
pub const AVPacketSideDataType_AV_PKT_DATA_3D_REFERENCE_DISPLAYS: AVPacketSideDataType = 38;
pub const AVPacketSideDataType_AV_PKT_DATA_RTCP_SR: AVPacketSideDataType = 39;
pub const AVPacketSideDataType_AV_PKT_DATA_EXIF: AVPacketSideDataType = 40;
pub const AVPacketSideDataType_AV_PKT_DATA_NB: AVPacketSideDataType = 41;
pub type AVPacketSideDataType = ::std::os::raw::c_int;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVPacketSideData {
    pub data: *mut u8,
    pub size: usize,
    pub type_: AVPacketSideDataType,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVPacketSideData"][::std::mem::size_of::<AVPacketSideData>() - 24usize];
    ["Alignment of AVPacketSideData"][::std::mem::align_of::<AVPacketSideData>() - 8usize];
    ["Offset of field: AVPacketSideData::data"]
        [::std::mem::offset_of!(AVPacketSideData, data) - 0usize];
    ["Offset of field: AVPacketSideData::size"]
        [::std::mem::offset_of!(AVPacketSideData, size) - 8usize];
    ["Offset of field: AVPacketSideData::type_"]
        [::std::mem::offset_of!(AVPacketSideData, type_) - 16usize];
};
unsafe extern "C" {
    pub fn av_packet_side_data_new(
        psd: *mut *mut AVPacketSideData,
        pnb_sd: *mut ::std::os::raw::c_int,
        type_: AVPacketSideDataType,
        size: usize,
        flags: ::std::os::raw::c_int,
    ) -> *mut AVPacketSideData;
}
unsafe extern "C" {
    pub fn av_packet_side_data_add(
        sd: *mut *mut AVPacketSideData,
        nb_sd: *mut ::std::os::raw::c_int,
        type_: AVPacketSideDataType,
        data: *mut ::std::os::raw::c_void,
        size: usize,
        flags: ::std::os::raw::c_int,
    ) -> *mut AVPacketSideData;
}
unsafe extern "C" {
    pub fn av_packet_side_data_get(
        sd: *const AVPacketSideData,
        nb_sd: ::std::os::raw::c_int,
        type_: AVPacketSideDataType,
    ) -> *const AVPacketSideData;
}
unsafe extern "C" {
    pub fn av_packet_side_data_remove(
        sd: *mut AVPacketSideData,
        nb_sd: *mut ::std::os::raw::c_int,
        type_: AVPacketSideDataType,
    );
}
unsafe extern "C" {
    pub fn av_packet_side_data_free(
        sd: *mut *mut AVPacketSideData,
        nb_sd: *mut ::std::os::raw::c_int,
    );
}
unsafe extern "C" {
    pub fn av_packet_side_data_from_frame(
        sd: *mut *mut AVPacketSideData,
        nb_sd: *mut ::std::os::raw::c_int,
        src: *const AVFrameSideData,
        flags: ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_packet_side_data_to_frame(
        sd: *mut *mut *mut AVFrameSideData,
        nb_sd: *mut ::std::os::raw::c_int,
        src: *const AVPacketSideData,
        flags: ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_packet_side_data_name(type_: AVPacketSideDataType) -> *const ::std::os::raw::c_char;
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVPacket {
    pub buf: *mut AVBufferRef,
    pub pts: i64,
    pub dts: i64,
    pub data: *mut u8,
    pub size: ::std::os::raw::c_int,
    pub stream_index: ::std::os::raw::c_int,
    pub flags: ::std::os::raw::c_int,
    pub side_data: *mut AVPacketSideData,
    pub side_data_elems: ::std::os::raw::c_int,
    pub duration: i64,
    pub pos: i64,
    pub opaque: *mut ::std::os::raw::c_void,
    pub opaque_ref: *mut AVBufferRef,
    pub time_base: AVRational,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVPacket"][::std::mem::size_of::<AVPacket>() - 104usize];
    ["Alignment of AVPacket"][::std::mem::align_of::<AVPacket>() - 8usize];
    ["Offset of field: AVPacket::buf"][::std::mem::offset_of!(AVPacket, buf) - 0usize];
    ["Offset of field: AVPacket::pts"][::std::mem::offset_of!(AVPacket, pts) - 8usize];
    ["Offset of field: AVPacket::dts"][::std::mem::offset_of!(AVPacket, dts) - 16usize];
    ["Offset of field: AVPacket::data"][::std::mem::offset_of!(AVPacket, data) - 24usize];
    ["Offset of field: AVPacket::size"][::std::mem::offset_of!(AVPacket, size) - 32usize];
    ["Offset of field: AVPacket::stream_index"]
        [::std::mem::offset_of!(AVPacket, stream_index) - 36usize];
    ["Offset of field: AVPacket::flags"][::std::mem::offset_of!(AVPacket, flags) - 40usize];
    ["Offset of field: AVPacket::side_data"][::std::mem::offset_of!(AVPacket, side_data) - 48usize];
    ["Offset of field: AVPacket::side_data_elems"]
        [::std::mem::offset_of!(AVPacket, side_data_elems) - 56usize];
    ["Offset of field: AVPacket::duration"][::std::mem::offset_of!(AVPacket, duration) - 64usize];
    ["Offset of field: AVPacket::pos"][::std::mem::offset_of!(AVPacket, pos) - 72usize];
    ["Offset of field: AVPacket::opaque"][::std::mem::offset_of!(AVPacket, opaque) - 80usize];
    ["Offset of field: AVPacket::opaque_ref"]
        [::std::mem::offset_of!(AVPacket, opaque_ref) - 88usize];
    ["Offset of field: AVPacket::time_base"][::std::mem::offset_of!(AVPacket, time_base) - 96usize];
};
unsafe extern "C" {
    pub fn av_packet_alloc() -> *mut AVPacket;
}
unsafe extern "C" {
    pub fn av_packet_clone(src: *const AVPacket) -> *mut AVPacket;
}
unsafe extern "C" {
    pub fn av_packet_free(pkt: *mut *mut AVPacket);
}
unsafe extern "C" {
    pub fn av_init_packet(pkt: *mut AVPacket);
}
unsafe extern "C" {
    pub fn av_new_packet(pkt: *mut AVPacket, size: ::std::os::raw::c_int) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_shrink_packet(pkt: *mut AVPacket, size: ::std::os::raw::c_int);
}
unsafe extern "C" {
    pub fn av_grow_packet(
        pkt: *mut AVPacket,
        grow_by: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_packet_from_data(
        pkt: *mut AVPacket,
        data: *mut u8,
        size: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_packet_new_side_data(
        pkt: *mut AVPacket,
        type_: AVPacketSideDataType,
        size: usize,
    ) -> *mut u8;
}
unsafe extern "C" {
    pub fn av_packet_add_side_data(
        pkt: *mut AVPacket,
        type_: AVPacketSideDataType,
        data: *mut u8,
        size: usize,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_packet_shrink_side_data(
        pkt: *mut AVPacket,
        type_: AVPacketSideDataType,
        size: usize,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_packet_get_side_data(
        pkt: *const AVPacket,
        type_: AVPacketSideDataType,
        size: *mut usize,
    ) -> *mut u8;
}
unsafe extern "C" {
    pub fn av_packet_pack_dictionary(dict: *mut AVDictionary, size: *mut usize) -> *mut u8;
}
unsafe extern "C" {
    pub fn av_packet_unpack_dictionary(
        data: *const u8,
        size: usize,
        dict: *mut *mut AVDictionary,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_packet_free_side_data(pkt: *mut AVPacket);
}
unsafe extern "C" {
    pub fn av_packet_ref(dst: *mut AVPacket, src: *const AVPacket) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_packet_unref(pkt: *mut AVPacket);
}
unsafe extern "C" {
    pub fn av_packet_move_ref(dst: *mut AVPacket, src: *mut AVPacket);
}
unsafe extern "C" {
    pub fn av_packet_copy_props(dst: *mut AVPacket, src: *const AVPacket) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_packet_make_refcounted(pkt: *mut AVPacket) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_packet_make_writable(pkt: *mut AVPacket) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_packet_rescale_ts(pkt: *mut AVPacket, tb_src: AVRational, tb_dst: AVRational);
}
#[repr(C)]
#[derive(Debug)]
pub struct AVContainerFifo {
    _unused: [u8; 0],
}
unsafe extern "C" {
    pub fn av_container_fifo_alloc_avpacket(flags: ::std::os::raw::c_uint) -> *mut AVContainerFifo;
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVCodecDescriptor {
    pub id: AVCodecID,
    pub type_: AVMediaType,
    pub name: *const ::std::os::raw::c_char,
    pub long_name: *const ::std::os::raw::c_char,
    pub props: ::std::os::raw::c_int,
    pub mime_types: *const *const ::std::os::raw::c_char,
    pub profiles: *const AVProfile,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVCodecDescriptor"][::std::mem::size_of::<AVCodecDescriptor>() - 48usize];
    ["Alignment of AVCodecDescriptor"][::std::mem::align_of::<AVCodecDescriptor>() - 8usize];
    ["Offset of field: AVCodecDescriptor::id"]
        [::std::mem::offset_of!(AVCodecDescriptor, id) - 0usize];
    ["Offset of field: AVCodecDescriptor::type_"]
        [::std::mem::offset_of!(AVCodecDescriptor, type_) - 4usize];
    ["Offset of field: AVCodecDescriptor::name"]
        [::std::mem::offset_of!(AVCodecDescriptor, name) - 8usize];
    ["Offset of field: AVCodecDescriptor::long_name"]
        [::std::mem::offset_of!(AVCodecDescriptor, long_name) - 16usize];
    ["Offset of field: AVCodecDescriptor::props"]
        [::std::mem::offset_of!(AVCodecDescriptor, props) - 24usize];
    ["Offset of field: AVCodecDescriptor::mime_types"]
        [::std::mem::offset_of!(AVCodecDescriptor, mime_types) - 32usize];
    ["Offset of field: AVCodecDescriptor::profiles"]
        [::std::mem::offset_of!(AVCodecDescriptor, profiles) - 40usize];
};
#[repr(C)]
#[derive(Copy, Clone)]
pub struct AVCodecParameters {
    pub codec_type: AVMediaType,
    pub codec_id: AVCodecID,
    pub codec_tag: u32,
    pub extradata: *mut u8,
    pub extradata_size: ::std::os::raw::c_int,
    pub coded_side_data: *mut AVPacketSideData,
    pub nb_coded_side_data: ::std::os::raw::c_int,
    pub format: ::std::os::raw::c_int,
    pub bit_rate: i64,
    pub bits_per_coded_sample: ::std::os::raw::c_int,
    pub bits_per_raw_sample: ::std::os::raw::c_int,
    pub profile: ::std::os::raw::c_int,
    pub level: ::std::os::raw::c_int,
    pub width: ::std::os::raw::c_int,
    pub height: ::std::os::raw::c_int,
    pub sample_aspect_ratio: AVRational,
    pub framerate: AVRational,
    pub field_order: AVFieldOrder,
    pub color_range: AVColorRange,
    pub color_primaries: AVColorPrimaries,
    pub color_trc: AVColorTransferCharacteristic,
    pub color_space: AVColorSpace,
    pub chroma_location: AVChromaLocation,
    pub video_delay: ::std::os::raw::c_int,
    pub ch_layout: AVChannelLayout,
    pub sample_rate: ::std::os::raw::c_int,
    pub block_align: ::std::os::raw::c_int,
    pub frame_size: ::std::os::raw::c_int,
    pub initial_padding: ::std::os::raw::c_int,
    pub trailing_padding: ::std::os::raw::c_int,
    pub seek_preroll: ::std::os::raw::c_int,
    pub alpha_mode: AVAlphaMode,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVCodecParameters"][::std::mem::size_of::<AVCodecParameters>() - 184usize];
    ["Alignment of AVCodecParameters"][::std::mem::align_of::<AVCodecParameters>() - 8usize];
    ["Offset of field: AVCodecParameters::codec_type"]
        [::std::mem::offset_of!(AVCodecParameters, codec_type) - 0usize];
    ["Offset of field: AVCodecParameters::codec_id"]
        [::std::mem::offset_of!(AVCodecParameters, codec_id) - 4usize];
    ["Offset of field: AVCodecParameters::codec_tag"]
        [::std::mem::offset_of!(AVCodecParameters, codec_tag) - 8usize];
    ["Offset of field: AVCodecParameters::extradata"]
        [::std::mem::offset_of!(AVCodecParameters, extradata) - 16usize];
    ["Offset of field: AVCodecParameters::extradata_size"]
        [::std::mem::offset_of!(AVCodecParameters, extradata_size) - 24usize];
    ["Offset of field: AVCodecParameters::coded_side_data"]
        [::std::mem::offset_of!(AVCodecParameters, coded_side_data) - 32usize];
    ["Offset of field: AVCodecParameters::nb_coded_side_data"]
        [::std::mem::offset_of!(AVCodecParameters, nb_coded_side_data) - 40usize];
    ["Offset of field: AVCodecParameters::format"]
        [::std::mem::offset_of!(AVCodecParameters, format) - 44usize];
    ["Offset of field: AVCodecParameters::bit_rate"]
        [::std::mem::offset_of!(AVCodecParameters, bit_rate) - 48usize];
    ["Offset of field: AVCodecParameters::bits_per_coded_sample"]
        [::std::mem::offset_of!(AVCodecParameters, bits_per_coded_sample) - 56usize];
    ["Offset of field: AVCodecParameters::bits_per_raw_sample"]
        [::std::mem::offset_of!(AVCodecParameters, bits_per_raw_sample) - 60usize];
    ["Offset of field: AVCodecParameters::profile"]
        [::std::mem::offset_of!(AVCodecParameters, profile) - 64usize];
    ["Offset of field: AVCodecParameters::level"]
        [::std::mem::offset_of!(AVCodecParameters, level) - 68usize];
    ["Offset of field: AVCodecParameters::width"]
        [::std::mem::offset_of!(AVCodecParameters, width) - 72usize];
    ["Offset of field: AVCodecParameters::height"]
        [::std::mem::offset_of!(AVCodecParameters, height) - 76usize];
    ["Offset of field: AVCodecParameters::sample_aspect_ratio"]
        [::std::mem::offset_of!(AVCodecParameters, sample_aspect_ratio) - 80usize];
    ["Offset of field: AVCodecParameters::framerate"]
        [::std::mem::offset_of!(AVCodecParameters, framerate) - 88usize];
    ["Offset of field: AVCodecParameters::field_order"]
        [::std::mem::offset_of!(AVCodecParameters, field_order) - 96usize];
    ["Offset of field: AVCodecParameters::color_range"]
        [::std::mem::offset_of!(AVCodecParameters, color_range) - 100usize];
    ["Offset of field: AVCodecParameters::color_primaries"]
        [::std::mem::offset_of!(AVCodecParameters, color_primaries) - 104usize];
    ["Offset of field: AVCodecParameters::color_trc"]
        [::std::mem::offset_of!(AVCodecParameters, color_trc) - 108usize];
    ["Offset of field: AVCodecParameters::color_space"]
        [::std::mem::offset_of!(AVCodecParameters, color_space) - 112usize];
    ["Offset of field: AVCodecParameters::chroma_location"]
        [::std::mem::offset_of!(AVCodecParameters, chroma_location) - 116usize];
    ["Offset of field: AVCodecParameters::video_delay"]
        [::std::mem::offset_of!(AVCodecParameters, video_delay) - 120usize];
    ["Offset of field: AVCodecParameters::ch_layout"]
        [::std::mem::offset_of!(AVCodecParameters, ch_layout) - 128usize];
    ["Offset of field: AVCodecParameters::sample_rate"]
        [::std::mem::offset_of!(AVCodecParameters, sample_rate) - 152usize];
    ["Offset of field: AVCodecParameters::block_align"]
        [::std::mem::offset_of!(AVCodecParameters, block_align) - 156usize];
    ["Offset of field: AVCodecParameters::frame_size"]
        [::std::mem::offset_of!(AVCodecParameters, frame_size) - 160usize];
    ["Offset of field: AVCodecParameters::initial_padding"]
        [::std::mem::offset_of!(AVCodecParameters, initial_padding) - 164usize];
    ["Offset of field: AVCodecParameters::trailing_padding"]
        [::std::mem::offset_of!(AVCodecParameters, trailing_padding) - 168usize];
    ["Offset of field: AVCodecParameters::seek_preroll"]
        [::std::mem::offset_of!(AVCodecParameters, seek_preroll) - 172usize];
    ["Offset of field: AVCodecParameters::alpha_mode"]
        [::std::mem::offset_of!(AVCodecParameters, alpha_mode) - 176usize];
};
unsafe extern "C" {
    pub fn av_get_audio_frame_duration2(
        par: *mut AVCodecParameters,
        frame_bytes: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct RcOverride {
    pub start_frame: ::std::os::raw::c_int,
    pub end_frame: ::std::os::raw::c_int,
    pub qscale: ::std::os::raw::c_int,
    pub quality_factor: f32,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of RcOverride"][::std::mem::size_of::<RcOverride>() - 16usize];
    ["Alignment of RcOverride"][::std::mem::align_of::<RcOverride>() - 4usize];
    ["Offset of field: RcOverride::start_frame"]
        [::std::mem::offset_of!(RcOverride, start_frame) - 0usize];
    ["Offset of field: RcOverride::end_frame"]
        [::std::mem::offset_of!(RcOverride, end_frame) - 4usize];
    ["Offset of field: RcOverride::qscale"][::std::mem::offset_of!(RcOverride, qscale) - 8usize];
    ["Offset of field: RcOverride::quality_factor"]
        [::std::mem::offset_of!(RcOverride, quality_factor) - 12usize];
};
#[repr(C)]
#[derive(Copy, Clone)]
pub struct AVCodecContext {
    pub av_class: *const AVClass,
    pub log_level_offset: ::std::os::raw::c_int,
    pub codec_type: AVMediaType,
    pub codec: *const AVCodec,
    pub codec_id: AVCodecID,
    pub codec_tag: ::std::os::raw::c_uint,
    pub priv_data: *mut ::std::os::raw::c_void,
    pub internal: *mut AVCodecInternal,
    pub opaque: *mut ::std::os::raw::c_void,
    pub bit_rate: i64,
    pub flags: ::std::os::raw::c_int,
    pub flags2: ::std::os::raw::c_int,
    pub extradata: *mut u8,
    pub extradata_size: ::std::os::raw::c_int,
    pub time_base: AVRational,
    pub pkt_timebase: AVRational,
    pub framerate: AVRational,
    pub delay: ::std::os::raw::c_int,
    pub width: ::std::os::raw::c_int,
    pub height: ::std::os::raw::c_int,
    pub coded_width: ::std::os::raw::c_int,
    pub coded_height: ::std::os::raw::c_int,
    pub sample_aspect_ratio: AVRational,
    pub pix_fmt: AVPixelFormat,
    pub sw_pix_fmt: AVPixelFormat,
    pub color_primaries: AVColorPrimaries,
    pub color_trc: AVColorTransferCharacteristic,
    pub colorspace: AVColorSpace,
    pub color_range: AVColorRange,
    pub chroma_sample_location: AVChromaLocation,
    pub field_order: AVFieldOrder,
    pub refs: ::std::os::raw::c_int,
    pub has_b_frames: ::std::os::raw::c_int,
    pub slice_flags: ::std::os::raw::c_int,
    pub draw_horiz_band: ::std::option::Option<
        unsafe extern "C" fn(
            s: *mut AVCodecContext,
            src: *const AVFrame,
            offset: *mut ::std::os::raw::c_int,
            y: ::std::os::raw::c_int,
            type_: ::std::os::raw::c_int,
            height: ::std::os::raw::c_int,
        ),
    >,
    pub get_format: ::std::option::Option<
        unsafe extern "C" fn(s: *mut AVCodecContext, fmt: *const AVPixelFormat) -> AVPixelFormat,
    >,
    pub max_b_frames: ::std::os::raw::c_int,
    pub b_quant_factor: f32,
    pub b_quant_offset: f32,
    pub i_quant_factor: f32,
    pub i_quant_offset: f32,
    pub lumi_masking: f32,
    pub temporal_cplx_masking: f32,
    pub spatial_cplx_masking: f32,
    pub p_masking: f32,
    pub dark_masking: f32,
    pub nsse_weight: ::std::os::raw::c_int,
    pub me_cmp: ::std::os::raw::c_int,
    pub me_sub_cmp: ::std::os::raw::c_int,
    pub mb_cmp: ::std::os::raw::c_int,
    pub ildct_cmp: ::std::os::raw::c_int,
    pub dia_size: ::std::os::raw::c_int,
    pub last_predictor_count: ::std::os::raw::c_int,
    pub me_pre_cmp: ::std::os::raw::c_int,
    pub pre_dia_size: ::std::os::raw::c_int,
    pub me_subpel_quality: ::std::os::raw::c_int,
    pub me_range: ::std::os::raw::c_int,
    pub mb_decision: ::std::os::raw::c_int,
    pub intra_matrix: *mut u16,
    pub inter_matrix: *mut u16,
    pub chroma_intra_matrix: *mut u16,
    pub intra_dc_precision: ::std::os::raw::c_int,
    pub mb_lmin: ::std::os::raw::c_int,
    pub mb_lmax: ::std::os::raw::c_int,
    pub bidir_refine: ::std::os::raw::c_int,
    pub keyint_min: ::std::os::raw::c_int,
    pub gop_size: ::std::os::raw::c_int,
    pub mv0_threshold: ::std::os::raw::c_int,
    pub slices: ::std::os::raw::c_int,
    pub sample_rate: ::std::os::raw::c_int,
    pub sample_fmt: AVSampleFormat,
    pub ch_layout: AVChannelLayout,
    pub frame_size: ::std::os::raw::c_int,
    pub block_align: ::std::os::raw::c_int,
    pub cutoff: ::std::os::raw::c_int,
    pub audio_service_type: AVAudioServiceType,
    pub request_sample_fmt: AVSampleFormat,
    pub initial_padding: ::std::os::raw::c_int,
    pub trailing_padding: ::std::os::raw::c_int,
    pub seek_preroll: ::std::os::raw::c_int,
    pub get_buffer2: ::std::option::Option<
        unsafe extern "C" fn(
            s: *mut AVCodecContext,
            frame: *mut AVFrame,
            flags: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int,
    >,
    pub bit_rate_tolerance: ::std::os::raw::c_int,
    pub global_quality: ::std::os::raw::c_int,
    pub compression_level: ::std::os::raw::c_int,
    pub qcompress: f32,
    pub qblur: f32,
    pub qmin: ::std::os::raw::c_int,
    pub qmax: ::std::os::raw::c_int,
    pub max_qdiff: ::std::os::raw::c_int,
    pub rc_buffer_size: ::std::os::raw::c_int,
    pub rc_override_count: ::std::os::raw::c_int,
    pub rc_override: *mut RcOverride,
    pub rc_max_rate: i64,
    pub rc_min_rate: i64,
    pub rc_max_available_vbv_use: f32,
    pub rc_min_vbv_overflow_use: f32,
    pub rc_initial_buffer_occupancy: ::std::os::raw::c_int,
    pub trellis: ::std::os::raw::c_int,
    pub stats_out: *mut ::std::os::raw::c_char,
    pub stats_in: *mut ::std::os::raw::c_char,
    pub workaround_bugs: ::std::os::raw::c_int,
    pub strict_std_compliance: ::std::os::raw::c_int,
    pub error_concealment: ::std::os::raw::c_int,
    pub debug: ::std::os::raw::c_int,
    pub err_recognition: ::std::os::raw::c_int,
    pub hwaccel: *const AVHWAccel,
    pub hwaccel_context: *mut ::std::os::raw::c_void,
    pub hw_frames_ctx: *mut AVBufferRef,
    pub hw_device_ctx: *mut AVBufferRef,
    pub hwaccel_flags: ::std::os::raw::c_int,
    pub extra_hw_frames: ::std::os::raw::c_int,
    pub error: [u64; 8usize],
    pub dct_algo: ::std::os::raw::c_int,
    pub idct_algo: ::std::os::raw::c_int,
    pub bits_per_coded_sample: ::std::os::raw::c_int,
    pub bits_per_raw_sample: ::std::os::raw::c_int,
    pub thread_count: ::std::os::raw::c_int,
    pub thread_type: ::std::os::raw::c_int,
    pub active_thread_type: ::std::os::raw::c_int,
    pub execute: ::std::option::Option<
        unsafe extern "C" fn(
            c: *mut AVCodecContext,
            func: ::std::option::Option<
                unsafe extern "C" fn(
                    c2: *mut AVCodecContext,
                    arg: *mut ::std::os::raw::c_void,
                ) -> ::std::os::raw::c_int,
            >,
            arg2: *mut ::std::os::raw::c_void,
            ret: *mut ::std::os::raw::c_int,
            count: ::std::os::raw::c_int,
            size: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int,
    >,
    pub execute2: ::std::option::Option<
        unsafe extern "C" fn(
            c: *mut AVCodecContext,
            func: ::std::option::Option<
                unsafe extern "C" fn(
                    c2: *mut AVCodecContext,
                    arg: *mut ::std::os::raw::c_void,
                    jobnr: ::std::os::raw::c_int,
                    threadnr: ::std::os::raw::c_int,
                ) -> ::std::os::raw::c_int,
            >,
            arg2: *mut ::std::os::raw::c_void,
            ret: *mut ::std::os::raw::c_int,
            count: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int,
    >,
    pub profile: ::std::os::raw::c_int,
    pub level: ::std::os::raw::c_int,
    pub properties: ::std::os::raw::c_uint,
    pub skip_loop_filter: AVDiscard,
    pub skip_idct: AVDiscard,
    pub skip_frame: AVDiscard,
    pub skip_alpha: ::std::os::raw::c_int,
    pub skip_top: ::std::os::raw::c_int,
    pub skip_bottom: ::std::os::raw::c_int,
    pub lowres: ::std::os::raw::c_int,
    pub codec_descriptor: *const AVCodecDescriptor,
    pub sub_charenc: *mut ::std::os::raw::c_char,
    pub sub_charenc_mode: ::std::os::raw::c_int,
    pub subtitle_header_size: ::std::os::raw::c_int,
    pub subtitle_header: *mut u8,
    pub dump_separator: *mut u8,
    pub codec_whitelist: *mut ::std::os::raw::c_char,
    pub coded_side_data: *mut AVPacketSideData,
    pub nb_coded_side_data: ::std::os::raw::c_int,
    pub export_side_data: ::std::os::raw::c_int,
    pub max_pixels: i64,
    pub apply_cropping: ::std::os::raw::c_int,
    pub discard_damaged_percentage: ::std::os::raw::c_int,
    pub max_samples: i64,
    pub get_encode_buffer: ::std::option::Option<
        unsafe extern "C" fn(
            s: *mut AVCodecContext,
            pkt: *mut AVPacket,
            flags: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int,
    >,
    pub frame_num: i64,
    pub side_data_prefer_packet: *mut ::std::os::raw::c_int,
    pub nb_side_data_prefer_packet: ::std::os::raw::c_uint,
    pub decoded_side_data: *mut *mut AVFrameSideData,
    pub nb_decoded_side_data: ::std::os::raw::c_int,
    pub alpha_mode: AVAlphaMode,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVCodecContext"][::std::mem::size_of::<AVCodecContext>() - 864usize];
    ["Alignment of AVCodecContext"][::std::mem::align_of::<AVCodecContext>() - 8usize];
    ["Offset of field: AVCodecContext::av_class"]
        [::std::mem::offset_of!(AVCodecContext, av_class) - 0usize];
    ["Offset of field: AVCodecContext::log_level_offset"]
        [::std::mem::offset_of!(AVCodecContext, log_level_offset) - 8usize];
    ["Offset of field: AVCodecContext::codec_type"]
        [::std::mem::offset_of!(AVCodecContext, codec_type) - 12usize];
    ["Offset of field: AVCodecContext::codec"]
        [::std::mem::offset_of!(AVCodecContext, codec) - 16usize];
    ["Offset of field: AVCodecContext::codec_id"]
        [::std::mem::offset_of!(AVCodecContext, codec_id) - 24usize];
    ["Offset of field: AVCodecContext::codec_tag"]
        [::std::mem::offset_of!(AVCodecContext, codec_tag) - 28usize];
    ["Offset of field: AVCodecContext::priv_data"]
        [::std::mem::offset_of!(AVCodecContext, priv_data) - 32usize];
    ["Offset of field: AVCodecContext::internal"]
        [::std::mem::offset_of!(AVCodecContext, internal) - 40usize];
    ["Offset of field: AVCodecContext::opaque"]
        [::std::mem::offset_of!(AVCodecContext, opaque) - 48usize];
    ["Offset of field: AVCodecContext::bit_rate"]
        [::std::mem::offset_of!(AVCodecContext, bit_rate) - 56usize];
    ["Offset of field: AVCodecContext::flags"]
        [::std::mem::offset_of!(AVCodecContext, flags) - 64usize];
    ["Offset of field: AVCodecContext::flags2"]
        [::std::mem::offset_of!(AVCodecContext, flags2) - 68usize];
    ["Offset of field: AVCodecContext::extradata"]
        [::std::mem::offset_of!(AVCodecContext, extradata) - 72usize];
    ["Offset of field: AVCodecContext::extradata_size"]
        [::std::mem::offset_of!(AVCodecContext, extradata_size) - 80usize];
    ["Offset of field: AVCodecContext::time_base"]
        [::std::mem::offset_of!(AVCodecContext, time_base) - 84usize];
    ["Offset of field: AVCodecContext::pkt_timebase"]
        [::std::mem::offset_of!(AVCodecContext, pkt_timebase) - 92usize];
    ["Offset of field: AVCodecContext::framerate"]
        [::std::mem::offset_of!(AVCodecContext, framerate) - 100usize];
    ["Offset of field: AVCodecContext::delay"]
        [::std::mem::offset_of!(AVCodecContext, delay) - 108usize];
    ["Offset of field: AVCodecContext::width"]
        [::std::mem::offset_of!(AVCodecContext, width) - 112usize];
    ["Offset of field: AVCodecContext::height"]
        [::std::mem::offset_of!(AVCodecContext, height) - 116usize];
    ["Offset of field: AVCodecContext::coded_width"]
        [::std::mem::offset_of!(AVCodecContext, coded_width) - 120usize];
    ["Offset of field: AVCodecContext::coded_height"]
        [::std::mem::offset_of!(AVCodecContext, coded_height) - 124usize];
    ["Offset of field: AVCodecContext::sample_aspect_ratio"]
        [::std::mem::offset_of!(AVCodecContext, sample_aspect_ratio) - 128usize];
    ["Offset of field: AVCodecContext::pix_fmt"]
        [::std::mem::offset_of!(AVCodecContext, pix_fmt) - 136usize];
    ["Offset of field: AVCodecContext::sw_pix_fmt"]
        [::std::mem::offset_of!(AVCodecContext, sw_pix_fmt) - 140usize];
    ["Offset of field: AVCodecContext::color_primaries"]
        [::std::mem::offset_of!(AVCodecContext, color_primaries) - 144usize];
    ["Offset of field: AVCodecContext::color_trc"]
        [::std::mem::offset_of!(AVCodecContext, color_trc) - 148usize];
    ["Offset of field: AVCodecContext::colorspace"]
        [::std::mem::offset_of!(AVCodecContext, colorspace) - 152usize];
    ["Offset of field: AVCodecContext::color_range"]
        [::std::mem::offset_of!(AVCodecContext, color_range) - 156usize];
    ["Offset of field: AVCodecContext::chroma_sample_location"]
        [::std::mem::offset_of!(AVCodecContext, chroma_sample_location) - 160usize];
    ["Offset of field: AVCodecContext::field_order"]
        [::std::mem::offset_of!(AVCodecContext, field_order) - 164usize];
    ["Offset of field: AVCodecContext::refs"]
        [::std::mem::offset_of!(AVCodecContext, refs) - 168usize];
    ["Offset of field: AVCodecContext::has_b_frames"]
        [::std::mem::offset_of!(AVCodecContext, has_b_frames) - 172usize];
    ["Offset of field: AVCodecContext::slice_flags"]
        [::std::mem::offset_of!(AVCodecContext, slice_flags) - 176usize];
    ["Offset of field: AVCodecContext::draw_horiz_band"]
        [::std::mem::offset_of!(AVCodecContext, draw_horiz_band) - 184usize];
    ["Offset of field: AVCodecContext::get_format"]
        [::std::mem::offset_of!(AVCodecContext, get_format) - 192usize];
    ["Offset of field: AVCodecContext::max_b_frames"]
        [::std::mem::offset_of!(AVCodecContext, max_b_frames) - 200usize];
    ["Offset of field: AVCodecContext::b_quant_factor"]
        [::std::mem::offset_of!(AVCodecContext, b_quant_factor) - 204usize];
    ["Offset of field: AVCodecContext::b_quant_offset"]
        [::std::mem::offset_of!(AVCodecContext, b_quant_offset) - 208usize];
    ["Offset of field: AVCodecContext::i_quant_factor"]
        [::std::mem::offset_of!(AVCodecContext, i_quant_factor) - 212usize];
    ["Offset of field: AVCodecContext::i_quant_offset"]
        [::std::mem::offset_of!(AVCodecContext, i_quant_offset) - 216usize];
    ["Offset of field: AVCodecContext::lumi_masking"]
        [::std::mem::offset_of!(AVCodecContext, lumi_masking) - 220usize];
    ["Offset of field: AVCodecContext::temporal_cplx_masking"]
        [::std::mem::offset_of!(AVCodecContext, temporal_cplx_masking) - 224usize];
    ["Offset of field: AVCodecContext::spatial_cplx_masking"]
        [::std::mem::offset_of!(AVCodecContext, spatial_cplx_masking) - 228usize];
    ["Offset of field: AVCodecContext::p_masking"]
        [::std::mem::offset_of!(AVCodecContext, p_masking) - 232usize];
    ["Offset of field: AVCodecContext::dark_masking"]
        [::std::mem::offset_of!(AVCodecContext, dark_masking) - 236usize];
    ["Offset of field: AVCodecContext::nsse_weight"]
        [::std::mem::offset_of!(AVCodecContext, nsse_weight) - 240usize];
    ["Offset of field: AVCodecContext::me_cmp"]
        [::std::mem::offset_of!(AVCodecContext, me_cmp) - 244usize];
    ["Offset of field: AVCodecContext::me_sub_cmp"]
        [::std::mem::offset_of!(AVCodecContext, me_sub_cmp) - 248usize];
    ["Offset of field: AVCodecContext::mb_cmp"]
        [::std::mem::offset_of!(AVCodecContext, mb_cmp) - 252usize];
    ["Offset of field: AVCodecContext::ildct_cmp"]
        [::std::mem::offset_of!(AVCodecContext, ildct_cmp) - 256usize];
    ["Offset of field: AVCodecContext::dia_size"]
        [::std::mem::offset_of!(AVCodecContext, dia_size) - 260usize];
    ["Offset of field: AVCodecContext::last_predictor_count"]
        [::std::mem::offset_of!(AVCodecContext, last_predictor_count) - 264usize];
    ["Offset of field: AVCodecContext::me_pre_cmp"]
        [::std::mem::offset_of!(AVCodecContext, me_pre_cmp) - 268usize];
    ["Offset of field: AVCodecContext::pre_dia_size"]
        [::std::mem::offset_of!(AVCodecContext, pre_dia_size) - 272usize];
    ["Offset of field: AVCodecContext::me_subpel_quality"]
        [::std::mem::offset_of!(AVCodecContext, me_subpel_quality) - 276usize];
    ["Offset of field: AVCodecContext::me_range"]
        [::std::mem::offset_of!(AVCodecContext, me_range) - 280usize];
    ["Offset of field: AVCodecContext::mb_decision"]
        [::std::mem::offset_of!(AVCodecContext, mb_decision) - 284usize];
    ["Offset of field: AVCodecContext::intra_matrix"]
        [::std::mem::offset_of!(AVCodecContext, intra_matrix) - 288usize];
    ["Offset of field: AVCodecContext::inter_matrix"]
        [::std::mem::offset_of!(AVCodecContext, inter_matrix) - 296usize];
    ["Offset of field: AVCodecContext::chroma_intra_matrix"]
        [::std::mem::offset_of!(AVCodecContext, chroma_intra_matrix) - 304usize];
    ["Offset of field: AVCodecContext::intra_dc_precision"]
        [::std::mem::offset_of!(AVCodecContext, intra_dc_precision) - 312usize];
    ["Offset of field: AVCodecContext::mb_lmin"]
        [::std::mem::offset_of!(AVCodecContext, mb_lmin) - 316usize];
    ["Offset of field: AVCodecContext::mb_lmax"]
        [::std::mem::offset_of!(AVCodecContext, mb_lmax) - 320usize];
    ["Offset of field: AVCodecContext::bidir_refine"]
        [::std::mem::offset_of!(AVCodecContext, bidir_refine) - 324usize];
    ["Offset of field: AVCodecContext::keyint_min"]
        [::std::mem::offset_of!(AVCodecContext, keyint_min) - 328usize];
    ["Offset of field: AVCodecContext::gop_size"]
        [::std::mem::offset_of!(AVCodecContext, gop_size) - 332usize];
    ["Offset of field: AVCodecContext::mv0_threshold"]
        [::std::mem::offset_of!(AVCodecContext, mv0_threshold) - 336usize];
    ["Offset of field: AVCodecContext::slices"]
        [::std::mem::offset_of!(AVCodecContext, slices) - 340usize];
    ["Offset of field: AVCodecContext::sample_rate"]
        [::std::mem::offset_of!(AVCodecContext, sample_rate) - 344usize];
    ["Offset of field: AVCodecContext::sample_fmt"]
        [::std::mem::offset_of!(AVCodecContext, sample_fmt) - 348usize];
    ["Offset of field: AVCodecContext::ch_layout"]
        [::std::mem::offset_of!(AVCodecContext, ch_layout) - 352usize];
    ["Offset of field: AVCodecContext::frame_size"]
        [::std::mem::offset_of!(AVCodecContext, frame_size) - 376usize];
    ["Offset of field: AVCodecContext::block_align"]
        [::std::mem::offset_of!(AVCodecContext, block_align) - 380usize];
    ["Offset of field: AVCodecContext::cutoff"]
        [::std::mem::offset_of!(AVCodecContext, cutoff) - 384usize];
    ["Offset of field: AVCodecContext::audio_service_type"]
        [::std::mem::offset_of!(AVCodecContext, audio_service_type) - 388usize];
    ["Offset of field: AVCodecContext::request_sample_fmt"]
        [::std::mem::offset_of!(AVCodecContext, request_sample_fmt) - 392usize];
    ["Offset of field: AVCodecContext::initial_padding"]
        [::std::mem::offset_of!(AVCodecContext, initial_padding) - 396usize];
    ["Offset of field: AVCodecContext::trailing_padding"]
        [::std::mem::offset_of!(AVCodecContext, trailing_padding) - 400usize];
    ["Offset of field: AVCodecContext::seek_preroll"]
        [::std::mem::offset_of!(AVCodecContext, seek_preroll) - 404usize];
    ["Offset of field: AVCodecContext::get_buffer2"]
        [::std::mem::offset_of!(AVCodecContext, get_buffer2) - 408usize];
    ["Offset of field: AVCodecContext::bit_rate_tolerance"]
        [::std::mem::offset_of!(AVCodecContext, bit_rate_tolerance) - 416usize];
    ["Offset of field: AVCodecContext::global_quality"]
        [::std::mem::offset_of!(AVCodecContext, global_quality) - 420usize];
    ["Offset of field: AVCodecContext::compression_level"]
        [::std::mem::offset_of!(AVCodecContext, compression_level) - 424usize];
    ["Offset of field: AVCodecContext::qcompress"]
        [::std::mem::offset_of!(AVCodecContext, qcompress) - 428usize];
    ["Offset of field: AVCodecContext::qblur"]
        [::std::mem::offset_of!(AVCodecContext, qblur) - 432usize];
    ["Offset of field: AVCodecContext::qmin"]
        [::std::mem::offset_of!(AVCodecContext, qmin) - 436usize];
    ["Offset of field: AVCodecContext::qmax"]
        [::std::mem::offset_of!(AVCodecContext, qmax) - 440usize];
    ["Offset of field: AVCodecContext::max_qdiff"]
        [::std::mem::offset_of!(AVCodecContext, max_qdiff) - 444usize];
    ["Offset of field: AVCodecContext::rc_buffer_size"]
        [::std::mem::offset_of!(AVCodecContext, rc_buffer_size) - 448usize];
    ["Offset of field: AVCodecContext::rc_override_count"]
        [::std::mem::offset_of!(AVCodecContext, rc_override_count) - 452usize];
    ["Offset of field: AVCodecContext::rc_override"]
        [::std::mem::offset_of!(AVCodecContext, rc_override) - 456usize];
    ["Offset of field: AVCodecContext::rc_max_rate"]
        [::std::mem::offset_of!(AVCodecContext, rc_max_rate) - 464usize];
    ["Offset of field: AVCodecContext::rc_min_rate"]
        [::std::mem::offset_of!(AVCodecContext, rc_min_rate) - 472usize];
    ["Offset of field: AVCodecContext::rc_max_available_vbv_use"]
        [::std::mem::offset_of!(AVCodecContext, rc_max_available_vbv_use) - 480usize];
    ["Offset of field: AVCodecContext::rc_min_vbv_overflow_use"]
        [::std::mem::offset_of!(AVCodecContext, rc_min_vbv_overflow_use) - 484usize];
    ["Offset of field: AVCodecContext::rc_initial_buffer_occupancy"]
        [::std::mem::offset_of!(AVCodecContext, rc_initial_buffer_occupancy) - 488usize];
    ["Offset of field: AVCodecContext::trellis"]
        [::std::mem::offset_of!(AVCodecContext, trellis) - 492usize];
    ["Offset of field: AVCodecContext::stats_out"]
        [::std::mem::offset_of!(AVCodecContext, stats_out) - 496usize];
    ["Offset of field: AVCodecContext::stats_in"]
        [::std::mem::offset_of!(AVCodecContext, stats_in) - 504usize];
    ["Offset of field: AVCodecContext::workaround_bugs"]
        [::std::mem::offset_of!(AVCodecContext, workaround_bugs) - 512usize];
    ["Offset of field: AVCodecContext::strict_std_compliance"]
        [::std::mem::offset_of!(AVCodecContext, strict_std_compliance) - 516usize];
    ["Offset of field: AVCodecContext::error_concealment"]
        [::std::mem::offset_of!(AVCodecContext, error_concealment) - 520usize];
    ["Offset of field: AVCodecContext::debug"]
        [::std::mem::offset_of!(AVCodecContext, debug) - 524usize];
    ["Offset of field: AVCodecContext::err_recognition"]
        [::std::mem::offset_of!(AVCodecContext, err_recognition) - 528usize];
    ["Offset of field: AVCodecContext::hwaccel"]
        [::std::mem::offset_of!(AVCodecContext, hwaccel) - 536usize];
    ["Offset of field: AVCodecContext::hwaccel_context"]
        [::std::mem::offset_of!(AVCodecContext, hwaccel_context) - 544usize];
    ["Offset of field: AVCodecContext::hw_frames_ctx"]
        [::std::mem::offset_of!(AVCodecContext, hw_frames_ctx) - 552usize];
    ["Offset of field: AVCodecContext::hw_device_ctx"]
        [::std::mem::offset_of!(AVCodecContext, hw_device_ctx) - 560usize];
    ["Offset of field: AVCodecContext::hwaccel_flags"]
        [::std::mem::offset_of!(AVCodecContext, hwaccel_flags) - 568usize];
    ["Offset of field: AVCodecContext::extra_hw_frames"]
        [::std::mem::offset_of!(AVCodecContext, extra_hw_frames) - 572usize];
    ["Offset of field: AVCodecContext::error"]
        [::std::mem::offset_of!(AVCodecContext, error) - 576usize];
    ["Offset of field: AVCodecContext::dct_algo"]
        [::std::mem::offset_of!(AVCodecContext, dct_algo) - 640usize];
    ["Offset of field: AVCodecContext::idct_algo"]
        [::std::mem::offset_of!(AVCodecContext, idct_algo) - 644usize];
    ["Offset of field: AVCodecContext::bits_per_coded_sample"]
        [::std::mem::offset_of!(AVCodecContext, bits_per_coded_sample) - 648usize];
    ["Offset of field: AVCodecContext::bits_per_raw_sample"]
        [::std::mem::offset_of!(AVCodecContext, bits_per_raw_sample) - 652usize];
    ["Offset of field: AVCodecContext::thread_count"]
        [::std::mem::offset_of!(AVCodecContext, thread_count) - 656usize];
    ["Offset of field: AVCodecContext::thread_type"]
        [::std::mem::offset_of!(AVCodecContext, thread_type) - 660usize];
    ["Offset of field: AVCodecContext::active_thread_type"]
        [::std::mem::offset_of!(AVCodecContext, active_thread_type) - 664usize];
    ["Offset of field: AVCodecContext::execute"]
        [::std::mem::offset_of!(AVCodecContext, execute) - 672usize];
    ["Offset of field: AVCodecContext::execute2"]
        [::std::mem::offset_of!(AVCodecContext, execute2) - 680usize];
    ["Offset of field: AVCodecContext::profile"]
        [::std::mem::offset_of!(AVCodecContext, profile) - 688usize];
    ["Offset of field: AVCodecContext::level"]
        [::std::mem::offset_of!(AVCodecContext, level) - 692usize];
    ["Offset of field: AVCodecContext::properties"]
        [::std::mem::offset_of!(AVCodecContext, properties) - 696usize];
    ["Offset of field: AVCodecContext::skip_loop_filter"]
        [::std::mem::offset_of!(AVCodecContext, skip_loop_filter) - 700usize];
    ["Offset of field: AVCodecContext::skip_idct"]
        [::std::mem::offset_of!(AVCodecContext, skip_idct) - 704usize];
    ["Offset of field: AVCodecContext::skip_frame"]
        [::std::mem::offset_of!(AVCodecContext, skip_frame) - 708usize];
    ["Offset of field: AVCodecContext::skip_alpha"]
        [::std::mem::offset_of!(AVCodecContext, skip_alpha) - 712usize];
    ["Offset of field: AVCodecContext::skip_top"]
        [::std::mem::offset_of!(AVCodecContext, skip_top) - 716usize];
    ["Offset of field: AVCodecContext::skip_bottom"]
        [::std::mem::offset_of!(AVCodecContext, skip_bottom) - 720usize];
    ["Offset of field: AVCodecContext::lowres"]
        [::std::mem::offset_of!(AVCodecContext, lowres) - 724usize];
    ["Offset of field: AVCodecContext::codec_descriptor"]
        [::std::mem::offset_of!(AVCodecContext, codec_descriptor) - 728usize];
    ["Offset of field: AVCodecContext::sub_charenc"]
        [::std::mem::offset_of!(AVCodecContext, sub_charenc) - 736usize];
    ["Offset of field: AVCodecContext::sub_charenc_mode"]
        [::std::mem::offset_of!(AVCodecContext, sub_charenc_mode) - 744usize];
    ["Offset of field: AVCodecContext::subtitle_header_size"]
        [::std::mem::offset_of!(AVCodecContext, subtitle_header_size) - 748usize];
    ["Offset of field: AVCodecContext::subtitle_header"]
        [::std::mem::offset_of!(AVCodecContext, subtitle_header) - 752usize];
    ["Offset of field: AVCodecContext::dump_separator"]
        [::std::mem::offset_of!(AVCodecContext, dump_separator) - 760usize];
    ["Offset of field: AVCodecContext::codec_whitelist"]
        [::std::mem::offset_of!(AVCodecContext, codec_whitelist) - 768usize];
    ["Offset of field: AVCodecContext::coded_side_data"]
        [::std::mem::offset_of!(AVCodecContext, coded_side_data) - 776usize];
    ["Offset of field: AVCodecContext::nb_coded_side_data"]
        [::std::mem::offset_of!(AVCodecContext, nb_coded_side_data) - 784usize];
    ["Offset of field: AVCodecContext::export_side_data"]
        [::std::mem::offset_of!(AVCodecContext, export_side_data) - 788usize];
    ["Offset of field: AVCodecContext::max_pixels"]
        [::std::mem::offset_of!(AVCodecContext, max_pixels) - 792usize];
    ["Offset of field: AVCodecContext::apply_cropping"]
        [::std::mem::offset_of!(AVCodecContext, apply_cropping) - 800usize];
    ["Offset of field: AVCodecContext::discard_damaged_percentage"]
        [::std::mem::offset_of!(AVCodecContext, discard_damaged_percentage) - 804usize];
    ["Offset of field: AVCodecContext::max_samples"]
        [::std::mem::offset_of!(AVCodecContext, max_samples) - 808usize];
    ["Offset of field: AVCodecContext::get_encode_buffer"]
        [::std::mem::offset_of!(AVCodecContext, get_encode_buffer) - 816usize];
    ["Offset of field: AVCodecContext::frame_num"]
        [::std::mem::offset_of!(AVCodecContext, frame_num) - 824usize];
    ["Offset of field: AVCodecContext::side_data_prefer_packet"]
        [::std::mem::offset_of!(AVCodecContext, side_data_prefer_packet) - 832usize];
    ["Offset of field: AVCodecContext::nb_side_data_prefer_packet"]
        [::std::mem::offset_of!(AVCodecContext, nb_side_data_prefer_packet) - 840usize];
    ["Offset of field: AVCodecContext::decoded_side_data"]
        [::std::mem::offset_of!(AVCodecContext, decoded_side_data) - 848usize];
    ["Offset of field: AVCodecContext::nb_decoded_side_data"]
        [::std::mem::offset_of!(AVCodecContext, nb_decoded_side_data) - 856usize];
    ["Offset of field: AVCodecContext::alpha_mode"]
        [::std::mem::offset_of!(AVCodecContext, alpha_mode) - 860usize];
};
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVHWAccel {
    pub name: *const ::std::os::raw::c_char,
    pub type_: AVMediaType,
    pub id: AVCodecID,
    pub pix_fmt: AVPixelFormat,
    pub capabilities: ::std::os::raw::c_int,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVHWAccel"][::std::mem::size_of::<AVHWAccel>() - 24usize];
    ["Alignment of AVHWAccel"][::std::mem::align_of::<AVHWAccel>() - 8usize];
    ["Offset of field: AVHWAccel::name"][::std::mem::offset_of!(AVHWAccel, name) - 0usize];
    ["Offset of field: AVHWAccel::type_"][::std::mem::offset_of!(AVHWAccel, type_) - 8usize];
    ["Offset of field: AVHWAccel::id"][::std::mem::offset_of!(AVHWAccel, id) - 12usize];
    ["Offset of field: AVHWAccel::pix_fmt"][::std::mem::offset_of!(AVHWAccel, pix_fmt) - 16usize];
    ["Offset of field: AVHWAccel::capabilities"]
        [::std::mem::offset_of!(AVHWAccel, capabilities) - 20usize];
};
pub const AVPictureStructure_AV_PICTURE_STRUCTURE_UNKNOWN: AVPictureStructure = 0;
pub const AVPictureStructure_AV_PICTURE_STRUCTURE_TOP_FIELD: AVPictureStructure = 1;
pub const AVPictureStructure_AV_PICTURE_STRUCTURE_BOTTOM_FIELD: AVPictureStructure = 2;
pub const AVPictureStructure_AV_PICTURE_STRUCTURE_FRAME: AVPictureStructure = 3;
pub type AVPictureStructure = ::std::os::raw::c_int;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVCodecParserContext {
    pub priv_data: *mut ::std::os::raw::c_void,
    pub parser: *const AVCodecParser,
    pub frame_offset: i64,
    pub cur_offset: i64,
    pub next_frame_offset: i64,
    pub pict_type: ::std::os::raw::c_int,
    pub repeat_pict: ::std::os::raw::c_int,
    pub pts: i64,
    pub dts: i64,
    pub last_pts: i64,
    pub last_dts: i64,
    pub fetch_timestamp: ::std::os::raw::c_int,
    pub cur_frame_start_index: ::std::os::raw::c_int,
    pub cur_frame_offset: [i64; 4usize],
    pub cur_frame_pts: [i64; 4usize],
    pub cur_frame_dts: [i64; 4usize],
    pub flags: ::std::os::raw::c_int,
    pub offset: i64,
    pub cur_frame_end: [i64; 4usize],
    pub key_frame: ::std::os::raw::c_int,
    pub dts_sync_point: ::std::os::raw::c_int,
    pub dts_ref_dts_delta: ::std::os::raw::c_int,
    pub pts_dts_delta: ::std::os::raw::c_int,
    pub cur_frame_pos: [i64; 4usize],
    pub pos: i64,
    pub last_pos: i64,
    pub duration: ::std::os::raw::c_int,
    pub field_order: AVFieldOrder,
    pub picture_structure: AVPictureStructure,
    pub output_picture_number: ::std::os::raw::c_int,
    pub width: ::std::os::raw::c_int,
    pub height: ::std::os::raw::c_int,
    pub coded_width: ::std::os::raw::c_int,
    pub coded_height: ::std::os::raw::c_int,
    pub format: ::std::os::raw::c_int,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVCodecParserContext"][::std::mem::size_of::<AVCodecParserContext>() - 336usize];
    ["Alignment of AVCodecParserContext"][::std::mem::align_of::<AVCodecParserContext>() - 8usize];
    ["Offset of field: AVCodecParserContext::priv_data"]
        [::std::mem::offset_of!(AVCodecParserContext, priv_data) - 0usize];
    ["Offset of field: AVCodecParserContext::parser"]
        [::std::mem::offset_of!(AVCodecParserContext, parser) - 8usize];
    ["Offset of field: AVCodecParserContext::frame_offset"]
        [::std::mem::offset_of!(AVCodecParserContext, frame_offset) - 16usize];
    ["Offset of field: AVCodecParserContext::cur_offset"]
        [::std::mem::offset_of!(AVCodecParserContext, cur_offset) - 24usize];
    ["Offset of field: AVCodecParserContext::next_frame_offset"]
        [::std::mem::offset_of!(AVCodecParserContext, next_frame_offset) - 32usize];
    ["Offset of field: AVCodecParserContext::pict_type"]
        [::std::mem::offset_of!(AVCodecParserContext, pict_type) - 40usize];
    ["Offset of field: AVCodecParserContext::repeat_pict"]
        [::std::mem::offset_of!(AVCodecParserContext, repeat_pict) - 44usize];
    ["Offset of field: AVCodecParserContext::pts"]
        [::std::mem::offset_of!(AVCodecParserContext, pts) - 48usize];
    ["Offset of field: AVCodecParserContext::dts"]
        [::std::mem::offset_of!(AVCodecParserContext, dts) - 56usize];
    ["Offset of field: AVCodecParserContext::last_pts"]
        [::std::mem::offset_of!(AVCodecParserContext, last_pts) - 64usize];
    ["Offset of field: AVCodecParserContext::last_dts"]
        [::std::mem::offset_of!(AVCodecParserContext, last_dts) - 72usize];
    ["Offset of field: AVCodecParserContext::fetch_timestamp"]
        [::std::mem::offset_of!(AVCodecParserContext, fetch_timestamp) - 80usize];
    ["Offset of field: AVCodecParserContext::cur_frame_start_index"]
        [::std::mem::offset_of!(AVCodecParserContext, cur_frame_start_index) - 84usize];
    ["Offset of field: AVCodecParserContext::cur_frame_offset"]
        [::std::mem::offset_of!(AVCodecParserContext, cur_frame_offset) - 88usize];
    ["Offset of field: AVCodecParserContext::cur_frame_pts"]
        [::std::mem::offset_of!(AVCodecParserContext, cur_frame_pts) - 120usize];
    ["Offset of field: AVCodecParserContext::cur_frame_dts"]
        [::std::mem::offset_of!(AVCodecParserContext, cur_frame_dts) - 152usize];
    ["Offset of field: AVCodecParserContext::flags"]
        [::std::mem::offset_of!(AVCodecParserContext, flags) - 184usize];
    ["Offset of field: AVCodecParserContext::offset"]
        [::std::mem::offset_of!(AVCodecParserContext, offset) - 192usize];
    ["Offset of field: AVCodecParserContext::cur_frame_end"]
        [::std::mem::offset_of!(AVCodecParserContext, cur_frame_end) - 200usize];
    ["Offset of field: AVCodecParserContext::key_frame"]
        [::std::mem::offset_of!(AVCodecParserContext, key_frame) - 232usize];
    ["Offset of field: AVCodecParserContext::dts_sync_point"]
        [::std::mem::offset_of!(AVCodecParserContext, dts_sync_point) - 236usize];
    ["Offset of field: AVCodecParserContext::dts_ref_dts_delta"]
        [::std::mem::offset_of!(AVCodecParserContext, dts_ref_dts_delta) - 240usize];
    ["Offset of field: AVCodecParserContext::pts_dts_delta"]
        [::std::mem::offset_of!(AVCodecParserContext, pts_dts_delta) - 244usize];
    ["Offset of field: AVCodecParserContext::cur_frame_pos"]
        [::std::mem::offset_of!(AVCodecParserContext, cur_frame_pos) - 248usize];
    ["Offset of field: AVCodecParserContext::pos"]
        [::std::mem::offset_of!(AVCodecParserContext, pos) - 280usize];
    ["Offset of field: AVCodecParserContext::last_pos"]
        [::std::mem::offset_of!(AVCodecParserContext, last_pos) - 288usize];
    ["Offset of field: AVCodecParserContext::duration"]
        [::std::mem::offset_of!(AVCodecParserContext, duration) - 296usize];
    ["Offset of field: AVCodecParserContext::field_order"]
        [::std::mem::offset_of!(AVCodecParserContext, field_order) - 300usize];
    ["Offset of field: AVCodecParserContext::picture_structure"]
        [::std::mem::offset_of!(AVCodecParserContext, picture_structure) - 304usize];
    ["Offset of field: AVCodecParserContext::output_picture_number"]
        [::std::mem::offset_of!(AVCodecParserContext, output_picture_number) - 308usize];
    ["Offset of field: AVCodecParserContext::width"]
        [::std::mem::offset_of!(AVCodecParserContext, width) - 312usize];
    ["Offset of field: AVCodecParserContext::height"]
        [::std::mem::offset_of!(AVCodecParserContext, height) - 316usize];
    ["Offset of field: AVCodecParserContext::coded_width"]
        [::std::mem::offset_of!(AVCodecParserContext, coded_width) - 320usize];
    ["Offset of field: AVCodecParserContext::coded_height"]
        [::std::mem::offset_of!(AVCodecParserContext, coded_height) - 324usize];
    ["Offset of field: AVCodecParserContext::format"]
        [::std::mem::offset_of!(AVCodecParserContext, format) - 328usize];
};
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVCodecParser {
    pub codec_ids: [::std::os::raw::c_int; 7usize],
    pub priv_data_size: ::std::os::raw::c_int,
    pub parser_init: ::std::option::Option<
        unsafe extern "C" fn(s: *mut AVCodecParserContext) -> ::std::os::raw::c_int,
    >,
    pub parser_parse: ::std::option::Option<
        unsafe extern "C" fn(
            s: *mut AVCodecParserContext,
            avctx: *mut AVCodecContext,
            poutbuf: *mut *const u8,
            poutbuf_size: *mut ::std::os::raw::c_int,
            buf: *const u8,
            buf_size: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int,
    >,
    pub parser_close: ::std::option::Option<unsafe extern "C" fn(s: *mut AVCodecParserContext)>,
    pub split: ::std::option::Option<
        unsafe extern "C" fn(
            avctx: *mut AVCodecContext,
            buf: *const u8,
            buf_size: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int,
    >,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVCodecParser"][::std::mem::size_of::<AVCodecParser>() - 64usize];
    ["Alignment of AVCodecParser"][::std::mem::align_of::<AVCodecParser>() - 8usize];
    ["Offset of field: AVCodecParser::codec_ids"]
        [::std::mem::offset_of!(AVCodecParser, codec_ids) - 0usize];
    ["Offset of field: AVCodecParser::priv_data_size"]
        [::std::mem::offset_of!(AVCodecParser, priv_data_size) - 28usize];
    ["Offset of field: AVCodecParser::parser_init"]
        [::std::mem::offset_of!(AVCodecParser, parser_init) - 32usize];
    ["Offset of field: AVCodecParser::parser_parse"]
        [::std::mem::offset_of!(AVCodecParser, parser_parse) - 40usize];
    ["Offset of field: AVCodecParser::parser_close"]
        [::std::mem::offset_of!(AVCodecParser, parser_close) - 48usize];
    ["Offset of field: AVCodecParser::split"]
        [::std::mem::offset_of!(AVCodecParser, split) - 56usize];
};
unsafe extern "C" {
    pub fn av_parser_iterate(opaque: *mut *mut ::std::os::raw::c_void) -> *const AVCodecParser;
}
unsafe extern "C" {
    pub fn av_parser_init(codec_id: ::std::os::raw::c_int) -> *mut AVCodecParserContext;
}
unsafe extern "C" {
    pub fn av_parser_parse2(
        s: *mut AVCodecParserContext,
        avctx: *mut AVCodecContext,
        poutbuf: *mut *mut u8,
        poutbuf_size: *mut ::std::os::raw::c_int,
        buf: *const u8,
        buf_size: ::std::os::raw::c_int,
        pts: i64,
        dts: i64,
        pos: i64,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_parser_close(s: *mut AVCodecParserContext);
}
unsafe extern "C" {
    pub fn av_get_audio_frame_duration(
        avctx: *mut AVCodecContext,
        frame_bytes: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_fast_padded_malloc(
        ptr: *mut ::std::os::raw::c_void,
        size: *mut ::std::os::raw::c_uint,
        min_size: usize,
    );
}
unsafe extern "C" {
    pub fn av_fast_padded_mallocz(
        ptr: *mut ::std::os::raw::c_void,
        size: *mut ::std::os::raw::c_uint,
        min_size: usize,
    );
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVIOInterruptCB {
    pub callback: ::std::option::Option<
        unsafe extern "C" fn(arg1: *mut ::std::os::raw::c_void) -> ::std::os::raw::c_int,
    >,
    pub opaque: *mut ::std::os::raw::c_void,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVIOInterruptCB"][::std::mem::size_of::<AVIOInterruptCB>() - 16usize];
    ["Alignment of AVIOInterruptCB"][::std::mem::align_of::<AVIOInterruptCB>() - 8usize];
    ["Offset of field: AVIOInterruptCB::callback"]
        [::std::mem::offset_of!(AVIOInterruptCB, callback) - 0usize];
    ["Offset of field: AVIOInterruptCB::opaque"]
        [::std::mem::offset_of!(AVIOInterruptCB, opaque) - 8usize];
};
pub const AVIODataMarkerType_AVIO_DATA_MARKER_HEADER: AVIODataMarkerType = 0;
pub const AVIODataMarkerType_AVIO_DATA_MARKER_SYNC_POINT: AVIODataMarkerType = 1;
pub const AVIODataMarkerType_AVIO_DATA_MARKER_BOUNDARY_POINT: AVIODataMarkerType = 2;
pub const AVIODataMarkerType_AVIO_DATA_MARKER_UNKNOWN: AVIODataMarkerType = 3;
pub const AVIODataMarkerType_AVIO_DATA_MARKER_TRAILER: AVIODataMarkerType = 4;
pub const AVIODataMarkerType_AVIO_DATA_MARKER_FLUSH_POINT: AVIODataMarkerType = 5;
pub type AVIODataMarkerType = ::std::os::raw::c_int;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVIOContext {
    pub av_class: *const AVClass,
    pub buffer: *mut ::std::os::raw::c_uchar,
    pub buffer_size: ::std::os::raw::c_int,
    pub buf_ptr: *mut ::std::os::raw::c_uchar,
    pub buf_end: *mut ::std::os::raw::c_uchar,
    pub opaque: *mut ::std::os::raw::c_void,
    pub read_packet: ::std::option::Option<
        unsafe extern "C" fn(
            opaque: *mut ::std::os::raw::c_void,
            buf: *mut u8,
            buf_size: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int,
    >,
    pub write_packet: ::std::option::Option<
        unsafe extern "C" fn(
            opaque: *mut ::std::os::raw::c_void,
            buf: *const u8,
            buf_size: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int,
    >,
    pub seek: ::std::option::Option<
        unsafe extern "C" fn(
            opaque: *mut ::std::os::raw::c_void,
            offset: i64,
            whence: ::std::os::raw::c_int,
        ) -> i64,
    >,
    pub pos: i64,
    pub eof_reached: ::std::os::raw::c_int,
    pub error: ::std::os::raw::c_int,
    pub write_flag: ::std::os::raw::c_int,
    pub max_packet_size: ::std::os::raw::c_int,
    pub min_packet_size: ::std::os::raw::c_int,
    pub checksum: ::std::os::raw::c_ulong,
    pub checksum_ptr: *mut ::std::os::raw::c_uchar,
    pub update_checksum: ::std::option::Option<
        unsafe extern "C" fn(
            checksum: ::std::os::raw::c_ulong,
            buf: *const u8,
            size: ::std::os::raw::c_uint,
        ) -> ::std::os::raw::c_ulong,
    >,
    pub read_pause: ::std::option::Option<
        unsafe extern "C" fn(
            opaque: *mut ::std::os::raw::c_void,
            pause: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int,
    >,
    pub read_seek: ::std::option::Option<
        unsafe extern "C" fn(
            opaque: *mut ::std::os::raw::c_void,
            stream_index: ::std::os::raw::c_int,
            timestamp: i64,
            flags: ::std::os::raw::c_int,
        ) -> i64,
    >,
    pub seekable: ::std::os::raw::c_int,
    pub direct: ::std::os::raw::c_int,
    pub protocol_whitelist: *const ::std::os::raw::c_char,
    pub protocol_blacklist: *const ::std::os::raw::c_char,
    pub write_data_type: ::std::option::Option<
        unsafe extern "C" fn(
            opaque: *mut ::std::os::raw::c_void,
            buf: *const u8,
            buf_size: ::std::os::raw::c_int,
            type_: AVIODataMarkerType,
            time: i64,
        ) -> ::std::os::raw::c_int,
    >,
    pub ignore_boundary_point: ::std::os::raw::c_int,
    pub buf_ptr_max: *mut ::std::os::raw::c_uchar,
    pub bytes_read: i64,
    pub bytes_written: i64,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVIOContext"][::std::mem::size_of::<AVIOContext>() - 200usize];
    ["Alignment of AVIOContext"][::std::mem::align_of::<AVIOContext>() - 8usize];
    ["Offset of field: AVIOContext::av_class"]
        [::std::mem::offset_of!(AVIOContext, av_class) - 0usize];
    ["Offset of field: AVIOContext::buffer"][::std::mem::offset_of!(AVIOContext, buffer) - 8usize];
    ["Offset of field: AVIOContext::buffer_size"]
        [::std::mem::offset_of!(AVIOContext, buffer_size) - 16usize];
    ["Offset of field: AVIOContext::buf_ptr"]
        [::std::mem::offset_of!(AVIOContext, buf_ptr) - 24usize];
    ["Offset of field: AVIOContext::buf_end"]
        [::std::mem::offset_of!(AVIOContext, buf_end) - 32usize];
    ["Offset of field: AVIOContext::opaque"][::std::mem::offset_of!(AVIOContext, opaque) - 40usize];
    ["Offset of field: AVIOContext::read_packet"]
        [::std::mem::offset_of!(AVIOContext, read_packet) - 48usize];
    ["Offset of field: AVIOContext::write_packet"]
        [::std::mem::offset_of!(AVIOContext, write_packet) - 56usize];
    ["Offset of field: AVIOContext::seek"][::std::mem::offset_of!(AVIOContext, seek) - 64usize];
    ["Offset of field: AVIOContext::pos"][::std::mem::offset_of!(AVIOContext, pos) - 72usize];
    ["Offset of field: AVIOContext::eof_reached"]
        [::std::mem::offset_of!(AVIOContext, eof_reached) - 80usize];
    ["Offset of field: AVIOContext::error"][::std::mem::offset_of!(AVIOContext, error) - 84usize];
    ["Offset of field: AVIOContext::write_flag"]
        [::std::mem::offset_of!(AVIOContext, write_flag) - 88usize];
    ["Offset of field: AVIOContext::max_packet_size"]
        [::std::mem::offset_of!(AVIOContext, max_packet_size) - 92usize];
    ["Offset of field: AVIOContext::min_packet_size"]
        [::std::mem::offset_of!(AVIOContext, min_packet_size) - 96usize];
    ["Offset of field: AVIOContext::checksum"]
        [::std::mem::offset_of!(AVIOContext, checksum) - 100usize];
    ["Offset of field: AVIOContext::checksum_ptr"]
        [::std::mem::offset_of!(AVIOContext, checksum_ptr) - 104usize];
    ["Offset of field: AVIOContext::update_checksum"]
        [::std::mem::offset_of!(AVIOContext, update_checksum) - 112usize];
    ["Offset of field: AVIOContext::read_pause"]
        [::std::mem::offset_of!(AVIOContext, read_pause) - 120usize];
    ["Offset of field: AVIOContext::read_seek"]
        [::std::mem::offset_of!(AVIOContext, read_seek) - 128usize];
    ["Offset of field: AVIOContext::seekable"]
        [::std::mem::offset_of!(AVIOContext, seekable) - 136usize];
    ["Offset of field: AVIOContext::direct"]
        [::std::mem::offset_of!(AVIOContext, direct) - 140usize];
    ["Offset of field: AVIOContext::protocol_whitelist"]
        [::std::mem::offset_of!(AVIOContext, protocol_whitelist) - 144usize];
    ["Offset of field: AVIOContext::protocol_blacklist"]
        [::std::mem::offset_of!(AVIOContext, protocol_blacklist) - 152usize];
    ["Offset of field: AVIOContext::write_data_type"]
        [::std::mem::offset_of!(AVIOContext, write_data_type) - 160usize];
    ["Offset of field: AVIOContext::ignore_boundary_point"]
        [::std::mem::offset_of!(AVIOContext, ignore_boundary_point) - 168usize];
    ["Offset of field: AVIOContext::buf_ptr_max"]
        [::std::mem::offset_of!(AVIOContext, buf_ptr_max) - 176usize];
    ["Offset of field: AVIOContext::bytes_read"]
        [::std::mem::offset_of!(AVIOContext, bytes_read) - 184usize];
    ["Offset of field: AVIOContext::bytes_written"]
        [::std::mem::offset_of!(AVIOContext, bytes_written) - 192usize];
};
unsafe extern "C" {
    pub fn av_get_packet(
        s: *mut AVIOContext,
        pkt: *mut AVPacket,
        size: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_append_packet(
        s: *mut AVIOContext,
        pkt: *mut AVPacket,
        size: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
#[repr(C)]
#[derive(Debug)]
pub struct AVCodecTag {
    _unused: [u8; 0],
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVProbeData {
    pub filename: *const ::std::os::raw::c_char,
    pub buf: *mut ::std::os::raw::c_uchar,
    pub buf_size: ::std::os::raw::c_int,
    pub mime_type: *const ::std::os::raw::c_char,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVProbeData"][::std::mem::size_of::<AVProbeData>() - 32usize];
    ["Alignment of AVProbeData"][::std::mem::align_of::<AVProbeData>() - 8usize];
    ["Offset of field: AVProbeData::filename"]
        [::std::mem::offset_of!(AVProbeData, filename) - 0usize];
    ["Offset of field: AVProbeData::buf"][::std::mem::offset_of!(AVProbeData, buf) - 8usize];
    ["Offset of field: AVProbeData::buf_size"]
        [::std::mem::offset_of!(AVProbeData, buf_size) - 16usize];
    ["Offset of field: AVProbeData::mime_type"]
        [::std::mem::offset_of!(AVProbeData, mime_type) - 24usize];
};
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVOutputFormat {
    pub name: *const ::std::os::raw::c_char,
    pub long_name: *const ::std::os::raw::c_char,
    pub mime_type: *const ::std::os::raw::c_char,
    pub extensions: *const ::std::os::raw::c_char,
    pub audio_codec: AVCodecID,
    pub video_codec: AVCodecID,
    pub subtitle_codec: AVCodecID,
    pub flags: ::std::os::raw::c_int,
    pub codec_tag: *const *const AVCodecTag,
    pub priv_class: *const AVClass,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVOutputFormat"][::std::mem::size_of::<AVOutputFormat>() - 64usize];
    ["Alignment of AVOutputFormat"][::std::mem::align_of::<AVOutputFormat>() - 8usize];
    ["Offset of field: AVOutputFormat::name"]
        [::std::mem::offset_of!(AVOutputFormat, name) - 0usize];
    ["Offset of field: AVOutputFormat::long_name"]
        [::std::mem::offset_of!(AVOutputFormat, long_name) - 8usize];
    ["Offset of field: AVOutputFormat::mime_type"]
        [::std::mem::offset_of!(AVOutputFormat, mime_type) - 16usize];
    ["Offset of field: AVOutputFormat::extensions"]
        [::std::mem::offset_of!(AVOutputFormat, extensions) - 24usize];
    ["Offset of field: AVOutputFormat::audio_codec"]
        [::std::mem::offset_of!(AVOutputFormat, audio_codec) - 32usize];
    ["Offset of field: AVOutputFormat::video_codec"]
        [::std::mem::offset_of!(AVOutputFormat, video_codec) - 36usize];
    ["Offset of field: AVOutputFormat::subtitle_codec"]
        [::std::mem::offset_of!(AVOutputFormat, subtitle_codec) - 40usize];
    ["Offset of field: AVOutputFormat::flags"]
        [::std::mem::offset_of!(AVOutputFormat, flags) - 44usize];
    ["Offset of field: AVOutputFormat::codec_tag"]
        [::std::mem::offset_of!(AVOutputFormat, codec_tag) - 48usize];
    ["Offset of field: AVOutputFormat::priv_class"]
        [::std::mem::offset_of!(AVOutputFormat, priv_class) - 56usize];
};
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVInputFormat {
    pub name: *const ::std::os::raw::c_char,
    pub long_name: *const ::std::os::raw::c_char,
    pub flags: ::std::os::raw::c_int,
    pub extensions: *const ::std::os::raw::c_char,
    pub codec_tag: *const *const AVCodecTag,
    pub priv_class: *const AVClass,
    pub mime_type: *const ::std::os::raw::c_char,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVInputFormat"][::std::mem::size_of::<AVInputFormat>() - 56usize];
    ["Alignment of AVInputFormat"][::std::mem::align_of::<AVInputFormat>() - 8usize];
    ["Offset of field: AVInputFormat::name"][::std::mem::offset_of!(AVInputFormat, name) - 0usize];
    ["Offset of field: AVInputFormat::long_name"]
        [::std::mem::offset_of!(AVInputFormat, long_name) - 8usize];
    ["Offset of field: AVInputFormat::flags"]
        [::std::mem::offset_of!(AVInputFormat, flags) - 16usize];
    ["Offset of field: AVInputFormat::extensions"]
        [::std::mem::offset_of!(AVInputFormat, extensions) - 24usize];
    ["Offset of field: AVInputFormat::codec_tag"]
        [::std::mem::offset_of!(AVInputFormat, codec_tag) - 32usize];
    ["Offset of field: AVInputFormat::priv_class"]
        [::std::mem::offset_of!(AVInputFormat, priv_class) - 40usize];
    ["Offset of field: AVInputFormat::mime_type"]
        [::std::mem::offset_of!(AVInputFormat, mime_type) - 48usize];
};
unsafe extern "C" {
    pub fn av_disposition_from_string(disp: *const ::std::os::raw::c_char)
        -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_disposition_to_string(
        disposition: ::std::os::raw::c_int,
    ) -> *const ::std::os::raw::c_char;
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVStream {
    pub av_class: *const AVClass,
    pub index: ::std::os::raw::c_int,
    pub id: ::std::os::raw::c_int,
    pub codecpar: *mut AVCodecParameters,
    pub priv_data: *mut ::std::os::raw::c_void,
    pub time_base: AVRational,
    pub start_time: i64,
    pub duration: i64,
    pub nb_frames: i64,
    pub disposition: ::std::os::raw::c_int,
    pub discard: AVDiscard,
    pub sample_aspect_ratio: AVRational,
    pub metadata: *mut AVDictionary,
    pub avg_frame_rate: AVRational,
    pub attached_pic: AVPacket,
    pub event_flags: ::std::os::raw::c_int,
    pub r_frame_rate: AVRational,
    pub pts_wrap_bits: ::std::os::raw::c_int,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVStream"][::std::mem::size_of::<AVStream>() - 216usize];
    ["Alignment of AVStream"][::std::mem::align_of::<AVStream>() - 8usize];
    ["Offset of field: AVStream::av_class"][::std::mem::offset_of!(AVStream, av_class) - 0usize];
    ["Offset of field: AVStream::index"][::std::mem::offset_of!(AVStream, index) - 8usize];
    ["Offset of field: AVStream::id"][::std::mem::offset_of!(AVStream, id) - 12usize];
    ["Offset of field: AVStream::codecpar"][::std::mem::offset_of!(AVStream, codecpar) - 16usize];
    ["Offset of field: AVStream::priv_data"][::std::mem::offset_of!(AVStream, priv_data) - 24usize];
    ["Offset of field: AVStream::time_base"][::std::mem::offset_of!(AVStream, time_base) - 32usize];
    ["Offset of field: AVStream::start_time"]
        [::std::mem::offset_of!(AVStream, start_time) - 40usize];
    ["Offset of field: AVStream::duration"][::std::mem::offset_of!(AVStream, duration) - 48usize];
    ["Offset of field: AVStream::nb_frames"][::std::mem::offset_of!(AVStream, nb_frames) - 56usize];
    ["Offset of field: AVStream::disposition"]
        [::std::mem::offset_of!(AVStream, disposition) - 64usize];
    ["Offset of field: AVStream::discard"][::std::mem::offset_of!(AVStream, discard) - 68usize];
    ["Offset of field: AVStream::sample_aspect_ratio"]
        [::std::mem::offset_of!(AVStream, sample_aspect_ratio) - 72usize];
    ["Offset of field: AVStream::metadata"][::std::mem::offset_of!(AVStream, metadata) - 80usize];
    ["Offset of field: AVStream::avg_frame_rate"]
        [::std::mem::offset_of!(AVStream, avg_frame_rate) - 88usize];
    ["Offset of field: AVStream::attached_pic"]
        [::std::mem::offset_of!(AVStream, attached_pic) - 96usize];
    ["Offset of field: AVStream::event_flags"]
        [::std::mem::offset_of!(AVStream, event_flags) - 200usize];
    ["Offset of field: AVStream::r_frame_rate"]
        [::std::mem::offset_of!(AVStream, r_frame_rate) - 204usize];
    ["Offset of field: AVStream::pts_wrap_bits"]
        [::std::mem::offset_of!(AVStream, pts_wrap_bits) - 212usize];
};
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVStreamGroupTileGrid {
    pub av_class: *const AVClass,
    pub nb_tiles: ::std::os::raw::c_uint,
    pub coded_width: ::std::os::raw::c_int,
    pub coded_height: ::std::os::raw::c_int,
    pub offsets: *mut AVStreamGroupTileGrid__bindgen_ty_1,
    pub background: [u8; 4usize],
    pub horizontal_offset: ::std::os::raw::c_int,
    pub vertical_offset: ::std::os::raw::c_int,
    pub width: ::std::os::raw::c_int,
    pub height: ::std::os::raw::c_int,
    pub coded_side_data: *mut AVPacketSideData,
    pub nb_coded_side_data: ::std::os::raw::c_int,
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVStreamGroupTileGrid__bindgen_ty_1 {
    pub idx: ::std::os::raw::c_uint,
    pub horizontal: ::std::os::raw::c_int,
    pub vertical: ::std::os::raw::c_int,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVStreamGroupTileGrid__bindgen_ty_1"]
        [::std::mem::size_of::<AVStreamGroupTileGrid__bindgen_ty_1>() - 12usize];
    ["Alignment of AVStreamGroupTileGrid__bindgen_ty_1"]
        [::std::mem::align_of::<AVStreamGroupTileGrid__bindgen_ty_1>() - 4usize];
    ["Offset of field: AVStreamGroupTileGrid__bindgen_ty_1::idx"]
        [::std::mem::offset_of!(AVStreamGroupTileGrid__bindgen_ty_1, idx) - 0usize];
    ["Offset of field: AVStreamGroupTileGrid__bindgen_ty_1::horizontal"]
        [::std::mem::offset_of!(AVStreamGroupTileGrid__bindgen_ty_1, horizontal) - 4usize];
    ["Offset of field: AVStreamGroupTileGrid__bindgen_ty_1::vertical"]
        [::std::mem::offset_of!(AVStreamGroupTileGrid__bindgen_ty_1, vertical) - 8usize];
};
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVStreamGroupTileGrid"][::std::mem::size_of::<AVStreamGroupTileGrid>() - 72usize];
    ["Alignment of AVStreamGroupTileGrid"]
        [::std::mem::align_of::<AVStreamGroupTileGrid>() - 8usize];
    ["Offset of field: AVStreamGroupTileGrid::av_class"]
        [::std::mem::offset_of!(AVStreamGroupTileGrid, av_class) - 0usize];
    ["Offset of field: AVStreamGroupTileGrid::nb_tiles"]
        [::std::mem::offset_of!(AVStreamGroupTileGrid, nb_tiles) - 8usize];
    ["Offset of field: AVStreamGroupTileGrid::coded_width"]
        [::std::mem::offset_of!(AVStreamGroupTileGrid, coded_width) - 12usize];
    ["Offset of field: AVStreamGroupTileGrid::coded_height"]
        [::std::mem::offset_of!(AVStreamGroupTileGrid, coded_height) - 16usize];
    ["Offset of field: AVStreamGroupTileGrid::offsets"]
        [::std::mem::offset_of!(AVStreamGroupTileGrid, offsets) - 24usize];
    ["Offset of field: AVStreamGroupTileGrid::background"]
        [::std::mem::offset_of!(AVStreamGroupTileGrid, background) - 32usize];
    ["Offset of field: AVStreamGroupTileGrid::horizontal_offset"]
        [::std::mem::offset_of!(AVStreamGroupTileGrid, horizontal_offset) - 36usize];
    ["Offset of field: AVStreamGroupTileGrid::vertical_offset"]
        [::std::mem::offset_of!(AVStreamGroupTileGrid, vertical_offset) - 40usize];
    ["Offset of field: AVStreamGroupTileGrid::width"]
        [::std::mem::offset_of!(AVStreamGroupTileGrid, width) - 44usize];
    ["Offset of field: AVStreamGroupTileGrid::height"]
        [::std::mem::offset_of!(AVStreamGroupTileGrid, height) - 48usize];
    ["Offset of field: AVStreamGroupTileGrid::coded_side_data"]
        [::std::mem::offset_of!(AVStreamGroupTileGrid, coded_side_data) - 56usize];
    ["Offset of field: AVStreamGroupTileGrid::nb_coded_side_data"]
        [::std::mem::offset_of!(AVStreamGroupTileGrid, nb_coded_side_data) - 64usize];
};
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVStreamGroupLCEVC {
    pub av_class: *const AVClass,
    pub lcevc_index: ::std::os::raw::c_uint,
    pub width: ::std::os::raw::c_int,
    pub height: ::std::os::raw::c_int,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVStreamGroupLCEVC"][::std::mem::size_of::<AVStreamGroupLCEVC>() - 24usize];
    ["Alignment of AVStreamGroupLCEVC"][::std::mem::align_of::<AVStreamGroupLCEVC>() - 8usize];
    ["Offset of field: AVStreamGroupLCEVC::av_class"]
        [::std::mem::offset_of!(AVStreamGroupLCEVC, av_class) - 0usize];
    ["Offset of field: AVStreamGroupLCEVC::lcevc_index"]
        [::std::mem::offset_of!(AVStreamGroupLCEVC, lcevc_index) - 8usize];
    ["Offset of field: AVStreamGroupLCEVC::width"]
        [::std::mem::offset_of!(AVStreamGroupLCEVC, width) - 12usize];
    ["Offset of field: AVStreamGroupLCEVC::height"]
        [::std::mem::offset_of!(AVStreamGroupLCEVC, height) - 16usize];
};
pub const AVStreamGroupParamsType_AV_STREAM_GROUP_PARAMS_NONE: AVStreamGroupParamsType = 0;
pub const AVStreamGroupParamsType_AV_STREAM_GROUP_PARAMS_IAMF_AUDIO_ELEMENT:
    AVStreamGroupParamsType = 1;
pub const AVStreamGroupParamsType_AV_STREAM_GROUP_PARAMS_IAMF_MIX_PRESENTATION:
    AVStreamGroupParamsType = 2;
pub const AVStreamGroupParamsType_AV_STREAM_GROUP_PARAMS_TILE_GRID: AVStreamGroupParamsType = 3;
pub const AVStreamGroupParamsType_AV_STREAM_GROUP_PARAMS_LCEVC: AVStreamGroupParamsType = 4;
pub type AVStreamGroupParamsType = ::std::os::raw::c_int;
#[repr(C)]
#[derive(Debug)]
pub struct AVIAMFAudioElement {
    _unused: [u8; 0],
}
#[repr(C)]
#[derive(Debug)]
pub struct AVIAMFMixPresentation {
    _unused: [u8; 0],
}
#[repr(C)]
#[derive(Copy, Clone)]
pub struct AVStreamGroup {
    pub av_class: *const AVClass,
    pub priv_data: *mut ::std::os::raw::c_void,
    pub index: ::std::os::raw::c_uint,
    pub id: i64,
    pub type_: AVStreamGroupParamsType,
    pub params: AVStreamGroup__bindgen_ty_1,
    pub metadata: *mut AVDictionary,
    pub nb_streams: ::std::os::raw::c_uint,
    pub streams: *mut *mut AVStream,
    pub disposition: ::std::os::raw::c_int,
}
#[repr(C)]
#[derive(Copy, Clone)]
pub union AVStreamGroup__bindgen_ty_1 {
    pub iamf_audio_element: *mut AVIAMFAudioElement,
    pub iamf_mix_presentation: *mut AVIAMFMixPresentation,
    pub tile_grid: *mut AVStreamGroupTileGrid,
    pub lcevc: *mut AVStreamGroupLCEVC,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVStreamGroup__bindgen_ty_1"]
        [::std::mem::size_of::<AVStreamGroup__bindgen_ty_1>() - 8usize];
    ["Alignment of AVStreamGroup__bindgen_ty_1"]
        [::std::mem::align_of::<AVStreamGroup__bindgen_ty_1>() - 8usize];
    ["Offset of field: AVStreamGroup__bindgen_ty_1::iamf_audio_element"]
        [::std::mem::offset_of!(AVStreamGroup__bindgen_ty_1, iamf_audio_element) - 0usize];
    ["Offset of field: AVStreamGroup__bindgen_ty_1::iamf_mix_presentation"]
        [::std::mem::offset_of!(AVStreamGroup__bindgen_ty_1, iamf_mix_presentation) - 0usize];
    ["Offset of field: AVStreamGroup__bindgen_ty_1::tile_grid"]
        [::std::mem::offset_of!(AVStreamGroup__bindgen_ty_1, tile_grid) - 0usize];
    ["Offset of field: AVStreamGroup__bindgen_ty_1::lcevc"]
        [::std::mem::offset_of!(AVStreamGroup__bindgen_ty_1, lcevc) - 0usize];
};
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVStreamGroup"][::std::mem::size_of::<AVStreamGroup>() - 80usize];
    ["Alignment of AVStreamGroup"][::std::mem::align_of::<AVStreamGroup>() - 8usize];
    ["Offset of field: AVStreamGroup::av_class"]
        [::std::mem::offset_of!(AVStreamGroup, av_class) - 0usize];
    ["Offset of field: AVStreamGroup::priv_data"]
        [::std::mem::offset_of!(AVStreamGroup, priv_data) - 8usize];
    ["Offset of field: AVStreamGroup::index"]
        [::std::mem::offset_of!(AVStreamGroup, index) - 16usize];
    ["Offset of field: AVStreamGroup::id"][::std::mem::offset_of!(AVStreamGroup, id) - 24usize];
    ["Offset of field: AVStreamGroup::type_"]
        [::std::mem::offset_of!(AVStreamGroup, type_) - 32usize];
    ["Offset of field: AVStreamGroup::params"]
        [::std::mem::offset_of!(AVStreamGroup, params) - 40usize];
    ["Offset of field: AVStreamGroup::metadata"]
        [::std::mem::offset_of!(AVStreamGroup, metadata) - 48usize];
    ["Offset of field: AVStreamGroup::nb_streams"]
        [::std::mem::offset_of!(AVStreamGroup, nb_streams) - 56usize];
    ["Offset of field: AVStreamGroup::streams"]
        [::std::mem::offset_of!(AVStreamGroup, streams) - 64usize];
    ["Offset of field: AVStreamGroup::disposition"]
        [::std::mem::offset_of!(AVStreamGroup, disposition) - 72usize];
};
unsafe extern "C" {
    pub fn av_stream_get_parser(s: *const AVStream) -> *mut AVCodecParserContext;
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVProgram {
    pub id: ::std::os::raw::c_int,
    pub flags: ::std::os::raw::c_int,
    pub discard: AVDiscard,
    pub stream_index: *mut ::std::os::raw::c_uint,
    pub nb_stream_indexes: ::std::os::raw::c_uint,
    pub metadata: *mut AVDictionary,
    pub program_num: ::std::os::raw::c_int,
    pub pmt_pid: ::std::os::raw::c_int,
    pub pcr_pid: ::std::os::raw::c_int,
    pub pmt_version: ::std::os::raw::c_int,
    pub start_time: i64,
    pub end_time: i64,
    pub pts_wrap_reference: i64,
    pub pts_wrap_behavior: ::std::os::raw::c_int,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVProgram"][::std::mem::size_of::<AVProgram>() - 88usize];
    ["Alignment of AVProgram"][::std::mem::align_of::<AVProgram>() - 8usize];
    ["Offset of field: AVProgram::id"][::std::mem::offset_of!(AVProgram, id) - 0usize];
    ["Offset of field: AVProgram::flags"][::std::mem::offset_of!(AVProgram, flags) - 4usize];
    ["Offset of field: AVProgram::discard"][::std::mem::offset_of!(AVProgram, discard) - 8usize];
    ["Offset of field: AVProgram::stream_index"]
        [::std::mem::offset_of!(AVProgram, stream_index) - 16usize];
    ["Offset of field: AVProgram::nb_stream_indexes"]
        [::std::mem::offset_of!(AVProgram, nb_stream_indexes) - 24usize];
    ["Offset of field: AVProgram::metadata"][::std::mem::offset_of!(AVProgram, metadata) - 32usize];
    ["Offset of field: AVProgram::program_num"]
        [::std::mem::offset_of!(AVProgram, program_num) - 40usize];
    ["Offset of field: AVProgram::pmt_pid"][::std::mem::offset_of!(AVProgram, pmt_pid) - 44usize];
    ["Offset of field: AVProgram::pcr_pid"][::std::mem::offset_of!(AVProgram, pcr_pid) - 48usize];
    ["Offset of field: AVProgram::pmt_version"]
        [::std::mem::offset_of!(AVProgram, pmt_version) - 52usize];
    ["Offset of field: AVProgram::start_time"]
        [::std::mem::offset_of!(AVProgram, start_time) - 56usize];
    ["Offset of field: AVProgram::end_time"][::std::mem::offset_of!(AVProgram, end_time) - 64usize];
    ["Offset of field: AVProgram::pts_wrap_reference"]
        [::std::mem::offset_of!(AVProgram, pts_wrap_reference) - 72usize];
    ["Offset of field: AVProgram::pts_wrap_behavior"]
        [::std::mem::offset_of!(AVProgram, pts_wrap_behavior) - 80usize];
};
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVChapter {
    pub id: i64,
    pub time_base: AVRational,
    pub start: i64,
    pub end: i64,
    pub metadata: *mut AVDictionary,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVChapter"][::std::mem::size_of::<AVChapter>() - 40usize];
    ["Alignment of AVChapter"][::std::mem::align_of::<AVChapter>() - 8usize];
    ["Offset of field: AVChapter::id"][::std::mem::offset_of!(AVChapter, id) - 0usize];
    ["Offset of field: AVChapter::time_base"]
        [::std::mem::offset_of!(AVChapter, time_base) - 8usize];
    ["Offset of field: AVChapter::start"][::std::mem::offset_of!(AVChapter, start) - 16usize];
    ["Offset of field: AVChapter::end"][::std::mem::offset_of!(AVChapter, end) - 24usize];
    ["Offset of field: AVChapter::metadata"][::std::mem::offset_of!(AVChapter, metadata) - 32usize];
};
pub type av_format_control_message = ::std::option::Option<
    unsafe extern "C" fn(
        s: *mut AVFormatContext,
        type_: ::std::os::raw::c_int,
        data: *mut ::std::os::raw::c_void,
        data_size: usize,
    ) -> ::std::os::raw::c_int,
>;
pub const AVDurationEstimationMethod_AVFMT_DURATION_FROM_PTS: AVDurationEstimationMethod = 0;
pub const AVDurationEstimationMethod_AVFMT_DURATION_FROM_STREAM: AVDurationEstimationMethod = 1;
pub const AVDurationEstimationMethod_AVFMT_DURATION_FROM_BITRATE: AVDurationEstimationMethod = 2;
pub type AVDurationEstimationMethod = ::std::os::raw::c_int;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVFormatContext {
    pub av_class: *const AVClass,
    pub iformat: *const AVInputFormat,
    pub oformat: *const AVOutputFormat,
    pub priv_data: *mut ::std::os::raw::c_void,
    pub pb: *mut AVIOContext,
    pub ctx_flags: ::std::os::raw::c_int,
    pub nb_streams: ::std::os::raw::c_uint,
    pub streams: *mut *mut AVStream,
    pub nb_stream_groups: ::std::os::raw::c_uint,
    pub stream_groups: *mut *mut AVStreamGroup,
    pub nb_chapters: ::std::os::raw::c_uint,
    pub chapters: *mut *mut AVChapter,
    pub url: *mut ::std::os::raw::c_char,
    pub start_time: i64,
    pub duration: i64,
    pub bit_rate: i64,
    pub packet_size: ::std::os::raw::c_uint,
    pub max_delay: ::std::os::raw::c_int,
    pub flags: ::std::os::raw::c_int,
    pub probesize: i64,
    pub max_analyze_duration: i64,
    pub key: *const u8,
    pub keylen: ::std::os::raw::c_int,
    pub nb_programs: ::std::os::raw::c_uint,
    pub programs: *mut *mut AVProgram,
    pub video_codec_id: AVCodecID,
    pub audio_codec_id: AVCodecID,
    pub subtitle_codec_id: AVCodecID,
    pub data_codec_id: AVCodecID,
    pub metadata: *mut AVDictionary,
    pub start_time_realtime: i64,
    pub fps_probe_size: ::std::os::raw::c_int,
    pub error_recognition: ::std::os::raw::c_int,
    pub interrupt_callback: AVIOInterruptCB,
    pub debug: ::std::os::raw::c_int,
    pub max_streams: ::std::os::raw::c_int,
    pub max_index_size: ::std::os::raw::c_uint,
    pub max_picture_buffer: ::std::os::raw::c_uint,
    pub max_interleave_delta: i64,
    pub max_ts_probe: ::std::os::raw::c_int,
    pub max_chunk_duration: ::std::os::raw::c_int,
    pub max_chunk_size: ::std::os::raw::c_int,
    pub max_probe_packets: ::std::os::raw::c_int,
    pub strict_std_compliance: ::std::os::raw::c_int,
    pub event_flags: ::std::os::raw::c_int,
    pub avoid_negative_ts: ::std::os::raw::c_int,
    pub audio_preload: ::std::os::raw::c_int,
    pub use_wallclock_as_timestamps: ::std::os::raw::c_int,
    pub skip_estimate_duration_from_pts: ::std::os::raw::c_int,
    pub avio_flags: ::std::os::raw::c_int,
    pub duration_estimation_method: AVDurationEstimationMethod,
    pub skip_initial_bytes: i64,
    pub correct_ts_overflow: ::std::os::raw::c_uint,
    pub seek2any: ::std::os::raw::c_int,
    pub flush_packets: ::std::os::raw::c_int,
    pub probe_score: ::std::os::raw::c_int,
    pub format_probesize: ::std::os::raw::c_int,
    pub codec_whitelist: *mut ::std::os::raw::c_char,
    pub format_whitelist: *mut ::std::os::raw::c_char,
    pub protocol_whitelist: *mut ::std::os::raw::c_char,
    pub protocol_blacklist: *mut ::std::os::raw::c_char,
    pub io_repositioned: ::std::os::raw::c_int,
    pub video_codec: *const AVCodec,
    pub audio_codec: *const AVCodec,
    pub subtitle_codec: *const AVCodec,
    pub data_codec: *const AVCodec,
    pub metadata_header_padding: ::std::os::raw::c_int,
    pub opaque: *mut ::std::os::raw::c_void,
    pub control_message_cb: av_format_control_message,
    pub output_ts_offset: i64,
    pub dump_separator: *mut u8,
    pub io_open: ::std::option::Option<
        unsafe extern "C" fn(
            s: *mut AVFormatContext,
            pb: *mut *mut AVIOContext,
            url: *const ::std::os::raw::c_char,
            flags: ::std::os::raw::c_int,
            options: *mut *mut AVDictionary,
        ) -> ::std::os::raw::c_int,
    >,
    pub io_close2: ::std::option::Option<
        unsafe extern "C" fn(
            s: *mut AVFormatContext,
            pb: *mut AVIOContext,
        ) -> ::std::os::raw::c_int,
    >,
    pub duration_probesize: i64,
    pub name: *mut ::std::os::raw::c_char,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVFormatContext"][::std::mem::size_of::<AVFormatContext>() - 480usize];
    ["Alignment of AVFormatContext"][::std::mem::align_of::<AVFormatContext>() - 8usize];
    ["Offset of field: AVFormatContext::av_class"]
        [::std::mem::offset_of!(AVFormatContext, av_class) - 0usize];
    ["Offset of field: AVFormatContext::iformat"]
        [::std::mem::offset_of!(AVFormatContext, iformat) - 8usize];
    ["Offset of field: AVFormatContext::oformat"]
        [::std::mem::offset_of!(AVFormatContext, oformat) - 16usize];
    ["Offset of field: AVFormatContext::priv_data"]
        [::std::mem::offset_of!(AVFormatContext, priv_data) - 24usize];
    ["Offset of field: AVFormatContext::pb"][::std::mem::offset_of!(AVFormatContext, pb) - 32usize];
    ["Offset of field: AVFormatContext::ctx_flags"]
        [::std::mem::offset_of!(AVFormatContext, ctx_flags) - 40usize];
    ["Offset of field: AVFormatContext::nb_streams"]
        [::std::mem::offset_of!(AVFormatContext, nb_streams) - 44usize];
    ["Offset of field: AVFormatContext::streams"]
        [::std::mem::offset_of!(AVFormatContext, streams) - 48usize];
    ["Offset of field: AVFormatContext::nb_stream_groups"]
        [::std::mem::offset_of!(AVFormatContext, nb_stream_groups) - 56usize];
    ["Offset of field: AVFormatContext::stream_groups"]
        [::std::mem::offset_of!(AVFormatContext, stream_groups) - 64usize];
    ["Offset of field: AVFormatContext::nb_chapters"]
        [::std::mem::offset_of!(AVFormatContext, nb_chapters) - 72usize];
    ["Offset of field: AVFormatContext::chapters"]
        [::std::mem::offset_of!(AVFormatContext, chapters) - 80usize];
    ["Offset of field: AVFormatContext::url"]
        [::std::mem::offset_of!(AVFormatContext, url) - 88usize];
    ["Offset of field: AVFormatContext::start_time"]
        [::std::mem::offset_of!(AVFormatContext, start_time) - 96usize];
    ["Offset of field: AVFormatContext::duration"]
        [::std::mem::offset_of!(AVFormatContext, duration) - 104usize];
    ["Offset of field: AVFormatContext::bit_rate"]
        [::std::mem::offset_of!(AVFormatContext, bit_rate) - 112usize];
    ["Offset of field: AVFormatContext::packet_size"]
        [::std::mem::offset_of!(AVFormatContext, packet_size) - 120usize];
    ["Offset of field: AVFormatContext::max_delay"]
        [::std::mem::offset_of!(AVFormatContext, max_delay) - 124usize];
    ["Offset of field: AVFormatContext::flags"]
        [::std::mem::offset_of!(AVFormatContext, flags) - 128usize];
    ["Offset of field: AVFormatContext::probesize"]
        [::std::mem::offset_of!(AVFormatContext, probesize) - 136usize];
    ["Offset of field: AVFormatContext::max_analyze_duration"]
        [::std::mem::offset_of!(AVFormatContext, max_analyze_duration) - 144usize];
    ["Offset of field: AVFormatContext::key"]
        [::std::mem::offset_of!(AVFormatContext, key) - 152usize];
    ["Offset of field: AVFormatContext::keylen"]
        [::std::mem::offset_of!(AVFormatContext, keylen) - 160usize];
    ["Offset of field: AVFormatContext::nb_programs"]
        [::std::mem::offset_of!(AVFormatContext, nb_programs) - 164usize];
    ["Offset of field: AVFormatContext::programs"]
        [::std::mem::offset_of!(AVFormatContext, programs) - 168usize];
    ["Offset of field: AVFormatContext::video_codec_id"]
        [::std::mem::offset_of!(AVFormatContext, video_codec_id) - 176usize];
    ["Offset of field: AVFormatContext::audio_codec_id"]
        [::std::mem::offset_of!(AVFormatContext, audio_codec_id) - 180usize];
    ["Offset of field: AVFormatContext::subtitle_codec_id"]
        [::std::mem::offset_of!(AVFormatContext, subtitle_codec_id) - 184usize];
    ["Offset of field: AVFormatContext::data_codec_id"]
        [::std::mem::offset_of!(AVFormatContext, data_codec_id) - 188usize];
    ["Offset of field: AVFormatContext::metadata"]
        [::std::mem::offset_of!(AVFormatContext, metadata) - 192usize];
    ["Offset of field: AVFormatContext::start_time_realtime"]
        [::std::mem::offset_of!(AVFormatContext, start_time_realtime) - 200usize];
    ["Offset of field: AVFormatContext::fps_probe_size"]
        [::std::mem::offset_of!(AVFormatContext, fps_probe_size) - 208usize];
    ["Offset of field: AVFormatContext::error_recognition"]
        [::std::mem::offset_of!(AVFormatContext, error_recognition) - 212usize];
    ["Offset of field: AVFormatContext::interrupt_callback"]
        [::std::mem::offset_of!(AVFormatContext, interrupt_callback) - 216usize];
    ["Offset of field: AVFormatContext::debug"]
        [::std::mem::offset_of!(AVFormatContext, debug) - 232usize];
    ["Offset of field: AVFormatContext::max_streams"]
        [::std::mem::offset_of!(AVFormatContext, max_streams) - 236usize];
    ["Offset of field: AVFormatContext::max_index_size"]
        [::std::mem::offset_of!(AVFormatContext, max_index_size) - 240usize];
    ["Offset of field: AVFormatContext::max_picture_buffer"]
        [::std::mem::offset_of!(AVFormatContext, max_picture_buffer) - 244usize];
    ["Offset of field: AVFormatContext::max_interleave_delta"]
        [::std::mem::offset_of!(AVFormatContext, max_interleave_delta) - 248usize];
    ["Offset of field: AVFormatContext::max_ts_probe"]
        [::std::mem::offset_of!(AVFormatContext, max_ts_probe) - 256usize];
    ["Offset of field: AVFormatContext::max_chunk_duration"]
        [::std::mem::offset_of!(AVFormatContext, max_chunk_duration) - 260usize];
    ["Offset of field: AVFormatContext::max_chunk_size"]
        [::std::mem::offset_of!(AVFormatContext, max_chunk_size) - 264usize];
    ["Offset of field: AVFormatContext::max_probe_packets"]
        [::std::mem::offset_of!(AVFormatContext, max_probe_packets) - 268usize];
    ["Offset of field: AVFormatContext::strict_std_compliance"]
        [::std::mem::offset_of!(AVFormatContext, strict_std_compliance) - 272usize];
    ["Offset of field: AVFormatContext::event_flags"]
        [::std::mem::offset_of!(AVFormatContext, event_flags) - 276usize];
    ["Offset of field: AVFormatContext::avoid_negative_ts"]
        [::std::mem::offset_of!(AVFormatContext, avoid_negative_ts) - 280usize];
    ["Offset of field: AVFormatContext::audio_preload"]
        [::std::mem::offset_of!(AVFormatContext, audio_preload) - 284usize];
    ["Offset of field: AVFormatContext::use_wallclock_as_timestamps"]
        [::std::mem::offset_of!(AVFormatContext, use_wallclock_as_timestamps) - 288usize];
    ["Offset of field: AVFormatContext::skip_estimate_duration_from_pts"]
        [::std::mem::offset_of!(AVFormatContext, skip_estimate_duration_from_pts) - 292usize];
    ["Offset of field: AVFormatContext::avio_flags"]
        [::std::mem::offset_of!(AVFormatContext, avio_flags) - 296usize];
    ["Offset of field: AVFormatContext::duration_estimation_method"]
        [::std::mem::offset_of!(AVFormatContext, duration_estimation_method) - 300usize];
    ["Offset of field: AVFormatContext::skip_initial_bytes"]
        [::std::mem::offset_of!(AVFormatContext, skip_initial_bytes) - 304usize];
    ["Offset of field: AVFormatContext::correct_ts_overflow"]
        [::std::mem::offset_of!(AVFormatContext, correct_ts_overflow) - 312usize];
    ["Offset of field: AVFormatContext::seek2any"]
        [::std::mem::offset_of!(AVFormatContext, seek2any) - 316usize];
    ["Offset of field: AVFormatContext::flush_packets"]
        [::std::mem::offset_of!(AVFormatContext, flush_packets) - 320usize];
    ["Offset of field: AVFormatContext::probe_score"]
        [::std::mem::offset_of!(AVFormatContext, probe_score) - 324usize];
    ["Offset of field: AVFormatContext::format_probesize"]
        [::std::mem::offset_of!(AVFormatContext, format_probesize) - 328usize];
    ["Offset of field: AVFormatContext::codec_whitelist"]
        [::std::mem::offset_of!(AVFormatContext, codec_whitelist) - 336usize];
    ["Offset of field: AVFormatContext::format_whitelist"]
        [::std::mem::offset_of!(AVFormatContext, format_whitelist) - 344usize];
    ["Offset of field: AVFormatContext::protocol_whitelist"]
        [::std::mem::offset_of!(AVFormatContext, protocol_whitelist) - 352usize];
    ["Offset of field: AVFormatContext::protocol_blacklist"]
        [::std::mem::offset_of!(AVFormatContext, protocol_blacklist) - 360usize];
    ["Offset of field: AVFormatContext::io_repositioned"]
        [::std::mem::offset_of!(AVFormatContext, io_repositioned) - 368usize];
    ["Offset of field: AVFormatContext::video_codec"]
        [::std::mem::offset_of!(AVFormatContext, video_codec) - 376usize];
    ["Offset of field: AVFormatContext::audio_codec"]
        [::std::mem::offset_of!(AVFormatContext, audio_codec) - 384usize];
    ["Offset of field: AVFormatContext::subtitle_codec"]
        [::std::mem::offset_of!(AVFormatContext, subtitle_codec) - 392usize];
    ["Offset of field: AVFormatContext::data_codec"]
        [::std::mem::offset_of!(AVFormatContext, data_codec) - 400usize];
    ["Offset of field: AVFormatContext::metadata_header_padding"]
        [::std::mem::offset_of!(AVFormatContext, metadata_header_padding) - 408usize];
    ["Offset of field: AVFormatContext::opaque"]
        [::std::mem::offset_of!(AVFormatContext, opaque) - 416usize];
    ["Offset of field: AVFormatContext::control_message_cb"]
        [::std::mem::offset_of!(AVFormatContext, control_message_cb) - 424usize];
    ["Offset of field: AVFormatContext::output_ts_offset"]
        [::std::mem::offset_of!(AVFormatContext, output_ts_offset) - 432usize];
    ["Offset of field: AVFormatContext::dump_separator"]
        [::std::mem::offset_of!(AVFormatContext, dump_separator) - 440usize];
    ["Offset of field: AVFormatContext::io_open"]
        [::std::mem::offset_of!(AVFormatContext, io_open) - 448usize];
    ["Offset of field: AVFormatContext::io_close2"]
        [::std::mem::offset_of!(AVFormatContext, io_close2) - 456usize];
    ["Offset of field: AVFormatContext::duration_probesize"]
        [::std::mem::offset_of!(AVFormatContext, duration_probesize) - 464usize];
    ["Offset of field: AVFormatContext::name"]
        [::std::mem::offset_of!(AVFormatContext, name) - 472usize];
};
unsafe extern "C" {
    pub fn av_muxer_iterate(opaque: *mut *mut ::std::os::raw::c_void) -> *const AVOutputFormat;
}
unsafe extern "C" {
    pub fn av_demuxer_iterate(opaque: *mut *mut ::std::os::raw::c_void) -> *const AVInputFormat;
}
unsafe extern "C" {
    pub fn av_stream_get_class() -> *const AVClass;
}
unsafe extern "C" {
    pub fn av_stream_group_get_class() -> *const AVClass;
}
unsafe extern "C" {
    pub fn av_new_program(s: *mut AVFormatContext, id: ::std::os::raw::c_int) -> *mut AVProgram;
}
unsafe extern "C" {
    pub fn av_find_input_format(short_name: *const ::std::os::raw::c_char) -> *const AVInputFormat;
}
unsafe extern "C" {
    pub fn av_probe_input_format(
        pd: *const AVProbeData,
        is_opened: ::std::os::raw::c_int,
    ) -> *const AVInputFormat;
}
unsafe extern "C" {
    pub fn av_probe_input_format2(
        pd: *const AVProbeData,
        is_opened: ::std::os::raw::c_int,
        score_max: *mut ::std::os::raw::c_int,
    ) -> *const AVInputFormat;
}
unsafe extern "C" {
    pub fn av_probe_input_format3(
        pd: *const AVProbeData,
        is_opened: ::std::os::raw::c_int,
        score_ret: *mut ::std::os::raw::c_int,
    ) -> *const AVInputFormat;
}
unsafe extern "C" {
    pub fn av_probe_input_buffer2(
        pb: *mut AVIOContext,
        fmt: *mut *const AVInputFormat,
        url: *const ::std::os::raw::c_char,
        logctx: *mut ::std::os::raw::c_void,
        offset: ::std::os::raw::c_uint,
        max_probe_size: ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_probe_input_buffer(
        pb: *mut AVIOContext,
        fmt: *mut *const AVInputFormat,
        url: *const ::std::os::raw::c_char,
        logctx: *mut ::std::os::raw::c_void,
        offset: ::std::os::raw::c_uint,
        max_probe_size: ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_find_program_from_stream(
        ic: *mut AVFormatContext,
        last: *mut AVProgram,
        s: ::std::os::raw::c_int,
    ) -> *mut AVProgram;
}
unsafe extern "C" {
    pub fn av_program_add_stream_index(
        ac: *mut AVFormatContext,
        progid: ::std::os::raw::c_int,
        idx: ::std::os::raw::c_uint,
    );
}
unsafe extern "C" {
    pub fn av_find_best_stream(
        ic: *mut AVFormatContext,
        type_: AVMediaType,
        wanted_stream_nb: ::std::os::raw::c_int,
        related_stream: ::std::os::raw::c_int,
        decoder_ret: *mut *const AVCodec,
        flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_read_frame(s: *mut AVFormatContext, pkt: *mut AVPacket) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_seek_frame(
        s: *mut AVFormatContext,
        stream_index: ::std::os::raw::c_int,
        timestamp: i64,
        flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_read_play(s: *mut AVFormatContext) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_read_pause(s: *mut AVFormatContext) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_write_frame(s: *mut AVFormatContext, pkt: *mut AVPacket) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_interleaved_write_frame(
        s: *mut AVFormatContext,
        pkt: *mut AVPacket,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_write_uncoded_frame(
        s: *mut AVFormatContext,
        stream_index: ::std::os::raw::c_int,
        frame: *mut AVFrame,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_interleaved_write_uncoded_frame(
        s: *mut AVFormatContext,
        stream_index: ::std::os::raw::c_int,
        frame: *mut AVFrame,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_write_uncoded_frame_query(
        s: *mut AVFormatContext,
        stream_index: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_write_trailer(s: *mut AVFormatContext) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_guess_format(
        short_name: *const ::std::os::raw::c_char,
        filename: *const ::std::os::raw::c_char,
        mime_type: *const ::std::os::raw::c_char,
    ) -> *const AVOutputFormat;
}
unsafe extern "C" {
    pub fn av_guess_codec(
        fmt: *const AVOutputFormat,
        short_name: *const ::std::os::raw::c_char,
        filename: *const ::std::os::raw::c_char,
        mime_type: *const ::std::os::raw::c_char,
        type_: AVMediaType,
    ) -> AVCodecID;
}
unsafe extern "C" {
    pub fn av_get_output_timestamp(
        s: *mut AVFormatContext,
        stream: ::std::os::raw::c_int,
        dts: *mut i64,
        wall: *mut i64,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_hex_dump(f: *mut FILE, buf: *const u8, size: ::std::os::raw::c_int);
}
unsafe extern "C" {
    pub fn av_hex_dump_log(
        avcl: *mut ::std::os::raw::c_void,
        level: ::std::os::raw::c_int,
        buf: *const u8,
        size: ::std::os::raw::c_int,
    );
}
unsafe extern "C" {
    pub fn av_pkt_dump2(
        f: *mut FILE,
        pkt: *const AVPacket,
        dump_payload: ::std::os::raw::c_int,
        st: *const AVStream,
    );
}
unsafe extern "C" {
    pub fn av_pkt_dump_log2(
        avcl: *mut ::std::os::raw::c_void,
        level: ::std::os::raw::c_int,
        pkt: *const AVPacket,
        dump_payload: ::std::os::raw::c_int,
        st: *const AVStream,
    );
}
unsafe extern "C" {
    pub fn av_codec_get_id(
        tags: *const *const AVCodecTag,
        tag: ::std::os::raw::c_uint,
    ) -> AVCodecID;
}
unsafe extern "C" {
    pub fn av_codec_get_tag(
        tags: *const *const AVCodecTag,
        id: AVCodecID,
    ) -> ::std::os::raw::c_uint;
}
unsafe extern "C" {
    pub fn av_codec_get_tag2(
        tags: *const *const AVCodecTag,
        id: AVCodecID,
        tag: *mut ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_find_default_stream_index(s: *mut AVFormatContext) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_index_search_timestamp(
        st: *mut AVStream,
        timestamp: i64,
        flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_add_index_entry(
        st: *mut AVStream,
        pos: i64,
        timestamp: i64,
        size: ::std::os::raw::c_int,
        distance: ::std::os::raw::c_int,
        flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_url_split(
        proto: *mut ::std::os::raw::c_char,
        proto_size: ::std::os::raw::c_int,
        authorization: *mut ::std::os::raw::c_char,
        authorization_size: ::std::os::raw::c_int,
        hostname: *mut ::std::os::raw::c_char,
        hostname_size: ::std::os::raw::c_int,
        port_ptr: *mut ::std::os::raw::c_int,
        path: *mut ::std::os::raw::c_char,
        path_size: ::std::os::raw::c_int,
        url: *const ::std::os::raw::c_char,
    );
}
unsafe extern "C" {
    pub fn av_dump_format(
        ic: *mut AVFormatContext,
        index: ::std::os::raw::c_int,
        url: *const ::std::os::raw::c_char,
        is_output: ::std::os::raw::c_int,
    );
}
unsafe extern "C" {
    pub fn av_get_frame_filename2(
        buf: *mut ::std::os::raw::c_char,
        buf_size: ::std::os::raw::c_int,
        path: *const ::std::os::raw::c_char,
        number: ::std::os::raw::c_int,
        flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_get_frame_filename(
        buf: *mut ::std::os::raw::c_char,
        buf_size: ::std::os::raw::c_int,
        path: *const ::std::os::raw::c_char,
        number: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_filename_number_test(
        filename: *const ::std::os::raw::c_char,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_sdp_create(
        ac: *mut *mut AVFormatContext,
        n_files: ::std::os::raw::c_int,
        buf: *mut ::std::os::raw::c_char,
        size: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_match_ext(
        filename: *const ::std::os::raw::c_char,
        extensions: *const ::std::os::raw::c_char,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_mime_codec_str(
        par: *const AVCodecParameters,
        frame_rate: AVRational,
        out: *mut AVBPrint,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_guess_sample_aspect_ratio(
        format: *mut AVFormatContext,
        stream: *mut AVStream,
        frame: *mut AVFrame,
    ) -> AVRational;
}
unsafe extern "C" {
    pub fn av_guess_frame_rate(
        ctx: *mut AVFormatContext,
        stream: *mut AVStream,
        frame: *mut AVFrame,
    ) -> AVRational;
}
unsafe extern "C" {
    pub fn av_stream_get_codec_timebase(st: *const AVStream) -> AVRational;
}
pub const AVOptionType_AV_OPT_TYPE_FLAGS: AVOptionType = 1;
pub const AVOptionType_AV_OPT_TYPE_INT: AVOptionType = 2;
pub const AVOptionType_AV_OPT_TYPE_INT64: AVOptionType = 3;
pub const AVOptionType_AV_OPT_TYPE_DOUBLE: AVOptionType = 4;
pub const AVOptionType_AV_OPT_TYPE_FLOAT: AVOptionType = 5;
pub const AVOptionType_AV_OPT_TYPE_STRING: AVOptionType = 6;
pub const AVOptionType_AV_OPT_TYPE_RATIONAL: AVOptionType = 7;
pub const AVOptionType_AV_OPT_TYPE_BINARY: AVOptionType = 8;
pub const AVOptionType_AV_OPT_TYPE_DICT: AVOptionType = 9;
pub const AVOptionType_AV_OPT_TYPE_UINT64: AVOptionType = 10;
pub const AVOptionType_AV_OPT_TYPE_CONST: AVOptionType = 11;
pub const AVOptionType_AV_OPT_TYPE_IMAGE_SIZE: AVOptionType = 12;
pub const AVOptionType_AV_OPT_TYPE_PIXEL_FMT: AVOptionType = 13;
pub const AVOptionType_AV_OPT_TYPE_SAMPLE_FMT: AVOptionType = 14;
pub const AVOptionType_AV_OPT_TYPE_VIDEO_RATE: AVOptionType = 15;
pub const AVOptionType_AV_OPT_TYPE_DURATION: AVOptionType = 16;
pub const AVOptionType_AV_OPT_TYPE_COLOR: AVOptionType = 17;
pub const AVOptionType_AV_OPT_TYPE_BOOL: AVOptionType = 18;
pub const AVOptionType_AV_OPT_TYPE_CHLAYOUT: AVOptionType = 19;
pub const AVOptionType_AV_OPT_TYPE_UINT: AVOptionType = 20;
pub const AVOptionType_AV_OPT_TYPE_FLAG_ARRAY: AVOptionType = 65536;
pub type AVOptionType = ::std::os::raw::c_int;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVOptionArrayDef {
    pub def: *const ::std::os::raw::c_char,
    pub size_min: ::std::os::raw::c_uint,
    pub size_max: ::std::os::raw::c_uint,
    pub sep: ::std::os::raw::c_char,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVOptionArrayDef"][::std::mem::size_of::<AVOptionArrayDef>() - 24usize];
    ["Alignment of AVOptionArrayDef"][::std::mem::align_of::<AVOptionArrayDef>() - 8usize];
    ["Offset of field: AVOptionArrayDef::def"]
        [::std::mem::offset_of!(AVOptionArrayDef, def) - 0usize];
    ["Offset of field: AVOptionArrayDef::size_min"]
        [::std::mem::offset_of!(AVOptionArrayDef, size_min) - 8usize];
    ["Offset of field: AVOptionArrayDef::size_max"]
        [::std::mem::offset_of!(AVOptionArrayDef, size_max) - 12usize];
    ["Offset of field: AVOptionArrayDef::sep"]
        [::std::mem::offset_of!(AVOptionArrayDef, sep) - 16usize];
};
#[repr(C)]
#[derive(Copy, Clone)]
pub struct AVOption {
    pub name: *const ::std::os::raw::c_char,
    pub help: *const ::std::os::raw::c_char,
    pub offset: ::std::os::raw::c_int,
    pub type_: AVOptionType,
    pub default_val: AVOption__bindgen_ty_1,
    pub min: f64,
    pub max: f64,
    pub flags: ::std::os::raw::c_int,
    pub unit: *const ::std::os::raw::c_char,
}
#[repr(C)]
#[derive(Copy, Clone)]
pub union AVOption__bindgen_ty_1 {
    pub i64_: i64,
    pub dbl: f64,
    pub str_: *const ::std::os::raw::c_char,
    pub q: AVRational,
    pub arr: *const AVOptionArrayDef,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVOption__bindgen_ty_1"][::std::mem::size_of::<AVOption__bindgen_ty_1>() - 8usize];
    ["Alignment of AVOption__bindgen_ty_1"]
        [::std::mem::align_of::<AVOption__bindgen_ty_1>() - 8usize];
    ["Offset of field: AVOption__bindgen_ty_1::i64_"]
        [::std::mem::offset_of!(AVOption__bindgen_ty_1, i64_) - 0usize];
    ["Offset of field: AVOption__bindgen_ty_1::dbl"]
        [::std::mem::offset_of!(AVOption__bindgen_ty_1, dbl) - 0usize];
    ["Offset of field: AVOption__bindgen_ty_1::str_"]
        [::std::mem::offset_of!(AVOption__bindgen_ty_1, str_) - 0usize];
    ["Offset of field: AVOption__bindgen_ty_1::q"]
        [::std::mem::offset_of!(AVOption__bindgen_ty_1, q) - 0usize];
    ["Offset of field: AVOption__bindgen_ty_1::arr"]
        [::std::mem::offset_of!(AVOption__bindgen_ty_1, arr) - 0usize];
};
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVOption"][::std::mem::size_of::<AVOption>() - 64usize];
    ["Alignment of AVOption"][::std::mem::align_of::<AVOption>() - 8usize];
    ["Offset of field: AVOption::name"][::std::mem::offset_of!(AVOption, name) - 0usize];
    ["Offset of field: AVOption::help"][::std::mem::offset_of!(AVOption, help) - 8usize];
    ["Offset of field: AVOption::offset"][::std::mem::offset_of!(AVOption, offset) - 16usize];
    ["Offset of field: AVOption::type_"][::std::mem::offset_of!(AVOption, type_) - 20usize];
    ["Offset of field: AVOption::default_val"]
        [::std::mem::offset_of!(AVOption, default_val) - 24usize];
    ["Offset of field: AVOption::min"][::std::mem::offset_of!(AVOption, min) - 32usize];
    ["Offset of field: AVOption::max"][::std::mem::offset_of!(AVOption, max) - 40usize];
    ["Offset of field: AVOption::flags"][::std::mem::offset_of!(AVOption, flags) - 48usize];
    ["Offset of field: AVOption::unit"][::std::mem::offset_of!(AVOption, unit) - 56usize];
};
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVOptionRange {
    pub str_: *const ::std::os::raw::c_char,
    pub value_min: f64,
    pub value_max: f64,
    pub component_min: f64,
    pub component_max: f64,
    pub is_range: ::std::os::raw::c_int,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVOptionRange"][::std::mem::size_of::<AVOptionRange>() - 48usize];
    ["Alignment of AVOptionRange"][::std::mem::align_of::<AVOptionRange>() - 8usize];
    ["Offset of field: AVOptionRange::str_"][::std::mem::offset_of!(AVOptionRange, str_) - 0usize];
    ["Offset of field: AVOptionRange::value_min"]
        [::std::mem::offset_of!(AVOptionRange, value_min) - 8usize];
    ["Offset of field: AVOptionRange::value_max"]
        [::std::mem::offset_of!(AVOptionRange, value_max) - 16usize];
    ["Offset of field: AVOptionRange::component_min"]
        [::std::mem::offset_of!(AVOptionRange, component_min) - 24usize];
    ["Offset of field: AVOptionRange::component_max"]
        [::std::mem::offset_of!(AVOptionRange, component_max) - 32usize];
    ["Offset of field: AVOptionRange::is_range"]
        [::std::mem::offset_of!(AVOptionRange, is_range) - 40usize];
};
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVOptionRanges {
    pub range: *mut *mut AVOptionRange,
    pub nb_ranges: ::std::os::raw::c_int,
    pub nb_components: ::std::os::raw::c_int,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVOptionRanges"][::std::mem::size_of::<AVOptionRanges>() - 16usize];
    ["Alignment of AVOptionRanges"][::std::mem::align_of::<AVOptionRanges>() - 8usize];
    ["Offset of field: AVOptionRanges::range"]
        [::std::mem::offset_of!(AVOptionRanges, range) - 0usize];
    ["Offset of field: AVOptionRanges::nb_ranges"]
        [::std::mem::offset_of!(AVOptionRanges, nb_ranges) - 8usize];
    ["Offset of field: AVOptionRanges::nb_components"]
        [::std::mem::offset_of!(AVOptionRanges, nb_components) - 12usize];
};
unsafe extern "C" {
    pub fn av_opt_set_defaults(s: *mut ::std::os::raw::c_void);
}
unsafe extern "C" {
    pub fn av_opt_set_defaults2(
        s: *mut ::std::os::raw::c_void,
        mask: ::std::os::raw::c_int,
        flags: ::std::os::raw::c_int,
    );
}
unsafe extern "C" {
    pub fn av_opt_free(obj: *mut ::std::os::raw::c_void);
}
unsafe extern "C" {
    pub fn av_opt_next(
        obj: *const ::std::os::raw::c_void,
        prev: *const AVOption,
    ) -> *const AVOption;
}
unsafe extern "C" {
    pub fn av_opt_child_next(
        obj: *mut ::std::os::raw::c_void,
        prev: *mut ::std::os::raw::c_void,
    ) -> *mut ::std::os::raw::c_void;
}
unsafe extern "C" {
    pub fn av_opt_child_class_iterate(
        parent: *const AVClass,
        iter: *mut *mut ::std::os::raw::c_void,
    ) -> *const AVClass;
}
unsafe extern "C" {
    pub fn av_opt_find(
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
        unit: *const ::std::os::raw::c_char,
        opt_flags: ::std::os::raw::c_int,
        search_flags: ::std::os::raw::c_int,
    ) -> *const AVOption;
}
unsafe extern "C" {
    pub fn av_opt_find2(
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
        unit: *const ::std::os::raw::c_char,
        opt_flags: ::std::os::raw::c_int,
        search_flags: ::std::os::raw::c_int,
        target_obj: *mut *mut ::std::os::raw::c_void,
    ) -> *const AVOption;
}
unsafe extern "C" {
    pub fn av_opt_show2(
        obj: *mut ::std::os::raw::c_void,
        av_log_obj: *mut ::std::os::raw::c_void,
        req_flags: ::std::os::raw::c_int,
        rej_flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_get_key_value(
        ropts: *mut *const ::std::os::raw::c_char,
        key_val_sep: *const ::std::os::raw::c_char,
        pairs_sep: *const ::std::os::raw::c_char,
        flags: ::std::os::raw::c_uint,
        rkey: *mut *mut ::std::os::raw::c_char,
        rval: *mut *mut ::std::os::raw::c_char,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_set_options_string(
        ctx: *mut ::std::os::raw::c_void,
        opts: *const ::std::os::raw::c_char,
        key_val_sep: *const ::std::os::raw::c_char,
        pairs_sep: *const ::std::os::raw::c_char,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_set_from_string(
        ctx: *mut ::std::os::raw::c_void,
        opts: *const ::std::os::raw::c_char,
        shorthand: *const *const ::std::os::raw::c_char,
        key_val_sep: *const ::std::os::raw::c_char,
        pairs_sep: *const ::std::os::raw::c_char,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_set_dict(
        obj: *mut ::std::os::raw::c_void,
        options: *mut *mut AVDictionary,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_set_dict2(
        obj: *mut ::std::os::raw::c_void,
        options: *mut *mut AVDictionary,
        search_flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_copy(
        dest: *mut ::std::os::raw::c_void,
        src: *const ::std::os::raw::c_void,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_set(
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
        val: *const ::std::os::raw::c_char,
        search_flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_set_int(
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
        val: i64,
        search_flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_set_double(
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
        val: f64,
        search_flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_set_q(
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
        val: AVRational,
        search_flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_set_bin(
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
        val: *const u8,
        size: ::std::os::raw::c_int,
        search_flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_set_image_size(
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
        w: ::std::os::raw::c_int,
        h: ::std::os::raw::c_int,
        search_flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_set_pixel_fmt(
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
        fmt: AVPixelFormat,
        search_flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_set_sample_fmt(
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
        fmt: AVSampleFormat,
        search_flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_set_video_rate(
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
        val: AVRational,
        search_flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_set_chlayout(
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
        layout: *const AVChannelLayout,
        search_flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_set_dict_val(
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
        val: *const AVDictionary,
        search_flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_set_array(
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
        search_flags: ::std::os::raw::c_int,
        start_elem: ::std::os::raw::c_uint,
        nb_elems: ::std::os::raw::c_uint,
        val_type: AVOptionType,
        val: *const ::std::os::raw::c_void,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_get(
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
        search_flags: ::std::os::raw::c_int,
        out_val: *mut *mut u8,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_get_int(
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
        search_flags: ::std::os::raw::c_int,
        out_val: *mut i64,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_get_double(
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
        search_flags: ::std::os::raw::c_int,
        out_val: *mut f64,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_get_q(
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
        search_flags: ::std::os::raw::c_int,
        out_val: *mut AVRational,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_get_image_size(
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
        search_flags: ::std::os::raw::c_int,
        w_out: *mut ::std::os::raw::c_int,
        h_out: *mut ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_get_pixel_fmt(
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
        search_flags: ::std::os::raw::c_int,
        out_fmt: *mut AVPixelFormat,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_get_sample_fmt(
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
        search_flags: ::std::os::raw::c_int,
        out_fmt: *mut AVSampleFormat,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_get_video_rate(
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
        search_flags: ::std::os::raw::c_int,
        out_val: *mut AVRational,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_get_chlayout(
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
        search_flags: ::std::os::raw::c_int,
        layout: *mut AVChannelLayout,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_get_dict_val(
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
        search_flags: ::std::os::raw::c_int,
        out_val: *mut *mut AVDictionary,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_get_array_size(
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
        search_flags: ::std::os::raw::c_int,
        out_val: *mut ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_get_array(
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
        search_flags: ::std::os::raw::c_int,
        start_elem: ::std::os::raw::c_uint,
        nb_elems: ::std::os::raw::c_uint,
        out_type: AVOptionType,
        out_val: *mut ::std::os::raw::c_void,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_eval_flags(
        obj: *mut ::std::os::raw::c_void,
        o: *const AVOption,
        val: *const ::std::os::raw::c_char,
        flags_out: *mut ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_eval_int(
        obj: *mut ::std::os::raw::c_void,
        o: *const AVOption,
        val: *const ::std::os::raw::c_char,
        int_out: *mut ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_eval_uint(
        obj: *mut ::std::os::raw::c_void,
        o: *const AVOption,
        val: *const ::std::os::raw::c_char,
        uint_out: *mut ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_eval_int64(
        obj: *mut ::std::os::raw::c_void,
        o: *const AVOption,
        val: *const ::std::os::raw::c_char,
        int64_out: *mut i64,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_eval_float(
        obj: *mut ::std::os::raw::c_void,
        o: *const AVOption,
        val: *const ::std::os::raw::c_char,
        float_out: *mut f32,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_eval_double(
        obj: *mut ::std::os::raw::c_void,
        o: *const AVOption,
        val: *const ::std::os::raw::c_char,
        double_out: *mut f64,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_eval_q(
        obj: *mut ::std::os::raw::c_void,
        o: *const AVOption,
        val: *const ::std::os::raw::c_char,
        q_out: *mut AVRational,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_ptr(
        avclass: *const AVClass,
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
    ) -> *mut ::std::os::raw::c_void;
}
unsafe extern "C" {
    pub fn av_opt_is_set_to_default(
        obj: *mut ::std::os::raw::c_void,
        o: *const AVOption,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_is_set_to_default_by_name(
        obj: *mut ::std::os::raw::c_void,
        name: *const ::std::os::raw::c_char,
        search_flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_flag_is_set(
        obj: *mut ::std::os::raw::c_void,
        field_name: *const ::std::os::raw::c_char,
        flag_name: *const ::std::os::raw::c_char,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_serialize(
        obj: *mut ::std::os::raw::c_void,
        opt_flags: ::std::os::raw::c_int,
        flags: ::std::os::raw::c_int,
        buffer: *mut *mut ::std::os::raw::c_char,
        key_val_sep: ::std::os::raw::c_char,
        pairs_sep: ::std::os::raw::c_char,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_freep_ranges(ranges: *mut *mut AVOptionRanges);
}
unsafe extern "C" {
    pub fn av_opt_query_ranges(
        arg1: *mut *mut AVOptionRanges,
        obj: *mut ::std::os::raw::c_void,
        key: *const ::std::os::raw::c_char,
        flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_opt_query_ranges_default(
        arg1: *mut *mut AVOptionRanges,
        obj: *mut ::std::os::raw::c_void,
        key: *const ::std::os::raw::c_char,
        flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVComponentDescriptor {
    pub plane: ::std::os::raw::c_int,
    pub step: ::std::os::raw::c_int,
    pub offset: ::std::os::raw::c_int,
    pub shift: ::std::os::raw::c_int,
    pub depth: ::std::os::raw::c_int,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVComponentDescriptor"][::std::mem::size_of::<AVComponentDescriptor>() - 20usize];
    ["Alignment of AVComponentDescriptor"]
        [::std::mem::align_of::<AVComponentDescriptor>() - 4usize];
    ["Offset of field: AVComponentDescriptor::plane"]
        [::std::mem::offset_of!(AVComponentDescriptor, plane) - 0usize];
    ["Offset of field: AVComponentDescriptor::step"]
        [::std::mem::offset_of!(AVComponentDescriptor, step) - 4usize];
    ["Offset of field: AVComponentDescriptor::offset"]
        [::std::mem::offset_of!(AVComponentDescriptor, offset) - 8usize];
    ["Offset of field: AVComponentDescriptor::shift"]
        [::std::mem::offset_of!(AVComponentDescriptor, shift) - 12usize];
    ["Offset of field: AVComponentDescriptor::depth"]
        [::std::mem::offset_of!(AVComponentDescriptor, depth) - 16usize];
};
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVPixFmtDescriptor {
    pub name: *const ::std::os::raw::c_char,
    pub nb_components: u8,
    pub log2_chroma_w: u8,
    pub log2_chroma_h: u8,
    pub flags: u64,
    pub comp: [AVComponentDescriptor; 4usize],
    pub alias: *const ::std::os::raw::c_char,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of AVPixFmtDescriptor"][::std::mem::size_of::<AVPixFmtDescriptor>() - 112usize];
    ["Alignment of AVPixFmtDescriptor"][::std::mem::align_of::<AVPixFmtDescriptor>() - 8usize];
    ["Offset of field: AVPixFmtDescriptor::name"]
        [::std::mem::offset_of!(AVPixFmtDescriptor, name) - 0usize];
    ["Offset of field: AVPixFmtDescriptor::nb_components"]
        [::std::mem::offset_of!(AVPixFmtDescriptor, nb_components) - 8usize];
    ["Offset of field: AVPixFmtDescriptor::log2_chroma_w"]
        [::std::mem::offset_of!(AVPixFmtDescriptor, log2_chroma_w) - 9usize];
    ["Offset of field: AVPixFmtDescriptor::log2_chroma_h"]
        [::std::mem::offset_of!(AVPixFmtDescriptor, log2_chroma_h) - 10usize];
    ["Offset of field: AVPixFmtDescriptor::flags"]
        [::std::mem::offset_of!(AVPixFmtDescriptor, flags) - 16usize];
    ["Offset of field: AVPixFmtDescriptor::comp"]
        [::std::mem::offset_of!(AVPixFmtDescriptor, comp) - 24usize];
    ["Offset of field: AVPixFmtDescriptor::alias"]
        [::std::mem::offset_of!(AVPixFmtDescriptor, alias) - 104usize];
};
unsafe extern "C" {
    pub fn av_get_bits_per_pixel(pixdesc: *const AVPixFmtDescriptor) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_get_padded_bits_per_pixel(
        pixdesc: *const AVPixFmtDescriptor,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_pix_fmt_desc_get(pix_fmt: AVPixelFormat) -> *const AVPixFmtDescriptor;
}
unsafe extern "C" {
    pub fn av_pix_fmt_desc_next(prev: *const AVPixFmtDescriptor) -> *const AVPixFmtDescriptor;
}
unsafe extern "C" {
    pub fn av_pix_fmt_desc_get_id(desc: *const AVPixFmtDescriptor) -> AVPixelFormat;
}
unsafe extern "C" {
    pub fn av_pix_fmt_get_chroma_sub_sample(
        pix_fmt: AVPixelFormat,
        h_shift: *mut ::std::os::raw::c_int,
        v_shift: *mut ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_pix_fmt_count_planes(pix_fmt: AVPixelFormat) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_color_range_name(range: AVColorRange) -> *const ::std::os::raw::c_char;
}
unsafe extern "C" {
    pub fn av_color_range_from_name(name: *const ::std::os::raw::c_char) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_color_primaries_name(primaries: AVColorPrimaries) -> *const ::std::os::raw::c_char;
}
unsafe extern "C" {
    pub fn av_color_primaries_from_name(
        name: *const ::std::os::raw::c_char,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_color_transfer_name(
        transfer: AVColorTransferCharacteristic,
    ) -> *const ::std::os::raw::c_char;
}
unsafe extern "C" {
    pub fn av_color_transfer_from_name(
        name: *const ::std::os::raw::c_char,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_color_space_name(space: AVColorSpace) -> *const ::std::os::raw::c_char;
}
unsafe extern "C" {
    pub fn av_color_space_from_name(name: *const ::std::os::raw::c_char) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_chroma_location_name(location: AVChromaLocation) -> *const ::std::os::raw::c_char;
}
unsafe extern "C" {
    pub fn av_chroma_location_from_name(
        name: *const ::std::os::raw::c_char,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_chroma_location_enum_to_pos(
        xpos: *mut ::std::os::raw::c_int,
        ypos: *mut ::std::os::raw::c_int,
        pos: AVChromaLocation,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_chroma_location_pos_to_enum(
        xpos: ::std::os::raw::c_int,
        ypos: ::std::os::raw::c_int,
    ) -> AVChromaLocation;
}
unsafe extern "C" {
    pub fn av_alpha_mode_name(mode: AVAlphaMode) -> *const ::std::os::raw::c_char;
}
unsafe extern "C" {
    pub fn av_alpha_mode_from_name(name: *const ::std::os::raw::c_char) -> AVAlphaMode;
}
unsafe extern "C" {
    pub fn av_get_pix_fmt(name: *const ::std::os::raw::c_char) -> AVPixelFormat;
}
unsafe extern "C" {
    pub fn av_get_pix_fmt_name(pix_fmt: AVPixelFormat) -> *const ::std::os::raw::c_char;
}
unsafe extern "C" {
    pub fn av_get_pix_fmt_string(
        buf: *mut ::std::os::raw::c_char,
        buf_size: ::std::os::raw::c_int,
        pix_fmt: AVPixelFormat,
    ) -> *mut ::std::os::raw::c_char;
}
unsafe extern "C" {
    pub fn av_read_image_line2(
        dst: *mut ::std::os::raw::c_void,
        data: *mut *const u8,
        linesize: *const ::std::os::raw::c_int,
        desc: *const AVPixFmtDescriptor,
        x: ::std::os::raw::c_int,
        y: ::std::os::raw::c_int,
        c: ::std::os::raw::c_int,
        w: ::std::os::raw::c_int,
        read_pal_component: ::std::os::raw::c_int,
        dst_element_size: ::std::os::raw::c_int,
    );
}
unsafe extern "C" {
    pub fn av_read_image_line(
        dst: *mut u16,
        data: *mut *const u8,
        linesize: *const ::std::os::raw::c_int,
        desc: *const AVPixFmtDescriptor,
        x: ::std::os::raw::c_int,
        y: ::std::os::raw::c_int,
        c: ::std::os::raw::c_int,
        w: ::std::os::raw::c_int,
        read_pal_component: ::std::os::raw::c_int,
    );
}
unsafe extern "C" {
    pub fn av_write_image_line2(
        src: *const ::std::os::raw::c_void,
        data: *mut *mut u8,
        linesize: *const ::std::os::raw::c_int,
        desc: *const AVPixFmtDescriptor,
        x: ::std::os::raw::c_int,
        y: ::std::os::raw::c_int,
        c: ::std::os::raw::c_int,
        w: ::std::os::raw::c_int,
        src_element_size: ::std::os::raw::c_int,
    );
}
unsafe extern "C" {
    pub fn av_write_image_line(
        src: *const u16,
        data: *mut *mut u8,
        linesize: *const ::std::os::raw::c_int,
        desc: *const AVPixFmtDescriptor,
        x: ::std::os::raw::c_int,
        y: ::std::os::raw::c_int,
        c: ::std::os::raw::c_int,
        w: ::std::os::raw::c_int,
    );
}
unsafe extern "C" {
    pub fn av_pix_fmt_swap_endianness(pix_fmt: AVPixelFormat) -> AVPixelFormat;
}
unsafe extern "C" {
    pub fn av_get_pix_fmt_loss(
        dst_pix_fmt: AVPixelFormat,
        src_pix_fmt: AVPixelFormat,
        has_alpha: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_find_best_pix_fmt_of_2(
        dst_pix_fmt1: AVPixelFormat,
        dst_pix_fmt2: AVPixelFormat,
        src_pix_fmt: AVPixelFormat,
        has_alpha: ::std::os::raw::c_int,
        loss_ptr: *mut ::std::os::raw::c_int,
    ) -> AVPixelFormat;
}
unsafe extern "C" {
    pub fn av_image_fill_max_pixsteps(
        max_pixsteps: *mut ::std::os::raw::c_int,
        max_pixstep_comps: *mut ::std::os::raw::c_int,
        pixdesc: *const AVPixFmtDescriptor,
    );
}
unsafe extern "C" {
    pub fn av_image_get_linesize(
        pix_fmt: AVPixelFormat,
        width: ::std::os::raw::c_int,
        plane: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_image_fill_linesizes(
        linesizes: *mut ::std::os::raw::c_int,
        pix_fmt: AVPixelFormat,
        width: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_image_fill_plane_sizes(
        size: *mut usize,
        pix_fmt: AVPixelFormat,
        height: ::std::os::raw::c_int,
        linesizes: *const isize,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_image_fill_pointers(
        data: *mut *mut u8,
        pix_fmt: AVPixelFormat,
        height: ::std::os::raw::c_int,
        ptr: *mut u8,
        linesizes: *const ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_image_alloc(
        pointers: *mut *mut u8,
        linesizes: *mut ::std::os::raw::c_int,
        w: ::std::os::raw::c_int,
        h: ::std::os::raw::c_int,
        pix_fmt: AVPixelFormat,
        align: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_image_copy_plane(
        dst: *mut u8,
        dst_linesize: ::std::os::raw::c_int,
        src: *const u8,
        src_linesize: ::std::os::raw::c_int,
        bytewidth: ::std::os::raw::c_int,
        height: ::std::os::raw::c_int,
    );
}
unsafe extern "C" {
    pub fn av_image_copy_plane_uc_from(
        dst: *mut u8,
        dst_linesize: isize,
        src: *const u8,
        src_linesize: isize,
        bytewidth: isize,
        height: ::std::os::raw::c_int,
    );
}
unsafe extern "C" {
    pub fn av_image_copy(
        dst_data: *const *mut u8,
        dst_linesizes: *const ::std::os::raw::c_int,
        src_data: *const *const u8,
        src_linesizes: *const ::std::os::raw::c_int,
        pix_fmt: AVPixelFormat,
        width: ::std::os::raw::c_int,
        height: ::std::os::raw::c_int,
    );
}
unsafe extern "C" {
    pub fn av_image_copy_uc_from(
        dst_data: *const *mut u8,
        dst_linesizes: *const isize,
        src_data: *const *const u8,
        src_linesizes: *const isize,
        pix_fmt: AVPixelFormat,
        width: ::std::os::raw::c_int,
        height: ::std::os::raw::c_int,
    );
}
unsafe extern "C" {
    pub fn av_image_fill_arrays(
        dst_data: *mut *mut u8,
        dst_linesize: *mut ::std::os::raw::c_int,
        src: *const u8,
        pix_fmt: AVPixelFormat,
        width: ::std::os::raw::c_int,
        height: ::std::os::raw::c_int,
        align: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_image_get_buffer_size(
        pix_fmt: AVPixelFormat,
        width: ::std::os::raw::c_int,
        height: ::std::os::raw::c_int,
        align: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_image_copy_to_buffer(
        dst: *mut u8,
        dst_size: ::std::os::raw::c_int,
        src_data: *const *const u8,
        src_linesize: *const ::std::os::raw::c_int,
        pix_fmt: AVPixelFormat,
        width: ::std::os::raw::c_int,
        height: ::std::os::raw::c_int,
        align: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_image_check_size(
        w: ::std::os::raw::c_uint,
        h: ::std::os::raw::c_uint,
        log_offset: ::std::os::raw::c_int,
        log_ctx: *mut ::std::os::raw::c_void,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_image_check_size2(
        w: ::std::os::raw::c_uint,
        h: ::std::os::raw::c_uint,
        max_pixels: i64,
        pix_fmt: AVPixelFormat,
        log_offset: ::std::os::raw::c_int,
        log_ctx: *mut ::std::os::raw::c_void,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_image_check_sar(
        w: ::std::os::raw::c_uint,
        h: ::std::os::raw::c_uint,
        sar: AVRational,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_image_fill_black(
        dst_data: *const *mut u8,
        dst_linesize: *const isize,
        pix_fmt: AVPixelFormat,
        range: AVColorRange,
        width: ::std::os::raw::c_int,
        height: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn av_image_fill_color(
        dst_data: *const *mut u8,
        dst_linesize: *const isize,
        pix_fmt: AVPixelFormat,
        color: *const u32,
        width: ::std::os::raw::c_int,
        height: ::std::os::raw::c_int,
        flags: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn sws_get_class() -> *const AVClass;
}
pub const SwsDither_SWS_DITHER_NONE: SwsDither = 0;
pub const SwsDither_SWS_DITHER_AUTO: SwsDither = 1;
pub const SwsDither_SWS_DITHER_BAYER: SwsDither = 2;
pub const SwsDither_SWS_DITHER_ED: SwsDither = 3;
pub const SwsDither_SWS_DITHER_A_DITHER: SwsDither = 4;
pub const SwsDither_SWS_DITHER_X_DITHER: SwsDither = 5;
pub const SwsDither_SWS_DITHER_NB: SwsDither = 6;
pub const SwsDither_SWS_DITHER_MAX_ENUM: SwsDither = 2147483647;
pub type SwsDither = ::std::os::raw::c_int;
pub const SwsAlphaBlend_SWS_ALPHA_BLEND_NONE: SwsAlphaBlend = 0;
pub const SwsAlphaBlend_SWS_ALPHA_BLEND_UNIFORM: SwsAlphaBlend = 1;
pub const SwsAlphaBlend_SWS_ALPHA_BLEND_CHECKERBOARD: SwsAlphaBlend = 2;
pub const SwsAlphaBlend_SWS_ALPHA_BLEND_NB: SwsAlphaBlend = 3;
pub const SwsAlphaBlend_SWS_ALPHA_BLEND_MAX_ENUM: SwsAlphaBlend = 2147483647;
pub type SwsAlphaBlend = ::std::os::raw::c_int;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct SwsContext {
    pub av_class: *const AVClass,
    pub opaque: *mut ::std::os::raw::c_void,
    pub flags: ::std::os::raw::c_uint,
    pub scaler_params: [f64; 2usize],
    pub threads: ::std::os::raw::c_int,
    pub dither: SwsDither,
    pub alpha_blend: SwsAlphaBlend,
    pub gamma_flag: ::std::os::raw::c_int,
    pub src_w: ::std::os::raw::c_int,
    pub src_h: ::std::os::raw::c_int,
    pub dst_w: ::std::os::raw::c_int,
    pub dst_h: ::std::os::raw::c_int,
    pub src_format: ::std::os::raw::c_int,
    pub dst_format: ::std::os::raw::c_int,
    pub src_range: ::std::os::raw::c_int,
    pub dst_range: ::std::os::raw::c_int,
    pub src_v_chr_pos: ::std::os::raw::c_int,
    pub src_h_chr_pos: ::std::os::raw::c_int,
    pub dst_v_chr_pos: ::std::os::raw::c_int,
    pub dst_h_chr_pos: ::std::os::raw::c_int,
    pub intent: ::std::os::raw::c_int,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of SwsContext"][::std::mem::size_of::<SwsContext>() - 112usize];
    ["Alignment of SwsContext"][::std::mem::align_of::<SwsContext>() - 8usize];
    ["Offset of field: SwsContext::av_class"]
        [::std::mem::offset_of!(SwsContext, av_class) - 0usize];
    ["Offset of field: SwsContext::opaque"][::std::mem::offset_of!(SwsContext, opaque) - 8usize];
    ["Offset of field: SwsContext::flags"][::std::mem::offset_of!(SwsContext, flags) - 16usize];
    ["Offset of field: SwsContext::scaler_params"]
        [::std::mem::offset_of!(SwsContext, scaler_params) - 24usize];
    ["Offset of field: SwsContext::threads"][::std::mem::offset_of!(SwsContext, threads) - 40usize];
    ["Offset of field: SwsContext::dither"][::std::mem::offset_of!(SwsContext, dither) - 44usize];
    ["Offset of field: SwsContext::alpha_blend"]
        [::std::mem::offset_of!(SwsContext, alpha_blend) - 48usize];
    ["Offset of field: SwsContext::gamma_flag"]
        [::std::mem::offset_of!(SwsContext, gamma_flag) - 52usize];
    ["Offset of field: SwsContext::src_w"][::std::mem::offset_of!(SwsContext, src_w) - 56usize];
    ["Offset of field: SwsContext::src_h"][::std::mem::offset_of!(SwsContext, src_h) - 60usize];
    ["Offset of field: SwsContext::dst_w"][::std::mem::offset_of!(SwsContext, dst_w) - 64usize];
    ["Offset of field: SwsContext::dst_h"][::std::mem::offset_of!(SwsContext, dst_h) - 68usize];
    ["Offset of field: SwsContext::src_format"]
        [::std::mem::offset_of!(SwsContext, src_format) - 72usize];
    ["Offset of field: SwsContext::dst_format"]
        [::std::mem::offset_of!(SwsContext, dst_format) - 76usize];
    ["Offset of field: SwsContext::src_range"]
        [::std::mem::offset_of!(SwsContext, src_range) - 80usize];
    ["Offset of field: SwsContext::dst_range"]
        [::std::mem::offset_of!(SwsContext, dst_range) - 84usize];
    ["Offset of field: SwsContext::src_v_chr_pos"]
        [::std::mem::offset_of!(SwsContext, src_v_chr_pos) - 88usize];
    ["Offset of field: SwsContext::src_h_chr_pos"]
        [::std::mem::offset_of!(SwsContext, src_h_chr_pos) - 92usize];
    ["Offset of field: SwsContext::dst_v_chr_pos"]
        [::std::mem::offset_of!(SwsContext, dst_v_chr_pos) - 96usize];
    ["Offset of field: SwsContext::dst_h_chr_pos"]
        [::std::mem::offset_of!(SwsContext, dst_h_chr_pos) - 100usize];
    ["Offset of field: SwsContext::intent"][::std::mem::offset_of!(SwsContext, intent) - 104usize];
};
unsafe extern "C" {
    pub fn sws_alloc_context() -> *mut SwsContext;
}
unsafe extern "C" {
    pub fn sws_free_context(ctx: *mut *mut SwsContext);
}
unsafe extern "C" {
    pub fn sws_test_format(
        format: AVPixelFormat,
        output: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn sws_test_hw_format(format: AVPixelFormat) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn sws_test_colorspace(
        colorspace: AVColorSpace,
        output: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn sws_test_primaries(
        primaries: AVColorPrimaries,
        output: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn sws_test_transfer(
        trc: AVColorTransferCharacteristic,
        output: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn sws_test_frame(
        frame: *const AVFrame,
        output: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn sws_frame_setup(
        ctx: *mut SwsContext,
        dst: *const AVFrame,
        src: *const AVFrame,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn sws_is_noop(dst: *const AVFrame, src: *const AVFrame) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn sws_scale_frame(
        c: *mut SwsContext,
        dst: *mut AVFrame,
        src: *const AVFrame,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn sws_getCoefficients(colorspace: ::std::os::raw::c_int) -> *const ::std::os::raw::c_int;
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct SwsVector {
    pub coeff: *mut f64,
    pub length: ::std::os::raw::c_int,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of SwsVector"][::std::mem::size_of::<SwsVector>() - 16usize];
    ["Alignment of SwsVector"][::std::mem::align_of::<SwsVector>() - 8usize];
    ["Offset of field: SwsVector::coeff"][::std::mem::offset_of!(SwsVector, coeff) - 0usize];
    ["Offset of field: SwsVector::length"][::std::mem::offset_of!(SwsVector, length) - 8usize];
};
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct SwsFilter {
    pub lumH: *mut SwsVector,
    pub lumV: *mut SwsVector,
    pub chrH: *mut SwsVector,
    pub chrV: *mut SwsVector,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of SwsFilter"][::std::mem::size_of::<SwsFilter>() - 32usize];
    ["Alignment of SwsFilter"][::std::mem::align_of::<SwsFilter>() - 8usize];
    ["Offset of field: SwsFilter::lumH"][::std::mem::offset_of!(SwsFilter, lumH) - 0usize];
    ["Offset of field: SwsFilter::lumV"][::std::mem::offset_of!(SwsFilter, lumV) - 8usize];
    ["Offset of field: SwsFilter::chrH"][::std::mem::offset_of!(SwsFilter, chrH) - 16usize];
    ["Offset of field: SwsFilter::chrV"][::std::mem::offset_of!(SwsFilter, chrV) - 24usize];
};
unsafe extern "C" {
    pub fn sws_isSupportedInput(pix_fmt: AVPixelFormat) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn sws_isSupportedOutput(pix_fmt: AVPixelFormat) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn sws_isSupportedEndiannessConversion(pix_fmt: AVPixelFormat) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn sws_init_context(
        sws_context: *mut SwsContext,
        srcFilter: *mut SwsFilter,
        dstFilter: *mut SwsFilter,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn sws_freeContext(swsContext: *mut SwsContext);
}
unsafe extern "C" {
    pub fn sws_getContext(
        srcW: ::std::os::raw::c_int,
        srcH: ::std::os::raw::c_int,
        srcFormat: AVPixelFormat,
        dstW: ::std::os::raw::c_int,
        dstH: ::std::os::raw::c_int,
        dstFormat: AVPixelFormat,
        flags: ::std::os::raw::c_int,
        srcFilter: *mut SwsFilter,
        dstFilter: *mut SwsFilter,
        param: *const f64,
    ) -> *mut SwsContext;
}
unsafe extern "C" {
    pub fn sws_scale(
        c: *mut SwsContext,
        srcSlice: *const *const u8,
        srcStride: *const ::std::os::raw::c_int,
        srcSliceY: ::std::os::raw::c_int,
        srcSliceH: ::std::os::raw::c_int,
        dst: *const *mut u8,
        dstStride: *const ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn sws_frame_start(
        c: *mut SwsContext,
        dst: *mut AVFrame,
        src: *const AVFrame,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn sws_frame_end(c: *mut SwsContext);
}
unsafe extern "C" {
    pub fn sws_send_slice(
        c: *mut SwsContext,
        slice_start: ::std::os::raw::c_uint,
        slice_height: ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn sws_receive_slice(
        c: *mut SwsContext,
        slice_start: ::std::os::raw::c_uint,
        slice_height: ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn sws_receive_slice_alignment(c: *const SwsContext) -> ::std::os::raw::c_uint;
}
unsafe extern "C" {
    pub fn sws_setColorspaceDetails(
        c: *mut SwsContext,
        inv_table: *const ::std::os::raw::c_int,
        srcRange: ::std::os::raw::c_int,
        table: *const ::std::os::raw::c_int,
        dstRange: ::std::os::raw::c_int,
        brightness: ::std::os::raw::c_int,
        contrast: ::std::os::raw::c_int,
        saturation: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn sws_getColorspaceDetails(
        c: *mut SwsContext,
        inv_table: *mut *mut ::std::os::raw::c_int,
        srcRange: *mut ::std::os::raw::c_int,
        table: *mut *mut ::std::os::raw::c_int,
        dstRange: *mut ::std::os::raw::c_int,
        brightness: *mut ::std::os::raw::c_int,
        contrast: *mut ::std::os::raw::c_int,
        saturation: *mut ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
unsafe extern "C" {
    pub fn sws_allocVec(length: ::std::os::raw::c_int) -> *mut SwsVector;
}
unsafe extern "C" {
    pub fn sws_getGaussianVec(variance: f64, quality: f64) -> *mut SwsVector;
}
unsafe extern "C" {
    pub fn sws_scaleVec(a: *mut SwsVector, scalar: f64);
}
unsafe extern "C" {
    pub fn sws_normalizeVec(a: *mut SwsVector, height: f64);
}
unsafe extern "C" {
    pub fn sws_freeVec(a: *mut SwsVector);
}
unsafe extern "C" {
    pub fn sws_getDefaultFilter(
        lumaGBlur: f32,
        chromaGBlur: f32,
        lumaSharpen: f32,
        chromaSharpen: f32,
        chromaHShift: f32,
        chromaVShift: f32,
        verbose: ::std::os::raw::c_int,
    ) -> *mut SwsFilter;
}
unsafe extern "C" {
    pub fn sws_freeFilter(filter: *mut SwsFilter);
}
unsafe extern "C" {
    pub fn sws_getCachedContext(
        context: *mut SwsContext,
        srcW: ::std::os::raw::c_int,
        srcH: ::std::os::raw::c_int,
        srcFormat: AVPixelFormat,
        dstW: ::std::os::raw::c_int,
        dstH: ::std::os::raw::c_int,
        dstFormat: AVPixelFormat,
        flags: ::std::os::raw::c_int,
        srcFilter: *mut SwsFilter,
        dstFilter: *mut SwsFilter,
        param: *const f64,
    ) -> *mut SwsContext;
}
unsafe extern "C" {
    pub fn sws_convertPalette8ToPacked32(
        src: *const u8,
        dst: *mut u8,
        num_pixels: ::std::os::raw::c_int,
        palette: *const u8,
    );
}
unsafe extern "C" {
    pub fn sws_convertPalette8ToPacked24(
        src: *const u8,
        dst: *mut u8,
        num_pixels: ::std::os::raw::c_int,
        palette: *const u8,
    );
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AVCodecInternal {
    pub _address: u8,
}
