using System;
using System.Collections;
using System.Linq;
using System.Threading;
using System.Windows;
using System.Windows.Media;
using InstantReplay.GUI;
using InstantReplay.Services;
using Microsoft.Win32;
using Wpf.Ui.Appearance;

namespace InstantReplay;

public partial class App : Application
{
    private const string SingleInstanceMutexName = "Global\\InstantReplay_SingleInstance_Mutex";
    private const string ShowWindowEventName = "Global\\InstantReplay_ShowWindow_Event";

    private MainWindow? _window;
    private Mutex? _singleInstanceMutex;
    private EventWaitHandle? _showWindowEvent;

    public App()
    {
        try
        {
            string webViewData = System.IO.Path.Combine(System.IO.Path.GetTempPath(), "InstantReplay", "WebView2");
            System.IO.Directory.CreateDirectory(webViewData);
            Environment.SetEnvironmentVariable("WEBVIEW2_USER_DATA_FOLDER", webViewData);
        }
        catch { }

        DispatcherUnhandledException += (_, e) =>
        {
            Logger.Error($"Unhandled exception: {e.Exception}", e.Exception);
            e.Handled = true;
        };
    }

    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);

        _singleInstanceMutex = new Mutex(initiallyOwned: false, SingleInstanceMutexName);
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
                        using (var existingShowEvent = EventWaitHandle.OpenExisting(ShowWindowEventName))
                        {
                            existingShowEvent.Set();
                        }
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

            Environment.Exit(0);
            return;
        }

        _showWindowEvent = new EventWaitHandle(initialState: false, EventResetMode.AutoReset, ShowWindowEventName);

        bool startMinimized = Environment.GetCommandLineArgs()
            .Any(a => string.Equals(a, "--tray", StringComparison.OrdinalIgnoreCase));

        AppTheme.Apply(WindowsTheme.IsSystemDarkMode());

        _window = new MainWindow(startMinimized);
        MainWindow = _window;
        _window.Show();

        StartShowWindowListener();
    }

    private void StartShowWindowListener()
    {
        var listenerThread = new Thread(() =>
        {
            while (true)
            {
                _showWindowEvent!.WaitOne();

                MainWindow? window = _window;
                window?.Dispatcher.BeginInvoke(new Action(() => window.BringToFront()));
            }
        })
        {
            IsBackground = true,
            Name = "InstantReplay-ShowWindowListener",
        };
        listenerThread.Start();
    }
}

/// <summary>
/// Theme plumbing that WinUI provided through ThemeDictionaries and SystemAccentColor* keys.
/// WPF-UI owns its own light/dark control resources; this class layers the app palette (Theme.xaml) and
/// the Windows accent shades on top, and points WPF-UI's WinUI-named control brushes at that palette.
/// </summary>
internal static class AppTheme
{
    // WinUI lightweight-styling keys that WPF-UI also reads, mapped to the palette brush they should use.
    private static readonly (string Key, string Source)[] ControlBrushMap =
    {
        ("ButtonBackground", "SurfaceBorderBrush"),
        ("ButtonBackgroundPointerOver", "SurfaceBorderBrush"),
        ("ButtonBackgroundPressed", "SurfaceBorderBrush"),
        ("ButtonForeground", "TextPrimaryBrush"),
        ("ButtonForegroundPointerOver", "TextPrimaryBrush"),
        ("ButtonForegroundPressed", "TextPrimaryBrush"),
        ("ComboBoxBackground", "BgBrush"),
        ("ComboBoxBackgroundPointerOver", "BgBrush"),
        ("ComboBoxForeground", "TextPrimaryBrush"),
        ("ComboBoxBorderBrush", "SurfaceBorderBrush"),
        ("ComboBoxDropDownBackground", "SurfaceBrush"),
        ("ComboBoxItemBackgroundSelected", "AccentDimBrush"),
        ("ComboBoxItemForegroundSelected", "TextPrimaryBrush"),
        ("TextControlBackground", "BgBrush"),
        ("TextControlBackgroundPointerOver", "BgBrush"),
        ("TextControlBackgroundFocused", "BgBrush"),
        ("TextControlBorderBrush", "SurfaceBorderBrush"),
        ("TextControlBorderBrushPointerOver", "AccentBrush"),
        ("TextControlBorderBrushFocused", "AccentBrush"),
        ("TextControlFocusedBorderBrush", "AccentBrush"),
        ("TextControlForeground", "TextPrimaryBrush"),
        ("TextControlForegroundFocused", "TextPrimaryBrush"),
        ("ToggleSwitchFillOff", "SurfaceBorderBrush"),
        ("ToggleSwitchFillOffPointerOver", "SurfaceBorderBrush"),
        ("ToggleSwitchFillOn", "AccentBrush"),
        ("ToggleSwitchFillOnPointerOver", "AccentBrush"),
        ("ToggleSwitchStrokeOff", "SurfaceBorderBrush"),
        ("ToggleSwitchStrokeOn", "AccentBrush"),
        ("ToggleSwitchKnobFillOff", "TextMutedBrush"),
        ("ToggleSwitchKnobFillOn", "AccentTextBrush"),
    };

