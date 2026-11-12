#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <objidl.h>
#include <gl/GL.h>
#include <gdiplus.h>
#include <d3d9.h>
#include <d3d11.h>
#include <d3d12.h>
#include <d3dcompiler.h>
#include <dxgi1_4.h>
#include <mfapi.h>
#include <mferror.h>
#include <mfidl.h>
#include <mfreadwrite.h>
#include <math.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <wchar.h>
#include <new>
#include <string>
#include <vector>

#include "third_party/minhook/include/MinHook.h"

#pragma comment(lib, "opengl32.lib")
#pragma comment(lib, "gdi32.lib")
#pragma comment(lib, "user32.lib")
#pragma comment(lib, "gdiplus.lib")
#pragma comment(lib, "d3d9.lib")
#pragma comment(lib, "d3d11.lib")
#pragma comment(lib, "d3dcompiler.lib")
#pragma comment(lib, "mfplat.lib")
#pragma comment(lib, "mfreadwrite.lib")
#pragma comment(lib, "mfuuid.lib")
#pragma comment(lib, "ole32.lib")

#ifndef GL_BGRA
#define GL_BGRA 0x80E1
#endif
#ifndef GL_CLAMP_TO_EDGE
#define GL_CLAMP_TO_EDGE 0x812F
#endif
#ifndef GL_TEXTURE0
#define GL_TEXTURE0 0x84C0
#endif
#ifndef GL_FRAMEBUFFER
#define GL_FRAMEBUFFER 0x8D40
#endif
#ifndef GL_FRAMEBUFFER_BINDING
#define GL_FRAMEBUFFER_BINDING 0x8CA6
#endif
#ifndef GL_CURRENT_PROGRAM
#define GL_CURRENT_PROGRAM 0x8B8D
#endif
#ifndef GL_CONTEXT_PROFILE_MASK
#define GL_CONTEXT_PROFILE_MASK 0x9126
#endif
#ifndef GL_CONTEXT_CORE_PROFILE_BIT
#define GL_CONTEXT_CORE_PROFILE_BIT 0x00000001
#endif
#ifndef GL_ARRAY_BUFFER
#define GL_ARRAY_BUFFER 0x8892
#endif
#ifndef GL_ARRAY_BUFFER_BINDING
#define GL_ARRAY_BUFFER_BINDING 0x8894
#endif
#ifndef GL_VERTEX_ARRAY_BINDING
#define GL_VERTEX_ARRAY_BINDING 0x85B5
#endif
#ifndef GL_ACTIVE_TEXTURE
#define GL_ACTIVE_TEXTURE 0x84E0
#endif
#ifndef GL_TEXTURE_BINDING_2D
#define GL_TEXTURE_BINDING_2D 0x8069
#endif
#ifndef GL_STATIC_DRAW
#define GL_STATIC_DRAW 0x88E4
#endif
#ifndef GL_BLEND_SRC_RGB
#define GL_BLEND_SRC_RGB 0x80C9
#endif
#ifndef GL_BLEND_DST_RGB
#define GL_BLEND_DST_RGB 0x80C8
#endif
#ifndef GL_BLEND_SRC_ALPHA
#define GL_BLEND_SRC_ALPHA 0x80CB
#endif
#ifndef GL_BLEND_DST_ALPHA
#define GL_BLEND_DST_ALPHA 0x80CA
#endif
#ifndef GL_BLEND_EQUATION_RGB
#define GL_BLEND_EQUATION_RGB 0x8009
#endif
#ifndef GL_BLEND_EQUATION_ALPHA
#define GL_BLEND_EQUATION_ALPHA 0x883D
#endif
#ifndef GL_COLOR_WRITEMASK
#define GL_COLOR_WRITEMASK 0x0C23
#endif
#ifndef GL_COLOR_LOGIC_OP
#define GL_COLOR_LOGIC_OP 0x0BF2
#endif
#ifndef GL_FRAMEBUFFER_SRGB
#define GL_FRAMEBUFFER_SRGB 0x8DB9
#endif
#ifndef GL_STENCIL_TEST
#define GL_STENCIL_TEST 0x0B90
#endif
#ifndef GL_POLYGON_MODE
#define GL_POLYGON_MODE 0x0B40
#endif
#ifndef GL_FUNC_ADD
#define GL_FUNC_ADD 0x8006
#endif
#ifndef GL_VERTEX_SHADER
#define GL_VERTEX_SHADER 0x8B31
#endif
#ifndef GL_FRAGMENT_SHADER
#define GL_FRAGMENT_SHADER 0x8B30
#endif
#ifndef GL_COMPILE_STATUS
#define GL_COMPILE_STATUS 0x8B81
#endif
#ifndef GL_LINK_STATUS
#define GL_LINK_STATUS 0x8B82
#endif

using namespace Gdiplus;


#define IR_CTL_NAME L"Local\\InstantReplay.Control.v1"
#define IR_READY_NAME L"Local\\InstantReplay.OverlayHook.Ready.v1"

constexpr uint32_t IR_HOOK_MAGIC = 0x49524652;
constexpr uint32_t IR_HOOK_VERSION = 2;

enum : uint32_t
{
    IR_CTL_MAGIC = 0,
    IR_CTL_VERSION = 4,
    IR_CTL_CAPTURE_ACTIVE = 8,
    IR_CTL_CAPTURE_FPS = 12,
    IR_CTL_OVERLAY_VISIBLE = 16,
    IR_CTL_OVERLAY_STATE = 20,
    IR_CTL_OVERLAY_PROGRESS = 24,
    IR_CTL_OVERLAY_SECONDS = 28,
    IR_CTL_OVERLAY_SEQ = 32,
    IR_CTL_DEBUG_LOG = 36,
    IR_CTL_TITLE = 40,
    IR_CTL_DETAIL = 168,
    IR_CTL_ICON_PATH = 424,
    IR_CTL_LOG_PATH = 944,
    IR_CTL_ANIMATION_PATH = 1464,
    IR_CTL_SIZE = 2048,
};

constexpr float IR_CARD_W = 382.0f;
constexpr float IR_CARD_H = 108.0f;
constexpr float IR_CARD_MARGIN = 24.0f;

// Motion and render pacing (units are 100 ns, like qpc_100ns()).
// The card used to crawl in over 3.2 s on a cubic ease-out whose long tail moved by sub-pixel amounts,
// which a whole-pixel quad turned into visible stepping. A short decelerating glide with a sub-pixel
// quad position reads as smooth motion.
constexpr int64_t IR_SLIDE_IN_100NS = 6500000LL;       // 650 ms
constexpr int64_t IR_SLIDE_OUT_100NS = 4500000LL;      // 450 ms
constexpr int64_t IR_HIDE_TAIL_100NS = 5500000LL;      // keep drawing until the slide-out has finished
constexpr int64_t IR_ANIM_MIN_INTERVAL_100NS = 160000LL; // video frames: at most ~60 card re-renders/s
constexpr int64_t IR_FADE_INTERVAL_100NS = 333333LL;     // text/bar fades: ~30 card re-renders/s


static HMODULE g_self;
static int64_t g_qpc_freq = 1;
static volatile LONG g_shutting_down = 0;
static volatile LONG g_active_present = 0;
static volatile LONG g_shutdown_started = 0;
static volatile LONG g_shutdown_complete = 0;

struct PresentGuard
{
    PresentGuard() { InterlockedIncrement(&g_active_present); }
    ~PresentGuard() { InterlockedDecrement(&g_active_present); }
};
static volatile LONG g_gdi_ready = 0;
static thread_local int t_depth = 0;

static inline int64_t qpc_ticks()
{
    LARGE_INTEGER v;
    QueryPerformanceCounter(&v);
    return v.QuadPart;
}
static inline int64_t qpc_100ns()
{
    return (qpc_ticks() * 10000000LL) / g_qpc_freq;
}
static inline float ir_clamp01(float v) { return v < 0 ? 0 : (v > 1 ? 1 : v); }


static HANDLE g_log = INVALID_HANDLE_VALUE;
static SRWLOCK g_log_lock = SRWLOCK_INIT;
static wchar_t g_log_path[260];

static void log_close()
{
    AcquireSRWLockExclusive(&g_log_lock);
    HANDLE h = g_log;
    g_log = INVALID_HANDLE_VALUE;
    ReleaseSRWLockExclusive(&g_log_lock);
    if (h != INVALID_HANDLE_VALUE)
        CloseHandle(h);
    g_log_path[0] = 0;
}

static void hook_log(const wchar_t *fmt, ...)
{
    if (g_log == INVALID_HANDLE_VALUE)
        return;

    wchar_t msg[900];
    va_list ap;
    va_start(ap, fmt);
    _vsnwprintf_s(msg, _countof(msg), _TRUNCATE, fmt, ap);
    va_end(ap);

    SYSTEMTIME st;
    GetLocalTime(&st);
    wchar_t line[1024];
    int n = _snwprintf_s(line, _countof(line), _TRUNCATE,
                         L"[%02u:%02u:%02u.%03u] [DEBUG] [hook:overlay] %s\r\n",
                         static_cast<unsigned>(st.wHour), static_cast<unsigned>(st.wMinute),
                         static_cast<unsigned>(st.wSecond), static_cast<unsigned>(st.wMilliseconds),
                         msg);
    if (n <= 0)
        return;

    char utf8[3072];
    int bytes = WideCharToMultiByte(CP_UTF8, 0, line, n, utf8, sizeof(utf8), nullptr, nullptr);
    if (bytes <= 0)
        return;

    AcquireSRWLockExclusive(&g_log_lock);
    if (g_log != INVALID_HANDLE_VALUE)
    {
        DWORD written = 0;
        WriteFile(g_log, utf8, static_cast<DWORD>(bytes), &written, nullptr);
    }
    ReleaseSRWLockExclusive(&g_log_lock);
}


struct ControlSnapshot
{
    bool valid;
    uint32_t capture_active;
    uint32_t overlay_visible;
    uint32_t overlay_state;
    int32_t overlay_progress;
    int32_t overlay_seconds;
    uint32_t overlay_seq;
    uint32_t debug_log;
    wchar_t log_path[260];
    std::wstring title;
    std::wstring detail;
    std::wstring icon_path;
    std::wstring animation_path;
    bool strings_changed;
};

static HANDLE g_ctl_handle = nullptr;
static uint8_t *g_ctl = nullptr;
static int64_t g_ctl_retry_at = 0;
static uint32_t g_copied_seq = 0xFFFFFFFFu;

static bool control_open()
{
    if (g_ctl)
        return true;
    int64_t now = qpc_100ns();
    if (now < g_ctl_retry_at)
        return false;
    g_ctl_retry_at = now + 10000000LL;
    HANDLE h = OpenFileMappingW(FILE_MAP_READ | FILE_MAP_WRITE, FALSE, IR_CTL_NAME);
    if (!h)
        return false;
    void *p = MapViewOfFile(h, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, 0);
    if (!p)
    {
        CloseHandle(h);
        return false;
    }
    g_ctl_handle = h;
    g_ctl = static_cast<uint8_t *>(p);
    return true;
}

static void control_close()
{
    if (g_ctl)
    {
        UnmapViewOfFile(g_ctl);
        g_ctl = nullptr;
    }
    if (g_ctl_handle)
    {
        CloseHandle(g_ctl_handle);
        g_ctl_handle = nullptr;
    }
}

static uint32_t rd_u32(const uint8_t *p, uint32_t off)
{
    return *reinterpret_cast<const volatile uint32_t *>(p + off);
}
static int32_t rd_i32(const uint8_t *p, uint32_t off)
{
    return *reinterpret_cast<const volatile int32_t *>(p + off);
}

static void copy_wide(const uint16_t *src, size_t cap, std::wstring &dst)
{
    size_t n = 0;
    while (n < cap && src[n] != 0)
        n++;
    dst.assign(reinterpret_cast<const wchar_t *>(src), n);
}

static void control_read(ControlSnapshot &s)
{
    s.valid = false;
    s.capture_active = 0;
    s.overlay_visible = 0;
    s.overlay_state = 0;
    s.overlay_progress = -1;
    s.overlay_seconds = 0;
    s.overlay_seq = 0;
    s.debug_log = 0;
    s.log_path[0] = 0;
    s.strings_changed = false;
    if (!control_open())
        return;
    const uint8_t *p = g_ctl;
    if (rd_u32(p, IR_CTL_MAGIC) != IR_HOOK_MAGIC ||
        rd_u32(p, IR_CTL_VERSION) != IR_HOOK_VERSION)
    {
        return;
    }
    s.overlay_seq = rd_u32(p, IR_CTL_OVERLAY_SEQ);
    s.capture_active = rd_u32(p, IR_CTL_CAPTURE_ACTIVE);
    s.overlay_visible = rd_u32(p, IR_CTL_OVERLAY_VISIBLE);
    s.overlay_state = rd_u32(p, IR_CTL_OVERLAY_STATE);
    s.overlay_progress = rd_i32(p, IR_CTL_OVERLAY_PROGRESS);
    s.overlay_seconds = rd_i32(p, IR_CTL_OVERLAY_SECONDS);
    s.debug_log = rd_u32(p, IR_CTL_DEBUG_LOG);
    memcpy(s.log_path, p + IR_CTL_LOG_PATH, sizeof(s.log_path));
    s.log_path[259] = 0;

    s.strings_changed = (s.overlay_seq != g_copied_seq);
    if (s.strings_changed)
    {
        copy_wide(reinterpret_cast<const uint16_t *>(p + IR_CTL_TITLE), 64, s.title);
        copy_wide(reinterpret_cast<const uint16_t *>(p + IR_CTL_DETAIL), 128, s.detail);
        copy_wide(reinterpret_cast<const uint16_t *>(p + IR_CTL_ICON_PATH), 260, s.icon_path);
        copy_wide(reinterpret_cast<const uint16_t *>(p + IR_CTL_ANIMATION_PATH), 260,
                  s.animation_path);
        g_copied_seq = s.overlay_seq;
    }
    s.valid = true;
}

