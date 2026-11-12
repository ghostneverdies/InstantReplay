using System.Diagnostics;
using System.Runtime.InteropServices;
using Microsoft.UI;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Windows.Graphics;
using WinRT.Interop;
using InstantReplay.GUI.Helpers;
using InstantReplay.Services;

namespace InstantReplay.GUI;

/// <summary>
/// Fullscreen player. Entering and leaving fullscreen follows the YouTube pattern: the player does not
/// fade in as a separate window, it grows out of the inline player's rectangle until it fills the
/// monitor, and on exit it shrinks back into that same rectangle.
///
/// While animating, the window is a borderless overlapped window whose bounds are interpolated every
/// rendered frame (so it keeps pace with the display, not with a coarse timer). Once it exactly covers
/// the monitor it is promoted to the real FullScreen presenter, which makes the hand-over invisible.
/// </summary>
public sealed class PlayerFullScreenWindow : Window
{
    private const int OpenMs = 320;
    private const int CloseMs = 260;

    private const uint SwpNoZOrder = 0x0004, SwpNoActivate = 0x0010, SwpNoOwnerZOrder = 0x0200;
    private const int DwmaCloak = 13;
    private const int GwlpHwndParent = -8;

    private readonly WebView2 _web = new();
    private readonly Grid _root = new() { Background = new SolidColorBrush(Microsoft.UI.Colors.Black) };

    private AppWindow? _appWindow;
    private IntPtr _hwnd;
    private RectInt32 _fullBounds;
    private RectInt32 _restBounds;
    private bool _opened;
    private bool _closing;
    private Action? _cancelAnimation;

    public PlayerFullScreenWindow()
    {
        Title = "Instant Replay";
        _web.DefaultBackgroundColor = Microsoft.UI.Colors.Black;
        _root.Children.Add(_web);
        Content = _root;
    }

    public bool WebReady { get; private set; }

    public event Action<bool>? NavigationCompleted;

    /// <summary>
    /// Where the player should shrink back to on exit (screen pixels), usually the inline player's
    /// current rectangle. Evaluated at the moment of closing so it follows window moves and resizes.
    /// </summary>
    public Func<RectInt32?>? ReturnBounds { get; set; }

    [DllImport("user32.dll", EntryPoint = "SetWindowLongPtrW")]
    private static extern IntPtr SetOwnerLongPtr(IntPtr hWnd, int nIndex, IntPtr dwNewLong);

    [DllImport("user32.dll")]
    private static extern bool SetWindowPos(IntPtr hWnd, IntPtr after, int x, int y, int cx, int cy, uint flags);

    [DllImport("dwmapi.dll")]
    private static extern int DwmSetWindowAttribute(IntPtr hwnd, int attribute, ref int value, int size);

    public async Task OpenAsync(string url, Action<string> onMessage, IntPtr ownerHwnd = default,
        RectInt32? sourceBounds = null)
    {
        try
        {
            _hwnd = WindowNative.GetWindowHandle(this);
            if (ownerHwnd != IntPtr.Zero)
                SetOwnerLongPtr(_hwnd, GwlpHwndParent, ownerHwnd);
            _appWindow = AppWindow.GetFromWindowId(Win32Interop.GetWindowIdFromWindow(_hwnd))
                ?? throw new InvalidOperationException("AppWindow not available for the fullscreen player.");

            _fullBounds = ResolveDisplayBounds(sourceBounds, ownerHwnd);
            _restBounds = sourceBounds ?? CenterInset(_fullBounds, 0.58);

            // Start as a borderless window sitting exactly on top of the inline player. It stays
            // cloaked while the page loads so nothing flashes.
            ConfigureBorderless(_appWindow);
            _appWindow.MoveAndResize(_restBounds);
            Cloak(true);
            Activate();

            await PlayerWebViewShared.WaitForLoadedAsync(_web, TimeSpan.FromSeconds(6));
            await _web.EnsureCoreWebView2Async();
            var cwv = _web.CoreWebView2!;
            cwv.Settings.IsZoomControlEnabled = false;
            cwv.Settings.AreBrowserAcceleratorKeysEnabled = false;
            cwv.WebMessageReceived += (_, e) => onMessage(PlayerWebViewShared.ToPlainJson(e.WebMessageAsJson ?? ""));
            cwv.NavigationCompleted += (_, e) =>
            {
                try { _web.Focus(FocusState.Programmatic); } catch { }
                PlayOpenAnimation();
                NavigationCompleted?.Invoke(e.IsSuccess);
            };
            cwv.Navigate(url);
            try { _web.Focus(FocusState.Programmatic); } catch { }
            WebReady = true;

            // If navigation never reports back, still show the window rather than leave it cloaked.
            var failsafe = DispatcherQueue.CreateTimer();
            failsafe.Interval = TimeSpan.FromSeconds(5);
            failsafe.IsRepeating = false;
            failsafe.Tick += (_, _) => PlayOpenAnimation();
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
            try { _web.Close(); } catch { }
            try { Close(); } catch { }
        }

        try
        {
            if (_appWindow == null || !_opened)
            {
                Finish();
                return;
            }

            // Where we are right now: the full monitor, or part-way through the opening animation.
            RectInt32 from = _appWindow.Presenter.Kind == AppWindowPresenterKind.FullScreen
                ? _fullBounds
                : new RectInt32(_appWindow.Position.X, _appWindow.Position.Y, _appWindow.Size.Width, _appWindow.Size.Height);

            RectInt32 target = ReturnBounds?.Invoke() ?? _restBounds;

            if (_appWindow.Presenter.Kind == AppWindowPresenterKind.FullScreen)
            {
                ConfigureBorderless(_appWindow);
                Apply(from);
            }

            AnimateBounds(from, target, CloseMs, EaseInOutCubic, Finish);
        }
        catch (Exception ex)
        {
            Logger.Warn($"Fullscreen close animation failed: {ex.Message}");
            Finish();
        }
    }

