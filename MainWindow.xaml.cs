using System.Globalization;
using System.Numerics;
using System.Runtime.InteropServices;
using System.Text.Json;
using Microsoft.UI;
using Microsoft.UI.Composition;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Animation;
using Windows.Graphics;
using Windows.System;
using Windows.UI;
using WinRT.Interop;
using Brush = Microsoft.UI.Xaml.Media.Brush;

namespace InstantReplay;

public partial class MainWindow : Window
{
    [DllImport("user32.dll")] private static extern bool RegisterHotKey(IntPtr hWnd, int id, uint fsModifiers, uint vk);
    [DllImport("user32.dll")] private static extern bool UnregisterHotKey(IntPtr hWnd, int id);
    [DllImport("user32.dll")] private static extern IntPtr SetWindowLongPtr(IntPtr hWnd, int nIndex, IntPtr dwNewLong);
    [DllImport("user32.dll")] private static extern IntPtr CallWindowProc(IntPtr lpPrevWndFunc, IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll")] private static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] private static extern uint GetDpiForWindow(IntPtr hWnd);
    [DllImport("user32.dll")] private static extern int GetSystemMetrics(int nIndex);
    [DllImport("user32.dll")] private static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] private static extern bool DestroyIcon(IntPtr hIcon);
    [DllImport("user32.dll")] private static extern bool EnableWindow(IntPtr hWnd, bool bEnable);
    [DllImport("user32.dll")] private static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern uint RegisterWindowMessage(string lpString);
    [DllImport("kernel32.dll")] private static extern bool GetSystemTimes(out FILETIME lpIdleTime, out FILETIME lpKernelTime, out FILETIME lpUserTime);
    [DllImport("kernel32.dll")] private static extern bool GlobalMemoryStatusEx(ref MEMORYSTATUSEX lpBuffer);

    [StructLayout(LayoutKind.Sequential)]
    private struct FILETIME
    {
        public uint dwLowDateTime;
        public uint dwHighDateTime;
        public ulong Ticks => ((ulong)dwHighDateTime << 32) | dwLowDateTime;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct MEMORYSTATUSEX
    {
        public uint dwLength;
        public uint dwMemoryLoad;
        public ulong ullTotalPhys;
        public ulong ullAvailPhys;
        public ulong ullTotalPageFile;
        public ulong ullAvailPageFile;
        public ulong ullTotalVirtual;
        public ulong ullAvailVirtual;
        public ulong ullAvailExtendedVirtual;
    }
    private static readonly uint s_taskbarButtonCreated = RegisterWindowMessage("TaskbarButtonCreated");

    private const int SmCyFullscreen = 17;
    private const int SwMinimize = 6;

    private delegate IntPtr WndProcDelegate(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);

    private const int GwlpWndProc = -4;
    private const int SwHide = 0;
    private const int SwShow = 5;
    private const int WM_HOTKEY = 0x0312;

    internal const int HotkeyId = 0x1000;
    internal const uint ModAlt = 0x0001;
    internal const uint ModControl = 0x0002;
    internal const uint ModShift = 0x0004;
    internal const uint ModWin = 0x0008;

    private static readonly TimeSpan SaveCooldown = TimeSpan.FromSeconds(10);
    private DateTime _lastSaveAcceptedUtc = DateTime.MinValue;

    private const long LeakThresholdMb = 500;
    private const int LeakConsecutiveSamplesRequired = 3;
    private static readonly TimeSpan LeakReminderInterval = TimeSpan.FromSeconds(20);

    private readonly Settings _settings;
    private readonly RecorderEngine _engine;
    private readonly string _tempDir;
    private readonly IntPtr _hwnd;
    private readonly AppWindow _appWindow;
    private readonly Microsoft.UI.Dispatching.DispatcherQueue _dq;

    private TrayIcon? _trayIcon;
    private DispatcherTimer? _debugStatsTimer;
    private DispatcherTimer? _bufferHealthTimer;
    private DispatcherTimer? _micRefreshTimer;
    private DispatcherTimer? _deviceChangeDebounceTimer;
    private DispatcherTimer? _statsTimer;
    private ulong _lastIdleTicks, _lastKernelTicks, _lastUserTicks;
    private bool _statsBaselineTaken;
    private AudioDeviceWatcher? _audioWatcher;
    private DispatcherTimer? _leakReminderTimer;
    private DispatcherTimer? _recDotPulseTimer;
    private DispatcherTimer? _hotkeyPulseTimer;
    private bool _isClosingForReal;
    private bool _isHidden;

    private bool? _lastBufferHealthy;
    private int _highMemStreak;
    private bool _leakWarningActive;
    private readonly bool _startMinimized;
    private bool _lastBalloonWasLeakWarning;

    private List<string> _knownMics = new();
    private bool _micMissing = false;

    private const double DefaultWindowWidthDips = 1120;
    private const double DefaultWindowHeightDips = 720;
    private bool _windowPlaced;

    private ReplayLibrary _library = null!;
    private DispatcherTimer? _infoBarTimer;
    private ReplayItem? _viewerItem;
    private bool _galleryRefreshing;
    private bool _galleryRefreshAgain;

    private PlayerFullScreenWindow? _fsWindow;

    private Storyboard? _heroStoryboard;

    private Compositor? _hoverCompositor;
    private SpringVector3NaturalMotionAnimation? _hoverGrowAnim;
    private SpringVector3NaturalMotionAnimation? _hoverShrinkAnim;
    private readonly HashSet<GridViewItem> _hoverWired = new();
    private bool _heroBuilt;

    private bool _capturingHotkey;
    private uint _pendingHotkeyModifier;

    private bool _initializing = true;
    private bool _syncingStartupToggle;
    private bool _syncingMicCombo;
    private bool _syncingAudioToggles;
    private bool _dirty;
    private bool _savingChanges;
    private PendingSettings _baseline;

    private string? _lastSavedReplayPath;
    private Action? _infoBarAction;

    private FrameworkElement? _currentPage;

    private Storyboard? _gearStoryboard;

    private TaskbarBadge? _taskbarBadge;
    private BadgeKind _badgeKind = BadgeKind.None;
    private IntPtr _badgeIcon;
    private IntPtr _trayBadgeIcon;
    private bool _hasErrored;
    private bool _flyoutShown;

    private WndProcDelegate? _wndProcDelegate;
    private IntPtr _originalWndProc;

    public MainWindow(bool startMinimized = false)
    {
        _startMinimized = startMinimized;
        InitializeComponent();
        NavView.SelectedItem = NavHomeItem;

        StatusInfoBar.ActionButton.Click += (_, _) =>
        {
            Action? action = _infoBarAction;
            _infoBarAction = null;
            action?.Invoke();
        };

        Title = "Instant Replay";
        _hwnd = WindowNative.GetWindowHandle(this);
        _appWindow = AppWindow.GetFromWindowId(Win32Interop.GetWindowIdFromWindow(_hwnd));
        _dq = Microsoft.UI.Dispatching.DispatcherQueue.GetForCurrentThread();
        _library = new ReplayLibrary(_dq);
        ElementSoundPlayer.State = ElementSoundPlayerState.On;

        SetInitialWindowSizeAndIcon();
        SetupCustomTitleBar();
        SubclassWndProc();

        _settings = Settings.Load();
        Logger.Init(_settings.DebugLogging);
        _baseline = CapturePending();

        _tempDir = Path.Combine(Path.GetTempPath(), "instant replay");
        _engine = new RecorderEngine(_settings);

        _currentPage = HomePage;

        RootGrid.Loaded += (_, _) => InitAudioToggles();

        RefreshMicrophonesInBackground();
        InitDurationSegments();
        InitFpsSegments();
        InitQualitySegments();
        InitEncoderSegments();
        InitCaptureSegments();
        UpdateHotkeyButtonDisplay();

        StartupToggle.IsOn = StartupManager.IsEnabled();
        DebugToggle.IsOn = _settings.DebugLogging;
        BackdropCombo.SelectedIndex = _settings.Backdrop switch { BackdropKind.Mica => 1, _ => 0 };
        ThemeToggle.IsOn = _settings.DarkMode;
        SaveLocationBox.Text = _settings.GetEffectiveSaveDestination();

        _initializing = false;

        GalleryGrid.ItemsSource = _library.Items;
        GalleryGrid.ContainerContentChanging += GalleryGrid_ContainerContentChanging;
        _library.Items.CollectionChanged += (_, _) => UpdateGalleryChrome();
        _library.FolderChanged += () =>
        {
            if (ReferenceEquals(_currentPage, GalleryPage)) _ = RefreshGalleryAsync();
        };
        UpdateGalleryChrome();

        Closed += (_, _) =>
        {
            if (_fsWindow != null) { var fs = _fsWindow; _fsWindow = null; try { fs.Shutdown(); } catch { } }
        };

        _dq.TryEnqueue(() =>
        {
            FitWindowToPage(HomePage);
            PlayPageEntrance(HomePage);
            StartHeroAnimations();
        });

        if (!RegisterHotKey(_hwnd, HotkeyId, _settings.HotkeyModifiers, _settings.HotkeyVk))
            Logger.Warn($"Could not register global hotkey {HotkeyDisplay.Format(_settings.HotkeyModifiers, _settings.HotkeyVk)} (may already be in use).");

        ApplyAppTheme(_settings.DarkMode);
        ApplyBackdrop();
        SetupSettingsGear();
        SetupTrayIcon();
        RefreshAppBadge();

        _debugStatsTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(30) };
        _debugStatsTimer.Tick += (_, _) => { LogDebugStats(); CheckForMemoryLeak(); };
        _debugStatsTimer.Start();

        _bufferHealthTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(3) };
        _bufferHealthTimer.Tick += (_, _) => CheckBufferHealth();
        _bufferHealthTimer.Start();