static void log_ensure(const ControlSnapshot &ctl)
{
    if (!ctl.debug_log || ctl.log_path[0] == 0)
    {
        if (g_log != INVALID_HANDLE_VALUE)
            log_close();
        return;
    }
    if (g_log != INVALID_HANDLE_VALUE && wcscmp(g_log_path, ctl.log_path) == 0)
        return;

    log_close();
    HANDLE h = CreateFileW(ctl.log_path, FILE_APPEND_DATA,
                           FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, nullptr,
                           OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (h == INVALID_HANDLE_VALUE)
        return;

    AcquireSRWLockExclusive(&g_log_lock);
    g_log = h;
    wcscpy_s(g_log_path, ctl.log_path);
    ReleaseSRWLockExclusive(&g_log_lock);
}


typedef void(APIENTRY *PFN_ActiveTexture)(GLenum);
typedef void(APIENTRY *PFN_UseProgram)(GLuint);
typedef void(APIENTRY *PFN_BindFramebuffer)(GLenum, GLuint);
typedef void(APIENTRY *PFN_BlendFuncSeparate)(GLenum, GLenum, GLenum, GLenum);
typedef void(APIENTRY *PFN_BlendEquationSeparate)(GLenum, GLenum);

typedef ptrdiff_t IR_GLsizeiptr;
typedef GLuint(APIENTRY *PFN_CreateShader)(GLenum);
typedef void(APIENTRY *PFN_ShaderSource)(GLuint, GLsizei, const char *const *, const GLint *);
typedef void(APIENTRY *PFN_CompileShader)(GLuint);
typedef void(APIENTRY *PFN_GetShaderiv)(GLuint, GLenum, GLint *);
typedef void(APIENTRY *PFN_GetShaderInfoLog)(GLuint, GLsizei, GLsizei *, char *);
typedef GLuint(APIENTRY *PFN_CreateProgram)(void);
typedef void(APIENTRY *PFN_AttachShader)(GLuint, GLuint);
typedef void(APIENTRY *PFN_LinkProgram)(GLuint);
typedef void(APIENTRY *PFN_GetProgramiv)(GLuint, GLenum, GLint *);
typedef void(APIENTRY *PFN_GetProgramInfoLog)(GLuint, GLsizei, GLsizei *, char *);
typedef void(APIENTRY *PFN_DeleteShader)(GLuint);
typedef void(APIENTRY *PFN_DeleteProgram)(GLuint);
typedef GLint(APIENTRY *PFN_GetAttribLocation)(GLuint, const char *);
typedef GLint(APIENTRY *PFN_GetUniformLocation)(GLuint, const char *);
typedef void(APIENTRY *PFN_Uniform1i)(GLint, GLint);
typedef void(APIENTRY *PFN_Uniform2f)(GLint, GLfloat, GLfloat);
typedef void(APIENTRY *PFN_Uniform4f)(GLint, GLfloat, GLfloat, GLfloat, GLfloat);
typedef void(APIENTRY *PFN_GenBuffers)(GLsizei, GLuint *);
typedef void(APIENTRY *PFN_BindBuffer)(GLenum, GLuint);
typedef void(APIENTRY *PFN_BufferData)(GLenum, IR_GLsizeiptr, const void *, GLenum);
typedef void(APIENTRY *PFN_DeleteBuffers)(GLsizei, const GLuint *);
typedef void(APIENTRY *PFN_GenVertexArrays)(GLsizei, GLuint *);
typedef void(APIENTRY *PFN_BindVertexArray)(GLuint);
typedef void(APIENTRY *PFN_DeleteVertexArrays)(GLsizei, const GLuint *);
typedef void(APIENTRY *PFN_EnableVertexAttribArray)(GLuint);
typedef void(APIENTRY *PFN_VertexAttribPointer)(GLuint, GLint, GLenum, GLboolean, GLsizei,
                                                const void *);

static PFN_ActiveTexture p_glActiveTexture;
static PFN_UseProgram p_glUseProgram;
static PFN_BindFramebuffer p_glBindFramebuffer;
static PFN_BlendFuncSeparate p_glBlendFuncSeparate;
static PFN_BlendEquationSeparate p_glBlendEquationSeparate;
static PFN_CreateShader p_glCreateShader;
static PFN_ShaderSource p_glShaderSource;
static PFN_CompileShader p_glCompileShader;
static PFN_GetShaderiv p_glGetShaderiv;
static PFN_GetShaderInfoLog p_glGetShaderInfoLog;
static PFN_CreateProgram p_glCreateProgram;
static PFN_AttachShader p_glAttachShader;
static PFN_LinkProgram p_glLinkProgram;
static PFN_GetProgramiv p_glGetProgramiv;
static PFN_GetProgramInfoLog p_glGetProgramInfoLog;
static PFN_DeleteShader p_glDeleteShader;
static PFN_DeleteProgram p_glDeleteProgram;
static PFN_GetAttribLocation p_glGetAttribLocation;
static PFN_GetUniformLocation p_glGetUniformLocation;
static PFN_Uniform1i p_glUniform1i;
static PFN_Uniform2f p_glUniform2f;
static PFN_Uniform4f p_glUniform4f;
static PFN_GenBuffers p_glGenBuffers;
static PFN_BindBuffer p_glBindBuffer;
static PFN_BufferData p_glBufferData;
static PFN_DeleteBuffers p_glDeleteBuffers;
static PFN_GenVertexArrays p_glGenVertexArrays;
static PFN_BindVertexArray p_glBindVertexArray;
static PFN_DeleteVertexArrays p_glDeleteVertexArrays;
static PFN_EnableVertexAttribArray p_glEnableVertexAttribArray;
static PFN_VertexAttribPointer p_glVertexAttribPointer;
static bool g_gl_loaded;

static void load_gl()
{
    if (g_gl_loaded)
        return;
    g_gl_loaded = true;
    if (!GetModuleHandleW(L"opengl32.dll"))
        return;
    p_glActiveTexture = (PFN_ActiveTexture)wglGetProcAddress("glActiveTexture");
    p_glUseProgram = (PFN_UseProgram)wglGetProcAddress("glUseProgram");
    p_glBindFramebuffer = (PFN_BindFramebuffer)wglGetProcAddress("glBindFramebuffer");
    p_glBlendFuncSeparate = (PFN_BlendFuncSeparate)wglGetProcAddress("glBlendFuncSeparate");
    p_glBlendEquationSeparate =
        (PFN_BlendEquationSeparate)wglGetProcAddress("glBlendEquationSeparate");
    p_glCreateShader = (PFN_CreateShader)wglGetProcAddress("glCreateShader");
    p_glShaderSource = (PFN_ShaderSource)wglGetProcAddress("glShaderSource");
    p_glCompileShader = (PFN_CompileShader)wglGetProcAddress("glCompileShader");
    p_glGetShaderiv = (PFN_GetShaderiv)wglGetProcAddress("glGetShaderiv");
    p_glGetShaderInfoLog = (PFN_GetShaderInfoLog)wglGetProcAddress("glGetShaderInfoLog");
    p_glCreateProgram = (PFN_CreateProgram)wglGetProcAddress("glCreateProgram");
    p_glAttachShader = (PFN_AttachShader)wglGetProcAddress("glAttachShader");
    p_glLinkProgram = (PFN_LinkProgram)wglGetProcAddress("glLinkProgram");
    p_glGetProgramiv = (PFN_GetProgramiv)wglGetProcAddress("glGetProgramiv");
    p_glGetProgramInfoLog = (PFN_GetProgramInfoLog)wglGetProcAddress("glGetProgramInfoLog");
    p_glDeleteShader = (PFN_DeleteShader)wglGetProcAddress("glDeleteShader");
    p_glDeleteProgram = (PFN_DeleteProgram)wglGetProcAddress("glDeleteProgram");
    p_glGetAttribLocation = (PFN_GetAttribLocation)wglGetProcAddress("glGetAttribLocation");
    p_glGetUniformLocation = (PFN_GetUniformLocation)wglGetProcAddress("glGetUniformLocation");
    p_glUniform1i = (PFN_Uniform1i)wglGetProcAddress("glUniform1i");
    p_glUniform2f = (PFN_Uniform2f)wglGetProcAddress("glUniform2f");
    p_glUniform4f = (PFN_Uniform4f)wglGetProcAddress("glUniform4f");
    p_glGenBuffers = (PFN_GenBuffers)wglGetProcAddress("glGenBuffers");
    p_glBindBuffer = (PFN_BindBuffer)wglGetProcAddress("glBindBuffer");
    p_glBufferData = (PFN_BufferData)wglGetProcAddress("glBufferData");
    p_glDeleteBuffers = (PFN_DeleteBuffers)wglGetProcAddress("glDeleteBuffers");
    p_glGenVertexArrays = (PFN_GenVertexArrays)wglGetProcAddress("glGenVertexArrays");
    p_glBindVertexArray = (PFN_BindVertexArray)wglGetProcAddress("glBindVertexArray");
    p_glDeleteVertexArrays = (PFN_DeleteVertexArrays)wglGetProcAddress("glDeleteVertexArrays");
    p_glEnableVertexAttribArray =
        (PFN_EnableVertexAttribArray)wglGetProcAddress("glEnableVertexAttribArray");
    p_glVertexAttribPointer =
        (PFN_VertexAttribPointer)wglGetProcAddress("glVertexAttribPointer");
}


struct OverlayState
{
    int64_t last_seq = -1;
    bool visible = false;
    int state = 0;
    int progress = -1;
    int seconds = 0;
    std::wstring title;
    std::wstring detail;
    std::wstring icon_path;
    std::wstring animation_path;
    int64_t animation_at = 0;

    int64_t show_at = 0;
    int64_t hide_at = 0;
    int64_t morph_at = 0;
    bool morphing = false;
    int64_t bar_at = 0;
    bool bar_fading = false;

    Bitmap *bitmap = nullptr;
    int bitmap_w = 0;
    int bitmap_h = 0;
    // Everything on the card except the icon cell: the completion clip only changes that cell,
    // so it is cached here and blitted under the new frame instead of re-running GDI+ text.
    Bitmap *static_bitmap = nullptr;
    bool static_valid = false;
    GLuint texture = 0;
    int texture_w = 0;
    int texture_h = 0;
    std::vector<uint8_t> upload;

    GLuint program = 0;
    GLuint vao = 0;
    GLuint vbo = 0;
    GLint u_rect = -1;
    GLint u_viewport = -1;
    GLint u_tex = -1;
    bool modern_initialized = false;
    bool modern_failed = false;
    int core_profile = -1;

    HGLRC context = nullptr;

    int64_t last_render_at = 0;
    int rendered_anim_index = -2;

    bool dirty = true;
    bool render_error_logged = false;
    bool upload_error_logged = false;
};

static OverlayState g_ov;

static std::wstring module_dir()
{
    wchar_t path[MAX_PATH];
    DWORD n = GetModuleFileNameW(g_self, path, MAX_PATH);
    if (n == 0 || n >= MAX_PATH)
        return std::wstring();
    std::wstring s(path, n);
    size_t slash = s.find_last_of(L"\\/");
    return slash == std::wstring::npos ? std::wstring() : s.substr(0, slash);
}

static std::wstring default_icon_path()
{
    std::wstring dir = module_dir();
    if (dir.empty())
        return std::wstring();
    for (int i = 0; i < 2; i++)
    {
        size_t slash = dir.find_last_of(L"\\/");
        if (slash == std::wstring::npos)
            break;
        dir.resize(slash);
    }
    const wchar_t *candidates[] = {
        L"\\assets\\icons\\clapperboard.png",
        L"\\assets\\icons\\icon.ico",
        L"\\assets\\icon.ico",
        L"\\icon.ico",
    };
    for (const wchar_t *suffix : candidates)
    {
        std::wstring candidate = dir + suffix;
        if (GetFileAttributesW(candidate.c_str()) != INVALID_FILE_ATTRIBUTES)
        {
            return candidate;
        }
    }
    return std::wstring();
}

struct AnimationFrame
{
    int64_t timestamp = 0;
    Bitmap *bitmap = nullptr;
};

static std::vector<AnimationFrame> g_animation_frames;
static int64_t g_animation_duration = 30000000LL;
static std::wstring g_loaded_animation;
static std::wstring g_attempted_animation;
static bool g_mf_started = false;

// The completion clip is decoded on a background thread and handed over through this staging area.
// Decoding it inside the game's Present call (as this used to) froze the game for ~2 s.
static SRWLOCK g_anim_lock = SRWLOCK_INIT;
static std::vector<AnimationFrame> g_anim_staging;
static int64_t g_anim_staging_duration = 0;
static std::wstring g_anim_staging_path;
static std::wstring g_anim_worker_path;
static volatile LONG g_anim_loading = 0;
static volatile LONG g_anim_ready = 0;
static HANDLE g_anim_thread = nullptr;

template <typename T>
static void ir_release(T *&value)
{
    if (value)
    {
        value->Release();
        value = nullptr;
    }
}

static void animation_clear()
{
    for (auto &frame : g_animation_frames)
        delete frame.bitmap;
    g_animation_frames.clear();
    g_loaded_animation.clear();
    g_animation_duration = 30000000LL;
}

static void animation_free_frames(std::vector<AnimationFrame> &frames)
{
    for (auto &frame : frames)
        delete frame.bitmap;
    frames.clear();
}

// Runs on the worker thread only: touches nothing but its arguments.
static bool animation_decode(const std::wstring &path, std::vector<AnimationFrame> &frames,
                             int64_t &duration)
{
    if (path.empty())
        return false;

    if (!g_mf_started)
    {
        if (FAILED(MFStartup(MF_VERSION, MFSTARTUP_LITE)))
            return false;
        g_mf_started = true;
    }

    IMFAttributes *attributes = nullptr;
    IMFSourceReader *reader = nullptr;
    IMFMediaType *type = nullptr;
    HRESULT hr = MFCreateAttributes(&attributes, 2);
    if (SUCCEEDED(hr))
    {
        attributes->SetUINT32(MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, TRUE);
        attributes->SetUINT32(MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, TRUE);
        hr = MFCreateSourceReaderFromURL(path.c_str(), attributes, &reader);
    }
    if (SUCCEEDED(hr))
        hr = MFCreateMediaType(&type);
    if (SUCCEEDED(hr))
        hr = type->SetGUID(MF_MT_MAJOR_TYPE, MFMediaType_Video);
    if (SUCCEEDED(hr))
        hr = type->SetGUID(MF_MT_SUBTYPE, MFVideoFormat_RGB32);
    if (SUCCEEDED(hr))
        MFSetAttributeSize(type, MF_MT_FRAME_SIZE, 84, 84);
    if (SUCCEEDED(hr))
        hr = reader->SetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM, nullptr, type);
    if (FAILED(hr) && reader && type)
    {
        type->DeleteItem(MF_MT_FRAME_SIZE);
        hr = reader->SetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM, nullptr, type);
    }

    UINT32 width = 0, height = 0;
    IMFMediaType *current = nullptr;
    if (SUCCEEDED(hr))
        hr = reader->GetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM, &current);
    if (SUCCEEDED(hr))
        hr = MFGetAttributeSize(current, MF_MT_FRAME_SIZE, &width, &height);
    // MF delivers RGB32 as a bottom-up DIB unless the type says otherwise.
    bool bottom_up = true;
    if (current)
    {
        UINT32 stride_attr = 0;
        if (SUCCEEDED(current->GetUINT32(MF_MT_DEFAULT_STRIDE, &stride_attr)))
            bottom_up = static_cast<LONG>(stride_attr) < 0;
    }
    ir_release(current);

    int64_t first_timestamp = -1;
    while (SUCCEEDED(hr) && !g_shutting_down)
    {
        DWORD stream = 0, flags = 0;
        LONGLONG timestamp = 0;
        IMFSample *sample = nullptr;
        hr = reader->ReadSample(MF_SOURCE_READER_FIRST_VIDEO_STREAM, 0, &stream, &flags,
                                &timestamp, &sample);
        if (FAILED(hr) || (flags & MF_SOURCE_READERF_ENDOFSTREAM))
        {
            ir_release(sample);
            break;
        }
        if (!sample)
            continue;

        IMFMediaBuffer *buffer = nullptr;
        BYTE *data = nullptr;
        DWORD length = 0;
        bool locked = false;
        if (SUCCEEDED(sample->ConvertToContiguousBuffer(&buffer)) && buffer)
            locked = SUCCEEDED(buffer->Lock(&data, nullptr, &length));
        if (locked && width && height && length >= width * height * 4)
        {
            if (first_timestamp < 0)
                first_timestamp = timestamp;
            BYTE *scan0 = bottom_up ? data + static_cast<size_t>(height - 1) * width * 4 : data;
            INT stride = static_cast<INT>(width * 4);
            Bitmap source(static_cast<INT>(width), static_cast<INT>(height),
                          bottom_up ? -stride : stride, PixelFormat32bppRGB, scan0);
            Bitmap *scaled = nullptr;
            try
            {
                scaled = new Bitmap(84, 84, PixelFormat32bppPARGB);
            }
            catch (...)
            {
                scaled = nullptr;
            }
            if (scaled && scaled->GetLastStatus() == Ok)
            {
                Graphics graphics(scaled);
                graphics.SetInterpolationMode(InterpolationModeHighQualityBicubic);
                graphics.Clear(Color(0, 0, 0, 0));
                graphics.DrawImage(&source, Rect(0, 0, 84, 84));
                frames.push_back({timestamp - first_timestamp, scaled});
            }
            else
            {
                delete scaled;
            }
        }
        if (locked)
            buffer->Unlock();
        ir_release(buffer);
        ir_release(sample);
    }

    ir_release(type);
    ir_release(reader);
    ir_release(attributes);
    if (frames.empty())
        return false;
    duration = 30000000LL;
    if (frames.size() > 1)
    {
        int64_t step = frames.back().timestamp - frames[frames.size() - 2].timestamp;
        duration = frames.back().timestamp + (step > 0 ? step : 333333LL);
    }
    return true;
}

static DWORD WINAPI animation_worker(LPVOID)
{
    const bool com = SUCCEEDED(CoInitializeEx(nullptr, COINIT_MULTITHREADED));
    std::vector<AnimationFrame> frames;
    int64_t duration = 30000000LL;
    const std::wstring path = g_anim_worker_path;

    bool ok = false;
    try
    {
        ok = !g_shutting_down && animation_decode(path, frames, duration);
    }
    catch (...)
    {
        ok = false;
    }

    if (ok && !g_shutting_down)
    {
        AcquireSRWLockExclusive(&g_anim_lock);
        animation_free_frames(g_anim_staging);
        g_anim_staging.swap(frames);
        g_anim_staging_duration = duration;
        g_anim_staging_path = path;
        ReleaseSRWLockExclusive(&g_anim_lock);
        InterlockedExchange(&g_anim_ready, 1);
    }
    else
    {
        animation_free_frames(frames);
    }

    InterlockedExchange(&g_anim_loading, 0);
    if (com)
        CoUninitialize();
    return 0;
}

static void animation_worker_join()
{
    if (g_anim_thread)
    {
        WaitForSingleObject(g_anim_thread, 3000);
        CloseHandle(g_anim_thread);
        g_anim_thread = nullptr;
    }
    AcquireSRWLockExclusive(&g_anim_lock);
    animation_free_frames(g_anim_staging);
    ReleaseSRWLockExclusive(&g_anim_lock);
}

static int animation_frame_index(int64_t now)
{
    if (g_animation_frames.empty() || g_ov.animation_at <= 0)
        return -1;
    const int64_t duration = g_animation_duration > 0 ? g_animation_duration : 1;
    int64_t elapsed = (now - g_ov.animation_at) % duration;
    if (elapsed < 0)
        elapsed = 0;
    size_t selected = 0;
    for (size_t i = 1; i < g_animation_frames.size(); ++i)
    {
        if (g_animation_frames[i].timestamp > elapsed)
            break;
        selected = i;
    }
    return static_cast<int>(selected);
}

static Bitmap *animation_frame(int64_t now)
{
    const int index = animation_frame_index(now);
    return index < 0 ? nullptr : g_animation_frames[index].bitmap;
}

// Called on every sync (render thread). It never decodes anything itself: it adopts a finished
// background decode, or starts one. Decoding starts as soon as the card is on screen, so the clip
// is normally ready long before the success state (the "saving" card precedes it by seconds).
// A failed decode is remembered for this card only, so a broken asset is not retried endlessly.
static void animation_ensure()
{
    if (g_ov.animation_path.empty())
        return;

    if (InterlockedCompareExchange(&g_anim_ready, 0, 1) == 1)
    {
        AcquireSRWLockExclusive(&g_anim_lock);
        if (g_anim_staging_path == g_ov.animation_path && !g_anim_staging.empty())
        {
            animation_clear();
            g_animation_frames.swap(g_anim_staging);
            g_animation_duration = g_anim_staging_duration;
            g_loaded_animation = g_anim_staging_path;
            g_anim_staging.clear();
            const size_t frame_count = g_animation_frames.size();
            ReleaseSRWLockExclusive(&g_anim_lock);
            hook_log(L"loaded completion animation (%zu frames)", frame_count);
            if (g_ov.state == 2)
                g_ov.animation_at = qpc_100ns();
            g_ov.dirty = true;
            return;
        }
        animation_free_frames(g_anim_staging);
        ReleaseSRWLockExclusive(&g_anim_lock);
    }

    // Decode as soon as the path is known, whether or not the card is on screen yet. The clip is
    // tiny and decodes on a background thread, so it is ready long before the success state
    // needs it — this is what removes the first-frame stall at encoding completion. A failed
    // decode is still remembered per card and retried when the success state arrives.
    if (!g_animation_frames.empty() && g_loaded_animation == g_ov.animation_path)
        return;
    if (g_attempted_animation == g_ov.animation_path)
        return;
    if (InterlockedCompareExchange(&g_anim_loading, 1, 0) != 0)
        return;

    g_attempted_animation = g_ov.animation_path;
    g_anim_worker_path = g_ov.animation_path;
    if (g_anim_thread)
    {
        CloseHandle(g_anim_thread);
        g_anim_thread = nullptr;
    }
    g_anim_thread = CreateThread(nullptr, 0, animation_worker, nullptr, 0, nullptr);
    if (g_anim_thread)
        SetThreadPriority(g_anim_thread, THREAD_PRIORITY_LOWEST);
    else
        InterlockedExchange(&g_anim_loading, 0);
}

static void overlay_free_bitmap()
{
    delete g_ov.bitmap;
    g_ov.bitmap = nullptr;
    delete g_ov.static_bitmap;
    g_ov.static_bitmap = nullptr;
    g_ov.static_valid = false;
    g_ov.bitmap_w = g_ov.bitmap_h = 0;
}

