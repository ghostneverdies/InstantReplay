// InstantReplay native engine.
//
// Single C11 translation unit.  The capture components publish timestamped
// textures and PCM blocks into bounded replay rings.  Save jobs re-encode a
// snapshot of the selected time window through the Media Foundation pipeline.
#define WINDOWS_FOUNDATION_UNIVERSALAPICONTRACT_VERSION 0x130000
#include "engine.h"

#include <objbase.h>
#include <propkey.h>
#include <mmdeviceapi.h>
#include <functiondiscoverykeys_devpkey.h>
#include <audioclient.h>
#include <audioclientactivationparams.h>
#include <avrt.h>
#include <mfapi.h>
#include <mferror.h>
#include <mfidl.h>
#include <mfreadwrite.h>
#include <codecapi.h>
#include <evr.h>
#include <cguid.h>
#include <wmcodecdsp.h>
#include <d3d11.h>
#include <d3d11_4.h>
#include <dxgi.h>
#include <dxgi1_2.h>
#include <dxgi1_6.h>
#include <dwmapi.h>
#include <d3dcompiler.h>
#include <roapi.h>
#include <windows.graphics.capture.h>

#include "gen/ResizePassH.h"
#include "gen/ResizePassV.h"
#include "gen/ResizeLinearPassH.h"
#include "gen/ResizeLinearPassV.h"
#include "gen/ConvertSinglePass.h"
#include "gen/ConvertPass1.h"
#include "gen/ConvertPass2.h"

// Forward declarations for types defined later in this translation unit.
typedef struct IrEngine IrEngine;
typedef struct IrPacket IrPacket;

// MFSetAttributeSize/MFSetAttributeRatio are C++ inline helpers in mfapi.h;
// provide the equivalent packed UINT64 setter for this C translation unit.
// Match the SDK convention: the first value is the high DWORD (width or
// numerator), the second is the low DWORD (height or denominator).
static HRESULT IrMFSetSize(void *a, REFGUID key, UINT32 high, UINT32 low)
{
    return IMFAttributes_SetUINT64((IMFAttributes *)a, key, ((UINT64)high << 32) | low);
}

#ifndef IR_INTPTR_DEFINED
#define IR_INTPTR_DEFINED
typedef void *IntPtr;
#endif

extern __declspec(dllimport) LONG WINAPI RtlGetVersion(RTL_OSVERSIONINFOW *);

DEFINE_GUID(IID_IGraphicsCaptureItemInterop, 0x3628e81b, 0x3cac, 0x4c60, 0xb7, 0xf4, 0x23, 0xce, 0x0e, 0x0c, 0x33, 0x56);
DEFINE_GUID(IID_IClosable, 0x30d5a829, 0x7fa4, 0x4026, 0x83, 0xbb, 0xd7, 0x5b, 0xae, 0x4e, 0xa9, 0x9e);
DEFINE_GUID(IID_IGraphicsCaptureSession2, 0x2c39ae40, 0x7d2e, 0x5044, 0x80, 0x4e, 0x8b, 0x67, 0x99, 0xd4, 0xcf, 0x9e);
DEFINE_GUID(IID_IGraphicsCaptureSession3, 0xf2cdd966, 0x22ae, 0x5ea1, 0x95, 0x96, 0x3a, 0x28, 0x93, 0x44, 0xc3, 0xbe);
DEFINE_GUID(IID_IGraphicsCaptureSession5, 0x67c0ea62, 0x1f85, 0x5061, 0x92, 0x5a, 0x23, 0x9b, 0xe0, 0xac, 0x09, 0xcb);
DEFINE_GUID(IID_IGraphicsCaptureSession6, 0xd7419236, 0xbe20, 0x5e9f, 0xbc, 0xd6, 0xc4, 0xe9, 0x8f, 0xd6, 0xaf, 0xdc);
DEFINE_GUID(IID_IGraphicsCaptureItem, 0x79c3f95b, 0x31f7, 0x4ec2, 0xa4, 0x64, 0x63, 0x2e, 0xf5, 0xd3, 0x07, 0x60);
DEFINE_GUID(IID_IGraphicsCaptureItemHandler, 0xe9c610c0, 0xa68c, 0x5bd9, 0x80, 0x21, 0x85, 0x89, 0x34, 0x6e, 0xee, 0xe2);
DEFINE_GUID(IID_IDirect3D11CaptureFramePoolStatics, 0x7784056a, 0x67aa, 0x4d53, 0xae, 0x54, 0x10, 0x88, 0xd5, 0xa8, 0xca, 0x21);
DEFINE_GUID(IID_IDirect3D11CaptureFramePoolStatics2, 0x589b103f, 0x6bbc, 0x5df5, 0xa9, 0x91, 0x02, 0xe2, 0x8b, 0x3b, 0x66, 0xd5);
DEFINE_GUID(IID_IDirect3D11CaptureFramePoolHandler, 0x51a947f7, 0x79cf, 0x5a3e, 0xa3, 0xa5, 0x12, 0x89, 0xcf, 0xa6, 0xdf, 0xe8);
DEFINE_GUID(IID_IDirect3DDxgiInterfaceAccess, 0xa9b3d012, 0x3df2, 0x4ee3, 0xb8, 0xd1, 0x86, 0x95, 0xf4, 0x57, 0xd3, 0xc1);

// The Windows SDK declares these IIDs as extern const without emitting storage
// in C translation units. Define the storage here because this is the engine's
// only translation unit.
DEFINE_GUID(CLSID_MMDeviceEnumerator, 0xbcde0395, 0xe52f, 0x467c, 0x8e, 0x3d, 0xc4, 0x57, 0x92, 0x91, 0x69, 0x2e);
DEFINE_GUID(IID_IMMDeviceEnumerator, 0xa95664d2, 0x9614, 0x4f35, 0xa7, 0x46, 0xde, 0x8d, 0xb6, 0x36, 0x17, 0xe6);
DEFINE_GUID(IID_IAudioClient, 0x1cb9ad4c, 0xdbfa, 0x4c32, 0xb1, 0x78, 0xc2, 0xf5, 0x68, 0xa7, 0x03, 0xb2);
DEFINE_GUID(IID_IAudioCaptureClient, 0xc8adbd64, 0xe71e, 0x48a0, 0xa4, 0xde, 0x18, 0x5c, 0x39, 0x5c, 0xd3, 0x17);
DEFINE_GUID(IID_IAudioRenderClient, 0xf294acfc, 0x3146, 0x4483, 0xa7, 0xbf, 0xad, 0xdc, 0xa7, 0xc2, 0x60, 0xe2);
DEFINE_GUID(IID_IActivateAudioInterfaceCompletionHandler, 0x41d949ab, 0x9862, 0x444a, 0x80, 0xf6, 0xc2, 0x61, 0x33, 0x4d, 0xa5, 0xeb);
DEFINE_GUID(KSDATAFORMAT_SUBTYPE_IEEE_FLOAT, 0x00000003, 0x0000, 0x0010, 0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71);

#define IR_VIDEO_BASE      0
#define IR_VIDEO_MAIN      1
#define IR_VIDEO_HIGH      2
#define IR_VIDEO_MAIN_10   3
#define IR_VIDEO_H264      0
#define IR_VIDEO_H265      1
#define IR_VIDEO_AV1       2
#define IR_AUDIO_AAC       0
#define IR_AUDIO_FLAC      1

// IR_Start preference bits (managed RecorderEngine.ResolveEncoderPreference).
#define IR_PREF_GPU_ENCODER   0x00000400u
#define IR_PREF_PREFER_IGPU   0x00000800u

// Capture option bits (managed RecorderEngine.CaptureOptions).
#define IR_OPT_AUDIO_BITRATE_SHIFT 8
#define IR_OPT_AUDIO_BITRATE_MASK  3u
#define IR_OPT_AUDIO_CODEC_SHIFT   10
#define IR_OPT_AUDIO_CODEC_MASK    3u

#define IR_ENCODER_VIDEO_BUFFER_COUNT 8
#define IR_ENCODER_AUDIO_BUFFER_COUNT 16

#ifndef clamp
#define clamp(v, lo, hi) ((v) < (lo) ? (lo) : ((v) > (hi) ? (hi) : (v)))
#endif

typedef struct
{
    ID3D11VertexShader *vs;
    ID3D11PixelShader *ps;
    ID3D11BlendState *blend;
    ID3D11SamplerState *sampler;
    BOOL ok;
} IrCursorComp;

typedef struct
{
    uint8_t *buf;
    UINT len;
    DXGI_OUTDUPL_POINTER_SHAPE_INFO shape;
    POINT at;
    BOOL visible;
    BOOL have_shape;
    BOOL failed;
    ID3D11Texture2D *sprite;
    ID3D11ShaderResourceView *srv;
} IrCursor;

typedef struct IrDxgi
{
    ID3D11Device *dev;
    ID3D11DeviceContext *ctx;
    IDXGIOutputDuplication *dup;
    ID3D11Texture2D *base;
    ID3D11Texture2D *desk;
    ID3D11RenderTargetView *rtv;
    IrCursorComp comp;
    IrCursor cursor;
    HMONITOR monitor;
    RECT mon_rect;
    HANDLE stop_event;
    HANDLE thread;
    IrEngine *engine;
    volatile long last_hr;
    volatile long running;
} IrDxgi;

BOOL IrPushFrame(IrEngine *e, ID3D11Texture2D *src, RECT rect, int64_t pts);
HMONITOR IrMonitorAtFixed(int index);

// ---------------------------------------------------------------------------
// Process-wide helpers
// ---------------------------------------------------------------------------
static INIT_ONCE g_ir_once = INIT_ONCE_STATIC_INIT;
static CRITICAL_SECTION g_ir_error_cs;
static char g_ir_error[1024] = "ok";
static LARGE_INTEGER g_ir_qpf = { 10000000, 0 };
static volatile long g_ir_last_hr = 0;
static volatile long g_ir_last_progress = -1;

static BOOL CALLBACK IrInitOnceCallback(PINIT_ONCE Once, PVOID Parameter, PVOID *Context)
{
    (void)Once; (void)Parameter; (void)Context;
    InitializeCriticalSection(&g_ir_error_cs);
    if (!QueryPerformanceFrequency(&g_ir_qpf) || g_ir_qpf.QuadPart <= 0)
        g_ir_qpf.QuadPart = 10000000;
    return TRUE;
}

static void IrEnsureGlobal(void)
{
    InitOnceExecuteOnce(&g_ir_once, IrInitOnceCallback, NULL, NULL);
}

static void IrSetLastHr(HRESULT hr)
{
    InterlockedExchange(&g_ir_last_hr, (long)hr);
}

static void IrSetErrorUtf8(const char *text)
{
    IrEnsureGlobal();
    EnterCriticalSection(&g_ir_error_cs);
    if (text && *text)
        _snprintf_s(g_ir_error, sizeof(g_ir_error), _TRUNCATE, "%s", text);
    else
        g_ir_error[0] = '\0';
    LeaveCriticalSection(&g_ir_error_cs);
}

static void IrSetErrorWide(const wchar_t *text)
{
    if (!text)
    {
        IrSetErrorUtf8("");
        return;
    }
    char buf[1024];
    int n = WideCharToMultiByte(CP_UTF8, 0, text, -1, buf, (int)sizeof(buf), NULL, NULL);
    IrSetErrorUtf8(n > 0 ? buf : "");
}

static void IrSetErrorFmt(const char *fmt, ...)
{
    char buf[1024];
    va_list ap;
    va_start(ap, fmt);
    _vsnprintf_s(buf, sizeof(buf), _TRUNCATE, fmt, ap);
    va_end(ap);
    IrSetErrorUtf8(buf);
}

static void IrReportHr(const wchar_t *what, HRESULT hr)
{
    wchar_t buf[512];
    _snwprintf_s(buf, _countof(buf), _TRUNCATE, L"%ls (hr=0x%08lX)", what, (unsigned long)hr);
    IrSetErrorWide(buf);
    IrSetLastHr(hr);
}

static int64_t IrQpcTo100ns(int64_t qpc)
{
    if (g_ir_qpf.QuadPart == 10000000)
        return qpc;
    return (int64_t)MFllMulDiv(qpc, 10000000ULL, (uint64_t)g_ir_qpf.QuadPart, 0);
}

static int64_t Ir100nsToQpc(int64_t t)
{
    if (g_ir_qpf.QuadPart == 10000000)
        return t;
    return (int64_t)MFllMulDiv(t, (uint64_t)g_ir_qpf.QuadPart, 10000000ULL, 0);
}

static int64_t IrNow100ns(void)
{
    LARGE_INTEGER now;
    QueryPerformanceCounter(&now);
    return IrQpcTo100ns(now.QuadPart);
}

static HRESULT IrCreateDevice(ID3D11Device **out_device, BOOL gpu_encoder, BOOL prefer_integrated)
{
    *out_device = NULL;
    IDXGIAdapter *adapter = NULL;
    if (gpu_encoder)
    {
        // wcap selects the preferred adapter only for GPU encoding. If DXGI cannot
        // provide one, fall through with no explicit adapter and let D3D choose.
        IDXGIFactory *factory = NULL;
        if (SUCCEEDED(CreateDXGIFactory(&IID_IDXGIFactory, (void **)&factory)))
        {
            IDXGIFactory6 *factory6 = NULL;
            if (SUCCEEDED(IDXGIFactory_QueryInterface(factory, &IID_IDXGIFactory6, (void **)&factory6)))
            {
                DXGI_GPU_PREFERENCE preference = prefer_integrated
                    ? DXGI_GPU_PREFERENCE_MINIMUM_POWER
                    : DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE;
                if (FAILED(IDXGIFactory6_EnumAdapterByGpuPreference(factory6, 0, preference,
                                                                   &IID_IDXGIAdapter, (void **)&adapter)))
                    adapter = NULL;
                IDXGIFactory6_Release(factory6);
            }
            IDXGIFactory_Release(factory);
        }
    }

    D3D_DRIVER_TYPE driver = adapter ? D3D_DRIVER_TYPE_UNKNOWN : D3D_DRIVER_TYPE_HARDWARE;
    D3D_FEATURE_LEVEL level = D3D_FEATURE_LEVEL_11_0;
    HRESULT hr = D3D11CreateDevice(adapter, driver, NULL, 0, &level, 1, D3D11_SDK_VERSION,
                                   out_device, NULL, NULL);
    if (adapter)
        IDXGIAdapter_Release(adapter);
    return hr;
}

//





typedef struct IGraphicsCaptureItemInterop IGraphicsCaptureItemInterop;

//
// interface
//

typedef struct
{
	// public
	ID3D11Texture2D* Texture;
	uint64_t Time;
	RECT Rect;
	// private
	uint32_t Width;
	uint32_t Height;
	IUnknown* NextFrame;
}
IrWgcFrame;

typedef struct IrWgc IrWgc;

// return true to continue capture, or false stop any further catpures
typedef bool IrWgcOnFrameCallback(IrWgc* Capture, IrWgcFrame* Frame);

typedef struct IrWgc
{
	IGraphicsCaptureItemInterop* ItemInterop;
	__x_ABI_CWindows_CGraphics_CCapture_CIDirect3D11CaptureFramePoolStatics* FramePoolStatics;
	__x_ABI_CWindows_CGraphics_CCapture_CIDirect3D11CaptureFramePoolStatics2* FramePoolStatics2;
	IInspectable* Controller; // actually IDispatcherQueueController

	__x_ABI_CWindows_CGraphics_CDirectX_CDirect3D11_CIDirect3DDevice* Device;
	__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureItem* Item;
	__x_ABI_CWindows_CGraphics_CCapture_CIDirect3D11CaptureFramePool* FramePool;
	__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureSession* Session;

	IrWgcOnFrameCallback* OnFrame;
    void* OnFrameUserData;
    HRESULT RoInitHr;
	__FITypedEventHandler_2_Windows__CGraphics__CCapture__CDirect3D11CaptureFramePool_IInspectable OnFrameHandler;
	__FITypedEventHandler_2_Windows__CGraphics__CCapture__CGraphicsCaptureItem_IInspectable OnCloseHandler;
	EventRegistrationToken OnFrameToken;
	EventRegistrationToken OnCloseToken;

	__x_ABI_CWindows_CGraphics_CSizeInt32 CurrentSize;
	RECT Rect;
	HWND Window;
	bool OnlyClientArea;

	bool RestoreWindowCornerPreference;
	DWM_WINDOW_CORNER_PREFERENCE WindowCornerPreference;
}
IrWgc;

static bool IrWgc_IsSupported(void);
static bool IrWgc_CanHideMouseCursor(void);
static bool IrWgc_CanHideRecordingBorder(void);
static bool IrWgc_CanDisableRoundedCorners(void);
static bool IrWgc_CanIncludeSecondaryWindows(void);

// if OnFrame is NULL then you need to periodically call GetFrame/ReleaseFrame manually
// if OnFrame is not NULL and CallbackOnThread is false then callback will be invoked from message processing loop on the same thread as Create call
// if OnFrame is not NULL and CallbackOnThread is true, then callback will be invoked on backgroud thread
static void IrWgc_Create(IrWgc* Capture, IrWgcOnFrameCallback* OnFrame, bool CallbackOnThread);
static void IrWgc_Release(IrWgc* Capture);

static bool IrWgc_CreateForWindow(IrWgc* Capture, ID3D11Device* Device, HWND Window, bool OnlyClientArea, bool DisableRoundedCorners);
static bool IrWgc_CreateForMonitor(IrWgc* Capture, ID3D11Device* Device, HMONITOR Monitor, const RECT* Rect);
static void IrWgc_Start(IrWgc* Capture, bool WithMouseCursor, bool WithRecordingBorder, bool IncludeSecondaryWindows);
static void IrWgc_Stop(IrWgc* Capture);

static bool IrWgc_GetFrame(IrWgc* Capture, IrWgcFrame* Frame);
static void IrWgc_ReleaseFrame(IrWgc* Capture, IrWgcFrame* Frame);

//
#ifndef IR_WGC_BUFFER_COUNT
#define IR_WGC_BUFFER_COUNT 2
#endif

#ifndef IR_WGC_BUFFER_FORMAT
#define IR_WGC_BUFFER_FORMAT DirectXPixelFormat_B8G8R8A8UIntNormalized // same as DXGI_FORMAT_B8G8R8A8_UNORM
#endif

// this really should be just these three includes, but Microsoft decided to not do proper COM headers that support C :(
//#include <dispatcherqueue.h>
//#include <windows.graphics.capture.interop.h>
//#include <windows.graphics.directx.direct3d11.interop.h>

typedef enum {
	DQTAT_COM_NONE = 0,
	DQTAT_COM_ASTA = 1,
	DQTAT_COM_STA = 2,
} DISPATCHERQUEUE_THREAD_APARTMENTTYPE;

typedef enum {
	DQTYPE_THREAD_DEDICATED = 1,
	DQTYPE_THREAD_CURRENT = 2,
} DISPATCHERQUEUE_THREAD_TYPE;

typedef struct {
	DWORD                                dwSize;
	DISPATCHERQUEUE_THREAD_TYPE          threadType;
	DISPATCHERQUEUE_THREAD_APARTMENTTYPE apartmentType;
} DispatcherQueueOptions;

struct IGraphicsCaptureItemInteropVtbl
{
	STDMETHOD(QueryInterface)(IGraphicsCaptureItemInterop* This, const GUID* Riid, void** Object);
	STDMETHOD_(ULONG, AddRef)(IGraphicsCaptureItemInterop* This);
	STDMETHOD_(ULONG, Release)(IGraphicsCaptureItemInterop* This);
	STDMETHOD(CreateForWindow)(IGraphicsCaptureItemInterop* This, HWND Window, const GUID* Riid, void** Result);
	STDMETHOD(CreateForMonitor)(IGraphicsCaptureItemInterop* This, HMONITOR Monitor, const GUID* Riid, void** Result);
};

struct IGraphicsCaptureItemInterop
{
	struct IGraphicsCaptureItemInteropVtbl* lpVtbl;
};

typedef struct IDirect3DDxgiInterfaceAccess IDirect3DDxgiInterfaceAccess;

struct IDirect3DDxgiInterfaceAccessVtbl
{
	STDMETHOD(QueryInterface)(IDirect3DDxgiInterfaceAccess* This, const GUID* Riid, void** Object);
	STDMETHOD_(ULONG, AddRef)(IDirect3DDxgiInterfaceAccess* This);
	STDMETHOD_(ULONG, Release)(IDirect3DDxgiInterfaceAccess* This);
	STDMETHOD(GetInterface)(IDirect3DDxgiInterfaceAccess* This, const GUID* Riid, void** Object);
};

struct IDirect3DDxgiInterfaceAccess
{
	struct IDirect3DDxgiInterfaceAccessVtbl* lpVtbl;
};

// from ntdll.dll

// from CoreMessaging.dll
extern   __declspec(dllimport) HRESULT WINAPI CreateDispatcherQueueController(DispatcherQueueOptions, IInspectable**);

// from d3d11.dll
extern __declspec(dllimport) HRESULT WINAPI CreateDirect3D11DeviceFromDXGIDevice(IDXGIDevice*, IInspectable**);


static HRESULT STDMETHODCALLTYPE IrWgc__FrameQueryInterface(__FITypedEventHandler_2_Windows__CGraphics__CCapture__CDirect3D11CaptureFramePool_IInspectable* This, REFIID Riid, void** Object)
{
	if (Object == NULL)
	{
		return E_POINTER;
	}
	if (IsEqualGUID(Riid, &IID_IDirect3D11CaptureFramePoolHandler) ||
		IsEqualGUID(Riid, &IID_IAgileObject) ||
		IsEqualGUID(Riid, &IID_IUnknown))
	{
		*Object = This;
		return S_OK;
	}
	return E_NOINTERFACE;
}

static ULONG STDMETHODCALLTYPE IrWgc__FrameAddRef(__FITypedEventHandler_2_Windows__CGraphics__CCapture__CDirect3D11CaptureFramePool_IInspectable* This)
{
	return 1;
}

static ULONG STDMETHODCALLTYPE IrWgc__FrameRelease(__FITypedEventHandler_2_Windows__CGraphics__CCapture__CDirect3D11CaptureFramePool_IInspectable* This)
{
	return 1;
}

static HRESULT STDMETHODCALLTYPE IrWgc__FrameInvoke(__FITypedEventHandler_2_Windows__CGraphics__CCapture__CDirect3D11CaptureFramePool_IInspectable* This, __x_ABI_CWindows_CGraphics_CCapture_CIDirect3D11CaptureFramePool* Sender, IInspectable* Args)
{
	IrWgc* Capture = CONTAINING_RECORD(This, IrWgc, OnFrameHandler);

	IrWgcFrame Frame;
	if (IrWgc_GetFrame(Capture, &Frame))
	{
		bool ContinueCapture = Capture->OnFrame(Capture, &Frame);
		IrWgc_ReleaseFrame(Capture, &Frame);

		if (!ContinueCapture)
		{
			IrWgc_Stop(Capture);
		}
	}

	return S_OK;
}

static HRESULT STDMETHODCALLTYPE IrWgc__CloseQueryInterface(__FITypedEventHandler_2_Windows__CGraphics__CCapture__CGraphicsCaptureItem_IInspectable* This, REFIID Riid, void** Object)
{
	if (Object == NULL)
	{
		return E_POINTER;
	}
	if (IsEqualGUID(Riid, &IID_IGraphicsCaptureItemHandler) ||
		IsEqualGUID(Riid, &IID_IAgileObject) ||
		IsEqualGUID(Riid, &IID_IUnknown))
	{
		*Object = This;
		return S_OK;
	}
	return E_NOINTERFACE;
}

static ULONG STDMETHODCALLTYPE IrWgc__CloseAddRef(__FITypedEventHandler_2_Windows__CGraphics__CCapture__CGraphicsCaptureItem_IInspectable* This)
{
	return 1;
}

static ULONG STDMETHODCALLTYPE IrWgc__CloseRelease(__FITypedEventHandler_2_Windows__CGraphics__CCapture__CGraphicsCaptureItem_IInspectable* This)
{
	return 1;
}

static HRESULT STDMETHODCALLTYPE IrWgc__CloseInvoke(__FITypedEventHandler_2_Windows__CGraphics__CCapture__CGraphicsCaptureItem_IInspectable* This, __x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureItem* Sender, IInspectable* Args)
{
	IrWgc* Capture = CONTAINING_RECORD(This, IrWgc, OnCloseHandler);
	Capture->OnFrame(Capture, NULL);
	return S_OK;
}

static __FITypedEventHandler_2_Windows__CGraphics__CCapture__CDirect3D11CaptureFramePool_IInspectableVtbl IrWgc__OnFrameHandlerVtbl =
{
	.QueryInterface = IrWgc__FrameQueryInterface,
	.AddRef         = IrWgc__FrameAddRef,
	.Release        = IrWgc__FrameRelease,
	.Invoke         = IrWgc__FrameInvoke,
};

static __FITypedEventHandler_2_Windows__CGraphics__CCapture__CGraphicsCaptureItem_IInspectableVtbl IrWgc__OnCloseHandlerVtbl =
{
	.QueryInterface = IrWgc__CloseQueryInterface,
	.AddRef         = IrWgc__CloseAddRef,
	.Release        = IrWgc__CloseRelease,
	.Invoke         = IrWgc__CloseInvoke,
};

static RECT IrWgc__GetRect(IrWgc* Capture, uint32_t Width, uint32_t Height)
{
	if (Capture->Window) // capturing window
	{
		RECT WindowRect;
		if (FAILED(DwmGetWindowAttribute(Capture->Window, DWMWA_EXTENDED_FRAME_BOUNDS, &WindowRect, sizeof(WindowRect))))
		{
			return (RECT) { 0 };
		}

		if (Capture->OnlyClientArea)
		{
			RECT ClientRect;
			GetClientRect(Capture->Window, &ClientRect);

			POINT TopLeft = { 0, 0 };
			ClientToScreen(Capture->Window, &TopLeft);

			RECT Rect;
			Rect.left = max(0, TopLeft.x - WindowRect.left);
			Rect.top = max(0, TopLeft.y - WindowRect.top);
			Rect.right = Rect.left + min((uint32_t)ClientRect.right, Width - Rect.left);
			Rect.bottom = Rect.top + min((uint32_t)ClientRect.bottom, Height - Rect.top);
			return Rect;
		}
		else // whole window size
		{
			return (RECT)
			{
				.left = 0,
				.top = 0,
				.right = WindowRect.right - WindowRect.left,
				.bottom = WindowRect.bottom - WindowRect.top,
			};
		}
	}
	else // capturing monitor, or region on it
	{
		return Capture->Rect;
	}
}

static bool IrWgc__CreateFramePool(IrWgc* Capture, __x_ABI_CWindows_CGraphics_CSizeInt32 Size, __x_ABI_CWindows_CGraphics_CCapture_CIDirect3D11CaptureFramePool** FramePool)
{
	if (Capture->FramePoolStatics2)
	{
		return SUCCEEDED(__x_ABI_CWindows_CGraphics_CCapture_CIDirect3D11CaptureFramePoolStatics2_CreateFreeThreaded(Capture->FramePoolStatics2, Capture->Device, IR_WGC_BUFFER_FORMAT, IR_WGC_BUFFER_COUNT, Size, FramePool));
	}
	else
	{
		return SUCCEEDED(__x_ABI_CWindows_CGraphics_CCapture_CIDirect3D11CaptureFramePoolStatics_Create(Capture->FramePoolStatics, Capture->Device, IR_WGC_BUFFER_FORMAT, IR_WGC_BUFFER_COUNT, Size, FramePool));
	}
}

bool IrWgc_IsSupported(void)
{
	RTL_OSVERSIONINFOW Version = { sizeof(Version) };
	RtlGetVersion(&Version);

	// available since Windows 10 version 1903, May 2019 Update (19H1), build 10.0.18362.0
	return Version.dwMajorVersion > 10 || (Version.dwMajorVersion == 10 && Version.dwBuildNumber >= 18362);
}

bool IrWgc_CanHideMouseCursor(void)
{
	RTL_OSVERSIONINFOW Version = { sizeof(Version) };
	RtlGetVersion(&Version);

	// available since Windows 10 version 2004, May 2020 Update (20H1), build 10.0.19041.0
	return Version.dwMajorVersion > 10 || (Version.dwMajorVersion == 10 && Version.dwBuildNumber >= 19041);
}

bool IrWgc_CanHideRecordingBorder(void)
{
	RTL_OSVERSIONINFOW Version = { sizeof(Version) };
	RtlGetVersion(&Version);

	// available since Windows 11 21H2, build 10.0.22000.0
	return Version.dwMajorVersion > 10 || (Version.dwMajorVersion == 10 && Version.dwBuildNumber >= 22000);
}

bool IrWgc_CanDisableRoundedCorners(void)
{
	RTL_OSVERSIONINFOW Version = { sizeof(Version) };
	RtlGetVersion(&Version);

	// available since Windows 11 21H2, build 10.0.22000.0
	return Version.dwMajorVersion > 10 || (Version.dwMajorVersion == 10 && Version.dwBuildNumber >= 22000);
}

bool IrWgc_CanIncludeSecondaryWindows(void)
{
	RTL_OSVERSIONINFOW Version = { sizeof(Version) };
	RtlGetVersion(&Version);

	// available since Windows 11 24H2, build 10.0.26100.0
	return Version.dwMajorVersion > 10 || (Version.dwMajorVersion == 10 && Version.dwBuildNumber >= 26100);
}

void IrWgc_Create(IrWgc* Capture, IrWgcOnFrameCallback* OnFrame, bool CallbackOnThread)
{
	Capture->RoInitHr = RoInitialize(RO_INIT_MULTITHREADED);

	typedef struct
	{
		DWORD Flags;
		DWORD Length;
		DWORD Padding1;
		DWORD Padding2;
		LPCWCHAR Ptr;
	}
	StaticHSTRING;

#define STATIC_HSTRING(Str) (HSTRING)&(StaticHSTRING){ 1, ARRAYSIZE(Str) - 1, 0, 0, Str }
	HSTRING GraphicsCaptureItem = STATIC_HSTRING(RuntimeClass_Windows_Graphics_Capture_GraphicsCaptureItem);
	HSTRING Direct3D11CaptureFramePool = STATIC_HSTRING(RuntimeClass_Windows_Graphics_Capture_Direct3D11CaptureFramePool);
#undef STATIC_HSTRING

	HR(RoGetActivationFactory(GraphicsCaptureItem, &IID_IGraphicsCaptureItemInterop, (void**)&Capture->ItemInterop));

	if (OnFrame && CallbackOnThread)
	{
		HR(RoGetActivationFactory(Direct3D11CaptureFramePool, &IID_IDirect3D11CaptureFramePoolStatics2, (void**)&Capture->FramePoolStatics2));
		Capture->FramePoolStatics = NULL;
	}
	else
	{
		HR(RoGetActivationFactory(Direct3D11CaptureFramePool, &IID_IDirect3D11CaptureFramePoolStatics, (void**)&Capture->FramePoolStatics));
		Capture->FramePoolStatics2 = NULL;
	}

	if (OnFrame && !CallbackOnThread)
	{
		// create dispatcher queue that will call callbacks on main thread as part of message loop
		DispatcherQueueOptions Options =
		{
			.dwSize = sizeof(Options),
			.threadType = DQTYPE_THREAD_CURRENT,
			.apartmentType = DQTAT_COM_NONE,
		};
		HR(CreateDispatcherQueueController(Options, &Capture->Controller));
	}

	Capture->OnFrameHandler.lpVtbl = &IrWgc__OnFrameHandlerVtbl;
	Capture->OnCloseHandler.lpVtbl = &IrWgc__OnCloseHandlerVtbl;
	Capture->OnFrame = OnFrame;
}

void IrWgc_Release(IrWgc* Capture)
{
	if (Capture->Controller)
	{
		IInspectable_Release(Capture->Controller);
		Capture->Controller = NULL;
	}
	if (Capture->FramePoolStatics)
	{
		__x_ABI_CWindows_CGraphics_CCapture_CIDirect3D11CaptureFramePoolStatics_Release(Capture->FramePoolStatics);
		Capture->FramePoolStatics = NULL;
	}
	if (Capture->FramePoolStatics2)
	{
		__x_ABI_CWindows_CGraphics_CCapture_CIDirect3D11CaptureFramePoolStatics2_Release(Capture->FramePoolStatics2);
		Capture->FramePoolStatics2 = NULL;
	}
	if (Capture->ItemInterop)
	{
		Capture->ItemInterop->lpVtbl->Release(Capture->ItemInterop);
		Capture->ItemInterop = NULL;
	}
	if (SUCCEEDED(Capture->RoInitHr)) RoUninitialize();
	Capture->RoInitHr = E_FAIL;
}

bool IrWgc_CreateForWindow(IrWgc* Capture, ID3D11Device* Device, HWND Window, bool OnlyClientArea, bool DisableRoundedCorners)
{
	IDXGIDevice* DxgiDevice;
	HR(ID3D11Device_QueryInterface(Device, &IID_IDXGIDevice, (LPVOID*)&DxgiDevice));
	HR(CreateDirect3D11DeviceFromDXGIDevice(DxgiDevice, (IInspectable**)&Capture->Device));
	IDXGIDevice_Release(DxgiDevice);

	__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureItem* Item;
	if (SUCCEEDED(Capture->ItemInterop->lpVtbl->CreateForWindow(Capture->ItemInterop, Window, &IID_IGraphicsCaptureItem, (void**)&Item)))
	{
		__x_ABI_CWindows_CGraphics_CSizeInt32 Size;
		HR(__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureItem_get_Size(Item, &Size));

		__x_ABI_CWindows_CGraphics_CCapture_CIDirect3D11CaptureFramePool* FramePool;
		if (IrWgc__CreateFramePool(Capture, Size, &FramePool))
		{
			Capture->Item = Item;
			Capture->FramePool = FramePool;
			Capture->OnlyClientArea = OnlyClientArea;
			Capture->Window = Window;

			RECT Rect = IrWgc__GetRect(Capture, Size.Width, Size.Height);
			Capture->CurrentSize.Width = Rect.right - Rect.left;
			Capture->CurrentSize.Height = Rect.bottom - Rect.top;
			Capture->Rect = Rect;

			Capture->RestoreWindowCornerPreference = false;
			if (DisableRoundedCorners)
			{
				if (SUCCEEDED(DwmGetWindowAttribute(Window, DWMWA_WINDOW_CORNER_PREFERENCE, &Capture->WindowCornerPreference, sizeof(Capture->WindowCornerPreference))))
				{
					DWM_WINDOW_CORNER_PREFERENCE Preference = DWMWCP_DONOTROUND;
					if (SUCCEEDED(DwmSetWindowAttribute(Window, DWMWA_WINDOW_CORNER_PREFERENCE, &Preference, sizeof(Preference))))
					{
						Capture->RestoreWindowCornerPreference = true;
					}
				}
			}

			return true;
		}
		__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureItem_Release(Item);
	}

	__x_ABI_CWindows_CGraphics_CDirectX_CDirect3D11_CIDirect3DDevice_Release(Capture->Device);
	return false;
}

bool IrWgc_CreateForMonitor(IrWgc* Capture, ID3D11Device* Device, HMONITOR Monitor, const RECT* Rect)
{
	IDXGIDevice* DxgiDevice;
	HR(ID3D11Device_QueryInterface(Device, &IID_IDXGIDevice, (LPVOID*)&DxgiDevice));
	HR(CreateDirect3D11DeviceFromDXGIDevice(DxgiDevice, (IInspectable**)&Capture->Device));
	IDXGIDevice_Release(DxgiDevice);

	__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureItem* Item;
	if (SUCCEEDED(Capture->ItemInterop->lpVtbl->CreateForMonitor(Capture->ItemInterop, Monitor, &IID_IGraphicsCaptureItem, (void**)&Item)))
	{
		__x_ABI_CWindows_CGraphics_CSizeInt32 Size;
		HR(__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureItem_get_Size(Item, &Size));

		__x_ABI_CWindows_CGraphics_CCapture_CIDirect3D11CaptureFramePool* FramePool;
		if (IrWgc__CreateFramePool(Capture, Size, &FramePool))
		{
			Capture->Item = Item;
			Capture->FramePool = FramePool;
			Capture->Window = NULL;
			Capture->CurrentSize = Size;
			Capture->Rect = Rect ? *Rect : (RECT) { 0, 0, Size.Width, Size.Height };

			Capture->RestoreWindowCornerPreference = false;
			return true;
		}
		__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureItem_Release(Item);
	}

	__x_ABI_CWindows_CGraphics_CDirectX_CDirect3D11_CIDirect3DDevice_Release(Capture->Device);
	return false;
}

void IrWgc_Start(IrWgc* Capture, bool WithMouseCursor, bool WithRecordingBorder, bool IncludeSecondaryWindows)
{
	__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureSession* Session;
	HR(__x_ABI_CWindows_CGraphics_CCapture_CIDirect3D11CaptureFramePool_CreateCaptureSession(Capture->FramePool, Capture->Item, &Session));

	__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureSession2* Session2;
	if (SUCCEEDED(__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureSession_QueryInterface(Session, &IID_IGraphicsCaptureSession2, (void**)&Session2)))
	{
		__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureSession2_put_IsCursorCaptureEnabled(Session2, (boolean)WithMouseCursor);
		__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureSession2_Release(Session2);
	}

	__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureSession3* Session3;
	if (SUCCEEDED(__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureSession_QueryInterface(Session, &IID_IGraphicsCaptureSession3, (void**)&Session3)))
	{
		__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureSession3_put_IsBorderRequired(Session3, (boolean)WithRecordingBorder);
		__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureSession3_Release(Session3);
	}

	__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureSession5* Session5;
	if (SUCCEEDED(__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureSession_QueryInterface(Session, &IID_IGraphicsCaptureSession5, (void**)&Session5)))
	{
		// TimeSpan value is in 100ns units
		// min duration supported by API is 1msec, this means max supported framerate possible is 1000 fps
		__x_ABI_CWindows_CFoundation_CTimeSpan Duration = { MF_UNITS_PER_SECOND / 1000 };
		__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureSession5_put_MinUpdateInterval(Session5, Duration);
		__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureSession5_Release(Session5);
	}

	__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureSession6* Session6;
	if (SUCCEEDED(__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureSession_QueryInterface(Session, &IID_IGraphicsCaptureSession6, (void**)&Session6)))
	{
		__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureSession6_put_IncludeSecondaryWindows(Session6, (boolean)IncludeSecondaryWindows);
		__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureSession6_Release(Session6);
	}

	if (Capture->OnFrame)
	{
		HR(__x_ABI_CWindows_CGraphics_CCapture_CIDirect3D11CaptureFramePool_add_FrameArrived(Capture->FramePool, &Capture->OnFrameHandler, &Capture->OnFrameToken));
		HR(__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureItem_add_Closed(Capture->Item, &Capture->OnCloseHandler, &Capture->OnCloseToken));
	}

	HR(__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureSession_StartCapture(Session));
	Capture->Session = Session;
}

