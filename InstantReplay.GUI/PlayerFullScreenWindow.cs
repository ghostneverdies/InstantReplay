using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Threading;
using InstantReplay.GUI.Helpers;
using InstantReplay.Services;
using Microsoft.Web.WebView2.Wpf;

namespace InstantReplay.GUI;

/// <summary>A rectangle in physical screen pixels (replaces Windows.Graphics.RectInt32).</summary>
public readonly struct ScreenRect
{
    public ScreenRect(int x, int y, int width, int height)
    {
        X = x; Y = y; Width = width; Height = height;
    }

    public int X { get; }
    public int Y { get; }
    public int Width { get; }
    public int Height { get; }
}

public sealed class PlayerFullScreenWindow : Window
{
    private const int OpenMs = 320;
    private const int CloseMs = 260;

    private const uint SwpNoZOrder = 0x0004, SwpNoActivate = 0x0010, SwpNoOwnerZOrder = 0x0200;
    private const int DwmaCloak = 13;
    private const int GwlpHwndParent = -8;

    private readonly WebView2 _web = new WebView2();
    private readonly Grid _root = new Grid { Background = Brushes.Black };

    private IntPtr _hwnd;
    private ScreenRect _fullBounds;
    private ScreenRect _restBounds;
    private bool _opened;
    private bool _closing;
    private Action? _cancelAnimation;

    public PlayerFullScreenWindow()
    {
        Title = "Instant Replay";
        WindowStyle = WindowStyle.None;
        ResizeMode = ResizeMode.NoResize;
        ShowInTaskbar = false;
        ShowActivated = true;
        Background = Brushes.Black;
        WindowStartupLocation = WindowStartupLocation.Manual;
        _web.DefaultBackgroundColor = System.Drawing.Color.Black;
        _root.Children.Add(_web);
        Content = _root;
    }

    public bool WebReady { get; private set; }

    public event Action<bool>? NavigationCompleted;

    public Func<ScreenRect?>? ReturnBounds { get; set; }

    [DllImport("user32.dll", EntryPoint = "SetWindowLongPtrW")]
    private static extern IntPtr SetOwnerLongPtr(IntPtr hWnd, int nIndex, IntPtr dwNewLong);

    [DllImport("user32.dll")]
    private static extern bool SetWindowPos(IntPtr hWnd, IntPtr after, int x, int y, int cx, int cy, uint flags);

    [DllImport("dwmapi.dll")]
    private static extern int DwmSetWindowAttribute(IntPtr hwnd, int attribute, ref int value, int size);

    public async Task OpenAsync(string url, Action<string> onMessage, IntPtr ownerHwnd = default,
        ScreenRect? sourceBounds = null)
    {
        try
        {
            _hwnd = new WindowInteropHelper(this).EnsureHandle();
            if (ownerHwnd != IntPtr.Zero)
                SetOwnerLongPtr(_hwnd, GwlpHwndParent, ownerHwnd);

            _fullBounds = ResolveDisplayBounds(sourceBounds, ownerHwnd);
            _restBounds = sourceBounds ?? CenterInset(_fullBounds, 0.58);

            Apply(_restBounds);
            Cloak(true);
            Show();
            Activate();

            await PlayerWebViewShared.WaitForLoadedAsync(_web, TimeSpan.FromSeconds(6));
            await _web.EnsureCoreWebView2Async();
            var cwv = _web.CoreWebView2!;
            cwv.Settings.IsZoomControlEnabled = false;
            cwv.Settings.AreBrowserAcceleratorKeysEnabled = false;
            cwv.WebMessageReceived += (_, e) => onMessage(PlayerWebViewShared.ToPlainJson(e.WebMessageAsJson ?? ""));
            cwv.NavigationCompleted += (_, e) =>
            {
                try { _web.Focus(); } catch { }
                PlayOpenAnimation();
                NavigationCompleted?.Invoke(e.IsSuccess);
            };
            cwv.Navigate(url);
            try { _web.Focus(); } catch { }
            WebReady = true;

            var failsafe = new DispatcherTimer { Interval = TimeSpan.FromSeconds(5) };
            failsafe.Tick += (_, _) => { failsafe.Stop(); PlayOpenAnimation(); };
            failsafe.Start();
        }
        catch (Exception ex)
        {
            Logger.Error($"Fullscreen WebView2 init failed: {ex.Message}", ex);
        }
    }

    public void SendScript(string js)
    {
        try
        {
            if (_web.CoreWebView2 != null) _ = _web.CoreWebView2.ExecuteScriptAsync(js);
        }
        catch { }
    }