// Scale of the card in percent. The WinUI card is drawn at the system UI scale on
// the desktop, so the in-game card must end up the same size relative to the
// screen. A DPI-unaware game window reports 96 DPI and a virtualized monitor size,
// so the real monitor DPI and native desktop height are queried explicitly and the
// result is scaled by how much of the native display height the game's
// back buffer covers (exclusive fullscreen can run below native resolution).
static int dpi_scale_100(HWND hwnd, int screen_h)
{
    static HWND cache_hwnd;
    static int cache_h;
    static int cache_scale;
    static ULONGLONG cache_tick;
    const ULONGLONG now = GetTickCount64();
    if (cache_scale && cache_hwnd == hwnd && cache_h == screen_h && now - cache_tick < 2000)
        return cache_scale;

    typedef UINT(WINAPI * PFN_GetDpiForWindow)(HWND);
    typedef DPI_AWARENESS_CONTEXT(WINAPI * PFN_SetThreadDpiAwarenessContext)(DPI_AWARENESS_CONTEXT);
    typedef HRESULT(WINAPI * PFN_GetDpiForMonitor)(HMONITOR, int, UINT *, UINT *);
    static PFN_GetDpiForWindow get_dpi_window = nullptr;
    static PFN_SetThreadDpiAwarenessContext set_context = nullptr;
    static PFN_GetDpiForMonitor get_dpi_monitor = nullptr;
    static bool tried = false;
    if (!tried)
    {
        tried = true;
        HMODULE user32 = GetModuleHandleW(L"user32.dll");
        if (user32)
        {
            get_dpi_window = (PFN_GetDpiForWindow)GetProcAddress(user32, "GetDpiForWindow");
            set_context = (PFN_SetThreadDpiAwarenessContext)GetProcAddress(
                user32, "SetThreadDpiAwarenessContext");
        }
        HMODULE shcore = LoadLibraryW(L"shcore.dll");
        if (shcore)
            get_dpi_monitor = (PFN_GetDpiForMonitor)GetProcAddress(shcore, "GetDpiForMonitor");
    }

    UINT dpi = 0;
    int native_h = 0;
    HMONITOR monitor = hwnd ? MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST)
                            : MonitorFromPoint(POINT{0, 0}, MONITOR_DEFAULTTOPRIMARY);
    DPI_AWARENESS_CONTEXT previous = nullptr;
    if (set_context)
        previous = set_context(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    if (monitor && get_dpi_monitor)
    {
        UINT dx = 0, dy = 0;
        if (SUCCEEDED(get_dpi_monitor(monitor, 0 /* MDT_EFFECTIVE_DPI */, &dx, &dy)) && dx)
            dpi = dx;
    }
    if (previous)
        set_context(previous);

    if (monitor)
    {
        MONITORINFOEXW info = {};
        info.cbSize = sizeof(info);
        if (GetMonitorInfoW(monitor, &info))
        {
            DEVMODEW mode = {};
            mode.dmSize = sizeof(mode);
            if (EnumDisplaySettingsW(info.szDevice, ENUM_REGISTRY_SETTINGS, &mode))
                native_h = static_cast<int>(mode.dmPelsHeight);
        }
    }

    if (!dpi && get_dpi_window && hwnd)
        dpi = get_dpi_window(hwnd);
    if (!dpi)
    {
        HDC dc = GetDC(nullptr);
        if (dc)
        {
            dpi = static_cast<UINT>(GetDeviceCaps(dc, LOGPIXELSX));
            ReleaseDC(nullptr, dc);
        }
    }
    if (!dpi)
        dpi = 96;

    double scale = dpi * 100.0 / 96.0;
    if (native_h >= 480 && screen_h >= 240)
        scale = scale * static_cast<double>(screen_h) / static_cast<double>(native_h);
    int result = static_cast<int>(scale + 0.5);
    if (result < 60)
        result = 60;
    if (result > 400)
        result = 400;
    cache_hwnd = hwnd;
    cache_h = screen_h;
    cache_scale = result;
    cache_tick = now;
    return result;
}

static bool client_size(int &w, int &h)
{
    HDC dc = wglGetCurrentDC();
    HWND hwnd = dc ? WindowFromDC(dc) : nullptr;
    if (!hwnd)
        hwnd = GetForegroundWindow();
    RECT r;
    if (hwnd && GetClientRect(hwnd, &r))
    {
        w = r.right - r.left;
        h = r.bottom - r.top;
        if (w >= 16 && h >= 16)
            return true;
    }
    return false;
}

static void add_round_rect(GraphicsPath &path, const RectF &r, float radius)
{
    float d = radius * 2.0f;
    if (d > r.Width)
        d = r.Width;
    if (d > r.Height)
        d = r.Height;
    path.AddArc(r.X, r.Y, d, d, 180.0f, 90.0f);
    path.AddArc(r.X + r.Width - d, r.Y, d, d, 270.0f, 90.0f);
    path.AddArc(r.X + r.Width - d, r.Y + r.Height - d, d, d, 0.0f, 90.0f);
    path.AddArc(r.X, r.Y + r.Height - d, d, d, 90.0f, 90.0f);
    path.CloseFigure();
}

// Everything expensive that does not change between renders lives here. Building FontFamily/Font
// objects, cloning StringFormats, measuring the tracked header glyph by glyph and decoding + scaling
// the icon PNG used to happen on every render, which is every game frame while the success clip plays.
static const wchar_t kHeaderText[] = L"INSTANT REPLAY";

struct RenderCache
{
    int scale100 = 0;
    FontFamily *regular = nullptr;
    FontFamily *message = nullptr;
    Font *f_header = nullptr;
    Font *f_msg = nullptr;
    Font *f_detail = nullptr;
    StringFormat *fmt_header = nullptr;
    StringFormat *fmt_msg = nullptr;
    StringFormat *fmt_detail = nullptr;
    std::vector<float> header_x;

    Bitmap *icon = nullptr;
    std::wstring icon_path;
    int icon_size = 0;
};

static RenderCache g_rc;

static void render_cache_free_fonts()
{
    delete g_rc.f_header;
    delete g_rc.f_msg;
    delete g_rc.f_detail;
    delete g_rc.fmt_header;
    delete g_rc.fmt_msg;
    delete g_rc.fmt_detail;
    delete g_rc.regular;
    delete g_rc.message;
    g_rc.f_header = g_rc.f_msg = g_rc.f_detail = nullptr;
    g_rc.fmt_header = g_rc.fmt_msg = g_rc.fmt_detail = nullptr;
    g_rc.regular = g_rc.message = nullptr;
    g_rc.header_x.clear();
    g_rc.scale100 = 0;
}

static void render_cache_free()
{
    render_cache_free_fonts();
    delete g_rc.icon;
    g_rc.icon = nullptr;
    g_rc.icon_path.clear();
    g_rc.icon_size = 0;
}

static FontFamily *render_cache_family(const wchar_t *name)
{
    FontFamily *family = new FontFamily(name);
    if (family->IsAvailable())
        return family;
    delete family;
    return nullptr;
}

static void render_cache_prepare(int scale100, Graphics &g)
{
    if (g_rc.scale100 == scale100 && g_rc.f_msg)
        return;
    render_cache_free_fonts();

    // The WinUI card uses the system UI font (Segoe UI Variable on Windows 11, Segoe UI on
    // Windows 10). GDI+ cannot open the variable font by its generic name, so use its static
    // "Text" instances when present.
    FontFamily *var_regular = render_cache_family(L"Segoe UI Variable Text");
    FontFamily *var_semibold = render_cache_family(L"Segoe UI Variable Text Semibold");
    FontFamily *seg_regular = render_cache_family(L"Segoe UI");
    FontFamily *seg_semibold = render_cache_family(L"Segoe UI Semibold");

    g_rc.regular = var_regular ? var_regular : seg_regular;
    g_rc.message = var_semibold ? var_semibold
                                : (var_regular ? var_regular : (seg_semibold ? seg_semibold : seg_regular));
    FontFamily *keep[2] = {g_rc.regular, g_rc.message};
    for (FontFamily *candidate : {var_regular, var_semibold, seg_regular, seg_semibold})
    {
        if (candidate && candidate != keep[0] && candidate != keep[1])
            delete candidate;
    }
    if (!g_rc.regular)
        g_rc.regular = FontFamily::GenericSansSerif()->Clone();
    if (!g_rc.message)
        g_rc.message = FontFamily::GenericSansSerif()->Clone();
    // When one family serves both roles it must not be freed twice.
    if (g_rc.regular == g_rc.message)
        g_rc.message = g_rc.message->Clone();

    const float s = scale100 / 100.0f;
    g_rc.f_header = new Font(g_rc.message, 10.0f * s, FontStyleRegular, UnitPixel);
    g_rc.f_msg = new Font(g_rc.message, 19.0f * s, FontStyleRegular, UnitPixel);
    g_rc.f_detail = new Font(g_rc.regular, 12.0f * s, FontStyleRegular, UnitPixel);

    g_rc.fmt_header = StringFormat::GenericTypographic()->Clone();
    g_rc.fmt_header->SetFormatFlags(g_rc.fmt_header->GetFormatFlags() | StringFormatFlagsMeasureTrailingSpaces);

    g_rc.fmt_msg = StringFormat::GenericTypographic()->Clone();
    g_rc.fmt_msg->SetTrimming(StringTrimmingEllipsisCharacter);
    g_rc.fmt_msg->SetFormatFlags(g_rc.fmt_msg->GetFormatFlags() | StringFormatFlagsNoWrap);

    g_rc.fmt_detail = StringFormat::GenericTypographic()->Clone();
    g_rc.fmt_detail->SetTrimming(StringTrimmingEllipsisCharacter);
    g_rc.fmt_detail->SetFormatFlags(g_rc.fmt_detail->GetFormatFlags() | StringFormatFlagsNoWrap);

    // Pen position of every glyph of the letter-spaced header, measured once.
    const float tracking = 1.4f * s;
    float x = 0.0f;
    const size_t len = wcslen(kHeaderText);
    g_rc.header_x.assign(len, 0.0f);
    for (size_t i = 0; i < len; i++)
    {
        g_rc.header_x[i] = x;
        wchar_t ch[2] = {kHeaderText[i], 0};
        RectF box;
        g.MeasureString(ch, 1, g_rc.f_header, PointF(0, 0), g_rc.fmt_header, &box);
        x += box.Width + tracking;
    }
    g_rc.scale100 = scale100;
}

// The app icon, decoded and scaled once for the current card size.
static Bitmap *render_cache_icon(const std::wstring &path, int size_px)
{
    if (path.empty() || size_px <= 0)
        return nullptr;
    if (g_rc.icon && g_rc.icon_path == path && g_rc.icon_size == size_px)
        return g_rc.icon;

    delete g_rc.icon;
    g_rc.icon = nullptr;
    g_rc.icon_path = path;
    g_rc.icon_size = size_px;

    Bitmap source(path.c_str());
    if (source.GetLastStatus() != Ok)
        return nullptr;
    Bitmap *scaled = new Bitmap(size_px, size_px, PixelFormat32bppPARGB);
    if (scaled->GetLastStatus() != Ok)
    {
        delete scaled;
        return nullptr;
    }
    Graphics graphics(scaled);
    graphics.SetInterpolationMode(InterpolationModeHighQualityBicubic);
    graphics.SetPixelOffsetMode(PixelOffsetModeHighQuality);
    graphics.Clear(Color(0, 0, 0, 0));
    graphics.DrawImage(&source, Rect(0, 0, size_px, size_px));
    g_rc.icon = scaled;
    return g_rc.icon;
}

// Renders the card. When frame_only is set the artwork layer is reused and only the completion
// clip's icon cell is redrawn. That path runs on the game's Present thread at up to 60 fps while
// the success animation plays; re-running GDI+ text and paths there is what made the in-game
// notification stutter.
static void overlay_render(int card_w, int card_h, int scale100, float text_alpha,
                           float text_shift, float bar_alpha, bool frame_only)
{
    if (card_w <= 0 || card_h <= 0)
    {
        overlay_free_bitmap();
        return;
    }

    const float s = scale100 / 100.0f;
    const float icon_x = 22.0f * s;
    const float icon_y = 27.0f * s;
    const float icon_size = 54.0f * s;
    Bitmap *frame = g_ov.state == 2 ? animation_frame(qpc_100ns()) : nullptr;

    // Fast path: a new animation frame is the only thing that changed. Blit the cached
    // artwork layer and stamp the frame over the icon cell (two alpha blits, no text).
    if (frame_only && frame && g_ov.static_valid && g_ov.static_bitmap && g_ov.bitmap &&
        g_ov.bitmap_w == card_w && g_ov.bitmap_h == card_h)
    {
        Graphics out(g_ov.bitmap);
        out.SetCompositingMode(CompositingModeSourceCopy);
        out.DrawImage(g_ov.static_bitmap, 0, 0);
        out.SetCompositingMode(CompositingModeSourceOver);
        out.SetPixelOffsetMode(PixelOffsetModeHighQuality);
        out.SetInterpolationMode(InterpolationModeHighQualityBicubic);
        out.DrawImage(frame, RectF(icon_x, icon_y, icon_size, icon_size));
        return;
    }

    // Reuse the bitmaps while the size is unchanged: they are overwritten below, so there is no
    // reason to allocate and free ~0.4 MB of pixels on every re-render.
    if (!g_ov.bitmap || !g_ov.static_bitmap ||
        g_ov.bitmap_w != card_w || g_ov.bitmap_h != card_h)
    {
        overlay_free_bitmap();
        try
        {
            g_ov.bitmap = new Bitmap(card_w, card_h, PixelFormat32bppPARGB);
            g_ov.static_bitmap = new Bitmap(card_w, card_h, PixelFormat32bppPARGB);
        }
        catch (...)
        {
            overlay_free_bitmap();
            if (!g_ov.render_error_logged)
            {
                g_ov.render_error_logged = true;
                hook_log(L"card bitmap allocation failed (%dx%d)", card_w, card_h);
            }
            return;
        }
        if (g_ov.bitmap->GetLastStatus() != Ok || g_ov.static_bitmap->GetLastStatus() != Ok)
        {
            overlay_free_bitmap();
            if (!g_ov.render_error_logged)
            {
                g_ov.render_error_logged = true;
                hook_log(L"card bitmap creation failed (%dx%d)", card_w, card_h);
            }
            return;
        }
        g_ov.bitmap_w = card_w;
        g_ov.bitmap_h = card_h;
    }

    Bitmap *bmp = g_ov.bitmap;
    Bitmap *layer = g_ov.static_bitmap;

    Graphics g(layer);
    g.SetSmoothingMode(SmoothingModeAntiAlias);
    g.SetPixelOffsetMode(PixelOffsetModeHighQuality);
    g.SetInterpolationMode(InterpolationModeHighQualityBicubic);
    // Grid-fitted anti-aliasing keeps stems crisp; the text is positioned on whole
    // pixels below, like the WinUI card (DirectWrite snaps its baselines).
    g.SetTextRenderingHint(TextRenderingHintAntiAliasGridFit);
    g.Clear(Color(0, 0, 0, 0));
    render_cache_prepare(scale100, g);

    const RectF card(0, 0, static_cast<float>(card_w), static_cast<float>(card_h));

    GraphicsPath card_path;
    add_round_rect(card_path, card, 12.0f * s);

    // The completion animation is opaque H.264 with a #242E40 background.
    // Use that exact solid colour so the 54x54 video blends into the card.
    SolidBrush bg(Color(255, 36, 46, 64));
    g.FillPath(&bg, &card_path);

    const bool failed = g_ov.state == 3;
    const Color accent = failed ? Color(255, 241, 115, 122) : Color(255, 111, 237, 205);

    {
        GraphicsPath p;
        RectF bar(0, 18.0f * s, 3.0f * s, (108.0f - 36.0f) * s);
        add_round_rect(p, bar, 2.0f * s);
        SolidBrush b(accent);
        g.FillPath(&b, &p);
    }

    const float text_x = floorf(92.0f * s + 0.5f);
    const float content_h = 76.0f * s;
    const float h_header = g_rc.f_header->GetHeight(&g);
    const float h_msg = g_rc.f_msg->GetHeight(&g);
    const float h_detail = g_rc.f_detail->GetHeight(&g);
    const float spacing = 5.0f * s;
    float stack_h = h_header + h_msg + h_detail + spacing * 2.0f;
    float top = 16.0f * s + (content_h - stack_h) * 0.5f;
    if (top < 16.0f * s)
        top = 16.0f * s;
    top = floorf(top + 0.5f);

    const BYTE alpha = static_cast<BYTE>(ir_clamp01(text_alpha) * 255.0f + 0.5f);
    const float y = floorf(top + text_shift * s + 0.5f);
    if (alpha > 0)
    {
        SolidBrush header_brush(Color(alpha, 151, 174, 205));
        for (size_t i = 0; i < g_rc.header_x.size(); i++)
        {
            wchar_t ch[2] = {kHeaderText[i], 0};
            g.DrawString(ch, 1, g_rc.f_header, PointF(text_x + g_rc.header_x[i], y), g_rc.fmt_header,
                         &header_brush);
        }

        SolidBrush msg_brush(Color(alpha, 255, 255, 255));
        RectF msg_rect(text_x, floorf(y + h_header + spacing + 0.5f), card_w - text_x - 20.0f * s,
                       h_msg + 4.0f * s);
        g.DrawString(g_ov.title.c_str(), -1, g_rc.f_msg, msg_rect, g_rc.fmt_msg, &msg_brush);

        SolidBrush detail_brush(Color(alpha, 171, 185, 207));
        RectF detail_rect(text_x, floorf(y + h_header + spacing + h_msg + spacing + 0.5f),
                          card_w - text_x - 20.0f * s, h_detail + 4.0f * s);
        g.DrawString(g_ov.detail.c_str(), -1, g_rc.f_detail, detail_rect, g_rc.fmt_detail,
                     &detail_brush);
    }

    const float bar_a = ir_clamp01(bar_alpha);
    if (bar_a > 0.001f)
    {
        g.SetClip(&card_path);
        const float bar_h = 3.0f * s;
        SolidBrush track(Color(static_cast<BYTE>(bar_a * 70.0f), 111, 237, 205));
        g.FillRectangle(&track, RectF(0, card_h - bar_h, static_cast<float>(card_w), bar_h));
        int progress = g_ov.progress;
        if (progress < 0)
            progress = 0;
        if (progress > 100)
            progress = 100;
        float fill_w = static_cast<float>(card_w) * progress / 100.0f;
        if (fill_w > 0.5f)
        {
            SolidBrush fill(Color(static_cast<BYTE>(bar_a * 255.0f), accent.GetR(),
                                  accent.GetG(), accent.GetB()));
            g.FillRectangle(&fill, RectF(0, card_h - bar_h, fill_w, bar_h));
        }
        g.ResetClip();
    }

    g_ov.static_valid = true;

    // Compose the icon cell (a clip frame while the success animation plays) over the artwork.
    Graphics out(bmp);
    out.SetCompositingMode(CompositingModeSourceCopy);
    out.DrawImage(layer, 0, 0);
    out.SetCompositingMode(CompositingModeSourceOver);
    out.SetPixelOffsetMode(PixelOffsetModeHighQuality);
    out.SetInterpolationMode(InterpolationModeHighQualityBicubic);
    if (frame)
    {
        out.DrawImage(frame, RectF(icon_x, icon_y, icon_size, icon_size));
    }
    else
    {
        std::wstring path = g_ov.icon_path.empty() ? default_icon_path() : g_ov.icon_path;
        Bitmap *icon = render_cache_icon(path, static_cast<int>(icon_size + 0.5f));
        if (icon)
            out.DrawImage(icon, static_cast<INT>(icon_x + 0.5f), static_cast<INT>(icon_y + 0.5f));
    }
}

// True while the card is on screen or still sliding out. Everything else returns before touching
// the game's graphics API, so an idle overlay costs the game nothing per frame.
static inline bool overlay_active(int64_t now)
{
    return g_ov.visible || (g_ov.hide_at && now - g_ov.hide_at < IR_HIDE_TAIL_100NS);
}

