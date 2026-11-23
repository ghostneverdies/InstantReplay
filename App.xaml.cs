global using System;
global using System.Collections.Generic;
global using System.Collections.ObjectModel;
global using System.ComponentModel;
global using System.IO;
global using System.Runtime.InteropServices;
global using System.Text;
global using System.Linq;
global using System.Net.Http;
global using System.Threading;
global using System.Threading.Tasks;
global using System.Windows;
global using System.Windows.Controls;
global using System.Windows.Input;
global using System.Windows.Media;
global using System.Windows.Media.Animation;
global using System.Windows.Threading;
global using static InstantReplay.Compat;

using System.Diagnostics;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Windows.Interop;
using InstantReplay.GUI;
using InstantReplay.Services;

namespace InstantReplay
{
public partial class App : System.Windows.Application
{
    private const string SingleInstanceMutexName = "Global\\InstantReplay_SingleInstance_Mutex";
    private const string ShowWindowEventName = "Global\\InstantReplay_ShowWindow_Event";

    // Debug runs must never hand off to a hidden instance built from older
    // code. A per-process name keeps `dotnet run` deterministic while Release
    // keeps the normal single-instance behavior.
    private static string EffectiveMutexName
    {
        get
        {
#if DEBUG
            return $"Global\\InstantReplay_SingleInstance_Mutex_Dev_{Process.GetCurrentProcess().Id}";
#else
            return SingleInstanceMutexName;
#endif
        }
    }

    private static string EffectiveShowEventName
    {
        get
        {
#if DEBUG
            return $"Global\\InstantReplay_ShowWindow_Event_Dev_{Process.GetCurrentProcess().Id}";
#else
            return ShowWindowEventName;
#endif
        }
    }

    private Window? _window;
    private Mutex? _singleInstanceMutex;
    private EventWaitHandle? _showWindowEvent;

    public App()
    {
        try
        {
            string webViewData = Path.Combine(Path.GetTempPath(), "InstantReplay", "WebView2");
                _ = Directory.CreateDirectory(webViewData);
            Environment.SetEnvironmentVariable("WEBVIEW2_USER_DATA_FOLDER", webViewData);
        }
        catch { }
    }

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);

        DispatcherUnhandledException += (_, args) =>
        {
            Logger.Error($"Unhandled exception: {args.Exception}", args.Exception);
            args.Handled = true;
        };

        _singleInstanceMutex = new Mutex(initiallyOwned: false, EffectiveMutexName);
        bool acquired;
        try { acquired = _singleInstanceMutex.WaitOne(0); }
        catch (AbandonedMutexException) { acquired = true; }

        if (!acquired)
        {
            Logger.Info("Another Instant Replay instance is already running; signaling it and exiting silently.");
            try
            {
                for (int attempt = 0; attempt < 40; attempt++)
                {
                    try
                    {
                        using var existingShowEvent = EventWaitHandle.OpenExisting(EffectiveShowEventName);
                            _ = existingShowEvent.Set();
                        break;
                    }
                    catch (WaitHandleCannotBeOpenedException) when (attempt < 39)
                    {
                        Thread.Sleep(50);
                    }
                }
            }
            catch (Exception ex)
            {
                Logger.Warn($"Could not signal existing instance: {ex.Message}");
            }

            Shutdown(0);
            return;
        }

        _showWindowEvent = new EventWaitHandle(initialState: false, EventResetMode.AutoReset, EffectiveShowEventName);

        bool startMinimized = Environment.GetCommandLineArgs()
            .Any(a => string.Equals(a, "--tray", StringComparison.OrdinalIgnoreCase));

        _window = new MainWindow(startMinimized);
        _window.SourceInitialized += (_, _) => WindowBackdropHelper.Apply(_window);
        _window.Show();

        StartShowWindowListener();
    }

    protected override void OnExit(ExitEventArgs e)
    {
        _showWindowEvent?.Dispose();
        _singleInstanceMutex?.Dispose();
        base.OnExit(e);
    }

    private void StartShowWindowListener()
    {
        var listenerThread = new Thread(() =>
        {
            while (true)
            {
                _ = _showWindowEvent!.WaitOne();
                var window = _window as MainWindow;
                window?.Dispatcher.Invoke(() => window.BringToFront());
            }
        })
        {
            IsBackground = true,
            Name = "InstantReplay-ShowWindowListener",
        };
        listenerThread.Start();
    }
}

