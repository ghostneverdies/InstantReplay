using System.Runtime.InteropServices;
using System.Text;
using Microsoft.Win32.SafeHandles;

namespace InstantReplay.Services;

public static class Logger
{
    private const uint FileAppendData = 0x0004;
    private const uint ShareAll = 0x0001 | 0x0002 | 0x0004;
    private const uint OpenAlways = 4;
    private const uint FileAttributeNormal = 0x80;

    private static readonly object s_lock = new();
    private static SafeFileHandle? s_handle;
    private static bool s_debugMode;

    public static string LogPath { get; private set; } = "";

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern SafeFileHandle CreateFileW(
        string fileName,
        uint desiredAccess,
        uint shareMode,
        IntPtr securityAttributes,
        uint creationDisposition,
        uint flagsAndAttributes,
        IntPtr templateFile);

    [DllImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool WriteFile(
        SafeFileHandle file,
        byte[] buffer,
        uint bytesToWrite,
        out uint bytesWritten,
        IntPtr overlapped);

    public static void Init(bool debugMode)
    {
        lock (s_lock)
        {
            s_debugMode = debugMode;
            string fileName = $"debug_{DateTime.Now:yyyyMMdd_HHmmss}.txt";
            LogPath = Path.Combine(Path.GetTempPath(), fileName);

            s_handle?.Dispose();
            s_handle = CreateFileW(LogPath, FileAppendData, ShareAll, IntPtr.Zero,
                OpenAlways, FileAttributeNormal, IntPtr.Zero);
            if (s_handle.IsInvalid)
            {
                int error = Marshal.GetLastWin32Error();
                s_handle = null;
                throw new IOException($"Unable to open debug log '{LogPath}' (Win32 {error}).");
            }
        }
        Info("Application Started");
        Info($"Version: {typeof(Logger).Assembly.GetName().Version?.ToString(3)}");
    }

    public static bool DebugMode => s_debugMode;

    public static void SetDebugMode(bool enabled)
    {
        s_debugMode = enabled;
        OverlayBridge.Refresh();
    }

    public static void Info(string message)
    {
        Write("INFO", message);
    }

    public static void Warn(string message)
    {
        Write("WARN", message);
    }

    public static void Error(string message, Exception? ex = null)
    {
        Write("ERROR", ex is null ? message : $"{message}\n{ex}");
    }


    public static void Debug(string message)
    {
        if (s_debugMode) Write("DEBUG", message);
    }

    private static void Write(string level, string message)
    {
        byte[] line = Encoding.UTF8.GetBytes(
            $"[{DateTime.Now:HH:mm:ss.fff}] [{level}] {message}\r\n");
        lock (s_lock)
        {
            if (s_handle is null || s_handle.IsInvalid) return;
            _ = WriteFile(s_handle, line, (uint)line.Length, out _, IntPtr.Zero);
        }
    }

    public static void Shutdown()
    {
        lock (s_lock)
        {
            s_handle?.Dispose();
            s_handle = null;
        }
    }
}