// Advances the text/bar fade state and re-renders the card bitmap only when its content can actually
// have changed: new state or text, a new video frame, or a fade step (rate-limited). Sliding the card
// across the screen never needs a re-render; that is only a different quad position.
// Returns true when the bitmap was re-rendered and must be uploaded again.
static bool overlay_update_content(int64_t now, int card_w, int card_h, int scale100)
{
    float bar_alpha = g_ov.state == 1 ? 1.0f : 0.0f;
    if (g_ov.state != 1 && g_ov.bar_fading)
        bar_alpha = 1.0f - ir_clamp01(static_cast<float>(now - g_ov.bar_at) / 1600000.0f);

    float text_alpha = 1.0f, text_shift = 0.0f;
    if (g_ov.morphing)
    {
        text_alpha = ir_clamp01(static_cast<float>(now - g_ov.morph_at) / 1900000.0f);
        const float t = ir_clamp01(static_cast<float>(now - g_ov.morph_at) / 2400000.0f);
        const float eased = 1.0f - (1.0f - t) * (1.0f - t) * (1.0f - t);
        text_shift = 10.0f * (1.0f - eased);
        if (t >= 1.0f)
        {
            g_ov.morphing = false;
            g_ov.dirty = true;
        }
    }
    if (g_ov.bar_fading && now - g_ov.bar_at >= 1600000LL)
    {
        g_ov.bar_fading = false;
        g_ov.dirty = true;
    }

    const bool fading = g_ov.morphing || g_ov.bar_fading;
    const int anim_index = g_ov.state == 2 ? animation_frame_index(now) : -1;
    const bool size_changed = !g_ov.bitmap || !g_ov.static_bitmap ||
                              g_ov.bitmap_w != card_w || g_ov.bitmap_h != card_h;
    const int64_t since = now - g_ov.last_render_at;
    const bool frame_due = anim_index != g_ov.rendered_anim_index && since >= IR_ANIM_MIN_INTERVAL_100NS;
    const bool fade_due = fading && since >= IR_FADE_INTERVAL_100NS;
    const bool content_due = g_ov.dirty || size_changed || fade_due;
    if (!(content_due || frame_due))
        return false;

    // A due animation frame with nothing else changed only touches the 54px icon cell.
    overlay_render(card_w, card_h, scale100, text_alpha, text_shift, bar_alpha,
                   /*frame_only=*/!content_due);
    g_ov.dirty = false;
    g_ov.last_render_at = now;
    g_ov.rendered_anim_index = anim_index;
    return true;
}

// Horizontal position of the card. Motion is time-based (independent of the game's frame rate) and uses
// a sub-pixel position while moving, so the glide is smooth instead of advancing in whole-pixel steps.
// Once settled the card is snapped to whole pixels so its text stays crisp.
static float overlay_slide_x(int64_t now, int margin, int card_w)
{
    float slide;
    bool moving;
    if (g_ov.visible)
    {
        const float t = ir_clamp01(static_cast<float>(now - g_ov.show_at) / static_cast<float>(IR_SLIDE_IN_100NS));
        const float inv = 1.0f - t;
        slide = 1.0f - inv * inv * inv * inv * inv; // ease-out quint: quick arrival, soft landing
        moving = t < 1.0f;
    }
    else
    {
        const float t = ir_clamp01(static_cast<float>(now - g_ov.hide_at) / static_cast<float>(IR_SLIDE_OUT_100NS));
        slide = 1.0f - t * t * t; // ease-in cubic
        moving = true;
    }
    const float hidden = -static_cast<float>(card_w) - 16.0f;
    const float x = hidden + (static_cast<float>(margin) - hidden) * slide;
    return moving ? x : static_cast<float>(margin);
}

struct OverlayLayout
{
    float x = 0;
    float y = 0;
    int w = 0;
    int h = 0;
    bool rerendered = false;
};

static bool overlay_prepare(HWND hwnd, int screen_w, int screen_h, OverlayLayout &layout)
{
    const int64_t now = qpc_100ns();
    if (!overlay_active(now) || screen_w < 16 || screen_h < 16)
        return false;
    const int scale100 = dpi_scale_100(hwnd, screen_h);
    const float s = scale100 / 100.0f;
    int card_w = static_cast<int>(IR_CARD_W * s + 0.5f);
    int card_h = static_cast<int>(IR_CARD_H * s + 0.5f);
    const int margin = static_cast<int>(IR_CARD_MARGIN * s + 0.5f);
    const int available = (screen_w - margin) < 64 ? 64 : screen_w - margin;
    if (card_w > available)
    {
        card_w = available;
        card_h = static_cast<int>(card_w * (IR_CARD_H / IR_CARD_W) + 0.5f);
    }
    layout.rerendered = overlay_update_content(now, card_w, card_h, scale100);
    if (!g_ov.bitmap)
        return false;
    layout.x = overlay_slide_x(now, margin, card_w);
    layout.y = static_cast<float>(margin);
    layout.w = card_w;
    layout.h = card_h;
    return true;
}

static bool overlay_bitmap_pixels()
{
    if (!g_ov.bitmap)
        return false;
    BitmapData data{};
    Rect lock(0, 0, g_ov.bitmap_w, g_ov.bitmap_h);
    if (g_ov.bitmap->LockBits(&lock, ImageLockModeRead, PixelFormat32bppPARGB, &data) != Ok)
        return false;
    const size_t row_bytes = static_cast<size_t>(g_ov.bitmap_w) * 4;
    g_ov.upload.resize(row_bytes * g_ov.bitmap_h);
    for (int y = 0; y < g_ov.bitmap_h; ++y)
    {
        const uint8_t *source = static_cast<const uint8_t *>(data.Scan0) +
                                static_cast<ptrdiff_t>(y) * data.Stride;
        memcpy(g_ov.upload.data() + static_cast<size_t>(y) * row_bytes, source, row_bytes);
    }
    g_ov.bitmap->UnlockBits(&data);
    return true;
}

static bool overlay_upload()
{
    if (!g_ov.bitmap)
        return false;
    if (!g_ov.texture)
    {
        glGenTextures(1, &g_ov.texture);
        if (!g_ov.texture)
        {
            if (!g_ov.upload_error_logged)
            {
                g_ov.upload_error_logged = true;
                hook_log(L"card texture creation failed");
            }
            return false;
        }
    }
    glBindTexture(GL_TEXTURE_2D, g_ov.texture);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);

    BitmapData data;
    Rect lock(0, 0, g_ov.bitmap_w, g_ov.bitmap_h);
    if (g_ov.bitmap->LockBits(&lock, ImageLockModeRead, PixelFormat32bppPARGB, &data) != Ok)
    {
        if (!g_ov.upload_error_logged)
        {
            g_ov.upload_error_logged = true;
            hook_log(L"card bitmap lock failed");
        }
        return false;
    }
    const int w = g_ov.bitmap_w;
    const int h = g_ov.bitmap_h;
    g_ov.upload.resize(static_cast<size_t>(w) * h * 4);
    const uint8_t *src = static_cast<const uint8_t *>(data.Scan0);
    const int stride = data.Stride;
    for (int row = 0; row < h; row++)
    {
        memcpy(g_ov.upload.data() + static_cast<size_t>(row) * w * 4,
               src + static_cast<ptrdiff_t>(row) * stride, static_cast<size_t>(w) * 4);
    }
    g_ov.bitmap->UnlockBits(&data);

    GLint prev_align = 4, prev_row = 0, prev_skip_rows = 0, prev_skip_pixels = 0;
    GLint prev_swap = 0, prev_lsb = 0;
    glGetIntegerv(GL_UNPACK_ALIGNMENT, &prev_align);
    glGetIntegerv(GL_UNPACK_ROW_LENGTH, &prev_row);
    glGetIntegerv(GL_UNPACK_SKIP_ROWS, &prev_skip_rows);
    glGetIntegerv(GL_UNPACK_SKIP_PIXELS, &prev_skip_pixels);
    glGetIntegerv(GL_UNPACK_SWAP_BYTES, &prev_swap);
    glGetIntegerv(GL_UNPACK_LSB_FIRST, &prev_lsb);
    glPixelStorei(GL_UNPACK_ALIGNMENT, 4);
    glPixelStorei(GL_UNPACK_ROW_LENGTH, 0);
    glPixelStorei(GL_UNPACK_SKIP_ROWS, 0);
    glPixelStorei(GL_UNPACK_SKIP_PIXELS, 0);
    glPixelStorei(GL_UNPACK_SWAP_BYTES, 0);
    glPixelStorei(GL_UNPACK_LSB_FIRST, 0);
    if (w != g_ov.texture_w || h != g_ov.texture_h)
    {
        glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA, w, h, 0, GL_BGRA, GL_UNSIGNED_BYTE,
                     g_ov.upload.data());
        g_ov.texture_w = w;
        g_ov.texture_h = h;
    }
    else
    {
        glTexSubImage2D(GL_TEXTURE_2D, 0, 0, 0, w, h, GL_BGRA, GL_UNSIGNED_BYTE,
                        g_ov.upload.data());
    }
    glPixelStorei(GL_UNPACK_ALIGNMENT, prev_align);
    glPixelStorei(GL_UNPACK_ROW_LENGTH, prev_row);
    glPixelStorei(GL_UNPACK_SKIP_ROWS, prev_skip_rows);
    glPixelStorei(GL_UNPACK_SKIP_PIXELS, prev_skip_pixels);
    glPixelStorei(GL_UNPACK_SWAP_BYTES, prev_swap);
    glPixelStorei(GL_UNPACK_LSB_FIRST, prev_lsb);
    return true;
}

static bool is_core_profile()
{
    if (g_ov.core_profile >= 0)
        return g_ov.core_profile != 0;
    bool core = false;
    const char *version = reinterpret_cast<const char *>(glGetString(GL_VERSION));
    int major = 0, minor = 0;
    if (version && sscanf_s(version, "%d.%d", &major, &minor) >= 2 &&
        (major > 3 || (major == 3 && minor >= 2)))
    {
        GLint mask = 0;
        glGetIntegerv(GL_CONTEXT_PROFILE_MASK, &mask);
        glGetError();
        core = (mask & GL_CONTEXT_CORE_PROFILE_BIT) != 0;
    }
    g_ov.core_profile = core ? 1 : 0;
    return core;
}

static GLuint compile_overlay_shader(GLenum type, const char *source)
{
    GLuint shader = p_glCreateShader(type);
    if (!shader)
        return 0;
    p_glShaderSource(shader, 1, &source, nullptr);
    p_glCompileShader(shader);
    GLint ok = 0;
    p_glGetShaderiv(shader, GL_COMPILE_STATUS, &ok);
    if (!ok)
    {
        char text[1024] = {0};
        p_glGetShaderInfoLog(shader, sizeof(text), nullptr, text);
        hook_log(L"core profile: shader compile failed: %hs", text);
        p_glDeleteShader(shader);
        return 0;
    }
    return shader;
}

static bool overlay_modern_ensure()
{
    if (g_ov.modern_initialized)
        return !g_ov.modern_failed;
    g_ov.modern_initialized = true;

    if (!p_glCreateShader || !p_glShaderSource || !p_glCompileShader || !p_glGetShaderiv ||
        !p_glGetShaderInfoLog || !p_glCreateProgram || !p_glAttachShader || !p_glLinkProgram ||
        !p_glGetProgramiv || !p_glGetProgramInfoLog || !p_glDeleteShader ||
        !p_glGetAttribLocation || !p_glGetUniformLocation || !p_glUniform1i ||
        !p_glUniform2f || !p_glUniform4f || !p_glGenBuffers || !p_glBindBuffer ||
        !p_glBufferData || !p_glGenVertexArrays || !p_glBindVertexArray ||
        !p_glEnableVertexAttribArray || !p_glVertexAttribPointer || !p_glUseProgram ||
        !p_glActiveTexture)
    {
        hook_log(L"core profile: required shader entry points are missing");
        g_ov.modern_failed = true;
        return false;
    }

    static const char *kVertex =
        "#version 150 core\n"
        "in vec2 a_pos;\n"
        "in vec2 a_uv;\n"
        "uniform vec2 u_viewport;\n"
        "uniform vec4 u_rect;\n"
        "out vec2 v_uv;\n"
        "void main() {\n"
        "    vec2 px = u_rect.xy + a_pos * u_rect.zw;\n"
        "    vec2 ndc = vec2(px.x / u_viewport.x * 2.0 - 1.0,\n"
        "                    1.0 - px.y / u_viewport.y * 2.0);\n"
        "    v_uv = a_uv;\n"
        "    gl_Position = vec4(ndc, 0.0, 1.0);\n"
        "}\n";
    static const char *kFragment =
        "#version 150 core\n"
        "uniform sampler2D u_tex;\n"
        "in vec2 v_uv;\n"
        "out vec4 frag;\n"
        "void main() { frag = texture(u_tex, v_uv); }\n";

    GLuint vs = compile_overlay_shader(GL_VERTEX_SHADER, kVertex);
    if (!vs)
    {
        g_ov.modern_failed = true;
        return false;
    }
    GLuint fs = compile_overlay_shader(GL_FRAGMENT_SHADER, kFragment);
    if (!fs)
    {
        p_glDeleteShader(vs);
        g_ov.modern_failed = true;
        return false;
    }

    GLuint program = p_glCreateProgram();
    if (!program)
    {
        p_glDeleteShader(vs);
        p_glDeleteShader(fs);
        g_ov.modern_failed = true;
        return false;
    }
    p_glAttachShader(program, vs);
    p_glAttachShader(program, fs);
    p_glLinkProgram(program);
    p_glDeleteShader(vs);
    p_glDeleteShader(fs);
    GLint linked = 0;
    p_glGetProgramiv(program, GL_LINK_STATUS, &linked);
    if (!linked)
    {
        char text[1024] = {0};
        p_glGetProgramInfoLog(program, sizeof(text), nullptr, text);
        hook_log(L"core profile: shader link failed: %hs", text);
        if (p_glDeleteProgram)
            p_glDeleteProgram(program);
        g_ov.modern_failed = true;
        return false;
    }

    GLint a_pos = p_glGetAttribLocation(program, "a_pos");
    GLint a_uv = p_glGetAttribLocation(program, "a_uv");
    g_ov.u_rect = p_glGetUniformLocation(program, "u_rect");
    g_ov.u_viewport = p_glGetUniformLocation(program, "u_viewport");
    g_ov.u_tex = p_glGetUniformLocation(program, "u_tex");
    if (a_pos < 0 || a_uv < 0 || g_ov.u_rect < 0 || g_ov.u_viewport < 0 || g_ov.u_tex < 0)
    {
        hook_log(L"core profile: shader attributes are missing");
        if (p_glDeleteProgram)
            p_glDeleteProgram(program);
        g_ov.modern_failed = true;
        return false;
    }

    static const GLfloat verts[] = {
        0.0f,
        0.0f,
        0.0f,
        0.0f,
        1.0f,
        0.0f,
        1.0f,
        0.0f,
        0.0f,
        1.0f,
        0.0f,
        1.0f,
        1.0f,
        1.0f,
        1.0f,
        1.0f,
    };
    p_glGenVertexArrays(1, &g_ov.vao);
    p_glGenBuffers(1, &g_ov.vbo);
    if (!g_ov.vao || !g_ov.vbo)
    {
        hook_log(L"core profile: card vertex buffers could not be created");
        if (p_glDeleteProgram)
            p_glDeleteProgram(program);
        g_ov.modern_failed = true;
        return false;
    }
    p_glBindVertexArray(g_ov.vao);
    p_glBindBuffer(GL_ARRAY_BUFFER, g_ov.vbo);
    p_glBufferData(GL_ARRAY_BUFFER, sizeof(verts), verts, GL_STATIC_DRAW);
    const GLsizei stride = static_cast<GLsizei>(4 * sizeof(GLfloat));
    p_glEnableVertexAttribArray(static_cast<GLuint>(a_pos));
    p_glVertexAttribPointer(static_cast<GLuint>(a_pos), 2, GL_FLOAT, GL_FALSE, stride,
                            reinterpret_cast<const void *>(0));
    p_glEnableVertexAttribArray(static_cast<GLuint>(a_uv));
    p_glVertexAttribPointer(static_cast<GLuint>(a_uv), 2, GL_FLOAT, GL_FALSE, stride,
                            reinterpret_cast<const void *>(2 * sizeof(GLfloat)));
    p_glBindVertexArray(0);
    p_glBindBuffer(GL_ARRAY_BUFFER, 0);

    g_ov.program = program;
    hook_log(L"core profile: shader card path ready");
    return true;
}