        _statsTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(3) };
        _statsTimer.Tick += (_, _) => UpdateSystemStats();
        _statsTimer.Start();
        UpdateSystemStats();

        _micRefreshTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(60) };
        _micRefreshTimer.Tick += (_, _) => RefreshMicrophonesInBackground();
        _micRefreshTimer.Start();

        _deviceChangeDebounceTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(400) };
        _deviceChangeDebounceTimer.Tick += (_, _) =>
        {
            _deviceChangeDebounceTimer!.Stop();
            RefreshMicrophonesInBackground();
        };
        _audioWatcher = new AudioDeviceWatcher();
        _audioWatcher.DevicesChanged += () => _dq.TryEnqueue(() =>
        {
            _deviceChangeDebounceTimer!.Stop();
            _deviceChangeDebounceTimer!.Start();
        });

        _ = InitialStartAsync();

        Microsoft.Win32.SystemEvents.UserPreferenceChanged += SystemEvents_UserPreferenceChanged;

        Activated += Window_Activated;
        Closed += Window_ClosedRequested;
        RootGrid.KeyDown += Window_PreviewKeyDown;
        RootGrid.KeyUp += Window_PreviewKeyUp;

        if (_startMinimized)
        {
            _dq.TryEnqueue(() =>
            {
                if (_appWindow.Presenter is OverlappedPresenter presenter) presenter.Minimize();
                ShowWindow(_hwnd, SwHide);
                _isHidden = true; StopHeroAnimations();
                NotifyRunningInBackground();
            });
        }
    }

    private void SetInitialWindowSizeAndIcon()
    {
        uint dpi = GetDpiForWindow(_hwnd);
        double scale = dpi / 96.0;
        _appWindow.ResizeClient(new SizeInt32((int)(760 * scale), (int)(480 * scale)));
        try { _appWindow.SetIcon(ResolveIconPath()); }
        catch (Exception ex) { Logger.Warn($"Could not set window icon: {ex.Message}"); }

        if (_appWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.IsResizable = false;
            presenter.IsMaximizable = false;
            presenter.IsMinimizable = true;
        }
    }

    private void SetupCustomTitleBar()
    {
        var tb = _appWindow.TitleBar;

        try { tb.ExtendsContentIntoTitleBar = true; }
        catch (Exception ex) { Logger.Warn($"Could not extend title bar: {ex.Message}"); return; }

        try
        {
            tb.PreferredHeightOption = TitleBarHeightOption.Collapsed;
        }
        catch (Exception ex)
        {
            Logger.Warn($"Could not collapse the title bar: {ex.Message}");
        }

        double scale = GetDpiForWindow(_hwnd) / 96.0;

        double stripDips = tb.Height > 0 ? tb.Height : 32;
        TitleBarHost.Height = stripDips;

        TitleBarButtonsPanel.Margin = new Thickness(0, 0, 8, 0);

        try
        {
            int stripPx = (int)Math.Round(stripDips * scale);
            double buttonsDips = 44 + 44 + 46 + 46 + 8;
            int buttonsPx = (int)Math.Round(buttonsDips * scale);
            int wPx = _appWindow.ClientSize.Width - buttonsPx;
            if (wPx > 0 && stripPx > 0)
                tb.SetDragRectangles(new[] { new RectInt32(0, 0, wPx, stripPx) });
        }
        catch (Exception ex)
        {
            Logger.Warn($"Could not set title bar drag region: {ex.Message}");
        }

        TitleBarButtons.InstallNoCaptionFrame(_hwnd);
    }

    private void DonateTitleButton_Click(object sender, RoutedEventArgs e)
    {
        DonateWindow.Show(_hwnd, _settings);
    }

    private void MinimizeTitleButton_Click(object sender, RoutedEventArgs e)
    {
        if (_appWindow.Presenter is OverlappedPresenter presenter) presenter.Minimize();
        else ShowWindow(_hwnd, SwMinimize);
    }

    private void CloseTitleButton_Click(object sender, RoutedEventArgs e) => Close();

    private async void GitHubTitleButton_Click(object sender, RoutedEventArgs e)
    {
        try
        {
            await Windows.System.Launcher.LaunchUriAsync(new Uri("https://github.com/ghostneverdies/InstantReplay"));
        }
        catch (Exception ex)
        {
            Logger.Warn($"Could not open GitHub page: {ex.Message}");
        }
    }

    private void SubclassWndProc()
    {
        _wndProcDelegate = WndProc;
        IntPtr newProcPtr = Marshal.GetFunctionPointerForDelegate(_wndProcDelegate);
        _originalWndProc = SetWindowLongPtr(_hwnd, GwlpWndProc, newProcPtr);
    }

    private IntPtr WndProc(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam)
    {
        if (msg == WM_HOTKEY && wParam.ToInt32() == HotkeyId)
        {
            _ = SaveReplayAsync();
            return IntPtr.Zero;
        }
        if (msg == s_taskbarButtonCreated && s_taskbarButtonCreated != 0)
        {
            _taskbarBadge?.Reset();
            _dq.TryEnqueue(() => RefreshAppBadge());
        }
        if (msg == TrayIcon.WM_TRAYICON)
        {
            _trayIcon?.HandleMessage(lParam);
            return IntPtr.Zero;
        }
        return CallWindowProc(_originalWndProc, hWnd, msg, wParam, lParam);
    }

    private void ApplyBackdrop()
    {
        Chrome.ApplyBackdrop(this, _settings.Backdrop);

        Brush transparent = new SolidColorBrush(Colors.Transparent);

        RootGrid.Background = transparent;
        TitleBarHost.Background = transparent;

        NavView.Resources["NavigationViewDefaultPaneBackground"] = transparent;
        NavView.Resources["NavigationViewContentBackground"] = transparent;
        NavView.Resources["NavigationViewExpandedPaneBackground"] = transparent;
        NavView.Resources["NavigationViewCollapsedPaneBackground"] = transparent;

        var theme = _settings.DarkMode ? ElementTheme.Dark : ElementTheme.Light;
        var other = theme == ElementTheme.Dark ? ElementTheme.Light : ElementTheme.Dark;
        NavView.RequestedTheme = other;
        NavView.RequestedTheme = theme;
    }

    private void ApplyAppTheme(bool dark)
    {
        RootGrid.RequestedTheme = dark ? ElementTheme.Dark : ElementTheme.Light;

        if (_appWindow.TitleBar != null)
            _appWindow.TitleBar.PreferredTheme = dark ? TitleBarTheme.Dark : TitleBarTheme.Light;

        TitleBarButtons.Apply(_appWindow.TitleBar, dark);
    }

    private void ApplySystemAccent() { }

    private void SystemEvents_UserPreferenceChanged(object? sender, Microsoft.Win32.UserPreferenceChangedEventArgs e)
    {
        if (e.Category is Microsoft.Win32.UserPreferenceCategory.General or Microsoft.Win32.UserPreferenceCategory.Color)
            _dq.TryEnqueue(() => ApplySystemAccent());
    }

    private Brush GetThemedBrush(string key)
    {
        string dictKey = _settings.DarkMode ? "Dark" : "Light";
        foreach (var merged in Application.Current.Resources.MergedDictionaries)
        {
            if (merged.ThemeDictionaries.TryGetValue(dictKey, out var dictObj) &&
                dictObj is ResourceDictionary dict && dict.TryGetValue(key, out var value) && value is Brush brush)
                return brush;
        }
        return (Brush)Application.Current.Resources[key];
    }

    private readonly record struct PendingSettings(
        int ReplayDurationSeconds,
        int FrameRate,
        string QualityPreset,
        string EncoderChoice,
        string CaptureMethod,
        string Microphone,
        string SaveDestination,
        bool RecordSystemAudio,
        bool RecordMicrophone);

    private PendingSettings CapturePending() => new(
        _settings.ReplayDurationSeconds,
        _settings.FrameRate,
        _settings.QualityPreset,
        _settings.EncoderChoice,
        _settings.CaptureMethod,
        _settings.Microphone,
        _settings.SaveDestination,
        _settings.RecordSystemAudio,
        _settings.RecordMicrophone);

    private static void PlayButtonSound() => Microsoft.UI.Xaml.ElementSoundPlayer.Play(Microsoft.UI.Xaml.ElementSoundKind.Invoke);

    private void Notify(string title, string message, int balloonIcon)
    {
        if (!ToastCenter.Show(title, message))
            _trayIcon?.ShowBalloonTip(title, message, balloonIcon);
    }

    private void NotifyRunningInBackground()
    {
        string mode = (_settings.BackgroundNotice ?? "Once").Trim();
        if (mode.Equals("Never", StringComparison.OrdinalIgnoreCase)) return;

        bool always = mode.Equals("Always", StringComparison.OrdinalIgnoreCase);
        if (!always && _settings.AppMinimizedNotificationShown) return;

        if (!_settings.AppMinimizedNotificationShown)
        {
            _settings.AppMinimizedNotificationShown = true;
            _settings.SaveFields(nameof(Settings.AppMinimizedNotificationShown));
        }

        Notify("Instant Replay", "Instant Replay is running in the background.", TrayIcon.IconInfo);
    }

    private void CloseButton_Click()
    {
        if (_appWindow.Presenter is OverlappedPresenter presenter) presenter.Minimize();

        var hideTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(220) };
        hideTimer.Tick += (_, _) =>
        {
            hideTimer.Stop();
            ShowWindow(_hwnd, SwHide);
            _isHidden = true; StopHeroAnimations();
            _taskbarBadge?.SetTaskbarVisible(false);
            _lastBalloonWasLeakWarning = false;

            NotifyRunningInBackground();
        };
        hideTimer.Start();
    }

    private void Window_ClosedRequested(object sender, WindowEventArgs args)
    {
        if (_isClosingForReal) return;
        args.Handled = true;

        if (_dirty) RevertPendingChanges();

        CloseButton_Click();
    }

    private void Window_Activated(object sender, WindowActivatedEventArgs args)
    {
        if (args.WindowActivationState == WindowActivationState.Deactivated && _capturingHotkey)
            EndHotkeyCapture();

        if (args.WindowActivationState != WindowActivationState.Deactivated)
        {
            RefreshStartupToggleState();
            _taskbarBadge?.StopFlashing();
            if (ReferenceEquals(_currentPage, GalleryPage)) _ = RefreshGalleryAsync();
        }
    }

    private void NavView_SelectionChanged(NavigationView sender, NavigationViewSelectionChangedEventArgs args)
    {
        if (_initializing) return;
        FrameworkElement target =
            ReferenceEquals(sender.SelectedItem, NavSettingsItem) ? SettingsPage
            : ReferenceEquals(sender.SelectedItem, NavGalleryItem) ? GalleryPage
            : HomePage;
        PlaySound(PageOrder(target) >= PageOrder(_currentPage ?? HomePage) ? ElementSoundKind.MoveNext : ElementSoundKind.GoBack);
        NavigateTo(target);
    }

    private int PageOrder(FrameworkElement page) =>
        ReferenceEquals(page, SettingsPage) ? 2
        : ReferenceEquals(page, GalleryPage) || ReferenceEquals(page, ViewerPage) ? 1
        : 0;

    private void NavView_ItemInvoked(NavigationView sender, NavigationViewItemInvokedEventArgs args)
    {
        if (ReferenceEquals(args.InvokedItemContainer, NavGalleryItem) && ReferenceEquals(_currentPage, ViewerPage))
            CloseViewer();
    }

    private void NavigateTo(FrameworkElement newPage, bool fadeOnly = false)
    {
        if (ReferenceEquals(newPage, _currentPage)) return;

        if (_dirty) RevertPendingChanges();

        if (ReferenceEquals(newPage, SettingsPage))
            RefreshStartupToggleState();

        PlayNavIconAnimation(newPage);

        var oldPage = _currentPage;
        _currentPage = newPage;

        Logger.Info($"NavigateTo: {newPage.Name}");

        if (ReferenceEquals(oldPage, ViewerPage))
        {
            StopViewerPlayback();
        }
        if (ReferenceEquals(oldPage, HomePage)) StopHeroAnimations();

        Canvas.SetZIndex(newPage, 1);
        if (oldPage != null) Canvas.SetZIndex(oldPage, 0);

        newPage.Visibility = Visibility.Visible;
        newPage.Opacity = 0;
        AnimateDouble(newPage, "Opacity", 0, 1, TimeSpan.FromMilliseconds(240), TimeSpan.Zero,
            new CubicEase { EasingMode = EasingMode.EaseOut });

        if (!fadeOnly) PlayPageEntrance(newPage);
        if (ReferenceEquals(newPage, HomePage) && !_isHidden) StartHeroAnimations();
        if (ReferenceEquals(newPage, GalleryPage)) _ = RefreshGalleryAsync();

        FitWindowToPage(newPage);

        TransitionOut(oldPage);
    }

    private void FitWindowToPage(FrameworkElement page)
    {
        _dq.TryEnqueue(() =>
        {
            if (_appWindow.Presenter is not OverlappedPresenter) return;

            try
            {
                double scale = GetDpiForWindow(_hwnd) / 96.0;
                if (scale <= 0) scale = 1.0;

                RectInt32 work = DisplayArea.GetFromWindowId(_appWindow.Id, DisplayAreaFallback.Nearest).WorkArea;
                int w = (int)Math.Min(DefaultWindowWidthDips * scale, work.Width * 0.94);
                int h = (int)Math.Min(DefaultWindowHeightDips * scale, work.Height * 0.94);

                SizeInt32 current = _appWindow.ClientSize;
                if (current.Width != w || current.Height != h)
                    _appWindow.ResizeClient(new SizeInt32(w, h));

                if (!_windowPlaced)
                {
                    _windowPlaced = true;
                    SizeInt32 outer = _appWindow.Size;
                    _appWindow.Move(new PointInt32(
                        work.X + Math.Max(0, (work.Width - outer.Width) / 2),
                        work.Y + Math.Max(0, (work.Height - outer.Height) / 2)));
                }
            }
            catch (Exception ex)
            {
                Logger.Warn($"FitWindowToPage failed: {ex.Message}");
            }
        });
    }

    private IEnumerable<FrameworkElement> EntranceTargets(FrameworkElement page)
    {
        if (ReferenceEquals(page, HomePage))
            return new FrameworkElement[] { HeroArt, HeroTitle, HeroSubtitle, HeroActions };
        if (ReferenceEquals(page, SettingsPage))
            return SettingsContent.Children.OfType<FrameworkElement>().ToList();
        if (ReferenceEquals(page, GalleryPage))
            return new FrameworkElement[] { GalleryHeader, GalleryBody };
        return Array.Empty<FrameworkElement>();
    }

    private void PlayPageEntrance(FrameworkElement page)
    {
        var ease = new CubicEase { EasingMode = EasingMode.EaseOut };
        int index = 0;
        foreach (FrameworkElement el in EntranceTargets(page))
        {
            if (el.RenderTransform is not CompositeTransform t)
            {
                t = new CompositeTransform();
                el.RenderTransform = t;
            }

            t.TranslateY = 28;
            el.Opacity = 0;

            var begin = TimeSpan.FromMilliseconds(index * 70);
            AnimateDouble(el, "Opacity", 0, 1, TimeSpan.FromMilliseconds(380), begin, ease);
            AnimateDouble(t, "TranslateY", 28, 0, TimeSpan.FromMilliseconds(460), begin, ease);
            index++;
        }
    }

    private readonly Dictionary<FrameworkElement, CompositeTransform> _navIcons = new();

    private void SetupSettingsGear()
    {
        try
        {
            RegisterNavIcon(HomePage, NavHomeItem);
            RegisterNavIcon(GalleryPage, NavGalleryItem);
            RegisterNavIcon(SettingsPage, NavSettingsItem);
        }
        catch (Exception ex)
        {
            Logger.Warn($"Navigation icon setup failed: {ex.Message}");
        }
    }

    private void RegisterNavIcon(FrameworkElement page, NavigationViewItem item)
    {
        if (item.Icon is not FrameworkElement icon) return;
        var transform = new CompositeTransform();
        icon.RenderTransformOrigin = new Windows.Foundation.Point(0.5, 0.5);
        icon.RenderTransform = transform;
        _navIcons[page] = transform;
    }

    private static void AddKeys(Storyboard sb, DependencyObject target, string property, params (int Ms, double Value)[] keys)
    {
        var anim = new DoubleAnimationUsingKeyFrames();
        foreach (var (ms, value) in keys)
        {
            anim.KeyFrames.Add(new EasingDoubleKeyFrame
            {
                KeyTime = KeyTime.FromTimeSpan(TimeSpan.FromMilliseconds(ms)),
                Value = value,
                EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut },
            });
        }
        Storyboard.SetTarget(anim, target);
        Storyboard.SetTargetProperty(anim, property);
        sb.Children.Add(anim);
    }

    private static void AddLoop(Storyboard sb, DependencyObject target, string property, double from, double to,
        double seconds, double delaySeconds = 0, bool autoReverse = true)
    {
        var anim = new DoubleAnimation
        {
            From = from,
            To = to,
            Duration = TimeSpan.FromSeconds(seconds),
            BeginTime = TimeSpan.FromSeconds(delaySeconds),
            AutoReverse = autoReverse,
            RepeatBehavior = RepeatBehavior.Forever,
            EasingFunction = new SineEase { EasingMode = EasingMode.EaseInOut },
        };
        Storyboard.SetTarget(anim, target);
        Storyboard.SetTargetProperty(anim, property);
        sb.Children.Add(anim);
    }

    private void PlayNavIconAnimation(FrameworkElement page)
    {
        if (!_navIcons.TryGetValue(page, out CompositeTransform? t)) return;

        try
        {
            _gearStoryboard?.Stop();
            foreach (CompositeTransform other in _navIcons.Values)
            {
                other.Rotation = 0;
                other.ScaleX = other.ScaleY = 1;
                other.TranslateY = 0;
            }

            var sb = new Storyboard();
            if (ReferenceEquals(page, SettingsPage))
            {
                AddKeys(sb, t, "Rotation", (0, 0), (400, 360));
            }
            else if (ReferenceEquals(page, HomePage))
            {
                AddKeys(sb, t, "ScaleX", (0, 1), (170, 1.3), (340, 1));
                AddKeys(sb, t, "ScaleY", (0, 1), (170, 1.3), (340, 1));
                AddKeys(sb, t, "TranslateY", (0, 0), (170, -3), (340, 0));
            }
            else
            {
                AddKeys(sb, t, "ScaleX", (0, 1), (150, 0.1), (320, 1));
                AddKeys(sb, t, "Rotation", (0, 0), (150, -8), (320, 0));
            }

            _gearStoryboard = sb;
            sb.Begin();
        }
        catch (Exception ex)
        {
            Logger.Warn($"Navigation icon animation failed: {ex.Message}");
        }
    }


    private void BuildHero()
    {
        if (_heroBuilt) return;
        _heroBuilt = true;

        HeroIcon.RenderTransform = new CompositeTransform();
        HeroGlow.RenderTransform = new CompositeTransform();
        HeroRingA.RenderTransform = new CompositeTransform();
        HeroRingB.RenderTransform = new CompositeTransform();

        var rng = new Random(11);
        var accent = (Brush)Application.Current.Resources["AccentBrush"];
        const double center = 120;

        ParticleCanvas.Children.Clear();
        for (int i = 0; i < 14; i++)
        {
            double angle = i * (2 * Math.PI / 14) + rng.NextDouble() * 0.35;
            double radius = 84 + rng.NextDouble() * 32;
            bool star = i % 2 == 0;
            double size = star ? 10 + rng.NextDouble() * 10 : 4 + rng.NextDouble() * 4;

            FrameworkElement particle = star
                ? new TextBlock { Text = "\u2726", FontSize = size, Foreground = accent, IsTextScaleFactorEnabled = false }
                : new Microsoft.UI.Xaml.Shapes.Ellipse { Width = size, Height = size, Fill = accent };

            particle.Opacity = 0.15;
            particle.RenderTransformOrigin = new Windows.Foundation.Point(0.5, 0.5);
            particle.RenderTransform = new CompositeTransform();
            Canvas.SetLeft(particle, center + Math.Cos(angle) * radius - size / 2);
            Canvas.SetTop(particle, center + Math.Sin(angle) * radius - size / 2);
            ParticleCanvas.Children.Add(particle);
        }
    }

    private void StartHeroAnimations()
    {
        try
        {
            BuildHero();
            StopHeroAnimations();

            var sb = new Storyboard();
            var rng = new Random(5);

            var icon = (CompositeTransform)HeroIcon.RenderTransform;
            AddLoop(sb, icon, "TranslateY", 0, -9, 3.2);
            AddLoop(sb, icon, "Rotation", -3, 3, 4.6);
            AddLoop(sb, icon, "ScaleX", 1, 1.05, 2.4);
            AddLoop(sb, icon, "ScaleY", 1, 1.05, 2.4);

            var glow = (CompositeTransform)HeroGlow.RenderTransform;
            AddLoop(sb, glow, "ScaleX", 0.92, 1.14, 2.8);
            AddLoop(sb, glow, "ScaleY", 0.92, 1.14, 2.8);
            AddLoop(sb, HeroGlow, "Opacity", 0.16, 0.34, 2.8);

            foreach (var (ring, delay) in new[] { (HeroRingA, 0.0), (HeroRingB, 1.6) })
            {
                var rt = (CompositeTransform)ring.RenderTransform;
                AddLoop(sb, rt, "ScaleX", 0.8, 1.55, 3.2, delay, autoReverse: false);
                AddLoop(sb, rt, "ScaleY", 0.8, 1.55, 3.2, delay, autoReverse: false);
                AddLoop(sb, ring, "Opacity", 0.55, 0, 3.2, delay, autoReverse: false);
            }

            foreach (UIElement child in ParticleCanvas.Children)
            {
                var t = (CompositeTransform)((FrameworkElement)child).RenderTransform;
                double dur = 1.4 + rng.NextDouble() * 2.0;
                double delay = rng.NextDouble() * 2.0;
                AddLoop(sb, child, "Opacity", 0.1, 1, dur, delay);
                AddLoop(sb, t, "ScaleX", 0.55, 1.2, dur, delay);
                AddLoop(sb, t, "ScaleY", 0.55, 1.2, dur, delay);
                AddLoop(sb, t, "TranslateY", 0, -(6 + rng.NextDouble() * 8), dur * 1.7, delay);
            }

            _heroStoryboard = sb;
            sb.Begin();
        }
        catch (Exception ex)
        {
            Logger.Warn($"Hero animation failed: {ex.Message}");
        }
    }

    private void StopHeroAnimations()
    {
        try { _heroStoryboard?.Stop(); } catch { }
        _heroStoryboard = null;
    }


    private void SetStartStopLabel(string text, string? glyph = null)
    {
        StartStopText.Text = text;
        if (glyph != null) StartStopIcon.Glyph = glyph;
    }

    private void ApplyStartStopLook(bool running)
    {
        SaveReplayButton.Visibility = running ? Visibility.Visible : Visibility.Collapsed;

        if (running)
        {
            StartStopButton.Style = (Style)Application.Current.Resources["DangerButtonStyle"];
            StartStopButton.ClearValue(Control.BackgroundProperty);
            StartStopButton.MinWidth = 140;
            SetStartStopLabel("STOP", "\uE71A");
        }
        else
        {
            StartStopButton.Style = (Style)Application.Current.Resources["AccentButtonStyle"];
            StartStopButton.Background = (Brush)Application.Current.Resources["AccentGradientBrush"];
            StartStopButton.MinWidth = 240;
            SetStartStopLabel("START REPLAY", "\uE768");
        }
    }


    private void UpdateGalleryChrome()
    {
        int n = _library.Items.Count;
        GalleryCountText.Text = n == 1 ? "1 replay" : $"{n} replays";
        GalleryEmptyState.Visibility = n == 0 ? Visibility.Visible : Visibility.Collapsed;
        GalleryGrid.Visibility = n == 0 ? Visibility.Collapsed : Visibility.Visible;
    }

    private async Task RefreshGalleryAsync()
    {
        if (_galleryRefreshing)
        {
            _galleryRefreshAgain = true;
            return;
        }

        _galleryRefreshing = true;
        try
        {
            await _library.RefreshAsync(_settings.GetEffectiveSaveDestination());
        }
        catch (Exception ex)
        {
            Logger.Warn($"Gallery refresh failed: {ex.Message}");
        }
        finally
        {
            _galleryRefreshing = false;
        }

        UpdateGalleryChrome();

        if (_galleryRefreshAgain)
        {
            _galleryRefreshAgain = false;
            _ = RefreshGalleryAsync();
        }
    }

    private void EmptyGoHomeButton_Click(object sender, RoutedEventArgs e) => NavView.SelectedItem = NavHomeItem;

    private void GalleryGrid_ItemClick(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is not ReplayItem item) return;
        PlaySound(ElementSoundKind.MoveNext);
        OpenViewer(item);
    }

    private void GalleryGrid_ContainerContentChanging(ListViewBase sender, ContainerContentChangingEventArgs args)
    {
        if (args.ItemContainer is not GridViewItem card || !_hoverWired.Add(card)) return;
        card.PointerEntered += (_, _) => AnimateCardHover(card, true);
        card.PointerExited += (_, _) => AnimateCardHover(card, false);
    }

    private void AnimateCardHover(GridViewItem card, bool hover)
    {
        try
        {
            _hoverCompositor ??= Microsoft.UI.Xaml.Media.CompositionTarget.GetCompositorForCurrentThread();
            if (hover)
            {
                _hoverGrowAnim ??= CreateHoverSpring(new Vector3(1.04f, 1.04f, 1f));
                Microsoft.UI.Xaml.Hosting.ElementCompositionPreview.GetElementVisual(card).StartAnimation("Scale", _hoverGrowAnim);
            }
            else
            {
                _hoverShrinkAnim ??= CreateHoverSpring(new Vector3(1f, 1f, 1f));
                Microsoft.UI.Xaml.Hosting.ElementCompositionPreview.GetElementVisual(card).StartAnimation("Scale", _hoverShrinkAnim);
            }
        }
        catch (Exception ex) { Logger.Warn($"Gallery hover animation failed: {ex.Message}"); }
    }

    private SpringVector3NaturalMotionAnimation CreateHoverSpring(Vector3 final)
    {
        var anim = _hoverCompositor!.CreateSpringVector3Animation();
        anim.Target = "Scale";
        anim.FinalValue = final;
        return anim;
    }

    private void PopulateViewer(ReplayItem item)
    {
        ViewerPoster.Source = item.Thumbnail;
        ViewerPoster.Visibility = Visibility.Visible;
    }

    private async void OpenViewer(ReplayItem item)
    {
        _viewerItem = item;

        if (!item.MetadataLoaded) await _library.EnsureAsync(item);

        _ = OpenFullscreenPlayerAsync(item);
    }

    private void ViewerBackButton_Click(object sender, RoutedEventArgs e)
    {
        PlaySound(ElementSoundKind.GoBack);
        CloseViewer();
    }

    private async void CloseViewer()
    {
        StopViewerPlayback();
        NavigateTo(GalleryPage, fadeOnly: true);
    }

    private void StopViewerPlayback()
    {
        CloseFullscreenPlayer();
    }

    private async void ConfirmDeleteButton_Click(object sender, RoutedEventArgs e)
    {
        ReplayItem? item = _viewerItem;
        if (item == null) return;

        StopViewerPlayback();
        await Task.Delay(250);

        bool deleted = await _library.DeleteAsync(item);
        if (deleted)
        {
            _viewerItem = null;
            NavigateTo(GalleryPage, fadeOnly: true);
            ShowInfoBar(InfoBarSeverity.Success, "Replay deleted", "The replay was moved to the Recycle Bin.", "delete");
        }
        else
        {
            ShowInfoBar(InfoBarSeverity.Error, "Couldn't delete replay",
                "The replay is in use or can't be removed. Check the debug log for details.", "delete");
        }
    }


    private async Task OpenFullscreenPlayerAsync(ReplayItem item)
    {
        if (_fsWindow != null) return;
        try
        {
            if (!File.Exists(item.VideoPath))
            {
                ShowInfoBar(InfoBarSeverity.Error, "Couldn't play replay",
                    "The video file could not be opened. It may have been moved or deleted.", "play");
                return;
            }

            Logger.Info("OpenViewer: opening fullscreen player");

            var fs = new PlayerFullScreenWindow();
            fs.Closed += (_, _) =>
            {
                if (_fsWindow != null) CloseFullscreenPlayer();
            };
            fs.NavigationCompleted += ok =>
            {
                if (ok) SendAccentFs();
            };
            _fsWindow = fs;
            EnableWindow(_hwnd, false);

            await fs.OpenAsync(BuildPlayerUrl(item.VideoPath, 0), HandlePlayerMessage, _hwnd);
        }
        catch (Exception ex)
        {
            Logger.Error($"Opening fullscreen player failed: {ex.Message}", ex);
            CloseFullscreenPlayer();
        }
    }

    private void CloseFullscreenPlayer()
    {
        var fs = _fsWindow;
        if (fs == null) return;
        _fsWindow = null;

        try { EnableWindow(_hwnd, true); } catch { }
        try { fs.Shutdown(); } catch { }
        try { SetForegroundWindow(_hwnd); } catch { }
        try { Activate(); } catch { }
    }

    private void SendScriptFs(string js) => _fsWindow?.SendScript(js);

    private async void CardDeleteButton_Click(object sender, RoutedEventArgs e)
    {
        if (sender is Button button && button.DataContext is ReplayItem item)
        {
            if (_fsWindow != null)
            {
                var fs = _fsWindow;
                _fsWindow = null;
                try { fs.Shutdown(); } catch { }
            }
            try { _appWindow.Show(); } catch { }
            try { SetForegroundWindow(_hwnd); } catch { }
            try { Activate(); } catch { }
            StopViewerPlayback();

            bool deleted = await _library.DeleteAsync(item);
            if (deleted)
            {
                ShowInfoBar(InfoBarSeverity.Success, "Replay deleted", "The replay was moved to the Recycle Bin.", "delete");
            }
            else
            {
                ShowInfoBar(InfoBarSeverity.Error, "Couldn't delete replay",
                    "The replay is in use or can't be removed.", "delete");
            }
        }
    }

    private void SendAccentFs()
    {
        string? accent = GetAccentHex();
        if (accent != null) SendScriptFs($"receiveCommand({{\"c\":\"accent\",\"v\":\"{accent}\"}})");
    }

    private static string? GetAccentHex()
    {
        try
        {
            if (Application.Current.Resources["AccentBrush"] is SolidColorBrush b)
                return $"#{b.Color.R:X2}{b.Color.G:X2}{b.Color.B:X2}";
        }
        catch { }
        return null;
    }

    private string BuildPlayerUrl(string videoPath, double startSeconds)
    {
        string page = new Uri(Path.Combine(AppContext.BaseDirectory, "mediaplayer", "player.html")).AbsoluteUri;
        string src = new Uri(videoPath).AbsoluteUri;
        string t = startSeconds.ToString("0.###", CultureInfo.InvariantCulture);
        return $"{page}?src={Uri.EscapeDataString(src)}&t={t}&mode=fullscreen";
    }

    private void HandlePlayerMessage(string json)
    {
        try
        {
            using var doc = JsonDocument.Parse(json);
            JsonElement root = doc.RootElement;
            string type = root.GetProperty("type").GetString() ?? "";

            switch (type)
            {
                case "fs":
                    string act = root.TryGetProperty("act", out var ae) ? ae.GetString() ?? "" : "";
                    if (act == "exit") CloseFullscreenPlayer();
                    break;

                case "close":
                    CloseFullscreenPlayer();
                    break;
            }
        }
        catch (Exception ex)
        {
            Logger.Warn($"Player message parse failed: {ex.Message}");
        }
    }

    private void TransitionOut(FrameworkElement? oldPage)
    {
        if (oldPage == null) return;

        oldPage.Opacity = 1;
        AnimateDouble(oldPage, "Opacity", 1, 0, TimeSpan.FromMilliseconds(140), TimeSpan.Zero,
            new CubicEase { EasingMode = EasingMode.EaseIn },
            onCompleted: () =>
            {
                if (!ReferenceEquals(oldPage, _currentPage)) oldPage.Visibility = Visibility.Collapsed;
            });
    }

    private static void AnimateDouble(DependencyObject target, string property, double from, double to,
        TimeSpan duration, TimeSpan? beginTime = null, EasingFunctionBase? easing = null, Action? onCompleted = null)
    {
        var anim = new DoubleAnimation { From = from, To = to, Duration = duration, EasingFunction = easing };
        if (beginTime.HasValue) anim.BeginTime = beginTime.Value;
        Storyboard.SetTarget(anim, target);
        Storyboard.SetTargetProperty(anim, property);
        var sb = new Storyboard();
        sb.Children.Add(anim);
        if (onCompleted != null) sb.Completed += (_, _) => onCompleted();
        sb.Begin();
    }

    private void StartPulse(FrameworkElement target, ref DispatcherTimer? timerField)
    {
        StopPulse(target, ref timerField);
        double phase = 0;
        var timer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(33) };
        timer.Tick += (_, _) =>
        {
            phase += 0.12;
            target.Opacity = 0.45 + 0.55 * Math.Abs(Math.Sin(phase));
        };
        timerField = timer;
        timer.Start();
    }

    private static void StopPulse(FrameworkElement target, ref DispatcherTimer? timerField)
    {
        timerField?.Stop();
        timerField = null;
        target.Opacity = 1;
    }

    private string? MicNotice()
    {
        if (!_settings.RecordMicrophone) return null;
        if (!_engine.TryGetMicStatus(out _, out _, out long gotFrames, out long failHr,
                out _, out int micPeak, out string active))
            return null;

        if (failHr != 0)
        {
            return failHr switch
            {
                -1 => "capture thread unavailable",
                -2 => "capture stream failed",
                -3 => "no microphone available",
                _ => $"engine error {failHr}",
            };
        }

        if (string.IsNullOrEmpty(_settings.Microphone) && active.Length > 0)
            return null;

        if (_settings.Microphone.Length > 0 && active.Length > 0
            && !string.Equals(active, _settings.Microphone, StringComparison.OrdinalIgnoreCase))
            return "selected device unavailable";

        if (!_engine.IsRunning) return null;
        if (gotFrames == 0) return "no signal yet";
        return micPeak == 0 ? "delivering silence" : null;
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

    private void SetupTrayIcon()
    {
        string iconPath = ResolveIconPath();
        _taskbarBadge = new TaskbarBadge(_hwnd);
        _trayIcon = new TrayIcon(_hwnd, iconPath, "Instant Replay");
        _trayIcon.MenuItemsProvider = () => new List<TrayMenuItem>
        {
            new() { Text = "Start with Windows", IsChecked = StartupManager.IsEnabled(), OnClick = () =>
            {
                bool newState = !StartupManager.IsEnabled();
                StartupManager.SetEnabled(newState);
                _dq.TryEnqueue(() => StartupToggle.IsOn = newState);
            }},
            TrayMenuItem.Separator(),
            new() { Text = "Save Replay", OnClick = () => _ = SaveReplayAsync() },
            new() { Text = "Open Window", OnClick = ShowFromTray },
            TrayMenuItem.Separator(),
            new() { Text = "Restart App", OnClick = RestartApplication },
            TrayMenuItem.Separator(),
            new() { Text = "Exit", OnClick = ExitApplicationAsync },
        };
        _trayIcon.DoubleClicked += ShowFromTray;
        _trayIcon.BalloonClicked += () =>
        {
            if (_lastBalloonWasLeakWarning)
            {
                RestartApplication();
                return;
            }

            if (_lastSavedReplayPath != null)
                OpenContainingFolder(_lastSavedReplayPath);
        };
    }

    private static void OpenContainingFolder(string filePath)
    {
        try
        {
            string? directory = System.IO.Path.GetDirectoryName(filePath);
            bool exists = !string.IsNullOrEmpty(filePath) && System.IO.File.Exists(filePath);

            if (exists && !string.IsNullOrEmpty(directory))
            {
                System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo
                {
                    FileName = "explorer.exe",
                    Arguments = $"/select,\"{filePath}\"",
                    UseShellExecute = true,
                });
                return;
            }

            if (!string.IsNullOrEmpty(directory) && System.IO.Directory.Exists(directory))
            {
                System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo
                {
                    FileName = directory,
                    UseShellExecute = true,
                });
                return;
            }

            Logger.Warn($"OpenContainingFolder: neither the file nor its folder exists ({filePath}).");
        }
        catch (Exception ex)
        {
            Logger.Warn($"Could not open the folder containing {filePath}: {ex.Message}");
        }
    }

    public void BringToFront() => ShowFromTray();

    private void ShowFromTray()
    {
        if (_fsWindow != null)
        {
            try { _fsWindow.Activate(); } catch { }
            return;
        }

        ShowWindow(_hwnd, SwShow);
        _isHidden = false;
        if (ReferenceEquals(_currentPage, HomePage)) StartHeroAnimations();
        _taskbarBadge?.SetTaskbarVisible(true);
        if (_appWindow.Presenter is OverlappedPresenter presenter) presenter.Restore();
        Activate();
        RefreshStartupToggleState();
        RefreshAppBadge();
    }

    private void RefreshAppBadge() =>
        UpdateAppBadge(_hasErrored ? BadgeKind.Error
                   : _engine.IsRunning ? BadgeKind.Recording
                   : BadgeKind.Paused, flash: false);

    private void UpdateAppBadge(BadgeKind kind, bool flash)
    {
        if (kind == BadgeKind.Error) _hasErrored = true;

        try
        {
            if (kind != _badgeKind)
            {
                _badgeKind = kind;
                ReleaseBadgeIcons();

                if (kind != BadgeKind.None)
                {
                    (int glyph, Color color) = kind switch
                    {
                        BadgeKind.Recording => (0xE714, Color.FromArgb(255, 229, 72, 77)),
                        BadgeKind.Paused => (0xE769, Color.FromArgb(255, 120, 120, 128)),
                        _ => (0xE783, Color.FromArgb(255, 245, 166, 35)),
                    };

                    _badgeIcon = BadgeGlyphs.CreateOverlayIcon(glyph, color, kind, 32);
                    _trayBadgeIcon = BadgeGlyphs.CreateBadgeIcon(glyph, color, kind, 32);
                    if (_badgeIcon == IntPtr.Zero || _trayBadgeIcon == IntPtr.Zero)
                        Logger.Warn($"Badge icon creation failed for {kind} (overlay={_badgeIcon != IntPtr.Zero}, tray={_trayBadgeIcon != IntPtr.Zero}).");

                    _trayIcon?.SetIcon(_trayBadgeIcon);
                }
                else
                {
                    _trayIcon?.SetIcon(IntPtr.Zero);
                }
            }

            if (kind == BadgeKind.None) _taskbarBadge?.Clear();
            else _taskbarBadge?.SetOverlay(_badgeIcon, kind);

            if (flash) _taskbarBadge?.Flash();
        }
        catch (Exception ex)
        {
            Logger.Warn($"App badge update failed: {ex.Message}");
        }
    }

    private void ReleaseBadgeIcons()
    {
        if (_badgeIcon != IntPtr.Zero) { DestroyIcon(_badgeIcon); _badgeIcon = IntPtr.Zero; }
        if (_trayBadgeIcon != IntPtr.Zero) { DestroyIcon(_trayBadgeIcon); _trayBadgeIcon = IntPtr.Zero; }
    }

    private void RestartApplication()
    {
        try
        {
            string? exePath = Environment.ProcessPath;
            if (!string.IsNullOrEmpty(exePath))
                System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo { FileName = exePath, UseShellExecute = true });
            else
                Logger.Error("RestartApplication: could not determine the running executable's path.");
        }
        catch (Exception ex)
        {
            Logger.Error("RestartApplication: failed to relaunch", ex);
        }

        ExitApplicationAsync();
    }

    private bool StartCaptureCore()
    {
        try
        {
            Directory.CreateDirectory(_tempDir);
            return _engine.Start(_tempDir);
        }
        catch (Exception ex)
        {
            Logger.Error("StartCapture failed", ex);
            return false;
        }
    }

    private async Task InitialStartAsync()
    {
        bool ok = await Task.Run(StartCaptureCore);
        SetRunningVisual(ok);

        if (ok) return;

        Logger.Warn("InitialStart: the capture engine did not start.");
        ShowInfoBar(InfoBarSeverity.Error, "Recording could not start",
            "Instant Replay could not start the capture engine. Check the debug log for details.", "start");
    }

    private void SetRunningVisual(bool running)
    {
        StopPulse(RecDot, ref _recDotPulseTimer);
        _lastBufferHealthy = running ? true : null;

        if (running)
        {
            StatusHeadline.Text = "RECORDING";
            RecDot.Fill = GetThemedBrush("RecBrush");
            RecDot.Opacity = 1;
            var t = _recDotPulseTimer;
            StartPulse(RecDot, ref t);
            _recDotPulseTimer = t;
            ApplyStartStopLook(true);
            _hasErrored = false;
            RefreshAppBadge();
        }
        else
        {
            StopPulse(RecDot, ref _recDotPulseTimer);
            RecDot.Fill = GetThemedBrush("TextMutedBrush");
            StatusHeadline.Text = _engine.IsRunning ? "RECORDING" : "STOPPED";
            ApplyStartStopLook(false);
            RefreshAppBadge();
        }
    }

    private void CheckBufferHealth()
    {

        if (!_engine.IsRunning)
        {
            if (_lastBufferHealthy != null) { _lastBufferHealthy = null; SetRunningVisual(false); }
            return;
        }

        bool healthy = _engine.IsBufferHealthy();
        if (_lastBufferHealthy == healthy) return;
        _lastBufferHealthy = healthy;

        if (healthy) { SetRunningVisual(true); return; }

        Logger.Warn("Buffer health check: capture looks stalled (process alive, but no fresh segments).");
        StopPulse(RecDot, ref _recDotPulseTimer);
        RecDot.Fill = GetThemedBrush("ErrorBrush");
        RecDot.Opacity = 1;
        StatusHeadline.Text = "ERROR";
        UpdateAppBadge(BadgeKind.Error, flash: true);

        if (_isHidden)
        {
            _lastBalloonWasLeakWarning = false;
            Notify("Instant Replay", "Error: recording is not running", TrayIcon.IconError);
        }
    }

    private void LogDebugStats()
    {
        if (!_engine.IsRunning) return;
        long memMb = Environment.WorkingSet / (1024 * 1024);
        Logger.Debug($"Buffer Status - Duration: {_settings.ReplayDurationSeconds}s, Memory: {memMb} MB");
    }

    private void CheckForMemoryLeak()
    {
        long memMb = Environment.WorkingSet / (1024 * 1024);
        if (memMb >= LeakThresholdMb) { _highMemStreak++; }
        else
        {
            _highMemStreak = 0;
            if (_leakWarningActive) StopLeakWarning();
            return;
        }

        if (_highMemStreak >= LeakConsecutiveSamplesRequired && !_leakWarningActive)
        {
            _leakWarningActive = true;
            Logger.Warn($"Memory usage has stayed above {LeakThresholdMb} MB for several minutes ({memMb} MB now) — possible leak.");
            _leakReminderTimer = new DispatcherTimer { Interval = LeakReminderInterval };
            _leakReminderTimer.Tick += (_, _) => ShowLeakReminder();
            _leakReminderTimer.Start();
            ShowLeakReminder();
        }
    }

    private void ShowLeakReminder()
    {
        _lastBalloonWasLeakWarning = true;
        Notify("Instant Replay",
            "Memory usage looks unusually high — consider restarting the app (right-click the tray icon → Restart App).",
            TrayIcon.IconWarning);
    }

    private void StopLeakWarning()
    {
        _leakWarningActive = false;
        _leakReminderTimer?.Stop();
        _leakReminderTimer = null;
        Logger.Info("Memory usage back to normal; leak warning cleared.");
    }

    private async Task SaveReplayAsync()
    {
        DateTime now = DateTime.UtcNow;
        TimeSpan sinceLast = now - _lastSaveAcceptedUtc;
        if (sinceLast < SaveCooldown)
        {
            Logger.Info($"Hotkey Triggered - Saving Replay (ignored, {(SaveCooldown - sinceLast).TotalSeconds:F1}s left in cooldown)");
            return;
        }
        _lastSaveAcceptedUtc = now;

        Logger.Info("Hotkey Triggered - Saving Replay");
        string destDir = _settings.GetEffectiveSaveDestination();
        Directory.CreateDirectory(destDir);

        var result = await Task.Run(() => _engine.SaveReplay(_tempDir, destDir));

        _lastSavedReplayPath = result.Success ? result.Path : null;

        if (result.Success && _engine.IsRunning)
        {
            Logger.Info("SaveReplay succeeded - starting a fresh buffer.");
            bool restarted = await Task.Run(() => _engine.Restart(_tempDir, wipeSegments: false));
            if (!restarted) Logger.Error("Failed to restart capture after save; buffer may be stale.");
        }

        _dq.TryEnqueue(() =>
        {
            _lastBalloonWasLeakWarning = false;
            ShowSaveResult(result);
        });
    }

    private void ShowSaveResult(SaveReplayResult result)
    {
        _lastBalloonWasLeakWarning = false;

        if (!result.Success)
        {
            Notify("Instant Replay", result.UserMessage(), TrayIcon.IconError);
            if (_isHidden || _isClosingForReal) return;
            ShowInfoBar(InfoBarSeverity.Error, "Replay not saved", result.UserMessage());
            return;
        }

        Notify("Instant Replay", result.UserMessage(), TrayIcon.IconInfo);
        if (_isHidden || _isClosingForReal || string.IsNullOrEmpty(result.Path)) return;

        string path = result.Path;
        ShowInfoBar(InfoBarSeverity.Success, "Replay saved",
            "Saved to " + path, "save",
            actionLabel: "Show folder", action: () => OpenContainingFolder(path));
    }

    private async void StartStopButton_Click(object sender, RoutedEventArgs e)
    {
        PlaySound(_engine.IsRunning ? ElementSoundKind.Hide : ElementSoundKind.Show);
        StartStopButton.IsEnabled = false;
        if (_engine.IsRunning)
        {
            SetStartStopLabel("STOPPING…");
            await Task.Run(() => _engine.Stop());
            SetRunningVisual(false);
        }
        else
        {
            SetStartStopLabel("STARTING…");
            bool ok = await Task.Run(StartCaptureCore);
            SetRunningVisual(ok);
        }
        StartStopButton.IsEnabled = true;
    }

    private void SaveReplayButton_Click(object sender, RoutedEventArgs e) => _ = SaveReplayAsync();

    private void RefreshMicrophonesInBackground()
    {
        _ = Task.Run(() =>
        {
            List<string> mics;
            try { mics = _engine.EnumerateMicrophones(); }
            catch (Exception ex) { Logger.Warn($"RefreshMicrophones failed: {ex.Message}"); return; }

            _dq.TryEnqueue(() =>
            {
                _knownMics = mics;
                _syncingMicCombo = true;
                try
                {
                    bool configuredMissing = !string.IsNullOrEmpty(_settings.Microphone) && !mics.Contains(_settings.Microphone);
                    _micMissing = configuredMissing;

                    if (mics.Count == 0)
                    {
                        if (!string.IsNullOrEmpty(_settings.Microphone))
                        {
                            MicCombo.ItemsSource = new List<string> { _settings.Microphone };
                            MicCombo.SelectedIndex = 0;
                            MicCombo.IsEnabled = true;
                        }
                        else
                        {
                            MicCombo.ItemsSource = new List<string> { "(no microphone found)" };
                            MicCombo.SelectedIndex = 0;
                            MicCombo.IsEnabled = false;
                        }
                        _micMissing = !string.IsNullOrEmpty(_settings.Microphone);
                    }
                    else
                    {
                        MicCombo.IsEnabled = true;
                        MicCombo.ItemsSource = mics;

                        if (string.IsNullOrEmpty(_settings.Microphone))
                        {
                            _settings.Microphone = mics[0];
                            _settings.SaveFields(nameof(Settings.Microphone));
                            _baseline = _baseline with { Microphone = mics[0] };
                            _micMissing = false;
                        }
                        else if (configuredMissing)
                        {
                            Logger.Warn($"Configured microphone '{_settings.Microphone}' is no longer available.");
                        }
                        else
                        {
                            _micMissing = false;
                        }

                        if (!string.IsNullOrEmpty(_settings.Microphone) && mics.Contains(_settings.Microphone))
                        {
                            MicCombo.ItemsSource = mics;
                            MicCombo.SelectedItem = _settings.Microphone;
                        }
                        else if (!string.IsNullOrEmpty(_settings.Microphone))
                        {
                            MicCombo.ItemsSource = new List<string>(mics) { _settings.Microphone };
                            MicCombo.SelectedItem = _settings.Microphone;
                            MicCombo.IsEnabled = true;
                        }
                        else
                        {
                            MicCombo.ItemsSource = mics;
                            MicCombo.SelectedItem = mics[0];
                        }
                    }

                    UpdateMicComboStyle();
                }
                finally
                {
                    _syncingMicCombo = false;
                }
            });
        });
    }

    private void UpdateMicComboStyle()
    {
        if (MicCombo == null) return;
        if (_micMissing)
        {
            try { MicCombo.Foreground = new Microsoft.UI.Xaml.Media.SolidColorBrush(Windows.UI.Color.FromArgb(255, 229, 72, 77)); }
            catch { }
            try { MicCombo.IsEnabled = true; } catch { }

        }
        else
        {
            try { MicCombo.ClearValue(Microsoft.UI.Xaml.Controls.ComboBox.ForegroundProperty); }
            catch { }
        }
    }

    private void MicCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_initializing || _syncingMicCombo) return;
        string mic = MicCombo.SelectedItem as string ?? "";
        if (string.IsNullOrEmpty(mic)) return;
        if (string.Equals(mic, _settings.Microphone, StringComparison.Ordinal)) return;
        _settings.Microphone = mic;
        _micMissing = _knownMics.Count > 0 && !_knownMics.Contains(mic);
        UpdateMicComboStyle();
        CheckDirty();
    }

    private void InitDurationSegments()
    {
        DurationCombo.SelectedIndex = _settings.ReplayDurationSeconds switch { 30 => 0, 120 => 2, _ => 1 };
    }

    private void DurationCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_initializing) return;
        int seconds = DurationCombo.SelectedIndex switch { 0 => 30, 2 => 120, _ => 60 };
        _settings.ReplayDurationSeconds = seconds;
        CheckDirty();
    }

    private void InitFpsSegments()
    {
        FpsCombo.SelectedIndex = _settings.FrameRate switch { 60 => 1, 120 => 2, _ => 0 };
    }

    private void FpsCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_initializing) return;
        int fps = FpsCombo.SelectedIndex switch { 1 => 60, 2 => 120, _ => 30 };
        _settings.FrameRate = fps;
        CheckDirty();
    }

    private void InitQualitySegments()
    {
        QualityCombo.SelectedIndex = _settings.QualityPreset switch { "Balanced" => 1, "Quality" => 2, _ => 0 };
    }

    private void QualityCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_initializing) return;
        string preset = QualityCombo.SelectedIndex switch { 1 => "Balanced", 2 => "Quality", _ => "Fast" };
        _settings.QualityPreset = preset;
        CheckDirty();
    }

    private void InitAudioToggles()
    {
        _syncingAudioToggles = true;
        SysAudioToggle.IsOn = _settings.RecordSystemAudio;
        MicToggle.IsOn = _settings.RecordMicrophone;
        _syncingAudioToggles = false;
    }

    private void InitEncoderSegments()
    {
        EncoderCombo.SelectedIndex = _settings.EncoderChoice switch { "Hardware" => 1, "Software" => 2, _ => 0 };
    }

    private void EncoderCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_initializing) return;
        string choice = EncoderCombo.SelectedIndex switch { 1 => "Hardware", 2 => "Software", _ => "Auto" };
        _settings.EncoderChoice = choice;
        CheckDirty();
    }

    private void InitCaptureSegments()
    {
        bool dxgiOk = false;
        try { dxgiOk = ReplayEngine.DxgiDuplicationSupported(); }
        catch (Exception ex)
        {
            Logger.Warn($"[Engine] DXGI availability probe failed ({ex.Message}); hiding DXGI option.");
        }

        CaptureDxgiItem.IsEnabled = dxgiOk;
        CaptureMethodCombo.SelectedIndex = _settings.CaptureMethod == "Dxgi" && dxgiOk ? 1 : 0;

        UpdateCaptureMethodHint();
    }

    private void UpdateCaptureMethodHint()
    {
    }

    private void CaptureMethodCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_initializing) return;

        string choice = CaptureMethodCombo.SelectedIndex == 1 ? "Dxgi" : "Wgc";
        UpdateCaptureMethodHint();
        _settings.CaptureMethod = choice;
        CheckDirty();
    }

    private void SysAudioToggle_Changed(object sender, RoutedEventArgs e)
    {
        if (_initializing || _syncingAudioToggles) return;
        _settings.RecordSystemAudio = SysAudioToggle.IsOn;
        CheckDirty();
    }

    private void MicToggle_Changed(object sender, RoutedEventArgs e)
    {
        if (_initializing || _syncingAudioToggles) return;
        _settings.RecordMicrophone = MicToggle.IsOn;
        CheckDirty();
    }

    private void HotkeyCaptureButton_Click(object sender, RoutedEventArgs e)
    {
        if (_capturingHotkey) return;
        BeginHotkeyCapture();
    }

    private void BeginHotkeyCapture()
    {
        _capturingHotkey = true;
        _pendingHotkeyModifier = 0;
        HotkeyCaptureButton.Style = (Style)Application.Current.Resources["AccentButtonStyle"];
        HotkeyCaptureText.Text = "PRESS A KEY…";
        var t = _hotkeyPulseTimer;
        StartPulse(HotkeyCaptureButton, ref t);
        _hotkeyPulseTimer = t;
        HotkeyCaptureButton.Focus(FocusState.Programmatic);
    }

    private void EndHotkeyCapture()
    {
        _capturingHotkey = false;
        _pendingHotkeyModifier = 0;
        StopPulse(HotkeyCaptureButton, ref _hotkeyPulseTimer);
        HotkeyCaptureButton.Style = (Style)Application.Current.Resources["BaseButtonStyle"];
        UpdateHotkeyButtonDisplay();
    }

    private void UpdateHotkeyButtonDisplay() =>
        HotkeyCaptureText.Text = HotkeyDisplay.Format(_settings.HotkeyModifiers, _settings.HotkeyVk);

    private void Window_PreviewKeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (!_capturingHotkey) return;
        var key = e.Key;

        if (key == VirtualKey.Escape)
        {
            e.Handled = true;
            EndHotkeyCapture();
            return;
        }

        uint? mod = HotkeyDisplay.ModifierBitForKey(key);
        if (mod != null)
        {
            _pendingHotkeyModifier = mod.Value;
            HotkeyCaptureText.Text = HotkeyDisplay.Format(_pendingHotkeyModifier, 0);
            e.Handled = true;
            return;
        }

        uint vk = (uint)key;
        e.Handled = true;
        FinalizeHotkey(_pendingHotkeyModifier, vk);
    }

    private void Window_PreviewKeyUp(object sender, KeyRoutedEventArgs e)
    {
        if (!_capturingHotkey || _pendingHotkeyModifier == 0) return;
        if (HotkeyDisplay.ModifierBitForKey(e.Key) == _pendingHotkeyModifier)
        {
            _pendingHotkeyModifier = 0;
            HotkeyCaptureText.Text = "PRESS A KEY…";
            e.Handled = true;
        }
    }

    private void FinalizeHotkey(uint modifiers, uint vk)
    {
        uint oldModifiers = _settings.HotkeyModifiers;
        uint oldVk = _settings.HotkeyVk;

        UnregisterHotKey(_hwnd, HotkeyId);

        if (!RegisterHotKey(_hwnd, HotkeyId, modifiers, vk))
        {
            RegisterHotKey(_hwnd, HotkeyId, oldModifiers, oldVk);
            Logger.Warn($"Hotkey {HotkeyDisplay.Format(modifiers, vk)} is already in use by another app.");

            _capturingHotkey = false;
            _pendingHotkeyModifier = 0;
            StopPulse(HotkeyCaptureButton, ref _hotkeyPulseTimer);
            HotkeyCaptureButton.Style = (Style)Application.Current.Resources["BaseButtonStyle"];
            HotkeyCaptureText.Text = "ALREADY IN USE";

            var revertTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(1.5) };
            revertTimer.Tick += (_, _) => { revertTimer.Stop(); UpdateHotkeyButtonDisplay(); };
            revertTimer.Start();
            return;
        }

        _settings.HotkeyModifiers = modifiers;
        _settings.HotkeyVk = vk;
        _settings.SaveFields(nameof(Settings.HotkeyModifiers), nameof(Settings.HotkeyVk));
        EndHotkeyCapture();
    }

    private void StartupToggle_Changed(object sender, RoutedEventArgs e)
    {
        if (_initializing || _syncingStartupToggle) return;
        bool enabled = StartupToggle.IsOn;
        StartupManager.SetEnabled(enabled);
    }

    private void RefreshStartupToggleState()
    {
        bool actuallyEnabled = StartupManager.IsEnabled();
        if (StartupToggle.IsOn == actuallyEnabled) return;

        _syncingStartupToggle = true;
        StartupToggle.IsOn = actuallyEnabled;
        _syncingStartupToggle = false;
    }

    private void DebugToggle_Changed(object sender, RoutedEventArgs e)
    {
        if (_initializing) return;
        bool enabled = DebugToggle.IsOn;
        _settings.DebugLogging = enabled;
        Logger.SetDebugMode(enabled);
        _settings.SaveFields(nameof(Settings.DebugLogging));
    }

    private void BackdropCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_initializing) return;
        _settings.Backdrop = BackdropCombo.SelectedIndex switch { 1 => BackdropKind.Mica, _ => BackdropKind.Acrylic };
        _settings.SaveFields(nameof(Settings.Backdrop));
        _dq.TryEnqueue(() => ApplyBackdrop());
    }

    private void ThemeToggle_Changed(object sender, RoutedEventArgs e)
    {
        if (_initializing) return;
        bool dark = ThemeToggle.IsOn;
        _settings.DarkMode = dark;
        _settings.SaveFields(nameof(Settings.DarkMode));
        _dq.TryEnqueue(() =>
        {
            ApplyAppTheme(dark);
            ApplyBackdrop();
        });
    }

    private void SaveLocationBox_TextChanged(object sender, TextChangedEventArgs e)
    {
        if (_initializing) return;
        _settings.SaveDestination = SaveLocationBox.Text;
        CheckDirty();
    }

    private async void BrowseSaveLocationButton_Click(object sender, RoutedEventArgs e)
    {
        try
        {
            var picker = new Windows.Storage.Pickers.FolderPicker();
            InitializeWithWindow.Initialize(picker, _hwnd);
            picker.FileTypeFilter.Add("*");
            picker.SuggestedStartLocation = Windows.Storage.Pickers.PickerLocationId.ComputerFolder;

            var folder = await picker.PickSingleFolderAsync();
            if (folder != null) SaveLocationBox.Text = folder.Path;
        }
        catch (Exception ex)
        {
            Logger.Warn($"Could not open save location folder picker: {ex}");
        }
    }

    private void CheckDirty()
    {
        _dirty = CapturePending() != _baseline;
        UpdatePendingFlyout();
    }

    private static void PlaySound(ElementSoundKind kind)
    {
        try { ElementSoundPlayer.Play(kind); } catch { }
    }

    private void EnsureInfoBarTimer()
    {
        if (_infoBarTimer != null) return;

        _infoBarTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(3) };
        _infoBarTimer.Tick += (_, _) =>
        {
            _infoBarTimer!.Stop();
            ClearInfoBar();
        };

        StatusInfoBar.PointerEntered += (_, _) => _infoBarTimer?.Stop();
        StatusInfoBar.PointerExited += (_, _) =>
        {
            if (!StatusInfoBar.IsOpen) return;
            _infoBarTimer?.Stop();
            _infoBarTimer?.Start();
        };
    }

    private void ShowInfoBar(InfoBarSeverity severity, string title, string message, string? tag = null,
        bool progress = false, string? actionLabel = null, Action? action = null)
    {
        StatusInfoBar.Severity = severity;
        StatusInfoBar.Title = title;
        StatusInfoBar.Message = message;
        StatusInfoBar.IsClosable = !progress;
        StatusInfoBar.Tag = tag;
        _infoBarAction = action;
        StatusInfoBar.ActionButton.Content = actionLabel ?? "";
        StatusInfoBar.ActionButton.Visibility = actionLabel is null
            ? Visibility.Collapsed
            : Visibility.Visible;
        StatusInfoBar.IsOpen = true;

        EnsureInfoBarTimer();
        _infoBarTimer!.Stop();
        _infoBarTimer.Start();
    }

    private void ClearInfoBar()
    {
        _infoBarTimer?.Stop();
        StatusInfoBar.IsOpen = false;
        StatusInfoBar.Tag = null;
        _infoBarAction = null;
        StatusInfoBar.ActionButton.Visibility = Visibility.Collapsed;
    }

    private void UpdatePendingFlyout()
    {
        if (_dirty == _flyoutShown) return;
        _flyoutShown = _dirty;

        if (_dirty)
        {
            PlaySound(ElementSoundKind.Show);
            PendingChangesFlyout.Visibility = Visibility.Visible;
            AnimateDouble(PendingChangesFlyout, "Opacity", 0, 1, TimeSpan.FromMilliseconds(160));
            AnimateDouble(PendingFlyoutSlide, "Y", -8, 0, TimeSpan.FromMilliseconds(200));
            return;
        }

        AnimateDouble(PendingChangesFlyout, "Opacity", 1, 0, TimeSpan.FromMilliseconds(140),
            onCompleted: () => PendingChangesFlyout.Visibility = Visibility.Collapsed);
    }

    private void RevertPendingChanges()
    {
        PendingSettings previous = _baseline;

        _settings.ReplayDurationSeconds = previous.ReplayDurationSeconds;
        _settings.FrameRate = previous.FrameRate;
        _settings.QualityPreset = previous.QualityPreset;
        _settings.EncoderChoice = previous.EncoderChoice;
        _settings.CaptureMethod = previous.CaptureMethod;
        _settings.Microphone = previous.Microphone;
        _settings.SaveDestination = previous.SaveDestination;
        _settings.RecordSystemAudio = previous.RecordSystemAudio;
        _settings.RecordMicrophone = previous.RecordMicrophone;

        SyncControlsFromSettings();

        Logger.Info("RevertPendingChanges: restored the previous settings values.");
        CheckDirty();
    }

    private void SyncControlsFromSettings()
    {
        bool wasInitializing = _initializing;
        _initializing = true;
        try
        {
            if (MicCombo.ItemsSource is IEnumerable<string> mics && mics.Contains(_settings.Microphone))
                MicCombo.SelectedItem = _settings.Microphone;

            MicToggle.IsOn = _settings.RecordMicrophone;
            SysAudioToggle.IsOn = _settings.RecordSystemAudio;
            InitDurationSegments();
            InitFpsSegments();
            InitQualitySegments();
            SaveLocationBox.Text = _settings.GetEffectiveSaveDestination();
            InitEncoderSegments();
            CaptureMethodCombo.SelectedIndex =
                _settings.CaptureMethod == "Dxgi" && CaptureDxgiItem.IsEnabled ? 1 : 0;
            UpdateCaptureMethodHint();
        }
        finally
        {
            _initializing = wasInitializing;
        }

    }

    private async void PendingSaveButton_Click(object sender, RoutedEventArgs e) => await SaveChangesAsync();

    private void PendingDiscardButton_Click(object sender, RoutedEventArgs e)
    {
        PlaySound(ElementSoundKind.Hide);
        if (_savingChanges) return;
        RevertPendingChanges();
        ClearInfoBar();
    }

    private async Task SaveChangesAsync()
    {
        if (_savingChanges || !_dirty) return;
        _savingChanges = true;
        PendingSaveButton.IsEnabled = false;
        PendingDiscardButton.IsEnabled = false;

        ShowInfoBar(InfoBarSeverity.Informational, "Saving changes",
            "Saving your settings and restarting the capture engine…", "apply", progress: true);

        bool ok = true;
        bool restarted = false;
        try
        {
            _settings.Save();

            if (_engine.IsRunning)
            {
                restarted = true;
                ok = await Task.Run(() => _engine.Restart(_tempDir, wipeSegments: true));
            }

            _baseline = CapturePending();
            _dirty = false;
            UpdatePendingFlyout();

            if (ok)
            {
                ShowInfoBar(InfoBarSeverity.Success, "Engine started",
                    restarted
                        ? "Your changes were saved and the capture engine successfully started."
                        : "Your changes were saved. They take effect the next time you start recording.");
            }
            else
            {
                Logger.Warn("Save changes: capture failed to restart with the new settings (see log).");
                ShowInfoBar(InfoBarSeverity.Error, "Engine failed to start",
                    "Your changes were saved, but the capture engine could not start with them. Check the debug log for details.");
            }
        }
        catch (Exception ex)
        {
            Logger.Error("Save changes failed", ex);
            ShowInfoBar(InfoBarSeverity.Error, "Could not save changes",
                $"{ex.Message} Check the debug log for details.");
        }
        finally
        {
            PendingSaveButton.IsEnabled = true;
            PendingDiscardButton.IsEnabled = true;
            _savingChanges = false;
            SetRunningVisual(_engine.IsRunning);
        }
    }


    private static readonly SolidColorBrush AlertBrush = MakeFrozen(225, 91, 91);

    private static SolidColorBrush MakeFrozen(byte r, byte g, byte b)
    {
        return new SolidColorBrush(Color.FromArgb(255, r, g, b));
    }

    private void UpdateSystemStats()
    {
        double? cpuPct = null;
        try
        {
            if (GetSystemTimes(out var idle, out var kernel, out var user))
            {
                ulong i = idle.Ticks, k = kernel.Ticks, u = user.Ticks;
                if (_statsBaselineTaken && k >= _lastKernelTicks && u >= _lastUserTicks && i >= _lastIdleTicks)
                {
                    ulong dTotal = (k - _lastKernelTicks) + (u - _lastUserTicks);
                    ulong dIdle = i - _lastIdleTicks;
                    if (dTotal > 0 && dTotal >= dIdle)
                        cpuPct = 100.0 * (dTotal - dIdle) / dTotal;
                }
                _lastIdleTicks = i; _lastKernelTicks = k; _lastUserTicks = u;
                _statsBaselineTaken = true;
            }
        }
        catch (Exception ex) { Logger.Warn($"CPU stats failed: {ex.Message}"); }

        if (cpuPct is { } pct)
        {
            CpuValueText.Text = $"{pct:0}%";
            ApplyStatAlert(CpuCard, CpuIconBg, CpuValueText, CpuHintText, pct >= 70);
        }
        else
        {
            CpuValueText.Text = "\u2014";
        }

        try
        {
            var m = new MEMORYSTATUSEX { dwLength = (uint)Marshal.SizeOf<MEMORYSTATUSEX>() };
            if (GlobalMemoryStatusEx(ref m))
            {
                double totalGb = m.ullTotalPhys / (1024.0 * 1024 * 1024);
                double usedGb = (m.ullTotalPhys - m.ullAvailPhys) / (1024.0 * 1024 * 1024);
                RamValueText.Text = $"{usedGb:0.0} / {totalGb:0} GB";
                RamHintText.Text = $"{m.dwMemoryLoad}% used";
                ApplyStatAlert(RamCard, RamIconBg, RamValueText, RamHintText, m.dwMemoryLoad >= 85);
            }
        }
        catch (Exception ex) { Logger.Warn($"Memory stats failed: {ex.Message}"); }
    }

    private void ApplyStatAlert(Border card, Border icon, TextBlock value, TextBlock hint, bool alert)
    {
        card.BorderBrush = alert ? AlertBrush : (Brush)Application.Current.Resources["SurfaceBorderBrush"];
        icon.Background = alert ? AlertBrush : (Brush)Application.Current.Resources["AccentGradientBrush"];
        bool dark = _settings.DarkMode;
        if (RootGrid.RequestedTheme == ElementTheme.Dark || Application.Current.RequestedTheme == ApplicationTheme.Dark) dark = true;
        if (RootGrid.RequestedTheme == ElementTheme.Light || Application.Current.RequestedTheme == ApplicationTheme.Light) dark = false;
        value.Foreground = alert ? AlertBrush : (dark ? new SolidColorBrush(Colors.White) : new SolidColorBrush(Colors.Black));
        hint.Foreground = alert ? AlertBrush : (dark ? new SolidColorBrush(Color.FromArgb(255, 212, 212, 212)) : new SolidColorBrush(Color.FromArgb(255, 64, 64, 64)));
    }

    private void ExitApplicationAsync()
    {
        if (_isClosingForReal) return;

        if (_dirty) RevertPendingChanges();

        _isClosingForReal = true;

        UnregisterHotKey(_hwnd, HotkeyId);

        _debugStatsTimer?.Stop();
        _bufferHealthTimer?.Stop();
        _statsTimer?.Stop();
        _micRefreshTimer?.Stop();
        _deviceChangeDebounceTimer?.Stop();
        _audioWatcher?.Dispose();
        Microsoft.Win32.SystemEvents.UserPreferenceChanged -= SystemEvents_UserPreferenceChanged;
        _leakReminderTimer?.Stop();
        _recDotPulseTimer?.Stop();
        _hotkeyPulseTimer?.Stop();
        EnableWindow(_hwnd, true);
        _engine.Stop();
        _engine.Dispose();

        _taskbarBadge?.Dispose();
        ReleaseBadgeIcons();
        _trayIcon?.Dispose();

        void FinishExit()
        {
            try { Close(); }
            catch (Exception ex) { Logger.Warn($"Window.Close() during exit failed: {ex.Message}"); }
            Environment.Exit(0);
        }

        if (!_dq.TryEnqueue(FinishExit)) FinishExit();
    }
}