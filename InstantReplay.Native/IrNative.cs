using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text;

namespace InstantReplay.Native;

internal static class IrNative
{
    internal const int OK = 0;
    internal const int Fail = -1;
    internal const int NotEnoughData = -30;
    internal const int LowMemory = -31;
    internal const int SavePending = -999;

    private const string Dll = "engine";

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr LoadLibraryW(string fileName);

    static IrNative()
    {
        try
        {
            string baseDir = AppContext.BaseDirectory;
            foreach (string candidate in new[]
            {
                Path.Combine(baseDir, "engine", "bin", "engine.dll"),
                Path.Combine(baseDir, "engine.dll"),
            })
            {
                if (File.Exists(candidate) && LoadLibraryW(candidate) != IntPtr.Zero) return;
            }
        }
        catch (Exception)
        {
        }
    }

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_Create(out IntPtr engine);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void IR_Destroy(IntPtr engine);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_Start(IntPtr engine, int monitorIndex, int ringSeconds,
        int fps, int bitrateKbps, int capW, int capH, int captureAudio, int captureMic,
        [MarshalAs(UnmanagedType.LPWStr)] string micDevice,
        int encoderPreference, int captureMethod, out long startQpc100ns);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_LastErrorMessage([Out] byte[] buffer, int capacity);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void IR_Stop(IntPtr engine);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_SetCaptureTarget(IntPtr engine, IntPtr window, uint options);
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_SetCaptureSource(IntPtr engine, int method, int monitorIndex,
        IntPtr window, uint options);
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_SetCanvasHint(IntPtr engine, int width, int height);
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_SetPaused(IntPtr engine, int paused);
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_GetWindowCount();
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void IR_GetWindowInfo(int index, out IntPtr window,
        [MarshalAs(UnmanagedType.LPWStr)] StringBuilder title, int titleCapacity, out int width, out int height);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_Save(IntPtr engine,
        [MarshalAs(UnmanagedType.LPWStr)] string outPath, int secondsBack);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_BeginSave(IntPtr engine,
        [MarshalAs(UnmanagedType.LPWStr)] string outPath, int secondsBack, out ulong jobId);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_PollSave(IntPtr engine, ulong jobId, out int progress, out int result);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_SaveProgress();

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void IR_GetStatus(IntPtr engine, out int isBuffering,
        out int ringSeconds, out long framesEncoded, out long framesDropped,
        out int hasError, out long audioFrames, out long audioEvents,
        out long audioGotFrames, out long healthHeartbeat);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void IR_GetTelemetry(IntPtr engine, out long captured,
        out long paced, out long rejected, out long encoded, out long dropped,
        out int encoder, out long lastPts);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void IR_GetTiming(IntPtr engine, out double mapMs,
        out double encodeMs, out double renderMs, out double submitMs,
        out double snapshotMs, out double unmapMs, out long frames);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void IR_AudioDiagnostics(IntPtr engine, int which,
        out long activateHr, out long initHr, out long startHr, out int tag,
        out int rate, out int channels, out int bits, out int isFloat,
        out int convertFailures, out long lastFailHr);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_GetMonitorCount();

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void IR_GetMonitorInfo(int index,
        [MarshalAs(UnmanagedType.LPWStr)] StringBuilder name, int nameCap,
        out int width, out int height);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_IsDxgiSupported();

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_DxgiDuplicationSupported();

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_ActiveCaptureMethod(IntPtr engine);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_LastHresult(IntPtr engine);
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_LastFailStage(IntPtr engine);
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_LastFailHr(IntPtr engine);
    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_LastCaptureHr(IntPtr engine);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_ActiveEncoder(IntPtr engine);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_HwInitHr(IntPtr engine);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern int IR_GetMicDeviceCount();

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void IR_GetMicDeviceName(int index,
        [MarshalAs(UnmanagedType.LPWStr)] StringBuilder name, int nameCap);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void IR_MicStatus(IntPtr engine, out long micFrames, out long micEvents,
        out long micGotFrames, out long micFailHr, out int sysPeak, out int micPeak,
        [MarshalAs(UnmanagedType.LPWStr)] StringBuilder activeName, int nameCap);

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern IntPtr IR_Version();

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern uint IR_ProbeEncoders();

    internal static string Version()
    {
        try { return Marshal.PtrToStringAnsi(IR_Version()) ?? ""; }
        catch { return ""; }
    }
}

[ComImport, Guid("BCDE0395-E52F-467C-8E3D-C4579291692E")]
internal class MMDeviceEnumeratorComObject
{
}

[ComImport, Guid("A95664D2-9614-4F35-A746-DE8DB63617E6"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
internal interface IMMDeviceEnumerator
{
    int EnumAudioEndpoints(int dataFlow, int stateMask, out IntPtr devices);
    int GetDefaultAudioEndpoint(int dataFlow, int role, out IntPtr endpoint);
    int GetDevice([MarshalAs(UnmanagedType.LPWStr)] string id, out IntPtr device);
    int RegisterEndpointNotificationCallback(IMMNotificationClient client);
    int UnregisterEndpointNotificationCallback(IMMNotificationClient client);
}

[StructLayout(LayoutKind.Sequential)]
internal struct PropertyKey
{
    public Guid FormatId;
    public int PropertyId;
}

[ComImport, Guid("7991EEC9-7E89-4D85-8390-6C703CEC60C0"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
internal interface IMMNotificationClient
{
    void OnDeviceStateChanged([MarshalAs(UnmanagedType.LPWStr)] string deviceId, int newState);
    void OnDeviceAdded([MarshalAs(UnmanagedType.LPWStr)] string deviceId);
    void OnDeviceRemoved([MarshalAs(UnmanagedType.LPWStr)] string deviceId);
    void OnDefaultDeviceChanged(int flow, int role, [MarshalAs(UnmanagedType.LPWStr)] string deviceId);
    void OnPropertyValueChanged([MarshalAs(UnmanagedType.LPWStr)] string deviceId, PropertyKey key);
}
