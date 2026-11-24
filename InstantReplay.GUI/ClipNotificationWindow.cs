using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Media.Imaging;
using System.Windows.Threading;
using InstantReplay.GUI.Helpers;
using InstantReplay.Services;

namespace InstantReplay.GUI;

public sealed class ClipNotificationWindow : Window
{
    private static ClipNotificationWindow? _instance;
    private readonly IntPtr _hwnd;
    private readonly Border _card;
    private readonly TextBlock _message;
    private readonly TextBlock _detail;
    private readonly StackPanel _textStack;
    private readonly TranslateTransform _textShift = new();
    private readonly Border _accent;
    private readonly Image _staticIcon;
    private readonly ScaleTransform _iconTransform = new();
    private readonly System.Windows.Shapes.Ellipse _pulse;
    private readonly ScaleTransform _pulseTransform = new();
    private readonly Grid _checkBadge;
    private readonly ScaleTransform _badgeTransform = new();
    private readonly Grid _progressTrack;
    private readonly System.Windows.Shapes.Rectangle _progressFill;
    private readonly DispatcherTimer _lifetime = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(2700) };
    private long _motionStart;
    private int _motionFrom, _motionTo, _motionDuration, _currentX, _shownX, _hiddenX, _y;
    private int _progressPercent;
    private bool _leaving, _animating, _visible;
    private State _state;

    private enum State { Idle, Encoding, Success, Failure }

    [DllImport("user32.dll")] private static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] private static extern uint GetDpiForWindow(IntPtr hwnd);
    [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW")] private static extern IntPtr GetStyle(IntPtr hwnd, int index);
    [DllImport("user32.dll", EntryPoint = "SetWindowLongPtrW")] private static extern IntPtr SetStyle(IntPtr hwnd, int index, IntPtr value);
    [DllImport("user32.dll", SetLastError = true)] private static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] private static extern bool SetWindowDisplayAffinity(IntPtr hwnd, uint affinity);
    [DllImport("dwmapi.dll")] private static extern int DwmSetWindowAttribute(IntPtr hwnd, int attribute, ref int value, int size);

    private ClipNotificationWindow()
    {
        Title = "Instant Replay clip notification";
        WindowStyle = WindowStyle.None;
        ResizeMode = ResizeMode.NoResize;
        ShowInTaskbar = false;
        ShowActivated = false;
        Topmost = true;
        UseLayoutRounding = true;
        SnapsToDevicePixels = true;
        TextOptions.SetTextFormattingMode(this, TextFormattingMode.Display);
        TextOptions.SetTextRenderingMode(this, TextRenderingMode.ClearType);
        RenderOptions.SetClearTypeHint(this, ClearTypeHint.Enabled);
        RenderOptions.SetBitmapScalingMode(this, BitmapScalingMode.HighQuality);
        Background = new SolidColorBrush(Color.FromArgb(255, 36, 46, 64));
        _message = new TextBlock
        {
            FontSize = 19,
            FontWeight = FontWeights.SemiBold,
            Foreground = new SolidColorBrush(Colors.White),
            Margin = new Thickness(0, 5, 0, 0)
        };
        _textStack = new StackPanel { VerticalAlignment = VerticalAlignment.Center };
        _textStack.RenderTransform = _textShift;
        _textStack.Children.Add(new TextBlock
        {
            Text = "INSTANT REPLAY",
            FontSize = 10,
            FontWeight = FontWeights.SemiBold,
            Foreground = new SolidColorBrush(Color.FromArgb(255, 151, 174, 205))
        });
        _textStack.Children.Add(_message);
        _detail = new TextBlock
        {
            Text = "Saved to your gallery",
            FontSize = 12,
            Margin = new Thickness(0, 5, 0, 0),
            Foreground = new SolidColorBrush(Color.FromArgb(255, 171, 185, 207))
        };
        _textStack.Children.Add(_detail);
        var content = new Grid { Margin = new Thickness(22, 16, 20, 16) };
        content.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        content.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        _staticIcon = new Image
        {
            Width = 54,
            Height = 54,
            Stretch = Stretch.Uniform,
            RenderTransformOrigin = new Point(0.5, 0.5),
            RenderTransform = _iconTransform
        };
        string[] iconPaths = {
            Path.Combine(AppContext.BaseDirectory, "assets", "icons", "icon.png"),
            Path.Combine(AppContext.BaseDirectory, "assets", "icons", "icon.ico"),
            Path.Combine(AppContext.BaseDirectory, "assets", "icon.ico"),
            Path.Combine(AppContext.BaseDirectory, "icon.ico")
        };
        string? iconPath = Array.Find(iconPaths, File.Exists);
        if (iconPath != null)
            _staticIcon.Source = new BitmapImage(new Uri(iconPath));

        _pulse = new System.Windows.Shapes.Ellipse
        {
            Width = 54,
            Height = 54,
            Stroke = new SolidColorBrush(SuccessAccent),
            StrokeThickness = 2,
            Opacity = 0,
            RenderTransformOrigin = new Point(0.5, 0.5),
            RenderTransform = _pulseTransform,
            IsHitTestVisible = false
        };

        _checkBadge = new Grid
        {
            Width = 20,
            Height = 20,
            Margin = new Thickness(0, 0, -2, -2),
            HorizontalAlignment = HorizontalAlignment.Right,
            VerticalAlignment = VerticalAlignment.Bottom,
            Opacity = 0,
            RenderTransformOrigin = new Point(0.5, 0.5),
            RenderTransform = _badgeTransform,
            IsHitTestVisible = false
        };
        _checkBadge.Children.Add(new System.Windows.Shapes.Ellipse
        {
            Fill = new SolidColorBrush(SuccessAccent)
        });
        var checkStroke = new SolidColorBrush(Colors.White);
        _checkBadge.Children.Add(new System.Windows.Shapes.Line
        {
            X1 = 4, Y1 = 10, X2 = 8, Y2 = 14,
            Stroke = checkStroke,
            StrokeThickness = 2.2,
            StrokeLineJoin = PenLineJoin.Round
        });
        _checkBadge.Children.Add(new System.Windows.Shapes.Line
        {
            X1 = 8, Y1 = 14, X2 = 16, Y2 = 5,
            Stroke = checkStroke,
            StrokeThickness = 2.2,
            StrokeLineJoin = PenLineJoin.Round
        });

        var iconContent = new Grid
        {
            Width = 54,
            Height = 54,
            VerticalAlignment = VerticalAlignment.Center
        };
        iconContent.Children.Add(_pulse);
        iconContent.Children.Add(_staticIcon);
        iconContent.Children.Add(_checkBadge);
        content.Children.Add(iconContent);
        _textStack.Margin = new Thickness(16, 0, 0, 0);
        Grid.SetColumn(_textStack, 1);
        content.Children.Add(_textStack);
        var background = new SolidColorBrush(Color.FromArgb(255, 36, 46, 64));
        var layout = new Grid();
        layout.Children.Add(content);
        _accent = new Border
        {
            Width = 3,
            Margin = new Thickness(0, 18, 0, 18),
            CornerRadius = new CornerRadius(2),
            HorizontalAlignment = HorizontalAlignment.Left,
            Background = new SolidColorBrush(Color.FromArgb(255, 111, 237, 205))
        };
        layout.Children.Add(_accent);
        _progressTrack = new Grid
        {
            Height = 3,
            VerticalAlignment = VerticalAlignment.Bottom,
            Opacity = 0,
            Background = new SolidColorBrush(Color.FromArgb(70, 111, 237, 205))
        };
        _progressFill = new System.Windows.Shapes.Rectangle
        {
            Width = 0,
            HorizontalAlignment = HorizontalAlignment.Left,
            VerticalAlignment = VerticalAlignment.Stretch,
            RadiusX = 1.5,
            RadiusY = 1.5,
            Fill = new SolidColorBrush(Color.FromArgb(255, 111, 237, 205))
        };
        _progressTrack.Children.Add(_progressFill);
        _progressTrack.SizeChanged += (_, _) => ApplyProgressWidth();
        layout.Children.Add(_progressTrack);
        _card = new Border
        {
            Background = background,
            CornerRadius = new CornerRadius(12),
            BorderThickness = new Thickness(0),
            Child = layout,
            IsHitTestVisible = false
        };
        Content = _card;
        _hwnd = new WindowInteropHelper(this).EnsureHandle();
        SetStyle(_hwnd, -20, new IntPtr((GetStyle(_hwnd, -20).ToInt64() | 0x08000080L) & ~0x00040000L));
        TitleBarButtons.StripWindowFrame(_hwnd);
        int noNonClientRendering = 2;
        _ = DwmSetWindowAttribute(_hwnd, 2, ref noNonClientRendering, sizeof(int));
        TitleBarButtons.SetCornerPreference(_hwnd, round: true);
        if (!SetWindowDisplayAffinity(_hwnd, 0x11)) Logger.Debug("Clip notification capture exclusion unavailable.");
        _lifetime.Tick += (_, _) =>
        {
            _lifetime.Stop();
            BeginSlide(_hiddenX, 300, leaving: true);
        };
        Closed += (_, _) =>
        {
            _lifetime.Stop();
            StopFrames();
            _instance = null;
        };
    }

    public static void BeginEncoding()
    {
        if (OverlayBridge.InGame)
        {
            OverlayBridge.BeginEncoding();
            return;
        }
        try
        {
            _instance ??= new ClipNotificationWindow();
            _instance.Begin();
        }
        catch (Exception ex)
        {
            Logger.Warn($"Clip notification unavailable: {ex.Message}");
            Shutdown();
        }
    }

    public static void WarmUp()
    {
        if (_instance != null) return;
        try
        {
            _instance = new ClipNotificationWindow();
            Logger.Debug("Clip notification warmed up.");
        }
        catch (Exception ex)
        {
            Logger.Debug($"Clip notification warm-up failed: {ex.Message}");
            _instance = null;
        }
    }

    public static void ReportProgress(int percent)
    {
        if (OverlayBridge.InGame)
        {
            OverlayBridge.ReportProgress(percent);
            return;
        }
        _instance?.SetProgress(percent);
    }

    public static void Complete(int seconds)
    {
        if (OverlayBridge.InGame)
        {
            OverlayBridge.Complete(seconds);
            return;
        }
        ShowResult(Math.Max(1, seconds), false);
    }

    public static void Fail()
    {
        if (OverlayBridge.InGame)
        {
            OverlayBridge.Fail();
            return;
        }
        ShowResult(0, true);
    }

    public static void Show(int seconds) => Complete(seconds);
    public static void ShowError() => Fail();

    private static void ShowResult(int seconds, bool failed)
    {
        try
        {
            _instance ??= new ClipNotificationWindow();
            _instance.Finish(seconds, failed);
        }
        catch (Exception ex)
        {
            Logger.Warn($"Clip notification unavailable: {ex.Message}");
            Shutdown();
        }
    }

    private static Color SuccessAccent => Color.FromArgb(255, 111, 237, 205);
    private static Color FailureAccent => Color.FromArgb(255, 241, 115, 122);

    private void Begin()
    {
        _state = State.Encoding;
        _lifetime.Stop();
        ResetCompletionAnimation();
        _progressPercent = 0;
        _progressFill.Width = 0;
        Morph("Clipping…", "Preparing clip…", SuccessAccent, showProgress: true);
        ShowAtEdge();
    }

    private void SetProgress(int percent)
    {
        if (_state != State.Encoding) return;
        _progressPercent = Compat.Clamp(percent, 0, 100);
        _detail.Text = _progressPercent <= 0 ? "Preparing clip…" : $"Encoding {_progressPercent}%";
        ApplyProgressWidth();
    }

    private void Finish(int seconds, bool failed)
    {
        _state = failed ? State.Failure : State.Success;
        if (failed)
        {
            Morph("Failed To Clip", "See debug log for more details", FailureAccent, showProgress: false);
        }
        else
        {
            Morph($"Clipped Last {seconds} {(seconds == 1 ? "Second" : "Seconds")}",
                  "Saved to your gallery", SuccessAccent, showProgress: false);
        }
        ShowAtEdge();
        if (!failed) PlayCompletionAnimation();
        _lifetime.Stop();
        _lifetime.Start();
    }

    private void ResetCompletionAnimation()
    {
        foreach (DependencyProperty property in new[] { ScaleTransform.ScaleXProperty, ScaleTransform.ScaleYProperty })
        {
            _iconTransform.BeginAnimation(property, null);
            _pulseTransform.BeginAnimation(property, null);
            _badgeTransform.BeginAnimation(property, null);
        }
        _pulse.BeginAnimation(OpacityProperty, null);
        _checkBadge.BeginAnimation(OpacityProperty, null);

        _iconTransform.ScaleX = _iconTransform.ScaleY = 1;
        _pulseTransform.ScaleX = _pulseTransform.ScaleY = 1;
        _pulse.Opacity = 0;
        _badgeTransform.ScaleX = _badgeTransform.ScaleY = 1;
        _checkBadge.Opacity = 0;
    }

    private void PlayCompletionAnimation()
    {
        ResetCompletionAnimation();
        AddKeys(_iconTransform, ScaleTransform.ScaleXProperty, (0, 1), (140, 1.12), (300, 0.96), (420, 1));
        AddKeys(_iconTransform, ScaleTransform.ScaleYProperty, (0, 1), (140, 1.12), (300, 0.96), (420, 1));
        AddKeys(_pulseTransform, ScaleTransform.ScaleXProperty, (0, 0.72), (620, 1.28));
        AddKeys(_pulseTransform, ScaleTransform.ScaleYProperty, (0, 0.72), (620, 1.28));
        AddKeys(_pulse, OpacityProperty, (0, 0), (80, 0.9), (620, 0));
        AddKeys(_badgeTransform, ScaleTransform.ScaleXProperty, (0, 0), (220, 1.14), (380, 1));
        AddKeys(_badgeTransform, ScaleTransform.ScaleYProperty, (0, 0), (220, 1.14), (380, 1));
        AddKeys(_checkBadge, OpacityProperty, (0, 0), (80, 1));
    }

    private static void AddKeys(IAnimatable target, DependencyProperty property, params (int Ms, double Value)[] keys)
    {
        var animation = new DoubleAnimationUsingKeyFrames();
        foreach (var key in keys)
        {
            animation.KeyFrames.Add(new EasingDoubleKeyFrame
            {
                KeyTime = KeyTime.FromTimeSpan(TimeSpan.FromMilliseconds(key.Ms)),
                Value = key.Value,
                EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }
            });
        }
        target.BeginAnimation(property, animation);
    }

    private void ApplyProgressWidth()
    {
        double width = _progressTrack.ActualWidth;
        if (width <= 0) width = 382;
        _progressFill.Width = width * _progressPercent / 100.0;
    }

    private void Morph(string message, string detail, Color accent, bool showProgress)
    {
        _message.Text = message;
        _detail.Text = detail;
        _accent.Background = new SolidColorBrush(accent);
        _progressFill.Fill = new SolidColorBrush(accent);
        System.Windows.Automation.AutomationProperties.SetName(_card, message + ". " + detail);

        var ease = new CubicEase { EasingMode = EasingMode.EaseOut };
        _textStack.BeginAnimation(OpacityProperty,
            new DoubleAnimation(0, 1, TimeSpan.FromMilliseconds(190)) { EasingFunction = ease });
        _textShift.BeginAnimation(TranslateTransform.YProperty,
            new DoubleAnimation(10, 0, TimeSpan.FromMilliseconds(240)) { EasingFunction = ease });
        _progressTrack.BeginAnimation(OpacityProperty,
            new DoubleAnimation(showProgress ? 1 : 0, TimeSpan.FromMilliseconds(160)) { EasingFunction = ease });
    }

    private void ShowAtEdge()
    {
        if (_visible)
        {
            if (_leaving) BeginSlide(_shownX, 280, leaving: false);
            return;
        }

        var foreground = GetForegroundWindow();
        System.Windows.Forms.Screen screen = foreground == IntPtr.Zero
            ? (System.Windows.Forms.Screen.PrimaryScreen ?? System.Windows.Forms.Screen.AllScreens[0])
            : System.Windows.Forms.Screen.FromHandle(foreground);
        System.Drawing.Rectangle area = screen.WorkingArea;
        double scale = Math.Max(1, GetDpiForWindow(_hwnd) / 96.0);
        int width = Math.Min((int)Math.Round(382 * scale), area.Width - 24);
        int height = (int)Math.Round(108 * scale);
        int margin = (int)Math.Round(24 * scale);
        _shownX = area.X + margin;
        _hiddenX = area.X - width - 16;
        _currentX = _hiddenX;
        _y = area.Y + margin;
        SetWindowPos(_hwnd, IntPtr.Zero, _hiddenX, _y, width, height, 0x0014);
        Show();
        _visible = true;
        if (!SetWindowPos(_hwnd, new IntPtr(-1), _hiddenX, _y, 0, 0, 0x0011))
            Logger.Debug("Clip notification could not be raised above fullscreen content.");
        BeginSlide(_shownX, 320, leaving: false);
    }

    private void BeginSlide(int destination, int milliseconds, bool leaving)
    {
        _motionFrom = _currentX;
        _motionTo = destination;
        _motionDuration = Math.Max(1, milliseconds);
        _leaving = leaving;
        _motionStart = System.Diagnostics.Stopwatch.GetTimestamp();
        if (_animating) return;
        _animating = true;
        CompositionTarget.Rendering += OnRenderFrame;
    }

    private void OnRenderFrame(object? sender, EventArgs e)
    {
        try
        {
            double elapsed = (System.Diagnostics.Stopwatch.GetTimestamp() - _motionStart) * 1000.0 / System.Diagnostics.Stopwatch.Frequency;
            double progress = Compat.Clamp(elapsed / _motionDuration, 0, 1);
            double eased = _leaving
                ? progress * progress * progress
                : 1 - Math.Pow(1 - progress, 3);
            _currentX = (int)Math.Round(_motionFrom + (_motionTo - _motionFrom) * eased);
            if (!SetWindowPos(_hwnd, new IntPtr(-1), _currentX, _y, 0, 0, 0x4011))
            {
                Logger.Debug("Clip notification animation interrupted by the display.");
                StopFrames();
                HideSafely();
                return;
            }
            if (progress < 1) return;
            StopFrames();
            if (_leaving) HideSafely();
        }
        catch (Exception ex)
        {
            Logger.Debug($"Clip notification animation unavailable: {ex.Message}");
            StopFrames();
            HideSafely();
        }
    }

    private void StopFrames()
    {
        if (!_animating) return;
        _animating = false;
        CompositionTarget.Rendering -= OnRenderFrame;
    }
    private void HideSafely()
    {
        _lifetime.Stop();
        ResetCompletionAnimation();
        StopFrames();
        _visible = false;
        try
        {
            Hide();
            SetWindowPos(_hwnd, new IntPtr(-2), 0, 0, 0, 0, 0x0013);
        }
        catch (Exception ex) { Logger.Debug($"Clip notification dismiss failed: {ex.Message}"); Shutdown(); }
    }
    public static void Shutdown()
    {
        OverlayBridge.Reset();
        var window = _instance; _instance = null;
        if (window == null) return;
        window._lifetime.Stop(); window.StopFrames();
        try { window.Close(); } catch (Exception ex) { Logger.Debug($"Clip notification cleanup: {ex.Message}"); }
    }

    public static int RecordedSeconds(string? path, int fallback)
    {
        try
        {
            using var stream = File.OpenRead(path!);
            using var reader = new BinaryReader(stream);
            uint U32() => System.Buffers.Binary.BinaryPrimitives.ReadUInt32BigEndian(reader.ReadBytes(4));
            ulong U64() => System.Buffers.Binary.BinaryPrimitives.ReadUInt64BigEndian(reader.ReadBytes(8));
            long end = stream.Length;
            while (stream.Position + 8 <= end)
            {
                long start = stream.Position;
                ulong length = U32(); string type = System.Text.Encoding.ASCII.GetString(reader.ReadBytes(4));
                if (length == 1) length = U64();
                if (length == 0) length = (ulong)(end - start);
                if (length < (ulong)(stream.Position - start) || length > (ulong)(end - start)) break;
                if (type == "moov") { end = start + (long)length; continue; }
                if (type == "mvhd")
                {
                    int version = reader.ReadByte(); stream.Position += 3;
                    if (version != 0 && version != 1) break;
                    stream.Position += version == 1 ? 16 : 8;
                    uint timescale = U32(); ulong duration = version == 1 ? U64() : U32();
                    if (timescale == 0) break;
                    return (int)Compat.Clamp(Math.Round((double)duration / timescale), 1, int.MaxValue);
                }
                stream.Position = start + (long)length;
            }
        }
        catch (Exception ex) { Logger.Debug($"Clip duration metadata unavailable: {ex.Message}"); }
        return Math.Max(1, fallback);
    }
}