static void overlay_draw_shader(float x, float y, float w, float h, int screen_w,
                                int screen_h)
{
    if (!overlay_modern_ensure())
        return;

    GLint prev_program = 0, prev_vao = 0, prev_vbo = 0, prev_tex = 0, prev_active = 0;
    GLint prev_vp[4] = {0, 0, 0, 0};
    GLint prev_poly[2] = {GL_FILL, GL_FILL};
    GLint prev_src_rgb = GL_ONE, prev_dst_rgb = GL_ZERO;
    GLint prev_src_a = GL_ONE, prev_dst_a = GL_ZERO;
    GLint prev_eq_rgb = GL_FUNC_ADD, prev_eq_a = GL_FUNC_ADD;
    GLboolean prev_blend = GL_FALSE, prev_depth = GL_TRUE, prev_cull = GL_FALSE;
    GLboolean prev_scissor = GL_FALSE, prev_stencil = GL_FALSE;
    GLboolean prev_logic = GL_FALSE, prev_srgb = GL_FALSE;
    GLboolean prev_mask[4] = {GL_TRUE, GL_TRUE, GL_TRUE, GL_TRUE};

    glGetIntegerv(GL_CURRENT_PROGRAM, &prev_program);
    glGetIntegerv(GL_VERTEX_ARRAY_BINDING, &prev_vao);
    glGetIntegerv(GL_ARRAY_BUFFER_BINDING, &prev_vbo);
    glGetIntegerv(GL_ACTIVE_TEXTURE, &prev_active);
    p_glActiveTexture(GL_TEXTURE0);
    glGetIntegerv(GL_TEXTURE_BINDING_2D, &prev_tex);
    glGetIntegerv(GL_BLEND_SRC_RGB, &prev_src_rgb);
    glGetIntegerv(GL_BLEND_DST_RGB, &prev_dst_rgb);
    glGetIntegerv(GL_BLEND_SRC_ALPHA, &prev_src_a);
    glGetIntegerv(GL_BLEND_DST_ALPHA, &prev_dst_a);
    glGetIntegerv(GL_BLEND_EQUATION_RGB, &prev_eq_rgb);
    glGetIntegerv(GL_BLEND_EQUATION_ALPHA, &prev_eq_a);
    glGetBooleanv(GL_BLEND, &prev_blend);
    glGetBooleanv(GL_DEPTH_TEST, &prev_depth);
    glGetBooleanv(GL_CULL_FACE, &prev_cull);
    glGetBooleanv(GL_SCISSOR_TEST, &prev_scissor);
    glGetBooleanv(GL_STENCIL_TEST, &prev_stencil);
    glGetBooleanv(GL_COLOR_LOGIC_OP, &prev_logic);
    glGetBooleanv(GL_FRAMEBUFFER_SRGB, &prev_srgb);
    glGetBooleanv(GL_COLOR_WRITEMASK, prev_mask);
    glGetIntegerv(GL_VIEWPORT, prev_vp);
    glGetIntegerv(GL_POLYGON_MODE, prev_poly);

    glViewport(0, 0, screen_w, screen_h);
    glDisable(GL_DEPTH_TEST);
    glDisable(GL_CULL_FACE);
    glDisable(GL_SCISSOR_TEST);
    glDisable(GL_STENCIL_TEST);
    glDisable(GL_COLOR_LOGIC_OP);
    glDisable(GL_FRAMEBUFFER_SRGB);
    glEnable(GL_BLEND);
    if (p_glBlendEquationSeparate)
        p_glBlendEquationSeparate(GL_FUNC_ADD, GL_FUNC_ADD);
    if (p_glBlendFuncSeparate)
    {
        p_glBlendFuncSeparate(GL_ONE, GL_ONE_MINUS_SRC_ALPHA, GL_ONE, GL_ONE_MINUS_SRC_ALPHA);
    }
    else
    {
        glBlendFunc(GL_ONE, GL_ONE_MINUS_SRC_ALPHA);
    }
    glColorMask(GL_TRUE, GL_TRUE, GL_TRUE, GL_TRUE);
    glPolygonMode(GL_FRONT_AND_BACK, GL_FILL);

    p_glUseProgram(g_ov.program);
    p_glActiveTexture(GL_TEXTURE0);
    glBindTexture(GL_TEXTURE_2D, g_ov.texture);
    p_glUniform1i(g_ov.u_tex, 0);
    p_glUniform4f(g_ov.u_rect, x, y, w, h);
    p_glUniform2f(g_ov.u_viewport, static_cast<GLfloat>(screen_w),
                  static_cast<GLfloat>(screen_h));
    p_glBindVertexArray(g_ov.vao);
    p_glBindBuffer(GL_ARRAY_BUFFER, g_ov.vbo);
    glDrawArrays(GL_TRIANGLE_STRIP, 0, 4);

    p_glBindVertexArray(static_cast<GLuint>(prev_vao));
    p_glBindBuffer(GL_ARRAY_BUFFER, static_cast<GLuint>(prev_vbo));
    glBindTexture(GL_TEXTURE_2D, static_cast<GLuint>(prev_tex));
    p_glActiveTexture(static_cast<GLenum>(prev_active));
    p_glUseProgram(static_cast<GLuint>(prev_program));
    if (p_glBlendEquationSeparate)
    {
        p_glBlendEquationSeparate(static_cast<GLenum>(prev_eq_rgb),
                                  static_cast<GLenum>(prev_eq_a));
    }
    if (p_glBlendFuncSeparate)
    {
        p_glBlendFuncSeparate(static_cast<GLenum>(prev_src_rgb),
                              static_cast<GLenum>(prev_dst_rgb),
                              static_cast<GLenum>(prev_src_a),
                              static_cast<GLenum>(prev_dst_a));
    }
    else
    {
        glBlendFunc(static_cast<GLenum>(prev_src_rgb), static_cast<GLenum>(prev_dst_rgb));
    }
    if (prev_blend)
        glEnable(GL_BLEND);
    else
        glDisable(GL_BLEND);
    if (prev_depth)
        glEnable(GL_DEPTH_TEST);
    else
        glDisable(GL_DEPTH_TEST);
    if (prev_cull)
        glEnable(GL_CULL_FACE);
    else
        glDisable(GL_CULL_FACE);
    if (prev_scissor)
        glEnable(GL_SCISSOR_TEST);
    else
        glDisable(GL_SCISSOR_TEST);
    if (prev_stencil)
        glEnable(GL_STENCIL_TEST);
    else
        glDisable(GL_STENCIL_TEST);
    if (prev_logic)
        glEnable(GL_COLOR_LOGIC_OP);
    else
        glDisable(GL_COLOR_LOGIC_OP);
    if (prev_srgb)
        glEnable(GL_FRAMEBUFFER_SRGB);
    else
        glDisable(GL_FRAMEBUFFER_SRGB);
    glColorMask(prev_mask[0], prev_mask[1], prev_mask[2], prev_mask[3]);
    if (prev_poly[0] != GL_FILL || prev_poly[1] != GL_FILL)
    {
        glPolygonMode(GL_FRONT, static_cast<GLenum>(prev_poly[0]));
        glPolygonMode(GL_BACK, static_cast<GLenum>(prev_poly[1]));
    }
    glViewport(prev_vp[0], prev_vp[1], prev_vp[2], prev_vp[3]);
}

static void overlay_draw()
{
    int64_t now = qpc_100ns();
    if (!overlay_active(now))
    {
        return;
    }

    int screen_w = 0, screen_h = 0;
    if (!client_size(screen_w, screen_h))
    {
        GLint vp[4] = {0, 0, 0, 0};
        glGetIntegerv(GL_VIEWPORT, vp);
        screen_w = vp[2];
        screen_h = vp[3];
    }
    if (screen_w < 16 || screen_h < 16)
        return;

    HDC dc = wglGetCurrentDC();
    HWND hwnd = dc ? WindowFromDC(dc) : nullptr;
    int scale100 = dpi_scale_100(hwnd, screen_h);
    float s = scale100 / 100.0f;

    int card_w = static_cast<int>((IR_CARD_W * s) + 0.5f);
    int card_h = static_cast<int>((IR_CARD_H * s) + 0.5f);
    int margin = static_cast<int>((IR_CARD_MARGIN * s) + 0.5f);
    int avail = screen_w - margin;
    if (avail < 64)
        avail = 64;
    if (card_w > avail)
    {
        card_w = avail;
        card_h = static_cast<int>(card_w * (IR_CARD_H / IR_CARD_W) + 0.5f);
    }

    const bool needs_render = overlay_update_content(now, card_w, card_h, scale100);
    if (!g_ov.bitmap)
        return;

    float x = overlay_slide_x(now, margin, card_w);
    float y = static_cast<float>(margin);

    if (is_core_profile())
    {
        if (needs_render || !g_ov.texture)
        {
            if (!overlay_upload())
                return;
        }
        overlay_draw_shader(x, y, static_cast<float>(card_w), static_cast<float>(card_h),
                            screen_w, screen_h);
        return;
    }

    GLint prev_viewport[4] = {0, 0, 0, 0};
    glGetIntegerv(GL_VIEWPORT, prev_viewport);

    GLint previous_program = 0;
    if (p_glUseProgram)
        glGetIntegerv(GL_CURRENT_PROGRAM, &previous_program);

    glPushAttrib(GL_ALL_ATTRIB_BITS);
    glMatrixMode(GL_PROJECTION);
    glPushMatrix();
    glLoadIdentity();
    glOrtho(0.0, screen_w, screen_h, 0.0, -1.0, 1.0);
    glMatrixMode(GL_MODELVIEW);
    glPushMatrix();
    glLoadIdentity();
    glViewport(0, 0, screen_w, screen_h);

    if ((needs_render || !g_ov.texture) && !overlay_upload())
    {
        glMatrixMode(GL_MODELVIEW);
        glPopMatrix();
        glMatrixMode(GL_PROJECTION);
        glPopMatrix();
        glPopAttrib();
        glViewport(prev_viewport[0], prev_viewport[1], prev_viewport[2],
                   prev_viewport[3]);
        return;
    }

    if (p_glUseProgram && previous_program != 0)
        p_glUseProgram(0);

    glDisable(GL_DEPTH_TEST);
    glDisable(GL_CULL_FACE);
    glDisable(GL_SCISSOR_TEST);
    glDisable(GL_LIGHTING);
    glDisable(GL_FOG);
    glDisable(GL_ALPHA_TEST);
    glPolygonMode(GL_FRONT_AND_BACK, GL_FILL);
    glEnable(GL_BLEND);
    glBlendFunc(GL_ONE, GL_ONE_MINUS_SRC_ALPHA);
    glEnable(GL_TEXTURE_2D);
    if (p_glActiveTexture)
        p_glActiveTexture(GL_TEXTURE0);
    glBindTexture(GL_TEXTURE_2D, g_ov.texture);
    glTexEnvi(GL_TEXTURE_ENV, GL_TEXTURE_ENV_MODE, GL_MODULATE);
    glColor4f(1.0f, 1.0f, 1.0f, 1.0f);

    float x0 = x;
    float y0 = y;
    float x1 = x + static_cast<float>(card_w);
    float y1 = y + static_cast<float>(card_h);
    glBegin(GL_QUADS);
    glTexCoord2f(0.0f, 0.0f);
    glVertex2f(x0, y0);
    glTexCoord2f(1.0f, 0.0f);
    glVertex2f(x1, y0);
    glTexCoord2f(1.0f, 1.0f);
    glVertex2f(x1, y1);
    glTexCoord2f(0.0f, 1.0f);
    glVertex2f(x0, y1);
    glEnd();

    glBindTexture(GL_TEXTURE_2D, 0);
    glMatrixMode(GL_MODELVIEW);
    glPopMatrix();
    glMatrixMode(GL_PROJECTION);
    glPopMatrix();
    glPopAttrib();
    glViewport(prev_viewport[0], prev_viewport[1], prev_viewport[2], prev_viewport[3]);
    if (p_glUseProgram && previous_program != 0)
    {
        p_glUseProgram(static_cast<GLuint>(previous_program));
    }
}

static void overlay_sync(const ControlSnapshot &ctl)
{
    int64_t now = qpc_100ns();
    bool changed = false;

    if (g_ov.last_seq >= 0 && static_cast<int64_t>(ctl.overlay_seq) < g_ov.last_seq)
    {
        g_ov.last_seq = -1;
        g_ov.visible = false;
        g_ov.hide_at = 0;
        g_ov.morphing = false;
        g_ov.bar_fading = false;
        changed = true;
    }

    if (g_ov.last_seq != static_cast<int64_t>(ctl.overlay_seq))
    {
        bool first = g_ov.last_seq < 0;
        bool state_changed = first || static_cast<int>(ctl.overlay_state) != g_ov.state;
        bool title_changed = ctl.strings_changed && ctl.title != g_ov.title;
        g_ov.last_seq = static_cast<int64_t>(ctl.overlay_seq);
        g_ov.state = static_cast<int>(ctl.overlay_state);
        g_ov.seconds = ctl.overlay_seconds;
        if (ctl.strings_changed)
        {
            g_ov.title = ctl.title;
            g_ov.detail = ctl.detail;
            if (!ctl.icon_path.empty())
                g_ov.icon_path = ctl.icon_path;
            if (!ctl.animation_path.empty())
                g_ov.animation_path = ctl.animation_path;
        }
        if (state_changed && ctl.overlay_state == 2)
        {
            g_ov.animation_at = now;
            g_attempted_animation.clear();
        }
        else if (state_changed)
        {
            g_ov.animation_at = 0;
            g_attempted_animation.clear();
        }
        if (ctl.overlay_progress != g_ov.progress)
        {
            g_ov.progress = ctl.overlay_progress;
            changed = true;
        }
        if (!first && state_changed)
        {
            g_ov.morphing = true;
            g_ov.morph_at = now;
            if (ctl.overlay_state == 1)
            {
                g_ov.bar_fading = false;
            }
            else
            {
                g_ov.bar_at = now;
                g_ov.bar_fading = true;
            }
            changed = true;
        }
        if (title_changed || state_changed)
            changed = true;
        if (ctl.overlay_visible && !g_ov.visible)
        {
            g_ov.visible = true;
            g_ov.show_at = now;
            g_ov.hide_at = 0;
            if (ctl.overlay_state == 2)
            {
                // A second save may reuse the already-decoded frames; restart the
                // clip from frame zero instead of continuing mid-loop.
                g_ov.animation_at = now;
                g_attempted_animation.clear();
            }
            hook_log(L"notification visible (state %d, seq %u)",
                     static_cast<int>(ctl.overlay_state), ctl.overlay_seq);
        }
        else if (!ctl.overlay_visible && g_ov.visible)
        {
            g_ov.visible = false;
            g_ov.hide_at = now;
            hook_log(L"notification hidden (seq %u)", ctl.overlay_seq);
        }
    }
    else if (ctl.overlay_visible != (g_ov.visible ? 1u : 0u))
    {
        if (ctl.overlay_visible)
        {
            g_ov.visible = true;
            g_ov.show_at = now;
            g_ov.hide_at = 0;
            hook_log(L"notification visible (no seq change)");
        }
        else
        {
            g_ov.visible = false;
            g_ov.hide_at = now;
            hook_log(L"notification hidden (no seq change)");
        }
        changed = true;
    }

    animation_ensure();

    if (changed)
        g_ov.dirty = true;
}


static void present_overlay()
{
    if (g_shutting_down || !g_gdi_ready)
        return;
    HGLRC context = wglGetCurrentContext();
    if (!context)
        return;
    load_gl();

    if (context != g_ov.context)
    {
        bool changed = g_ov.context != nullptr;
        g_ov.context = context;
        g_ov.texture = 0;
        g_ov.texture_w = 0;
        g_ov.texture_h = 0;
        g_ov.program = 0;
        g_ov.vao = 0;
        g_ov.vbo = 0;
        g_ov.u_rect = -1;
        g_ov.u_viewport = -1;
        g_ov.u_tex = -1;
        g_ov.modern_initialized = false;
        g_ov.modern_failed = false;
        g_ov.core_profile = -1;
        g_ov.upload_error_logged = false;
        g_ov.render_error_logged = false;
        g_ov.dirty = true;
        if (changed)
            hook_log(L"GL context changed; recreating card resources");
    }

    ControlSnapshot ctl;
    control_read(ctl);
    if (!ctl.valid)
        return;
    log_ensure(ctl);
    overlay_sync(ctl);
    // Build the shader card path while the overlay is still idle, so the first
    // notification does not pay the GLSL compile on the game's render thread.
    if (is_core_profile() && !g_ov.modern_initialized)
    {
        GLint prev_vao = 0, prev_vbo = 0;
        glGetIntegerv(GL_VERTEX_ARRAY_BINDING, &prev_vao);
        glGetIntegerv(GL_ARRAY_BUFFER_BINDING, &prev_vbo);
        overlay_modern_ensure();
        if (p_glBindVertexArray && p_glBindBuffer)
        {
            p_glBindVertexArray(static_cast<GLuint>(prev_vao));
            p_glBindBuffer(GL_ARRAY_BUFFER, static_cast<GLuint>(prev_vbo));
        }
        glGetError();
    }
    // Idle overlay: leave before any glGet* call (those stall the GL pipeline every frame).
    if (!overlay_active(qpc_100ns()))
        return;

    GLint previous_fbo = 0;
    if (p_glBindFramebuffer)
    {
        glGetIntegerv(GL_FRAMEBUFFER_BINDING, &previous_fbo);
        if (previous_fbo != 0)
        {
            p_glBindFramebuffer(GL_FRAMEBUFFER, 0);
        }
    }
    overlay_draw();
    if (p_glBindFramebuffer && previous_fbo != 0)
    {
        p_glBindFramebuffer(GL_FRAMEBUFFER, static_cast<GLuint>(previous_fbo));
    }
}


static IDirect3DDevice9 *g_ov9_device;
static IDirect3DTexture9 *g_ov9_texture;
static IDirect3DStateBlock9 *g_ov9_state;
static int g_ov9_w, g_ov9_h;

static void overlay_d3d9_release()
{
    if (g_ov9_texture)
        g_ov9_texture->Release();
    g_ov9_texture = nullptr;
    if (g_ov9_state)
        g_ov9_state->Release();
    g_ov9_state = nullptr;
    g_ov9_device = nullptr;
    g_ov9_w = g_ov9_h = 0;
}

static void present_overlay_d3d9(IDirect3DDevice9 *device)
{
    if (!device || g_shutting_down)
        return;
    ControlSnapshot ctl;
    control_read(ctl);
    if (!ctl.valid)
        return;
    log_ensure(ctl);
    overlay_sync(ctl);
    if (!overlay_active(qpc_100ns()))
        return;
    D3DVIEWPORT9 viewport{};
    if (FAILED(device->GetViewport(&viewport)))
        return;
    D3DDEVICE_CREATION_PARAMETERS creation{};
    device->GetCreationParameters(&creation);
    OverlayLayout layout;
    if (!overlay_prepare(creation.hFocusWindow, viewport.Width, viewport.Height, layout))
        return;
    if (g_ov9_device != device || g_ov9_w != layout.w || g_ov9_h != layout.h)
    {
        overlay_d3d9_release();
        if (FAILED(device->CreateTexture(layout.w, layout.h, 1, D3DUSAGE_DYNAMIC,
                                         D3DFMT_A8R8G8B8, D3DPOOL_DEFAULT,
                                         &g_ov9_texture, nullptr)))
            return;
        g_ov9_device = device;
        g_ov9_w = layout.w;
        g_ov9_h = layout.h;
        layout.rerendered = true;
    }
    if (layout.rerendered)
    {
        if (!overlay_bitmap_pixels())
            return;
        D3DLOCKED_RECT lock{};
        if (FAILED(g_ov9_texture->LockRect(0, &lock, nullptr, D3DLOCK_DISCARD)))
            return;
        const size_t row = static_cast<size_t>(layout.w) * 4;
        for (int y = 0; y < layout.h; ++y)
            memcpy(static_cast<uint8_t *>(lock.pBits) + y * lock.Pitch,
                   g_ov.upload.data() + static_cast<size_t>(y) * row, row);
        g_ov9_texture->UnlockRect(0);
    }
    if (!g_ov9_state && FAILED(device->CreateStateBlock(D3DSBT_ALL, &g_ov9_state)))
        return;
    if (FAILED(g_ov9_state->Capture()))
        return;
    struct Vertex
    {
        float x, y, z, rhw, u, v;
    };
    // The D3D9 path samples with the default point filter, so keep its quad on whole pixels.
    const float snapped_x = floorf(layout.x + 0.5f);
    const float x0 = snapped_x - 0.5f, y0 = layout.y - 0.5f;
    const float x1 = snapped_x + layout.w - 0.5f, y1 = layout.y + layout.h - 0.5f;
    const Vertex vertices[4] = {
        {x0, y0, 0, 1, 0, 0},
        {x1, y0, 0, 1, 1, 0},
        {x0, y1, 0, 1, 0, 1},
        {x1, y1, 0, 1, 1, 1},
    };
    device->SetTexture(0, g_ov9_texture);
    device->SetFVF(D3DFVF_XYZRHW | D3DFVF_TEX1);
    device->SetRenderState(D3DRS_ZENABLE, FALSE);
    device->SetRenderState(D3DRS_CULLMODE, D3DCULL_NONE);
    device->SetRenderState(D3DRS_ALPHABLENDENABLE, TRUE);
    device->SetRenderState(D3DRS_SRCBLEND, D3DBLEND_ONE);
    device->SetRenderState(D3DRS_DESTBLEND, D3DBLEND_INVSRCALPHA);
    device->SetTextureStageState(0, D3DTSS_COLOROP, D3DTOP_SELECTARG1);
    device->SetTextureStageState(0, D3DTSS_COLORARG1, D3DTA_TEXTURE);
    device->SetTextureStageState(0, D3DTSS_ALPHAOP, D3DTOP_SELECTARG1);
    device->SetTextureStageState(0, D3DTSS_ALPHAARG1, D3DTA_TEXTURE);
    device->DrawPrimitiveUP(D3DPT_TRIANGLESTRIP, 2, vertices, sizeof(Vertex));
    g_ov9_state->Apply();
}