void IrWgc_Stop(IrWgc* Capture)
{
	__x_ABI_CWindows_CFoundation_CIClosable* Closable;

	if (Capture->OnFrameToken.value)
	{
		HR(__x_ABI_CWindows_CGraphics_CCapture_CIDirect3D11CaptureFramePool_remove_FrameArrived(Capture->FramePool, Capture->OnFrameToken));
		Capture->OnFrameToken.value = 0;
	}

	if (Capture->OnCloseToken.value)
	{
		HR(__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureItem_remove_Closed(Capture->Item, Capture->OnCloseToken));
		Capture->OnCloseToken.value = 0;
	}

	if (Capture->Session)
	{
		HR(__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureSession_QueryInterface(Capture->Session, &IID_IClosable, (void**)&Closable));
		HR(__x_ABI_CWindows_CFoundation_CIClosable_Close(Closable));
		__x_ABI_CWindows_CFoundation_CIClosable_Release(Closable);
		__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureSession_Release(Capture->Session);

		Capture->Session = NULL;
	}

	if (Capture->Item)
	{
		HR(__x_ABI_CWindows_CGraphics_CCapture_CIDirect3D11CaptureFramePool_QueryInterface(Capture->FramePool, &IID_IClosable, (void**)&Closable));
		HR(__x_ABI_CWindows_CFoundation_CIClosable_Close(Closable));
		__x_ABI_CWindows_CFoundation_CIClosable_Release(Closable);
		__x_ABI_CWindows_CGraphics_CCapture_CIDirect3D11CaptureFramePool_Release(Capture->FramePool);
		__x_ABI_CWindows_CGraphics_CCapture_CIGraphicsCaptureItem_Release(Capture->Item);
		Capture->Item = NULL;
	}

	if (Capture->Device)
	{
		__x_ABI_CWindows_CGraphics_CDirectX_CDirect3D11_CIDirect3DDevice_Release(Capture->Device);
		Capture->Device = NULL;
	}

	if (Capture->RestoreWindowCornerPreference)
	{
		DwmSetWindowAttribute(Capture->Window, DWMWA_WINDOW_CORNER_PREFERENCE, &Capture->WindowCornerPreference, sizeof(Capture->WindowCornerPreference));
	}
}

bool IrWgc_GetFrame(IrWgc* Capture, IrWgcFrame* Frame)
{
	__x_ABI_CWindows_CGraphics_CCapture_CIDirect3D11CaptureFrame* NextFrame;
	if (SUCCEEDED(__x_ABI_CWindows_CGraphics_CCapture_CIDirect3D11CaptureFramePool_TryGetNextFrame(Capture->FramePool, &NextFrame)) && NextFrame != NULL)
	{
		__x_ABI_CWindows_CFoundation_CTimeSpan Time;
		HR(__x_ABI_CWindows_CGraphics_CCapture_CIDirect3D11CaptureFrame_get_SystemRelativeTime(NextFrame, &Time));

		__x_ABI_CWindows_CGraphics_CSizeInt32 Size;
		HR(__x_ABI_CWindows_CGraphics_CCapture_CIDirect3D11CaptureFrame_get_ContentSize(NextFrame, &Size));

		__x_ABI_CWindows_CGraphics_CDirectX_CDirect3D11_CIDirect3DSurface* Surface;
		HR(__x_ABI_CWindows_CGraphics_CCapture_CIDirect3D11CaptureFrame_get_Surface(NextFrame, &Surface));

		IDirect3DDxgiInterfaceAccess* Access;
		HR(__x_ABI_CWindows_CGraphics_CDirectX_CDirect3D11_CIDirect3DSurface_QueryInterface(Surface, &IID_IDirect3DDxgiInterfaceAccess, (void**)&Access));
		__x_ABI_CWindows_CGraphics_CDirectX_CDirect3D11_CIDirect3DSurface_Release(Surface);

		ID3D11Texture2D* Texture;
		HR(Access->lpVtbl->GetInterface(Access, &IID_ID3D11Texture2D, (void**)&Texture));
		Access->lpVtbl->Release(Access);

		D3D11_TEXTURE2D_DESC TextureDesc;
		ID3D11Texture2D_GetDesc(Texture, &TextureDesc);

		RECT Rect = IrWgc__GetRect(Capture, min((uint32_t)Size.Width, TextureDesc.Width), min((uint32_t)Size.Height, TextureDesc.Height));
		if (Rect.right > Rect.left && Rect.bottom > Rect.top)
		{
			*Frame = (IrWgcFrame)
			{
				.Texture = Texture,
				.Time = Time.Duration,
				.Rect = Rect,
				.Width = Size.Width,
				.Height = Size.Height,
				.NextFrame = (IUnknown*)NextFrame,
			};
			return true;
		}
		__x_ABI_CWindows_CGraphics_CCapture_CIDirect3D11CaptureFrame_Release(NextFrame);
	}

	return false;
}

void IrWgc_ReleaseFrame(IrWgc* Capture, IrWgcFrame* Frame)
{
	ID3D11Texture2D_Release(Frame->Texture);
	IUnknown_Release(Frame->NextFrame);

	if (Capture->CurrentSize.Width != Frame->Width || Capture->CurrentSize.Height != Frame->Height)
	{
		Capture->CurrentSize = (__x_ABI_CWindows_CGraphics_CSizeInt32){ Frame->Width, Frame->Height };
		HR(__x_ABI_CWindows_CGraphics_CCapture_CIDirect3D11CaptureFramePool_Recreate(Capture->FramePool, Capture->Device, IR_WGC_BUFFER_FORMAT, IR_WGC_BUFFER_COUNT, Capture->CurrentSize));
	}
}

//




//
// interface
//

typedef struct
{
	IAudioClient* PlayClient;
	IAudioClient* RecordClient;
	IAudioCaptureClient* CaptureClient;
	WAVEFORMATEX* Format;
	uint64_t StartQpc;
	uint64_t StartPos;
	uint64_t Freq;
	bool UseDeviceTimestamp;
	bool CheckDeviceTimestamp;

	bool Stop;
	HANDLE Event;
	HANDLE Thread;

	uint8_t* Buffer;
	uint32_t BufferSize;
	_Atomic(uint32_t) BufferRead;
	_Atomic(uint32_t) BufferWrite;
}
IrAudio;

typedef struct
{
	void* Samples;
	size_t Count;
	uint64_t Time; // compatible with QPC
}
IrAudioData;

static bool IrAudio_CanCaptureApplicationLocal(void);

// make sure CoInitializeEx has been called before calling Start()
static bool IrAudio_Start(IrAudio* Capture, HWND ApplicationWindow);
static void IrAudio_Stop(IrAudio* Capture);
static void IrAudio_Flush(IrAudio* Capture);

// expectedTimestamp is used only first time GetData() is called to detect abnormal device timestamps
static bool IrAudio_GetData(IrAudio* Capture, IrAudioData* Data, uint64_t ExpectedTimestamp);
static void IrAudio_ReleaseData(IrAudio* Capture, IrAudioData* Data);

//
// from ntdll.dll

bool IrAudio_CanCaptureApplicationLocal(void)
{
	RTL_OSVERSIONINFOW Version = { sizeof(Version) };
	RtlGetVersion(&Version);

	// not exactly sure which version
	// available since Windows 10 version 2004, May 2020 Update (20H1), build 10.0.19041.0
	return Version.dwMajorVersion > 10 || (Version.dwMajorVersion == 10 && Version.dwBuildNumber >= 19041);
}


typedef struct
{
	IActivateAudioInterfaceCompletionHandler Handler;
	_Atomic(uint32_t) ReadyFlag;
}
IrAudioActivate;

static HRESULT STDMETHODCALLTYPE IrAudioActivate__QueryInterface(IActivateAudioInterfaceCompletionHandler* This, REFIID Riid, void** Object)
{
	if (Object == NULL)
	{
		return E_POINTER;
	}
	if (IsEqualGUID(Riid, &IID_IActivateAudioInterfaceCompletionHandler) ||
		IsEqualGUID(Riid, &IID_IAgileObject) ||
		IsEqualGUID(Riid, &IID_IUnknown))
	{
		*Object = This;
		return S_OK;
	}
	return E_NOINTERFACE;
}

static ULONG STDMETHODCALLTYPE IrAudioActivate__AddRef(IActivateAudioInterfaceCompletionHandler* This)
{
	return 1;
}

static ULONG STDMETHODCALLTYPE IrAudioActivate__Release(IActivateAudioInterfaceCompletionHandler* This)
{
	return 1;
}

static HRESULT STDMETHODCALLTYPE IrAudioActivate__ActivateCompleted(IActivateAudioInterfaceCompletionHandler* This, IActivateAudioInterfaceAsyncOperation* ActivateOperation)
{
	IrAudioActivate* Activate = CONTAINING_RECORD(This, IrAudioActivate, Handler);

	atomic_store_explicit(&Activate->ReadyFlag, 1, memory_order_release);
	WakeByAddressSingle((PVOID)&Activate->ReadyFlag);

	return S_OK;
}

static IActivateAudioInterfaceCompletionHandlerVtbl IrAudioActivateVtbl =
{
	.QueryInterface    = &IrAudioActivate__QueryInterface,
	.AddRef            = &IrAudioActivate__AddRef,
	.Release           = &IrAudioActivate__Release,
	.ActivateCompleted = &IrAudioActivate__ActivateCompleted,
};

static DWORD CALLBACK IrAudio__Thread(LPVOID Arg)
{
	IrAudio* Capture = Arg;
	HRESULT com_hr = CoInitializeEx(NULL, COINIT_MULTITHREADED);

	DWORD Task = 0;
	HANDLE Handle = AvSetMmThreadCharacteristicsW(L"Pro Audio", &Task);
	Assert(Handle);

	IAudioCaptureClient* CaptureClient = Capture->CaptureClient;
	uint32_t BytesPerFrame = Capture->Format->nBlockAlign;
	uint32_t BufferSize = Capture->BufferSize;
	uint32_t BufferWrite = 0;
	HANDLE Event = Capture->Event;

	while (WaitForSingleObject(Event, INFINITE) == WAIT_OBJECT_0)
	{
		if (Capture->Stop)
		{
			break;
		}

		BYTE* Buffer = NULL;
		DWORD Flags = 0;
		UINT32 Frames = 0;
		UINT64 Position = 0; // in sample count from beginning of stream
		UINT64 Timestamp = 0; // in QPC unuts
		while (SUCCEEDED(IAudioCaptureClient_GetBuffer(CaptureClient, &Buffer, &Frames, &Flags, &Position, &Timestamp)) && Frames != 0)
		{
			uint32_t BufferAvailable = BufferSize - (BufferWrite - atomic_load_explicit(&Capture->BufferRead, memory_order_relaxed));

			uint32_t WriteSize = sizeof(Frames) + sizeof(Position) + sizeof(Timestamp) + Frames * BytesPerFrame;
			if (WriteSize <= BufferAvailable)
			{
				uint8_t* BufferPtr = Capture->Buffer + (BufferWrite & (BufferSize - 1));
				CopyMemory(BufferPtr, &Frames, sizeof(Frames)); BufferPtr += sizeof(Frames);
				CopyMemory(BufferPtr, &Position, sizeof(Position)); BufferPtr += sizeof(Position);
				CopyMemory(BufferPtr, &Timestamp, sizeof(Timestamp)); BufferPtr += sizeof(Timestamp);
				if (Flags & AUDCLNT_BUFFERFLAGS_SILENT)
				{
					ZeroMemory(BufferPtr, Frames * BytesPerFrame);
				}
				else
				{
					CopyMemory(BufferPtr, Buffer, Frames * BytesPerFrame);
				}

				BufferWrite += WriteSize;
				atomic_store_explicit(&Capture->BufferWrite, BufferWrite, memory_order_release);
			}
			else
			{
				// TODO: logging/stats when audio ringbuffer is overflowing
			}

			HR(IAudioCaptureClient_ReleaseBuffer(CaptureClient, Frames));
			Buffer = NULL;
			Frames = 0;
			Position = 0;
			Timestamp = 0;
		}
	}
	
	AvRevertMmThreadCharacteristics(Handle);
	if (SUCCEEDED(com_hr)) CoUninitialize();

	return 0;
}

bool IrAudio_Start(IrAudio* Capture, HWND ApplicationWindow)
{
	bool Result = false;

	if (ApplicationWindow)
	{
		DWORD ProcessId;
		DWORD ThreadId = GetWindowThreadProcessId(ApplicationWindow, &ProcessId);
		if (!ThreadId)
		{
			return false;
		}

		AUDIOCLIENT_ACTIVATION_PARAMS Activation =
		{
			.ActivationType = AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
			.ProcessLoopbackParams.ProcessLoopbackMode = PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
			.ProcessLoopbackParams.TargetProcessId = ProcessId,
		};

		PROPVARIANT Params =
		{
			.vt = VT_BLOB,
			.blob.cbSize = sizeof(Activation),
			.blob.pBlobData = (BYTE*)&Activation,
		};

		IrAudioActivate ActivateCompletion =
		{
			.Handler.lpVtbl = &IrAudioActivateVtbl,
		};
		atomic_init(&ActivateCompletion.ReadyFlag, 0);

		IActivateAudioInterfaceAsyncOperation* AsyncOperation;
		if (FAILED(ActivateAudioInterfaceAsync(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, &IID_IAudioClient, &Params, &ActivateCompletion.Handler, &AsyncOperation)))
		{
			return false;
		}

		while (!atomic_load_explicit(&ActivateCompletion.ReadyFlag, memory_order_acquire))
		{
			uint32_t ReadyFlag = 0;
			WaitOnAddress((PVOID)&ActivateCompletion.ReadyFlag, &ReadyFlag, sizeof(ReadyFlag), INFINITE);
		}

		HRESULT ActivateResult;
		IUnknown* ActivateUnknown;
		if (SUCCEEDED(IActivateAudioInterfaceAsyncOperation_GetActivateResult(AsyncOperation, &ActivateResult, &ActivateUnknown)) && SUCCEEDED(ActivateResult))
		{
			IAudioClient* Client;
			HR(IUnknown_QueryInterface(ActivateUnknown, &IID_IAudioClient, &Client));
			IUnknown_Release(ActivateUnknown);

			WAVEFORMATEXTENSIBLE* FormatEx = CoTaskMemAlloc(sizeof(*FormatEx));
			Assert(FormatEx);

			// IAudioClient you get from ActivateAudioInterfaceAsync does not support GetMixFormat() and GetDevicePeriod() methods
			FormatEx->Format.wFormatTag = WAVE_FORMAT_EXTENSIBLE;
			FormatEx->Format.nChannels = 2;
			FormatEx->Format.wBitsPerSample = 32;
			FormatEx->Format.nSamplesPerSec = 48000;
			FormatEx->Format.nBlockAlign = (FormatEx->Format.nChannels * FormatEx->Format.wBitsPerSample) / 8;
			FormatEx->Format.nAvgBytesPerSec = FormatEx->Format.nSamplesPerSec * FormatEx->Format.nBlockAlign;
			FormatEx->Format.cbSize = sizeof(*FormatEx) - sizeof(FormatEx->Format);
			FormatEx->Samples.wValidBitsPerSample = FormatEx->Format.wBitsPerSample;
			FormatEx->dwChannelMask = SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT;
			FormatEx->SubFormat = KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;

			DWORD Flags = AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY | AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
			HR(IAudioClient_Initialize(Client, AUDCLNT_SHAREMODE_SHARED, Flags, MF_UNITS_PER_SECOND, 0, &FormatEx->Format, NULL));
			HR(IAudioClient_GetService(Client, &IID_IAudioCaptureClient, (void**)&Capture->CaptureClient));

			Capture->PlayClient = NULL;
			Capture->RecordClient = Client;
			Capture->Format = &FormatEx->Format;

			Capture->StartPos = 0;
			Capture->UseDeviceTimestamp = true;
			Capture->CheckDeviceTimestamp = false;

			Result = true;

			IActivateAudioInterfaceAsyncOperation_Release(AsyncOperation);
		}
	}
	else
	{
		IMMDeviceEnumerator* Enumerator;
		HR(CoCreateInstance(&CLSID_MMDeviceEnumerator, NULL, CLSCTX_ALL, &IID_IMMDeviceEnumerator, (void**)&Enumerator));

		IMMDevice* Device;
		if (FAILED(IMMDeviceEnumerator_GetDefaultAudioEndpoint(Enumerator, eRender, eConsole, &Device)))
		{
			// no playback device found
			IMMDeviceEnumerator_Release(Enumerator);
			return false;
		}

		// setup playback for slience, otherwise loopback recording does not provide any data if nothing is playing
		{
			IAudioClient* Client;
			HR(IMMDevice_Activate(Device, &IID_IAudioClient, CLSCTX_ALL, NULL, (void**)&Client));

			WAVEFORMATEX* Format;
			HR(IAudioClient_GetMixFormat(Client, &Format));
			HR(IAudioClient_Initialize(Client, AUDCLNT_SHAREMODE_SHARED, 0, MF_UNITS_PER_SECOND, 0, Format, NULL));

			IAudioRenderClient* Render;
			HR(IAudioClient_GetService(Client, &IID_IAudioRenderClient, &Render));

			BYTE* Buffer;
			HR(IAudioRenderClient_GetBuffer(Render, Format->nSamplesPerSec, &Buffer));
			HR(IAudioRenderClient_ReleaseBuffer(Render, Format->nSamplesPerSec, AUDCLNT_BUFFERFLAGS_SILENT));
			IAudioRenderClient_Release(Render);
			CoTaskMemFree(Format);

			HR(IAudioClient_Start(Client));
			Capture->PlayClient = Client;
		}

		// loopback recording
		{
			IAudioClient* Client;
			HR(IMMDevice_Activate(Device, &IID_IAudioClient, CLSCTX_ALL, NULL, (void**)&Client));

			WAVEFORMATEX* Format;
			HR(IAudioClient_GetMixFormat(Client, &Format));

			REFERENCE_TIME DefaultPeriod, MinimumPeriod;
			HR(IAudioClient_GetDevicePeriod(Client, &DefaultPeriod, &MinimumPeriod));

			DWORD Flags = AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
			HR(IAudioClient_Initialize(Client, AUDCLNT_SHAREMODE_SHARED, Flags, DefaultPeriod, 0, Format, NULL));
			HR(IAudioClient_GetService(Client, &IID_IAudioCaptureClient, (void**)&Capture->CaptureClient));

			Capture->RecordClient = Client;
			Capture->Format = Format;

			Capture->StartPos = 0;
			Capture->UseDeviceTimestamp = true;
			Capture->CheckDeviceTimestamp = true;
		}

		Result = true;

		IMMDevice_Release(Device);
		IMMDeviceEnumerator_Release(Enumerator);
	}

	if (Result)
	{
		// it seems process local loopback device does not use any buffering, even when we asked for 1 second of buffer
		// so we must implement our own ringbuffer to be able to dequeue incoming data as fast as possible
		
		DWORD BufferSizeIndex;
		_BitScanReverse(&BufferSizeIndex, max(65535, Capture->Format->nAvgBytesPerSec - 1));
		uint32_t BufferSize = 1 << (BufferSizeIndex + 1);
		Assert(BufferSize % 65536 == 0);

		// allocate ringbuffer for 1 second of data, rounded up to next pow2, at least 64KB
		uint8_t* Placeholder1 = (uint8_t*)VirtualAlloc2(NULL, NULL, 2 * BufferSize, MEM_RESERVE | MEM_RESERVE_PLACEHOLDER, PAGE_NOACCESS, NULL, 0);
		uint8_t* Placeholder2 = (uint8_t*)Placeholder1 + BufferSize;
		Assert(Placeholder1);

		BOOL Ok = VirtualFree(Placeholder1, BufferSize, MEM_RELEASE | MEM_PRESERVE_PLACEHOLDER);
		Assert(Ok);

		HANDLE Section = CreateFileMappingW(INVALID_HANDLE_VALUE, NULL, PAGE_READWRITE, 0, BufferSize, NULL);
		Assert(Section);

		void* View1 = MapViewOfFile3(Section, NULL, Placeholder1, 0, BufferSize, MEM_REPLACE_PLACEHOLDER, PAGE_READWRITE, NULL, 0);
		Assert(View1);

		void* View2 = MapViewOfFile3(Section, NULL, Placeholder2, 0, BufferSize, MEM_REPLACE_PLACEHOLDER, PAGE_READWRITE, NULL, 0);
		Assert(View2);

		CloseHandle(Section);
		VirtualFree(Placeholder1, 0, MEM_RELEASE);
		VirtualFree(Placeholder2, 0, MEM_RELEASE);

		Capture->Buffer = View1;
		Capture->BufferSize = BufferSize;
		atomic_init(&Capture->BufferRead, 0);
		atomic_init(&Capture->BufferWrite, 0);

		Capture->Stop = false;

		Capture->Event = CreateEventW(NULL, FALSE, FALSE, NULL);
		Assert(Capture->Event);

		Capture->Thread = CreateThread(NULL, 0, &IrAudio__Thread, Capture, 0, NULL);
		Assert(Capture->Thread);

		HR(IAudioClient_SetEventHandle(Capture->RecordClient, Capture->Event));
		HR(IAudioClient_Start(Capture->RecordClient));

		LARGE_INTEGER Start;
		QueryPerformanceCounter(&Start);
		Capture->StartQpc = Start.QuadPart;

		LARGE_INTEGER Freq;
		QueryPerformanceFrequency(&Freq);
		Capture->Freq = Freq.QuadPart;
	}

	return Result;
}

void IrAudio_Stop(IrAudio* Capture)
{
	Capture->Stop = true;
	SetEvent(Capture->Event);
	WaitForSingleObject(Capture->Thread, INFINITE);

	CloseHandle(Capture->Thread);
	CloseHandle(Capture->Event);

	UnmapViewOfFileEx(Capture->Buffer, 0);
	UnmapViewOfFileEx(Capture->Buffer + Capture->BufferSize, 0);

	CoTaskMemFree(Capture->Format);
	if (Capture->PlayClient)
	{
		IAudioClient_Release(Capture->PlayClient);
		Capture->PlayClient = NULL;
	}
	IAudioCaptureClient_Release(Capture->CaptureClient);
	IAudioClient_Release(Capture->RecordClient);
}

void IrAudio_Flush(IrAudio* Capture)
{
	if (Capture->PlayClient)
	{
		HR(IAudioClient_Stop(Capture->PlayClient));
	}
	HR(IAudioClient_Stop(Capture->RecordClient));
}

bool IrAudio_GetData(IrAudio* Capture, IrAudioData* Data, uint64_t ExpectedTimestamp)
{
	uint32_t Frames;
	uint64_t Position;
	uint64_t Timestamp;

	uint32_t BufferRead = atomic_load_explicit(&Capture->BufferRead, memory_order_relaxed);
	uint32_t AvailableSize = atomic_load_explicit(&Capture->BufferWrite, memory_order_acquire) - BufferRead;
	if (AvailableSize < sizeof(Frames) + sizeof(Position) + sizeof(Timestamp))
	{
		return false;
	}

	uint8_t* BufferPtr = Capture->Buffer + (BufferRead & (Capture->BufferSize - 1));
	CopyMemory(&Frames, BufferPtr, sizeof(Frames)); BufferPtr += sizeof(Frames);
	CopyMemory(&Position, BufferPtr, sizeof(Position)); BufferPtr += sizeof(Position);
	CopyMemory(&Timestamp, BufferPtr, sizeof(Timestamp)); BufferPtr += sizeof(Timestamp);

	uint32_t ReadSize = sizeof(Frames) + sizeof(Position) + sizeof(Timestamp) + Frames * Capture->Format->nBlockAlign;
	if (AvailableSize < ReadSize)
	{
		return false;
	}

	if (Capture->CheckDeviceTimestamp)
	{
		// first time we check if device timestamp is resonable - not more than 500 msec away from expected
		if (ExpectedTimestamp)
		{
			const int64_t MaxDelta = 500 * Capture->Freq;
			int64_t Delta = 1000 * (ExpectedTimestamp - Timestamp);

			if (Delta < -MaxDelta || Delta > +MaxDelta)
			{
				Capture->UseDeviceTimestamp = false;
			}
			Capture->StartPos = Position;
		}
		Capture->CheckDeviceTimestamp = false;
	}

	if (Capture->UseDeviceTimestamp)
	{
		Data->Time = MFllMulDiv(Timestamp, Capture->Freq, MF_UNITS_PER_SECOND, 0);
	}
	else
	{
		Data->Time = Capture->StartQpc + MFllMulDiv(Position - Capture->StartPos, Capture->Freq, Capture->Format->nSamplesPerSec, 0);
	}

	Data->Samples = BufferPtr;
	Data->Count = Frames;

	return true;
}

void IrAudio_ReleaseData(IrAudio* Capture, IrAudioData* Data)
{
	uint32_t ReadSize = (uint32_t)(sizeof(uint32_t) + sizeof(uint64_t) + sizeof(uint64_t) + Data->Count * Capture->Format->nBlockAlign);
	Assert(ReadSize <= atomic_load_explicit(&Capture->BufferWrite, memory_order_relaxed) - atomic_load_explicit(&Capture->BufferRead, memory_order_relaxed));
	atomic_fetch_add_explicit(&Capture->BufferRead, ReadSize, memory_order_release);
}

//






//
// interface
//

typedef struct
{
	ID3D11Texture2D* InputTexture;
	ID3D11Texture2D* OutputTexture;

	ID3D11ShaderResourceView* InputViewIn;
	ID3D11UnorderedAccessView* OutputViewOut;
	ID3D11ShaderResourceView* MiddleViewIn;
	ID3D11UnorderedAccessView* MiddleViewOut;
	ID3D11ComputeShader* PassH;
	ID3D11ComputeShader* PassV;
	uint32_t InputWidth;
	uint32_t InputHeight;
	uint32_t OutputWidth;
	uint32_t OutputHeight;
}
IrResize;

static void IrResize_Create(IrResize* Resize, ID3D11Device* Device, uint32_t InputWidth, uint32_t InputHeight, uint32_t OutputWidth, uint32_t OutputHeight, bool LinearSpace, D3D11_BIND_FLAG InputUsage);
static void IrResize_Release(IrResize* Resize);

static void IrResize_Dispatch(IrResize* Resize, ID3D11DeviceContext* Context);

//
void IrResize_Create(IrResize* Resize, ID3D11Device* Device, uint32_t InputWidth, uint32_t InputHeight, uint32_t OutputWidth, uint32_t OutputHeight, bool LinearSpace, D3D11_BIND_FLAG InputUsage)
{
	D3D11_TEXTURE2D_DESC InputTextureDesc =
	{
		.Width = InputWidth,
		.Height = InputHeight,
		.MipLevels = 1,
		.ArraySize = 1,
		.Format = DXGI_FORMAT_B8G8R8A8_TYPELESS,
		.SampleDesc = { 1, 0 },
		.Usage = D3D11_USAGE_DEFAULT,
		.BindFlags = D3D11_BIND_SHADER_RESOURCE | InputUsage,
	};
	ID3D11Device_CreateTexture2D(Device, &InputTextureDesc, NULL, &Resize->InputTexture);

	if (InputWidth == OutputWidth && InputHeight == OutputHeight)
	{
		Resize->InputViewIn = NULL;
		Resize->OutputTexture = Resize->InputTexture;
	}
	else
	{
		ID3DBlob* Shader;

		HR(D3DDecompressShaders(
			LinearSpace ? IrResizeLinearPassHShaderBytes : IrResizePassHShaderBytes,
			LinearSpace ? sizeof(IrResizeLinearPassHShaderBytes) : sizeof(IrResizePassHShaderBytes),
			1, 0, NULL, 0, &Shader, NULL));
		ID3D11Device_CreateComputeShader(Device, ID3D10Blob_GetBufferPointer(Shader), ID3D10Blob_GetBufferSize(Shader), NULL, &Resize->PassH);
		ID3D10Blob_Release(Shader);

		HR(D3DDecompressShaders(
			LinearSpace ? IrResizeLinearPassVShaderBytes : IrResizePassVShaderBytes,
			LinearSpace ? sizeof(IrResizeLinearPassVShaderBytes) : sizeof(IrResizePassVShaderBytes),
			1, 0, NULL, 0, &Shader, NULL));
		ID3D11Device_CreateComputeShader(Device, ID3D10Blob_GetBufferPointer(Shader), ID3D10Blob_GetBufferSize(Shader), NULL, &Resize->PassV);
		ID3D10Blob_Release(Shader);

		D3D11_SHADER_RESOURCE_VIEW_DESC ViewInDesc =
		{
			.Format = LinearSpace ? DXGI_FORMAT_B8G8R8A8_UNORM_SRGB : DXGI_FORMAT_B8G8R8A8_UNORM,
			.ViewDimension = D3D11_SRV_DIMENSION_TEXTURE2D,
			.Texture2D.MipLevels = -1,
		};
		ID3D11Device_CreateShaderResourceView(Device, (ID3D11Resource*)Resize->InputTexture, &ViewInDesc, &Resize->InputViewIn);

		D3D11_TEXTURE2D_DESC OutputTextureDesc =
		{
			.Width = OutputWidth,
			.Height = OutputHeight,
			.MipLevels = 1,
			.ArraySize = 1,
			.Format = DXGI_FORMAT_B8G8R8A8_TYPELESS,
			.SampleDesc = { 1, 0 },
			.Usage = D3D11_USAGE_DEFAULT,
			.BindFlags = D3D11_BIND_UNORDERED_ACCESS | D3D11_BIND_SHADER_RESOURCE,
		};
		ID3D11Device_CreateTexture2D(Device, &OutputTextureDesc, NULL, &Resize->OutputTexture);

		// because D3D 11.0 does not support B8G8R8A8 for UAV stores, create R32_UINT for packing BGRA bytes manually in shader
		D3D11_UNORDERED_ACCESS_VIEW_DESC ViewOutDesc =
		{
			.Format = DXGI_FORMAT_R32_UINT,
			.ViewDimension = D3D11_UAV_DIMENSION_TEXTURE2D,
			.Texture2D.MipSlice = 0,
		};
		ID3D11Device_CreateUnorderedAccessView(Device, (ID3D11Resource*)Resize->OutputTexture, &ViewOutDesc, &Resize->OutputViewOut);

		D3D11_TEXTURE2D_DESC MiddleTextureDesc =
		{
			.Width = OutputWidth,
			.Height = InputHeight,
			.MipLevels = 1,
			.ArraySize = 1,
			.Format = DXGI_FORMAT_B8G8R8A8_TYPELESS,
			.SampleDesc = { 1, 0 },
			.Usage = D3D11_USAGE_DEFAULT,
			.BindFlags = D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_UNORDERED_ACCESS,
		};

		ID3D11Texture2D* MiddleTexture;
		ID3D11Device_CreateTexture2D(Device, &MiddleTextureDesc, NULL, &MiddleTexture);
		ID3D11Device_CreateShaderResourceView(Device, (ID3D11Resource*)MiddleTexture, &ViewInDesc, &Resize->MiddleViewIn);
		ID3D11Device_CreateUnorderedAccessView(Device, (ID3D11Resource*)MiddleTexture, &ViewOutDesc, &Resize->MiddleViewOut);
		ID3D11Texture2D_Release(MiddleTexture);
	}

	Resize->InputWidth = InputWidth;
	Resize->InputHeight = InputHeight;
	Resize->OutputWidth = OutputWidth;
	Resize->OutputHeight = OutputHeight;
}

void IrResize_Release(IrResize* Resize)
{
	ID3D11Texture2D_Release(Resize->InputTexture);

	if (Resize->InputViewIn)
	{
		ID3D11ShaderResourceView_Release(Resize->InputViewIn);
		ID3D11UnorderedAccessView_Release(Resize->OutputViewOut);

		ID3D11ShaderResourceView_Release(Resize->MiddleViewIn);
		ID3D11UnorderedAccessView_Release(Resize->MiddleViewOut);

		ID3D11Texture2D_Release(Resize->OutputTexture);

		ID3D11ComputeShader_Release(Resize->PassH);
		ID3D11ComputeShader_Release(Resize->PassV);
	}
}

void IrResize_Dispatch(IrResize* Resize, ID3D11DeviceContext* Context)
{
	if (Resize->InputViewIn == NULL)
	{
		return;
	}

	ID3D11DeviceContext_ClearState(Context);
	ID3D11DeviceContext_CSSetShader(Context, Resize->PassH, NULL, 0);
	ID3D11DeviceContext_CSSetShaderResources(Context, 0, 1, &Resize->InputViewIn);
	ID3D11DeviceContext_CSSetUnorderedAccessViews(Context, 0, 1, &Resize->MiddleViewOut, NULL);
	ID3D11DeviceContext_Dispatch(Context, DIV_ROUND_UP(Resize->OutputWidth, 16), DIV_ROUND_UP(Resize->InputHeight, 16), 1);

	ID3D11DeviceContext_ClearState(Context);
	ID3D11DeviceContext_CSSetShader(Context, Resize->PassV, NULL, 0);
	ID3D11DeviceContext_CSSetShaderResources(Context, 0, 1, &Resize->MiddleViewIn);
	ID3D11DeviceContext_CSSetUnorderedAccessViews(Context, 0, 1, &Resize->OutputViewOut, NULL);
	ID3D11DeviceContext_Dispatch(Context, DIV_ROUND_UP(Resize->OutputWidth, 16), DIV_ROUND_UP(Resize->OutputHeight, 16), 1);
}

//





//
// interface
//

typedef struct
{
	ID3D11Texture2D* Texture;
	ID3D11ShaderResourceView* ViewInUV;
	ID3D11UnorderedAccessView* ViewOutUV;
	ID3D11UnorderedAccessView* ViewOutY;
}
IrYuvOutput;

typedef struct
{
	ID3D11ShaderResourceView* InputView;
	ID3D11ComputeShader* SinglePass;
	ID3D11ComputeShader* Pass1;
	ID3D11ComputeShader* Pass2;
	ID3D11Buffer* ConstantBuffer;
	uint32_t Width;
	uint32_t Height;
}
IrYuv;

typedef enum
{
	IrYuvColorSpace_BT601,
	IrYuvColorSpace_BT709,
	IrYuvColorSpace_BT2020,
}
IrYuvColorSpace;

static void IrYuvOutput_Create(IrYuvOutput* Output, ID3D11Device* Device, uint32_t Width, uint32_t Height, DXGI_FORMAT Format);
static void IrYuvOutput_Release(IrYuvOutput* Output);

static void IrYuv_Create(IrYuv* Convert, ID3D11Device* Device, ID3D11Texture2D* InputTexture, uint32_t Width, uint32_t Height, IrYuvColorSpace ColorSpace, bool ImprovedConversion);
static void IrYuv_Release(IrYuv* Convert);

static void IrYuv_Dispatch(IrYuv* Convert, ID3D11DeviceContext* Context, IrYuvOutput* Output);

//
void IrYuvOutput_Create(IrYuvOutput* Output, ID3D11Device* Device, uint32_t Width, uint32_t Height, DXGI_FORMAT Format)
{
	Assert(Width % 2 == 0 && Height % 2 == 0);
	Assert(Format == DXGI_FORMAT_NV12 || Format == DXGI_FORMAT_P010);

	D3D11_TEXTURE2D_DESC TextureDesc =
	{
		.Width = Width,
		.Height = Height,
		.MipLevels = 1,
		.ArraySize = 1,
		.Format = Format,
		.SampleDesc = { 1, 0 },
		.Usage = D3D11_USAGE_DEFAULT,
		.BindFlags = D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_UNORDERED_ACCESS,
	};

	D3D11_UNORDERED_ACCESS_VIEW_DESC ViewOutDescY =
	{
		.Format = (Format == DXGI_FORMAT_NV12) ? DXGI_FORMAT_R8_UNORM : DXGI_FORMAT_R16_UNORM,
		.ViewDimension = D3D11_UAV_DIMENSION_TEXTURE2D,
	};

	D3D11_UNORDERED_ACCESS_VIEW_DESC ViewOutDescUV =
	{
		.Format = (Format == DXGI_FORMAT_NV12) ? DXGI_FORMAT_R8G8_UNORM : DXGI_FORMAT_R16G16_UNORM,
		.ViewDimension = D3D11_UAV_DIMENSION_TEXTURE2D,
	};

	D3D11_SHADER_RESOURCE_VIEW_DESC ViewInDescUV =
	{
		.Format = (Format == DXGI_FORMAT_NV12) ? DXGI_FORMAT_R8G8_UNORM : DXGI_FORMAT_R16G16_UNORM,
		.ViewDimension = D3D_SRV_DIMENSION_TEXTURE2D,
		.Texture2D.MipLevels = -1,
	};

	ID3D11Device_CreateTexture2D(Device, &TextureDesc, NULL, &Output->Texture);
	ID3D11Device_CreateUnorderedAccessView(Device, (ID3D11Resource*)Output->Texture, &ViewOutDescY, &Output->ViewOutY);
	ID3D11Device_CreateUnorderedAccessView(Device, (ID3D11Resource*)Output->Texture, &ViewOutDescUV, &Output->ViewOutUV);
	ID3D11Device_CreateShaderResourceView(Device, (ID3D11Resource*)Output->Texture, &ViewInDescUV, &Output->ViewInUV);
}

void IrYuvOutput_Release(IrYuvOutput* Output)
{
	ID3D11Texture2D_Release(Output->Texture);
	ID3D11UnorderedAccessView_Release(Output->ViewOutUV);
	ID3D11UnorderedAccessView_Release(Output->ViewOutY);
	ID3D11ShaderResourceView_Release(Output->ViewInUV);
}

