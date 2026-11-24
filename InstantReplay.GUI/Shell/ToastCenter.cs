using System;
using System.Runtime.InteropServices;
using InstantReplay.Services;

namespace InstantReplay.GUI.Shell;

/// <summary>
/// The WinUI build raised Windows App SDK toast notifications. That SDK is not available to a .NET Framework 4.8
/// WPF app, so notifications are delivered as tray balloon tips (MainWindow.Notify already falls back to them).
/// The process AppUserModelID is still set so taskbar grouping and pinning behave as before.
/// </summary>
public static class ToastCenter
{
    public const string AumId = "InstantReplay.InstantReplay";

    private static readonly object s_lock = new();
    private static bool _initialized;

    public static bool IsAvailable => false;

    public static bool EnsureRegistered()
    {
        lock (s_lock)
        {
            if (_initialized) return false;
            _initialized = true;
            try
            {
                SetCurrentProcessExplicitAppUserModelID(AumId);
            }
            catch (Exception ex)
            {
                Logger.Warn($"Could not set the AppUserModelID: {ex.Message}");
            }
            return false;
        }
    }

    public static bool Show(string title, string message) => false;

    [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
    private static extern int SetCurrentProcessExplicitAppUserModelID([MarshalAs(UnmanagedType.LPWStr)] string appId);
}