struct D3D11Overlay
{
    ID3D11Device *device = nullptr;
    ID3D11Texture2D *texture = nullptr;
    ID3D11ShaderResourceView *view = nullptr;
    ID3D11VertexShader *vertex = nullptr;
    ID3D11PixelShader *pixel = nullptr;
    ID3D11SamplerState *sampler = nullptr;
    ID3D11BlendState *blend = nullptr;
    ID3D11DepthStencilState *depth = nullptr;
    ID3D11RasterizerState *raster = nullptr;
    ID3D11Buffer *constants = nullptr;
    int w = 0, h = 0;
};
static D3D11Overlay g_ov11;
static bool g_ov11_warm_failed;

template <typename T>
static void release_com(T *&value)
{
    if (value)
        value->Release();
    value = nullptr;
}

static void overlay_d3d11_release()
{
    release_com(g_ov11.texture);
    release_com(g_ov11.view);
    release_com(g_ov11.vertex);
    release_com(g_ov11.pixel);
    release_com(g_ov11.sampler);
    release_com(g_ov11.blend);
    release_com(g_ov11.depth);
    release_com(g_ov11.raster);
    release_com(g_ov11.constants);
    g_ov11 = {};
}

static bool overlay_d3d11_init(ID3D11Device *device)
{
    if (g_ov11.device == device && g_ov11.vertex)
        return true;
    overlay_d3d11_release();
    static const char *source =
        "cbuffer C:register(b0){float4 r;} struct O{float4 p:SV_Position;float2 u:TEXCOORD0;};"
        "O vs(uint i:SV_VertexID){float2 q[6]={float2(0,0),float2(1,0),float2(0,1),"
        "float2(0,1),float2(1,0),float2(1,1)};O o;o.u=q[i];o.p=float4(lerp(r.xy,r.zw,q[i]),0,1);return o;}"
        "Texture2D t:register(t0);SamplerState s:register(s0);float4 ps(O i):SV_Target{return t.Sample(s,i.u);}";
    ID3DBlob *vs = nullptr;
    ID3DBlob *ps = nullptr;
    ID3DBlob *errors = nullptr;
    HRESULT hr = D3DCompile(source, strlen(source), nullptr, nullptr, nullptr, "vs", "vs_4_0",
                            D3DCOMPILE_OPTIMIZATION_LEVEL3, 0, &vs, &errors);
    if (errors)
        errors->Release();
    if (FAILED(hr))
        return false;
    hr = D3DCompile(source, strlen(source), nullptr, nullptr, nullptr, "ps", "ps_4_0",
                    D3DCOMPILE_OPTIMIZATION_LEVEL3, 0, &ps, &errors);
    if (errors)
        errors->Release();
    if (FAILED(hr))
    {
        vs->Release();
        return false;
    }
    hr = device->CreateVertexShader(vs->GetBufferPointer(), vs->GetBufferSize(), nullptr, &g_ov11.vertex);
    if (SUCCEEDED(hr))
        hr = device->CreatePixelShader(ps->GetBufferPointer(), ps->GetBufferSize(), nullptr, &g_ov11.pixel);
    vs->Release();
    ps->Release();
    D3D11_SAMPLER_DESC sd{};
    sd.Filter = D3D11_FILTER_MIN_MAG_MIP_LINEAR;
    sd.AddressU = sd.AddressV = sd.AddressW = D3D11_TEXTURE_ADDRESS_CLAMP;
    sd.MaxLOD = D3D11_FLOAT32_MAX;
    if (SUCCEEDED(hr))
        hr = device->CreateSamplerState(&sd, &g_ov11.sampler);
    D3D11_BLEND_DESC bd{};
    bd.RenderTarget[0].BlendEnable = TRUE;
    bd.RenderTarget[0].SrcBlend = D3D11_BLEND_ONE;
    bd.RenderTarget[0].DestBlend = D3D11_BLEND_INV_SRC_ALPHA;
    bd.RenderTarget[0].BlendOp = D3D11_BLEND_OP_ADD;
    bd.RenderTarget[0].SrcBlendAlpha = D3D11_BLEND_ONE;
    bd.RenderTarget[0].DestBlendAlpha = D3D11_BLEND_INV_SRC_ALPHA;
    bd.RenderTarget[0].BlendOpAlpha = D3D11_BLEND_OP_ADD;
    bd.RenderTarget[0].RenderTargetWriteMask = D3D11_COLOR_WRITE_ENABLE_ALL;
    if (SUCCEEDED(hr))
        hr = device->CreateBlendState(&bd, &g_ov11.blend);
    D3D11_DEPTH_STENCIL_DESC dd{};
    dd.DepthEnable = FALSE;
    dd.StencilEnable = FALSE;
    if (SUCCEEDED(hr))
        hr = device->CreateDepthStencilState(&dd, &g_ov11.depth);
    D3D11_RASTERIZER_DESC rd{};
    rd.FillMode = D3D11_FILL_SOLID;
    rd.CullMode = D3D11_CULL_NONE;
    rd.DepthClipEnable = TRUE;
    rd.ScissorEnable = FALSE;
    if (SUCCEEDED(hr))
        hr = device->CreateRasterizerState(&rd, &g_ov11.raster);
    D3D11_BUFFER_DESC cb{};
    cb.ByteWidth = 16;
    cb.Usage = D3D11_USAGE_DYNAMIC;
    cb.BindFlags = D3D11_BIND_CONSTANT_BUFFER;
    cb.CPUAccessFlags = D3D11_CPU_ACCESS_WRITE;
    if (SUCCEEDED(hr))
        hr = device->CreateBuffer(&cb, nullptr, &g_ov11.constants);
    if (FAILED(hr))
    {
        overlay_d3d11_release();
        return false;
    }
    g_ov11.device = device;
    return true;
}

static void present_overlay_d3d11(IDXGISwapChain *swap)
{
    if (!swap || g_shutting_down)
        return;
    ControlSnapshot ctl;
    control_read(ctl);
    if (!ctl.valid)
        return;
    log_ensure(ctl);
    overlay_sync(ctl);
    // Compile and cache the D3D11 card pipeline once, while the overlay is idle.
    if (!g_ov11.vertex && !g_ov11_warm_failed)
    {
        ID3D11Device *warm = nullptr;
        if (SUCCEEDED(swap->GetDevice(__uuidof(ID3D11Device), reinterpret_cast<void **>(&warm))) && warm)
        {
            if (!overlay_d3d11_init(warm))
                g_ov11_warm_failed = true;
            warm->Release();
        }
    }
    if (!overlay_active(qpc_100ns()))
        return;
    DXGI_SWAP_CHAIN_DESC swap_desc{};
    if (FAILED(swap->GetDesc(&swap_desc)))
        return;
    ID3D11Device *device = nullptr;
    if (FAILED(swap->GetDevice(__uuidof(ID3D11Device), reinterpret_cast<void **>(&device))))
        return;
    ID3D11DeviceContext *context = nullptr;
    device->GetImmediateContext(&context);
    ID3D11Texture2D *back = nullptr;
    if (!context || FAILED(swap->GetBuffer(0, __uuidof(ID3D11Texture2D), reinterpret_cast<void **>(&back))))
    {
        if (context)
            context->Release();
        device->Release();
        return;
    }
    D3D11_TEXTURE2D_DESC back_desc{};
    back->GetDesc(&back_desc);
    OverlayLayout layout;
    if (!overlay_prepare(swap_desc.OutputWindow, back_desc.Width, back_desc.Height, layout) ||
        !overlay_d3d11_init(device))
    {
        back->Release();
        context->Release();
        device->Release();
        return;
    }
    if (!g_ov11.texture || g_ov11.w != layout.w || g_ov11.h != layout.h)
    {
        release_com(g_ov11.texture);
        release_com(g_ov11.view);
        D3D11_TEXTURE2D_DESC td{};
        td.Width = layout.w;
        td.Height = layout.h;
        td.MipLevels = td.ArraySize = 1;
        td.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
        td.SampleDesc.Count = 1;
        td.Usage = D3D11_USAGE_DYNAMIC;
        td.BindFlags = D3D11_BIND_SHADER_RESOURCE;
        td.CPUAccessFlags = D3D11_CPU_ACCESS_WRITE;
        if (FAILED(device->CreateTexture2D(&td, nullptr, &g_ov11.texture)) ||
            FAILED(device->CreateShaderResourceView(g_ov11.texture, nullptr, &g_ov11.view)))
        {
            back->Release();
            context->Release();
            device->Release();
            return;
        }
        g_ov11.w = layout.w;
        g_ov11.h = layout.h;
        layout.rerendered = true;
    }
    if (layout.rerendered)
    {
        if (!overlay_bitmap_pixels())
        {
            back->Release();
            context->Release();
            device->Release();
            return;
        }
        D3D11_MAPPED_SUBRESOURCE map{};
        if (FAILED(context->Map(g_ov11.texture, 0, D3D11_MAP_WRITE_DISCARD, 0, &map)))
        {
            back->Release();
            context->Release();
            device->Release();
            return;
        }
        const size_t row = static_cast<size_t>(layout.w) * 4;
        for (int y = 0; y < layout.h; ++y)
            memcpy(static_cast<uint8_t *>(map.pData) + y * map.RowPitch,
                   g_ov.upload.data() + static_cast<size_t>(y) * row, row);
        context->Unmap(g_ov11.texture, 0);
    }
    ID3D11RenderTargetView *target = nullptr;
    if (FAILED(device->CreateRenderTargetView(back, nullptr, &target)))
    {
        back->Release();
        context->Release();
        device->Release();
        return;
    }
    ID3D11RenderTargetView *old_target = nullptr;
    ID3D11DepthStencilView *old_depth = nullptr;
    ID3D11VertexShader *old_vs = nullptr;
    ID3D11PixelShader *old_ps = nullptr;
    ID3D11GeometryShader *old_gs = nullptr;
    ID3D11HullShader *old_hs = nullptr;
    ID3D11DomainShader *old_ds = nullptr;
    ID3D11InputLayout *old_layout = nullptr;
    D3D11_PRIMITIVE_TOPOLOGY old_topology{};
    ID3D11Buffer *old_constant = nullptr;
    ID3D11ShaderResourceView *old_resource = nullptr;
    ID3D11SamplerState *old_sampler = nullptr;
    ID3D11BlendState *old_blend = nullptr;
    ID3D11DepthStencilState *old_depth_state = nullptr;
    ID3D11RasterizerState *old_raster = nullptr;
    UINT old_stencil_ref = 0;
    FLOAT old_factor[4]{};
    UINT old_mask = 0;
    D3D11_VIEWPORT old_viewports[D3D11_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE]{};
    UINT old_count = _countof(old_viewports);
    D3D11_RECT old_scissors[D3D11_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE]{};
    UINT old_scissor_count = _countof(old_scissors);
    context->OMGetRenderTargets(1, &old_target, &old_depth);
    context->OMGetBlendState(&old_blend, old_factor, &old_mask);
    context->OMGetDepthStencilState(&old_depth_state, &old_stencil_ref);
    context->VSGetShader(&old_vs, nullptr, nullptr);
    context->PSGetShader(&old_ps, nullptr, nullptr);
    context->GSGetShader(&old_gs, nullptr, nullptr);
    context->HSGetShader(&old_hs, nullptr, nullptr);
    context->DSGetShader(&old_ds, nullptr, nullptr);
    context->IAGetInputLayout(&old_layout);
    context->IAGetPrimitiveTopology(&old_topology);
    context->VSGetConstantBuffers(0, 1, &old_constant);
    context->PSGetShaderResources(0, 1, &old_resource);
    context->PSGetSamplers(0, 1, &old_sampler);
    context->RSGetViewports(&old_count, old_viewports);
    context->RSGetScissorRects(&old_scissor_count, old_scissors);
    context->RSGetState(&old_raster);
    D3D11_MAPPED_SUBRESOURCE cb{};
    if (SUCCEEDED(context->Map(g_ov11.constants, 0, D3D11_MAP_WRITE_DISCARD, 0, &cb)))
    {
        float *r = static_cast<float *>(cb.pData);
        r[0] = layout.x * 2.0f / back_desc.Width - 1.0f;
        r[1] = 1.0f - layout.y * 2.0f / back_desc.Height;
        r[2] = (layout.x + layout.w) * 2.0f / back_desc.Width - 1.0f;
        r[3] = 1.0f - (layout.y + layout.h) * 2.0f / back_desc.Height;
        context->Unmap(g_ov11.constants, 0);
        D3D11_VIEWPORT vp{0, 0, static_cast<float>(back_desc.Width), static_cast<float>(back_desc.Height), 0, 1};
        const float factor[4] = {0, 0, 0, 0};
        context->OMSetRenderTargets(1, &target, nullptr);
        context->OMSetBlendState(g_ov11.blend, factor, 0xffffffff);
        context->OMSetDepthStencilState(g_ov11.depth, 0);
        context->RSSetViewports(1, &vp);
        context->RSSetState(g_ov11.raster);
        context->IASetInputLayout(nullptr);
        context->IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        context->VSSetShader(g_ov11.vertex, nullptr, 0);
        context->VSSetConstantBuffers(0, 1, &g_ov11.constants);
        context->PSSetShader(g_ov11.pixel, nullptr, 0);
        context->GSSetShader(nullptr, nullptr, 0);
        context->HSSetShader(nullptr, nullptr, 0);
        context->DSSetShader(nullptr, nullptr, 0);
        context->PSSetShaderResources(0, 1, &g_ov11.view);
        context->PSSetSamplers(0, 1, &g_ov11.sampler);
        context->Draw(6, 0);
        ID3D11ShaderResourceView *null_view = nullptr;
        context->PSSetShaderResources(0, 1, &null_view);
    }
    context->OMSetRenderTargets(1, &old_target, old_depth);
    context->OMSetBlendState(old_blend, old_factor, old_mask);
    context->OMSetDepthStencilState(old_depth_state, old_stencil_ref);
    context->VSSetShader(old_vs, nullptr, 0);
    context->PSSetShader(old_ps, nullptr, 0);
    context->GSSetShader(old_gs, nullptr, 0);
    context->HSSetShader(old_hs, nullptr, 0);
    context->DSSetShader(old_ds, nullptr, 0);
    context->IASetInputLayout(old_layout);
    context->IASetPrimitiveTopology(old_topology);
    context->VSSetConstantBuffers(0, 1, &old_constant);
    context->PSSetShaderResources(0, 1, &old_resource);
    context->PSSetSamplers(0, 1, &old_sampler);
    if (old_count)
        context->RSSetViewports(old_count, old_viewports);
    if (old_scissor_count)
        context->RSSetScissorRects(old_scissor_count, old_scissors);
    context->RSSetState(old_raster);
    release_com(old_target);
    release_com(old_depth);
    release_com(old_vs);
    release_com(old_ps);
    release_com(old_gs);
    release_com(old_hs);
    release_com(old_ds);
    release_com(old_blend);
    release_com(old_depth_state);
    release_com(old_raster);
    release_com(old_layout);
    release_com(old_constant);
    release_com(old_resource);
    release_com(old_sampler);
    target->Release();
    back->Release();
    context->Release();
    device->Release();
}

struct D3D12Overlay
{
    ID3D12Device *device = nullptr;
    ID3D12CommandQueue *queue = nullptr;
    ID3D12CommandAllocator *allocator = nullptr;
    ID3D12GraphicsCommandList *list = nullptr;
    ID3D12Fence *fence = nullptr;
    ID3D12RootSignature *root = nullptr;
    ID3D12PipelineState *pipeline = nullptr;
    ID3D12Resource *texture = nullptr;
    ID3D12Resource *upload = nullptr;
    ID3D12DescriptorHeap *srv_heap = nullptr;
    ID3D12DescriptorHeap *rtv_heap = nullptr;
    HANDLE event_handle = nullptr;
    UINT64 fence_value = 0;
    D3D12_PLACED_SUBRESOURCE_FOOTPRINT footprint{};
    UINT64 upload_bytes = 0;
    int w = 0;
    int h = 0;
    DXGI_FORMAT format = DXGI_FORMAT_UNKNOWN;
    bool texture_ready = false;
};
static D3D12Overlay g_ov12;

static bool overlay_d3d12_wait()
{
    if (!g_ov12.fence || !g_ov12.event_handle || !g_ov12.fence_value)
        return true;
    if (g_ov12.fence->GetCompletedValue() >= g_ov12.fence_value)
        return true;
    if (FAILED(g_ov12.fence->SetEventOnCompletion(g_ov12.fence_value, g_ov12.event_handle)))
        return false;
    return WaitForSingleObject(g_ov12.event_handle, 2000) == WAIT_OBJECT_0;
}

static void overlay_d3d12_release()
{
    if (!g_shutting_down)
        overlay_d3d12_wait();
    release_com(g_ov12.texture);
    release_com(g_ov12.upload);
    release_com(g_ov12.pipeline);
    release_com(g_ov12.root);
    release_com(g_ov12.srv_heap);
    release_com(g_ov12.rtv_heap);
    release_com(g_ov12.list);
    release_com(g_ov12.allocator);
    release_com(g_ov12.fence);
    release_com(g_ov12.queue);
    release_com(g_ov12.device);
    if (g_ov12.event_handle)
        CloseHandle(g_ov12.event_handle);
    g_ov12 = {};
}