void IrYuv_Create(IrYuv* Convert, ID3D11Device* Device, ID3D11Texture2D* InputTexture, uint32_t Width, uint32_t Height, IrYuvColorSpace ColorSpace, bool ImprovedConversion)
{
	Assert(Width % 2 == 0 && Height % 2 == 0);

	D3D11_SHADER_RESOURCE_VIEW_DESC InputViewDesc =
	{
		.Format = DXGI_FORMAT_B8G8R8A8_UNORM,
		.ViewDimension = D3D11_SRV_DIMENSION_TEXTURE2D,
		.Texture2D.MipLevels = -1,
	};
	ID3D11Device_CreateShaderResourceView(Device, (ID3D11Resource*)InputTexture, &InputViewDesc, &Convert->InputView);

	// https://en.wikipedia.org/wiki/YCbCr#ITU-R_BT.2020_conversion
	static const float BT2020[6][4] =
	{
		// RGB to YUV
		{ +0.26270f, +0.678000f, +0.0593000f },
		{ -0.13963f, -0.360370f, +0.5000000f },
		{ +0.50000f, -0.459786f, -0.0402143f },
		// YUV to RGB
		{ 1.f, +0.000000f, +1.474600f },
		{ 1.f, -0.164553f, -0.571353f },
		{ 1.f, +1.881400f, +0.000000f },
	};

	// https://en.wikipedia.org/wiki/YCbCr#ITU-R_BT.709_conversion
	static const float BT709[6][4] =
	{
		// RGB to YUV
		{ +0.2126f, +0.7152f, +0.0722f },
		{ -0.1146f, -0.3854f, +0.5000f },
		{ +0.5000f, -0.4542f, -0.0458f },
		// YUV to RGB
		{ 1.f, +0.0000f, +1.5748f },
		{ 1.f, -0.1873f, -0.4681f },
		{ 1.f, +1.8556f, +0.0000f },
	};

	// https://en.wikipedia.org/wiki/YCbCr#ITU-R_BT.601_conversion
	static const float BT601[6][4] =
	{
		// RGB to YUV
		{ +0.299000f, +0.587000f, +0.114000f },
		{ -0.168736f, -0.331264f, +0.500000f },
		{ +0.500000f, -0.418688f, -0.081312f },
		// YUV to RGB
		{ 1.f, +0.000000f, +1.402000f },
		{ 1.f, -0.344136f, -0.714136f },
		{ 1.f, +1.772000f, +0.000000f },
	};

	const float (*ConvertMatrix)[4];
	switch (ColorSpace)
	{
	case IrYuvColorSpace_BT601:  ConvertMatrix = BT601;  break;
	case IrYuvColorSpace_BT709:  ConvertMatrix = BT709;  break;
	case IrYuvColorSpace_BT2020: ConvertMatrix = BT2020; break;
	default: Assert(false);
	}

	D3D11_BUFFER_DESC ConstantBufferDesc =
	{
		.ByteWidth = 6 * 4 * sizeof(float),
		.Usage = D3D11_USAGE_IMMUTABLE,
		.BindFlags = D3D11_BIND_CONSTANT_BUFFER,
	};

	D3D11_SUBRESOURCE_DATA ConstantBufferData =
	{
		.pSysMem = ConvertMatrix,
	};

	ID3D11Device_CreateBuffer(Device, &ConstantBufferDesc, &ConstantBufferData, &Convert->ConstantBuffer);

	if (ImprovedConversion)
	{
		ID3DBlob* Shader;
		HR(D3DDecompressShaders(IrConvertPass1ShaderBytes, sizeof(IrConvertPass1ShaderBytes), 1, 0, NULL, 0, &Shader, NULL));
		ID3D11Device_CreateComputeShader(Device, ID3D10Blob_GetBufferPointer(Shader), ID3D10Blob_GetBufferSize(Shader), NULL, &Convert->Pass1);
		ID3D10Blob_Release(Shader);

		HR(D3DDecompressShaders(IrConvertPass2ShaderBytes, sizeof(IrConvertPass2ShaderBytes), 1, 0, NULL, 0, &Shader, NULL));
		ID3D11Device_CreateComputeShader(Device, ID3D10Blob_GetBufferPointer(Shader), ID3D10Blob_GetBufferSize(Shader), NULL, &Convert->Pass2);
		ID3D10Blob_Release(Shader);
		
		Convert->SinglePass = NULL;
	}
	else
	{
		ID3DBlob* Shader;
		HR(D3DDecompressShaders(IrConvertSinglePassShaderBytes, sizeof(IrConvertSinglePassShaderBytes), 1, 0, NULL, 0, &Shader, NULL));
		ID3D11Device_CreateComputeShader(Device, ID3D10Blob_GetBufferPointer(Shader), ID3D10Blob_GetBufferSize(Shader), NULL, &Convert->SinglePass);
		ID3D10Blob_Release(Shader);
	}

	Convert->Width = Width;
	Convert->Height = Height;
}

void IrYuv_Release(IrYuv* Convert)
{
	ID3D11ShaderResourceView_Release(Convert->InputView);
	ID3D11Buffer_Release(Convert->ConstantBuffer);

	if (Convert->SinglePass)
	{
		ID3D11ComputeShader_Release(Convert->SinglePass);
	}
	else
	{
		ID3D11ComputeShader_Release(Convert->Pass1);
		ID3D11ComputeShader_Release(Convert->Pass2);
	}
}

static void IrYuv_Dispatch(IrYuv* Convert, ID3D11DeviceContext* Context, IrYuvOutput* Output)
{
	if (Convert->SinglePass)
	{
		ID3D11UnorderedAccessView* OutputViews[] = { Output->ViewOutY, Output->ViewOutUV };

		ID3D11DeviceContext_ClearState(Context);
		ID3D11DeviceContext_CSSetShader(Context, Convert->SinglePass, NULL, 0);
		ID3D11DeviceContext_CSSetConstantBuffers(Context, 0, 1, &Convert->ConstantBuffer);
		ID3D11DeviceContext_CSSetShaderResources(Context, 0, 1, &Convert->InputView);
		ID3D11DeviceContext_CSSetUnorderedAccessViews(Context, 0, ARRAYSIZE(OutputViews), OutputViews, NULL);
		ID3D11DeviceContext_Dispatch(Context, DIV_ROUND_UP(Convert->Width / 2, 16), DIV_ROUND_UP(Convert->Height / 2, 16), 1);
	}
	else
	{
		ID3D11DeviceContext_ClearState(Context);
		ID3D11DeviceContext_CSSetShader(Context, Convert->Pass1, NULL, 0);
		ID3D11DeviceContext_CSSetConstantBuffers(Context, 0, 1, &Convert->ConstantBuffer);
		ID3D11DeviceContext_CSSetShaderResources(Context, 0, 1, &Convert->InputView);
		ID3D11DeviceContext_CSSetUnorderedAccessViews(Context, 1, 1, &Output->ViewOutUV, NULL);
		ID3D11DeviceContext_Dispatch(Context, DIV_ROUND_UP(Convert->Width / 2, 16), DIV_ROUND_UP(Convert->Height / 2, 16), 1);

		ID3D11ShaderResourceView* InputViews[] = { Convert->InputView, Output->ViewInUV };

		ID3D11DeviceContext_ClearState(Context);
		ID3D11DeviceContext_CSSetShader(Context, Convert->Pass2, NULL, 0);
		ID3D11DeviceContext_CSSetConstantBuffers(Context, 0, 1, &Convert->ConstantBuffer);
		ID3D11DeviceContext_CSSetShaderResources(Context, 0, ARRAYSIZE(InputViews), InputViews);
		ID3D11DeviceContext_CSSetUnorderedAccessViews(Context, 0, 1, &Output->ViewOutY, NULL);
		ID3D11DeviceContext_Dispatch(Context, DIV_ROUND_UP(Convert->Width, 16), DIV_ROUND_UP(Convert->Height, 16), 1);
	}
}

//


// ---------------------------------------------------------------------------
// Audio normalization, microphone capture and encoded packet replay
// ---------------------------------------------------------------------------
#define IR_AUDIO_RATE         48000u
#define IR_AUDIO_CHANNELS     2u
#define IR_AUDIO_BYTES_FRAME  (IR_AUDIO_RATE * IR_AUDIO_CHANNELS * sizeof(int16_t))
#define IR_AAC_FRAME          1024
#define IR_SECOND_100NS       10000000LL
#define IR_VIDEO_BUDGET       (128ull * 1024 * 1024)

static void IrPushPacket(IrEngine *e, const uint8_t *data, uint32_t bytes,
                         uint32_t codec_data, int64_t pts, int64_t duration, BOOL key);
static void IrPushMicBlock(IrEngine *e, const int16_t *samples, uint32_t frames, int64_t pts);

static double IrMsDiff(int64_t from, int64_t to)
{
    return (double)(to - from) / 10000.0;
}

static int32_t IrClamp16(double v)
{
    if (v < -32768.0) return -32768;
    if (v > 32767.0) return 32767;
    return (int32_t)(v >= 0 ? floor(v + 0.5) : ceil(v - 0.5));
}

static BOOL IrIsFloatFormat(const WAVEFORMATEX *f)
{
    if (!f) return FALSE;
    if (f->wFormatTag == WAVE_FORMAT_IEEE_FLOAT) return TRUE;
    if (f->wFormatTag == WAVE_FORMAT_EXTENSIBLE && f->cbSize >= sizeof(WAVEFORMATEXTENSIBLE) - sizeof(WAVEFORMATEX))
    {
        static const GUID float_subtype = { 0x00000003, 0x0000, 0x0010, { 0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71 } };
        const WAVEFORMATEXTENSIBLE *ext = (const WAVEFORMATEXTENSIBLE *)f;
        return IsEqualGUID(&ext->SubFormat, &float_subtype);
    }
    return FALSE;
}

// Stateful PCM normalizer. Keeping interpolation state across WASAPI packets is
// what prevents the repeating click that a per-packet resampler produces.
typedef struct
{
    int src_rate, src_channels, bits, is_float;
    int dst_channels;
    double next_src; // next output frame expressed in source-frame coordinates
} IrPcmConverter;

static void IrPcm_Init(IrPcmConverter *c, const WAVEFORMATEX *format, int dst_channels)
{
    memset(c, 0, sizeof(*c));
    if (!format) return;
    c->src_rate = (int)format->nSamplesPerSec;
    c->src_channels = (int)format->nChannels;
    c->bits = (int)format->wBitsPerSample;
    c->is_float = IrIsFloatFormat(format) ? 1 : 0;
    c->dst_channels = dst_channels;
}

static double IrPcm_Read(const uint8_t *data, uint32_t frames, uint32_t frame, uint32_t channel,
                         const IrPcmConverter *c)
{
    if (!data || c->src_rate <= 0 || c->src_channels <= 0) return 0.0;
    if (frame >= frames) frame = frames ? frames - 1 : 0;
    uint32_t ch = channel < (uint32_t)c->src_channels ? channel : 0;
    size_t index = ((size_t)frame * (size_t)c->src_channels + ch);
    if (c->is_float && c->bits == 32)
    {
        float value;
        memcpy(&value, data + index * 4, sizeof(value));
        return value;
    }
    switch (c->bits)
    {
    case 8:
        return (double)((int)data[index] - 128) / 128.0;
    case 16:
    {
        int16_t value;
        memcpy(&value, data + index * 2, sizeof(value));
        return (double)value / 32768.0;
    }
    case 24:
    {
        const uint8_t *p = data + index * 3;
        int32_t value = (int32_t)(p[0] | (p[1] << 8) | (p[2] << 16));
        if (value & 0x800000) value |= (int32_t)0xff000000;
        return (double)value / 8388608.0;
    }
    case 32:
    {
        int32_t value;
        memcpy(&value, data + index * 4, sizeof(value));
        return (double)value / 2147483648.0;
    }
    default:
        return 0.0;
    }
}

// Returns a newly allocated 48 kHz s16 buffer and sets *out_frames. Caller frees.
static int16_t *IrPcm_Convert(IrPcmConverter *c, const uint8_t *data, uint32_t frames, uint32_t *out_frames)
{
    *out_frames = 0;
    if (!c || c->src_rate <= 0 || c->dst_channels <= 0 || frames == 0) return NULL;
    const double step = (double)c->src_rate / (double)IR_AUDIO_RATE;
    uint32_t count = 0;
    if (c->next_src < (double)frames)
        count = (uint32_t)floor(((double)frames - c->next_src) / step) + 1;
    if (count == 0) return NULL;
    // A hostile format must not grow the replay allocator without bound.
    uint64_t cap = ((uint64_t)frames * IR_AUDIO_RATE / (uint64_t)c->src_rate) + 8;
    if (count > cap) count = (uint32_t)cap;
    int16_t *out = (int16_t *)malloc((size_t)count * (size_t)c->dst_channels * sizeof(int16_t));
    if (!out) return NULL;
    for (uint32_t i = 0; i < count; ++i)
    {
        double pos = c->next_src + (double)i * step;
        uint32_t i0 = (uint32_t)floor(pos);
        if (i0 >= frames) i0 = frames - 1;
        uint32_t i1 = i0 + 1 < frames ? i0 + 1 : i0;
        double frac = pos - floor(pos);
        for (int ch = 0; ch < c->dst_channels; ++ch)
        {
            double a = IrPcm_Read(data, frames, i0, (uint32_t)ch, c);
            double b = IrPcm_Read(data, frames, i1, (uint32_t)ch, c);
            out[(size_t)i * c->dst_channels + ch] = (int16_t)IrClamp16((a + (b - a) * frac) * 32768.0);
        }
    }
    c->next_src += (double)count * step - (double)frames;
    if (c->next_src < 0.0) c->next_src = 0.0;
    *out_frames = count;
    return out;
}

// ---------------------------------------------------------------------------
// Independent microphone capture (eCapture, 48 kHz mono s16)
// ---------------------------------------------------------------------------
typedef struct IrMic IrMic;
static DWORD WINAPI IrMicThread(LPVOID arg);
static void IrMic_Stop(IrMic *m);

struct IrMic
{
    IMMDeviceEnumerator *enumerator;
    IMMDevice *device;
    IAudioClient *client;
    IAudioCaptureClient *capture;
    WAVEFORMATEX *format;
    IrPcmConverter converter;
    HANDLE stop;
    HANDLE event;
    HANDLE thread;
    BOOL running;
    HRESULT activate_hr, init_hr, start_hr, fail_hr;
    int legacy_fail; // -1 thread unavailable, -2 stream failed, -3 no device
    long convert_failures;
    volatile LONG64 frames, events, got_frames;
    volatile long peak;
    wchar_t name[256];
    IrEngine *engine;
};

static void IrMic_SetPeak(IrMic *m, const int16_t *data, uint32_t frames)
{
    int peak = 0;
    for (uint32_t i = 0; i < frames; ++i)
    {
        int v = data[i] < 0 ? -(int)data[i] : (int)data[i];
        if (v > peak) peak = v;
    }
    for (;;)
    {
        long old = InterlockedCompareExchange(&m->peak, 0, 0);
        if (peak <= old || InterlockedCompareExchange(&m->peak, peak, old) == old) break;
    }
}

static HRESULT IrMic_FindDevice(IrMic *m, const wchar_t *wanted)
{
    HRESULT hr = CoCreateInstance(&CLSID_MMDeviceEnumerator, NULL, CLSCTX_ALL,
                                  &IID_IMMDeviceEnumerator, (void **)&m->enumerator);
    if (FAILED(hr)) return hr;
    if (wanted && *wanted)
    {
        IMMDeviceCollection *collection = NULL;
        if (SUCCEEDED(IMMDeviceEnumerator_EnumAudioEndpoints(m->enumerator, eCapture,
                                                             DEVICE_STATE_ACTIVE, &collection)))
        {
            UINT count = 0;
            IMMDeviceCollection_GetCount(collection, &count);
            for (UINT i = 0; i < count; ++i)
            {
                IMMDevice *device = NULL;
                if (FAILED(IMMDeviceCollection_Item(collection, i, &device))) continue;
                IPropertyStore *store = NULL;
                if (SUCCEEDED(IMMDevice_OpenPropertyStore(device, STGM_READ, &store)))
                {
                    PROPVARIANT value;
                    PropVariantInit(&value);
                    if (SUCCEEDED(IPropertyStore_GetValue(store, &PKEY_Device_FriendlyName, &value)) &&
                        value.vt == VT_LPWSTR && value.pwszVal && wcscmp(value.pwszVal, wanted) == 0)
                    {
                        m->device = device;
                        device = NULL;
                    }
                    PropVariantClear(&value);
                    IPropertyStore_Release(store);
                }
                if (device) IMMDevice_Release(device);
                if (m->device) break;
            }
            IMMDeviceCollection_Release(collection);
        }
    }
    if (!m->device)
        hr = IMMDeviceEnumerator_GetDefaultAudioEndpoint(m->enumerator, eCapture, eConsole, &m->device);
    if (SUCCEEDED(hr) && m->device)
    {
        IPropertyStore *store = NULL;
        m->name[0] = L'\0';
        if (SUCCEEDED(IMMDevice_OpenPropertyStore(m->device, STGM_READ, &store)))
        {
            PROPVARIANT value;
            PropVariantInit(&value);
            if (SUCCEEDED(IPropertyStore_GetValue(store, &PKEY_Device_FriendlyName, &value)) &&
                value.vt == VT_LPWSTR && value.pwszVal)
                wcsncpy_s(m->name, _countof(m->name), value.pwszVal, _TRUNCATE);
            PropVariantClear(&value);
            IPropertyStore_Release(store);
        }
    }
    return hr;
}

static BOOL IrMic_Start(IrMic *m, IrEngine *e, const wchar_t *device_name)
{
    // IrMic_Stop is safe on a partially initialized structure; every failure
    // path below releases what was acquired so a later IR_Start cannot orphan
    // COM objects, events or the device enumerator.
    memset(m, 0, sizeof(*m));
    m->engine = e;
    m->activate_hr = IrMic_FindDevice(m, device_name);
    if (FAILED(m->activate_hr) || !m->device)
    {
        m->legacy_fail = -3;
        m->fail_hr = m->activate_hr;
        IrMic_Stop(m);
        return FALSE;
    }

    HRESULT hr = IMMDevice_Activate(m->device, &IID_IAudioClient, CLSCTX_ALL, NULL, (void **)&m->client);
    if (FAILED(hr)) { m->fail_hr = hr; m->legacy_fail = -2; IrMic_Stop(m); return FALSE; }

    WAVEFORMATEX format;
    memset(&format, 0, sizeof(format));
    format.wFormatTag = WAVE_FORMAT_PCM;
    format.nChannels = 1;
    format.nSamplesPerSec = IR_AUDIO_RATE;
    format.wBitsPerSample = 16;
    format.nBlockAlign = 2;
    format.nAvgBytesPerSec = IR_AUDIO_RATE * 2;
    m->format = (WAVEFORMATEX *)CoTaskMemAlloc(sizeof(format));
    if (!m->format) { m->fail_hr = E_OUTOFMEMORY; m->legacy_fail = -2; IrMic_Stop(m); return FALSE; }
    memcpy(m->format, &format, sizeof(format));

    DWORD flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM |
                  AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
    m->init_hr = IAudioClient_Initialize(m->client, AUDCLNT_SHAREMODE_SHARED, flags,
                                         0, 0, m->format, NULL);
    m->activate_hr = S_OK;
    if (FAILED(m->init_hr)) { m->fail_hr = m->init_hr; m->legacy_fail = -2; IrMic_Stop(m); return FALSE; }
    hr = IAudioClient_GetService(m->client, &IID_IAudioCaptureClient, (void **)&m->capture);
    m->stop = CreateEventW(NULL, TRUE, FALSE, NULL);
    m->event = CreateEventW(NULL, FALSE, FALSE, NULL);
    if (SUCCEEDED(hr) && m->event) hr = IAudioClient_SetEventHandle(m->client, m->event);
    if (!m->stop || !m->event) hr = E_OUTOFMEMORY;
    if (SUCCEEDED(hr)) hr = IAudioClient_Start(m->client);
    m->start_hr = hr;
    if (FAILED(hr)) { m->fail_hr = hr; m->legacy_fail = -2; IrMic_Stop(m); return FALSE; }

    IrPcm_Init(&m->converter, m->format, 1);
    m->thread = CreateThread(NULL, 0, IrMicThread, m, 0, NULL);
    if (!m->thread)
    {
        m->fail_hr = HRESULT_FROM_WIN32(GetLastError());
        m->legacy_fail = -1;
        IrMic_Stop(m);
        return FALSE;
    }
    m->running = TRUE;
    return TRUE;
}

static void IrMic_Stop(IrMic *m)
{
    if (!m) return;
    m->running = FALSE;
    if (m->stop) SetEvent(m->stop);
    if (m->thread)
    {
        WaitForSingleObject(m->thread, INFINITE);
        CloseHandle(m->thread);
        m->thread = NULL;
    }
    if (m->stop) { CloseHandle(m->stop); m->stop = NULL; }
    if (m->event) { CloseHandle(m->event); m->event = NULL; }
    if (m->client) IAudioClient_Stop(m->client);
    if (m->capture) { IAudioCaptureClient_Release(m->capture); m->capture = NULL; }
    if (m->client) { IAudioClient_Release(m->client); m->client = NULL; }
    if (m->device) { IMMDevice_Release(m->device); m->device = NULL; }
    if (m->enumerator) { IMMDeviceEnumerator_Release(m->enumerator); m->enumerator = NULL; }
    if (m->format) { CoTaskMemFree(m->format); m->format = NULL; }
}

static DWORD WINAPI IrMicThread(LPVOID arg)
{
    IrMic *m = (IrMic *)arg;
    HRESULT co = CoInitializeEx(NULL, COINIT_MULTITHREADED);
    if (FAILED(co) && co != RPC_E_CHANGED_MODE)
    {
        m->fail_hr = co;
        m->legacy_fail = -1;
        return 0;
    }
    DWORD task = 0;
    HANDLE mm = AvSetMmThreadCharacteristicsW(L"Pro Audio", &task);
    HANDLE waits[2] = { m->stop, m->event };
    for (;;)
    {
        DWORD rc = WaitForMultipleObjects(2, waits, FALSE, INFINITE);
        if (rc == WAIT_OBJECT_0) break;
        if (rc != WAIT_OBJECT_0 + 1) continue;
        UINT32 next = 0;
        while (m->running && SUCCEEDED(IAudioCaptureClient_GetNextPacketSize(m->capture, &next)) && next > 0)
        {
            BYTE *data = NULL;
            UINT32 frames = 0;
            DWORD flags = 0;
            UINT64 position = 0, timestamp = 0;
            if (FAILED(IAudioCaptureClient_GetBuffer(m->capture, &data, &frames, &flags, &position, &timestamp)))
            {
                InterlockedIncrement(&m->convert_failures);
                m->fail_hr = E_FAIL;
                m->legacy_fail = -2;
                break;
            }
            InterlockedIncrement64(&m->events);
            if (frames)
            {
                uint32_t out_frames = 0;
                // Feeding NULL keeps the resampler phase continuous across
                // silent WASAPI packets instead of resetting it at each one.
                int16_t *pcm = IrPcm_Convert(&m->converter,
                                             (flags & AUDCLNT_BUFFERFLAGS_SILENT) ? NULL : data,
                                             frames, &out_frames);
                if (!pcm) InterlockedIncrement(&m->convert_failures);
                int64_t pts = timestamp ? IrQpcTo100ns((int64_t)timestamp) : IrNow100ns();
                if (pcm && out_frames)
                {
                    IrMic_SetPeak(m, pcm, out_frames);
                    IrPushMicBlock(m->engine, pcm, out_frames, pts);
                    InterlockedIncrement64(&m->got_frames);
                    free(pcm);
                }
                InterlockedExchangeAdd64(&m->frames, frames);
            }
            IAudioCaptureClient_ReleaseBuffer(m->capture, frames);
        }
    }
    if (mm) AvRevertMmThreadCharacteristics(mm);
    if (SUCCEEDED(co)) CoUninitialize();
    return 0;
}

#define MFT64(high, low) (((UINT64)high << 32) | (low))

// ---------------------------------------------------------------------------
// Long-lived Media Foundation video encoder feeding the compressed replay ring
// ---------------------------------------------------------------------------
static const GUID ir_mft_friendly_name = { 0x314ffbae, 0x5b41, 0x4c95, { 0x9c, 0x19, 0x4e, 0x7d, 0x58, 0x6f, 0xac, 0xe3 } };
static const GUID ir_mft_friendly_name_utf8 = { 0x314ff923, 0x2c6d, 0x4cd2, { 0x8d, 0x32, 0x20, 0xe0, 0xa3, 0xe0, 0xd6, 0x8e } };

typedef struct
{
    IMFTransform *mft;
    ICodecAPI *api;
    IMFMediaEventGenerator *events;
    IMFDXGIDeviceManager *manager;
    UINT32 manager_token;
    BOOL async, provides_samples;
    int pending_need;
    BOOL drained;
    DWORD out_size;
    uint32_t width, height;
    int64_t frame_duration;
    GUID subtype;
    BOOL hevc, av1, segmented;
    uint8_t *seq_header;
    uint32_t seq_header_size;
    BOOL wrote_header;
    uint32_t max_width, max_height;
    int active_vendor;

    // GPU conversion state, reused for every frame.
    IrResize resize;
    IrYuv yuv;
    IrYuvOutput output;
    ID3D11Texture2D *staging;
    ID3D11RenderTargetView *input_view;
    uint8_t *nv12;
    uint32_t input_width, input_height, output_width, output_height;
    BOOL gpu_ready;
} IrLiveEncoder;

// ---------------------------------------------------------------------------
// Replay buffer and engine object types (defined early for the live encoder).
// ---------------------------------------------------------------------------
typedef struct
{
    ID3D11Texture2D *texture;
    RECT rect;
    int64_t pts100ns;
    ULONG refs;
} IrFrame;

typedef struct
{
    uint8_t *data;
    UINT32 frames;
    UINT32 bytes;
    int64_t pts100ns;
    ULONG refs;
} IrAudioBlock;

struct IrPacket
{
    uint8_t *data;
    uint32_t bytes;
    uint32_t codec_data;
    int64_t pts100ns;
    int64_t duration;
    BOOL key;
    ULONG refs;
};

typedef struct
{
    const int16_t *pcm;
    int32_t frames;
    int64_t pts100ns;
} IrAudioChunk;

typedef struct IrSaveJob IrSaveJob;

struct IrEngine
{
    CRITICAL_SECTION cs;
    CRITICAL_SECTION ring_cs;
    volatile long running;
    volatile long paused;
    volatile long shutdown;

    ID3D11Device *dev;
    ID3D11DeviceContext *ctx;
    Config config;

    int method;
    int monitor_index;
    HWND window;
    uint32_t options;
    int fps;
    int ring_seconds;
    int bitrate_kbps;
    uint32_t cap_w;
    uint32_t cap_h;
    int want_audio;
    int want_mic;
    wchar_t mic_device[256];
    int pref;
    int active_encoder;
    HRESULT hw_init_hr;

    IrLiveEncoder live;
    IrMic mic;
    int64_t pause_start;
    int64_t time_offset;
    int64_t next_idr_pts;
    volatile long force_idr;
    int last_encoder_width;
    int last_encoder_height;
    int encoded_width;
    int encoded_height;

    IrWgc wgc;
    IrDxgi dxgi;
    IrAudio audio;
    WAVEFORMATEXTENSIBLE audio_fmt;
    BOOL audio_running;
    HANDLE audio_thread;
    HANDLE audio_stop;
    HRESULT audio_activate_hr;
    HRESULT audio_init_hr;
    HRESULT audio_start_hr;
    int audio_tag;
    long audio_convert_failures;
    IrPcmConverter audio_converter;
    volatile long audio_peak;

    int64_t start_qpc;
    int64_t start_pts;
    volatile LONG64 last_frame_pts;
    volatile LONG64 last_frame_tick;
    volatile LONG64 repeat_pts;
    CRITICAL_SECTION encoder_cs;

    IrFrame **frames;
    int frame_cap;
    int frame_head;
    int frame_count;
    IrAudioBlock **audio_blocks;
    int audio_cap;
    int audio_head;
    int audio_count;
    IrAudioBlock **mic_blocks;
    int mic_cap;
    int mic_head;
    int mic_count;
    IrPacket **packets;
    int packet_cap;
    int packet_head;
    int packet_count;
    uint64_t packet_bytes;

    volatile LONG64 captured;
    volatile LONG64 paced;
    volatile LONG64 rejected;
    volatile LONG64 encoded;
    volatile LONG64 dropped;
    volatile LONG64 frames_encoded;
    volatile LONG64 audio_frames;
    volatile LONG64 audio_events;
    volatile LONG64 audio_got;
    volatile LONG64 heartbeat;
    volatile LONG64 last_pts;
    volatile LONG64 timing_frames;
    double timing_map_ms;
    double timing_encode_ms;
    double timing_render_ms;
    double timing_submit_ms;
    double timing_snapshot_ms;
    double timing_unmap_ms;
    volatile long fail_stage;
    volatile long fail_hr;
    volatile long capture_hr;

    HANDLE save_wake;
    HANDLE save_stop;
    HANDLE save_thread;
    HANDLE hb_stop;
    HANDLE hb_thread;
    CRITICAL_SECTION job_cs;
    IrSaveJob *job_head;
    UINT64 next_job_id;
};

struct IrSaveJob
{
    UINT64 id;
    wchar_t path[MAX_PATH];
    int seconds_back;
    Config config;
    int width;
    int height;
    int fps;
    BOOL segmented;
    GUID subtype;
    uint8_t *seq_header;
    uint32_t seq_header_size;
    int64_t trim_pts;
    int64_t base_pts;
    int64_t end_pts;
    IrPacket **packets;
    int packet_count;
    IrAudioBlock **sys;
    int sys_count;
    IrAudioBlock **mic;
    int mic_count;
    volatile long progress;
    volatile long result;
    volatile long done;
    volatile long refs;
    IrSaveJob *next;
};

static GUID IrCodecSubtype(int codec_bits)
{
    switch (codec_bits)
    {
    case 1: return MFVideoFormat_HEVC;
    case 2: return MFVideoFormat_H264;
    case 3: return MFVideoFormat_AV1;
    default: return MFVideoFormat_H264;
    }
}

static int IrVendorOrdinal(int pref)
{
    switch (pref & 0xff)
    {
    case 5: return 1; // QSV
    case 6: return 2; // VP12
    case 7: return 3; // NVENC
    case 8: return 4; // AMF
    case 11: return 5; // software
    default: return 0; // auto
    }
}

// Compact availability bits (pref bits 24..31) win over the ordinal when the
// caller supplied them, matching the managed encoder preference mapping.
static int IrWantedVendorOrdinal(int pref)
{
    // Compact bits are QSV, VP12, NVENC, AMF in that order (software is the
    // implicit MFT fallback and has no compact bit).
    unsigned compact = ((unsigned)pref >> 24) & 0xffu;
    if (compact & 0x10u) return 1;
    if (compact & 0x20u) return 2;
    if (compact & 0x40u) return 3;
    if (compact & 0x80u) return 4;
    return IrVendorOrdinal(pref);
}

static void IrLowerName(const WCHAR *src, WCHAR *dst, size_t capacity)
{
    size_t i = 0;
    for (; src && src[i] && i + 1 < capacity; ++i) dst[i] = (WCHAR)towlower(src[i]);
    dst[i] = L'\0';
}

static int IrActivateVendor(IMFActivate *activate)
{
    WCHAR *name = NULL;
    if (FAILED(IMFActivate_GetAllocatedString(activate, &ir_mft_friendly_name_utf8, &name, NULL)) || !name)
        IMFActivate_GetAllocatedString(activate, &ir_mft_friendly_name, &name, NULL);
    if (!name) return 0;
    WCHAR lower[256];
    IrLowerName(name, lower, _countof(lower));
    CoTaskMemFree(name);
    if (wcsstr(lower, L"nvidia") || wcsstr(lower, L"nvenc")) return 3;
    if (wcsstr(lower, L"amd") || wcsstr(lower, L"amf") || wcsstr(lower, L"radeon")) return 4;
    if (wcsstr(lower, L"microsoft") || wcsstr(lower, L"software")) return 5;
    if (wcsstr(lower, L"intel") || wcsstr(lower, L"qsv") || wcsstr(lower, L"quick sync") ||
        wcsstr(lower, L"arc") || wcsstr(lower, L"core ultra"))
        return wcsstr(lower, L"arc") || wcsstr(lower, L"core ultra") ? 2 : 1;
    return 0;
}

static HRESULT IrSetCodecU32(ICodecAPI *api, const GUID *key, uint32_t value)
{
    VARIANT v;
    VariantInit(&v);
    v.vt = VT_UI4;
    v.ulVal = value;
    HRESULT hr = ICodecAPI_SetValue(api, key, &v);
    VariantClear(&v);
    return hr;
}

static HRESULT IrSetCodecBool(ICodecAPI *api, const GUID *key, BOOL value)
{
    VARIANT v;
    VariantInit(&v);
    v.vt = VT_BOOL;
    v.boolVal = value ? VARIANT_TRUE : VARIANT_FALSE;
    HRESULT hr = ICodecAPI_SetValue(api, key, &v);
    VariantClear(&v);
    return hr;
}

static void IrLive_ReadSeqHeader(IrLiveEncoder *enc)
{
    free(enc->seq_header);
    enc->seq_header = NULL;
    enc->seq_header_size = 0;
    IMFMediaType *type = NULL;
    if (FAILED(IMFTransform_GetOutputCurrentType(enc->mft, 0, &type)) || !type) return;
    UINT32 size = 0;
    if (SUCCEEDED(IMFMediaType_GetBlobSize(type, &MF_MT_MPEG_SEQUENCE_HEADER, &size)) && size > 4)
    {
        uint8_t *buf = (uint8_t *)malloc(size);
        UINT32 written = 0;
        if (buf && SUCCEEDED(IMFMediaType_GetBlob(type, &MF_MT_MPEG_SEQUENCE_HEADER, buf, size, &written)) &&
            written > 4 && ((buf[0] == 0 && buf[1] == 0 && buf[2] == 1) ||
                            (buf[0] == 0 && buf[1] == 0 && buf[2] == 0 && buf[3] == 1)))
        {
            enc->seq_header = buf;
            enc->seq_header_size = written;
            buf = NULL;
        }
        free(buf);
    }
    IMFMediaType_Release(type);
}

static BOOL IrHasKeyNal(const uint8_t *data, uint32_t size, BOOL hevc)
{
    for (uint32_t i = 0; i + 3 < size; ++i)
    {
        if (data[i] != 0 || data[i + 1] != 0 || data[i + 2] != 1) continue;
        uint32_t at = i + 3;
        uint8_t b = data[at];
        int type = hevc ? ((b >> 1) & 0x3f) : (b & 0x1f);
        if (hevc ? (type >= 16 && type <= 21) : (type == 5)) return TRUE;
    }
    return FALSE;
}

static BOOL IrHasParamSets(const uint8_t *data, uint32_t size, BOOL hevc)
{
    BOOL vps = FALSE, sps = FALSE, pps = FALSE;
    for (uint32_t i = 0; i + 3 < size; ++i)
    {
        if (data[i] != 0 || data[i + 1] != 0 || data[i + 2] != 1) continue;
        uint32_t at = i + 3;
        uint8_t b = data[at];
        int type = hevc ? ((b >> 1) & 0x3f) : (b & 0x1f);
        if (hevc)
        {
            if (type == 32) vps = TRUE;
            else if (type == 33) sps = TRUE;
            else if (type == 34) pps = TRUE;
        }
        else
        {
            if (type == 7) sps = TRUE;
            else if (type == 8) pps = TRUE;
        }
    }
    return hevc ? (vps && sps && pps) : (sps && pps);
}

// Copy VPS/SPS/PPS (HEVC) or SPS/PPS (H.264) NAL units, including their
// Annex-B start codes, from an in-band keyframe. Some MFTs emit parameter sets
// inside the sample instead of exposing them as MF_MT_MPEG_SEQUENCE_HEADER.
static uint8_t *IrCollectParamSets(const uint8_t *data, uint32_t size, BOOL hevc, uint32_t *out_size)
{
    *out_size = 0;
    if (!data || size < 4) return NULL;
    uint8_t *out = (uint8_t *)malloc(size);
    if (!out) return NULL;
    uint32_t used = 0, pos = 0;
    for (;;)
    {
        while (pos + 3 <= size)
        {
            if (data[pos] == 0 && data[pos + 1] == 0 && data[pos + 2] == 1) break;
            if (pos + 4 <= size && data[pos] == 0 && data[pos + 1] == 0 &&
                data[pos + 2] == 0 && data[pos + 3] == 1) break;
            ++pos;
        }
        if (pos + 3 > size) break;
        uint32_t start = pos;
        uint32_t sc = (data[pos] == 0 && data[pos + 1] == 0 && data[pos + 2] == 1) ? 3 : 4;
        pos += sc;
        if (pos >= size) break;
        uint32_t end = pos;
        while (end + 3 <= size)
        {
            if (data[end] == 0 && data[end + 1] == 0 && data[end + 2] == 1) break;
            if (end + 4 <= size && data[end] == 0 && data[end + 1] == 0 &&
                data[end + 2] == 0 && data[end + 3] == 1) break;
            ++end;
        }
        uint8_t header = data[pos];
        int type = hevc ? ((header >> 1) & 0x3f) : (header & 0x1f);
        BOOL keep = hevc ? (type == 32 || type == 33 || type == 34) : (type == 7 || type == 8);
        if (keep && end > start)
        {
            uint32_t len = end - start;
            memcpy(out + used, data + start, len);
            used += len;
        }
        pos = end;
    }
    if (!used) { free(out); return NULL; }
    *out_size = used;
    return out;
}

