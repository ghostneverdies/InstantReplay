#pragma once

#include <stdint.h>

#if defined(_WIN32)
#define IR_API extern "C" __declspec(dllexport)
#else
#define IR_API extern "C"
#endif

#define IR_OK 0
#define IR_FAIL (-1)
#define IR_NOT_ENOUGH_DATA (-30)
#define IR_LOW_MEMORY (-31)
#define IR_PENDING (-999)

#define IR_CAPTURE_WGC 0
#define IR_CAPTURE_DXGI 1
#define IR_CAPTURE_EXTERNAL 3

#define IR_FAIL_NONE 0
#define IR_FAIL_DEVICE 1
#define IR_FAIL_CAPTURE 2
#define IR_FAIL_AUDIO 3
#define IR_FAIL_ENCODER 4
#define IR_FAIL_FRAME 5
#define IR_FAIL_SAVE 6

#define IR_ENC_HEVC_QSV 20
#define IR_ENC_HEVC_VP12 21
#define IR_ENC_HEVC_NVENC 22
#define IR_ENC_HEVC_AMF 23
#define IR_ENC_HEVC_SOFTWARE 24
#define IR_ENC_H264_QSV 30
#define IR_ENC_H264_VP12 31
#define IR_ENC_H264_NVENC 32
#define IR_ENC_H264_AMF 33
#define IR_ENC_H264_SOFTWARE 34
#define IR_ENC_AV1_QSV 40
#define IR_ENC_AV1_VP12 41
#define IR_ENC_AV1_NVENC 42
#define IR_ENC_AV1_AMF 43
#define IR_ENC_AV1_SOFTWARE 44

#define IR_ENC_H264_QSV_BIT (1u << 8)
#define IR_ENC_HEVC_QSV_BIT (1u << 9)
#define IR_ENC_H264_VP12_BIT (1u << 10)
#define IR_ENC_HEVC_VP12_BIT (1u << 11)
#define IR_ENC_H264_NVENC_BIT (1u << 12)
#define IR_ENC_HEVC_NVENC_BIT (1u << 13)
#define IR_ENC_H264_AMF_BIT (1u << 14)
#define IR_ENC_HEVC_AMF_BIT (1u << 15)
#define IR_ENC_AV1_QSV_BIT (1u << 16)
#define IR_ENC_AV1_VP12_BIT (1u << 17)
#define IR_ENC_AV1_NVENC_BIT (1u << 18)
#define IR_ENC_AV1_AMF_BIT (1u << 19)
#define IR_ENC_H264_SOFTWARE_BIT (1u << 20)
#define IR_ENC_HEVC_SOFTWARE_BIT (1u << 21)
#define IR_ENC_AV1_SOFTWARE_BIT (1u << 22)

#define IR_CAP_CLIENT_AREA 0x00000001u
#define IR_CAP_CURSOR 0x00000002u
#define IR_CAP_BORDER 0x00000004u
#define IR_CAP_ROUNDED 0x00000008u
#define IR_CAP_SECONDARY 0x00000010u
#define IR_CAP_PROCESS_AUDIO 0x00000040u
#define IR_CAP_LIVE_SWITCH 0x00000080u

IR_API int IR_Create(void **out_engine);
IR_API void IR_Destroy(void *engine);

IR_API int IR_Start(void *engine,
                    int monitor_index,
                    int ring_seconds,
                    int fps,
                    int bitrate_kbps,
                    int cap_w,
                    int cap_h,
                    int capture_audio,
                    int capture_mic,
                    const wchar_t *mic_device,
                    int encoder_preference,
                    int capture_method,
                    int64_t *start_qpc_100ns);

IR_API void IR_Stop(void *engine);
IR_API int IR_SetCaptureTarget(void *engine, void *window, uint32_t options);
IR_API int IR_SetCaptureSource(void *engine, int method, int monitor_index, void *window, uint32_t options);
IR_API int IR_SetCanvasHint(void *engine, int width, int height);
IR_API int IR_SetPaused(void *engine, int paused);

IR_API int IR_GetWindowCount(void);
IR_API void IR_GetWindowInfo(int index, void **window, wchar_t *title, int title_capacity, int *width, int *height);

IR_API int IR_Save(void *engine, const wchar_t *out_path, int seconds_back);
IR_API int IR_BeginSave(void *engine, const wchar_t *out_path, int seconds_back, uint64_t *job_id);
IR_API int IR_PollSave(void *engine, uint64_t job_id, int *progress, int *result);
IR_API int IR_SaveProgress(void);

IR_API void IR_GetStatus(void *engine,
                         int *is_buffering,
                         int *ring_seconds,
                         int64_t *frames_encoded,
                         int64_t *frames_dropped,
                         int *has_error,
                         int64_t *audio_frames,
                         int64_t *audio_events,
                         int64_t *audio_got_frames,
                         int64_t *health_heartbeat);

IR_API void IR_GetTelemetry(void *engine,
                            int64_t *captured,
                            int64_t *paced,
                            int64_t *rejected,
                            int64_t *encoded,
                            int64_t *dropped,
                            int *encoder,
                            int64_t *last_pts);

IR_API void IR_GetTiming(void *engine,
                         double *map_ms,
                         double *encode_ms,
                         double *render_ms,
                         double *submit_ms,
                         double *snapshot_ms,
                         double *unmap_ms,
                         int64_t *frames);

IR_API void IR_AudioDiagnostics(void *engine,
                                int which,
                                int64_t *activate_hr,
                                int64_t *init_hr,
                                int64_t *start_hr,
                                int *tag,
                                int *rate,
                                int *channels,
                                int *bits,
                                int *is_float,
                                int *convert_failures,
                                int64_t *last_fail_hr);

IR_API int IR_GetMonitorCount(void);
IR_API void IR_GetMonitorInfo(int index, wchar_t *name, int name_capacity, int *width, int *height);
IR_API int IR_IsDxgiSupported(void);
IR_API int IR_DxgiDuplicationSupported(void);

IR_API int IR_ActiveCaptureMethod(void *engine);
IR_API int IR_LastHresult(void *engine);
IR_API int IR_LastFailStage(void *engine);
IR_API int IR_LastFailHr(void *engine);
IR_API int IR_LastCaptureHr(void *engine);
IR_API int IR_ActiveEncoder(void *engine);
IR_API int IR_HwInitHr(void *engine);

IR_API int IR_GetMicDeviceCount(void);
IR_API void IR_GetMicDeviceName(int index, wchar_t *name, int name_capacity);

IR_API void IR_MicStatus(void *engine,
                         int64_t *mic_frames,
                         int64_t *mic_events,
                         int64_t *mic_got_frames,
                         int64_t *mic_fail_hr,
                         int *sys_peak,
                         int *mic_peak,
                         wchar_t *active_name,
                         int name_capacity);

IR_API const char *IR_Version(void);
IR_API uint32_t IR_ProbeEncoders(void);
IR_API int IR_LastErrorMessage(uint8_t *buffer, int capacity);