static bool overlay_d3d12_pipeline(DXGI_FORMAT format)
{
    if (g_ov12.pipeline && g_ov12.format == format)
        return true;
    release_com(g_ov12.pipeline);
    static const char *source =
        "cbuffer C:register(b0){float4 r;} struct O{float4 p:SV_Position;float2 u:TEXCOORD0;} ;"
        "O vs(uint i:SV_VertexID){float2 q[6]={float2(0,0),float2(1,0),float2(0,1),"
        "float2(0,1),float2(1,0),float2(1,1)};O o;o.u=q[i];o.p=float4(lerp(r.xy,r.zw,q[i]),0,1);return o;}"
        "Texture2D t:register(t0);SamplerState s:register(s0);float4 ps(O i):SV_Target{return t.Sample(s,i.u);}";
    ID3DBlob *vs = nullptr;
    ID3DBlob *ps = nullptr;
    ID3DBlob *errors = nullptr;
    HRESULT hr = D3DCompile(source, strlen(source), nullptr, nullptr, nullptr, "vs", "vs_5_0",
                            D3DCOMPILE_OPTIMIZATION_LEVEL3, 0, &vs, &errors);
    release_com(errors);
    if (SUCCEEDED(hr))
        hr = D3DCompile(source, strlen(source), nullptr, nullptr, nullptr, "ps", "ps_5_0",
                        D3DCOMPILE_OPTIMIZATION_LEVEL3, 0, &ps, &errors);
    release_com(errors);
    if (FAILED(hr))
    {
        release_com(vs);
        release_com(ps);
        return false;
    }
    D3D12_GRAPHICS_PIPELINE_STATE_DESC desc{};
    desc.pRootSignature = g_ov12.root;
    desc.VS = {vs->GetBufferPointer(), vs->GetBufferSize()};
    desc.PS = {ps->GetBufferPointer(), ps->GetBufferSize()};
    desc.BlendState.RenderTarget[0].BlendEnable = TRUE;
    desc.BlendState.RenderTarget[0].SrcBlend = D3D12_BLEND_ONE;
    desc.BlendState.RenderTarget[0].DestBlend = D3D12_BLEND_INV_SRC_ALPHA;
    desc.BlendState.RenderTarget[0].BlendOp = D3D12_BLEND_OP_ADD;
    desc.BlendState.RenderTarget[0].SrcBlendAlpha = D3D12_BLEND_ONE;
    desc.BlendState.RenderTarget[0].DestBlendAlpha = D3D12_BLEND_INV_SRC_ALPHA;
    desc.BlendState.RenderTarget[0].BlendOpAlpha = D3D12_BLEND_OP_ADD;
    desc.BlendState.RenderTarget[0].RenderTargetWriteMask = D3D12_COLOR_WRITE_ENABLE_ALL;
    desc.SampleMask = UINT_MAX;
    desc.RasterizerState.FillMode = D3D12_FILL_MODE_SOLID;
    desc.RasterizerState.CullMode = D3D12_CULL_MODE_NONE;
    desc.RasterizerState.DepthClipEnable = TRUE;
    desc.DepthStencilState.DepthEnable = FALSE;
    desc.DepthStencilState.StencilEnable = FALSE;
    desc.PrimitiveTopologyType = D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE;
    desc.NumRenderTargets = 1;
    desc.RTVFormats[0] = format;
    desc.SampleDesc.Count = 1;
    hr = g_ov12.device->CreateGraphicsPipelineState(&desc, __uuidof(ID3D12PipelineState),
                                                    reinterpret_cast<void **>(&g_ov12.pipeline));
    release_com(vs);
    release_com(ps);
    if (SUCCEEDED(hr))
        g_ov12.format = format;
    return SUCCEEDED(hr);
}

static bool overlay_d3d12_init(ID3D12Device *device, ID3D12CommandQueue *queue)
{
    if (g_ov12.device == device && g_ov12.queue == queue && g_ov12.root)
        return true;
    overlay_d3d12_release();
    g_ov12.device = device;
    device->AddRef();
    g_ov12.queue = queue;
    queue->AddRef();

    D3D12_DESCRIPTOR_RANGE range{};
    range.RangeType = D3D12_DESCRIPTOR_RANGE_TYPE_SRV;
    range.NumDescriptors = 1;
    range.BaseShaderRegister = 0;
    range.OffsetInDescriptorsFromTableStart = D3D12_DESCRIPTOR_RANGE_OFFSET_APPEND;
    D3D12_ROOT_PARAMETER parameters[2]{};
    parameters[0].ParameterType = D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS;
    parameters[0].Constants.Num32BitValues = 4;
    parameters[0].Constants.ShaderRegister = 0;
    parameters[0].ShaderVisibility = D3D12_SHADER_VISIBILITY_VERTEX;
    parameters[1].ParameterType = D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE;
    parameters[1].DescriptorTable.NumDescriptorRanges = 1;
    parameters[1].DescriptorTable.pDescriptorRanges = &range;
    parameters[1].ShaderVisibility = D3D12_SHADER_VISIBILITY_PIXEL;
    D3D12_STATIC_SAMPLER_DESC sampler{};
    sampler.Filter = D3D12_FILTER_MIN_MAG_MIP_LINEAR;
    sampler.AddressU = sampler.AddressV = sampler.AddressW = D3D12_TEXTURE_ADDRESS_MODE_CLAMP;
    sampler.ShaderVisibility = D3D12_SHADER_VISIBILITY_PIXEL;
    sampler.MaxLOD = D3D12_FLOAT32_MAX;
    D3D12_ROOT_SIGNATURE_DESC root_desc{};
    root_desc.NumParameters = 2;
    root_desc.pParameters = parameters;
    root_desc.NumStaticSamplers = 1;
    root_desc.pStaticSamplers = &sampler;
    root_desc.Flags = D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT;
    HMODULE d3d12 = GetModuleHandleW(L"d3d12.dll");
    typedef HRESULT(WINAPI * SerializeFn)(const D3D12_ROOT_SIGNATURE_DESC *, D3D_ROOT_SIGNATURE_VERSION,
                                          ID3DBlob **, ID3DBlob **);
    auto serialize = d3d12 ? reinterpret_cast<SerializeFn>(
                                 GetProcAddress(d3d12, "D3D12SerializeRootSignature"))
                           : nullptr;
    ID3DBlob *blob = nullptr;
    ID3DBlob *errors = nullptr;
    HRESULT hr = serialize ? serialize(&root_desc, D3D_ROOT_SIGNATURE_VERSION_1, &blob, &errors) : E_FAIL;
    release_com(errors);
    if (SUCCEEDED(hr))
        hr = device->CreateRootSignature(0, blob->GetBufferPointer(), blob->GetBufferSize(),
                                         __uuidof(ID3D12RootSignature),
                                         reinterpret_cast<void **>(&g_ov12.root));
    release_com(blob);
    if (SUCCEEDED(hr))
        hr = device->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT,
                                            __uuidof(ID3D12CommandAllocator), reinterpret_cast<void **>(&g_ov12.allocator));
    if (SUCCEEDED(hr))
        hr = device->CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, g_ov12.allocator, nullptr,
                                       __uuidof(ID3D12GraphicsCommandList), reinterpret_cast<void **>(&g_ov12.list));
    if (SUCCEEDED(hr))
        hr = g_ov12.list->Close();
    if (SUCCEEDED(hr))
        hr = device->CreateFence(0, D3D12_FENCE_FLAG_NONE, __uuidof(ID3D12Fence),
                                 reinterpret_cast<void **>(&g_ov12.fence));
    D3D12_DESCRIPTOR_HEAP_DESC heap{};
    heap.NumDescriptors = 1;
    heap.Type = D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV;
    heap.Flags = D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE;
    if (SUCCEEDED(hr))
        hr = device->CreateDescriptorHeap(&heap, __uuidof(ID3D12DescriptorHeap),
                                          reinterpret_cast<void **>(&g_ov12.srv_heap));
    heap.Type = D3D12_DESCRIPTOR_HEAP_TYPE_RTV;
    heap.Flags = D3D12_DESCRIPTOR_HEAP_FLAG_NONE;
    if (SUCCEEDED(hr))
        hr = device->CreateDescriptorHeap(&heap, __uuidof(ID3D12DescriptorHeap),
                                          reinterpret_cast<void **>(&g_ov12.rtv_heap));
    if (SUCCEEDED(hr))
        g_ov12.event_handle = CreateEventW(nullptr, FALSE, FALSE, nullptr);
    if (FAILED(hr) || !g_ov12.event_handle)
    {
        overlay_d3d12_release();
        return false;
    }
    return true;
}

static bool overlay_d3d12_texture(int w, int h)
{
    if (g_ov12.texture && g_ov12.w == w && g_ov12.h == h)
        return true;
    if (!overlay_d3d12_wait())
        return false;
    release_com(g_ov12.texture);
    release_com(g_ov12.upload);
    D3D12_HEAP_PROPERTIES default_heap{};
    default_heap.Type = D3D12_HEAP_TYPE_DEFAULT;
    default_heap.CreationNodeMask = default_heap.VisibleNodeMask = 1;
    D3D12_RESOURCE_DESC texture{};
    texture.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE2D;
    texture.Width = w;
    texture.Height = h;
    texture.DepthOrArraySize = 1;
    texture.MipLevels = 1;
    texture.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
    texture.SampleDesc.Count = 1;
    texture.Layout = D3D12_TEXTURE_LAYOUT_UNKNOWN;
    HRESULT hr = g_ov12.device->CreateCommittedResource(&default_heap, D3D12_HEAP_FLAG_NONE,
                                                        &texture, D3D12_RESOURCE_STATE_COPY_DEST, nullptr, __uuidof(ID3D12Resource),
                                                        reinterpret_cast<void **>(&g_ov12.texture));
    UINT rows = 0;
    UINT64 row_bytes = 0;
    if (SUCCEEDED(hr))
        g_ov12.device->GetCopyableFootprints(&texture, 0, 1, 0, &g_ov12.footprint,
                                             &rows, &row_bytes, &g_ov12.upload_bytes);
    D3D12_HEAP_PROPERTIES upload_heap{};
    upload_heap.Type = D3D12_HEAP_TYPE_UPLOAD;
    upload_heap.CreationNodeMask = upload_heap.VisibleNodeMask = 1;
    D3D12_RESOURCE_DESC upload{};
    upload.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER;
    upload.Width = g_ov12.upload_bytes;
    upload.Height = 1;
    upload.DepthOrArraySize = 1;
    upload.MipLevels = 1;
    upload.SampleDesc.Count = 1;
    upload.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
    if (SUCCEEDED(hr))
        hr = g_ov12.device->CreateCommittedResource(&upload_heap, D3D12_HEAP_FLAG_NONE, &upload,
                                                    D3D12_RESOURCE_STATE_GENERIC_READ, nullptr, __uuidof(ID3D12Resource),
                                                    reinterpret_cast<void **>(&g_ov12.upload));
    if (FAILED(hr))
    {
        release_com(g_ov12.texture);
        release_com(g_ov12.upload);
        return false;
    }
    D3D12_SHADER_RESOURCE_VIEW_DESC view{};
    view.Format = texture.Format;
    view.ViewDimension = D3D12_SRV_DIMENSION_TEXTURE2D;
    view.Shader4ComponentMapping = D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING;
    view.Texture2D.MipLevels = 1;
    g_ov12.device->CreateShaderResourceView(g_ov12.texture, &view,
                                            g_ov12.srv_heap->GetCPUDescriptorHandleForHeapStart());
    g_ov12.w = w;
    g_ov12.h = h;
    g_ov12.texture_ready = false;
    return true;
}

static void present_overlay_d3d12(IDXGISwapChain *swap, ID3D12CommandQueue *queue)
{
    if (!swap || !queue || g_shutting_down)
        return;
    ControlSnapshot ctl;
    control_read(ctl);
    if (!ctl.valid)
        return;
    log_ensure(ctl);
    overlay_sync(ctl);
    if (!overlay_active(qpc_100ns()))
        return;
    ID3D12Device *device = nullptr;
    IDXGISwapChain3 *swap3 = nullptr;
    DXGI_SWAP_CHAIN_DESC swap_desc{};
    if (FAILED(swap->GetDevice(__uuidof(ID3D12Device), reinterpret_cast<void **>(&device))) ||
        FAILED(swap->QueryInterface(__uuidof(IDXGISwapChain3), reinterpret_cast<void **>(&swap3))) ||
        FAILED(swap->GetDesc(&swap_desc)))
    {
        release_com(device);
        release_com(swap3);
        return;
    }
    ID3D12Resource *back = nullptr;
    if (FAILED(swap3->GetBuffer(swap3->GetCurrentBackBufferIndex(), __uuidof(ID3D12Resource),
                                reinterpret_cast<void **>(&back))))
    {
        swap3->Release();
        device->Release();
        return;
    }
    D3D12_RESOURCE_DESC back_desc = back->GetDesc();
    OverlayLayout layout;
    bool ok = back_desc.SampleDesc.Count == 1 &&
              overlay_prepare(swap_desc.OutputWindow, static_cast<int>(back_desc.Width),
                              static_cast<int>(back_desc.Height), layout) &&
              overlay_d3d12_init(device, queue) && overlay_d3d12_pipeline(back_desc.Format) &&
              overlay_d3d12_texture(layout.w, layout.h) && overlay_d3d12_wait();
    if (!ok || FAILED(g_ov12.allocator->Reset()) ||
        FAILED(g_ov12.list->Reset(g_ov12.allocator, g_ov12.pipeline)))
    {
        back->Release();
        swap3->Release();
        device->Release();
        return;
    }
    if (layout.rerendered || !g_ov12.texture_ready)
    {
        if (!overlay_bitmap_pixels())
        {
            g_ov12.list->Close();
            back->Release();
            swap3->Release();
            device->Release();
            return;
        }
        void *mapped = nullptr;
        D3D12_RANGE read_range{0, 0};
        if (FAILED(g_ov12.upload->Map(0, &read_range, &mapped)))
        {
            g_ov12.list->Close();
            back->Release();
            swap3->Release();
            device->Release();
            return;
        }
        const size_t row = static_cast<size_t>(layout.w) * 4;
        uint8_t *destination = static_cast<uint8_t *>(mapped) + g_ov12.footprint.Offset;
        for (int y = 0; y < layout.h; ++y)
            memcpy(destination + static_cast<size_t>(y) * g_ov12.footprint.Footprint.RowPitch,
                   g_ov.upload.data() + static_cast<size_t>(y) * row, row);
        D3D12_RANGE written{0, static_cast<SIZE_T>(g_ov12.upload_bytes)};
        g_ov12.upload->Unmap(0, &written);
        if (g_ov12.texture_ready)
        {
            D3D12_RESOURCE_BARRIER to_copy{};
            to_copy.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
            to_copy.Transition.pResource = g_ov12.texture;
            to_copy.Transition.Subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
            to_copy.Transition.StateBefore = D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE;
            to_copy.Transition.StateAfter = D3D12_RESOURCE_STATE_COPY_DEST;
            g_ov12.list->ResourceBarrier(1, &to_copy);
        }
        D3D12_TEXTURE_COPY_LOCATION source{};
        source.pResource = g_ov12.upload;
        source.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
        source.PlacedFootprint = g_ov12.footprint;
        D3D12_TEXTURE_COPY_LOCATION destination_texture{};
        destination_texture.pResource = g_ov12.texture;
        destination_texture.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
        g_ov12.list->CopyTextureRegion(&destination_texture, 0, 0, 0, &source, nullptr);
        D3D12_RESOURCE_BARRIER to_shader{};
        to_shader.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
        to_shader.Transition.pResource = g_ov12.texture;
        to_shader.Transition.Subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
        to_shader.Transition.StateBefore = D3D12_RESOURCE_STATE_COPY_DEST;
        to_shader.Transition.StateAfter = D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE;
        g_ov12.list->ResourceBarrier(1, &to_shader);
        g_ov12.texture_ready = true;
    }
    D3D12_RESOURCE_BARRIER back_barriers[2]{};
    for (int i = 0; i < 2; ++i)
    {
        back_barriers[i].Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
        back_barriers[i].Transition.pResource = back;
        back_barriers[i].Transition.Subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
    }
    back_barriers[0].Transition.StateBefore = D3D12_RESOURCE_STATE_PRESENT;
    back_barriers[0].Transition.StateAfter = D3D12_RESOURCE_STATE_RENDER_TARGET;
    back_barriers[1].Transition.StateBefore = D3D12_RESOURCE_STATE_RENDER_TARGET;
    back_barriers[1].Transition.StateAfter = D3D12_RESOURCE_STATE_PRESENT;
    g_ov12.list->ResourceBarrier(1, &back_barriers[0]);
    D3D12_CPU_DESCRIPTOR_HANDLE target = g_ov12.rtv_heap->GetCPUDescriptorHandleForHeapStart();
    g_ov12.device->CreateRenderTargetView(back, nullptr, target);
    D3D12_VIEWPORT viewport{0, 0, static_cast<float>(back_desc.Width),
                            static_cast<float>(back_desc.Height), 0, 1};
    D3D12_RECT scissor{0, 0, static_cast<LONG>(back_desc.Width), static_cast<LONG>(back_desc.Height)};
    float bounds[4] = {
        layout.x * 2.0f / back_desc.Width - 1.0f,
        1.0f - layout.y * 2.0f / back_desc.Height,
        (layout.x + layout.w) * 2.0f / back_desc.Width - 1.0f,
        1.0f - (layout.y + layout.h) * 2.0f / back_desc.Height};
    ID3D12DescriptorHeap *heaps[] = {g_ov12.srv_heap};
    g_ov12.list->SetGraphicsRootSignature(g_ov12.root);
    g_ov12.list->SetDescriptorHeaps(1, heaps);
    g_ov12.list->SetGraphicsRoot32BitConstants(0, 4, bounds, 0);
    g_ov12.list->SetGraphicsRootDescriptorTable(1, g_ov12.srv_heap->GetGPUDescriptorHandleForHeapStart());
    g_ov12.list->RSSetViewports(1, &viewport);
    g_ov12.list->RSSetScissorRects(1, &scissor);
    g_ov12.list->OMSetRenderTargets(1, &target, FALSE, nullptr);
    g_ov12.list->IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
    g_ov12.list->DrawInstanced(6, 1, 0, 0);
    g_ov12.list->ResourceBarrier(1, &back_barriers[1]);
    if (SUCCEEDED(g_ov12.list->Close()))
    {
        ID3D12CommandList *lists[] = {g_ov12.list};
        queue->ExecuteCommandLists(1, lists);
        const UINT64 value = ++g_ov12.fence_value;
        if (FAILED(queue->Signal(g_ov12.fence, value)))
            g_ov12.fence_value = 0;
    }
    back->Release();
    swap3->Release();
    device->Release();
}

extern "C" __declspec(dllexport) void WINAPI IR_OverlayPresent()
{
    PresentGuard guard;
    present_overlay();
}

extern "C" __declspec(dllexport) void WINAPI IR_OverlayPresentD3D9(void *device)
{
    PresentGuard guard;
    present_overlay_d3d9(static_cast<IDirect3DDevice9 *>(device));
}

extern "C" __declspec(dllexport) void WINAPI IR_OverlayPresentD3D11(void *swap)
{
    PresentGuard guard;
    present_overlay_d3d11(static_cast<IDXGISwapChain *>(swap));
}

extern "C" __declspec(dllexport) void WINAPI IR_OverlayPresentD3D12(void *swap, void *queue)
{
    PresentGuard guard;
    present_overlay_d3d12(static_cast<IDXGISwapChain *>(swap),
                          static_cast<ID3D12CommandQueue *>(queue));
}

