#pragma once

// This is the only translation unit in the native engine. Define GUID storage
// here so DEFINE_GUID constants from the SDK and from this header get emitted.
#ifndef INITGUID
#define INITGUID
#endif

// InstantReplay native engine common prelude (C11 / MSVC).
//
// This header contains only C-compatible declarations shared by the single
// engine translation unit.  All public entry points use the cdecl ABI declared
// by InstantReplay.Native.IrNative.

#ifndef _WIN32_WINNT
#define _WIN32_WINNT 0x0A00
#endif
#ifndef NTDDI_VERSION
// Windows 10 Fall Creators Update: process-loopback audio, MFTEnum2 and the
// placeholder-backed audio ring APIs are all declared at this level.
#define NTDDI_VERSION 0x0A00000A
#endif
#ifndef UNICODE
#define UNICODE
#endif
#ifndef _UNICODE
#define _UNICODE
#endif
#ifndef COBJMACROS
#define COBJMACROS
#endif
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef _CRT_SECURE_NO_WARNINGS
#define _CRT_SECURE_NO_WARNINGS
#endif

#include <initguid.h>
#include <windows.h>

#include <stdatomic.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wchar.h>
#include <intrin.h>
#include <math.h>
#include <float.h>
#include <limits.h>

#define IR_EXPORT __declspec(dllexport)

#define IR_OK                0
#define IR_FAIL              (-1)
#define IR_NOT_ENOUGH_DATA   (-30)
#define IR_LOW_MEMORY        (-31)
#define IR_PENDING           (-999)

#define IR_CAPTURE_WGC       0
#define IR_CAPTURE_DXGI      1
#define IR_CAPTURE_EXTERNAL  3

#define CAP_CLIENT_AREA      0x00000001u
#define CAP_CURSOR           0x00000002u
#define CAP_BORDER           0x00000004u
#define CAP_KEEP_CORNERS     0x00000008u
#define CAP_SECONDARY        0x00000010u
#define CAP_LOCAL_AUDIO      0x00000040u
#define CAP_LIVE_SWITCH      0x00000080u

#define ENC_H264_MF_QSV      (1u << 8)
#define ENC_HEVC_MF_QSV      (1u << 9)
#define ENC_H264_MF_VP12     (1u << 10)
#define ENC_HEVC_MF_VP12     (1u << 11)
#define ENC_H264_MF_NVENC    (1u << 12)
#define ENC_HEVC_MF_NVENC    (1u << 13)
#define ENC_H264_MF_AMF      (1u << 14)
#define ENC_HEVC_MF_AMF      (1u << 15)
#define ENC_AV1_MF_QSV       (1u << 16)
#define ENC_AV1_MF_VP12      (1u << 17)
#define ENC_AV1_MF_NVENC     (1u << 18)
#define ENC_AV1_MF_AMF       (1u << 19)
#define ENC_H264_MF_SOFTWARE (1u << 20)
#define ENC_HEVC_MF_SOFTWARE (1u << 21)
#define ENC_AV1_MF_SOFTWARE  (1u << 22)
#define ENC_ALL              ((1u << 23) - 1u)

#define STAGE_NONE             0
#define STAGE_CREATE_DEVICE    1
#define STAGE_START_CAPTURE    2
#define STAGE_START_AUDIO      3
#define STAGE_START_ENCODER    4
#define STAGE_FRAME            5
#define STAGE_SAVE             6

#define IR_TITLE              L"InstantReplay"
#define IR_VERSION_STRING     "InstantReplay native engine 5.0"

#ifdef _DEBUG
#define Assert(Cond) do { if (!(Cond)) __debugbreak(); } while (0)
#else
#define Assert(Cond) ((void)(Cond))
#endif

#define MUL_DIV_ROUND_UP(X, Num, Den) (((X) * (Num) - 1) / (Den) + 1)
#define DIV_ROUND_UP(X, Y) (((X) + (Y) - 1) / (Y))

// Media Foundation timestamps are expressed in 100ns units.
#define MF_UNITS_PER_SECOND 10000000ULL

typedef struct IrEngine IrEngine;

typedef struct
{
    BOOL MouseCursor;
    BOOL OnlyClientArea;
    BOOL ShowRecordingBorder;
    BOOL KeepRoundedWindowCorners;
    BOOL IncludeSecondaryWindows;
    BOOL HardwareEncoder;
    BOOL HardwarePreferIntegrated;
    WCHAR OutputFolder[MAX_PATH];
    BOOL OpenFolder;
    BOOL FragmentedOutput;
    BOOL EnableLimitLength;
    BOOL EnableLimitSize;
    DWORD LimitLength;
    DWORD LimitSize;
    BOOL GammaCorrectResize;
    BOOL ImprovedColorConversion;
    DWORD VideoCodec;
    DWORD VideoProfile;
    DWORD VideoMaxWidth;
    DWORD VideoMaxHeight;
    DWORD VideoMaxFramerate;
    DWORD VideoBitrate;
    BOOL CaptureAudio;
    BOOL ApplicationLocalAudio;
    DWORD AudioCodec;
    DWORD AudioChannels;
    DWORD AudioSamplerate;
    DWORD AudioBitrate;
    DWORD ShortcutMonitor;
    DWORD ShortcutWindow;
    DWORD ShortcutRegion;
} Config;

static void IrConfigDefaults(Config *c)
{
    memset(c, 0, sizeof(*c));
    c->MouseCursor = TRUE;
    c->OnlyClientArea = TRUE;
    c->ShowRecordingBorder = TRUE;
    c->KeepRoundedWindowCorners = TRUE;
    c->IncludeSecondaryWindows = FALSE;
    c->HardwareEncoder = TRUE;
    c->HardwarePreferIntegrated = FALSE;
    c->FragmentedOutput = FALSE;
    c->EnableLimitLength = FALSE;
    c->EnableLimitSize = FALSE;
    c->LimitLength = 60;
    c->LimitSize = 25;
    c->GammaCorrectResize = FALSE;
    c->ImprovedColorConversion = FALSE;
    c->VideoCodec = 0;  // H.264
    c->VideoProfile = 2; // High
    c->VideoMaxWidth = 1920;
    c->VideoMaxHeight = 1080;
    c->VideoMaxFramerate = 60;
    c->VideoBitrate = 8000;
    c->CaptureAudio = TRUE;
    c->ApplicationLocalAudio = TRUE;
    c->AudioCodec = 0;  // AAC
    c->AudioChannels = 2;
    c->AudioSamplerate = 48000;
    c->AudioBitrate = 160;
}

static void IrSetLastHr(HRESULT hr);
static void IrSetErrorUtf8(const char *text);
static void IrSetErrorWide(const wchar_t *text);
static void IrSetErrorFmt(const char *fmt, ...);
static void IrReportHr(const wchar_t *what, HRESULT hr);
static int64_t IrNow100ns(void);
static int64_t IrQpcTo100ns(int64_t qpc);

#define HR(hr) do { HRESULT ir_hr_ = (HRESULT)(hr); Assert(SUCCEEDED(ir_hr_)); if (FAILED(ir_hr_)) IrSetLastHr(ir_hr_); } while (0)
