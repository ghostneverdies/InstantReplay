using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Text.Json;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Threading.Tasks;
using Microsoft.UI;
using Microsoft.UI.Composition.SystemBackdrops;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Animation;
using Windows.ApplicationModel.DataTransfer;
using Windows.Graphics;
using Windows.System;
using WinRT.Interop;
using Brush = Microsoft.UI.Xaml.Media.Brush;
using Color = Windows.UI.Color;

namespace InstantReplay;

public partial class MainWindow : Window
{
    [DllImport("user32.dll")] private static extern bool RegisterHotKey(IntPtr hWnd, int id, uint fsModifiers, uint vk);
    [DllImport("user32.dll")] private static extern bool UnregisterHotKey(IntPtr hWnd, int id);
    [DllImport("user32.dll")] private static extern IntPtr SetWindowLongPtr(IntPtr hWnd, int nIndex, IntPtr dwNewLong);
    [DllImport("user32.dll")] private static extern IntPtr CallWindowProc(IntPtr lpPrevWndFunc, IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll")] private static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] private static extern uint GetDpiForWindow(IntPtr hWnd);

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

    private const int OffsetStepMs = 10;
    private const int OffsetMinMs = -2000;
    private const int OffsetMaxMs = 2000;

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

    private bool _capturingHotkey;
    private uint _pendingHotkeyModifier;

    private bool _initializing = true;
    private bool _dirty;
    private string _settingsSnapshot = "";

    private FrameworkElement? _currentPage;

    private bool _navCollapsed;
    private const double NavExpandedWidth = 185;
    private const double NavCollapsedWidth = 56;

    private WndProcDelegate? _wndProcDelegate;
    private IntPtr _originalWndProc;

    public MainWindow(bool startMinimized = false)
    {
        _startMinimized = startMinimized;
        InitializeComponent();

        Title = "Instant Replay";
        _hwnd = WindowNative.GetWindowHandle(this);
        _appWindow = AppWindow.GetFromWindowId(Win32Interop.GetWindowIdFromWindow(_hwnd));
        _dq = Microsoft.UI.Dispatching.DispatcherQueue.GetForCurrentThread();

        SetInitialWindowSizeAndIcon();
        SubclassWndProc();

        _settings = Settings.Load();
        Logger.Init(_settings.DebugLogging);
        _settingsSnapshot = SnapshotJson();

        _tempDir = Path.Combine(Path.GetTempPath(), "instant replay");
        _engine = new RecorderEngine(_settings);

        _currentPage = HomePage;

        RefreshMicrophonesInBackground();
        InitDurationSegments();
        InitFpsSegments();
        InitQualitySegments();
        InitOffsetDisplays();
        UpdateHotkeyButtonDisplay();
        UpdateSessionInfoDisplay();

        StartupToggle.IsOn = StartupManager.IsEnabled();
        DebugToggle.IsOn = _settings.DebugLogging;
        TransparencyToggle.IsOn = _settings.WindowTransparency;
        ThemeToggle.IsOn = _settings.DarkMode;
        FFmpegPathBox.Text = _settings.FFmpegPath;
        SaveLocationBox.Text = _settings.GetEffectiveSaveDestination();

        _initializing = false;

        if (!RegisterHotKey(_hwnd, HotkeyId, _settings.HotkeyModifiers, _settings.HotkeyVk))
            Logger.Warn($"Could not register global hotkey {HotkeyDisplay.Format(_settings.HotkeyModifiers, _settings.HotkeyVk)} (may already be in use).");

        ApplyAppTheme(_settings.DarkMode);
        ApplyBackdrop(_settings.WindowTransparency);
        SyncTransparencyToggle();

        SetupTrayIcon();

        _debugStatsTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(30) };
        _debugStatsTimer.Tick += (_, _) => { LogDebugStats(); CheckForMemoryLeak(); };
        _debugStatsTimer.Start();

        _bufferHealthTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(3) };
        _bufferHealthTimer.Tick += (_, _) => CheckBufferHealth();
        _bufferHealthTimer.Start();

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
                _isHidden = true;
            });
        }
    }

    private void SetInitialWindowSizeAndIcon()
    {
        uint dpi = GetDpiForWindow(_hwnd);
        double scale = dpi / 96.0;
        _appWindow.ResizeClient(new SizeInt32((int)(941 * scale), (int)(806 * scale)));
        try { _appWindow.SetIcon("icon.ico"); }
        catch (Exception ex) { Logger.Warn($"Could not set window icon: {ex.Message}"); }

        if (_appWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.IsResizable = false;
            presenter.IsMaximizable = false;
            presenter.IsMinimizable = true;
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
        if (msg == TrayIcon.WM_TRAYICON)
        {
            _trayIcon?.HandleMessage(lParam);
            return IntPtr.Zero;
        }
        return CallWindowProc(_originalWndProc, hWnd, msg, wParam, lParam);
    }

    private void ApplyBackdrop(bool enabled)
    {
        if (enabled)
        {
            SystemBackdrop = new MicaBackdrop { Kind = MicaKind.Base };
            RootGrid.Background = new SolidColorBrush(Colors.Transparent);
            NavSidebar.Background = new SolidColorBrush(Colors.Transparent);
        }
        else
        {
            SystemBackdrop = null;
            RootGrid.Background = GetThemedBrush("BgBrush");
            NavSidebar.Background = GetThemedBrush("SurfaceBrush");
        }
    }

    private void ApplyAppTheme(bool dark)
    {
        RootGrid.RequestedTheme = dark ? ElementTheme.Dark : ElementTheme.Light;

        if (_appWindow.TitleBar != null)
            _appWindow.TitleBar.PreferredTheme = dark ? TitleBarTheme.Dark : TitleBarTheme.Light;
    }

    private void ApplySystemAccent() { }

    private void SystemEvents_UserPreferenceChanged(object? sender, Microsoft.Win32.UserPreferenceChangedEventArgs e)
    {
        if (e.Category is Microsoft.Win32.UserPreferenceCategory.General or Microsoft.Win32.UserPreferenceCategory.Color)
            _dq.TryEnqueue(() => { ApplySystemAccent(); SyncTransparencyToggle(); });
    }

    private void SyncTransparencyToggle()
    {
        bool osOk = WindowsTheme.IsTransparencyEnabled();
        TransparencyToggle.IsEnabled = osOk;
        if (!osOk && _settings.WindowTransparency)
        {
            TransparencyToggle.IsOn = false;
            _settings.WindowTransparency = false;
            _settings.Save();
            ApplyBackdrop(false);
        }
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

    private string SnapshotJson() => System.Text.Json.JsonSerializer.Serialize(new
    {
        _settings.ReplayDurationSeconds,
        _settings.FrameRate,
        _settings.QualityPreset,
        _settings.Microphone,
        _settings.SaveDestination,
        _settings.FFmpegPath,
        _settings.SystemAudioOffsetMs,
        _settings.MicOffsetMs,
        _settings.HotkeyModifiers,
        _settings.HotkeyVk
    });

    private void CloseButton_Click()
    {
        if (_appWindow.Presenter is OverlappedPresenter presenter) presenter.Minimize();

        var hideTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(220) };
        hideTimer.Tick += (_, _) =>
        {
            hideTimer.Stop();
            ShowWindow(_hwnd, SwHide);
            _isHidden = true;
            _lastBalloonWasLeakWarning = false;
            _trayIcon?.ShowBalloonTip("Instant Replay", "Instant Replay is minimized to tray", TrayIcon.IconInfo);
        };
        hideTimer.Start();
    }

    private void Window_ClosedRequested(object sender, WindowEventArgs args)
    {
        if (_isClosingForReal) return;
        args.Handled = true;
        CloseButton_Click();
    }

    private void Window_Activated(object sender, WindowActivatedEventArgs args)
    {
        if (args.WindowActivationState == WindowActivationState.Deactivated && _capturingHotkey)
            EndHotkeyCapture();
    }

    private void NavToggleButton_Click(object sender, RoutedEventArgs e)
    {
        _navCollapsed = !_navCollapsed;
        ToolTipService.SetToolTip(NavToggleButton, _navCollapsed ? "Expand menu" : "Collapse menu");

        double targetWidth = _navCollapsed ? NavCollapsedWidth : NavExpandedWidth;
        AnimateNavWidth(NavSidebar.Width, targetWidth);

        string sidebarState = _navCollapsed ? "SidebarCollapsed" : "SidebarExpanded";
        foreach (var item in new Control[] { NavHome, NavSettings, NavAdvanced, NavDonate })
            VisualStateManager.GoToState(item, sidebarState, true);

        var labels = new[] { NavHomeLabel, NavSettingsLabel, NavAdvancedLabel, NavDonateLabel };
        foreach (var label in labels)
        {
            if (_navCollapsed)
            {
                AnimateDouble(label, "Opacity", label.Opacity, 0, TimeSpan.FromMilliseconds(90),
                    onCompleted: () => label.Visibility = Visibility.Collapsed);
            }
            else
            {
                label.Visibility = Visibility.Visible;
                AnimateDouble(label, "Opacity", 0, 1, TimeSpan.FromMilliseconds(160), TimeSpan.FromMilliseconds(60));
            }
        }
    }

    private void Nav_Checked(object sender, RoutedEventArgs e)
    {
        if (_initializing) return;
        FrameworkElement target = sender switch
        {
            _ when ReferenceEquals(sender, NavSettings) => SettingsPage,
            _ when ReferenceEquals(sender, NavAdvanced) => AdvancedPage,
            _ when ReferenceEquals(sender, NavDonate) => DonatePage,
            _ => HomePage,
        };
        NavigateTo(target);
    }

    private void NavigateTo(FrameworkElement newPage)
    {
        if (ReferenceEquals(newPage, _currentPage)) return;
        var oldPage = _currentPage;
        _currentPage = newPage;

        if (oldPage != null)
            oldPage.Visibility = Visibility.Collapsed;

        newPage.Visibility = Visibility.Visible;
        newPage.Opacity = 0;
        AnimateDouble(newPage, "Opacity", 0, 1, TimeSpan.FromMilliseconds(150));
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

    private void AnimateNavWidth(double from, double to)
    {
        var anim = new DoubleAnimation
        {
            From = from,
            To = to,
            Duration = TimeSpan.FromMilliseconds(180),
            EasingFunction = new QuadraticEase { EasingMode = EasingMode.EaseOut },
            EnableDependentAnimation = true
        };
        Storyboard.SetTarget(anim, NavSidebar);
        Storyboard.SetTargetProperty(anim, "Width");
        var sb = new Storyboard();
        sb.Children.Add(anim);
        sb.Completed += (_, _) => NavSidebar.Width = to;
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

    private void UpdateSessionInfoDisplay()
    {
        string mic = string.IsNullOrEmpty(_settings.Microphone) ? "Desktop audio only" : _settings.Microphone;
        SessionMicText.Text = mic;
        SessionDurationText.Text = $"{_settings.ReplayDurationSeconds}s";
        SessionFpsText.Text = $"{_settings.FrameRate} FPS";
        SessionQualityText.Text = _settings.QualityPreset;
        SessionHotkeyText.Text = HotkeyDisplay.Format(_settings.HotkeyModifiers, _settings.HotkeyVk);
    }

    private void SetupTrayIcon()
    {
        string iconPath = Path.Combine(AppContext.BaseDirectory, "icon.ico");
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
            new() { Text = "Exit", OnClick = () => _ = ExitApplicationAsync() },
        };
        _trayIcon.DoubleClicked += ShowFromTray;
        _trayIcon.BalloonClicked += () => { if (_lastBalloonWasLeakWarning) RestartApplication(); };
    }

    private void ShowFromTray()
    {
        ShowWindow(_hwnd, SwShow);
        _isHidden = false;
        if (_appWindow.Presenter is OverlappedPresenter presenter) presenter.Restore();
        Activate();
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

        _isClosingForReal = true;
        _ = ExitApplicationAsync();
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
            StartStopButton.Content = "STOP";
        }
        else
        {
            StopPulse(RecDot, ref _recDotPulseTimer);
            RecDot.Fill = GetThemedBrush("TextMutedBrush");
            StatusHeadline.Text = _engine.IsRunning ? "RECORDING" : "STOPPED";
            StartStopButton.Content = "START";
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

        if (_isHidden)
        {
            _lastBalloonWasLeakWarning = false;
            _trayIcon?.ShowBalloonTip("Instant Replay", "Error: recording is not running", TrayIcon.IconError);
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
        _trayIcon?.ShowBalloonTip("Instant Replay",
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
        string message = result.UserMessage();

        _dq.TryEnqueue(() =>
        {
            _lastBalloonWasLeakWarning = false;
            _trayIcon?.ShowBalloonTip("Instant Replay", message, result.Success ? TrayIcon.IconInfo : TrayIcon.IconError);
        });

        if (result.Success && _engine.IsRunning)
        {
            Logger.Info("SaveReplay succeeded - starting a fresh buffer.");
            bool restarted = await Task.Run(() => _engine.Restart(_tempDir));
            if (!restarted) Logger.Error("Failed to restart capture after save; buffer may be stale.");
        }
    }

    private async void StartStopButton_Click(object sender, RoutedEventArgs e)
    {
        StartStopButton.IsEnabled = false;
        if (_engine.IsRunning)
        {
            StartStopButton.Content = "STOPPING…";
            await Task.Run(() => _engine.Stop());
            SetRunningVisual(false);
        }
        else
        {
            StartStopButton.Content = "STARTING…";
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
                bool wasInitializing = _initializing;
                _initializing = true;

                if (mics.Count == 0)
                {
                    MicCombo.ItemsSource = new List<string> { "(no microphone found)" };
                    MicCombo.SelectedIndex = 0;
                    MicCombo.IsEnabled = false;
                }
                else
                {
                    MicCombo.IsEnabled = true;
                    MicCombo.ItemsSource = mics;

                    if (string.IsNullOrEmpty(_settings.Microphone))
                    {
                        _settings.Microphone = mics[0];
                        _settings.Save();
                    }
                    else if (!mics.Contains(_settings.Microphone))
                    {
                        Logger.Warn($"Configured microphone '{_settings.Microphone}' is no longer available.");
                    }

                    MicCombo.SelectedItem = mics.Contains(_settings.Microphone) ? _settings.Microphone : mics[0];
                }

                _initializing = wasInitializing;
                UpdateSessionInfoDisplay();
            });
        });
    }

    private void MicCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_initializing) return;
        string mic = MicCombo.SelectedItem as string ?? "";
        if (string.IsNullOrEmpty(mic) || mic == _settings.Microphone) return;
        _settings.Microphone = mic;
        CheckDirty();
    }

    private void InitDurationSegments()
    {
        var target = _settings.ReplayDurationSeconds switch { 30 => Duration30, 120 => Duration120, _ => Duration60 };
        target.IsChecked = true;
    }

    private void DurationSegment_Checked(object sender, RoutedEventArgs e)
    {
        if (_initializing) return;
        int seconds = sender switch
        {
            _ when ReferenceEquals(sender, Duration30) => 30,
            _ when ReferenceEquals(sender, Duration120) => 120,
            _ => 60,
        };
        if (seconds == _settings.ReplayDurationSeconds) return;
        _settings.ReplayDurationSeconds = seconds;
        CheckDirty();
    }

    private void InitFpsSegments()
    {
        var target = _settings.FrameRate switch { 60 => Fps60, 120 => Fps120, _ => Fps30 };
        target.IsChecked = true;
    }

    private void FpsSegment_Checked(object sender, RoutedEventArgs e)
    {
        if (_initializing) return;
        int fps = sender switch
        {
            _ when ReferenceEquals(sender, Fps60) => 60,
            _ when ReferenceEquals(sender, Fps120) => 120,
            _ => 30,
        };
        if (fps == _settings.FrameRate) return;
        _settings.FrameRate = fps;
        CheckDirty();
    }

    private void InitQualitySegments()
    {
        var target = _settings.QualityPreset switch { "Balanced" => QualityBalanced, "Quality" => QualityHigh, _ => QualityFast };
        target.IsChecked = true;
    }

    private void QualitySegment_Checked(object sender, RoutedEventArgs e)
    {
        if (_initializing) return;
        string preset = sender switch
        {
            _ when ReferenceEquals(sender, QualityBalanced) => "Balanced",
            _ when ReferenceEquals(sender, QualityHigh) => "Quality",
            _ => "Fast",
        };
        if (preset == _settings.QualityPreset) return;
        _settings.QualityPreset = preset;
        CheckDirty();
    }

    private void InitOffsetDisplays()
    {
        SysOffsetValue.Text = FormatOffset(_settings.SystemAudioOffsetMs);
        MicOffsetValue.Text = FormatOffset(_settings.MicOffsetMs);
    }

    private static string FormatOffset(int ms) => (ms >= 0 ? "+" : "") + ms + " ms";

    private void SysOffsetMinus_Click(object sender, RoutedEventArgs e) => AdjustSysOffset(-OffsetStepMs);
    private void SysOffsetPlus_Click(object sender, RoutedEventArgs e) => AdjustSysOffset(OffsetStepMs);
    private void MicOffsetMinus_Click(object sender, RoutedEventArgs e) => AdjustMicOffset(-OffsetStepMs);
    private void MicOffsetPlus_Click(object sender, RoutedEventArgs e) => AdjustMicOffset(OffsetStepMs);

    private void AdjustSysOffset(int deltaMs)
    {
        _settings.SystemAudioOffsetMs = Math.Clamp(_settings.SystemAudioOffsetMs + deltaMs, OffsetMinMs, OffsetMaxMs);
        SysOffsetValue.Text = FormatOffset(_settings.SystemAudioOffsetMs);
        CheckDirty();
    }

    private void AdjustMicOffset(int deltaMs)
    {
        _settings.MicOffsetMs = Math.Clamp(_settings.MicOffsetMs + deltaMs, OffsetMinMs, OffsetMaxMs);
        MicOffsetValue.Text = FormatOffset(_settings.MicOffsetMs);
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
        HotkeyCaptureButton.Content = "PRESS A KEY…";
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
        HotkeyCaptureButton.Content = HotkeyDisplay.Format(_settings.HotkeyModifiers, _settings.HotkeyVk);

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
            HotkeyCaptureButton.Content = HotkeyDisplay.Format(_pendingHotkeyModifier, 0);
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
            HotkeyCaptureButton.Content = "PRESS A KEY…";
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
            HotkeyCaptureButton.Content = "ALREADY IN USE";

            var revertTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(1.5) };
            revertTimer.Tick += (_, _) => { revertTimer.Stop(); UpdateHotkeyButtonDisplay(); };
            revertTimer.Start();
            return;
        }

        _settings.HotkeyModifiers = modifiers;
        _settings.HotkeyVk = vk;
        _settings.Save();
        UpdateSessionInfoDisplay();
        EndHotkeyCapture();
    }

    private void StartupToggle_Changed(object sender, RoutedEventArgs e)
    {
        if (_initializing) return;
        bool enabled = StartupToggle.IsOn;
        StartupManager.SetEnabled(enabled);
    }

    private void DebugToggle_Changed(object sender, RoutedEventArgs e)
    {
        if (_initializing) return;
        bool enabled = DebugToggle.IsOn;
        _settings.DebugLogging = enabled;
        Logger.SetDebugMode(enabled);
        _settings.Save();
    }

    private void TransparencyToggle_Changed(object sender, RoutedEventArgs e)
    {
        if (_initializing) return;
        if (!TransparencyToggle.IsEnabled) return;
        bool enabled = TransparencyToggle.IsOn;
        _settings.WindowTransparency = enabled;
        _settings.Save();
        _dq.TryEnqueue(() => ApplyBackdrop(enabled));
    }

    private void ThemeToggle_Changed(object sender, RoutedEventArgs e)
    {
        if (_initializing) return;
        bool dark = ThemeToggle.IsOn;
        _settings.DarkMode = dark;
        _settings.Save();
        _dq.TryEnqueue(() =>
        {
            ApplyAppTheme(dark);
            ApplyBackdrop(_settings.WindowTransparency);
        });
    }

    private void FFmpegPathBox_TextChanged(object sender, TextChangedEventArgs e)
    {
        if (_initializing) return;
        if (FFmpegPathBox.Text == _settings.FFmpegPath) return;
        _settings.FFmpegPath = FFmpegPathBox.Text;
        CheckDirty();
    }

    private async void BrowseFFmpegButton_Click(object sender, RoutedEventArgs e)
    {
        var picker = new Windows.Storage.Pickers.FileOpenPicker();
        InitializeWithWindow.Initialize(picker, _hwnd);
        picker.FileTypeFilter.Add(".exe");
        picker.SuggestedStartLocation = Windows.Storage.Pickers.PickerLocationId.ComputerFolder;

        var file = await picker.PickSingleFileAsync();
        if (file != null) FFmpegPathBox.Text = file.Path;
    }

    private void SaveLocationBox_TextChanged(object sender, TextChangedEventArgs e)
    {
        if (_initializing) return;
        if (SaveLocationBox.Text == _settings.SaveDestination) return;
        _settings.SaveDestination = SaveLocationBox.Text;
        CheckDirty();
    }

    private async void BrowseSaveLocationButton_Click(object sender, RoutedEventArgs e)
    {
        var picker = new Windows.Storage.Pickers.FolderPicker();
        InitializeWithWindow.Initialize(picker, _hwnd);
        picker.FileTypeFilter.Add("*");
        picker.SuggestedStartLocation = Windows.Storage.Pickers.PickerLocationId.ComputerFolder;

        var folder = await picker.PickSingleFolderAsync();
        if (folder != null) SaveLocationBox.Text = folder.Path;
    }

    private void CopyAddressButton_Click(object sender, RoutedEventArgs e)
    {
        try
        {
            var package = new DataPackage();
            package.SetText(WalletAddressText.Text);
            Clipboard.SetContent(package);
            CopyAddressButton.Content = "COPIED!";
            var revertTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(1.5) };
            revertTimer.Tick += (_, _) => { revertTimer.Stop(); CopyAddressButton.Content = "COPY ADDRESS"; };
            revertTimer.Start();
        }
        catch (Exception ex)
        {
            Logger.Warn($"Could not copy wallet address to clipboard: {ex.Message}");
        }
    }

    private void CheckDirty()
    {
        _dirty = SnapshotJson() != _settingsSnapshot;
        UpdateApplyButtonState();
    }

    private void UpdateApplyButtonState()
    {
        ApplyButton.IsEnabled = _dirty;
        ApplyButton.Style = (Style)Application.Current.Resources[_dirty ? "AccentButtonStyle" : "BaseButtonStyle"];
        ApplyButton.Content = _dirty ? "APPLY CHANGES" : "NO CHANGES PENDING";
    }

    private async void ApplyButton_Click(object sender, RoutedEventArgs e) => await DoApplyAsync();

    private async Task DoApplyAsync()
    {
        ApplyButton.IsEnabled = false;
        ApplyButton.Content = "APPLYING…";

        _settings.Save();

        bool ok = true;
        if (_engine.IsRunning) ok = await Task.Run(() => _engine.Restart(_tempDir));
        if (!ok) Logger.Warn("Apply: capture failed to restart with the new settings (see log).");

        SetRunningVisual(_engine.IsRunning);
        UpdateSessionInfoDisplay();

        _dirty = false;
        UpdateApplyButtonState();
        _settingsSnapshot = SnapshotJson();
    }

    private async Task ExitApplicationAsync()
    {
        if (_dirty)
        {
            var dialog = new ContentDialog
            {
                XamlRoot = RootGrid.XamlRoot,
                Title = "Instant Replay",
                Content = "You have unapplied settings changes. Apply them before exiting?",
                PrimaryButtonText = "Apply & Exit",
                SecondaryButtonText = "Exit Without Applying",
                CloseButtonText = "Cancel",
                DefaultButton = ContentDialogButton.Primary,
            };
            var result = await dialog.ShowAsync();
            if (result == ContentDialogResult.None) return;

            if (result == ContentDialogResult.Primary)
            {
                _settings.Save();
                if (_engine.IsRunning) _engine.Restart(_tempDir);
            }
        }

        _isClosingForReal = true;

        UnregisterHotKey(_hwnd, HotkeyId);

        _debugStatsTimer?.Stop();
        _bufferHealthTimer?.Stop();
        _micRefreshTimer?.Stop();
        _deviceChangeDebounceTimer?.Stop();
        _audioWatcher?.Dispose();
        Microsoft.Win32.SystemEvents.UserPreferenceChanged -= SystemEvents_UserPreferenceChanged;
        _leakReminderTimer?.Stop();
        _recDotPulseTimer?.Stop();
        _hotkeyPulseTimer?.Stop();
        _engine.Stop();
        _engine.Dispose();

        _trayIcon?.Dispose();

        Close();
        Environment.Exit(0);
    }
}