internal static class WindowBackdropHelper
{
    private const int DwmwaUseImmersiveDarkMode = 20;
    private const int DwmwaSystemBackdropType = 38;
    private const int DwmwaMicaEffectLegacy = 1029;
    private const int DwmSystemBackdropMainWindow = 2; // Mica
    private const int WcaAccentPolicy = 19;
    private const int AccentEnableAcrylicBlurBehind = 4;

    public static void Apply(Window window)
    {
        try
        {
            IntPtr hwnd = new WindowInteropHelper(window).Handle;
            if (hwnd == IntPtr.Zero) return;

            Version os = Environment.OSVersion.Version;
            bool win11 = os.Major >= 10 && os.Build >= 22000;

            if (win11)
            {
                int dark = 1;
                    _ = DwmSetWindowAttribute(hwnd, DwmwaUseImmersiveDarkMode, ref dark, sizeof(int));

                if (os.Build >= 22621)
                {
                    int mica = DwmSystemBackdropMainWindow;
                        _ = DwmSetWindowAttribute(hwnd, DwmwaSystemBackdropType, ref mica, sizeof(int));
                }
                else
                {
                    int enable = 1;
                        _ = DwmSetWindowAttribute(hwnd, DwmwaMicaEffectLegacy, ref enable, sizeof(int));
                }
            }
            else if (os.Major == 10)
            {
                ApplyWindows10Acrylic(hwnd);
            }
        }
        catch (Exception ex)
        {
            Logger.Warn($"Backdrop helper could not apply material: {ex.Message}");
        }
    }

    private static void ApplyWindows10Acrylic(IntPtr hwnd)
    {
        var policy = new AccentPolicy
        {
            AccentState = AccentEnableAcrylicBlurBehind,
            AccentFlags = 0,
            // AARRGGBB: 60% opaque dark tint over the desktop.
            GradientColor = 0x99131116,
            AnimationId = 0,
        };

        var data = new WindowCompositionAttributeData
        {
            Attribute = WcaAccentPolicy,
            Data = Marshal.AllocHGlobal(Marshal.SizeOf(policy)),
            SizeOfData = (uint)Marshal.SizeOf(policy),
        };

        try
        {
            Marshal.StructureToPtr(policy, data.Data, false);
                _ = SetWindowCompositionAttribute(hwnd, ref data);
        }
        finally
        {
            if (data.Data != IntPtr.Zero) Marshal.FreeHGlobal(data.Data);
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct AccentPolicy
    {
        public int AccentState;
        public uint AccentFlags;
        public uint GradientColor;
        public uint AnimationId;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct WindowCompositionAttributeData
    {
        public int Attribute;
        public IntPtr Data;
        public uint SizeOfData;
    }

    [DllImport("dwmapi.dll", PreserveSig = true)]
    private static extern int DwmSetWindowAttribute(IntPtr hwnd, int attribute, ref int value, int size);

    [DllImport("user32.dll", PreserveSig = true)]
    private static extern int SetWindowCompositionAttribute(IntPtr hwnd, ref WindowCompositionAttributeData data);
}
}

namespace System.Runtime.CompilerServices
{
    // Required by the C# compiler for record / init-only members on .NET Framework.
    internal static class IsExternalInit { }

    [AttributeUsage(AttributeTargets.Class | AttributeTargets.Struct | AttributeTargets.Field | AttributeTargets.Property, AllowMultiple = false, Inherited = false)]
    internal sealed class RequiredMemberAttribute : Attribute { }

    [AttributeUsage(AttributeTargets.All, AllowMultiple = true, Inherited = false)]
    internal sealed class CompilerFeatureRequiredAttribute : Attribute
    {
        public CompilerFeatureRequiredAttribute(string featureName) => FeatureName = featureName;
        public string FeatureName { get; }
        public bool IsOptional { get; init; }
    }
}

namespace InstantReplay
{
    internal static class Compat
    {
        public static int Clamp(int value, int min, int max)
        {
            return value < min ? min : value > max ? max : value;
        }


        public static double Clamp(double value, double min, double max)
        {
            return value < min ? min : value > max ? max : value;
        }

        public static float Clamp(float value, float min, float max)
        {
            return value < min ? min : value > max ? max : value;
        }

        public static bool IsFinite(double value)
        {
            return !double.IsNaN(value) && !double.IsInfinity(value);
        }


        public static bool Contains(this string value, string other, StringComparison comparison)
        {
            return value.IndexOf(other, comparison) >= 0;
        }


        public static string ProcessPath =>
            Process.GetCurrentProcess().MainModule?.FileName ?? Assembly.GetEntryAssembly()?.Location ?? "InstantReplay.exe";
    }
}