static void IrLive_EmitSample(IrLiveEncoder *enc, IMFSample *sample, IrEngine *engine)
{
    IMFMediaBuffer *buffer = NULL;
    if (FAILED(IMFSample_ConvertToContiguousBuffer(sample, &buffer)) || !buffer) return;
    BYTE *data = NULL;
    DWORD size = 0;
    if (SUCCEEDED(IMFMediaBuffer_Lock(buffer, &data, NULL, &size)) && data && size)
    {
        UINT32 clean = 0;
        BOOL key = (enc->av1 ? FALSE : IrHasKeyNal(data, size, enc->hevc)) ||
                   (SUCCEEDED(IMFSample_GetUINT32(sample, &MFSampleExtension_CleanPoint, &clean)) && clean);
        BOOL packet_params = enc->av1 ? FALSE : IrHasParamSets(data, size, enc->hevc);
        LONGLONG pts = 0;
        IMFSample_GetSampleTime(sample, &pts);
        LONGLONG duration = 0;
        IMFSample_GetSampleDuration(sample, &duration);
        if (duration <= 0) duration = enc->frame_duration;

        uint8_t *packet = NULL;
        uint32_t bytes = 0, codec_data = 0;
        if (!packet_params && !enc->av1 && enc->seq_header_size &&
            (key || !enc->wrote_header) && (uint64_t)size + enc->seq_header_size <= 0x40000000ull)
        {
            bytes = size + enc->seq_header_size;
            packet = (uint8_t *)malloc(bytes);
            if (packet)
            {
                memcpy(packet, enc->seq_header, enc->seq_header_size);
                memcpy(packet + enc->seq_header_size, data, size);
                codec_data = enc->seq_header_size;
            }
        }
        if (!packet)
        {
            packet = (uint8_t *)malloc(size);
            if (packet) { memcpy(packet, data, size); bytes = size; }
        }
        if (packet)
        {
            IrPushPacket(engine, packet, bytes, codec_data, (int64_t)pts, (int64_t)duration, key);
            if (key || packet_params) enc->wrote_header = TRUE;
            free(packet);
        }
        IMFMediaBuffer_Unlock(buffer);
    }
    IMFMediaBuffer_Release(buffer);
}

static BOOL IrLive_ProcessOutput(IrLiveEncoder *enc, IrEngine *engine, BOOL *need_more, char *error, size_t error_size)
{
    MFT_OUTPUT_DATA_BUFFER output;
    memset(&output, 0, sizeof(output));
    IMFSample *owned = NULL;
    if (!enc->provides_samples)
    {
        IMFMediaBuffer *buffer = NULL;
        if (FAILED(MFCreateSample(&owned)) || !owned ||
            FAILED(MFCreateMemoryBuffer(enc->out_size, &buffer)) || !buffer)
        {
            if (owned) IMFSample_Release(owned);
            _snprintf_s(error, error_size, _TRUNCATE, "Cannot allocate encoder output sample");
            return FALSE;
        }
        IMFSample_AddBuffer(owned, buffer);
        IMFMediaBuffer_Release(buffer);
        output.pSample = owned;
    }
    DWORD status = 0;
    HRESULT hr = IMFTransform_ProcessOutput(enc->mft, 0, 1, &output, &status);
    if (need_more) *need_more = FALSE;
    if (hr == MF_E_TRANSFORM_NEED_MORE_INPUT)
    {
        if (need_more) *need_more = TRUE;
    }
    else if (hr == MF_E_TRANSFORM_STREAM_CHANGE)
    {
        IrLive_ReadSeqHeader(enc);
        enc->wrote_header = FALSE;
    }
    else if (FAILED(hr))
    {
        _snprintf_s(error, error_size, _TRUNCATE, "Encoder output failed (0x%08lX)", (unsigned long)hr);
    }
    else if (output.pSample)
    {
        IrLive_EmitSample(enc, output.pSample, engine);
    }
    if (owned) IMFSample_Release(owned);
    if (output.pEvents) IMFCollection_Release(output.pEvents);
    // MF_E_TRANSFORM_NEED_MORE_INPUT (the normal "drained" result, signalled
    // through *need_more) and MF_E_TRANSFORM_STREAM_CHANGE (handled above) are
    // both failure HRESULTs but are expected encoder events, not errors.
    // Returning SUCCEEDED(hr) here would latch STAGE_FRAME on every frame and
    // leave the error message empty, even though packets were emitted.
    if (hr == MF_E_TRANSFORM_NEED_MORE_INPUT || hr == MF_E_TRANSFORM_STREAM_CHANGE)
        return TRUE;
    return SUCCEEDED(hr);
}

static BOOL IrLive_Pump(IrLiveEncoder *enc, IrEngine *engine, BOOL wait_need, char *error, size_t error_size)
{
    if (!enc->async)
    {
        for (;;)
        {
            BOOL need = FALSE;
            if (!IrLive_ProcessOutput(enc, engine, &need, error, error_size)) return FALSE;
            if (need) return TRUE;
        }
    }
    uint64_t deadline = GetTickCount64() + 200;
    for (;;)
    {
        if (wait_need && enc->pending_need > 0) return TRUE;
        if (wait_need && GetTickCount64() >= deadline)
        {
            _snprintf_s(error, error_size, _TRUNCATE, "Encoder timed out waiting for input");
            return FALSE;
        }
        IMFMediaEvent *event = NULL;
        HRESULT hr = IMFMediaEventGenerator_GetEvent(enc->events, MF_EVENT_FLAG_NO_WAIT, &event);
        if (hr == MF_E_NO_EVENTS_AVAILABLE)
        {
            if (!wait_need) return TRUE;
            Sleep(1);
            continue;
        }
        if (FAILED(hr))
        {
            _snprintf_s(error, error_size, _TRUNCATE, "Encoder event failed (0x%08lX)", (unsigned long)hr);
            return FALSE;
        }
        MediaEventType type = MEUnknown;
        IMFMediaEvent_GetType(event, &type);
        if (type == METransformNeedInput)
        {
            enc->pending_need++;
            if (wait_need) { IMFMediaEvent_Release(event); return TRUE; }
        }
        else if (type == METransformHaveOutput)
        {
            BOOL need = FALSE;
            if (!IrLive_ProcessOutput(enc, engine, &need, error, error_size))
            {
                IMFMediaEvent_Release(event);
                return FALSE;
            }
        }
        else if (type == METransformDrainComplete)
        {
            enc->drained = TRUE;
        }
        else if (type == MEError)
        {
            HRESULT status = S_OK;
            IMFMediaEvent_GetStatus(event, &status);
            _snprintf_s(error, error_size, _TRUNCATE, "Encoder error (0x%08lX)", (unsigned long)status);
            IMFMediaEvent_Release(event);
            return FALSE;
        }
        IMFMediaEvent_Release(event);
    }
}

static BOOL IrLive_Feed(IrLiveEncoder *enc, IrEngine *engine, IMFSample *sample,
                        int64_t pts, BOOL idr, char *error, size_t error_size)
{
    IMFSample_SetSampleTime(sample, pts);
    IMFSample_SetSampleDuration(sample, enc->frame_duration);
    if (idr && enc->api) IrSetCodecU32(enc->api, &CODECAPI_AVEncVideoForceKeyFrame, 1);
    if (enc->async && !IrLive_Pump(enc, engine, TRUE, error, error_size)) return FALSE;
    HRESULT hr = IMFTransform_ProcessInput(enc->mft, 0, sample, 0);
    if (hr == MF_E_NOTACCEPTING && !enc->async)
    {
        if (!IrLive_Pump(enc, engine, FALSE, error, error_size)) return FALSE;
        hr = IMFTransform_ProcessInput(enc->mft, 0, sample, 0);
    }
    if (FAILED(hr))
    {
        _snprintf_s(error, error_size, _TRUNCATE, "Encoder input failed (0x%08lX)", (unsigned long)hr);
        return FALSE;
    }
    if (enc->async && enc->pending_need > 0) enc->pending_need--;
    return IrLive_Pump(enc, engine, FALSE, error, error_size);
}