    /// <summary>Windows accent palette, WinUI naming: Light3, Light2, Light1, Base, Dark1, Dark2, Dark3.</summary>
    public readonly struct AccentShades
    {
        public AccentShades(Color light3, Color light2, Color light1, Color accent, Color dark1, Color dark2, Color dark3)
        {
            Light3 = light3; Light2 = light2; Light1 = light1; Base = accent; Dark1 = dark1; Dark2 = dark2; Dark3 = dark3;
        }

        public Color Light3 { get; }
        public Color Light2 { get; }
        public Color Light1 { get; }
        public Color Base { get; }
        public Color Dark1 { get; }
        public Color Dark2 { get; }
        public Color Dark3 { get; }
    }

    public static AccentShades GetAccentShades()
    {
        try
        {
            using (RegistryKey? key = Registry.CurrentUser.OpenSubKey(@"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\Accent"))
            {
                if (key?.GetValue("AccentPalette") is byte[] b && b.Length >= 28)
                {
                    Color At(int i) => Color.FromRgb(b[i * 4], b[i * 4 + 1], b[i * 4 + 2]);
                    return new AccentShades(At(0), At(1), At(2), At(3), At(4), At(5), At(6));
                }
            }
        }
        catch { }

        Color accent = SystemParameters.WindowGlassColor;
        accent.A = 255;
        return new AccentShades(Mix(accent, Colors.White, 0.60f), Mix(accent, Colors.White, 0.40f), Mix(accent, Colors.White, 0.20f),
            accent, Mix(accent, Colors.Black, 0.15f), Mix(accent, Colors.Black, 0.30f), Mix(accent, Colors.Black, 0.45f));
    }

    public static Color GetSystemAccent() => GetAccentShades().Base;

    private static Color Mix(Color from, Color to, float amount) => Color.FromRgb(
        (byte)(from.R + (to.R - from.R) * amount),
        (byte)(from.G + (to.G - from.G) * amount),
        (byte)(from.B + (to.B - from.B) * amount));

    private static SolidColorBrush Frozen(Color c)
    {
        var brush = new SolidColorBrush(c);
        brush.Freeze();
        return brush;
    }

    public static void Apply(bool dark)
    {
        Application app = Application.Current;
        if (app == null) return;

        try
        {
            ApplicationThemeManager.Apply(
                dark ? ApplicationTheme.Dark : ApplicationTheme.Light,
                Wpf.Ui.Controls.WindowBackdropType.Mica);
        }
        catch (Exception ex)
        {
            Logger.Warn($"Could not apply WPF-UI theme: {ex.Message}");
        }

        if (app.Resources[dark ? "DarkPalette" : "LightPalette"] is ResourceDictionary palette)
        {
            foreach (DictionaryEntry entry in palette)
                app.Resources[entry.Key] = entry.Value;
        }

        AccentShades shades = GetAccentShades();
        app.Resources["AccentBrush"] = Frozen(shades.Base);
        app.Resources["AccentDimBrush"] = Frozen(dark ? shades.Dark2 : shades.Light3);
        app.Resources["HeroBackgroundBrush"] = Frozen(dark ? shades.Dark3 : shades.Dark2);

        foreach ((string key, string source) in ControlBrushMap)
        {
            if (app.Resources[source] is Brush brush)
                app.Resources[key] = brush;
        }
    }
}
