using System;
using System.Collections;
using System.IO;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Controls.Primitives;
using System.Windows.Input;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using System.Windows.Threading;
using InstantReplay.Models;
using InstantReplay.Services;

namespace InstantReplay.GUI;

public sealed class DonateWindow : Wpf.Ui.Controls.FluentWindow
{
    [StructLayout(LayoutKind.Sequential)]
    private struct Rect
    {
        public int Left, Top, Right, Bottom;
    }

    [DllImport("user32.dll")] private static extern bool GetWindowRect(IntPtr hWnd, out Rect rect);
    [DllImport("user32.dll")] private static extern uint GetDpiForWindow(IntPtr hWnd);
    [DllImport("user32.dll")] private static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter, int x, int y, int cx, int cy, uint uFlags);

    private const uint SwpNoZOrder = 0x0004;
    private const uint SwpNoActivate = 0x0010;
    private const double WindowWidthDips = 480;
    private const double WindowHeightDips = 540;
    private const double TitleStripDips = 32;

    private static DonateWindow? _instance;

    public static void Show(IntPtr parentHwnd, Settings settings)
    {
        if (_instance != null)
        {
            try { _instance.Activate(); } catch { }
            return;
        }

        _instance = new DonateWindow(parentHwnd, settings);
        _instance.Closed += (_, _) => _instance = null;
        _instance.Show();
        _instance.Dispatcher.BeginInvoke(new Action(() =>
        {
            try { _instance?.Activate(); }
            catch { }
        }));
    }

    private DonateWindow(IntPtr parentHwnd, Settings settings)
    {
        Title = "Support Instant Replay";
        ResizeMode = ResizeMode.NoResize;
        ShowInTaskbar = true;
        ExtendsContentIntoTitleBar = true;
        WindowBackdropType = Wpf.Ui.Controls.WindowBackdropType.Acrylic;
        WindowCornerPreference = Wpf.Ui.Controls.WindowCornerPreference.Round;
        WindowStartupLocation = WindowStartupLocation.Manual;
        UseLayoutRounding = true;
        SnapsToDevicePixels = true;
        TextOptions.SetTextFormattingMode(this, TextFormattingMode.Display);
        TextOptions.SetTextRenderingMode(this, TextRenderingMode.ClearType);
        RenderOptions.SetClearTypeHint(this, ClearTypeHint.Enabled);
        RenderOptions.SetBitmapScalingMode(this, BitmapScalingMode.HighQuality);

        var view = new SupportView();
        // The donate card was always rendered with the dark theme: scope the dark palette to this view.
        if (Application.Current.Resources["DarkPalette"] is ResourceDictionary dark)
        {
            var scoped = new ResourceDictionary();
            foreach (DictionaryEntry entry in dark) scoped[entry.Key] = entry.Value;
            view.Resources.MergedDictionaries.Add(scoped);
        }
        view.CloseRequested += (_, _) => CloseNow();
        view.MouseLeftButtonDown += (_, e) =>
        {
            if (e.GetPosition(view).Y > TitleStripDips) return;
            if (e.OriginalSource is DependencyObject source && FindAncestor<ButtonBase>(source) != null) return;
            try { DragMove(); } catch { }
        };
        Content = view;

        try
        {
            string icon = ResolveIconPath();
            if (File.Exists(icon)) Icon = BitmapFrame.Create(new Uri(icon, UriKind.Absolute));
        }
        catch (Exception ex) { Logger.Warn($"Could not set donate window icon: {ex.Message}"); }

        IntPtr hwnd = new WindowInteropHelper(this).EnsureHandle();
        PositionOverParent(hwnd, parentHwnd);
    }

    private static T? FindAncestor<T>(DependencyObject start) where T : DependencyObject
    {
        for (DependencyObject? current = start; current != null; current = System.Windows.Media.VisualTreeHelper.GetParent(current))
        {
            if (current is T match) return match;
        }
        return null;
    }

    private void CloseNow()
    {
        try { Hide(); }
        catch (Exception ex) { Logger.Warn($"Could not hide donate window before closing: {ex.Message}"); }

        Dispatcher.BeginInvoke(new Action(() =>
        {
            try { Close(); }
            catch (Exception ex) { Logger.Warn($"Could not close donate window: {ex.Message}"); }
        }));
    }

    private void PositionOverParent(IntPtr hwnd, IntPtr parentHwnd)
    {
        double scale = parentHwnd != IntPtr.Zero ? GetDpiForWindow(parentHwnd) / 96.0 : 1.0;
        if (scale <= 0) scale = 1.0;

        double heightDips = WindowHeightDips;
        double workDips = SystemParameters.WorkArea.Height;
        if (heightDips > workDips - 12) heightDips = Math.Max(480, workDips - 12);

        Width = WindowWidthDips;
        Height = heightDips;

        int width = (int)Math.Round(WindowWidthDips * scale);
        int height = (int)Math.Round(heightDips * scale);
        int x = 0, y = 0;
        if (parentHwnd != IntPtr.Zero && GetWindowRect(parentHwnd, out Rect parent))
        {
            x = parent.Left + ((parent.Right - parent.Left) - width) / 2;
            y = parent.Top + ((parent.Bottom - parent.Top) - height) / 3;
        }
        SetWindowPos(hwnd, IntPtr.Zero, x, y, width, height, SwpNoZOrder | SwpNoActivate);
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
