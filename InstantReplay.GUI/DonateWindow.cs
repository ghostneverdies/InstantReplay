using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;
using System.Windows.Media.Imaging;
using InstantReplay.Models;
using InstantReplay.Services;
using Wpf.Ui.Controls;

namespace InstantReplay.GUI;

public sealed class DonateWindow : FluentWindow
{
    [StructLayout(LayoutKind.Sequential)]
    private struct Rect
    {
        public int Left, Top, Right, Bottom;
    }

    [DllImport("user32.dll")] private static extern bool GetWindowRect(IntPtr hWnd, out Rect rect);
    [DllImport("user32.dll")] private static extern uint GetDpiForWindow(IntPtr hWnd);
    [DllImport("user32.dll")] private static extern int GetSystemMetrics(int nIndex);

    private const int SmCyFullscreen = 17;
    private const double WindowWidthDips = 480;
    private const double WindowHeightDips = 540;

    private static DonateWindow? _instance;

    public static void Show(IntPtr parentHwnd, Settings settings)
    {
        if (_instance != null)
        {
            try { _ = _instance.Activate(); } catch { }
            return;
        }

        _instance = new DonateWindow(parentHwnd, settings);
        _instance.Closed += (_, _) => _instance = null;
        _instance.Show();
        _ = _instance.Dispatcher.BeginInvoke(new Action(() =>
        {
            try { _ = (_instance?.Activate()); }
            catch { }
        }));
    }

    private DonateWindow(IntPtr parentHwnd, Settings settings)
    {
        _ = settings;
        Title = "Support Instant Replay";
        WindowBackdropType = Wpf.Ui.Controls.WindowBackdropType.Acrylic;
        WindowCornerPreference = Wpf.Ui.Controls.WindowCornerPreference.Round;
        ResizeMode = System.Windows.ResizeMode.NoResize;
        WindowStartupLocation = System.Windows.WindowStartupLocation.Manual;

        var view = new SupportView();
        view.CloseRequested += (_, _) => CloseWithFade();
        Content = view;

        var helper = new WindowInteropHelper(this) { Owner = parentHwnd };
        IntPtr hwnd = helper.EnsureHandle();
        double scale = GetDpiForWindow(hwnd) / 96.0;
        if (scale <= 0) scale = 1.0;

        try
        {
            string iconPath = ResolveIconPath();
            if (File.Exists(iconPath)) Icon = new BitmapImage(new Uri(iconPath));
        }
        catch (Exception ex) { Logger.Warn($"Could not set donate window icon: {ex.Message}"); }

        PositionOverParent(parentHwnd, scale);
    }

    private void CloseWithFade()
    {
        try
        {
            var fade = new DoubleAnimation(1, 0, TimeSpan.FromMilliseconds(140))
            {
                EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }
            };
            fade.Completed += (_, _) =>
            {
                try { Close(); }
                catch (Exception ex) { Logger.Warn($"Could not close donate window: {ex.Message}"); }
            };
            BeginAnimation(OpacityProperty, fade);
        }
        catch (Exception ex)
        {
            Logger.Warn($"Could not fade donate window: {ex.Message}");
            try { Close(); } catch { }
        }
    }

    private void PositionOverParent(IntPtr parentHwnd, double scale)
    {
        int widthPx = (int)Math.Round(WindowWidthDips * scale);
        int heightPx = (int)Math.Round(WindowHeightDips * scale);
        int workH = GetSystemMetrics(SmCyFullscreen);
        if (heightPx > workH - 12) heightPx = Math.Max((int)Math.Round(480 * scale), workH - 12);

        int x = 0, y = 0;
        if (GetWindowRect(parentHwnd, out Rect parent))
        {
            x = parent.Left + ((parent.Right - parent.Left) - widthPx) / 2;
            y = parent.Top + ((parent.Bottom - parent.Top) - heightPx) / 3;
        }

        Left = x / scale;
        Top = y / scale;
        Width = WindowWidthDips;
        Height = heightPx / scale;
    }

    private static string ResolveIconPath()
    {
        string[] candidates =
        {
            Path.Combine(AppContext.BaseDirectory, "assets", "icons", "icon.ico"),
            Path.Combine(AppContext.BaseDirectory, "assets", "icon.ico"),
            Path.Combine(AppContext.BaseDirectory, "icon.ico"),
        };
        foreach (string candidate in candidates)
        {
            if (File.Exists(candidate)) return candidate;
        }
        return candidates[0];
    }
}
