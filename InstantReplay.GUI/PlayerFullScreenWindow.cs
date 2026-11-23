using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Interop;
using System.Windows.Media;
using InstantReplay.GUI.Helpers;
using InstantReplay.Services;
using Microsoft.Web.WebView2.Wpf;
using Wpf.Ui.Controls;

namespace InstantReplay.GUI;

public sealed class PlayerFullScreenWindow : FluentWindow
{
    private const int OpenMs = 320;
    private const int CloseMs = 260;

    private const uint SwpNoZOrder = 0x0004, SwpNoActivate = 0x0010, SwpNoOwnerZOrder = 0x0200;
    private const int DwmaCloak = 13;

    private readonly WebView2 _web = new();
    private readonly Grid _root = new() { Background = new SolidColorBrush(Colors.Black) };
    private readonly InfoBar _errorBar;

    private IntPtr _hwnd;
    private Int32Rect _fullBounds;
    private Int32Rect _restBounds;
    private Int32Rect _currentBounds;
    private bool _opened;
    private bool _closing;
    private Action? _cancelAnimation;

    public PlayerFullScreenWindow()
    {
        Title = "Instant Replay";
        WindowBackdropType = Wpf.Ui.Controls.WindowBackdropType.None;
        WindowCornerPreference = Wpf.Ui.Controls.WindowCornerPreference.DoNotRound;
        WindowStyle = System.Windows.WindowStyle.None;
        ResizeMode = System.Windows.ResizeMode.NoResize;
        ShowInTaskbar = false;
        WindowStartupLocation = System.Windows.WindowStartupLocation.Manual;

        _web.DefaultBackgroundColor = System.Drawing.Color.Black;
        _errorBar = new InfoBar
        {
            Severity = InfoBarSeverity.Error,
            Title = "Couldn't play replay",
            Message = "",
            IsClosable = true,
            IsOpen = false,
            Margin = new Thickness(32),
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
        };

        _ = _root.Children.Add(_web);
        _ = _root.Children.Add(_errorBar);
        Content = _root;

        KeyUp += (_, e) =>
        {
            if (e.Key == Key.Escape) Shutdown();
        };
    }

    public bool WebReady { get; private set; }

    public event Action<bool>? NavigationCompleted;

    /// <summary>Notified when WebView2 initialization fails (for example, a missing runtime).</summary>
    public event Action<Exception>? WebError;

    public Func<Int32Rect?>? ReturnBounds { get; set; }

    [DllImport("user32.dll")]
    private static extern bool SetWindowPos(IntPtr hWnd, IntPtr after, int x, int y, int cx, int cy, uint flags);

    [DllImport("dwmapi.dll")]
    private static extern int DwmSetWindowAttribute(IntPtr hwnd, int attribute, ref int value, int size);

    public async Task OpenAsync(string url, Action<string> onMessage, IntPtr ownerHwnd = default,
        Int32Rect? sourceBounds = null)
    {
        try
        {
            var helper = new WindowInteropHelper(this);
            if (ownerHwnd != IntPtr.Zero) helper.Owner = ownerHwnd;
            _hwnd = helper.EnsureHandle();

            _fullBounds = ResolveDisplayBounds(sourceBounds, ownerHwnd);
            _restBounds = sourceBounds ?? CenterInset(_fullBounds, 0.58);
            _currentBounds = _restBounds;

            Cloak(true);
            Show();
            Apply(_restBounds);
            _ = Activate();

            await PlayerWebViewShared.WaitForLoadedAsync(_web, TimeSpan.FromSeconds(6));
            await _web.EnsureCoreWebView2Async();
            var cwv = _web.CoreWebView2!;
            cwv.Settings.IsZoomControlEnabled = false;
            cwv.Settings.AreBrowserAcceleratorKeysEnabled = false;
            cwv.WebMessageReceived += (_, e) => onMessage(PlayerWebViewShared.ToPlainJson(e.WebMessageAsJson ?? ""));
            cwv.NavigationCompleted += (_, e) =>
            {
                try { _ = _web.Focus(); } catch { }
                PlayOpenAnimation();
                NavigationCompleted?.Invoke(e.IsSuccess);
            };
            cwv.Navigate(url);
            try { _ = _web.Focus(); } catch { }
            WebReady = true;

            var failsafe = new DispatcherTimer { Interval = TimeSpan.FromSeconds(5) };
            failsafe.Tick += (_, _) => { failsafe.Stop(); PlayOpenAnimation(); };
            failsafe.Start();
        }
        catch (Exception ex)
        {
            Logger.Error($"Fullscreen WebView2 init failed: {ex.Message}", ex);
            ShowLoadError(ex);
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
            if (!_opened)
            {
                Finish();
                return;
            }

            Int32Rect from = _currentBounds;
            Int32Rect target = ReturnBounds?.Invoke() ?? _restBounds;
            AnimateBounds(from, target, CloseMs, EaseInOutCubic, Finish);
        }
        catch (Exception ex)
        {
            Logger.Warn($"Fullscreen close animation failed: {ex.Message}");
            Finish();
        }
    }

