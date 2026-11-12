using System.Reflection;
using System.Runtime.InteropServices;
using System.Text;

namespace InstantReplay.Native;

internal static class IrNative
{
    internal const int OK = 0;
    internal const int Fail = -1;
    internal const int NotEnoughData = -30;

    private const string Dll = "engine";

    static IrNative()
    {
        try
        {
            NativeLibrary.SetDllImportResolver(typeof(IrNative).Assembly, ResolveEngine);
        }
        catch (InvalidOperationException)
        {
        }
    }

    private static IntPtr ResolveEngine(string name, Assembly assembly, DllImportSearchPath? path)
    {
        if (!string.Equals(name, Dll, StringComparison.OrdinalIgnoreCase)) return IntPtr.Zero;

        string baseDir = AppContext.BaseDirectory;
        foreach (string candidate in new[]
        {
            Path.Combine(baseDir, "engine", "bin", "engine.dll"),
            Path.Combine(baseDir, "engine.dll"),
        })
        {
            if (File.Exists(candidate)) return NativeLibrary.Load(candidate);
        }
        return IntPtr.Zero;
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
    internal static extern int IR_SaveProgress();

    [DllImport(Dll, CallingConvention = CallingConvention.Cdecl)]
    internal static extern void IR_GetStatus(IntPtr engine, out int isBuffering,
        out int ringSeconds, out long framesEncoded, out long framesDropped,
        out int hasError, out long audioFrames, out long audioEvents,
        out long audioGotFrames, out long healthHeartbeat);

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

    internal static string Version()
    {
        try { return Marshal.PtrToStringAnsi(IR_Version()) ?? ""; }
        catch { return ""; }
    }
}