    // ------------------------------------------------------------------ animation

    private void PlayOpenAnimation()
    {
        if (_opened || _closing || _appWindow == null) return;
        _opened = true;

        Cloak(false);
        AnimateBounds(_restBounds, _fullBounds, OpenMs, EaseOutQuint, () =>
        {
            // Already exactly on the monitor, so promoting to the real fullscreen presenter is seamless.
            try { _appWindow.SetPresenter(AppWindowPresenterKind.FullScreen); } catch { }
        });
    }

    /// <summary>
    /// Interpolates the window rectangle once per rendered frame. A dispatcher timer would tick at
    /// roughly 15 ms granularity and visibly stutter; the rendering callback follows the display.
    /// </summary>
    private void AnimateBounds(RectInt32 from, RectInt32 to, int milliseconds, Func<double, double> ease, Action done)
    {
        _cancelAnimation?.Invoke();

        var clock = Stopwatch.StartNew();
        bool finished = false;
        Microsoft.UI.Dispatching.DispatcherQueueTimer? safety = null;

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

        void OnFrame(object? sender, object e)
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

        // Guarantees the end state even if no frames are delivered (for example a minimized owner).
        safety = DispatcherQueue.CreateTimer();
        safety.Interval = TimeSpan.FromMilliseconds(milliseconds + 600);
        safety.IsRepeating = false;
        safety.Tick += (_, _) => Finish();
        safety.Start();
    }

    private void Apply(RectInt32 r)
    {
        if (_hwnd == IntPtr.Zero) return;
        SetWindowPos(_hwnd, IntPtr.Zero, r.X, r.Y, Math.Max(1, r.Width), Math.Max(1, r.Height),
            SwpNoZOrder | SwpNoActivate | SwpNoOwnerZOrder);
    }

    private static RectInt32 Lerp(RectInt32 a, RectInt32 b, double t) => new(
        (int)Math.Round(a.X + (b.X - a.X) * t),
        (int)Math.Round(a.Y + (b.Y - a.Y) * t),
        (int)Math.Round(a.Width + (b.Width - a.Width) * t),
        (int)Math.Round(a.Height + (b.Height - a.Height) * t));

    // Fast start, long gentle landing: the same feel as YouTube's player expansion.
    private static double EaseOutQuint(double t) => 1 - Math.Pow(1 - t, 5);

    private static double EaseInOutCubic(double t) =>
        t < 0.5 ? 4 * t * t * t : 1 - Math.Pow(-2 * t + 2, 3) / 2;

    // ------------------------------------------------------------------ window helpers

    private static void ConfigureBorderless(AppWindow window)
    {
        if (window.Presenter.Kind != AppWindowPresenterKind.Overlapped)
            window.SetPresenter(AppWindowPresenterKind.Overlapped);

        if (window.Presenter is OverlappedPresenter presenter)
        {
            presenter.SetBorderAndTitleBar(false, false);
            presenter.IsResizable = false;
            presenter.IsMaximizable = false;
            presenter.IsMinimizable = false;
        }
    }

    private void Cloak(bool on)
    {
        try
        {
            int value = on ? 1 : 0;
            DwmSetWindowAttribute(_hwnd, DwmaCloak, ref value, sizeof(int));
        }
        catch { /* cosmetic only */ }
    }

    private static RectInt32 ResolveDisplayBounds(RectInt32? source, IntPtr ownerHwnd)
    {
        try
        {
            DisplayArea area;
            if (source is { } r)
                area = DisplayArea.GetFromPoint(new PointInt32(r.X + r.Width / 2, r.Y + r.Height / 2), DisplayAreaFallback.Nearest);
            else if (ownerHwnd != IntPtr.Zero)
                area = DisplayArea.GetFromWindowId(Win32Interop.GetWindowIdFromWindow(ownerHwnd), DisplayAreaFallback.Nearest);
            else
                area = DisplayArea.Primary;
            return area.OuterBounds;
        }
        catch
        {
            return DisplayArea.Primary.OuterBounds;
        }
    }

    private static RectInt32 CenterInset(RectInt32 outer, double fraction)
    {
        int w = (int)(outer.Width * fraction);
        int h = (int)(outer.Height * fraction);
        return new RectInt32(outer.X + (outer.Width - w) / 2, outer.Y + (outer.Height - h) / 2, w, h);
    }
}