typedef BOOL(WINAPI *SwapFn)(HDC);
static SwapFn g_swap_original;
static SwapFn g_wgl_original;

static BOOL WINAPI swap_detour(HDC hdc)
{
    const bool outer = (t_depth++ == 0);
    if (outer)
        IR_OverlayPresent();
    BOOL result = g_swap_original ? g_swap_original(hdc) : FALSE;
    t_depth--;
    return result;
}

static BOOL WINAPI wgl_swap_detour(HDC hdc)
{
    const bool outer = (t_depth++ == 0);
    if (outer)
        IR_OverlayPresent();
    BOOL result = g_wgl_original ? g_wgl_original(hdc) : FALSE;
    t_depth--;
    return result;
}


// ---------------------------------------------------------------------------
// Standalone present hooks. The overlay DLL is injected on its own (the video is
// recorded outside the game with WGC/DXGI), so it must find the game's present
// calls itself instead of being driven by capture_hook.dll. Only entry points of
// graphics APIs the game already loaded are hooked, and the hooks never touch
// frame contents: they draw the small clip card while it is visible and
// otherwise return immediately.
// ---------------------------------------------------------------------------
typedef HRESULT(STDMETHODCALLTYPE *DxgiPresentFn)(IDXGISwapChain *, UINT, UINT);
typedef HRESULT(STDMETHODCALLTYPE *DxgiPresent1Fn)(IDXGISwapChain1 *, UINT, UINT,
                                                  const DXGI_PRESENT_PARAMETERS *);
typedef void(STDMETHODCALLTYPE *D3D12ExecuteFn)(ID3D12CommandQueue *, UINT,
                                               ID3D12CommandList *const *);
typedef HRESULT(STDMETHODCALLTYPE *D3D9PresentFn)(IDirect3DDevice9 *, const RECT *, const RECT *,
                                                 HWND, const RGNDATA *);

static DxgiPresentFn g_dxgi_present_original;
static DxgiPresent1Fn g_dxgi_present1_original;
static D3D12ExecuteFn g_d3d12_execute_original;
static D3D9PresentFn g_d3d9_present_original;
static SRWLOCK g_queue_lock = SRWLOCK_INIT;
static ID3D12CommandQueue *g_direct_queue;

static ID3D12CommandQueue *direct_queue_for(ID3D12Device *device)
{
    ID3D12CommandQueue *result = nullptr;
    AcquireSRWLockShared(&g_queue_lock);
    ID3D12CommandQueue *queue = g_direct_queue;
    if (queue)
    {
        ID3D12Device *queue_device = nullptr;
        if (SUCCEEDED(queue->GetDevice(__uuidof(ID3D12Device),
                                       reinterpret_cast<void **>(&queue_device))))
        {
            if (queue_device == device)
            {
                queue->AddRef();
                result = queue;
            }
            queue_device->Release();
        }
    }
    ReleaseSRWLockShared(&g_queue_lock);
    return result;
}

static void overlay_for_swap(IDXGISwapChain *swap)
{
    ID3D12Device *device12 = nullptr;
    if (SUCCEEDED(swap->GetDevice(__uuidof(ID3D12Device), reinterpret_cast<void **>(&device12))))
    {
        ID3D12CommandQueue *queue = direct_queue_for(device12);
        device12->Release();
        if (queue)
        {
            IR_OverlayPresentD3D12(swap, queue);
            queue->Release();
        }
    }
    else
    {
        IR_OverlayPresentD3D11(swap);
    }
}

static HRESULT STDMETHODCALLTYPE dxgi_present_detour(IDXGISwapChain *swap, UINT sync, UINT flags)
{
    // Counted for the whole call, including the original, so unloading waits
    // for threads still blocked inside Present.
    PresentGuard in_detour;
    const bool outer = (t_depth++ == 0);
    if (outer && !(flags & DXGI_PRESENT_TEST))
        overlay_for_swap(swap);
    HRESULT hr = g_dxgi_present_original ? g_dxgi_present_original(swap, sync, flags)
                                         : DXGI_ERROR_INVALID_CALL;
    t_depth--;
    return hr;
}

static HRESULT STDMETHODCALLTYPE dxgi_present1_detour(IDXGISwapChain1 *swap, UINT sync, UINT flags,
                                                      const DXGI_PRESENT_PARAMETERS *params)
{
    // Counted for the whole call, including the original, so unloading waits
    // for threads still blocked inside Present.
    PresentGuard in_detour;
    const bool outer = (t_depth++ == 0);
    if (outer && !(flags & DXGI_PRESENT_TEST))
        overlay_for_swap(swap);
    HRESULT hr = g_dxgi_present1_original ? g_dxgi_present1_original(swap, sync, flags, params)
                                          : DXGI_ERROR_INVALID_CALL;
    t_depth--;
    return hr;
}

static void STDMETHODCALLTYPE d3d12_execute_detour(ID3D12CommandQueue *queue, UINT count,
                                                   ID3D12CommandList *const *lists)
{
    // Counted for the whole call, including the original, so unloading waits
    // for threads still blocked inside Present.
    PresentGuard in_detour;
    if (g_d3d12_execute_original)
        g_d3d12_execute_original(queue, count, lists);
    if (queue->GetDesc().Type != D3D12_COMMAND_LIST_TYPE_DIRECT)
        return;
    AcquireSRWLockExclusive(&g_queue_lock);
    if (g_direct_queue != queue)
    {
        queue->AddRef();
        ID3D12CommandQueue *old = g_direct_queue;
        g_direct_queue = queue;
        if (old)
            old->Release();
    }
    ReleaseSRWLockExclusive(&g_queue_lock);
}

static HRESULT STDMETHODCALLTYPE d3d9_present_detour(IDirect3DDevice9 *device, const RECT *src,
                                                     const RECT *dst, HWND window,
                                                     const RGNDATA *dirty)
{
    // Counted for the whole call, including the original, so unloading waits
    // for threads still blocked inside Present.
    PresentGuard in_detour;
    const bool outer = (t_depth++ == 0);
    if (outer)
        IR_OverlayPresentD3D9(device);
    HRESULT hr = g_d3d9_present_original ? g_d3d9_present_original(device, src, dst, window, dirty)
                                         : D3DERR_INVALIDCALL;
    t_depth--;
    return hr;
}

static void *vtable_entry(void *object, size_t index)
{
    return (*reinterpret_cast<void ***>(object))[index];
}

static HWND create_probe_window()
{
    return CreateWindowExW(0, L"STATIC", L"InstantReplayOverlayProbe", WS_POPUP, 0, 0, 8, 8,
                           nullptr, nullptr, GetModuleHandleW(nullptr), nullptr);
}

static bool install_dxgi_present_hooks()
{
    HMODULE d3d11 = LoadLibraryW(L"d3d11.dll"); // system DLL, needed for the probe swap chain
    auto create = d3d11 ? reinterpret_cast<PFN_D3D11_CREATE_DEVICE_AND_SWAP_CHAIN>(
                              GetProcAddress(d3d11, "D3D11CreateDeviceAndSwapChain"))
                        : nullptr;
    if (!create)
        return false;
    HWND window = create_probe_window();
    if (!window)
        return false;

    DXGI_SWAP_CHAIN_DESC desc = {};
    desc.BufferDesc.Width = 8;
    desc.BufferDesc.Height = 8;
    desc.BufferDesc.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
    desc.SampleDesc.Count = 1;
    desc.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
    desc.BufferCount = 2;
    desc.OutputWindow = window;
    desc.Windowed = TRUE;
    desc.SwapEffect = DXGI_SWAP_EFFECT_DISCARD;

    IDXGISwapChain *swap = nullptr;
    ID3D11Device *device = nullptr;
    ID3D11DeviceContext *context = nullptr;
    HRESULT hr = create(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                        nullptr, 0, D3D11_SDK_VERSION, &desc, &swap, &device, nullptr, &context);
    bool hooked = false;
    if (SUCCEEDED(hr) && swap)
    {
        void *present = vtable_entry(swap, 8);
        hooked = MH_CreateHook(present, reinterpret_cast<void *>(&dxgi_present_detour),
                               reinterpret_cast<void **>(&g_dxgi_present_original)) == MH_OK;
        IDXGISwapChain1 *swap1 = nullptr;
        if (hooked && SUCCEEDED(swap->QueryInterface(__uuidof(IDXGISwapChain1),
                                                     reinterpret_cast<void **>(&swap1))))
        {
            MH_CreateHook(vtable_entry(swap1, 22), reinterpret_cast<void *>(&dxgi_present1_detour),
                          reinterpret_cast<void **>(&g_dxgi_present1_original));
            swap1->Release();
        }
    }
    if (swap)
        swap->Release();
    if (context)
        context->Release();
    if (device)
        device->Release();
    DestroyWindow(window);
    return hooked;
}

static bool install_d3d12_queue_hook()
{
    HMODULE d3d12 = GetModuleHandleW(L"d3d12.dll");
    auto create = d3d12 ? reinterpret_cast<PFN_D3D12_CREATE_DEVICE>(
                              GetProcAddress(d3d12, "D3D12CreateDevice"))
                        : nullptr;
    if (!create)
        return false;
    ID3D12Device *device = nullptr;
    if (FAILED(create(nullptr, D3D_FEATURE_LEVEL_11_0, __uuidof(ID3D12Device),
                      reinterpret_cast<void **>(&device))) || !device)
        return false;
    D3D12_COMMAND_QUEUE_DESC desc = {};
    desc.Type = D3D12_COMMAND_LIST_TYPE_DIRECT;
    ID3D12CommandQueue *queue = nullptr;
    bool hooked = false;
    if (SUCCEEDED(device->CreateCommandQueue(&desc, __uuidof(ID3D12CommandQueue),
                                             reinterpret_cast<void **>(&queue))) && queue)
    {
        hooked = MH_CreateHook(vtable_entry(queue, 10), reinterpret_cast<void *>(&d3d12_execute_detour),
                               reinterpret_cast<void **>(&g_d3d12_execute_original)) == MH_OK;
        queue->Release();
    }
    device->Release();
    return hooked;
}

static bool install_d3d9_present_hook()
{
    HMODULE d3d9 = GetModuleHandleW(L"d3d9.dll");
    auto create = d3d9 ? reinterpret_cast<IDirect3D9 *(WINAPI *)(UINT)>(
                             GetProcAddress(d3d9, "Direct3DCreate9"))
                       : nullptr;
    if (!create)
        return false;
    IDirect3D9 *api = create(D3D_SDK_VERSION);
    if (!api)
        return false;
    HWND window = create_probe_window();
    bool hooked = false;
    if (window)
    {
        D3DPRESENT_PARAMETERS params = {};
        params.BackBufferWidth = 8;
        params.BackBufferHeight = 8;
        params.BackBufferFormat = D3DFMT_A8R8G8B8;
        params.BackBufferCount = 1;
        params.SwapEffect = D3DSWAPEFFECT_DISCARD;
        params.hDeviceWindow = window;
        params.Windowed = TRUE;
        IDirect3DDevice9 *device = nullptr;
        if (SUCCEEDED(api->CreateDevice(D3DADAPTER_DEFAULT, D3DDEVTYPE_HAL, window,
                                        D3DCREATE_SOFTWARE_VERTEXPROCESSING, &params, &device)) &&
            device)
        {
            hooked = MH_CreateHook(vtable_entry(device, 17), reinterpret_cast<void *>(&d3d9_present_detour),
                                   reinterpret_cast<void **>(&g_d3d9_present_original)) == MH_OK;
            device->Release();
        }
        DestroyWindow(window);
    }
    api->Release();
    return hooked;
}

static HANDLE g_ready_event;
static ULONG_PTR g_gdi_token;

static void set_ready()
{
    g_ready_event = CreateEventW(nullptr, TRUE, FALSE, IR_READY_NAME);
    if (g_ready_event)
        SetEvent(g_ready_event);
}

static bool install_standalone_hooks()
{
    MH_STATUS status = MH_Initialize();
    if (status != MH_OK)
    {
        hook_log(L"standalone mode: MH_Initialize failed (%d)", static_cast<int>(status));
        return false;
    }
    bool any = false;

    // Direct3D 9 / 11 / 12 through the game's own DLLs (only what it already loaded).
    // Decide from the game's modules before this DLL loads d3d11.dll for its probe.
    const bool uses_d3d12 = GetModuleHandleW(L"d3d12.dll") != nullptr;
    const bool uses_dxgi_api = uses_d3d12 || GetModuleHandleW(L"d3d11.dll") != nullptr;
    if (uses_dxgi_api && GetModuleHandleW(L"dxgi.dll") && install_dxgi_present_hooks())
    {
        hook_log(L"standalone mode: DXGI present hooked");
        any = true;
    }
    if (uses_d3d12 && install_d3d12_queue_hook())
        hook_log(L"standalone mode: D3D12 queue hooked");
    if (!uses_dxgi_api && GetModuleHandleW(L"d3d9.dll") && install_d3d9_present_hook())
    {
        hook_log(L"standalone mode: D3D9 present hooked");
        any = true;
    }

    // OpenGL
    if (GetModuleHandleW(L"opengl32.dll"))
    {
        HMODULE gdi32 = GetModuleHandleW(L"gdi32.dll");
        if (gdi32)
        {
            void *target = reinterpret_cast<void *>(GetProcAddress(gdi32, "SwapBuffers"));
            if (target && MH_CreateHook(target, reinterpret_cast<void *>(&swap_detour),
                                        reinterpret_cast<void **>(&g_swap_original)) == MH_OK)
                any = true;
        }
        HMODULE opengl32 = GetModuleHandleW(L"opengl32.dll");
        void *target = reinterpret_cast<void *>(GetProcAddress(opengl32, "wglSwapBuffers"));
        if (target && MH_CreateHook(target, reinterpret_cast<void *>(&wgl_swap_detour),
                                    reinterpret_cast<void **>(&g_wgl_original)) == MH_OK)
            any = true;
    }

    if (!any || MH_EnableHook(MH_ALL_HOOKS) != MH_OK)
    {
        hook_log(L"standalone mode: no present entry points hooked");
        MH_DisableHook(MH_ALL_HOOKS);
        MH_Uninitialize();
        return false;
    }
    return true;
}

static DWORD WINAPI init_thread(LPVOID)
{
    LARGE_INTEGER f;
    if (!QueryPerformanceFrequency(&f) || f.QuadPart == 0)
        return 0;
    g_qpc_freq = f.QuadPart;

    GdiplusStartupInput gdi_input;
    if (GdiplusStartup(&g_gdi_token, &gdi_input, nullptr) == Ok)
    {
        InterlockedExchange(&g_gdi_ready, 1);
    }

    for (int i = 0; i < 40 && !control_open(); i++)
        Sleep(50);
    {
        ControlSnapshot ctl;
        control_read(ctl);
        log_ensure(ctl);
    }
    hook_log(L"overlay_hook loaded (pid %lu, v%u)", GetCurrentProcessId(), IR_HOOK_VERSION);

    if (GetModuleHandleW(L"capture_hook.dll"))
    {
        hook_log(L"passive mode (capture_hook.dll drives the overlay)");
        set_ready();
        return 0;
    }
    hook_log(L"standalone mode (hooking presents directly)");
    // Ready only when something was hooked, so the host falls back to the normal
    // popup instead of waiting for a card that can never be drawn.
    if (install_standalone_hooks())
        set_ready();
    else
        hook_log(L"standalone mode: no hooks installed; overlay disabled for this process");
    return 0;
}

static void shutdown()
{
    if (InterlockedCompareExchange(&g_shutdown_complete, 1, 0) != 0)
        return;
    InterlockedExchange(&g_shutting_down, 1);
    MH_DisableHook(MH_ALL_HOOKS);
    MH_Uninitialize();
    animation_worker_join();
    g_ov.texture = 0;
    overlay_d3d9_release();
    overlay_d3d11_release();
    overlay_d3d12_release();
    overlay_free_bitmap();
    animation_clear();
    render_cache_free();
    if (g_mf_started)
    {
        MFShutdown();
        g_mf_started = false;
    }
    log_close();
    control_close();
    if (g_ready_event)
    {
        CloseHandle(g_ready_event);
        g_ready_event = nullptr;
    }
    if (g_gdi_token)
    {
        GdiplusShutdown(g_gdi_token);
        g_gdi_token = 0;
    }
}

static DWORD WINAPI cooperative_shutdown_thread(LPVOID)
{
    InterlockedExchange(&g_shutting_down, 1);
    hook_log(L"cooperative shutdown requested");
    MH_DisableHook(MH_ALL_HOOKS);
    for (int i = 0; i < 500 && InterlockedCompareExchange(&g_active_present, 0, 0) != 0; ++i)
        Sleep(10);
    shutdown();
    FreeLibraryAndExitThread(g_self, 0);
    return 0;
}

extern "C" __declspec(dllexport) BOOL WINAPI IR_RequestShutdown(void *)
{
    if (InterlockedCompareExchange(&g_shutdown_started, 1, 0) != 0)
        return TRUE;
    HANDLE thread = CreateThread(nullptr, 0, cooperative_shutdown_thread, nullptr, 0, nullptr);
    if (!thread)
    {
        InterlockedExchange(&g_shutdown_started, 0);
        return FALSE;
    }
    CloseHandle(thread);
    return TRUE;
}

// Anti-cheat friendly injection, same scheme as OBS's "compatibility hook":
// the host installs a WH_GETMESSAGE hook on the game's window thread and posts
// WM_USER+432 carrying the hook handle.  Windows loads this DLL into the game
// when the thread next pumps a message; the first such message removes the
// hook again and pins the module so only IR_RequestShutdown unloads it.
#define IR_MSG_SAFE_INJECT (WM_USER + 432)
extern "C" __declspec(dllexport) LRESULT CALLBACK dummy_debug_proc(int code, WPARAM wparam,
                                                                  LPARAM lparam)
{
    static volatile LONG hooking = 1;
    MSG *msg = reinterpret_cast<MSG *>(lparam);
    if (code >= 0 && msg && msg->message == IR_MSG_SAFE_INJECT &&
        InterlockedExchange(&hooking, 0) == 1)
    {
        HMODULE pinned = nullptr;
        GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
                           reinterpret_cast<LPCWSTR>(&dummy_debug_proc), &pinned);
        UnhookWindowsHookEx(reinterpret_cast<HHOOK>(msg->lParam));
    }
    return CallNextHookEx(nullptr, code, wparam, lparam);
}

BOOL WINAPI DllMain(HMODULE module, DWORD reason, LPVOID reserved)
{
    if (reason == DLL_PROCESS_ATTACH)
    {
        g_self = module;
        DisableThreadLibraryCalls(module);
        HANDLE thread = CreateThread(nullptr, 0, init_thread, nullptr, 0, nullptr);
        if (thread)
            CloseHandle(thread);
    }
    else if (reason == DLL_PROCESS_DETACH)
    {
        if (reserved == nullptr)
        {
            shutdown();
        }
    }
    return TRUE;
}