static void IrLive_Release(IrLiveEncoder *enc)
{
    if (enc->mft)
    {
        IMFTransform_ProcessMessage(enc->mft, MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
        IMFTransform_ProcessMessage(enc->mft, MFT_MESSAGE_COMMAND_DRAIN, 0);
    }
    if (enc->api) ICodecAPI_Release(enc->api);
    if (enc->events) IMFMediaEventGenerator_Release(enc->events);
    if (enc->mft) IMFTransform_Release(enc->mft);
    // The transform may still hold a reference, but keep the manager alive
    // until after the transform is gone so device handles stay valid.
    if (enc->manager) IMFDXGIDeviceManager_Release(enc->manager);
    free(enc->seq_header);

    if (enc->staging) ID3D11Texture2D_Release(enc->staging);
    if (enc->input_view) ID3D11RenderTargetView_Release(enc->input_view);
    if (enc->gpu_ready)
    {
        IrYuvOutput_Release(&enc->output);
        IrYuv_Release(&enc->yuv);
        IrResize_Release(&enc->resize);
    }
    free(enc->nv12);
    memset(enc, 0, sizeof(*enc));
}

static void IrReleaseActivates(IMFActivate **list, UINT32 count)
{
    if (!list) return;
    for (UINT32 i = 0; i < count; ++i)
    {
        if (list[i]) IMFActivate_Release(list[i]);
    }
    CoTaskMemFree(list);
}

static BOOL IrLive_Create(IrLiveEncoder *enc, IrEngine *engine, ID3D11Device *device,
                          uint32_t width, uint32_t height, int fps, int bitrate_kbps,
                          int pref, char *error, size_t error_size)
{
    memset(enc, 0, sizeof(*enc));
    HRESULT hr = MFStartup(MF_VERSION, MFSTARTUP_LITE);
    if (FAILED(hr) && hr != MF_E_ALREADY_INITIALIZED)
    {
        _snprintf_s(error, error_size, _TRUNCATE, "Media Foundation startup failed (0x%08lX)", (unsigned long)hr);
        return FALSE;
    }
    int codec_bits = (pref >> 15) & 3;
    enc->subtype = IrCodecSubtype(codec_bits);
    enc->hevc = IsEqualGUID(&enc->subtype, &MFVideoFormat_HEVC);
    enc->av1 = IsEqualGUID(&enc->subtype, &MFVideoFormat_AV1);
    enc->segmented = (pref & 0x200) != 0;
    enc->width = width;
    enc->height = height;
    enc->max_width = engine && engine->config.VideoMaxWidth ? engine->config.VideoMaxWidth : 0;
    enc->max_height = engine && engine->config.VideoMaxHeight ? engine->config.VideoMaxHeight : 0;
    enc->frame_duration = IR_SECOND_100NS / max(fps, 1);

    MFT_REGISTER_TYPE_INFO input_type = { MFMediaType_Video, MFVideoFormat_NV12 };
    MFT_REGISTER_TYPE_INFO output_type = { MFMediaType_Video, enc->subtype };
    // VBR/CBR is set through ICodecAPI below; it does not affect enumeration.
    // GPU mode is an explicit bit now: no compact availability bits are packed,
    // so SORTANDFILTER picks the MFT matching the D3D11 adapter chosen above.
    BOOL hardware = (pref & IR_PREF_GPU_ENCODER) != 0 && codec_bits != 3;
    UINT flags = hardware
                     ? (MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_ASYNCMFT | MFT_ENUM_FLAG_SORTANDFILTER)
                     : (MFT_ENUM_FLAG_SYNCMFT | MFT_ENUM_FLAG_SORTANDFILTER);

    // Restrict hardware MFTs to the adapter owning our D3D11 device. Without
    // this, hybrid systems can pick a transformer attached to another GPU.
    IMFAttributes *enum_attrs = NULL;
    if (hardware && device)
    {
        IDXGIDevice *dxgi = NULL;
        IDXGIAdapter *adapter = NULL;
        if (SUCCEEDED(ID3D11Device_QueryInterface(device, &IID_IDXGIDevice, (void **)&dxgi)) &&
            SUCCEEDED(IDXGIDevice_GetAdapter(dxgi, &adapter)))
        {
            DXGI_ADAPTER_DESC desc;
            if (SUCCEEDED(IDXGIAdapter_GetDesc(adapter, &desc)) &&
                SUCCEEDED(MFCreateAttributes(&enum_attrs, 1)))
            {
                IMFAttributes_SetBlob(enum_attrs, &MFT_ENUM_ADAPTER_LUID,
                                      (UINT8 *)&desc.AdapterLuid, sizeof(desc.AdapterLuid));
            }
        }
        if (adapter) IDXGIAdapter_Release(adapter);
        if (dxgi) IDXGIDevice_Release(dxgi);
    }

    IMFActivate **list = NULL;
    UINT32 count = 0;
    hr = MFTEnum2(MFT_CATEGORY_VIDEO_ENCODER, flags, &input_type, &output_type, enum_attrs, &list, &count);
    if (FAILED(hr) || count == 0)
    {
        // Allow the software encoder as a fallback when no hardware MFT exists.
        IrReleaseActivates(list, count);
        list = NULL;
        count = 0;
        hardware = FALSE;
        flags = MFT_ENUM_FLAG_SYNCMFT | MFT_ENUM_FLAG_SORTANDFILTER;
        hr = MFTEnum2(MFT_CATEGORY_VIDEO_ENCODER, flags, &input_type, &output_type, NULL, &list, &count);
    }
    if (enum_attrs) IMFAttributes_Release(enum_attrs);
    if (FAILED(hr) || count == 0)
    {
        _snprintf_s(error, error_size, _TRUNCATE, "No Media Foundation video encoder is available (0x%08lX)", (unsigned long)hr);
        return FALSE;
    }

    int wanted_vendor = IrWantedVendorOrdinal(pref);
    IMFActivate *chosen = NULL;
    int chosen_vendor = 0;
    for (UINT32 i = 0; i < count; ++i)
    {
        int vendor = IrActivateVendor(list[i]);
        chosen_vendor = vendor;
        if (!wanted_vendor || vendor == wanted_vendor || vendor == 0 || wanted_vendor == 5)
        {
            chosen = list[i];
            list[i] = NULL;
            break;
        }
    }
    if (!chosen)
    {
        chosen = list[0];
        chosen_vendor = IrActivateVendor(chosen);
        list[0] = NULL;
    }
    enc->active_vendor = chosen_vendor ? chosen_vendor : (wanted_vendor ? wanted_vendor : 1);
    IrReleaseActivates(list, count);

    hr = IMFActivate_ActivateObject(chosen, &IID_IMFTransform, (void **)&enc->mft);
    IMFActivate_Release(chosen);
    if (FAILED(hr) || !enc->mft)
    {
        _snprintf_s(error, error_size, _TRUNCATE, "Cannot activate video encoder (0x%08lX)", (unsigned long)hr);
        goto fail;
    }

    IMFAttributes *attributes = NULL;
    if (SUCCEEDED(IMFTransform_GetAttributes(enc->mft, &attributes)) && attributes)
    {
        UINT32 is_async = 0;
        if (SUCCEEDED(IMFAttributes_GetUINT32(attributes, &MF_TRANSFORM_ASYNC, &is_async)) && is_async)
        {
            IMFAttributes_SetUINT32(attributes, &MF_TRANSFORM_ASYNC_UNLOCK, TRUE);
            enc->async = TRUE;
        }
        IMFAttributes_SetUINT32(attributes, &MF_LOW_LATENCY, TRUE);
        IMFAttributes_Release(attributes);
    }
    if (enc->async)
    {
        hr = IMFTransform_QueryInterface(enc->mft, &IID_IMFMediaEventGenerator, (void **)&enc->events);
        if (FAILED(hr) || !enc->events) { enc->async = FALSE; }
    }
    // Hardware transforms need access to the same D3D11 device that produces
    // our frames. The manager stays attached for the lifetime of the MFT.
    if (hardware && device)
    {
        hr = MFCreateDXGIDeviceManager(&enc->manager_token, &enc->manager);
        if (SUCCEEDED(hr) && enc->manager)
            hr = IMFDXGIDeviceManager_ResetDevice(enc->manager, (IUnknown *)device, enc->manager_token);
        if (SUCCEEDED(hr))
            hr = IMFTransform_ProcessMessage(enc->mft, MFT_MESSAGE_SET_D3D_MANAGER,
                                             (ULONG_PTR)enc->manager);
        if (FAILED(hr))
        {
            if (enc->manager) IMFDXGIDeviceManager_Release(enc->manager);
            enc->manager = NULL;
            enc->manager_token = 0;
        }
    }
    IMFTransform_QueryInterface(enc->mft, &IID_ICodecAPI, (void **)&enc->api);

    if (enc->api)
    {
        IrSetCodecU32(enc->api, &CODECAPI_AVEncCommonRateControlMode,
                      (pref & 0x100) ? eAVEncCommonRateControlMode_PeakConstrainedVBR
                                     : eAVEncCommonRateControlMode_CBR);
        IrSetCodecU32(enc->api, &CODECAPI_AVEncCommonMeanBitRate, (uint32_t)bitrate_kbps * 1000);
        IrSetCodecU32(enc->api, &CODECAPI_AVEncCommonMaxBitRate, (uint32_t)bitrate_kbps * 1500);
        IrSetCodecU32(enc->api, &CODECAPI_AVEncMPVGOPSize, (uint32_t)(fps * (enc->segmented ? 1 : 2)));
        IrSetCodecU32(enc->api, &CODECAPI_AVEncMPVDefaultBPictureCount, 0);
        IrSetCodecBool(enc->api, &CODECAPI_AVLowLatencyMode, TRUE);
        IrSetCodecU32(enc->api, &CODECAPI_AVEncCommonQualityVsSpeed,
                      (uint32_t)clamp((pref >> 12) & 7, 0, 6) * 100 / 6);
    }

    BOOL highdef = width >= 1280 || height > 576;
    IMFMediaType *out_type = NULL;
    MFCreateMediaType(&out_type);
    IMFMediaType_SetGUID(out_type, &MF_MT_MAJOR_TYPE, &MFMediaType_Video);
    IMFMediaType_SetGUID(out_type, &MF_MT_SUBTYPE, &enc->subtype);
    IMFMediaType_SetUINT32(out_type, &MF_MT_AVG_BITRATE, bitrate_kbps * 1000);
    IrMFSetSize(out_type, &MF_MT_FRAME_SIZE, width, height);
    IrMFSetSize(out_type, &MF_MT_FRAME_RATE, (UINT32)fps, 1);
    IrMFSetSize(out_type, &MF_MT_PIXEL_ASPECT_RATIO, 1, 1);
    IMFMediaType_SetUINT32(out_type, &MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive);
    IMFMediaType_SetUINT32(out_type, &MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235);
    IMFMediaType_SetUINT32(out_type, &MF_MT_YUV_MATRIX, highdef ? MFVideoTransferMatrix_BT709 : MFVideoTransferMatrix_BT601);
    IMFMediaType_SetUINT32(out_type, &MF_MT_VIDEO_PRIMARIES, highdef ? MFVideoPrimaries_BT709 : MFVideoPrimaries_SMPTE170M);
    IMFMediaType_SetUINT32(out_type, &MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709);
    if (!enc->av1)
        IMFMediaType_SetUINT32(out_type, &MF_MT_MPEG2_PROFILE,
                               enc->hevc ? (UINT32)eAVEncH265VProfile_Main_420_8 : (UINT32)eAVEncH264VProfile_High);
    hr = IMFTransform_SetOutputType(enc->mft, 0, out_type, 0);
    IMFMediaType_Release(out_type);
    if (FAILED(hr))
    {
        _snprintf_s(error, error_size, _TRUNCATE, "Encoder rejected the output format (0x%08lX)", (unsigned long)hr);
        goto fail;
    }

    IMFMediaType *in_type = NULL;
    MFCreateMediaType(&in_type);
    IMFMediaType_SetGUID(in_type, &MF_MT_MAJOR_TYPE, &MFMediaType_Video);
    IMFMediaType_SetGUID(in_type, &MF_MT_SUBTYPE, &MFVideoFormat_NV12);
    IrMFSetSize(in_type, &MF_MT_FRAME_SIZE, width, height);
    IrMFSetSize(in_type, &MF_MT_FRAME_RATE, (UINT32)fps, 1);
    IrMFSetSize(in_type, &MF_MT_PIXEL_ASPECT_RATIO, 1, 1);
    IMFMediaType_SetUINT32(in_type, &MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive);
    IMFMediaType_SetUINT32(in_type, &MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235);
    IMFMediaType_SetUINT32(in_type, &MF_MT_YUV_MATRIX, highdef ? MFVideoTransferMatrix_BT709 : MFVideoTransferMatrix_BT601);
    IMFMediaType_SetUINT32(in_type, &MF_MT_VIDEO_PRIMARIES, highdef ? MFVideoPrimaries_BT709 : MFVideoPrimaries_SMPTE170M);
    IMFMediaType_SetUINT32(in_type, &MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709);
    hr = IMFTransform_SetInputType(enc->mft, 0, in_type, 0);
    IMFMediaType_Release(in_type);
    if (FAILED(hr))
    {
        _snprintf_s(error, error_size, _TRUNCATE, "Encoder rejected NV12 input (0x%08lX)", (unsigned long)hr);
        goto fail;
    }

    MFT_OUTPUT_STREAM_INFO info;
    memset(&info, 0, sizeof(info));
    if (SUCCEEDED(IMFTransform_GetOutputStreamInfo(enc->mft, 0, &info)))
    {
        enc->provides_samples = (info.dwFlags & (MFT_OUTPUT_STREAM_PROVIDES_SAMPLES |
                                                  MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES)) != 0;
        enc->out_size = max(info.cbSize, 1u << 20);
    }
    else enc->out_size = max(width * height, 1u << 20);

    IrLive_ReadSeqHeader(enc);
    IMFTransform_ProcessMessage(enc->mft, MFT_MESSAGE_COMMAND_FLUSH, 0);
    IMFTransform_ProcessMessage(enc->mft, MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0);
    IMFTransform_ProcessMessage(enc->mft, MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0);
    return TRUE;

fail:
    IrLive_Release(enc);
    return FALSE;
}

static void IrComputeOutputSize(uint32_t input_w, uint32_t input_h, uint32_t max_w, uint32_t max_h,
                                uint32_t *out_w, uint32_t *out_h)
{
    *out_w = input_w;
    *out_h = input_h;
    if (max_w && max_h)
    {
        if ((uint64_t)max_w * input_h < (uint64_t)max_h * input_w)
            *out_h = input_h * max_w / input_w, *out_w = max_w;
        else
            *out_w = input_w * max_h / input_h, *out_h = max_h;
    }
    else if (max_w && max_w < input_w)
    {
        *out_w = max_w;
        *out_h = input_h * max_w / input_w;
    }
    else if (max_h && max_h < input_h)
    {
        *out_h = max_h;
        *out_w = input_w * max_h / input_h;
    }
    *out_w = max(*out_w & ~1u, 2u);
    *out_h = max(*out_h & ~1u, 2u);
}

static BOOL IrLive_PrepareGpu(IrLiveEncoder *enc, IrEngine *engine, ID3D11Device *device,
                              uint32_t input_w, uint32_t input_h)
{
    uint32_t output_w = 0, output_h = 0;
    IrComputeOutputSize(input_w, input_h, enc->max_width, enc->max_height, &output_w, &output_h);
    if (enc->mft && (enc->width != output_w || enc->height != output_h))
    {
        // The MFT output type is fixed at creation time. Rebuild it when the
        // capture size or canvas hint changes; the caller forces an IDR.
        IrLive_Release(enc);
        char create_error[192];
        create_error[0] = '\0';
        if (!IrLive_Create(enc, engine, device, output_w, output_h, engine->fps,
                           engine->bitrate_kbps, engine->pref, create_error, sizeof(create_error)))
        {
            IrSetErrorFmt("%s", create_error);
            return FALSE;
        }
    }
    if (enc->gpu_ready && enc->input_width == input_w && enc->input_height == input_h &&
        enc->output_width == output_w && enc->output_height == output_h)
        return TRUE;
    if (enc->gpu_ready)
    {
        IrYuvOutput_Release(&enc->output);
        IrYuv_Release(&enc->yuv);
        IrResize_Release(&enc->resize);
        if (enc->staging) ID3D11Texture2D_Release(enc->staging);
        if (enc->input_view) ID3D11RenderTargetView_Release(enc->input_view);
        enc->staging = NULL;
        enc->input_view = NULL;
        enc->gpu_ready = FALSE;
    }
    enc->input_width = input_w;
    enc->input_height = input_h;
    enc->output_width = output_w;
    enc->output_height = output_h;
    // Remember the real encoded size: IrLive_Release clears enc on stop, but a
    // save issued after IR_Stop still needs the dimensions of the buffered
    // bitstream, not the configured canvas maximum.
    if (engine)
    {
        engine->encoded_width = (int)output_w;
        engine->encoded_height = (int)output_h;
    }

    D3D11_TEXTURE2D_DESC staging_desc;
    memset(&staging_desc, 0, sizeof(staging_desc));
    staging_desc.Width = enc->output_width;
    staging_desc.Height = enc->output_height;
    staging_desc.MipLevels = staging_desc.ArraySize = 1;
    staging_desc.Format = DXGI_FORMAT_NV12;
    staging_desc.SampleDesc.Count = 1;
    staging_desc.Usage = D3D11_USAGE_STAGING;
    staging_desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
    if (FAILED(ID3D11Device_CreateTexture2D(device, &staging_desc, NULL, &enc->staging)))
        return FALSE;

    IrResize_Create(&enc->resize, device, input_w, input_h, enc->output_width, enc->output_height,
                    FALSE, D3D11_BIND_RENDER_TARGET);
    D3D11_RENDER_TARGET_VIEW_DESC view_desc;
    memset(&view_desc, 0, sizeof(view_desc));
    view_desc.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
    view_desc.ViewDimension = D3D11_RTV_DIMENSION_TEXTURE2D;
    ID3D11Device_CreateRenderTargetView(device, (ID3D11Resource *)enc->resize.InputTexture,
                                        &view_desc, &enc->input_view);
    IrYuv_Create(&enc->yuv, device, enc->resize.OutputTexture, enc->output_width, enc->output_height,
                 (enc->output_width >= 1280 || enc->output_height > 576)
                     ? IrYuvColorSpace_BT709 : IrYuvColorSpace_BT601, FALSE);
    IrYuvOutput_Create(&enc->output, device, enc->output_width, enc->output_height, DXGI_FORMAT_NV12);
    free(enc->nv12);
    enc->nv12 = (uint8_t *)malloc((size_t)enc->output_width * enc->output_height * 3 / 2);
    if (!enc->nv12) return FALSE;
    enc->gpu_ready = TRUE;
    return TRUE;
}

// Feed the already-converted NV12 buffer. Used for both new frames and the
// periodic repeats that keep the replay ring populated on a static desktop.
static BOOL IrLive_FeedNv12(IrLiveEncoder *enc, IrEngine *engine, int64_t pts, BOOL idr,
                            char *error, size_t error_size)
{
    if (!enc->mft || !enc->nv12 || !enc->output_width || !enc->output_height)
    {
        _snprintf_s(error, error_size, _TRUNCATE, "Encoder input is not ready");
        return FALSE;
    }
    uint32_t w = enc->output_width, h = enc->output_height;
    IMFMediaBuffer *buffer = NULL;
    IMFSample *sample = NULL;
    BOOL ok = FALSE;
    if (SUCCEEDED(MFCreateMemoryBuffer((DWORD)((size_t)w * h * 3 / 2), &buffer)) && buffer &&
        SUCCEEDED(MFCreateSample(&sample)) && sample)
    {
        BYTE *dst = NULL;
        if (SUCCEEDED(IMFMediaBuffer_Lock(buffer, &dst, NULL, NULL)) && dst)
        {
            memcpy(dst, enc->nv12, (size_t)w * h * 3 / 2);
            IMFMediaBuffer_Unlock(buffer);
            IMFMediaBuffer_SetCurrentLength(buffer, (DWORD)((size_t)w * h * 3 / 2));
            IMFSample_AddBuffer(sample, buffer);
            ok = IrLive_Feed(enc, engine, sample, pts, idr, error, error_size);
        }
        else _snprintf_s(error, error_size, _TRUNCATE, "Cannot lock the encoder input buffer");
    }
    else _snprintf_s(error, error_size, _TRUNCATE, "Cannot allocate the encoder input sample");
    if (sample) IMFSample_Release(sample);
    if (buffer) IMFMediaBuffer_Release(buffer);
    return ok;
}

static BOOL IrLive_EncodeFrame(IrLiveEncoder *enc, IrEngine *engine, ID3D11DeviceContext *context,
                               ID3D11Texture2D *texture, RECT rect, int64_t pts, BOOL idr,
                               char *error, size_t error_size)
{
    if (!enc->mft || !texture || !context) return FALSE;
    LONG width = rect.right - rect.left;
    LONG height = rect.bottom - rect.top;
    if (width < 2 || height < 2) return FALSE;
    int64_t t0 = IrNow100ns();
    if (!IrLive_PrepareGpu(enc, engine, engine->dev, (uint32_t)width, (uint32_t)height))
    {
        _snprintf_s(error, error_size, _TRUNCATE, "Cannot prepare the live video converter");
        return FALSE;
    }
    int64_t t1 = IrNow100ns();

    D3D11_BOX box;
    memset(&box, 0, sizeof(box));
    box.left = (UINT)max(rect.left, 0);
    box.top = (UINT)max(rect.top, 0);
    box.right = (UINT)rect.right;
    box.bottom = (UINT)rect.bottom;
    box.back = 1;
    FLOAT black[4] = { 0, 0, 0, 0 };
    ID3D11DeviceContext_ClearRenderTargetView(context, enc->input_view, black);
    ID3D11DeviceContext_CopySubresourceRegion(context, (ID3D11Resource *)enc->resize.InputTexture,
                                              0, 0, 0, 0, (ID3D11Resource *)texture, 0, &box);
    IrResize_Dispatch(&enc->resize, context);
    IrYuv_Dispatch(&enc->yuv, context, &enc->output);
    ID3D11DeviceContext_CopyResource(context, (ID3D11Resource *)enc->staging,
                                     (ID3D11Resource *)enc->output.Texture);
    ID3D11DeviceContext_Flush(context);
    int64_t t2 = IrNow100ns();

    D3D11_MAPPED_SUBRESOURCE mapped;
    memset(&mapped, 0, sizeof(mapped));
    if (FAILED(ID3D11DeviceContext_Map(context, (ID3D11Resource *)enc->staging, 0,
                                       D3D11_MAP_READ, 0, &mapped)))
    {
        _snprintf_s(error, error_size, _TRUNCATE, "Cannot read back the encoded frame");
        return FALSE;
    }
    const uint8_t *src = (const uint8_t *)mapped.pData;
    uint32_t w = enc->output_width, h = enc->output_height;
    for (uint32_t row = 0; row < h; ++row)
        memcpy(enc->nv12 + (size_t)row * w, src + (size_t)row * mapped.RowPitch, w);
    uint8_t *uv_dst = enc->nv12 + (size_t)w * h;
    const uint8_t *uv_src = (const uint8_t *)mapped.pData + (size_t)(h / 2) * mapped.RowPitch;
    for (uint32_t row = 0; row < h / 2; ++row)
        memcpy(uv_dst + (size_t)row * w, uv_src + (size_t)row * mapped.RowPitch, w);
    int64_t t3 = IrNow100ns();
    ID3D11DeviceContext_Unmap(context, (ID3D11Resource *)enc->staging, 0);
    int64_t t4 = IrNow100ns();

    int64_t t4b = IrNow100ns();
    BOOL ok = IrLive_FeedNv12(enc, engine, pts, idr, error, error_size);
    int64_t t5 = IrNow100ns();

    engine->timing_render_ms += IrMsDiff(t1, t2);
    engine->timing_submit_ms += IrMsDiff(t4b, t5);
    engine->timing_map_ms += IrMsDiff(t2, t3);
    engine->timing_unmap_ms += IrMsDiff(t3, t4);
    engine->timing_encode_ms += IrMsDiff(t4, t5);
    engine->timing_snapshot_ms += IrMsDiff(t0, t5);
    InterlockedIncrement64(&engine->timing_frames);
    return ok;
}

static BOOL IrLive_FlushEncoder(IrLiveEncoder *enc, IrEngine *engine)
{
    if (!enc->mft) return TRUE;
    enc->drained = FALSE;
    IMFTransform_ProcessMessage(enc->mft, MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
    IMFTransform_ProcessMessage(enc->mft, MFT_MESSAGE_COMMAND_DRAIN, 0);
    char error[192];
    if (enc->async)
    {
        uint64_t deadline = GetTickCount64() + 2000;
        while (!enc->drained && GetTickCount64() < deadline)
        {
            if (!IrLive_Pump(enc, engine, FALSE, error, sizeof(error))) break;
            Sleep(2);
        }
    }
    else
    {
        IrLive_Pump(enc, engine, FALSE, error, sizeof(error));
    }
    return TRUE;
}

// ---------------------------------------------------------------------------
// Compressed remux writer. The SinkWriter receives the already encoded video
// packets; only the audio stream is encoded from a single continuous PCM input.
// ---------------------------------------------------------------------------
typedef struct
{
    IMFSinkWriter *writer;
    DWORD video_stream;
    DWORD audio_stream;
    BOOL has_audio;
} IrMux;

static BOOL IrMux_Create(IrMux *mux, const wchar_t *path, const Config *config, BOOL want_audio,
                         const GUID *subtype, const uint8_t *sequence_header,
                         uint32_t sequence_size, uint32_t width, uint32_t height, int fps)
{
    memset(mux, 0, sizeof(*mux));
    mux->video_stream = mux->audio_stream = (DWORD)-1;

    IMFAttributes *attributes = NULL;
    if (FAILED(MFCreateAttributes(&attributes, 4))) return FALSE;
    IMFAttributes_SetUINT32(attributes, &MF_SINK_WRITER_DISABLE_THROTTLING, TRUE);
    IMFAttributes_SetGUID(attributes, &MF_TRANSCODE_CONTAINERTYPE,
                          config->FragmentedOutput ? &MFTranscodeContainerType_FMPEG4
                                                   : &MFTranscodeContainerType_MPEG4);
    HRESULT hr = MFCreateSinkWriterFromURL(path, NULL, attributes, &mux->writer);
    IMFAttributes_Release(attributes);
    if (FAILED(hr) || !mux->writer) return FALSE;

    IMFMediaType *video = NULL;
    MFCreateMediaType(&video);
    IMFMediaType_SetGUID(video, &MF_MT_MAJOR_TYPE, &MFMediaType_Video);
    IMFMediaType_SetGUID(video, &MF_MT_SUBTYPE, subtype);
    IrMFSetSize(video, &MF_MT_FRAME_SIZE, width, height);
    IrMFSetSize(video, &MF_MT_FRAME_RATE, (UINT32)max(fps, 1), 1);
    IrMFSetSize(video, &MF_MT_PIXEL_ASPECT_RATIO, 1, 1);
    IMFMediaType_SetUINT32(video, &MF_MT_AVG_BITRATE, config->VideoBitrate * 1000);
    IMFMediaType_SetUINT32(video, &MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive);
    if (IsEqualGUID(subtype, &MFVideoFormat_H264))
        IMFMediaType_SetUINT32(video, &MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_High);
    else if (IsEqualGUID(subtype, &MFVideoFormat_HEVC))
        IMFMediaType_SetUINT32(video, &MF_MT_MPEG2_PROFILE, eAVEncH265VProfile_Main_420_8);
    if (sequence_header && sequence_size)
        IMFMediaType_SetBlob(video, &MF_MT_MPEG_SEQUENCE_HEADER, sequence_header, sequence_size);
    hr = IMFSinkWriter_AddStream(mux->writer, video, &mux->video_stream);
    if (SUCCEEDED(hr)) hr = IMFSinkWriter_SetInputMediaType(mux->writer, mux->video_stream, video, NULL);
    IMFMediaType_Release(video);
    if (FAILED(hr)) goto fail;

    if (want_audio)
    {
        IMFMediaType *output = NULL;
        MFCreateMediaType(&output);
        IMFMediaType_SetGUID(output, &MF_MT_MAJOR_TYPE, &MFMediaType_Audio);
        IMFMediaType_SetGUID(output, &MF_MT_SUBTYPE, config->AudioCodec == IR_AUDIO_FLAC
                                                         ? &MFAudioFormat_FLAC : &MFAudioFormat_AAC);
        IMFMediaType_SetUINT32(output, &MF_MT_AUDIO_BITS_PER_SAMPLE, 16);
        IMFMediaType_SetUINT32(output, &MF_MT_AUDIO_SAMPLES_PER_SECOND, config->AudioSamplerate);
        IMFMediaType_SetUINT32(output, &MF_MT_AUDIO_NUM_CHANNELS, config->AudioChannels);
        if (config->AudioCodec == IR_AUDIO_AAC)
        {
            IMFMediaType_SetUINT32(output, &MF_MT_AUDIO_AVG_BYTES_PER_SECOND, config->AudioBitrate * 1000 / 8);
            IMFMediaType_SetUINT32(output, &MF_MT_AAC_PAYLOAD_TYPE, 0);
        }
        hr = IMFSinkWriter_AddStream(mux->writer, output, &mux->audio_stream);
        IMFMediaType_Release(output);
        if (FAILED(hr)) goto fail;

        IMFMediaType *input = NULL;
        MFCreateMediaType(&input);
        IMFMediaType_SetGUID(input, &MF_MT_MAJOR_TYPE, &MFMediaType_Audio);
        IMFMediaType_SetGUID(input, &MF_MT_SUBTYPE, &MFAudioFormat_PCM);
        IMFMediaType_SetUINT32(input, &MF_MT_AUDIO_BITS_PER_SAMPLE, 16);
        IMFMediaType_SetUINT32(input, &MF_MT_AUDIO_SAMPLES_PER_SECOND, config->AudioSamplerate);
        IMFMediaType_SetUINT32(input, &MF_MT_AUDIO_NUM_CHANNELS, config->AudioChannels);
        IMFMediaType_SetUINT32(input, &MF_MT_AUDIO_BLOCK_ALIGNMENT, config->AudioChannels * 2);
        IMFMediaType_SetUINT32(input, &MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
                               config->AudioSamplerate * config->AudioChannels * 2);
        IMFMediaType_SetUINT32(input, &MF_MT_ALL_SAMPLES_INDEPENDENT, TRUE);
        hr = IMFSinkWriter_SetInputMediaType(mux->writer, mux->audio_stream, input, NULL);
        IMFMediaType_Release(input);
        if (FAILED(hr)) goto fail;
        mux->has_audio = TRUE;
    }

    hr = IMFSinkWriter_BeginWriting(mux->writer);
    if (FAILED(hr)) goto fail;
    return TRUE;

fail:
    if (mux->writer) { IMFSinkWriter_Release(mux->writer); mux->writer = NULL; }
    DeleteFileW(path);
    return FALSE;
}

static IMFSample *IrMux_MemorySample(const void *data, uint32_t bytes, int64_t pts, int64_t duration)
{
    IMFMediaBuffer *buffer = NULL;
    IMFSample *sample = NULL;
    if (FAILED(MFCreateMemoryBuffer(bytes, &buffer)) || !buffer) return NULL;
    BYTE *dst = NULL;
    if (FAILED(IMFMediaBuffer_Lock(buffer, &dst, NULL, NULL)))
    {
        IMFMediaBuffer_Release(buffer);
        return NULL;
    }
    memcpy(dst, data, bytes);
    IMFMediaBuffer_Unlock(buffer);
    IMFMediaBuffer_SetCurrentLength(buffer, bytes);
    if (FAILED(MFCreateSample(&sample)) || !sample)
    {
        IMFMediaBuffer_Release(buffer);
        return NULL;
    }
    IMFSample_AddBuffer(sample, buffer);
    IMFMediaBuffer_Release(buffer);
    IMFSample_SetSampleTime(sample, pts);
    IMFSample_SetSampleDuration(sample, duration);
    return sample;
}

static BOOL IrMux_WriteVideo(IrMux *mux, const IrPacket *packet, int64_t pts, int64_t duration)
{
    if (!mux->writer || packet->bytes <= packet->codec_data) return TRUE;
    IMFSample *sample = IrMux_MemorySample(packet->data + packet->codec_data,
                                           packet->bytes - packet->codec_data, pts, duration);
    if (!sample) return FALSE;
    if (packet->key) IMFSample_SetUINT32(sample, &MFSampleExtension_CleanPoint, TRUE);
    HRESULT hr = IMFSinkWriter_WriteSample(mux->writer, mux->video_stream, sample);
    IMFSample_Release(sample);
    return SUCCEEDED(hr);
}

static BOOL IrMux_WriteAudio(IrMux *mux, const int16_t *pcm, uint32_t frames, int64_t duration)
{
    if (!mux->writer || !mux->has_audio || !pcm || !frames) return TRUE;
    IMFSample *sample = IrMux_MemorySample(pcm, frames * IR_AUDIO_CHANNELS * sizeof(int16_t), 0, duration);
    if (!sample) return FALSE;
    HRESULT hr = IMFSinkWriter_WriteSample(mux->writer, mux->audio_stream, sample);
    IMFSample_Release(sample);
    return SUCCEEDED(hr);
}

static BOOL IrMux_Finalize(IrMux *mux, const wchar_t *path)
{
    if (!mux->writer) return TRUE;
    HRESULT hr = IMFSinkWriter_Finalize(mux->writer);
    IMFSinkWriter_Release(mux->writer);
    mux->writer = NULL;
    if (FAILED(hr)) { DeleteFileW(path); return FALSE; }
    return TRUE;
}




//
// interface
//

typedef struct
{
	DWORD InputWidth;   // width to what input will be cropped
	DWORD InputHeight;  // height to what input will be cropped
	DWORD OutputWidth;  // width of video output
	DWORD OutputHeight; // height of video output
	DWORD FramerateNum; // video output framerate numerator
	DWORD FramerateDen; // video output framerate denumerator
	UINT64 StartTime;   // time in QPC ticks since first call of NewFrame

	IMFAsyncCallback VideoSampleCallback;
	IMFAsyncCallback AudioSampleCallback;
	ID3D11DeviceContext* Context;
	ID3D11Multithread* Multithread;
	IMFSinkWriter* Writer;
	int VideoStreamIndex;
	int AudioStreamIndex;

	ID3D11RenderTargetView* InputView;

	IrResize Resize;
	IrYuv Convert;

	IrYuvOutput  ConvertOutput[IR_ENCODER_VIDEO_BUFFER_COUNT];
	IMFSample*        VideoSample[IR_ENCODER_VIDEO_BUFFER_COUNT];
	_Atomic(uint64_t) VideoSampleAvailable;

	BOOL   VideoDiscontinuity;
	UINT64 VideoLastTime;

	IMFTransform*     Resampler;
	IMFSample*        AudioSample[IR_ENCODER_AUDIO_BUFFER_COUNT];
	_Atomic(uint64_t) AudioSampleAvailable;

	IMFSample*      AudioInputSample;
	DWORD           AudioFrameSize;
	DWORD           AudioSampleRate;
}
IrEncoder;

typedef struct
{
	DWORD Width;
	DWORD Height;
	DWORD FramerateNum;
	DWORD FramerateDen;
	WAVEFORMATEX* AudioFormat;
	Config* Config;
}
IrEncoderConfig;

static void IrEncoder_Init(IrEncoder* IrEncoder);
static BOOL IrEncoder_Start(IrEncoder* IrEncoder, ID3D11Device* Device, LPWSTR FileName, const IrEncoderConfig* Config);
static void IrEncoder_Stop(IrEncoder* IrEncoder);

static BOOL IrEncoder_NewFrame(IrEncoder* IrEncoder, ID3D11Texture2D* Texture, RECT Rect, UINT64 Time, UINT64 TimePeriod);
static void IrEncoder_NewSamples(IrEncoder* IrEncoder, LPCVOID Samples, DWORD FrameCount, UINT64 Time, UINT64 TimePeriod);
static void IrEncoder_Update(IrEncoder* IrEncoder, UINT64 Time, UINT64 TimePeriod);
static void IrEncoder_GetStats(IrEncoder* IrEncoder, DWORD* Bitrate, DWORD* LengthMsec, UINT64* FileSize);

//
static HRESULT STDMETHODCALLTYPE IrEncoder__QueryInterface(IMFAsyncCallback* this, REFIID riid, void** Object)
{
	if (Object == NULL)
	{
		return E_POINTER;
	}
	if (IsEqualGUID(&IID_IUnknown, riid) || IsEqualGUID(&IID_IMFAsyncCallback, riid))
	{
		*Object = this;
		return S_OK;
	}
	return E_NOINTERFACE;
}

static ULONG STDMETHODCALLTYPE IrEncoder__AddRef(IMFAsyncCallback* this)
{
	return 1;
}

static ULONG STDMETHODCALLTYPE IrEncoder__Release(IMFAsyncCallback* this)
{
	return 1;
}

static HRESULT STDMETHODCALLTYPE IrEncoder__GetParameters(IMFAsyncCallback* this, DWORD* Flags, DWORD* Queue)
{
	*Flags = 0;
	*Queue = MFASYNC_CALLBACK_QUEUE_MULTITHREADED;
	return S_OK;
}

static HRESULT STDMETHODCALLTYPE IrEncoder__VideoInvoke(IMFAsyncCallback* this, IMFAsyncResult* Result)
{
	IrEncoder* Enc = CONTAINING_RECORD(this, IrEncoder, VideoSampleCallback);

	IUnknown* Object;
	IMFSample* Sample;

	HR(IMFAsyncResult_GetObject(Result, &Object));
	HR(IUnknown_QueryInterface(Object, &IID_IMFSample, (LPVOID*)&Sample));
	IUnknown_Release(Object);
	// keep Sample object reference count incremented to reuse for new frame submission

	for (size_t Index = 0; Index < ARRAYSIZE(Enc->VideoSample); Index++)
	{
		if (Sample == Enc->VideoSample[Index])
		{
			atomic_fetch_or(&Enc->VideoSampleAvailable, 1ULL << Index);
			WakeByAddressSingle((PVOID)&Enc->VideoSampleAvailable);
			break;
		}
	}

	return S_OK;
}

static HRESULT STDMETHODCALLTYPE IrEncoder__AudioInvoke(IMFAsyncCallback* this, IMFAsyncResult* Result)
{
	IrEncoder* Enc = CONTAINING_RECORD(this, IrEncoder, AudioSampleCallback);

	IUnknown* Object;
	IMFSample* Sample;

	HR(IMFAsyncResult_GetObject(Result, &Object));
	HR(IUnknown_QueryInterface(Object, &IID_IMFSample, (LPVOID*)&Sample));
	IUnknown_Release(Object);
	// keep Sample object reference count incremented to reuse for new sample submission

	for (size_t Index = 0; Index < ARRAYSIZE(Enc->AudioSample); Index++)
	{
		if (Sample == Enc->AudioSample[Index])
		{
			atomic_fetch_or(&Enc->AudioSampleAvailable, 1ULL << Index);
			WakeByAddressSingle((PVOID)&Enc->AudioSampleAvailable);
			break;
		}
	}

	return S_OK;
}

static IMFAsyncCallbackVtbl IrEncoder__VideoSampleCallbackVtbl =
{
	.QueryInterface = &IrEncoder__QueryInterface,
	.AddRef         = &IrEncoder__AddRef,
	.Release        = &IrEncoder__Release,
	.GetParameters  = &IrEncoder__GetParameters,
	.Invoke         = &IrEncoder__VideoInvoke,
};

static IMFAsyncCallbackVtbl IrEncoder__AudioSampleCallbackVtbl =
{
	.QueryInterface = &IrEncoder__QueryInterface,
	.AddRef         = &IrEncoder__AddRef,
	.Release        = &IrEncoder__Release,
	.GetParameters  = &IrEncoder__GetParameters,
	.Invoke         = &IrEncoder__AudioInvoke,
};

static void IrEncoder__OutputAudioSamples(IrEncoder* IrEncoder)
{
	for (;;)
	{
		// we don't want to drop any audio frames, so wait for available sample/buffer
		uint64_t Available = atomic_load(&IrEncoder->AudioSampleAvailable);
		while (Available == 0)
		{
			uint64_t Zero = 0;
			WaitOnAddress((PVOID)&IrEncoder->AudioSampleAvailable, &Zero, sizeof(Zero), INFINITE);
			Available = atomic_load(&IrEncoder->AudioSampleAvailable);
		}

		DWORD Index;
		_BitScanForward64(&Index, Available);

		IMFSample* Sample = IrEncoder->AudioSample[Index];

		DWORD Status;
		MFT_OUTPUT_DATA_BUFFER Output = { .dwStreamID = 0, .pSample = Sample };
		HRESULT hr = IMFTransform_ProcessOutput(IrEncoder->Resampler, 0, 1, &Output, &Status);
		if (hr == MF_E_TRANSFORM_NEED_MORE_INPUT)
		{
			// no output is available
			break;
		}
		Assert(SUCCEEDED(hr));

		atomic_fetch_and(&IrEncoder->AudioSampleAvailable, ~(1ULL << Index));

		IMFTrackedSample* Tracked;
		HR(IMFSample_QueryInterface(Sample, &IID_IMFTrackedSample, (LPVOID*)&Tracked));
		HR(IMFTrackedSample_SetAllocator(Tracked, &IrEncoder->AudioSampleCallback, (IUnknown*)Tracked));

		HR(IMFSinkWriter_WriteSample(IrEncoder->Writer, IrEncoder->AudioStreamIndex, Sample));

		IMFSample_Release(Sample);
		IMFTrackedSample_Release(Tracked);
	}
}

void IrEncoder_Init(IrEncoder* IrEncoder)
{
	HR(MFStartup(MF_VERSION, MFSTARTUP_LITE));
	IrEncoder->VideoSampleCallback.lpVtbl = &IrEncoder__VideoSampleCallbackVtbl;
	IrEncoder->AudioSampleCallback.lpVtbl = &IrEncoder__AudioSampleCallbackVtbl;
}

BOOL IrEncoder_Start(IrEncoder* IrEncoder, ID3D11Device* Device, LPWSTR FileName, const IrEncoderConfig* Config)
{
	ID3D11DeviceContext* Context;
	ID3D11Device_GetImmediateContext(Device, &Context);

	ID3D11Multithread* Multithread;

	// it is very unclear if this is needed or not, it used to be that D3D11 debug runtime
	// was complaining if this is not done, but nowadays it doesn't complain anymore? very confusing
	HR(ID3D11DeviceContext_QueryInterface(Context, &IID_ID3D11Multithread, &Multithread));
	ID3D11Multithread_SetMultithreadProtected(Multithread, TRUE);

	DWORD InputWidth = Config->Width;
	DWORD InputHeight = Config->Height;
	DWORD OutputWidth = Config->Config->VideoMaxWidth;
	DWORD OutputHeight = Config->Config->VideoMaxHeight;

	if (OutputWidth != 0 && OutputHeight == 0)
	{
		// limit max width
		if (OutputWidth < InputWidth)
		{
			OutputHeight = InputHeight * OutputWidth / InputWidth;
		}
		else
		{
			OutputWidth = InputWidth;
			OutputHeight = InputHeight;
		}
	}
	else if (OutputWidth == 0 && OutputHeight != 0)
	{
		// limit max height
		if (OutputHeight < InputHeight)
		{
			OutputWidth = InputWidth * OutputHeight / InputHeight;
		}
		else
		{
			OutputWidth = InputWidth;
			OutputHeight = InputHeight;
		}
	}
	else if (OutputWidth != 0 && OutputHeight != 0)
	{
		// limit max width and max height
		if (OutputWidth * InputHeight < OutputHeight * InputWidth)
		{
			if (OutputWidth < InputWidth)
			{
				OutputHeight = InputHeight * OutputWidth / InputWidth;
			}
			else
			{
				OutputWidth = InputWidth;
				OutputHeight = InputHeight;
			}
		}
		else
		{
			if (OutputHeight < InputHeight)
			{
				OutputWidth = InputWidth * OutputHeight / InputHeight;
			}
			else
			{
				OutputWidth = InputWidth;
				OutputHeight = InputHeight;
			}
		}
	}
	else
	{
		// use original size
		OutputWidth = InputWidth;
		OutputHeight = InputHeight;
	}

	// must be multiple of 2, round upwards
	InputWidth = (InputWidth + 1) & ~1;
	InputHeight = (InputHeight + 1) & ~1;
	OutputWidth = (OutputWidth + 1) & ~1;
	OutputHeight = (OutputHeight + 1) & ~1;

	BOOL Result = FALSE;
	IMFSinkWriter* Writer = NULL;
	IMFTransform* Resampler = NULL;
	HRESULT hr;

	IrEncoder->VideoStreamIndex = -1;
	IrEncoder->AudioStreamIndex = -1;

	const GUID* Container;
	const GUID* Codec;
	UINT32 Profile;
	const GUID* VideoInputFormat;
	if (Config->Config->VideoCodec == IR_VIDEO_H264)
	{
		VideoInputFormat = &MFVideoFormat_NV12;
		Container = Config->Config->FragmentedOutput ? &MFTranscodeContainerType_FMPEG4 : &MFTranscodeContainerType_MPEG4;
		Codec = &MFVideoFormat_H264;
		Profile = ((UINT32[]) { eAVEncH264VProfile_Base, eAVEncH264VProfile_Main, eAVEncH264VProfile_High })[Config->Config->VideoProfile];
	}
	else if (Config->Config->VideoCodec == IR_VIDEO_H265 && Config->Config->VideoProfile == IR_VIDEO_MAIN)
	{
		VideoInputFormat = &MFVideoFormat_NV12;
		Container = &MFTranscodeContainerType_MPEG4;
		Codec = &MFVideoFormat_HEVC;
		Profile = eAVEncH265VProfile_Main_420_8;
	}
	else if (Config->Config->VideoCodec == IR_VIDEO_H265 && Config->Config->VideoProfile == IR_VIDEO_MAIN_10)
	{
		VideoInputFormat = &MFVideoFormat_P010;
		Container = &MFTranscodeContainerType_MPEG4;
		Codec = &MFVideoFormat_HEVC;
		Profile = eAVEncH265VProfile_Main_420_10;
	}
	else if (Config->Config->VideoCodec == IR_VIDEO_AV1 && Config->Config->VideoProfile == IR_VIDEO_MAIN)
	{
		VideoInputFormat = &MFVideoFormat_NV12;
		Container = &MFTranscodeContainerType_MPEG4;
		Codec = &MFVideoFormat_AV1;
		Profile = eAVEncAV1VProfile_Main_420_8;
	}
	else if (Config->Config->VideoCodec == IR_VIDEO_AV1 && Config->Config->VideoProfile == IR_VIDEO_MAIN_10)
	{
		VideoInputFormat = &MFVideoFormat_P010;
		Container = &MFTranscodeContainerType_MPEG4;
		Codec = &MFVideoFormat_AV1;
		Profile = eAVEncAV1VProfile_Main_420_10;
	}
	else
	{
		Assert(0);
	}

	// make sure MFT video encoder exists, some vendors wrongly allow SinkWriter to be created for invalid configuration
	{
		bool Ok = false;

		MFT_REGISTER_TYPE_INFO InputType = { MFMediaType_Video, *VideoInputFormat };
		MFT_REGISTER_TYPE_INFO OutputType = { MFMediaType_Video, *Codec };

		IMFAttributes* EnumAttributes;
		HR(MFCreateAttributes(&EnumAttributes, 1));

		UINT32 Flags = MFT_ENUM_FLAG_SORTANDFILTER;

		if (Config->Config->HardwareEncoder)
		{
			IDXGIDevice* DxgiDevice;
			HR(ID3D11Device_QueryInterface(Device, &IID_IDXGIDevice, (void**)&DxgiDevice));

			IDXGIAdapter* DxgiAdapter;
			HR(IDXGIDevice_GetAdapter(DxgiDevice, &DxgiAdapter));
			IDXGIDevice_Release(DxgiDevice);

			DXGI_ADAPTER_DESC AdapterDesc;
			IDXGIAdapter_GetDesc(DxgiAdapter, &AdapterDesc);
			IDXGIAdapter_Release(DxgiAdapter);

			HR(IMFAttributes_SetBlob(EnumAttributes, &MFT_ENUM_ADAPTER_LUID, (UINT8*)&AdapterDesc.AdapterLuid, sizeof(AdapterDesc.AdapterLuid)));

			Flags |= MFT_ENUM_FLAG_ASYNCMFT | MFT_ENUM_FLAG_HARDWARE;
		}
		else
		{
			Flags |= MFT_ENUM_FLAG_SYNCMFT;
		}

		UINT32 ActivateCount = 0;
		IMFActivate** Activate = NULL;
		if (SUCCEEDED(MFTEnum2(MFT_CATEGORY_VIDEO_ENCODER, Flags, &InputType, &OutputType, EnumAttributes, &Activate, &ActivateCount)) && ActivateCount != 0)
		{
			Ok = true;
		}

		if (Activate)
		{
			for (size_t ActivateIndex = 0; ActivateIndex != ActivateCount; ActivateIndex++)
			{
				IMFActivate_Release(Activate[ActivateIndex]);
			}
			CoTaskMemFree(Activate);
		}
		IMFAttributes_Release(EnumAttributes);

		if (!Ok)
		{
			IrSetErrorWide(L"Cannot find video encoder!"); goto bail;
			goto bail;
		}
	}

	// https://github.com/mpv-player/mpv/blob/release/0.38/video/csputils.c#L150-L153
	bool IsHD = (OutputWidth >= 1280) || (OutputHeight > 576)
		|| Config->Config->VideoCodec == IR_VIDEO_H265
		|| Config->Config->VideoCodec == IR_VIDEO_AV1;

	// output file
	{
		IMFAttributes* Attributes;
		HR(MFCreateAttributes(&Attributes, 4));
		if (Config->Config->HardwareEncoder)
		{
			HR(IMFAttributes_SetUINT32(Attributes, &MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, TRUE));

			UINT Token;
			IMFDXGIDeviceManager* Manager;
			HR(MFCreateDXGIDeviceManager(&Token, &Manager));
			HR(IMFDXGIDeviceManager_ResetDevice(Manager, (IUnknown*)Device, Token));
			HR(IMFAttributes_SetUnknown(Attributes, &MF_SINK_WRITER_D3D_MANAGER, (IUnknown*)Manager));

			IMFDXGIDeviceManager_Release(Manager);
		}
		HR(IMFAttributes_SetUINT32(Attributes, &MF_SINK_WRITER_DISABLE_THROTTLING, TRUE));
		HR(IMFAttributes_SetGUID(Attributes, &MF_TRANSCODE_CONTAINERTYPE, Container));

		hr = MFCreateSinkWriterFromURL(FileName, NULL, Attributes, &Writer);
		IMFAttributes_Release(Attributes);

		if (FAILED(hr))
		{
			IrSetErrorWide(L"Cannot create output mp4 file!"); goto bail;
			goto bail;
		}
	}

	// video output type
	{
		IMFMediaType* Type;
		HR(MFCreateMediaType(&Type));

		HR(IMFMediaType_SetGUID(Type, &MF_MT_MAJOR_TYPE, &MFMediaType_Video));
		HR(IMFMediaType_SetGUID(Type, &MF_MT_SUBTYPE, Codec));
		HR(IMFMediaType_SetUINT32(Type, &MF_MT_MPEG2_PROFILE, Profile));
		HR(IMFMediaType_SetUINT32(Type, &MF_MT_VIDEO_CHROMA_SITING, MFVideoChromaSubsampling_MPEG2));
		HR(IMFMediaType_SetUINT32(Type, &MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_Wide));
		HR(IMFMediaType_SetUINT32(Type, &MF_MT_VIDEO_PRIMARIES, IsHD ? MFVideoPrimaries_BT709 : MFVideoPrimaries_SMPTE170M));
		HR(IMFMediaType_SetUINT32(Type, &MF_MT_YUV_MATRIX, IsHD ? MFVideoTransferMatrix_BT709 : MFVideoTransferMatrix_BT601));
		HR(IMFMediaType_SetUINT32(Type, &MF_MT_TRANSFER_FUNCTION, MFVideoPrimaries_BT709));
		HR(IMFMediaType_SetUINT32(Type, &MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive));
		HR(IMFMediaType_SetUINT64(Type, &MF_MT_FRAME_RATE, MFT64(Config->FramerateNum, Config->FramerateDen)));
		HR(IMFMediaType_SetUINT64(Type, &MF_MT_FRAME_SIZE, MFT64(OutputWidth, OutputHeight)));
		HR(IMFMediaType_SetUINT32(Type, &MF_MT_AVG_BITRATE, Config->Config->VideoBitrate * 1000));

		hr = IMFSinkWriter_AddStream(Writer, Type, &IrEncoder->VideoStreamIndex);
		IMFMediaType_Release(Type);

		if (FAILED(hr))
		{
			IrSetErrorWide(L"Cannot configure video encoder!"); goto bail;
			goto bail;
		}
	}

	// video input type, NV12 or P010 format
	{
		IMFMediaType* Type;
		HR(MFCreateMediaType(&Type));
		HR(IMFMediaType_SetGUID(Type, &MF_MT_MAJOR_TYPE, &MFMediaType_Video));
		HR(IMFMediaType_SetGUID(Type, &MF_MT_SUBTYPE, VideoInputFormat));
		HR(IMFMediaType_SetUINT32(Type, &MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive));
		HR(IMFMediaType_SetUINT64(Type, &MF_MT_FRAME_RATE, MFT64(Config->FramerateNum, Config->FramerateDen)));
		HR(IMFMediaType_SetUINT64(Type, &MF_MT_FRAME_SIZE, MFT64(OutputWidth, OutputHeight)));

		hr = IMFSinkWriter_SetInputMediaType(Writer, IrEncoder->VideoStreamIndex, Type, NULL);
		IMFMediaType_Release(Type);

		if (FAILED(hr))
		{
			IrSetErrorWide(L"Cannot configure video encoder input!"); goto bail;
			goto bail;
		}
	}

	// video encoder parameters
	{
		ICodecAPI* Codec;
		HR(IMFSinkWriter_GetServiceForStream(Writer, 0, &GUID_NULL, &IID_ICodecAPI, (LPVOID*)&Codec));

		// VBR rate control
		VARIANT RateControl = { .vt = VT_UI4, .ulVal = eAVEncCommonRateControlMode_UnconstrainedVBR };
		ICodecAPI_SetValue(Codec, &CODECAPI_AVEncCommonRateControlMode, &RateControl);

		// VBR bitrate to use, some MFT encoders override MF_MT_AVG_BITRATE setting with this one
		VARIANT Bitrate = { .vt = VT_UI4, .ulVal = Config->Config->VideoBitrate * 1000 };
		ICodecAPI_SetValue(Codec, &CODECAPI_AVEncCommonMeanBitRate, &Bitrate);

		// set GOP size to 4 seconds
		VARIANT GopSize = { .vt = VT_UI4, .ulVal = MUL_DIV_ROUND_UP(4, Config->FramerateNum, Config->FramerateDen) };
		ICodecAPI_SetValue(Codec, &CODECAPI_AVEncMPVGOPSize, &GopSize);

		// disable low latency, for higher quality & better performance
		VARIANT LowLatency = { .vt = VT_BOOL, .boolVal = VARIANT_FALSE };
		ICodecAPI_SetValue(Codec, &CODECAPI_AVLowLatencyMode, &LowLatency);

		// enable 2 B-frames, for better compression
		VARIANT Bframes = { .vt = VT_UI4, .ulVal = 2 };
		ICodecAPI_SetValue(Codec, &CODECAPI_AVEncMPVDefaultBPictureCount, &Bframes);

		ICodecAPI_Release(Codec);
	}

	if (Config->AudioFormat)
	{
		HR(CoCreateInstance(&CLSID_CResamplerMediaObject, NULL, CLSCTX_INPROC_SERVER, &IID_IMFTransform, (LPVOID*)&Resampler));

		// audio resampler input
		{
			IMFMediaType* Type;
			HR(MFCreateMediaType(&Type));
			HR(MFInitMediaTypeFromWaveFormatEx(Type, Config->AudioFormat, sizeof(*Config->AudioFormat) + Config->AudioFormat->cbSize));
			HR(IMFTransform_SetInputType(Resampler, 0, Type, 0));
			IMFMediaType_Release(Type);
		}

		// audio resampler output
		{
			WAVEFORMATEX Format =
			{
				.wFormatTag = WAVE_FORMAT_PCM,
				.nChannels = (WORD)Config->Config->AudioChannels,
				.nSamplesPerSec = Config->Config->AudioSamplerate,
				.wBitsPerSample = sizeof(short) * 8,
			};
			Format.nBlockAlign = Format.nChannels * Format.wBitsPerSample / 8;
			Format.nAvgBytesPerSec = Format.nSamplesPerSec * Format.nBlockAlign;

			IMFMediaType* Type;
			HR(MFCreateMediaType(&Type));
			HR(MFInitMediaTypeFromWaveFormatEx(Type, &Format, sizeof(Format)));
			HR(IMFTransform_SetOutputType(Resampler, 0, Type, 0));
			IMFMediaType_Release(Type);
		}

		HR(IMFTransform_ProcessMessage(Resampler, MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0));

		// audio output type
		{
			const GUID* Codec = &((GUID[]){ MFAudioFormat_AAC, MFAudioFormat_FLAC })[Config->Config->AudioCodec];

			IMFMediaType* Type;
			HR(MFCreateMediaType(&Type));
			HR(IMFMediaType_SetGUID(Type, &MF_MT_MAJOR_TYPE, &MFMediaType_Audio));
			HR(IMFMediaType_SetGUID(Type, &MF_MT_SUBTYPE, Codec));
			HR(IMFMediaType_SetUINT32(Type, &MF_MT_AUDIO_BITS_PER_SAMPLE, 16));
			HR(IMFMediaType_SetUINT32(Type, &MF_MT_AUDIO_SAMPLES_PER_SECOND, Config->Config->AudioSamplerate));
			HR(IMFMediaType_SetUINT32(Type, &MF_MT_AUDIO_NUM_CHANNELS, Config->Config->AudioChannels));
			if (Config->Config->AudioCodec == IR_AUDIO_AAC)
			{
				HR(IMFMediaType_SetUINT32(Type, &MF_MT_AUDIO_AVG_BYTES_PER_SECOND, Config->Config->AudioBitrate * 1000 / 8));
			}

			hr = IMFSinkWriter_AddStream(Writer, Type, &IrEncoder->AudioStreamIndex);
			IMFMediaType_Release(Type);

			if (FAILED(hr))
			{
				IrSetErrorWide(L"Cannot configure audio encoder output!"); goto bail;
				goto bail;
			}
		}

		// audio input type
		{
			IMFMediaType* Type;
			HR(MFCreateMediaType(&Type));
			HR(IMFMediaType_SetGUID(Type, &MF_MT_MAJOR_TYPE, &MFMediaType_Audio));
			HR(IMFMediaType_SetGUID(Type, &MF_MT_SUBTYPE, &MFAudioFormat_PCM));
			HR(IMFMediaType_SetUINT32(Type, &MF_MT_AUDIO_BITS_PER_SAMPLE, 16));
			HR(IMFMediaType_SetUINT32(Type, &MF_MT_AUDIO_SAMPLES_PER_SECOND, Config->Config->AudioSamplerate));
			HR(IMFMediaType_SetUINT32(Type, &MF_MT_AUDIO_NUM_CHANNELS, Config->Config->AudioChannels));

			hr = IMFSinkWriter_SetInputMediaType(Writer, IrEncoder->AudioStreamIndex, Type, NULL);
			IMFMediaType_Release(Type);

			if (FAILED(hr))
			{
				IrSetErrorWide(L"Cannot configure audio encoder input!"); goto bail;
				goto bail;
			}
		}
	}

	hr = IMFSinkWriter_BeginWriting(Writer);
	if (FAILED(hr))
	{
		IrSetErrorWide(L"Cannot start writing to mp4 file!"); goto bail;
		goto bail;
	}

	// input texture
	{
		IrResize_Create(&IrEncoder->Resize, Device, InputWidth, InputHeight, OutputWidth, OutputHeight, Config->Config->GammaCorrectResize, D3D11_BIND_RENDER_TARGET);

		D3D11_RENDER_TARGET_VIEW_DESC InputViewDesc =
		{
			.Format = DXGI_FORMAT_B8G8R8A8_UNORM,
			.ViewDimension = D3D11_RTV_DIMENSION_TEXTURE2D,
		};
		ID3D11Device_CreateRenderTargetView(Device, (ID3D11Resource*)IrEncoder->Resize.InputTexture, &InputViewDesc, &IrEncoder->InputView);

		FLOAT Black[] = { 0, 0, 0, 0 };
		ID3D11DeviceContext_ClearRenderTargetView(Context, IrEncoder->InputView, Black);
	}

	// yuv converter
	{
		IrYuvColorSpace ColorSpace = IsHD ? IrYuvColorSpace_BT709 : IrYuvColorSpace_BT601;
		IrYuv_Create(&IrEncoder->Convert, Device, IrEncoder->Resize.OutputTexture, OutputWidth, OutputHeight, ColorSpace, Config->Config->ImprovedColorConversion);

		DXGI_FORMAT ConvertFormat = IsEqualGUID(VideoInputFormat, &MFVideoFormat_NV12) ? DXGI_FORMAT_NV12 : DXGI_FORMAT_P010;

		for (size_t OutputIndex = 0; OutputIndex < IR_ENCODER_VIDEO_BUFFER_COUNT; OutputIndex++)
		{
			IrYuvOutput_Create(&IrEncoder->ConvertOutput[OutputIndex], Device, OutputWidth, OutputHeight, ConvertFormat);
			ID3D11Texture2D* ConvertOutputTexture = IrEncoder->ConvertOutput[OutputIndex].Texture;

			IMFSample* VideoSample;
			HR(MFCreateVideoSampleFromSurface(NULL, &VideoSample));

			IMFMediaBuffer* Buffer;
			HR(MFCreateDXGISurfaceBuffer(&IID_ID3D11Texture2D, (IUnknown*)ConvertOutputTexture, 0, FALSE, &Buffer));

			UINT32 MaxLength;
			HR(IMFMediaBuffer_GetMaxLength(Buffer, &MaxLength));
			HR(IMFMediaBuffer_SetCurrentLength(Buffer, MaxLength));

			HR(IMFSample_AddBuffer(VideoSample, Buffer));
			IMFMediaBuffer_Release(Buffer);

			IrEncoder->VideoSample[OutputIndex] = VideoSample;
		}
	}

	IrEncoder->InputWidth = Config->Width;
	IrEncoder->InputHeight = Config->Height;
	IrEncoder->OutputWidth = OutputWidth;
	IrEncoder->OutputHeight = OutputHeight;
	IrEncoder->FramerateNum = Config->FramerateNum;
	IrEncoder->FramerateDen = Config->FramerateDen;
	IrEncoder->VideoDiscontinuity = FALSE;
	IrEncoder->VideoLastTime = 0x8000000000000000ULL; // some large time in future

	Assert(IR_ENCODER_VIDEO_BUFFER_COUNT <= 64);
	atomic_init(&IrEncoder->VideoSampleAvailable, (1ULL << IR_ENCODER_VIDEO_BUFFER_COUNT) - 1);

	if (IrEncoder->AudioStreamIndex >= 0)
	{
		// resampler input sample
		HR(MFCreateSample(&IrEncoder->AudioInputSample));

		// resampler output & audio encoding input buffer/samples
		for (int i = 0; i < IR_ENCODER_AUDIO_BUFFER_COUNT; i++)
		{
			IMFSample* Sample;
			IMFMediaBuffer* Buffer;
			IMFTrackedSample* Tracked;

			HR(MFCreateTrackedSample(&Tracked));
			HR(IMFTrackedSample_QueryInterface(Tracked, &IID_IMFSample, (LPVOID*)&Sample));
			HR(MFCreateMemoryBuffer(Config->Config->AudioSamplerate * Config->Config->AudioChannels * sizeof(short), &Buffer));
			HR(IMFSample_AddBuffer(Sample, Buffer));
			IMFMediaBuffer_Release(Buffer);
			IMFTrackedSample_Release(Tracked);

			IrEncoder->AudioSample[i] = Sample;
		}

		IrEncoder->AudioFrameSize = Config->AudioFormat->nBlockAlign;
		IrEncoder->AudioSampleRate = Config->AudioFormat->nSamplesPerSec;
		IrEncoder->Resampler = Resampler;

		Assert(IR_ENCODER_AUDIO_BUFFER_COUNT <= 64);
		atomic_init(&IrEncoder->AudioSampleAvailable, (1ULL << IR_ENCODER_AUDIO_BUFFER_COUNT) - 1);
	}

	ID3D11DeviceContext_AddRef(Context);
	ID3D11Multithread_AddRef(Multithread);
	IrEncoder->Context = Context;
	IrEncoder->Multithread = Multithread;

	IrEncoder->StartTime = 0;
	IrEncoder->Writer = Writer;
	Writer = NULL;
	Resampler = NULL;
	Result = TRUE;

bail:
	if (Resampler)
	{
		IMFTransform_Release(Resampler);
	}
	if (Writer)
	{
		IMFSinkWriter_Release(Writer);
		DeleteFileW(FileName);
	}

	ID3D11Multithread_Release(Multithread);
	ID3D11DeviceContext_Release(Context);

	return Result;
}

void IrEncoder_Stop(IrEncoder* IrEncoder)
{
	if (IrEncoder->AudioStreamIndex >= 0)
	{
		HR(IMFTransform_ProcessMessage(IrEncoder->Resampler, MFT_MESSAGE_COMMAND_DRAIN, 0));
		IrEncoder__OutputAudioSamples(IrEncoder);
		IMFTransform_Release(IrEncoder->Resampler);
	}

	IMFSinkWriter_Finalize(IrEncoder->Writer);
	IMFSinkWriter_Release(IrEncoder->Writer);

	if (IrEncoder->AudioStreamIndex >= 0)
	{
		for (int i = 0; i < IR_ENCODER_AUDIO_BUFFER_COUNT; i++)
		{
			IMFSample_Release(IrEncoder->AudioSample[i]);
		}
		IMFSample_Release(IrEncoder->AudioInputSample);
	}

	for (size_t OutputIndex = 0; OutputIndex < IR_ENCODER_VIDEO_BUFFER_COUNT; OutputIndex++)
	{
		IrYuvOutput_Release(&IrEncoder->ConvertOutput[OutputIndex]);
		IMFSample_Release(IrEncoder->VideoSample[OutputIndex]);
	}
	IrYuv_Release(&IrEncoder->Convert);
	IrResize_Release(&IrEncoder->Resize);
	ID3D11RenderTargetView_Release(IrEncoder->InputView);

	ID3D11Multithread_Release(IrEncoder->Multithread);
	ID3D11DeviceContext_Release(IrEncoder->Context);
}

BOOL IrEncoder_NewFrame(IrEncoder* IrEncoder, ID3D11Texture2D* Texture, RECT Rect, UINT64 Time, UINT64 TimePeriod)
{
	IrEncoder->VideoLastTime = Time;

	uint64_t Available = atomic_load(&IrEncoder->VideoSampleAvailable);
	if (Available == 0)
	{
		// dropped frame
		LONGLONG Timestamp = MFllMulDiv(Time - IrEncoder->StartTime, MF_UNITS_PER_SECOND, TimePeriod, 0);
		HR(IMFSinkWriter_SendStreamTick(IrEncoder->Writer, IrEncoder->VideoStreamIndex, Timestamp));
		IrEncoder->VideoDiscontinuity = TRUE;
		return FALSE;
	}

	DWORD Index;
	_BitScanForward64(&Index, Available);
	atomic_fetch_and(&IrEncoder->VideoSampleAvailable, ~(1ULL << Index));

	ID3D11DeviceContext* Context = IrEncoder->Context;
	ID3D11Multithread_Enter(IrEncoder->Multithread);

	// copy to input texture
	{
		D3D11_BOX Box =
		{
			.left = Rect.left,
			.top = Rect.top,
			.right = Rect.right,
			.bottom = Rect.bottom,
			.front = 0,
			.back = 1,
		};

		DWORD Width = Box.right - Box.left;
		DWORD Height = Box.bottom - Box.top;
		if (Width < IrEncoder->InputWidth || Height < IrEncoder->InputHeight)
		{
			FLOAT Black[] = { 0, 0, 0, 0 };
			ID3D11DeviceContext_ClearRenderTargetView(Context, IrEncoder->InputView, Black);

			Box.right = Box.left + min(IrEncoder->InputWidth, Box.right);
			Box.bottom = Box.top + min(IrEncoder->InputHeight, Box.bottom);
		}
		ID3D11DeviceContext_CopySubresourceRegion(Context, (ID3D11Resource*)IrEncoder->Resize.InputTexture, 0, 0, 0, 0, (ID3D11Resource*)Texture, 0, &Box);
	}

	// resize if needed
	IrResize_Dispatch(&IrEncoder->Resize, Context);

	// convert to YUV
	IrYuv_Dispatch(&IrEncoder->Convert, Context, &IrEncoder->ConvertOutput[Index]);

	ID3D11DeviceContext_Flush(Context);
	ID3D11Multithread_Leave(IrEncoder->Multithread);

	// setup input time & duration
	if (IrEncoder->StartTime == 0)
	{
		IrEncoder->StartTime = Time;
	}

	IMFSample* Sample = IrEncoder->VideoSample[Index];
	HR(IMFSample_SetSampleDuration(Sample, MFllMulDiv(IrEncoder->FramerateDen, MF_UNITS_PER_SECOND, IrEncoder->FramerateNum, 0)));
	HR(IMFSample_SetSampleTime(Sample, MFllMulDiv(Time - IrEncoder->StartTime, MF_UNITS_PER_SECOND, TimePeriod, 0)));

	if (IrEncoder->VideoDiscontinuity)
	{
		HR(IMFSample_SetUINT32(Sample, &MFSampleExtension_Discontinuity, TRUE));
		IrEncoder->VideoDiscontinuity = FALSE;
	}
	else
	{
		// don't care about success or no, we just don't want this attribute set at all
		IMFSample_DeleteItem(Sample, &MFSampleExtension_Discontinuity);
	}

	IMFTrackedSample* Tracked;
	HR(IMFSample_QueryInterface(Sample, &IID_IMFTrackedSample, (LPVOID*)&Tracked));
	IMFTrackedSample_SetAllocator(Tracked, &IrEncoder->VideoSampleCallback, NULL);
	IMFTrackedSample_Release(Tracked);

	// submit to encoder which will happen in background
	HR(IMFSinkWriter_WriteSample(IrEncoder->Writer, IrEncoder->VideoStreamIndex, Sample));

	IMFSample_Release(Sample);

	return TRUE;
}

typedef struct
{
	IMFMediaBuffer Buffer;
	uint8_t* SampleData;
	uint32_t SampleByteCount;
	uint32_t References;
}
IrEncoderAudioBuffer;

static HRESULT STDMETHODCALLTYPE IrEncoderAudioBuffer__QueryInterface(IMFMediaBuffer* This, REFIID Riid, void** Object)
{
	if (Object == NULL)
	{
		return E_POINTER;
	}
	if (IsEqualGUID(Riid, &IID_IMFMediaBuffer) || IsEqualGUID(Riid, &IID_IUnknown))
	{
		*Object = This;
		return S_OK;
	}
	return E_NOINTERFACE;
}

static ULONG STDMETHODCALLTYPE IrEncoderAudioBuffer__AddRef(IMFMediaBuffer* This)
{
	IrEncoderAudioBuffer* Buffer = CONTAINING_RECORD(This, IrEncoderAudioBuffer, Buffer);
	return Buffer->References += 1;
}

static ULONG STDMETHODCALLTYPE IrEncoderAudioBuffer__Release(IMFMediaBuffer* This)
{
	IrEncoderAudioBuffer* Buffer = CONTAINING_RECORD(This, IrEncoderAudioBuffer, Buffer);
	return Buffer->References -= 1;
}

static HRESULT STDMETHODCALLTYPE IrEncoderAudioBuffer__Lock(IMFMediaBuffer* This, BYTE** OutBuffer, DWORD* OutMaxLength, DWORD* OutCurrentLength)
{
	IrEncoderAudioBuffer* Buffer = CONTAINING_RECORD(This, IrEncoderAudioBuffer, Buffer);
	*OutBuffer = Buffer->SampleData;
	if (OutMaxLength)
	{
		*OutMaxLength = Buffer->SampleByteCount;
	}
	if (OutCurrentLength)
	{
		*OutCurrentLength = Buffer->SampleByteCount;
	}
	return S_OK;
}

static HRESULT STDMETHODCALLTYPE IrEncoderAudioBuffer__Unlock(IMFMediaBuffer* This)
{
	return S_OK;
}

static HRESULT STDMETHODCALLTYPE IrEncoderAudioBuffer__GetCurrentLength(IMFMediaBuffer* This, DWORD* OutCurrentLength)
{
	IrEncoderAudioBuffer* Buffer = CONTAINING_RECORD(This, IrEncoderAudioBuffer, Buffer);
	*OutCurrentLength = Buffer->SampleByteCount;
	return S_OK;
}

static HRESULT STDMETHODCALLTYPE IrEncoderAudioBuffer__SetCurrentLength(IMFMediaBuffer* This, DWORD InCurrentLength)
{
	return E_NOTIMPL;
}

static HRESULT STDMETHODCALLTYPE IrEncoderAudioBuffer__GetMaxLength(IMFMediaBuffer* This, DWORD* OutMaxLength)
{
	IrEncoderAudioBuffer* Buffer = CONTAINING_RECORD(This, IrEncoderAudioBuffer, Buffer);
	*OutMaxLength = Buffer->SampleByteCount;
	return S_OK;
}

static IMFMediaBufferVtbl IrEncoderAudioBufferVtbl =
{
	.QueryInterface   = &IrEncoderAudioBuffer__QueryInterface,
	.AddRef           = &IrEncoderAudioBuffer__AddRef,
	.Release          = &IrEncoderAudioBuffer__Release,
	.Lock             = &IrEncoderAudioBuffer__Lock,
	.Unlock           = &IrEncoderAudioBuffer__Unlock,
	.GetCurrentLength = &IrEncoderAudioBuffer__GetCurrentLength,
	.SetCurrentLength = &IrEncoderAudioBuffer__SetCurrentLength,
	.GetMaxLength     = &IrEncoderAudioBuffer__GetMaxLength,
};

void IrEncoder_NewSamples(IrEncoder* IrEncoder, LPCVOID Samples, DWORD FrameCount, UINT64 Time, UINT64 TimePeriod)
{
	IrEncoderAudioBuffer Input =
	{
		.Buffer.lpVtbl = &IrEncoderAudioBufferVtbl,
		.SampleData = (BYTE*)Samples,
		.SampleByteCount = FrameCount * IrEncoder->AudioFrameSize,
	};

	IMFSample* AudioSample = IrEncoder->AudioInputSample;
	HR(IMFSample_AddBuffer(AudioSample, &Input.Buffer));

	// setup input time & duration
	Assert(IrEncoder->StartTime != 0);
	HR(IMFSample_SetSampleDuration(AudioSample, MFllMulDiv(FrameCount, MF_UNITS_PER_SECOND, IrEncoder->AudioSampleRate, 0)));
	HR(IMFSample_SetSampleTime(AudioSample, MFllMulDiv(Time - IrEncoder->StartTime, MF_UNITS_PER_SECOND, TimePeriod, 0)));

	HR(IMFTransform_ProcessInput(IrEncoder->Resampler, 0, AudioSample, 0));
	IrEncoder__OutputAudioSamples(IrEncoder);

	HR(IMFSample_RemoveAllBuffers(AudioSample));
	Assert(Input.References == 0);
}

void IrEncoder_Update(IrEncoder* IrEncoder, UINT64 Time, UINT64 TimePeriod)
{
	// if there was no frame during last second, add discontinuity
	if ((int64_t)(Time - IrEncoder->VideoLastTime) >= (int64_t)TimePeriod)
	{
		IrEncoder->VideoLastTime = Time;
		LONGLONG Timestamp = MFllMulDiv(Time - IrEncoder->StartTime, MF_UNITS_PER_SECOND, TimePeriod, 0);
		HR(IMFSinkWriter_SendStreamTick(IrEncoder->Writer, IrEncoder->VideoStreamIndex, Timestamp));
		IrEncoder->VideoDiscontinuity = TRUE;
	}
}

void IrEncoder_GetStats(IrEncoder* IrEncoder, DWORD* Bitrate, DWORD* LengthMsec, UINT64* FileSize)
{
	MF_SINK_WRITER_STATISTICS Stats = { .cb = sizeof(Stats) };
	HR(IMFSinkWriter_GetStatistics(IrEncoder->Writer, IrEncoder->VideoStreamIndex, &Stats));

	*Bitrate = (DWORD)MFllMulDiv(8 * Stats.qwByteCountProcessed, MF_UNITS_PER_SECOND, 1000 * Stats.llLastTimestampProcessed, 0);
	*LengthMsec = (DWORD)(Stats.llLastTimestampProcessed / 10000);
	*FileSize = Stats.qwByteCountProcessed;

	if (IrEncoder->AudioStreamIndex >= 0)
	{
		HR(IMFSinkWriter_GetStatistics(IrEncoder->Writer, IrEncoder->AudioStreamIndex, &Stats));
		*Bitrate += (DWORD)MFllMulDiv(8 * Stats.qwByteCountProcessed, MF_UNITS_PER_SECOND, 1000 * Stats.llLastTimestampProcessed, 0);
		*FileSize += Stats.qwByteCountProcessed;
	}
}

// ---------------------------------------------------------------------------
// (Buffer types are defined near the live encoder above.)

static void IrFrameRelease(IrFrame *f)
{
    if (!f) return;
    if (InterlockedDecrement(&f->refs) != 0) return;
    if (f->texture) ID3D11Texture2D_Release(f->texture);
    free(f);
}

static void IrAudioBlockRelease(IrAudioBlock *b)
{
    if (!b) return;
    if (InterlockedDecrement(&b->refs) != 0) return;
    free(b->data);
    free(b);
}

// ---------------------------------------------------------------------------
// (Engine and save-job types are defined near the live encoder above.)

static BOOL CALLBACK IrMonitorCountCallback(HMONITOR monitor, HDC hdc, LPRECT rect, LPARAM param)
{
    (void)monitor; (void)hdc; (void)rect;
    int *count = (int *)param;
    (*count)++;
    return TRUE;
}

typedef struct
{
    int want;
    int index;
    HMONITOR handle;
} IrMonitorFind;

static BOOL CALLBACK IrMonitorFindCallback(HMONITOR monitor, HDC hdc, LPRECT rect, LPARAM param)
{
    (void)hdc; (void)rect;
    IrMonitorFind *f = (IrMonitorFind *)param;
    if (f->index++ == f->want)
    {
        f->handle = monitor;
        return FALSE;
    }
    return TRUE;
}

static HMONITOR IrMonitorAtFixed(int index)
{
    IrMonitorFind f;
    f.want = index;
    f.index = 0;
    f.handle = NULL;
    EnumDisplayMonitors(NULL, NULL, IrMonitorFindCallback, (LPARAM)&f);
    return f.handle;
}

static void IrPacketRelease(IrPacket *packet)
{
    if (!packet) return;
    if (InterlockedDecrement(&packet->refs) != 0) return;
    free(packet->data);
    free(packet);
}

static void IrTrimPackets(IrEngine *e)
{
    if (!e || e->packet_count <= 1 || !e->packets) return;
    int64_t newest = e->packets[(e->packet_head + e->packet_count - 1) % e->packet_cap]->pts100ns;
    int64_t cutoff = newest - ((int64_t)max(e->ring_seconds, 1) + 2) * IR_SECOND_100NS;

    // Retain the last keyframe at or before the cutoff: a full-length save
    // needs that pre-roll to start on a decodable packet.
    int keep = 0;
    for (int i = 0; i < e->packet_count; ++i)
    {
        IrPacket *p = e->packets[(e->packet_head + i) % e->packet_cap];
        if (p->pts100ns > cutoff) break;
        if (p->key) keep = i;
    }
    for (int i = 0; i < keep; ++i)
    {
        IrPacket *old = e->packets[e->packet_head];
        e->packet_head = (e->packet_head + 1) % e->packet_cap;
        e->packet_count--;
        e->packet_bytes -= old->bytes;
        IrPacketRelease(old);
        InterlockedIncrement64(&e->dropped);
    }

    // Budget trimming may only cross complete GOPs and never the retained
    // save window.
    while (e->packet_bytes > IR_VIDEO_BUDGET && e->packet_count > 1)
    {
        int next_key = 1;
        while (next_key < e->packet_count &&
               !e->packets[(e->packet_head + next_key) % e->packet_cap]->key)
            next_key++;
        if (next_key >= e->packet_count) break;
        if (e->packets[(e->packet_head + next_key) % e->packet_cap]->pts100ns > cutoff) break;
        for (int i = 0; i < next_key; ++i)
        {
            IrPacket *old = e->packets[e->packet_head];
            e->packet_head = (e->packet_head + 1) % e->packet_cap;
            e->packet_count--;
            e->packet_bytes -= old->bytes;
            IrPacketRelease(old);
            InterlockedIncrement64(&e->dropped);
        }
    }
}

static void IrPushPacket(IrEngine *e, const uint8_t *data, uint32_t bytes,
                         uint32_t codec_data, int64_t pts, int64_t duration, BOOL key)
{
    if (!e || !data || !bytes) return;
    IrPacket *packet = (IrPacket *)calloc(1, sizeof(*packet));
    if (!packet) { InterlockedIncrement64(&e->dropped); return; }
    packet->data = (uint8_t *)malloc(bytes);
    if (!packet->data) { free(packet); InterlockedIncrement64(&e->dropped); return; }
    memcpy(packet->data, data, bytes);
    packet->bytes = bytes;
    packet->codec_data = min(codec_data, bytes);
    packet->pts100ns = pts;
    packet->duration = duration > 0 ? duration : (IR_SECOND_100NS / max(e->fps, 1));
    packet->key = key;
    packet->refs = 1;

    EnterCriticalSection(&e->ring_cs);
    if (!e->packets || !e->packet_cap)
    {
        e->packet_cap = 8192;
        e->packets = (IrPacket **)calloc((size_t)e->packet_cap, sizeof(IrPacket *));
    }
    if (!e->packets)
    {
        LeaveCriticalSection(&e->ring_cs);
        IrPacketRelease(packet);
        InterlockedIncrement64(&e->dropped);
        return;
    }
    if (e->packet_count == e->packet_cap)
    {
        IrPacket *old = e->packets[e->packet_head];
        e->packet_head = (e->packet_head + 1) % e->packet_cap;
        e->packet_count--;
        e->packet_bytes -= old->bytes;
        IrPacketRelease(old);
        InterlockedIncrement64(&e->dropped);
    }
    int slot = (e->packet_head + e->packet_count) % e->packet_cap;
    e->packets[slot] = packet;
    e->packet_count++;
    e->packet_bytes += packet->bytes;
    IrTrimPackets(e);
    LeaveCriticalSection(&e->ring_cs);
    InterlockedIncrement64(&e->encoded);
    InterlockedExchange64(&e->last_pts, pts + packet->duration);
}

static void IrPushAudioBlockTo(IrEngine *e, uint8_t *data, UINT32 bytes, UINT32 frames,
                               int64_t pts, BOOL mic)
{
    IrAudioBlock ***blocks = mic ? &e->mic_blocks : &e->audio_blocks;
    int *cap = mic ? &e->mic_cap : &e->audio_cap;
    int *head = mic ? &e->mic_head : &e->audio_head;
    int *count = mic ? &e->mic_count : &e->audio_count;
    if (!e || !*blocks || !data || !bytes) return;
    IrAudioBlock *b = (IrAudioBlock *)calloc(1, sizeof(*b));
    if (!b) return;
    b->data = (uint8_t *)malloc(bytes);
    if (!b->data) { free(b); return; }
    memcpy(b->data, data, bytes);
    b->bytes = bytes;
    b->frames = frames;
    b->pts100ns = pts;
    b->refs = 1;

    EnterCriticalSection(&e->ring_cs);
    if (*count == *cap)
    {
        IrAudioBlock *old = (*blocks)[*head];
        (*blocks)[*head] = b;
        *head = (*head + 1) % *cap;
        LeaveCriticalSection(&e->ring_cs);
        IrAudioBlockRelease(old);
    }
    else
    {
        int slot = (*head + *count) % *cap;
        (*blocks)[slot] = b;
        (*count)++;
        LeaveCriticalSection(&e->ring_cs);
    }
    // Mic source frames are counted in IrMicThread; only system audio blocks
    // are counted here to avoid double-reporting the microphone rate.
    if (!mic) InterlockedIncrement64(&e->audio_frames);
}

static void IrPushAudioBlock(IrEngine *e, uint8_t *data, UINT32 bytes, UINT32 frames, int64_t pts)
{
    IrPushAudioBlockTo(e, data, bytes, frames, pts, FALSE);
}

static void IrPushMicBlock(IrEngine *e, const int16_t *samples, uint32_t frames, int64_t pts)
{
    IrPushAudioBlockTo(e, (uint8_t *)samples, frames * sizeof(int16_t), frames, pts, TRUE);
}

BOOL IrPushFrame(IrEngine *e, ID3D11Texture2D *src, RECT rect, int64_t pts)
{
    if (!e || !src || !e->dev || !e->live.mft ||
        InterlockedCompareExchange(&e->running, 1, 1) == 0 || e->paused)
        return FALSE;
    LONG w = rect.right - rect.left;
    LONG h = rect.bottom - rect.top;
    if (w < 2 || h < 2) return FALSE;
    D3D11_TEXTURE2D_DESC sd;
    ID3D11Texture2D_GetDesc(src, &sd);
    rect.left = max(rect.left, 0);
    rect.top = max(rect.top, 0);
    rect.right = min(rect.right, (LONG)sd.Width);
    rect.bottom = min(rect.bottom, (LONG)sd.Height);
    if (rect.right - rect.left < 2 || rect.bottom - rect.top < 2) return FALSE;

    int64_t stamp = pts + e->time_offset;
    BOOL idr = InterlockedCompareExchange(&e->force_idr, 0, 1) != 0;
    if (e->last_encoder_width != (int)(rect.right - rect.left) ||
        e->last_encoder_height != (int)(rect.bottom - rect.top))
    {
        idr = TRUE;
        e->last_encoder_width = (int)(rect.right - rect.left);
        e->last_encoder_height = (int)(rect.bottom - rect.top);
        e->next_idr_pts = stamp + ((e->pref & 0x200) ? IR_SECOND_100NS : 2 * IR_SECOND_100NS);
    }
    if (stamp >= e->next_idr_pts)
    {
        idr = TRUE;
        e->next_idr_pts = stamp + ((e->pref & 0x200) ? IR_SECOND_100NS : 2 * IR_SECOND_100NS);
    }
    char error[192];
    error[0] = '\0';
    EnterCriticalSection(&e->encoder_cs);
    BOOL encoded = IrLive_EncodeFrame(&e->live, e, e->ctx, src, rect, stamp, idr, error, sizeof(error));
    LeaveCriticalSection(&e->encoder_cs);
    if (!encoded)
    {
        InterlockedExchange(&e->fail_stage, STAGE_FRAME);
        IrSetErrorFmt("%s", error);
        InterlockedIncrement64(&e->rejected);
        return FALSE;
    }
    InterlockedExchange64(&e->last_frame_tick, IrNow100ns());
    int64_t rp = InterlockedCompareExchange64(&e->repeat_pts, 0, 0);
    InterlockedExchange64(&e->repeat_pts, max(rp, stamp));
    InterlockedIncrement64(&e->captured);
    // Frame-driven video tick. IrHeartbeatThread also advances the heartbeat
    // while capture is alive but the source is static and delivers no frames.
    InterlockedIncrement64(&e->heartbeat);
    InterlockedExchange64(&e->last_frame_pts, pts);
    (void)w; (void)h;
    return TRUE;
}

// Desktop capture APIs only signal changed content. Re-feed the last converted
// frame at the target rate so a static screen still produces a replay clip.
static void IrPushStaticFrame(IrEngine *e)
{
    if (!e || !e->live.mft || !e->live.nv12) return;
    int64_t last = InterlockedCompareExchange64(&e->last_frame_tick, 0, 0);
    if (!last) return;
    int64_t interval = IR_SECOND_100NS / max(e->fps, 1);
    if (interval < 1) interval = 1;
    int64_t now = IrNow100ns();
    if (now - last < interval * 2) return;

    EnterCriticalSection(&e->encoder_cs);
    if (InterlockedCompareExchange(&e->running, 1, 1) == 0 || e->paused ||
        !e->live.mft || !e->live.nv12)
    {
        LeaveCriticalSection(&e->encoder_cs);
        return;
    }
    int64_t rp = InterlockedCompareExchange64(&e->repeat_pts, 0, 0);
    if (rp == 0) rp = last;
    int64_t target = now - interval;
    char error[192];
    error[0] = '\0';
    int emitted = 0;
    while (rp + interval <= target && emitted < 8)
    {
        rp += interval;
        if (!IrLive_FeedNv12(&e->live, e, rp, FALSE, error, sizeof(error))) break;
        ++emitted;
    }
    InterlockedExchange64(&e->repeat_pts, rp);
    LeaveCriticalSection(&e->encoder_cs);
}

// ---------------------------------------------------------------------------
// DXGI desktop duplication with cursor compositing
// ---------------------------------------------------------------------------
static const char g_ir_cursor_hlsl[] =
    "struct VSOut { float4 pos : SV_POSITION; float2 uv : TEXCOORD0; };\n"
    "VSOut vs(uint id : SV_VertexID)\n"
    "{\n"
    "    VSOut o;\n"
    "    float2 p = float2(float((id << 1) & 2), float(id & 2));\n"
    "    o.pos = float4(p * 2.0 - 1.0, 0.0, 1.0);\n"
    "    o.uv = float2(p.x, 1.0f - p.y);\n"
    "    return o;\n"
    "}\n"
    "Texture2D sprite : register(t0);\n"
    "SamplerState samp : register(s0);\n"
    "float4 ps(VSOut i) : SV_TARGET\n"
    "{\n"
    "    return sprite.Sample(samp, i.uv);\n"
    "}\n";

static void IrCursorComp_Release(IrCursorComp *c)
{
    if (c->vs) ID3D11VertexShader_Release(c->vs);
    if (c->ps) ID3D11PixelShader_Release(c->ps);
    if (c->blend) ID3D11BlendState_Release(c->blend);
    if (c->sampler) ID3D11SamplerState_Release(c->sampler);
    memset(c, 0, sizeof(*c));
}

static HRESULT IrCursorComp_Init(IrCursorComp *c, ID3D11Device *dev)
{
    if (c->ok)
        return S_OK;
    ID3DBlob *vs_blob = NULL, *ps_blob = NULL, *err = NULL;
    HRESULT hr = D3DCompile(g_ir_cursor_hlsl, sizeof(g_ir_cursor_hlsl) - 1, NULL, NULL, NULL,
                            "vs", "vs_5_0", 0, 0, &vs_blob, &err);
    if (SUCCEEDED(hr))
        hr = D3DCompile(g_ir_cursor_hlsl, sizeof(g_ir_cursor_hlsl) - 1, NULL, NULL, NULL,
                        "ps", "ps_5_0", 0, 0, &ps_blob, &err);
    if (err)
        ID3D10Blob_Release(err);
    if (FAILED(hr))
        goto bail;

    hr = ID3D11Device_CreateVertexShader(dev, ID3D10Blob_GetBufferPointer(vs_blob),
                                         ID3D10Blob_GetBufferSize(vs_blob), NULL, &c->vs);
    if (FAILED(hr)) goto bail;
    hr = ID3D11Device_CreatePixelShader(dev, ID3D10Blob_GetBufferPointer(ps_blob),
                                        ID3D10Blob_GetBufferSize(ps_blob), NULL, &c->ps);
    if (FAILED(hr)) goto bail;

    D3D11_BLEND_DESC bd;
    memset(&bd, 0, sizeof(bd));
    bd.RenderTarget[0].BlendEnable = TRUE;
    bd.RenderTarget[0].SrcBlend = D3D11_BLEND_SRC_ALPHA;
    bd.RenderTarget[0].DestBlend = D3D11_BLEND_INV_SRC_ALPHA;
    bd.RenderTarget[0].BlendOp = D3D11_BLEND_OP_ADD;
    bd.RenderTarget[0].SrcBlendAlpha = D3D11_BLEND_ONE;
    bd.RenderTarget[0].DestBlendAlpha = D3D11_BLEND_INV_SRC_ALPHA;
    bd.RenderTarget[0].BlendOpAlpha = D3D11_BLEND_OP_ADD;
    bd.RenderTarget[0].RenderTargetWriteMask = D3D11_COLOR_WRITE_ENABLE_ALL;
    hr = ID3D11Device_CreateBlendState(dev, &bd, &c->blend);
    if (FAILED(hr)) goto bail;

    D3D11_SAMPLER_DESC sd;
    memset(&sd, 0, sizeof(sd));
    sd.Filter = D3D11_FILTER_MIN_MAG_MIP_POINT;
    sd.AddressU = D3D11_TEXTURE_ADDRESS_CLAMP;
    sd.AddressV = D3D11_TEXTURE_ADDRESS_CLAMP;
    sd.AddressW = D3D11_TEXTURE_ADDRESS_CLAMP;
    sd.MaxLOD = 16.0f;
    hr = ID3D11Device_CreateSamplerState(dev, &sd, &c->sampler);
    if (FAILED(hr)) goto bail;
    c->ok = TRUE;

bail:
    if (vs_blob) ID3D10Blob_Release(vs_blob);
    if (ps_blob) ID3D10Blob_Release(ps_blob);
    if (FAILED(hr)) IrCursorComp_Release(c);
    return hr;
}

static void IrCursor_Release(IrCursor *p)
{
    free(p->buf);
    if (p->sprite) ID3D11Texture2D_Release(p->sprite);
    if (p->srv) ID3D11ShaderResourceView_Release(p->srv);
    memset(p, 0, sizeof(*p));
}

static void IrCursor_Update(IrCursor *p, IDXGIOutputDuplication *dup,
                            const DXGI_OUTDUPL_FRAME_INFO *info, int origin_x, int origin_y)
{
    BOOL fresh = info->LastMouseUpdateTime.QuadPart != 0;
    if (info->PointerPosition.Visible)
    {
        p->at.x = info->PointerPosition.Position.x - origin_x;
        p->at.y = info->PointerPosition.Position.y - origin_y;
        p->visible = TRUE;
    }
    else if (fresh)
    {
        p->visible = FALSE;
    }

    UINT offered = info->PointerShapeBufferSize;
    if (!offered && p->have_shape)
        return;
    if (p->buf && p->len < max(offered, 4096))
    {
        free(p->buf);
        p->buf = NULL;
    }
    if (!p->buf)
    {
        p->len = max(offered, 4096);
        p->buf = (uint8_t *)malloc(p->len);
        if (!p->buf)
        {
            p->len = 0;
            return;
        }
    }
    UINT needed = 0;
    DXGI_OUTDUPL_POINTER_SHAPE_INFO shape;
    memset(&shape, 0, sizeof(shape));
    HRESULT hr = IDXGIOutputDuplication_GetFramePointerShape(dup, p->len, p->buf, &needed, &shape);
    if (FAILED(hr) && needed > p->len)
    {
        free(p->buf);
        p->len = needed;
        p->buf = (uint8_t *)malloc(p->len);
        if (!p->buf)
        {
            p->len = 0;
            return;
        }
        hr = IDXGIOutputDuplication_GetFramePointerShape(dup, p->len, p->buf, &needed, &shape);
    }
    if (SUCCEEDED(hr) && needed >= 4)
    {
        p->len = needed;
        p->shape = shape;
        p->have_shape = TRUE;
        p->failed = FALSE;
    }
    else
    {
        p->failed = !p->have_shape;
    }
}

static const uint8_t *IrCursorRow(const IrCursor *p, UINT y, size_t bytes)
{
    size_t start = (size_t)y * p->shape.Pitch;
    if (p->shape.Pitch == 0 || start > p->len || bytes > p->len - start)
        return NULL;
    return p->buf + start;
}

static void IrCursorBuildSprite(const IrCursor *p, uint32_t *pixels, UINT rw, UINT rh, int px, int py)
{
    const IrCursor *cur = p;
    const DXGI_OUTDUPL_POINTER_SHAPE_INFO *shape = &cur->shape;
    if (shape->Type == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR)
    {
        for (UINT y = 0; y < shape->Height; ++y)
        {
            const uint8_t *s = IrCursorRow(cur, y, (size_t)shape->Width * 4);
            if (!s) continue;
            for (UINT x = 0; x < shape->Width; ++x)
            {
                int dx = px + (int)x, dy = py + (int)y;
                if (dx < 0 || dy < 0 || dx >= (int)rw || dy >= (int)rh) continue;
                const uint8_t *q = s + x * 4;
                pixels[(size_t)dy * rw + dx] = (uint32_t)q[0] | ((uint32_t)q[1] << 8) |
                                                ((uint32_t)q[2] << 16) | ((uint32_t)q[3] << 24);
            }
        }
    }
    else if (shape->Type == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR)
    {
        for (UINT y = 0; y < shape->Height; ++y)
        {
            const uint8_t *s = IrCursorRow(cur, y, (size_t)shape->Width * 4);
            if (!s) continue;
            for (UINT x = 0; x < shape->Width; ++x)
            {
                int dx = px + (int)x, dy = py + (int)y;
                if (dx < 0 || dy < 0 || dx >= (int)rw || dy >= (int)rh) continue;
                const uint8_t *q = s + x * 4;
                pixels[(size_t)dy * rw + dx] = 0xFF000000u | (uint32_t)q[0] |
                                                ((uint32_t)q[1] << 8) | ((uint32_t)q[2] << 16);
            }
        }
    }
    else if (shape->Type == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME)
    {
        UINT half = shape->Height / 2;
        size_t stride = ((size_t)shape->Width + 31) / 32 * 4;
        for (UINT y = 0; y < half; ++y)
        {
            const uint8_t *and_row = IrCursorRow(cur, y, stride);
            const uint8_t *xor_row = IrCursorRow(cur, y + half, stride);
            if (!and_row || !xor_row) continue;
            for (UINT x = 0; x < shape->Width; ++x)
            {
                uint8_t bit = 0x80u >> (x & 7);
                BOOL and_bit = (and_row[x / 8] & bit) != 0;
                BOOL xor_bit = (xor_row[x / 8] & bit) != 0;
                if (and_bit && !xor_bit) continue;
                int dx = px + (int)x, dy = py + (int)y;
                if (dx < 0 || dy < 0 || dx >= (int)rw || dy >= (int)rh) continue;
                pixels[(size_t)dy * rw + dx] = (!and_bit && !xor_bit) ? 0xFF000000u : 0xFFFFFFFFu;
            }
        }
    }
}

static void IrCursor_Draw(IrCursor *p, ID3D11Device *dev, ID3D11DeviceContext *ctx,
                          ID3D11Texture2D *desk, ID3D11RenderTargetView *desk_rtv, IrCursorComp *comp)
{
    if (!p->visible || !p->have_shape || p->failed || !comp->ok || !desk_rtv)
        return;
    D3D11_TEXTURE2D_DESC td;
    ID3D11Texture2D_GetDesc(desk, &td);
    if (td.Width < 2 || td.Height < 2 || p->shape.Width == 0 || p->shape.Pitch == 0)
        return;
    int sh = p->shape.Type == DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME ? (int)p->shape.Height / 2 : (int)p->shape.Height;
    if (sh <= 0) return;
    int x0 = max(0, p->at.x), y0 = max(0, p->at.y);
    int x1 = min(p->at.x + (int)p->shape.Width, (int)td.Width);
    int y1 = min(p->at.y + sh, (int)td.Height);
    if (x1 <= x0 || y1 <= y0) return;
    UINT rw = (UINT)(x1 - x0), rh = (UINT)(y1 - y0);

    uint32_t *pixels = (uint32_t *)calloc((size_t)rw * rh, sizeof(uint32_t));
    if (!pixels) return;
    IrCursorBuildSprite(p, pixels, rw, rh, p->at.x - x0, p->at.y - y0);

    D3D11_TEXTURE2D_DESC cur;
    memset(&cur, 0, sizeof(cur));
    if (p->sprite) ID3D11Texture2D_GetDesc(p->sprite, &cur);
    if (!p->sprite || cur.Width < rw || cur.Height < rh || cur.Format != DXGI_FORMAT_B8G8R8A8_UNORM)
    {
        if (p->sprite) ID3D11Texture2D_Release(p->sprite);
        if (p->srv) ID3D11ShaderResourceView_Release(p->srv);
        p->sprite = NULL;
        p->srv = NULL;
        D3D11_TEXTURE2D_DESC sd;
        memset(&sd, 0, sizeof(sd));
        sd.Width = rw;
        sd.Height = rh;
        sd.MipLevels = sd.ArraySize = 1;
        sd.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
        sd.SampleDesc.Count = 1;
        sd.Usage = D3D11_USAGE_DEFAULT;
        sd.BindFlags = D3D11_BIND_SHADER_RESOURCE;
        if (FAILED(ID3D11Device_CreateTexture2D(dev, &sd, NULL, &p->sprite))) goto done;
        if (FAILED(ID3D11Device_CreateShaderResourceView(dev, (ID3D11Resource *)p->sprite, NULL, &p->srv))) goto done;
    }
    {
        D3D11_BOX box;
        memset(&box, 0, sizeof(box));
        box.right = rw;
        box.bottom = rh;
        box.back = 1;
        ID3D11DeviceContext_UpdateSubresource(ctx, (ID3D11Resource *)p->sprite, 0, &box, pixels, rw * 4, 0);
    }

    ID3D11RenderTargetView *prev_rtv = NULL;
    ID3D11DeviceContext_OMGetRenderTargets(ctx, 1, &prev_rtv, NULL);
    UINT nv = D3D11_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE;
    D3D11_VIEWPORT prev_vp[D3D11_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE];
    ID3D11DeviceContext_RSGetViewports(ctx, &nv, prev_vp);

    D3D11_VIEWPORT vp;
    memset(&vp, 0, sizeof(vp));
    vp.TopLeftX = (float)x0;
    vp.TopLeftY = (float)y0;
    vp.Width = (float)rw;
    vp.Height = (float)rh;
    vp.MaxDepth = 1.0f;
    ID3D11DeviceContext_RSSetViewports(ctx, 1, &vp);
    ID3D11DeviceContext_OMSetRenderTargets(ctx, 1, &desk_rtv, NULL);
    {
        const float zero[4] = { 0, 0, 0, 0 };
        ID3D11DeviceContext_OMSetBlendState(ctx, comp->blend, zero, 0xFFFFFFFF);
    }
    ID3D11DeviceContext_VSSetShader(ctx, comp->vs, NULL, 0);
    ID3D11DeviceContext_PSSetShader(ctx, comp->ps, NULL, 0);
    ID3D11DeviceContext_PSSetShaderResources(ctx, 0, 1, &p->srv);
    ID3D11DeviceContext_PSSetSamplers(ctx, 0, 1, &comp->sampler);
    ID3D11DeviceContext_Draw(ctx, 3, 0);

    ID3D11ShaderResourceView *none = NULL;
    ID3D11DeviceContext_PSSetShaderResources(ctx, 0, 1, &none);
    {
        const float zero[4] = { 0, 0, 0, 0 };
        ID3D11DeviceContext_OMSetBlendState(ctx, NULL, zero, 0xFFFFFFFF);
    }
    ID3D11DeviceContext_OMSetRenderTargets(ctx, 1, &prev_rtv, NULL);
    if (prev_rtv) ID3D11RenderTargetView_Release(prev_rtv);
    ID3D11DeviceContext_RSSetViewports(ctx, nv, prev_vp);

done:
    free(pixels);
}

static HRESULT IrDxgiOpenDuplication(IrDxgi *d)
{
    if (d->dup) IDXGIOutputDuplication_Release(d->dup);
    d->dup = NULL;
    IDXGIDevice *dxgi = NULL;
    IDXGIAdapter *adapter = NULL;
    HRESULT hr = ID3D11Device_QueryInterface(d->dev, &IID_IDXGIDevice, (void **)&dxgi);
    if (FAILED(hr)) return hr;
    hr = IDXGIDevice_GetAdapter(dxgi, &adapter);
    IDXGIDevice_Release(dxgi);
    if (FAILED(hr)) return hr;

    for (UINT i = 0;; ++i)
    {
        IDXGIOutput *out = NULL;
        if (IDXGIAdapter_EnumOutputs(adapter, i, &out) == DXGI_ERROR_NOT_FOUND)
            break;
        DXGI_OUTPUT_DESC od;
        if (FAILED(IDXGIOutput_GetDesc(out, &od)) || od.Monitor != d->monitor)
        {
            IDXGIOutput_Release(out);
            continue;
        }
        IDXGIOutput1 *out1 = NULL;
        hr = IDXGIOutput_QueryInterface(out, &IID_IDXGIOutput1, (void **)&out1);
        IDXGIOutput_Release(out);
        if (FAILED(hr)) break;
        hr = IDXGIOutput1_DuplicateOutput(out1, (IUnknown *)d->dev, &d->dup);
        IDXGIOutput1_Release(out1);
        if (FAILED(hr)) break;

        DXGI_OUTDUPL_DESC dd;
        IDXGIOutputDuplication_GetDesc(d->dup, &dd);
        if (dd.ModeDesc.Width < 2 || dd.ModeDesc.Height < 2)
        {
            hr = E_INVALIDARG;
            break;
        }
        if (d->base) ID3D11Texture2D_Release(d->base);
        if (d->desk) ID3D11Texture2D_Release(d->desk);
        if (d->rtv) ID3D11RenderTargetView_Release(d->rtv);
        d->base = d->desk = NULL;
        d->rtv = NULL;
        D3D11_TEXTURE2D_DESC td;
        memset(&td, 0, sizeof(td));
        td.Width = dd.ModeDesc.Width;
        td.Height = dd.ModeDesc.Height;
        td.MipLevels = td.ArraySize = 1;
        td.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
        td.SampleDesc.Count = 1;
        td.Usage = D3D11_USAGE_DEFAULT;
        td.BindFlags = D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE;
        td.MiscFlags = D3D11_RESOURCE_MISC_GDI_COMPATIBLE;
        hr = ID3D11Device_CreateTexture2D(d->dev, &td, NULL, &d->base);
        if (FAILED(hr)) break;
        hr = ID3D11Device_CreateTexture2D(d->dev, &td, NULL, &d->desk);
        if (FAILED(hr)) break;
        hr = ID3D11Device_CreateRenderTargetView(d->dev, (ID3D11Resource *)d->desk, NULL, &d->rtv);
        break;
    }
    IDXGIAdapter_Release(adapter);
    return hr;
}

static DWORD WINAPI IrDxgiThread(LPVOID arg)
{
    IrDxgi *d = (IrDxgi *)arg;
    BOOL have_frame = FALSE;
    while (InterlockedCompareExchange(&d->running, 1, 1))
    {
        DXGI_OUTDUPL_FRAME_INFO info;
        memset(&info, 0, sizeof(info));
        IDXGIResource *res = NULL;
        HRESULT hr = IDXGIOutputDuplication_AcquireNextFrame(d->dup, 50, &info, &res);
        if (hr == DXGI_ERROR_WAIT_TIMEOUT) continue;
        if (hr == DXGI_ERROR_ACCESS_LOST)
        {
            have_frame = FALSE;
            while (InterlockedCompareExchange(&d->running, 1, 1) && FAILED(IrDxgiOpenDuplication(d)))
                Sleep(100);
            continue;
        }
        if (FAILED(hr))
        {
            InterlockedExchange(&d->last_hr, (long)hr);
            Sleep(10);
            continue;
        }
        if (info.LastPresentTime.QuadPart && res)
        {
            ID3D11Texture2D *frame = NULL;
            if (SUCCEEDED(IDXGIResource_QueryInterface(res, &IID_ID3D11Texture2D, (void **)&frame)))
            {
                ID3D11DeviceContext_CopyResource(d->ctx, (ID3D11Resource *)d->base, (ID3D11Resource *)frame);
                ID3D11Texture2D_Release(frame);
                have_frame = TRUE;
            }
        }
        if (res) IDXGIResource_Release(res);
        if (d->cursor.buf || (d->engine->options & CAP_CURSOR))
            IrCursor_Update(&d->cursor, d->dup, &info, (int)d->mon_rect.left, (int)d->mon_rect.top);
        IDXGIOutputDuplication_ReleaseFrame(d->dup);
        if (!have_frame) continue;

        int64_t tick = info.LastPresentTime.QuadPart;
        if (d->engine->options & CAP_CURSOR)
            tick = max(tick, info.LastMouseUpdateTime.QuadPart);
        if (tick <= 0) continue;
        IrEngine *e = d->engine;
        if (e->paused)
        {
            InterlockedIncrement64(&e->rejected);
            continue;
        }
        int64_t pts = IrQpcTo100ns(tick);
        if (e->fps > 0 && e->last_frame_pts > 0 && pts - e->last_frame_pts < 10000000LL / e->fps)
        {
            InterlockedIncrement64(&e->rejected);
            continue;
        }

        D3D11_TEXTURE2D_DESC td;
        ID3D11Texture2D_GetDesc(d->desk, &td);
        RECT box;
        box.left = box.top = 0;
        box.right = (LONG)td.Width;
        box.bottom = (LONG)td.Height;
        if (e->window && IsWindow(e->window))
        {
            RECT r;
            BOOL ok = FALSE;
            if (e->options & CAP_CLIENT_AREA)
            {
                RECT c;
                POINT o;
                if (GetClientRect(e->window, &c) && ClientToScreen(e->window, &o))
                {
                    r.left = o.x; r.top = o.y;
                    r.right = o.x + c.right; r.bottom = o.y + c.bottom;
                    ok = TRUE;
                }
            }
            else
            {
                ok = SUCCEEDED(DwmGetWindowAttribute(e->window, DWMWA_EXTENDED_FRAME_BOUNDS, &r, sizeof(r)));
            }
            if (!ok || IsIconic(e->window)) continue;
            LONG l = max(r.left - d->mon_rect.left, 0);
            LONG t = max(r.top - d->mon_rect.top, 0);
            LONG rr = min(r.right - d->mon_rect.left, (LONG)td.Width);
            LONG bb = min(r.bottom - d->mon_rect.top, (LONG)td.Height);
            if (rr - l < 2 || bb - t < 2) continue;
            box.left = l; box.top = t; box.right = rr; box.bottom = bb;
        }
        ID3D11DeviceContext_CopyResource(d->ctx, (ID3D11Resource *)d->desk, (ID3D11Resource *)d->base);
        if (e->options & CAP_CURSOR)
            IrCursor_Draw(&d->cursor, d->dev, d->ctx, d->desk, d->rtv, &d->comp);
        if (IrPushFrame(e, d->desk, box, pts))
        {
            InterlockedIncrement64(&e->paced);
            InterlockedExchange64(&e->last_frame_pts, pts);
        }
        else
        {
            InterlockedIncrement64(&e->rejected);
        }
    }
    return 0;
}

static void IrDxgiStop(IrDxgi *d)
{
    InterlockedExchange(&d->running, 0);
    if (d->stop_event) SetEvent(d->stop_event);
    if (d->thread)
    {
        WaitForSingleObject(d->thread, INFINITE);
        CloseHandle(d->thread);
        d->thread = NULL;
    }
    if (d->stop_event)
    {
        CloseHandle(d->stop_event);
        d->stop_event = NULL;
    }
    if (d->dup) IDXGIOutputDuplication_Release(d->dup);
    if (d->rtv) ID3D11RenderTargetView_Release(d->rtv);
    if (d->base) ID3D11Texture2D_Release(d->base);
    if (d->desk) ID3D11Texture2D_Release(d->desk);
    IrCursor_Release(&d->cursor);
    IrCursorComp_Release(&d->comp);
    memset(d, 0, sizeof(*d));
}

static HRESULT IrDxgiStart(IrDxgi *d, IrEngine *e)
{
    memset(d, 0, sizeof(*d));
    d->dev = e->dev;
    d->ctx = e->ctx;
    d->engine = e;
    d->monitor = IrMonitorAtFixed(e->monitor_index);
    if (!d->monitor && e->window)
        d->monitor = MonitorFromWindow(e->window, MONITOR_DEFAULTTOPRIMARY);
    if (!d->monitor)
        d->monitor = IrMonitorAtFixed(0);
    if (!d->monitor)
        return E_FAIL;
    MONITORINFO mi;
    memset(&mi, 0, sizeof(mi));
    mi.cbSize = sizeof(mi);
    if (!GetMonitorInfoW(d->monitor, &mi))
        return HRESULT_FROM_WIN32(GetLastError());
    d->mon_rect = mi.rcMonitor;
    HRESULT hr = IrDxgiOpenDuplication(d);
    if (FAILED(hr)) return hr;
    if (e->options & CAP_CURSOR)
    {
        hr = IrCursorComp_Init(&d->comp, d->dev);
        if (FAILED(hr)) return hr;
    }
    d->stop_event = CreateEventW(NULL, TRUE, FALSE, NULL);
    if (!d->stop_event) return HRESULT_FROM_WIN32(GetLastError());
    InterlockedExchange(&d->running, 1);
    d->thread = CreateThread(NULL, 0, IrDxgiThread, d, 0, NULL);
    if (!d->thread)
    {
        InterlockedExchange(&d->running, 0);
        CloseHandle(d->stop_event);
        d->stop_event = NULL;
        return HRESULT_FROM_WIN32(GetLastError());
    }
    return S_OK;
}

// ---------------------------------------------------------------------------
// Replay rings
static bool IrWgcFrameCallback(IrWgc *capture, IrWgcFrame *frame)
{
    IrEngine *e = (IrEngine *)capture->OnFrameUserData;
    if (!e || !frame || !frame->Texture) return true;
    if (!InterlockedCompareExchange(&e->running, 1, 1) || e->paused)
        return true;
    int64_t pts = IrNow100ns();
    if (e->fps > 0 && e->last_frame_pts > 0 && pts - e->last_frame_pts < 10000000LL / e->fps)
        return true;
    if (IrPushFrame(e, frame->Texture, frame->Rect, pts))
    {
        InterlockedIncrement64(&e->paced);
        InterlockedExchange64(&e->last_frame_pts, pts);
    }
    else
    {
        InterlockedIncrement64(&e->rejected);
    }
    return true;
}

// Capture APIs only deliver frames when the source changes. Tick the UI
// heartbeat from capture liveness and keep the compressed ring populated with
// repeated frames so clipping still works on a static desktop.
static DWORD WINAPI IrHeartbeatThread(LPVOID arg)
{
    IrEngine *e = (IrEngine *)arg;
    while (WaitForSingleObject(e->hb_stop, 100) == WAIT_TIMEOUT)
    {
        if (InterlockedCompareExchange(&e->running, 1, 1) &&
            InterlockedCompareExchange(&e->paused, 0, 0) == 0)
        {
            InterlockedIncrement64(&e->heartbeat);
            IrPushStaticFrame(e);
        }
    }
    return 0;
}

static DWORD WINAPI IrAudioPumpThread(LPVOID arg)
{
    IrEngine *e = (IrEngine *)arg;
    CoInitializeEx(NULL, COINIT_MULTITHREADED);
    while (WaitForSingleObject(e->audio_stop, 20) == WAIT_TIMEOUT)
    {
        IrAudioData data;
        memset(&data, 0, sizeof(data));
        while (IrAudio_GetData(&e->audio, &data, (uint64_t)e->start_qpc))
        {
            if (e->audio.Format && data.Count && data.Samples && !e->paused)
            {
                uint32_t out_frames = 0;
                int16_t *pcm = IrPcm_Convert(&e->audio_converter, (const uint8_t *)data.Samples,
                                             (uint32_t)data.Count, &out_frames);
                if (pcm && out_frames)
                {
                    IrPushAudioBlock(e, (uint8_t *)pcm, out_frames * IR_AUDIO_CHANNELS * sizeof(int16_t),
                                     out_frames,
                                     IrQpcTo100ns((int64_t)data.Time) +
                                         InterlockedCompareExchange64(&e->time_offset, 0, 0));
                    int peak = 0;
                    for (uint32_t i = 0; i < out_frames * IR_AUDIO_CHANNELS; ++i)
                    {
                        int v = pcm[i] < 0 ? -(int)pcm[i] : (int)pcm[i];
                        if (v > peak) peak = v;
                    }
                    for (;;)
                    {
                        long old = InterlockedCompareExchange(&e->audio_peak, 0, 0);
                        if (peak <= old || InterlockedCompareExchange(&e->audio_peak, peak, old) == old) break;
                    }
                    InterlockedIncrement64(&e->audio_got);
                    free(pcm);
                }
                else
                {
                    InterlockedIncrement(&e->audio_convert_failures);
                    e->audio_init_hr = E_FAIL;
                }
            }
            IrAudio_ReleaseData(&e->audio, &data);
            memset(&data, 0, sizeof(data));
        }
        InterlockedIncrement64(&e->audio_events);
    }
    CoUninitialize();
    return 0;
}

static void IrStopCapture(IrEngine *e)
{
    if (e->wgc.ItemInterop || e->wgc.Session || e->wgc.Item || e->wgc.FramePool)
    {
        IrWgc_Stop(&e->wgc);
        IrWgc_Release(&e->wgc);
    }
    IrDxgiStop(&e->dxgi);
}

static HRESULT IrStartCapture(IrEngine *e)
{
    IrStopCapture(e);
    if (e->method == IR_CAPTURE_EXTERNAL)
        return S_OK;
    if (e->method == IR_CAPTURE_DXGI)
        return IrDxgiStart(&e->dxgi, e);

    IrWgc_Create(&e->wgc, IrWgcFrameCallback, true);
    e->wgc.OnFrameUserData = e;
    BOOL created = FALSE;
    if (e->window && IsWindow(e->window))
    {
        created = IrWgc_CreateForWindow(&e->wgc, e->dev, e->window,
                                         (e->options & CAP_CLIENT_AREA) != 0,
                                         (e->options & CAP_KEEP_CORNERS) == 0);
    }
    else
    {
        HMONITOR monitor = IrMonitorAtFixed(e->monitor_index);
        created = IrWgc_CreateForMonitor(&e->wgc, e->dev, monitor, NULL);
    }
    if (!created)
    {
        InterlockedExchange(&e->capture_hr, (long)E_FAIL);
        return E_FAIL;
    }
    IrWgc_Start(&e->wgc,
                e->config.MouseCursor,
                e->config.ShowRecordingBorder,
                e->config.IncludeSecondaryWindows);
    InterlockedExchange(&e->capture_hr, 0);
    return S_OK;
}

static void IrFreeRings(IrEngine *e)
{
    EnterCriticalSection(&e->ring_cs);
    for (int i = 0; i < e->packet_count; ++i)
        IrPacketRelease(e->packets[(e->packet_head + i) % e->packet_cap]);
    for (int i = 0; i < e->audio_count; ++i)
        IrAudioBlockRelease(e->audio_blocks[(e->audio_head + i) % e->audio_cap]);
    for (int i = 0; i < e->mic_count; ++i)
        IrAudioBlockRelease(e->mic_blocks[(e->mic_head + i) % e->mic_cap]);
    e->packet_head = e->packet_count = 0;
    e->packet_bytes = 0;
    e->audio_head = e->audio_count = 0;
    e->mic_head = e->mic_count = 0;
    LeaveCriticalSection(&e->ring_cs);
    free(e->packets);
    free(e->audio_blocks);
    free(e->mic_blocks);
    e->packets = NULL;
    e->audio_blocks = NULL;
    e->mic_blocks = NULL;
    e->packet_cap = e->audio_cap = e->mic_cap = 0;
}

static void IrAllocateRings(IrEngine *e)
{
    IrFreeRings(e);
    int audio_wanted = e->ring_seconds * 100;
    if (audio_wanted < 64) audio_wanted = 64;
    if (audio_wanted > 32768) audio_wanted = 32768;
    e->audio_cap = audio_wanted;
    e->audio_blocks = (IrAudioBlock **)calloc((size_t)e->audio_cap, sizeof(IrAudioBlock *));
    e->audio_head = e->audio_count = 0;
    e->mic_cap = max(audio_wanted / 2, 32);
    e->mic_blocks = (IrAudioBlock **)calloc((size_t)e->mic_cap, sizeof(IrAudioBlock *));
    e->mic_head = e->mic_count = 0;
    e->packet_cap = max(e->ring_seconds * max(e->fps, 1) + 16, 128);
    if (e->packet_cap > 8192) e->packet_cap = 8192;
    e->packets = (IrPacket **)calloc((size_t)e->packet_cap, sizeof(IrPacket *));
    e->packet_head = e->packet_count = 0;
    e->packet_bytes = 0;
}

// ---------------------------------------------------------------------------
// Save pipeline
// ---------------------------------------------------------------------------
static IrSaveJob *IrRetainJob(IrSaveJob *j)
{
    if (j) InterlockedIncrement(&j->refs);
    return j;
}

static void IrReleaseJob(IrSaveJob *j)
{
    if (!j || InterlockedDecrement(&j->refs) != 0) return;
    for (int i = 0; i < j->packet_count; ++i) IrPacketRelease(j->packets[i]);
    for (int i = 0; i < j->sys_count; ++i) IrAudioBlockRelease(j->sys[i]);
    for (int i = 0; i < j->mic_count; ++i) IrAudioBlockRelease(j->mic[i]);
    free(j->packets);
    free(j->sys);
    free(j->mic);
    free(j->seq_header);
    free(j);
}

static void IrFinishJob(IrSaveJob *j, int result, int progress)
{
    InterlockedExchange(&j->result, result);
    InterlockedExchange(&j->progress, progress);
    InterlockedExchange(&j->done, 1);
    InterlockedExchange(&g_ir_last_progress, progress);
}

// Exact 100 ns <-> 48 kHz conversions used by the replay timeline mapper.
static int64_t IrCeilFrames(int64_t ticks)
{
    if (ticks >= 0)
        return (ticks * (int64_t)IR_AUDIO_RATE + IR_SECOND_100NS - 1) / IR_SECOND_100NS;
    return -(((-ticks) * (int64_t)IR_AUDIO_RATE) / IR_SECOND_100NS);
}

typedef struct
{
    int64_t begin, end, out_base;
} IrTimeRange;

// Mix the system stereo and microphone mono rings into one continuous 48 kHz
// stereo buffer. Both sources are placed by timestamp; overlapping samples are
// summed and scaled only when they would clip.
static int16_t *IrMixRange(int64_t t0, int64_t t1,
                           IrAudioBlock **sys, int sys_count,
                           IrAudioBlock **mic, int mic_count,
                           int *out_frames)
{
    *out_frames = 0;
    int64_t n = (t1 - t0) * (int64_t)IR_AUDIO_RATE / IR_SECOND_100NS;
    if (n <= 0) return NULL;
    int32_t *acc = (int32_t *)calloc((size_t)n * IR_AUDIO_CHANNELS, sizeof(int32_t));
    if (!acc) return NULL;
    BOOL have_sys = FALSE, have_mic = FALSE;

    for (int side = 0; side < 2; ++side)
    {
        IrAudioBlock **blocks = side ? mic : sys;
        int count = side ? mic_count : sys_count;
        for (int b = 0; b < count; ++b)
        {
            IrAudioBlock *block = blocks[b];
            if (!block || !block->data || !block->frames) continue;
            int64_t first = (block->pts100ns - t0) * (int64_t)IR_AUDIO_RATE / IR_SECOND_100NS;
            const int16_t *pcm = (const int16_t *)block->data;
            for (uint32_t i = 0; i < block->frames; ++i)
            {
                int64_t at = first + (int64_t)i;
                if (at < 0 || at >= n) continue;
                if (side)
                {
                    acc[(size_t)at * 2] += pcm[i];
                    acc[(size_t)at * 2 + 1] += pcm[i];
                    have_mic = TRUE;
                }
                else
                {
                    acc[(size_t)at * 2] += pcm[(size_t)i * 2];
                    acc[(size_t)at * 2 + 1] += pcm[(size_t)i * 2 + 1];
                    have_sys = TRUE;
                }
            }
        }
    }

    int peak = 0;
    for (int64_t i = 0; i < n * (int64_t)IR_AUDIO_CHANNELS; ++i)
    {
        int v = acc[i] < 0 ? -acc[i] : acc[i];
        if (v > peak) peak = v;
    }
    int limit = (have_sys && have_mic) ? 30000 : 32767;
    double gain = peak > limit ? (double)limit / (double)peak : 1.0;

    int16_t *out = (int16_t *)malloc((size_t)n * IR_AUDIO_CHANNELS * sizeof(int16_t));
    if (out)
    {
        for (int64_t i = 0; i < n * (int64_t)IR_AUDIO_CHANNELS; ++i)
            out[i] = (int16_t)IrClamp16(gain < 1.0 ? (double)acc[i] * gain : (double)acc[i]);
        *out_frames = (int)n;
    }
    free(acc);
    return out;
}

// Maps an absolute packet timestamp into the output clip timeline.
static int64_t IrMapVideoPts(int64_t pts, BOOL segmented,
                             const int64_t *seg_starts, int nseg, int64_t end_time,
                             BOOL exact_cap, int64_t trim_start, int64_t first_pts,
                             int64_t out_end, int64_t frame_dur)
{
    if (segmented)
    {
        int si = 0;
        for (int i = 0; i < nseg; ++i)
        {
            if (seg_starts[i] <= pts) si = i;
            else break;
        }
        int64_t origin = seg_starts[si];
        int64_t rel = max((int64_t)0, pts - origin);
        int64_t next = si + 1 < nseg ? seg_starts[si + 1] : end_time;
        int64_t limit = min(IR_SECOND_100NS, max((int64_t)1, next - origin));
        if (limit > 0 && rel >= limit)
            rel = limit > frame_dur ? limit - frame_dur : 0;
        return clamp((int64_t)si * IR_SECOND_100NS + rel, (int64_t)0, out_end - 1);
    }
    if (exact_cap)
    {
        if (pts <= trim_start) return 0;
        return clamp(pts - trim_start, (int64_t)0, out_end - 1);
    }
    return max((int64_t)0, pts - first_pts);
}

static IrSaveJob *IrEnqueueSave(IrEngine *e, const wchar_t *path, int seconds_back)
{
    if (!e || !path || !*path) return NULL;
    IrSaveJob *j = (IrSaveJob *)calloc(1, sizeof(*j));
    if (!j) return NULL;
    wcsncpy_s(j->path, _countof(j->path), path, _TRUNCATE);
    j->seconds_back = seconds_back;
    j->refs = 1;
    j->result = IR_PENDING;
    j->progress = 0;
    j->config = e->config;
    j->fps = e->fps;
    j->segmented = (e->pref & 0x200) != 0;
    j->subtype = IrCodecSubtype((e->pref >> 15) & 3);
    j->width = e->encoded_width >= 2 ? e->encoded_width
                                     : (int)(e->config.VideoMaxWidth ? e->config.VideoMaxWidth : 1920);
    j->height = e->encoded_height >= 2 ? e->encoded_height
                                      : (int)(e->config.VideoMaxHeight ? e->config.VideoMaxHeight : 1080);
    if (j->width < 2) j->width = 2;
    if (j->height < 2) j->height = 2;

    EnterCriticalSection(&e->ring_cs);
    if (e->packet_count > 0)
    {
        int idx = (e->packet_head + e->packet_count - 1) % e->packet_cap;
        IrPacket *newest = e->packets[idx];
        if (newest) j->end_pts = newest->pts100ns + newest->duration;
    }
    j->packet_count = e->packet_count;
    if (e->packet_count > 0)
    {
        j->packets = (IrPacket **)malloc((size_t)e->packet_count * sizeof(IrPacket *));
        if (j->packets)
        {
            for (int i = 0; i < e->packet_count; ++i)
            {
                IrPacket *p = e->packets[(e->packet_head + i) % e->packet_cap];
                j->packets[i] = p;
                if (p) InterlockedIncrement(&p->refs);
            }
        }
        else j->packet_count = 0;
    }

    j->sys_count = e->audio_count;
    if (e->audio_count > 0)
    {
        j->sys = (IrAudioBlock **)malloc((size_t)e->audio_count * sizeof(IrAudioBlock *));
        if (j->sys)
        {
            for (int i = 0; i < e->audio_count; ++i)
            {
                IrAudioBlock *b = e->audio_blocks[(e->audio_head + i) % e->audio_cap];
                j->sys[i] = b;
                if (b) InterlockedIncrement(&b->refs);
            }
        }
        else j->sys_count = 0;
    }

    j->mic_count = e->mic_count;
    if (e->mic_count > 0)
    {
        j->mic = (IrAudioBlock **)malloc((size_t)e->mic_count * sizeof(IrAudioBlock *));
        if (j->mic)
        {
            for (int i = 0; i < e->mic_count; ++i)
            {
                IrAudioBlock *b = e->mic_blocks[(e->mic_head + i) % e->mic_cap];
                j->mic[i] = b;
                if (b) InterlockedIncrement(&b->refs);
            }
        }
        else j->mic_count = 0;
    }
    LeaveCriticalSection(&e->ring_cs);

    if (j->packet_count <= 0)
    {
        IrReleaseJob(j);
        return NULL;
    }

    EnterCriticalSection(&e->job_cs);
    j->id = e->next_job_id++;
    j->next = e->job_head;
    e->job_head = j;
    IrRetainJob(j);
    LeaveCriticalSection(&e->job_cs);
    SetEvent(e->save_wake);
    return j;
}

static int IrRunSave(IrEngine *e, IrSaveJob *j)
{
    (void)e;
    if (!j || j->packet_count <= 0) return IR_NOT_ENOUGH_DATA;

    IrPacket **pkts = (IrPacket **)malloc((size_t)j->packet_count * sizeof(IrPacket *));
    if (!pkts) { IrSetErrorFmt("Not enough memory to plan the replay."); return IR_LOW_MEMORY; }
    int np = 0;
    for (int i = 0; i < j->packet_count; ++i)
    {
        IrPacket *p = j->packets[i];
        if (p && p->data && p->bytes > p->codec_data) pkts[np++] = p;
    }
    if (!np) { free(pkts); return IR_NOT_ENOUGH_DATA; }

    int fps = max(j->fps, 1);
    int w = max(j->width, 2), h = max(j->height, 2);
    int64_t frame_dur = IR_SECOND_100NS / fps;
    int64_t requested = j->seconds_back > 0 ? (int64_t)j->seconds_back * IR_SECOND_100NS : 0;
    int64_t end_time = j->end_pts;
    int64_t trim_start = j->seconds_back > 0 ? end_time - requested : INT64_MIN;

    int *keys = (int *)malloc((size_t)np * sizeof(int));
    if (!keys) { free(pkts); IrSetErrorFmt("Not enough memory to plan the replay."); return IR_LOW_MEMORY; }
    int nk = 0;
    for (int i = 0; i < np; ++i)
        if (pkts[i]->key) keys[nk++] = i;

    int start = 0;
    if (j->seconds_back > 0)
    {
        int before = -1;
        for (int i = 0; i < np; ++i)
        {
            if (pkts[i]->pts100ns > trim_start) break;
            if (pkts[i]->key) before = i;
        }
        start = before >= 0 ? before : (nk ? keys[0] : 0);
    }

    int64_t *seg_starts = NULL;
    int nseg = 0;
    BOOL exact_cap = FALSE;
    if (j->segmented)
    {
        seg_starts = (int64_t *)malloc((size_t)max(nk, 1) * sizeof(int64_t));
        if (!seg_starts) { free(keys); free(pkts); IrSetErrorFmt("Not enough memory to plan the replay."); return IR_LOW_MEMORY; }
        int seg_first = 0;
        if (j->seconds_back > 0 && nk > 0)
        {
            int at_or_after = 0;
            while (at_or_after < nk && pkts[keys[at_or_after]]->pts100ns <= trim_start)
                ++at_or_after;
            seg_first = at_or_after > 0 ? at_or_after - 1 : 0;
            int needed = max(j->seconds_back, 1);
            if (nk - seg_first >= needed)
            {
                exact_cap = TRUE;
                start = keys[seg_first];
                for (int i = 0; i < needed; ++i)
                    seg_starts[nseg++] = pkts[keys[seg_first + i]]->pts100ns;
            }
            else
            {
                start = keys[seg_first];
                for (int i = seg_first; i < nk; ++i)
                    seg_starts[nseg++] = pkts[keys[i]]->pts100ns;
            }
        }
        else
        {
            for (int i = 0; i < nk; ++i)
            {
                if (keys[i] >= start)
                {
                    for (; i < nk; ++i) seg_starts[nseg++] = pkts[keys[i]]->pts100ns;
                    break;
                }
            }
        }
        if (!nseg) seg_starts[nseg++] = pkts[start]->pts100ns;
    }

    int64_t first_pts = pkts[start]->pts100ns;
    int64_t last_end = max(end_time, pkts[np - 1]->pts100ns + pkts[np - 1]->duration);
    if (last_end - first_pts < IR_SECOND_100NS / 2)
    {
        free(seg_starts); free(keys); free(pkts);
        return IR_NOT_ENOUGH_DATA;
    }
    if (!j->segmented && j->seconds_back > 0 &&
        end_time - first_pts >= requested - min(IR_SECOND_100NS, 2 * frame_dur))
        exact_cap = TRUE;
    int64_t out_end = (exact_cap && !j->segmented) ? requested
                    : (j->segmented ? (int64_t)nseg * IR_SECOND_100NS
                                    : max((int64_t)0, last_end - first_pts));
    if (out_end <= 0)
    {
        free(seg_starts); free(keys); free(pkts);
        return IR_NOT_ENOUGH_DATA;
    }

    BOOL av1 = IsEqualGUID(&j->subtype, &MFVideoFormat_AV1);
    BOOL hevc = IsEqualGUID(&j->subtype, &MFVideoFormat_HEVC);
    uint8_t *seq = NULL;
    uint32_t seq_size = 0;
    if (!av1)
    {
        for (int i = start; i < np; ++i)
        {
            if (!pkts[i]->key) continue;
            if (pkts[i]->codec_data)
            {
                seq_size = pkts[i]->codec_data;
                seq = (uint8_t *)malloc(seq_size);
                if (seq) { memcpy(seq, pkts[i]->data, seq_size); break; }
            }
            else
            {
                // Parameter sets were emitted in-band; recover them from the
                // Annex-B keyframe instead of rejecting the whole replay.
                seq = IrCollectParamSets(pkts[i]->data, pkts[i]->bytes, hevc, &seq_size);
                if (seq) break;
            }
        }
        if (!seq)
        {
            IrSetErrorFmt("%s", hevc ? "The buffered HEVC video has no parameter sets."
                                     : "The buffered H.264 video has no SPS/PPS.");
            free(seg_starts); free(keys); free(pkts);
            return IR_FAIL;
        }
    }

    // One range per one-second segment; the ring can hold thousands of them.
    // Allocate to the actual segment count instead of a fixed 64-entry cap.
    int range_cap = max(nseg, 1);
    IrTimeRange *ranges = (IrTimeRange *)malloc((size_t)range_cap * sizeof(IrTimeRange));
    if (!ranges)
    {
        IrSetErrorFmt("Not enough memory to plan the replay.");
        free(seg_starts); free(keys); free(pkts);
        return IR_LOW_MEMORY;
    }
    int nranges = 0;
    if (j->segmented)
    {
        for (int i = 0; i < nseg; ++i)
        {
            int64_t range_end = i + 1 < nseg ? seg_starts[i + 1] : end_time;
            if (range_end > seg_starts[i])
            {
                ranges[nranges].begin = seg_starts[i];
                ranges[nranges].end = range_end;
                ranges[nranges].out_base = (int64_t)i * IR_SECOND_100NS;
                ++nranges;
            }
        }
    }
    else if (exact_cap)
    {
        ranges[0].begin = trim_start;
        ranges[0].end = end_time;
        ranges[0].out_base = 0;
        nranges = 1;
    }
    else
    {
        ranges[0].begin = first_pts;
        ranges[0].end = last_end;
        ranges[0].out_base = 0;
        nranges = 1;
    }

    // Mix the replay audio before creating the writer so the MP4 only contains
    // an audio stream when there is actually PCM available to encode.
    int16_t *mixed = NULL;
    int mixed_frames = 0;
    int64_t mix_t0 = first_pts;
    if (j->sys_count + j->mic_count > 0)
    {
        int64_t t0 = first_pts;
        if (j->seconds_back > 0)
            t0 = max(first_pts, end_time - (int64_t)j->seconds_back * IR_SECOND_100NS - 6 * IR_SECOND_100NS);
        int64_t span = end_time > t0 ? end_time - t0 : 0;
        int64_t nframes64 = (span * (int64_t)IR_AUDIO_RATE + IR_SECOND_100NS - 1) / IR_SECOND_100NS;
        if (nframes64 > 0)
        {
            int64_t audio_end = t0 + nframes64 * IR_SECOND_100NS / IR_AUDIO_RATE;
            mixed = IrMixRange(t0, audio_end, j->sys, j->sys_count, j->mic, j->mic_count, &mixed_frames);
            mix_t0 = t0;
        }
    }

    int16_t *audio_pcm = NULL;
    uint32_t audio_frames_total = 0;
    int64_t audio_duration = 0;
    if (mixed && mixed_frames > 0)
    {
        int64_t total_frames = out_end * (int64_t)IR_AUDIO_RATE / IR_SECOND_100NS;
        int64_t aligned = ((total_frames + IR_AAC_FRAME - 1) / IR_AAC_FRAME) * IR_AAC_FRAME;
        if (aligned > 0)
        {
            audio_pcm = (int16_t *)calloc((size_t)aligned * IR_AUDIO_CHANNELS, sizeof(int16_t));
            if (audio_pcm)
            {
                for (int r = 0; r < nranges; ++r)
                {
                    int64_t out_span = j->segmented
                        ? max((int64_t)0, min(ranges[r].out_base + IR_SECOND_100NS, out_end) - ranges[r].out_base)
                        : max((int64_t)0, out_end - ranges[r].out_base);
                    int64_t out_frames = out_span * (int64_t)IR_AUDIO_RATE / IR_SECOND_100NS;
                    int64_t out_base_frames = ranges[r].out_base * (int64_t)IR_AUDIO_RATE / IR_SECOND_100NS;
                    if (out_frames <= 0) continue;
                    int64_t from = clamp(IrCeilFrames(ranges[r].begin - mix_t0), (int64_t)0, (int64_t)mixed_frames);
                    int64_t to = clamp(IrCeilFrames(ranges[r].end - mix_t0), (int64_t)0, (int64_t)mixed_frames);
                    if (to <= from) continue;
                    int64_t src_frames = to - from;
                    for (int64_t at = from; at < to; ++at)
                    {
                        int64_t dst = ((at - from) * out_frames + src_frames / 2) / src_frames;
                        if (dst < 0 || dst >= out_frames) continue;
                        int64_t d = out_base_frames + dst;
                        if (d < 0 || d >= aligned) continue;
                        memcpy(audio_pcm + (size_t)d * IR_AUDIO_CHANNELS,
                               mixed + (size_t)at * IR_AUDIO_CHANNELS,
                               IR_AUDIO_CHANNELS * sizeof(int16_t));
                    }
                }
                audio_frames_total = (uint32_t)aligned;
                audio_duration = aligned * IR_SECOND_100NS / IR_AUDIO_RATE;
            }
        }
    }

    BOOL want_audio = (audio_pcm && audio_frames_total > 0);
    IrMux mux;
    if (!IrMux_Create(&mux, j->path, &j->config, want_audio, &j->subtype,
                      seq, seq_size, w, h, fps))
    {
        IrSetErrorFmt("Could not create the MP4 writer for the replay.");
        free(ranges); free(audio_pcm); free(mixed); free(seq); free(seg_starts); free(keys); free(pkts);
        return IR_FAIL;
    }
    free(seq);

    typedef struct
    {
        int64_t pts;
        IrPacket *packet;
        int64_t duration;
    } IrVideoWrite;
    IrVideoWrite *vw = (IrVideoWrite *)malloc((size_t)(np - start) * sizeof(IrVideoWrite));
    if (!vw)
    {
        IrSetErrorFmt("Not enough memory to plan the replay.");
        free(ranges); free(mixed); free(audio_pcm); free(seg_starts); free(keys); free(pkts);
        IrMux_Finalize(&mux, j->path); // releases the writer before the file is removed
        DeleteFileW(j->path);
        return IR_LOW_MEMORY;
    }
    int nv = 0;
    int64_t previous = -1;
    for (int i = start; i < np; ++i)
    {
        int64_t pts = IrMapVideoPts(pkts[i]->pts100ns, j->segmented, seg_starts, nseg,
                                    end_time, exact_cap, trim_start, first_pts, out_end, frame_dur);
        if (pts <= previous) pts = previous + 1;
        previous = pts;
        vw[nv].pts = pts;
        vw[nv].packet = pkts[i];
        vw[nv].duration = frame_dur;
        ++nv;
    }
    for (int i = 0; i < nv; ++i)
    {
        if (!exact_cap && !j->segmented) continue;
        int64_t next = i + 1 < nv ? vw[i + 1].pts : out_end;
        vw[i].duration = clamp(next - vw[i].pts, (int64_t)1, out_end - vw[i].pts);
    }

    int result = IR_OK;
    if (audio_pcm && audio_frames_total)
    {
        if (!IrMux_WriteAudio(&mux, audio_pcm, audio_frames_total, audio_duration))
            result = IR_FAIL;
    }
    for (int i = 0; i < nv && result == IR_OK; ++i)
    {
        if (!IrMux_WriteVideo(&mux, vw[i].packet, vw[i].pts, vw[i].duration))
            result = IR_FAIL;
    }
    // Always finalize so the SinkWriter releases its file handle, even when a
    // sample write failed; otherwise DeleteFileW below cannot remove the file.
    if (!IrMux_Finalize(&mux, j->path) && result == IR_OK)
        result = IR_FAIL;
    if (result != IR_OK)
    {
        DeleteFileW(j->path);
        IrSetErrorFmt("Writing the replay file failed.");
    }

    free(vw);
    free(ranges);
    free(mixed);
    free(audio_pcm);
    free(seg_starts);
    free(keys);
    free(pkts);
    return result;
}

static DWORD WINAPI IrSaveThread(LPVOID arg)
{
    IrEngine *e = (IrEngine *)arg;
    CoInitializeEx(NULL, COINIT_MULTITHREADED);
    HANDLE waits[2] = { e->save_wake, e->save_stop };
    for (;;)
    {
        DWORD rc = WaitForMultipleObjects(2, waits, FALSE, INFINITE);
        if (rc == WAIT_OBJECT_0 + 1) break;
        if (rc != WAIT_OBJECT_0) continue;
        ResetEvent(e->save_wake);
        for (;;)
        {
            EnterCriticalSection(&e->job_cs);
            IrSaveJob *j = e->job_head;
            while (j && InterlockedCompareExchange(&j->result, IR_PENDING, IR_PENDING) != IR_PENDING)
                j = j->next;
            if (j) IrRetainJob(j);
            LeaveCriticalSection(&e->job_cs);
            if (!j) break;
            int result = IrRunSave(e, j);
            IrFinishJob(j, result, result == IR_OK ? 100 : 0);
            IrReleaseJob(j);
        }
    }
    CoUninitialize();
    return 0;
}

static void IrStopEngine(IrEngine *e)
{
    if (!e) return;
    InterlockedExchange(&e->running, 0);
    if (e->hb_stop)
    {
        SetEvent(e->hb_stop);
        if (e->hb_thread)
        {
            WaitForSingleObject(e->hb_thread, INFINITE);
            CloseHandle(e->hb_thread);
            e->hb_thread = NULL;
        }
        CloseHandle(e->hb_stop);
        e->hb_stop = NULL;
    }
    IrStopCapture(e);

    if (e->mic.running || e->mic.thread)
        IrMic_Stop(&e->mic);

    if (e->audio_running)
    {
        if (e->audio_stop)
        {
            SetEvent(e->audio_stop);
            if (e->audio_thread)
            {
                WaitForSingleObject(e->audio_thread, INFINITE);
                CloseHandle(e->audio_thread);
                e->audio_thread = NULL;
            }
            CloseHandle(e->audio_stop);
            e->audio_stop = NULL;
        }
        IrAudio_Flush(&e->audio);
        IrAudio_Stop(&e->audio);
        memset(&e->audio, 0, sizeof(e->audio));
        e->audio_running = FALSE;
    }

    // Flush and release the live encoder but keep the encoded packets and the
    // save thread alive: a compressed replay can be remuxed without D3D.
    if (e->live.mft)
    {
        IrLive_FlushEncoder(&e->live, e);
        IrLive_Release(&e->live);
    }

    // Do not free the rings here. They survive IR_Stop until the next IR_Start
    // or IR_Destroy so a save can still be initiated after capture has stopped.
    if (e->ctx)
    {
        ID3D11DeviceContext_Release(e->ctx);
        e->ctx = NULL;
    }
    if (e->dev)
    {
        ID3D11Device_Release(e->dev);
        e->dev = NULL;
    }
}

// ---------------------------------------------------------------------------
// Diagnostics helpers
// ---------------------------------------------------------------------------
typedef struct
{
    int count;
    WCHAR names[64][256];
} IrDeviceList;

static void IrGetMicDevices(IrDeviceList *list)
{
    memset(list, 0, sizeof(*list));
    HRESULT co = CoInitializeEx(NULL, COINIT_MULTITHREADED);
    IMMDeviceEnumerator *enumerator = NULL;
    if (FAILED(CoCreateInstance(&CLSID_MMDeviceEnumerator, NULL, CLSCTX_ALL,
                                &IID_IMMDeviceEnumerator, (void **)&enumerator)))
    {
        if (SUCCEEDED(co)) CoUninitialize();
        return;
    }
    IMMDeviceCollection *collection = NULL;
    if (SUCCEEDED(IMMDeviceEnumerator_EnumAudioEndpoints(enumerator, eCapture, DEVICE_STATE_ACTIVE, &collection)))
    {
        UINT count = 0;
        IMMDeviceCollection_GetCount(collection, &count);
        if (count > 64) count = 64;
        for (UINT i = 0; i < count; ++i)
        {
            IMMDevice *device = NULL;
            if (FAILED(IMMDeviceCollection_Item(collection, i, &device))) continue;
            IPropertyStore *store = NULL;
            if (SUCCEEDED(IMMDevice_OpenPropertyStore(device, STGM_READ, &store)))
            {
                PROPVARIANT value;
                PropVariantInit(&value);
                if (SUCCEEDED(IPropertyStore_GetValue(store, &PKEY_Device_FriendlyName, &value)) && value.pwszVal)
                    wcsncpy_s(list->names[list->count], 256, value.pwszVal, _TRUNCATE);
                else
                    _snwprintf_s(list->names[list->count], 256, _TRUNCATE, L"Microphone %u", i + 1);
                PropVariantClear(&value);
                IPropertyStore_Release(store);
                list->count++;
            }
            IMMDevice_Release(device);
        }
        IMMDeviceCollection_Release(collection);
    }
    IMMDeviceEnumerator_Release(enumerator);
    if (SUCCEEDED(co)) CoUninitialize();
}

static uint32_t IrProbeEncoderSet(void)
{
    HRESULT co = CoInitializeEx(NULL, COINIT_MULTITHREADED);
    MFStartup(MF_VERSION, MFSTARTUP_LITE);
    uint32_t mask = 0;
    const GUID *codecs[] = { &MFVideoFormat_H264, &MFVideoFormat_HEVC, &MFVideoFormat_AV1 };
    const uint32_t bits[3][5] = {
        { ENC_H264_MF_QSV, ENC_H264_MF_VP12, ENC_H264_MF_NVENC, ENC_H264_MF_AMF, ENC_H264_MF_SOFTWARE },
        { ENC_HEVC_MF_QSV, ENC_HEVC_MF_VP12, ENC_HEVC_MF_NVENC, ENC_HEVC_MF_AMF, ENC_HEVC_MF_SOFTWARE },
        { ENC_AV1_MF_QSV, ENC_AV1_MF_VP12, ENC_AV1_MF_NVENC, ENC_AV1_MF_AMF, ENC_AV1_MF_SOFTWARE },
    };
    const UINT flags[2] = {
        MFT_ENUM_FLAG_SORTANDFILTER | MFT_ENUM_FLAG_ASYNCMFT | MFT_ENUM_FLAG_HARDWARE,
        MFT_ENUM_FLAG_SORTANDFILTER | MFT_ENUM_FLAG_SYNCMFT,
    };
    for (int codec = 0; codec < 3; ++codec)
    {
        MFT_REGISTER_TYPE_INFO in = { MFMediaType_Video, MFVideoFormat_NV12 };
        MFT_REGISTER_TYPE_INFO out = { MFMediaType_Video, *codecs[codec] };
        for (int pass = 0; pass < 2; ++pass)
        {
            IMFActivate **activates = NULL;
            UINT32 count = 0;
            if (FAILED(MFTEnum2(MFT_CATEGORY_VIDEO_ENCODER, flags[pass], &in, &out, NULL, &activates, &count)))
                continue;
            for (UINT32 i = 0; i < count; ++i)
            {
                // IrActivateVendor returns 1..5 in QSV/VP12/NVENC/AMF/software
                // order, which maps directly to the availability bit index.
                int vendor = pass == 1 ? 5 : IrActivateVendor(activates[i]);
                if (vendor < 1 || vendor > 5) vendor = pass == 0 ? 1 : 5;
                mask |= bits[codec][vendor - 1];
            }
            if (activates)
            {
                for (UINT32 i = 0; i < count; ++i) IMFActivate_Release(activates[i]);
                CoTaskMemFree(activates);
            }
        }
    }
    if (SUCCEEDED(co)) CoUninitialize();
    return mask & ENC_ALL;
}

static int IrVendorFromPref(int pref)
{
    unsigned compact = ((unsigned)pref >> 24) & 0xff;
    int ordinal = pref & 0xff;
    if (compact & 0x10) return 30;
    if (compact & 0x20) return 31;
    if (compact & 0x40) return 32;
    if (compact & 0x80) return 33;
    switch (ordinal)
    {
    case 5: return 30;
    case 6: return 31;
    case 7: return 32;
    case 8: return 33;
    case 11: return 34;
    default: return 30;
    }
}

static void IrApplyAudioOptions(IrEngine *e, uint32_t options)
{
    static const DWORD bitrates[] = { 96, 128, 160, 192 };
    uint32_t index = (options >> IR_OPT_AUDIO_BITRATE_SHIFT) & IR_OPT_AUDIO_BITRATE_MASK;
    e->config.AudioBitrate = bitrates[index];
    uint32_t codec = (options >> IR_OPT_AUDIO_CODEC_SHIFT) & IR_OPT_AUDIO_CODEC_MASK;
    e->config.AudioCodec = codec == 1 ? IR_AUDIO_FLAC : IR_AUDIO_AAC;
}

// ---------------------------------------------------------------------------
// Public ABI
// ---------------------------------------------------------------------------
IR_EXPORT int __cdecl IR_Create(IntPtr *out_engine)
{
    IrEnsureGlobal();
    if (!out_engine) return IR_FAIL;
    *out_engine = NULL;
    IrEngine *e = (IrEngine *)calloc(1, sizeof(*e));
    if (!e) return IR_LOW_MEMORY;
    InitializeCriticalSection(&e->cs);
    InitializeCriticalSection(&e->ring_cs);
    InitializeCriticalSection(&e->job_cs);
    InitializeCriticalSection(&e->encoder_cs);
    IrConfigDefaults(&e->config);
    e->method = IR_CAPTURE_WGC;
    e->options = CAP_CLIENT_AREA | CAP_CURSOR | CAP_KEEP_CORNERS;
    e->fps = 60;
    e->ring_seconds = 60;
    e->save_wake = CreateEventW(NULL, TRUE, FALSE, NULL);
    e->save_stop = CreateEventW(NULL, TRUE, FALSE, NULL);
    if (!e->save_wake || !e->save_stop)
    {
        if (e->save_wake) CloseHandle(e->save_wake);
        if (e->save_stop) CloseHandle(e->save_stop);
        DeleteCriticalSection(&e->job_cs);
        DeleteCriticalSection(&e->encoder_cs);
        DeleteCriticalSection(&e->ring_cs);
        DeleteCriticalSection(&e->cs);
        free(e);
        return IR_FAIL;
    }
    e->next_job_id = 1;
    e->active_encoder = 30;
    *out_engine = (IntPtr)e;
    return IR_OK;
}

IR_EXPORT void __cdecl IR_Destroy(IntPtr engine)
{
    IrEngine *e = (IrEngine *)engine;
    if (!e) return;
    IrStopEngine(e);
    if (e->save_thread)
    {
        SetEvent(e->save_stop);
        WaitForSingleObject(e->save_thread, INFINITE);
        CloseHandle(e->save_thread);
        e->save_thread = NULL;
    }
    EnterCriticalSection(&e->job_cs);
    IrSaveJob *j = e->job_head;
    e->job_head = NULL;
    LeaveCriticalSection(&e->job_cs);
    while (j)
    {
        IrSaveJob *next = j->next;
        IrReleaseJob(j);
        j = next;
    }
    IrFreeRings(e);
    if (e->save_wake) CloseHandle(e->save_wake);
    if (e->save_stop) CloseHandle(e->save_stop);
    DeleteCriticalSection(&e->job_cs);
    DeleteCriticalSection(&e->encoder_cs);
    DeleteCriticalSection(&e->ring_cs);
    DeleteCriticalSection(&e->cs);
    free(e);
}

IR_EXPORT int __cdecl IR_Start(IntPtr engine, int monitor_index, int ring_seconds,
                               int fps, int bitrate_kbps, int cap_w, int cap_h,
                               int capture_audio, int capture_mic,
                               const wchar_t *mic_device, int encoder_preference,
                               int capture_method, int64_t *start_qpc_100ns)
{
    IrEnsureGlobal();
    IrEngine *e = (IrEngine *)engine;
    if (start_qpc_100ns) *start_qpc_100ns = 0;
    if (!e) return IR_FAIL;
    EnterCriticalSection(&e->cs);
    if (e->running)
        IrStopEngine(e);

    IrConfigDefaults(&e->config);
    e->monitor_index = max(monitor_index, 0);
    e->ring_seconds = clamp(ring_seconds, 1, 3600);
    e->fps = clamp(fps, 1, 240);
    e->bitrate_kbps = bitrate_kbps > 0 ? bitrate_kbps : 8000;
    e->config.VideoBitrate = (DWORD)e->bitrate_kbps;
    e->config.VideoMaxFramerate = (DWORD)e->fps;
    e->cap_w = cap_w > 0 ? (uint32_t)cap_w : 0;
    e->cap_h = cap_h > 0 ? (uint32_t)cap_h : 0;
    if (e->cap_w) e->config.VideoMaxWidth = e->cap_w;
    if (e->cap_h) e->config.VideoMaxHeight = e->cap_h;
    e->want_audio = capture_audio != 0;
    e->want_mic = capture_mic != 0;
    e->config.CaptureAudio = e->want_audio ? TRUE : FALSE;
    e->config.ApplicationLocalAudio = (e->options & CAP_LOCAL_AUDIO) ? TRUE : FALSE;
    e->config.MouseCursor = (e->options & CAP_CURSOR) ? TRUE : FALSE;
    e->config.OnlyClientArea = (e->options & CAP_CLIENT_AREA) ? TRUE : FALSE;
    e->config.ShowRecordingBorder = (e->options & CAP_BORDER) ? TRUE : FALSE;
    e->config.KeepRoundedWindowCorners = (e->options & CAP_KEEP_CORNERS) ? TRUE : FALSE;
    e->config.HardwareEncoder = (encoder_preference & IR_PREF_GPU_ENCODER) ? TRUE : FALSE;
    e->config.HardwarePreferIntegrated = (encoder_preference & IR_PREF_PREFER_IGPU) ? TRUE : FALSE;
    IrApplyAudioOptions(e, e->options);
    e->pref = encoder_preference;
    e->active_encoder = IrVendorFromPref(encoder_preference);
    e->method = (capture_method == IR_CAPTURE_DXGI || capture_method == IR_CAPTURE_EXTERNAL)
                ? capture_method : IR_CAPTURE_WGC;
    e->paused = 0;
    if (mic_device) wcsncpy_s(e->mic_device, _countof(e->mic_device), mic_device, _TRUNCATE);
    else e->mic_device[0] = L'\0';

    CoInitializeEx(NULL, COINIT_MULTITHREADED);
    LARGE_INTEGER now;
    QueryPerformanceCounter(&now);
    e->start_qpc = now.QuadPart;
    e->start_pts = IrQpcTo100ns(now.QuadPart);
    e->last_frame_pts = 0;
    e->last_frame_tick = 0;
    e->repeat_pts = 0;
    e->pause_start = 0;
    e->time_offset = 0;
    e->next_idr_pts = 0;
    e->last_encoder_width = 0;
    e->last_encoder_height = 0;
    e->encoded_width = 0;
    e->encoded_height = 0;
    InterlockedExchange(&e->force_idr, 1);
    e->captured = e->paced = e->rejected = 0;
    e->encoded = e->dropped = e->frames_encoded = 0;
    e->audio_frames = e->audio_events = e->audio_got = 0;
    e->heartbeat = e->last_pts = e->timing_frames = 0;
    e->timing_map_ms = e->timing_encode_ms = e->timing_render_ms = 0;
    e->timing_submit_ms = e->timing_snapshot_ms = e->timing_unmap_ms = 0;
    e->audio_convert_failures = 0;
    e->audio_peak = 0;
    if (start_qpc_100ns) *start_qpc_100ns = e->start_pts;

    HRESULT hr = IrCreateDevice(&e->dev, e->config.HardwareEncoder, e->config.HardwarePreferIntegrated);
    if (FAILED(hr))
    {
        InterlockedExchange(&e->fail_stage, STAGE_CREATE_DEVICE);
        InterlockedExchange(&e->fail_hr, (long)hr);
        IrReportHr(L"Cannot create D3D11 device", hr);
        LeaveCriticalSection(&e->cs);
        return IR_FAIL;
    }
    e->hw_init_hr = hr;
    ID3D11Device_GetImmediateContext(e->dev, &e->ctx);
    IrAllocateRings(e);

    if (!e->save_wake) e->save_wake = CreateEventW(NULL, TRUE, FALSE, NULL);
    if (!e->save_stop) e->save_stop = CreateEventW(NULL, TRUE, FALSE, NULL);
    if (!e->save_wake || !e->save_stop)
    {
        IrReportHr(L"Cannot create save events", HRESULT_FROM_WIN32(GetLastError()));
        goto fail;
    }
    ResetEvent(e->save_wake);
    ResetEvent(e->save_stop);
    if (!e->save_thread)
    {
        e->save_thread = CreateThread(NULL, 0, IrSaveThread, e, 0, NULL);
        if (!e->save_thread)
        {
            IrReportHr(L"Cannot create save thread", HRESULT_FROM_WIN32(GetLastError()));
            goto fail;
        }
    }

    if (e->want_audio)
    {
        HWND app_window = (e->config.ApplicationLocalAudio && IrAudio_CanCaptureApplicationLocal())
                          ? e->window : NULL;
        if (!IrAudio_Start(&e->audio, app_window))
        {
            InterlockedExchange(&e->fail_stage, STAGE_START_AUDIO);
            IrSetErrorWide(L"Cannot start system audio capture");
            goto fail;
        }
        e->audio_running = TRUE;
        e->audio_activate_hr = S_OK;
        e->audio_init_hr = S_OK;
        e->audio_start_hr = S_OK;
        DWORD format_size = sizeof(WAVEFORMATEX) + (e->audio.Format ? e->audio.Format->cbSize : 0);
        if (format_size > sizeof(e->audio_fmt)) format_size = sizeof(e->audio_fmt);
        if (e->audio.Format) memcpy(&e->audio_fmt, e->audio.Format, format_size);
        e->audio_tag = e->audio.Format ? e->audio.Format->wFormatTag : 0;
        e->audio_stop = CreateEventW(NULL, TRUE, FALSE, NULL);
        e->audio_thread = e->audio_stop ? CreateThread(NULL, 0, IrAudioPumpThread, e, 0, NULL) : NULL;
        if (!e->audio_thread)
        {
            IrSetErrorWide(L"Cannot create audio pump thread");
            goto fail;
        }
        IrPcm_Init(&e->audio_converter, e->audio.Format, IR_AUDIO_CHANNELS);
    }

    // Long-lived encoder feeding the compressed replay ring. Its output type is
    // fixed at creation time; IrLive_PrepareGpu rebuilds it if the first real
    // frame maps to a different size.
    {
        uint32_t init_w = e->config.VideoMaxWidth ? e->config.VideoMaxWidth : 1920;
        uint32_t init_h = e->config.VideoMaxHeight ? e->config.VideoMaxHeight : 1080;
        char create_error[192];
        create_error[0] = '\0';
        if (!IrLive_Create(&e->live, e, e->dev, init_w, init_h, e->fps,
                           e->bitrate_kbps, e->pref, create_error, sizeof(create_error)))
        {
            InterlockedExchange(&e->fail_stage, STAGE_START_ENCODER);
            IrSetErrorFmt("%s", create_error);
            goto fail;
        }
        int vendor = e->live.active_vendor;
        if (vendor < 1 || vendor > 5) vendor = max(IrVendorOrdinal(e->pref), 1);
        e->active_encoder = 29 + vendor;
    }

    // Microphone failure is fail-soft: capture and saves continue without it.
    if (e->want_mic)
        IrMic_Start(&e->mic, e, e->mic_device);

    hr = IrStartCapture(e);
    if (FAILED(hr))
    {
        InterlockedExchange(&e->fail_stage, STAGE_START_CAPTURE);
        InterlockedExchange(&e->fail_hr, (long)hr);
        IrReportHr(L"Cannot start capture", hr);
        goto fail;
    }
    e->hb_stop = CreateEventW(NULL, TRUE, FALSE, NULL);
    e->hb_thread = e->hb_stop ? CreateThread(NULL, 0, IrHeartbeatThread, e, 0, NULL) : NULL;
    if (!e->hb_thread)
    {
        IrSetErrorWide(L"Cannot create capture heartbeat thread");
        goto fail;
    }
    InterlockedExchange(&e->running, 1);
    InterlockedExchange(&e->fail_stage, STAGE_NONE);
    InterlockedExchange(&e->fail_hr, 0);
    InterlockedExchange(&g_ir_last_progress, -1);
    LeaveCriticalSection(&e->cs);
    return IR_OK;

fail:
    IrStopEngine(e);
    if (start_qpc_100ns) *start_qpc_100ns = 0;
    LeaveCriticalSection(&e->cs);
    return IR_FAIL;
}

IR_EXPORT void __cdecl IR_Stop(IntPtr engine)
{
    IrEngine *e = (IrEngine *)engine;
    if (!e) return;
    EnterCriticalSection(&e->cs);
    IrStopEngine(e);
    LeaveCriticalSection(&e->cs);
}

IR_EXPORT int __cdecl IR_LastErrorMessage(uint8_t *buffer, int capacity)
{
    IrEnsureGlobal();
    if (!buffer || capacity <= 0) return 0;
    EnterCriticalSection(&g_ir_error_cs);
    size_t len = strlen(g_ir_error);
    size_t copy = min((size_t)(capacity - 1), len);
    memcpy(buffer, g_ir_error, copy);
    buffer[copy] = 0;
    LeaveCriticalSection(&g_ir_error_cs);
    return (int)copy;
}

static void IrApplyOptions(IrEngine *e, uint32_t options)
{
    e->options = options;
    e->config.MouseCursor = (options & CAP_CURSOR) != 0;
    e->config.OnlyClientArea = (options & CAP_CLIENT_AREA) != 0;
    e->config.ShowRecordingBorder = (options & CAP_BORDER) != 0;
    e->config.KeepRoundedWindowCorners = (options & CAP_KEEP_CORNERS) != 0;
    e->config.IncludeSecondaryWindows = (options & CAP_SECONDARY) != 0;
    e->config.ApplicationLocalAudio = (options & CAP_LOCAL_AUDIO) != 0;
    IrApplyAudioOptions(e, options);
}

IR_EXPORT int __cdecl IR_SetCaptureTarget(IntPtr engine, IntPtr window, uint32_t options)
{
    IrEngine *e = (IrEngine *)engine;
    if (!e) return IR_FAIL;
    EnterCriticalSection(&e->cs);
    e->window = (HWND)window;
    IrApplyOptions(e, options);
    InterlockedExchange(&e->force_idr, 1);
    HRESULT hr = S_OK;
    if (e->running && (options & CAP_LIVE_SWITCH)) hr = IrStartCapture(e);
    LeaveCriticalSection(&e->cs);
    InterlockedExchange(&e->capture_hr, (long)hr);
    return SUCCEEDED(hr) ? IR_OK : IR_FAIL;
}

IR_EXPORT int __cdecl IR_SetCaptureSource(IntPtr engine, int method, int monitor_index,
                                          IntPtr window, uint32_t options)
{
    IrEngine *e = (IrEngine *)engine;
    if (!e) return IR_FAIL;
    EnterCriticalSection(&e->cs);
    e->method = method;
    e->monitor_index = max(monitor_index, 0);
    e->window = (HWND)window;
    IrApplyOptions(e, options);
    InterlockedExchange(&e->force_idr, 1);
    HRESULT hr = S_OK;
    if (e->running) hr = IrStartCapture(e);
    LeaveCriticalSection(&e->cs);
    InterlockedExchange(&e->capture_hr, (long)hr);
    return SUCCEEDED(hr) ? IR_OK : IR_FAIL;
}

IR_EXPORT int __cdecl IR_SetCanvasHint(IntPtr engine, int width, int height)
{
    IrEngine *e = (IrEngine *)engine;
    if (!e) return IR_FAIL;
    EnterCriticalSection(&e->cs);
    if (width > 0) e->config.VideoMaxWidth = (DWORD)width;
    if (height > 0) e->config.VideoMaxHeight = (DWORD)height;
    e->live.max_width = e->config.VideoMaxWidth;
    e->live.max_height = e->config.VideoMaxHeight;
    InterlockedExchange(&e->force_idr, 1);
    LeaveCriticalSection(&e->cs);
    return IR_OK;
}

IR_EXPORT int __cdecl IR_SetPaused(IntPtr engine, int paused)
{
    IrEngine *e = (IrEngine *)engine;
    if (!e) return IR_FAIL;
    EnterCriticalSection(&e->cs);
    int was = (int)InterlockedCompareExchange(&e->paused, 0, 0);
    if (paused && !was)
    {
        e->pause_start = IrNow100ns();
        InterlockedExchange(&e->paused, 1);
    }
    else if (!paused && was)
    {
        int64_t now = IrNow100ns();
        int64_t duration = max((int64_t)0, now - e->pause_start);
        InterlockedExchange(&e->paused, 0);
        if (!e->want_mic)
        {
            // Application audio and video never sample while paused, so drop
            // the wall-clock gap from their shared timeline. Static-frame
            // repeats use the same timeline and must skip the pause too.
            InterlockedExchangeAdd64(&e->time_offset, -duration);
            InterlockedExchangeAdd64(&e->repeat_pts, -duration);
        }
        else if (duration > 0 && e->packet_count > 0)
        {
            // Mic keeps sampling wall-clock time. Hold the last encoded image
            // for the whole pause so a save freezes the picture while the mic
            // continues to record into the gap.
            EnterCriticalSection(&e->ring_cs);
            int idx = (e->packet_head + e->packet_count - 1) % e->packet_cap;
            IrPacket *last = e->packets[idx];
            if (last) last->duration += duration;
            LeaveCriticalSection(&e->ring_cs);
        }
        e->pause_start = 0;
        InterlockedExchange(&e->force_idr, 1);
    }
    LeaveCriticalSection(&e->cs);
    return IR_OK;
}

typedef struct
{
    int index;
    int count;
    HWND result;
} IrWindowFind;

static BOOL CALLBACK IrWindowEnum(HWND window, LPARAM param)
{
    IrWindowFind *f = (IrWindowFind *)param;
    if (!IsWindowVisible(window) || GetParent(window)) return TRUE;
    wchar_t title[256];
    if (!GetWindowTextW(window, title, _countof(title)) || !title[0]) return TRUE;
    if (f->index++ == f->count)
    {
        f->result = window;
        return FALSE;
    }
    return TRUE;
}

static BOOL CALLBACK IrWindowCountEnum(HWND window, LPARAM param)
{
    int *count = (int *)param;
    if (!IsWindowVisible(window) || GetParent(window)) return TRUE;
    wchar_t title[256];
    if (!GetWindowTextW(window, title, _countof(title)) || !title[0]) return TRUE;
    (*count)++;
    return TRUE;
}

IR_EXPORT int __cdecl IR_GetWindowCount(void)
{
    int count = 0;
    EnumWindows(IrWindowCountEnum, (LPARAM)&count);
    return count;
}

IR_EXPORT void __cdecl IR_GetWindowInfo(int index, IntPtr *window, wchar_t *title, int title_capacity,
                                        int *width, int *height)
{
    if (window) *window = NULL;
    if (title && title_capacity > 0) title[0] = L'\0';
    if (width) *width = 0;
    if (height) *height = 0;
    if (index < 0) return;
    IrWindowFind f;
    f.index = 0;
    f.count = index;
    f.result = NULL;
    EnumWindows(IrWindowEnum, (LPARAM)&f);
    if (!f.result) return;
    if (window) *window = (IntPtr)f.result;
    if (title && title_capacity > 0)
        GetWindowTextW(f.result, title, title_capacity);
    RECT r;
    if (GetWindowRect(f.result, &r))
    {
        if (width) *width = (int)(r.right - r.left);
        if (height) *height = (int)(r.bottom - r.top);
    }
}

IR_EXPORT int __cdecl IR_GetMonitorCount(void)
{
    int count = 0;
    EnumDisplayMonitors(NULL, NULL, IrMonitorCountCallback, (LPARAM)&count);
    return count;
}

IR_EXPORT void __cdecl IR_GetMonitorInfo(int index, wchar_t *name, int name_capacity,
                                         int *width, int *height)
{
    if (name && name_capacity > 0) name[0] = L'\0';
    if (width) *width = 0;
    if (height) *height = 0;
    HMONITOR monitor = IrMonitorAtFixed(index);
    if (!monitor) return;
    MONITORINFOEXW mi;
    memset(&mi, 0, sizeof(mi));
    mi.cbSize = sizeof(mi);
    if (!GetMonitorInfoW(monitor, (LPMONITORINFO)&mi)) return;
    if (name && name_capacity > 0)
        wcsncpy_s(name, name_capacity, mi.szDevice, _TRUNCATE);
    if (width) *width = mi.rcMonitor.right - mi.rcMonitor.left;
    if (height) *height = mi.rcMonitor.bottom - mi.rcMonitor.top;
}

IR_EXPORT int __cdecl IR_BeginSave(IntPtr engine, const wchar_t *out_path, int seconds_back, uint64_t *job_id)
{
    IrEngine *e = (IrEngine *)engine;
    if (!e || !out_path || !job_id) return IR_FAIL;
    *job_id = 0;
    IrSaveJob *j = IrEnqueueSave(e, out_path, seconds_back);
    if (!j) return IR_LOW_MEMORY;
    *job_id = j->id;
    IrReleaseJob(j);
    return IR_OK;
}

IR_EXPORT int __cdecl IR_PollSave(IntPtr engine, uint64_t job_id, int *progress, int *result)
{
    IrEngine *e = (IrEngine *)engine;
    if (!e || !progress || !result) return IR_FAIL;
    *progress = -1;
    *result = IR_PENDING;
    EnterCriticalSection(&e->job_cs);
    IrSaveJob *j = e->job_head;
    while (j && j->id != job_id) j = j->next;
    if (j)
    {
        *progress = (int)j->progress;
        *result = j->done ? (int)j->result : IR_PENDING;
    }
    LeaveCriticalSection(&e->job_cs);
    return j ? IR_OK : IR_FAIL;
}

IR_EXPORT int __cdecl IR_Save(IntPtr engine, const wchar_t *out_path, int seconds_back)
{
    uint64_t job_id = 0;
    int rc = IR_BeginSave(engine, out_path, seconds_back, &job_id);
    if (rc != IR_OK) return rc;
    for (;;)
    {
        int progress = 0, result = IR_PENDING;
        rc = IR_PollSave(engine, job_id, &progress, &result);
        if (rc != IR_OK) return rc;
        if (result != IR_PENDING) return result;
        Sleep(25);
    }
}

IR_EXPORT int __cdecl IR_SaveProgress(void)
{
    IrEnsureGlobal();
    return (int)InterlockedCompareExchange(&g_ir_last_progress, 0, 0);
}

IR_EXPORT void __cdecl IR_GetStatus(IntPtr engine, int *is_buffering, int *ring_seconds,
                                    int64_t *frames_encoded, int64_t *frames_dropped,
                                    int *has_error, int64_t *audio_frames, int64_t *audio_events,
                                    int64_t *audio_got, int64_t *health_heartbeat)
{
    IrEngine *e = (IrEngine *)engine;
    if (is_buffering) *is_buffering = e && e->running ? 1 : 0;
    if (ring_seconds) *ring_seconds = e ? e->ring_seconds : 0;
    if (frames_encoded) *frames_encoded = e ? InterlockedCompareExchange64(&e->encoded, 0, 0) : 0;
    if (frames_dropped) *frames_dropped = e ? InterlockedCompareExchange64(&e->dropped, 0, 0) : 0;
    if (has_error) *has_error = e && InterlockedCompareExchange(&e->fail_stage, 0, 0) != 0 ? 1 : 0;
    if (audio_frames) *audio_frames = e ? InterlockedCompareExchange64(&e->audio_frames, 0, 0) : 0;
    if (audio_events) *audio_events = e ? InterlockedCompareExchange64(&e->audio_events, 0, 0) : 0;
    if (audio_got) *audio_got = e ? InterlockedCompareExchange64(&e->audio_got, 0, 0) : 0;
    if (health_heartbeat) *health_heartbeat = e ? InterlockedCompareExchange64(&e->heartbeat, 0, 0) : 0;
}

IR_EXPORT void __cdecl IR_GetTelemetry(IntPtr engine, int64_t *captured, int64_t *paced,
                                       int64_t *rejected, int64_t *encoded, int64_t *dropped,
                                       int *encoder, int64_t *last_pts)
{
    IrEngine *e = (IrEngine *)engine;
    if (captured) *captured = e ? InterlockedCompareExchange64(&e->captured, 0, 0) : 0;
    if (paced) *paced = e ? InterlockedCompareExchange64(&e->paced, 0, 0) : 0;
    if (rejected) *rejected = e ? InterlockedCompareExchange64(&e->rejected, 0, 0) : 0;
    if (encoded) *encoded = e ? InterlockedCompareExchange64(&e->encoded, 0, 0) : 0;
    if (dropped) *dropped = e ? InterlockedCompareExchange64(&e->dropped, 0, 0) : 0;
    if (encoder) *encoder = e ? e->active_encoder : 0;
    if (last_pts) *last_pts = e ? InterlockedCompareExchange64(&e->last_pts, 0, 0) : 0;
}

IR_EXPORT void __cdecl IR_GetTiming(IntPtr engine, double *map_ms, double *encode_ms,
                                    double *render_ms, double *submit_ms, double *snapshot_ms,
                                    double *unmap_ms, int64_t *frames)
{
    IrEngine *e = (IrEngine *)engine;
    int64_t n = e ? InterlockedCompareExchange64(&e->timing_frames, 0, 0) : 0;
    double div = n > 0 ? (double)n : 1.0;
    if (map_ms) *map_ms = e ? e->timing_map_ms / div : 0;
    if (encode_ms) *encode_ms = e ? e->timing_encode_ms / div : 0;
    if (render_ms) *render_ms = e ? e->timing_render_ms / div : 0;
    if (submit_ms) *submit_ms = e ? e->timing_submit_ms / div : 0;
    if (snapshot_ms) *snapshot_ms = e ? e->timing_snapshot_ms / div : 0;
    if (unmap_ms) *unmap_ms = e ? e->timing_unmap_ms / div : 0;
    if (frames) *frames = n;
}

IR_EXPORT void __cdecl IR_AudioDiagnostics(IntPtr engine, int which, int64_t *activate_hr,
                                            int64_t *init_hr, int64_t *start_hr, int *tag,
                                            int *rate, int *channels, int *bits, int *is_float,
                                            int *convert_failures, int64_t *last_fail_hr)
{
    IrEngine *e = (IrEngine *)engine;
    WAVEFORMATEX *format = NULL;
    if (e && which == 1)
    {
        IrMic *m = &e->mic;
        if (activate_hr) *activate_hr = (int64_t)m->activate_hr;
        if (init_hr) *init_hr = (int64_t)m->init_hr;
        if (start_hr) *start_hr = (int64_t)m->start_hr;
        format = m->format;
        if (convert_failures) *convert_failures = (int)InterlockedCompareExchange(&m->convert_failures, 0, 0);
        if (last_fail_hr) *last_fail_hr = (int64_t)m->fail_hr;
    }
    else
    {
        if (activate_hr) *activate_hr = e ? (int64_t)e->audio_activate_hr : 0;
        if (init_hr) *init_hr = e ? (int64_t)e->audio_init_hr : 0;
        if (start_hr) *start_hr = e ? (int64_t)e->audio_start_hr : 0;
        format = e ? e->audio.Format : NULL;
        if (convert_failures) *convert_failures = e ? (int)InterlockedCompareExchange(&e->audio_convert_failures, 0, 0) : 0;
        if (last_fail_hr) *last_fail_hr = e ? (int64_t)e->audio_start_hr : 0;
    }
    if (tag) *tag = format ? format->wFormatTag : 0;
    if (rate) *rate = format ? (int)format->nSamplesPerSec : 0;
    if (channels) *channels = format ? format->nChannels : 0;
    if (bits) *bits = format ? format->wBitsPerSample : 0;
    if (is_float) *is_float = format && IrIsFloatFormat(format) ? 1 : 0;
}

IR_EXPORT int __cdecl IR_IsDxgiSupported(void)
{
    return 1;
}

IR_EXPORT int __cdecl IR_DxgiDuplicationSupported(void)
{
    IrEnsureGlobal();
    ID3D11Device *dev = NULL;
    HRESULT hr = IrCreateDevice(&dev, FALSE, FALSE);
    if (FAILED(hr)) return 0;
    IDXGIDevice *dxgi = NULL;
    IDXGIAdapter *adapter = NULL;
    BOOL supported = FALSE;
    if (SUCCEEDED(ID3D11Device_QueryInterface(dev, &IID_IDXGIDevice, (void **)&dxgi)) &&
        SUCCEEDED(IDXGIDevice_GetAdapter(dxgi, &adapter)))
    {
        IDXGIOutput *output = NULL;
        if (SUCCEEDED(IDXGIAdapter_EnumOutputs(adapter, 0, &output)))
        {
            IDXGIOutput1 *output1 = NULL;
            if (SUCCEEDED(IDXGIOutput_QueryInterface(output, &IID_IDXGIOutput1, (void **)&output1)))
            {
                IDXGIOutputDuplication *dup = NULL;
                supported = SUCCEEDED(IDXGIOutput1_DuplicateOutput(output1, (IUnknown *)dev, &dup));
                if (dup) IDXGIOutputDuplication_Release(dup);
                IDXGIOutput1_Release(output1);
            }
            IDXGIOutput_Release(output);
        }
    }
    if (adapter) IDXGIAdapter_Release(adapter);
    if (dxgi) IDXGIDevice_Release(dxgi);
    ID3D11Device_Release(dev);
    return supported ? 1 : 0;
}

IR_EXPORT int __cdecl IR_ActiveCaptureMethod(IntPtr engine)
{
    IrEngine *e = (IrEngine *)engine;
    return e ? e->method : IR_CAPTURE_WGC;
}

IR_EXPORT int __cdecl IR_LastHresult(IntPtr engine)
{
    (void)engine;
    IrEnsureGlobal();
    return (int)InterlockedCompareExchange(&g_ir_last_hr, 0, 0);
}

IR_EXPORT int __cdecl IR_LastFailStage(IntPtr engine)
{
    IrEngine *e = (IrEngine *)engine;
    return e ? (int)InterlockedCompareExchange(&e->fail_stage, 0, 0) : STAGE_NONE;
}

IR_EXPORT int __cdecl IR_LastFailHr(IntPtr engine)
{
    IrEngine *e = (IrEngine *)engine;
    return e ? (int)InterlockedCompareExchange(&e->fail_hr, 0, 0) : 0;
}

IR_EXPORT int __cdecl IR_LastCaptureHr(IntPtr engine)
{
    IrEngine *e = (IrEngine *)engine;
    return e ? (int)InterlockedCompareExchange(&e->capture_hr, 0, 0) : 0;
}

IR_EXPORT int __cdecl IR_ActiveEncoder(IntPtr engine)
{
    IrEngine *e = (IrEngine *)engine;
    return e ? e->active_encoder : 0;
}

IR_EXPORT int __cdecl IR_HwInitHr(IntPtr engine)
{
    IrEngine *e = (IrEngine *)engine;
    return e ? (int)e->hw_init_hr : 0;
}

IR_EXPORT int __cdecl IR_GetMicDeviceCount(void)
{
    IrDeviceList list;
    IrGetMicDevices(&list);
    return list.count;
}

IR_EXPORT void __cdecl IR_GetMicDeviceName(int index, wchar_t *name, int name_capacity)
{
    if (name && name_capacity > 0) name[0] = L'\0';
    if (index < 0) return;
    IrDeviceList list;
    IrGetMicDevices(&list);
    if (index < list.count && name && name_capacity > 0)
        wcsncpy_s(name, name_capacity, list.names[index], _TRUNCATE);
}

IR_EXPORT void __cdecl IR_MicStatus(IntPtr engine, int64_t *mic_frames, int64_t *mic_events,
                                    int64_t *mic_got_frames, int64_t *mic_fail_hr,
                                    int *sys_peak, int *mic_peak, wchar_t *active_name, int name_capacity)
{
    IrEngine *e = (IrEngine *)engine;
    IrMic *m = e ? &e->mic : NULL;
    if (mic_frames) *mic_frames = m ? InterlockedCompareExchange64(&m->frames, 0, 0) : 0;
    if (mic_events) *mic_events = m ? InterlockedCompareExchange64(&m->events, 0, 0) : 0;
    if (mic_got_frames) *mic_got_frames = m ? InterlockedCompareExchange64(&m->got_frames, 0, 0) : 0;
    // The managed UI still interprets the legacy -1/-2/-3 codes; the real
    // HRESULT is exposed through IR_AudioDiagnostics(1).last_fail_hr.
    if (mic_fail_hr) *mic_fail_hr = m ? m->legacy_fail : 0;
    if (sys_peak) *sys_peak = e ? InterlockedCompareExchange(&e->audio_peak, 0, 0) : 0;
    if (mic_peak) *mic_peak = m ? InterlockedCompareExchange(&m->peak, 0, 0) : 0;
    if (active_name && name_capacity > 0)
    {
        active_name[0] = L'\0';
        if (m && m->name[0]) wcsncpy_s(active_name, (size_t)name_capacity, m->name, _TRUNCATE);
    }
}

IR_EXPORT const char *__cdecl IR_Version(void)
{
    return IR_VERSION_STRING;
}

IR_EXPORT uint32_t __cdecl IR_ProbeEncoders(void)
{
    return IrProbeEncoderSet();
}