    public void Shutdown()
    {
        if (_closing) return;
        _closing = true;
        _cancelAnimation?.Invoke();

        void Finish()
        {
            try { _web.Dispose(); } catch { }
            try { Close(); } catch { }
        }

        try
        {
            if (_hwnd == IntPtr.Zero || !_opened)
            {
                Finish();
                return;
            }

            ScreenRect from = CurrentBounds();
            ScreenRect target = ReturnBounds?.Invoke() ?? _restBounds;
            AnimateBounds(from, target, CloseMs, EaseInOutCubic, Finish);
        }
        catch (Exception ex)
        {
            Logger.Warn($"Fullscreen close animation failed: {ex.Message}");
            Finish();
        }
    }

    private ScreenRect CurrentBounds()
    {
        if (GetWindowRect(_hwnd, out NativeRect r))
            return new ScreenRect(r.Left, r.Top, r.Right - r.Left, r.Bottom - r.Top);
        return _fullBounds;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct NativeRect { public int Left, Top, Right, Bottom; }

    [DllImport("user32.dll")]
    private static extern bool GetWindowRect(IntPtr hWnd, out NativeRect rect);

    private void PlayOpenAnimation()
    {
        if (_opened || _closing || _hwnd == IntPtr.Zero) return;
        _opened = true;

        Cloak(false);
        AnimateBounds(_restBounds, _fullBounds, OpenMs, EaseOutQuint, () => { });
    }

    private void AnimateBounds(ScreenRect from, ScreenRect to, int milliseconds, Func<double, double> ease, Action done)
    {
        _cancelAnimation?.Invoke();

        var clock = Stopwatch.StartNew();
        bool finished = false;
        DispatcherTimer? safety = null;

        void Finish()
        {
            if (finished) return;
            finished = true;
            CompositionTarget.Rendering -= OnFrame;
            safety?.Stop();
            _cancelAnimation = null;
            Apply(to);
            done();
        }

        void OnFrame(object? sender, EventArgs e)
        {
            double t = Math.Min(1.0, clock.Elapsed.TotalMilliseconds / milliseconds);
            if (t >= 1.0) { Finish(); return; }
            Apply(Lerp(from, to, ease(t)));
        }

        _cancelAnimation = () =>
        {
            if (finished) return;
            finished = true;
            CompositionTarget.Rendering -= OnFrame;
            safety?.Stop();
        };

        Apply(from);
        CompositionTarget.Rendering += OnFrame;

        safety = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(milliseconds + 600) };
        safety.Tick += (_, _) => Finish();
        safety.Start();
    }

    private void Apply(ScreenRect r)
    {
        if (_hwnd == IntPtr.Zero) return;
        SetWindowPos(_hwnd, IntPtr.Zero, r.X, r.Y, Math.Max(1, r.Width), Math.Max(1, r.Height),
            SwpNoZOrder | SwpNoActivate | SwpNoOwnerZOrder);
    }

    private static ScreenRect Lerp(ScreenRect a, ScreenRect b, double t) => new ScreenRect(
        (int)Math.Round(a.X + (b.X - a.X) * t),
        (int)Math.Round(a.Y + (b.Y - a.Y) * t),
        (int)Math.Round(a.Width + (b.Width - a.Width) * t),
        (int)Math.Round(a.Height + (b.Height - a.Height) * t));

    private static double EaseOutQuint(double t) => 1 - Math.Pow(1 - t, 5);

    private static double EaseInOutCubic(double t) =>
        t < 0.5 ? 4 * t * t * t : 1 - Math.Pow(-2 * t + 2, 3) / 2;

    private void Cloak(bool on)
    {
        try
        {
            int value = on ? 1 : 0;
            DwmSetWindowAttribute(_hwnd, DwmaCloak, ref value, sizeof(int));
        }
        catch { }
    }

    private static ScreenRect ResolveDisplayBounds(ScreenRect? source, IntPtr ownerHwnd)
    {
        try
        {
            System.Windows.Forms.Screen screen;
            if (source is { } r)
                screen = System.Windows.Forms.Screen.FromPoint(new System.Drawing.Point(r.X + r.Width / 2, r.Y + r.Height / 2));
            else if (ownerHwnd != IntPtr.Zero)
                screen = System.Windows.Forms.Screen.FromHandle(ownerHwnd);
            else
                screen = System.Windows.Forms.Screen.PrimaryScreen ?? System.Windows.Forms.Screen.AllScreens[0];
            System.Drawing.Rectangle b = screen.Bounds;
            return new ScreenRect(b.X, b.Y, b.Width, b.Height);
        }
        catch
        {
            System.Drawing.Rectangle b = System.Windows.Forms.Screen.AllScreens[0].Bounds;
            return new ScreenRect(b.X, b.Y, b.Width, b.Height);
        }
    }

    private static ScreenRect CenterInset(ScreenRect outer, double fraction)
    {
        int w = (int)(outer.Width * fraction);
        int h = (int)(outer.Height * fraction);
        return new ScreenRect(outer.X + (outer.Width - w) / 2, outer.Y + (outer.Height - h) / 2, w, h);
    }
}