    private void ShowLoadError(Exception ex)
    {
        WebReady = false;
        try
        {
            Cloak(false);
            bool runtimeMissing = PlayerWebViewShared.IsRuntimeMissing(ex);
            _errorBar.Title = runtimeMissing ? "WebView2 runtime is missing" : "Couldn't play replay";
            _errorBar.Message = PlayerWebViewShared.DescribeLoadFailure(ex);
            _web.Visibility = Visibility.Collapsed;
            _errorBar.IsOpen = true;
        }
        catch (Exception uiEx)
        {
            Logger.Warn($"Could not show fullscreen player error: {uiEx.Message}");
        }
        WebError?.Invoke(ex);
    }

    private void PlayOpenAnimation()
    {
        if (_opened || _closing) return;
        _opened = true;

        Cloak(false);
        AnimateBounds(_restBounds, _fullBounds, OpenMs, EaseOutQuint, () => Apply(_fullBounds));
    }

    private void AnimateBounds(Int32Rect from, Int32Rect to, int milliseconds, Func<double, double> ease, Action done)
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

    private void Apply(Int32Rect r)
    {
        _currentBounds = r;
        if (_hwnd == IntPtr.Zero) return;
        _ = SetWindowPos(_hwnd, IntPtr.Zero, r.X, r.Y, Math.Max(1, r.Width), Math.Max(1, r.Height),
            SwpNoZOrder | SwpNoActivate | SwpNoOwnerZOrder);
    }

    private static Int32Rect Lerp(Int32Rect a, Int32Rect b, double t)
    {
        return new(
        (int)Math.Round(a.X + (b.X - a.X) * t),
        (int)Math.Round(a.Y + (b.Y - a.Y) * t),
        (int)Math.Round(a.Width + (b.Width - a.Width) * t),
        (int)Math.Round(a.Height + (b.Height - a.Height) * t));
    }

    private static double EaseOutQuint(double t)
    {
        return 1 - Math.Pow(1 - t, 5);
    }

    private static double EaseInOutCubic(double t)
    {
        return t < 0.5 ? 4 * t * t * t : 1 - Math.Pow(-2 * t + 2, 3) / 2;
    }


    private void Cloak(bool on)
    {
        try
        {
            int value = on ? 1 : 0;
            _ = DwmSetWindowAttribute(_hwnd, DwmaCloak, ref value, sizeof(int));
        }
        catch { }
    }

    private static Int32Rect ResolveDisplayBounds(Int32Rect? source, IntPtr ownerHwnd)
    {
        try
        {
            var screen = source is { } r
                ? System.Windows.Forms.Screen.FromPoint(new System.Drawing.Point(r.X + r.Width / 2, r.Y + r.Height / 2))
                : ownerHwnd != IntPtr.Zero
                    ? System.Windows.Forms.Screen.FromHandle(ownerHwnd)
                    : System.Windows.Forms.Screen.PrimaryScreen;
            var bounds = screen!.Bounds;
            return new Int32Rect(bounds.X, bounds.Y, bounds.Width, bounds.Height);
        }
        catch
        {
            var bounds = System.Windows.Forms.Screen.PrimaryScreen!.Bounds;
            return new Int32Rect(bounds.X, bounds.Y, bounds.Width, bounds.Height);
        }
    }

    private static Int32Rect CenterInset(Int32Rect outer, double fraction)
    {
        int w = (int)(outer.Width * fraction);
        int h = (int)(outer.Height * fraction);
        return new Int32Rect(outer.X + (outer.Width - w) / 2, outer.Y + (outer.Height - h) / 2, w, h);
    }
}
