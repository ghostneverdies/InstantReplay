using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;
using System.Windows.Input;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Media.Imaging;
using System.Windows.Threading;
using Microsoft.Web.WebView2.Core;
using InstantReplay.Core;
using InstantReplay.GUI.Helpers;
using InstantReplay.GUI.Shell;
using InstantReplay.Models;
using InstantReplay.Services;
using Brush = System.Windows.Media.Brush;
using Color = System.Windows.Media.Color;
using FontIcon = Wpf.Ui.Controls.FontIcon;
using InfoBarSeverity = Wpf.Ui.Controls.InfoBarSeverity;
using ToggleSwitch = Wpf.Ui.Controls.ToggleSwitch;
using WebView2 = Microsoft.Web.WebView2.Wpf.WebView2;

namespace InstantReplay.GUI;

public partial class MainWindow : Wpf.Ui.Controls.FluentWindow
{
    [DllImport("user32.dll")] private static extern bool RegisterHotKey(IntPtr hWnd, int id, uint fsModifiers, uint vk);
    [DllImport("user32.dll")] private static extern bool UnregisterHotKey(IntPtr hWnd, int id);
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

    private const int SwHide = 0;
    private const int SwShow = 5;
    private const int SwRestore = 9;
    private const int WM_HOTKEY = 0x0312;
    private const int WM_INPUT = 0x00FF;
    private const uint WM_APP = 0x8000;

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool RegisterRawInputDevices(RawInputDevice[] devices, uint count, uint size);
    [DllImport("user32.dll")]
    private static extern uint GetRawInputData(IntPtr rawInput, uint command, IntPtr data, ref uint size, uint headerSize);
    [DllImport("user32.dll")]
    private static extern short GetAsyncKeyState(int vk);

    [StructLayout(LayoutKind.Sequential)]
    private struct RawInputDevice
    {
        public ushort UsagePage;
        public ushort Usage;
        public uint Flags;
        public IntPtr Target;
    }

    private uint _rawModifiers;
    private bool _rawHotkeyDown;
    private long _lastHotkeyTick;

    private void RegisterRawKeyboard(bool enable)
    {
        var device = new RawInputDevice
        {
            UsagePage = 1,
            Usage = 6,
            Flags = enable ? 0x00000100u : 0x00000001u ,
            Target = enable ? _hwnd : IntPtr.Zero,
        };
        if (!RegisterRawInputDevices(new[] { device }, 1, (uint)Marshal.SizeOf<RawInputDevice>()))
        {
            if (enable) Logger.Warn($"Raw keyboard input for the hotkey could not be registered (error {Marshal.GetLastWin32Error()}).");
            return;
        }
        if (!enable) return;
        _rawModifiers = 0;
        if ((GetAsyncKeyState(0x12) & 0x8000) != 0) _rawModifiers |= 1;
        if ((GetAsyncKeyState(0x11) & 0x8000) != 0) _rawModifiers |= 2;
        if ((GetAsyncKeyState(0x10) & 0x8000) != 0) _rawModifiers |= 4;
        if ((GetAsyncKeyState(0x5B) & 0x8000) != 0 || (GetAsyncKeyState(0x5C) & 0x8000) != 0) _rawModifiers |= 8;
    }

    private void HandleRawKeyboard(IntPtr rawInput)
    {
        uint header = (uint)(IntPtr.Size == 8 ? 24 : 16);
        uint size = 0;
        GetRawInputData(rawInput, 0x10000003 , IntPtr.Zero, ref size, header);
        if (size == 0 || size > 512) return;
        IntPtr buffer = Marshal.AllocHGlobal((int)size);
        try
        {
            if (GetRawInputData(rawInput, 0x10000003, buffer, ref size, header) != size) return;
            if (Marshal.ReadInt32(buffer, 0) != 1 ) return;
            ushort flags = (ushort)Marshal.ReadInt16(buffer, (int)header + 2);
            ushort vk = (ushort)Marshal.ReadInt16(buffer, (int)header + 6);
            ProcessRawKey(vk, (flags & 1) != 0);
        }
        finally { Marshal.FreeHGlobal(buffer); }
    }

    private void ProcessRawKey(ushort vk, bool released)
    {
        uint modifier = vk switch
        {
            0x12 or 0xA4 or 0xA5 => 1u,
            0x11 or 0xA2 or 0xA3 => 2u,
            0x10 or 0xA0 or 0xA1 => 4u,
            0x5B or 0x5C => 8u,
            _ => 0u,
        };
        if (modifier != 0)
        {
            if (released) _rawModifiers &= ~modifier; else _rawModifiers |= modifier;
            return;
        }
        if (vk != _settings.HotkeyVk) return;
        if (released) { _rawHotkeyDown = false; return; }
        if (_rawHotkeyDown) return;
        _rawHotkeyDown = true;
        if (_capturingHotkey || _rawModifiers != (_settings.HotkeyModifiers & 0xF)) return;
        TriggerHotkeySave();
    }

    private void TriggerHotkeySave()
    {
        long now = Compat.TickCount64;
        if (now - _lastHotkeyTick < 500) return;
        _lastHotkeyTick = now;
        _ = SaveReplayAsync();
    }

    internal const int HotkeyId = 0x1000;
    internal const uint ModAlt = 0x0001;
    internal const uint ModControl = 0x0002;
    internal const uint ModShift = 0x0004;
    internal const uint ModWin = 0x0008;

    private static readonly TimeSpan SaveCooldown = TimeSpan.FromSeconds(10);
    private DateTime _lastSaveAcceptedUtc = DateTime.MinValue;

    private const long LeakGrowthMb = 1024;
    private const int LeakConsecutiveSamplesRequired = 6;
    private static readonly TimeSpan LeakReminderInterval = TimeSpan.FromMinutes(10);

    private readonly Settings _settings;
    private readonly RecorderEngine _engine;
    private readonly string _tempDir;
    private readonly IntPtr _hwnd;
    private readonly UiQueue _dq;

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
    private int _memoryGeneration;
    private DateTime _memoryObservationStartedUtc = DateTime.MinValue;
    private long _memoryBaselineMb;
    private long _lastMemorySampleMb;
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
    private WebView2? _inlinePlayer;
    private double _playerTime;
    private bool _playerPlaying = true;
    private double _playerVolume = 1;
    private bool _playerMuted;
    private bool _switchingPlayer;
    private int _viewerGeneration;
    private double _playerAspect = 16.0 / 9.0;

    private ScrollViewer? _galleryScroll;
    private bool _galleryDirty;
    private string _galleryDir = "";
    private DispatcherTimer? _prewarmTimer;
    private readonly Queue<FrameworkElement> _prewarmPages = new();
    private FrameworkElement? _prewarmedPage;

    private SolidColorBrush _accentPlateBrush = new(Color.FromArgb(0x30, 0x8B, 0x5F, 0xBF));
    private SolidColorBrush _normalBarBrush = new(Color.FromArgb(0xFF, 0x60, 0xA5, 0xFA));
    private const int SparkSamples = 40;
    private const double SparkAlertPct = 90;
    private readonly List<double> _cpuHistory = new();
    private readonly List<double> _ramHistory = new();
    private bool _animationsEnabled = true;

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


    private TaskbarBadge? _taskbarBadge;
    private BadgeKind _badgeKind = BadgeKind.None;
    private IntPtr _badgeIcon;
    private IntPtr _trayBadgeIcon;
    private bool _hasErrored;
    private bool _flyoutShown;

    public MainWindow(bool startMinimized = false)
    {
        _startMinimized = startMinimized;
        InitializeComponent();
        _hwnd = new WindowInteropHelper(this).EnsureHandle();
        SelectNav(NavHomeItem);

        StatusInfoActionButton.Click += (_, _) =>
        {
            Action? action = _infoBarAction;
            _infoBarAction = null;
            action?.Invoke();
        };

        Title = "Instant Replay";
        _dq = new UiQueue(Dispatcher);
        _library = new ReplayLibrary(Dispatcher);

        _animationsEnabled = SystemParameters.ClientAreaAnimation;

        if (_startMinimized)
        {
            ShowActivated = false;
            WindowState = WindowState.Minimized;
        }

        SetInitialWindowSizeAndIcon();
        HookWndProc();

        _settings = Settings.Load();
        _settings.DarkMode = WindowsTheme.IsSystemDarkMode();
        Logger.Init(_settings.DebugLogging);
        UpdateWebView2Requirement();
        _baseline = CapturePending();

        _tempDir = Path.Combine(Path.GetTempPath(), "instant replay");
        _engine = new RecorderEngine(_settings);

        _currentPage = HomePage;

        RootGrid.Loaded += (_, _) => InitAudioToggles();

        RefreshMicrophonesInBackground();
        InitDurationSegments();
        InitFpsSegments();
        InitQualitySegments();
        InitEncoderSettings();
        InitAudioSettings();
        InitCaptureTargets();
        DebugLoggingToggle.IsChecked = _settings.DebugLogging;
        UpdateHotkeyButtonDisplay();

        SetNavPaneOpen(_settings.NavigationPaneOpen, persist: false);

        StartupToggle.IsChecked = StartupManager.IsEnabled();
        SaveLocationBox.Text = _settings.GetEffectiveSaveDestination();

        _initializing = false;
        _baseline = CapturePending();

        GalleryGrid.ItemsSource = _library.Items;
        _library.Items.CollectionChanged += (_, _) => UpdateGalleryChrome();
        _library.FolderChanged += () =>
        {
            _galleryDirty = true;
            if (ReferenceEquals(_currentPage, GalleryPage)) _ = RefreshGalleryAsync();
        };
        UpdateGalleryChrome();

        _galleryDir = _settings.GetEffectiveSaveDestination();
        _ = _library.WarmUpAsync(_galleryDir);

        _ = Task.Run(GameDetector.Preload);

        Closed += (_, _) =>
        {
            if (_fsWindow != null) { var fs = _fsWindow; _fsWindow = null; try { fs.Shutdown(); } catch { } }
        };

        _dq.TryEnqueue(() =>
        {
            FitWindowToPage(HomePage);
            PlayPageEntrance(HomePage);
            StartPagePrewarm();
        });

        if (!RegisterHotKey(_hwnd, HotkeyId, _settings.HotkeyModifiers, _settings.HotkeyVk))
            Logger.Warn($"Could not register global hotkey {HotkeyDisplay.Format(_settings.HotkeyModifiers, _settings.HotkeyVk)} (may already be in use).");
        RegisterRawKeyboard(true);

        ApplyAppTheme(_settings.DarkMode);
        ApplyBackdrop();
        SetupSettingsGear();
        SetupTrayIcon();
        RefreshAppBadge();
        InitializeUpdates();

        _debugStatsTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(30) };
        _debugStatsTimer.Tick += (_, _) => { LogDebugStats(); CheckForMemoryLeak(); };
        _debugStatsTimer.Start();

        _bufferHealthTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(500) };
        _bufferHealthTimer.Tick += (_, _) => CheckBufferHealth();
        _bufferHealthTimer.Start();

        _statsTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(1) };
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

        _dq.TryEnqueue(ClipNotificationWindow.WarmUp);

        Microsoft.Win32.SystemEvents.UserPreferenceChanged += SystemEvents_UserPreferenceChanged;

        if (_startMinimized)
        {
            _dq.TryEnqueue(() =>
            {
                Hide();
                _isHidden = true;
                NotifyRunningInBackground();
            });
        }
    }

    private void SetInitialWindowSizeAndIcon()
    {
        ApplyFixedWindowSize();
        try
        {
            string iconPath = ResolveIconPath();
            if (File.Exists(iconPath)) Icon = BitmapFrame.Create(new Uri(iconPath, UriKind.Absolute));
        }
        catch (Exception ex) { Logger.Warn($"Could not set window icon: {ex.Message}"); }
    }

    private void ApplyFixedWindowSize()
    {
        Width = 760;
        Height = 480;
    }

    private void DonateTitleButton_Click(object sender, RoutedEventArgs e)
    {
        DonateWindow.Show(_hwnd, _settings);
    }

    private static void OpenUrl(string url)
    {
        Process.Start(new ProcessStartInfo(url) { UseShellExecute = true });
    }

    private void UpdateWebView2Requirement()
    {
        bool installed;
        try
        {
            string version = CoreWebView2Environment.GetAvailableBrowserVersionString();
            installed = !string.IsNullOrWhiteSpace(version);
            if (installed) Logger.Info($"WebView2 Runtime available: {version}");
        }
        catch (Exception ex)
        {
            installed = false;
            Logger.Warn($"WebView2 Runtime unavailable: {ex.Message}");
        }
        WebViewRequirementBar.IsOpen = !installed;
        WebViewRequirementBar.Visibility = installed ? Visibility.Collapsed : Visibility.Visible;
    }

    private void DownloadWebView2Button_Click(object sender, RoutedEventArgs e)
    {
        try
        {
            OpenUrl("https://developer.microsoft.com/en-us/microsoft-edge/webview2/");
        }
        catch (Exception ex)
        {
            Logger.Warn($"Could not open the WebView2 download page: {ex.Message}");
            ShowInfoBar(InfoBarSeverity.Error, "Couldn't open download page", "Open Microsoft's WebView2 page in your browser to install the Runtime.");
        }
    }

    private void GitHubTitleButton_Click(object sender, RoutedEventArgs e)
    {
        try
        {
            OpenUrl("https://github.com/ghostneverdies/InstantReplay");
        }
        catch (Exception ex)
        {
            Logger.Warn($"Could not open GitHub page: {ex.Message}");
        }
    }

    private void HookWndProc()
    {
        HwndSource.FromHwnd(_hwnd)?.AddHook(WndProc);
    }

    private IntPtr WndProc(IntPtr hWnd, int message, IntPtr wParam, IntPtr lParam, ref bool handled)
    {
        uint msg = (uint)message;
        if (msg == WM_HOTKEY && wParam.ToInt32() == HotkeyId)
        {
            TriggerHotkeySave();
            handled = true;
            return IntPtr.Zero;
        }
        if (msg == WM_INPUT)
        {
            try { HandleRawKeyboard(lParam); }
            catch (Exception ex) { Logger.Debug($"Raw keyboard input failed: {ex.Message}"); }
        }
        if (msg == s_taskbarButtonCreated && s_taskbarButtonCreated != 0)
        {
            _taskbarBadge?.Reset();
            _dq.TryEnqueue(() => RefreshAppBadge());
        }
        if (msg == WM_APP + 1 && _trayIcon is not null)
        {
            _trayIcon.HandleMessage(lParam);
            handled = true;
        }
        return IntPtr.Zero;
    }

    private void ApplyBackdrop()
    {
        // The Mica backdrop and transparent window background come from FluentWindow / ApplicationThemeManager.
        try
        {
            TitleBarButtons.SetCornerPreference(_hwnd, round: true);
            TitleBarButtons.SetAccentBorder(_hwnd);
        }
        catch (Exception ex)
        {
            Logger.Warn($"Could not set corner preference: {ex.Message}");
        }
    }

    private void ApplyAppTheme(bool dark)
    {
        AppTheme.Apply(dark);
        ApplyHomeAccent();
    }

    private void SystemEvents_UserPreferenceChanged(object? sender, Microsoft.Win32.UserPreferenceChangedEventArgs e)
    {
        if (e.Category is not (Microsoft.Win32.UserPreferenceCategory.General or Microsoft.Win32.UserPreferenceCategory.Color))
            return;

        _dq.TryEnqueue(() =>
        {
            _animationsEnabled = SystemParameters.ClientAreaAnimation;

            if (e.Category == Microsoft.Win32.UserPreferenceCategory.General)
            {
                bool dark = WindowsTheme.IsSystemDarkMode();
                if (dark != _settings.DarkMode)
                {
                    _settings.DarkMode = dark;
                    ApplyAppTheme(dark);
                    ApplyBackdrop();
                    return;
                }
            }
            AppTheme.Apply(_settings.DarkMode);
            ApplyHomeAccent();
        });
    }

    private Brush GetThemedBrush(string key)
    {
        return (Brush)Application.Current.Resources[key];
    }

    private readonly record struct PendingSettings(
        int ReplayDurationSeconds,
        int FrameRate,
        string QualityPreset,
        string VideoCodec,
        bool GpuEncoder,
        string GpuPreference,
        string BufferMethod,
        string CaptureTarget,
        string CaptureMethod,
        int CaptureMonitorIndex,
        bool CaptureClientArea,
        bool CaptureCursor,
        bool CaptureBorder,
        string Microphone,
        string SaveDestination,
        bool RecordSystemAudio,
        bool RecordMicrophone,
        string AudioCodec,
        int AudioBitrate,
        bool ApplicationLocalAudio,
        bool DebugLogging);

    private PendingSettings CapturePending() => new(
        _settings.ReplayDurationSeconds,
        _settings.FrameRate,
        _settings.QualityPreset,
        _settings.VideoCodec,
        _settings.GpuEncoder,
        _settings.GpuPreference,
        _settings.BufferMethod,
        _settings.CaptureTarget,
        _settings.CaptureMethod,
        _settings.CaptureMonitorIndex,
        _settings.CaptureClientArea,
        _settings.CaptureCursor,
        _settings.CaptureBorder,
        _settings.Microphone,
        _settings.SaveDestination,
        _settings.RecordSystemAudio,
        _settings.RecordMicrophone,
        _settings.AudioCodec,
        _settings.AudioBitrate,
        _settings.ApplicationLocalAudio,
        _settings.DebugLogging);

    private static void PlayButtonSound() { }

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
        WindowState = WindowState.Minimized;

        var hideTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(220) };
        hideTimer.Tick += (_, _) =>
        {
            hideTimer.Stop();
            Hide();
            _isHidden = true;
            _taskbarBadge?.SetTaskbarVisible(false);
            _lastBalloonWasLeakWarning = false;

            NotifyRunningInBackground();
        };
        hideTimer.Start();
    }

    private void Window_Closing(object? sender, System.ComponentModel.CancelEventArgs e)
    {
        if (_isClosingForReal) return;
        e.Cancel = true;

        if (_dirty) RevertPendingChanges();

        CloseButton_Click();
    }

    private void Window_Activated(object? sender, EventArgs e)
    {
        RefreshStartupToggleState();
        _dq.TryEnqueue(async () => await ShowUpdateReminderAsync());
        _taskbarBadge?.StopFlashing();
        if (ReferenceEquals(_currentPage, GalleryPage)) _ = RefreshGalleryAsync();
    }

    private void Window_Deactivated(object? sender, EventArgs e)
    {
        if (_capturingHotkey)
            EndHotkeyCapture();
    }

    // ---- Sidebar (RadioButton items + collapsible pane; WPF-UI's NavigationView only navigates to Page types) ----

    private const double NavPaneOpenWidth = 220;
    private const double NavPaneCompactWidth = 48;
    private bool _navPaneOpen = true;

    private IEnumerable<RadioButton> NavItems()
    {
        yield return NavHomeItem;
        yield return NavGalleryItem;
        yield return NavGamesItem;
        yield return NavSettingsItem;
        yield return NavUpdatesItem;
    }

    private void SelectNav(RadioButton item)
    {
        foreach (RadioButton nav in NavItems()) nav.IsChecked = ReferenceEquals(nav, item);
    }

    private RadioButton? SelectedNav() => NavItems().FirstOrDefault(n => n.IsChecked == true);

    private void SetNavPaneOpen(bool open, bool persist)
    {
        _navPaneOpen = open;
        NavPane.Width = open ? NavPaneOpenWidth : NavPaneCompactWidth;
        foreach (RadioButton nav in NavItems())
        {
            if (nav.Content is StackPanel row && row.Children.Count > 1)
                row.Children[1].Visibility = open ? Visibility.Visible : Visibility.Collapsed;
        }
        UpdateUpdatesBadgeVisibility();
        if (!persist || _initializing) return;
        _settings.NavigationPaneOpen = open;
        _settings.SaveFields(nameof(Settings.NavigationPaneOpen));
    }

    private void NavPaneToggle_Click(object sender, RoutedEventArgs e) => SetNavPaneOpen(!_navPaneOpen, persist: true);

    private async void NavItem_Click(object sender, RoutedEventArgs e)
    {
        if (_initializing) return;
        var item = (RadioButton)sender;
        FrameworkElement target =
            ReferenceEquals(item, NavUpdatesItem) ? UpdatesPage
            : ReferenceEquals(item, NavGamesItem) ? GamesPage
            : ReferenceEquals(item, NavSettingsItem) ? SettingsPage
            : ReferenceEquals(item, NavGalleryItem) ? GalleryPage
            : HomePage;

        // Clicking Gallery while a clip is open returns to the gallery (the viewer lives under Gallery).
        if (ReferenceEquals(item, NavGalleryItem) && ReferenceEquals(_currentPage, ViewerPage))
        {
            CloseViewer();
            return;
        }

        if (ReferenceEquals(target, GamesPage) && _dirty) await SaveChangesAsync();
        if (ReferenceEquals(target, GamesPage)) RefreshGamesPage();
        if (ReferenceEquals(target, UpdatesPage)) { HideUpdatesBadge(); RefreshUpdatesPage(); }
        NavigateTo(target);
    }

    private void HideUpdatesBadge()
    {
        _updatesBadgeVisible = false;
        UpdateUpdatesBadgeVisibility();
    }

    private bool _updatesBadgeVisible;

    private void UpdateUpdatesBadgeVisibility()
    {
        NavUpdatesItemBadge.Visibility = _updatesBadgeVisible && _navPaneOpen ? Visibility.Visible : Visibility.Collapsed;
    }

    private int PageOrder(FrameworkElement page) =>
        ReferenceEquals(page, UpdatesPage) ? 4
        : ReferenceEquals(page, SettingsPage) ? 3
        : ReferenceEquals(page, GamesPage) ? 2
        : ReferenceEquals(page, GalleryPage) || ReferenceEquals(page, ViewerPage) ? 1
        : 0;

    private void ResetPageScroll(FrameworkElement page)
    {
        if (page is ScrollViewer scroll)
        {
            scroll.ScrollToVerticalOffset(0);
            scroll.ScrollToHorizontalOffset(0);
            return;
        }

        if (ReferenceEquals(page, GalleryPage))
        {
            _galleryScroll ??= FindDescendant<ScrollViewer>(GalleryGrid);
            _galleryScroll?.ScrollToVerticalOffset(0);
        }
    }

    private static T? FindDescendant<T>(DependencyObject root) where T : DependencyObject
    {
        if (!(root is Visual) && !(root is System.Windows.Media.Media3D.Visual3D)) return null;
        int count = VisualTreeHelper.GetChildrenCount(root);
        for (int i = 0; i < count; i++)
        {
            DependencyObject child = VisualTreeHelper.GetChild(root, i);
            if (child is T match) return match;
            if (FindDescendant<T>(child) is { } nested) return nested;
        }
        return null;
    }

    private void StartPagePrewarm()
    {
        if (_prewarmTimer != null) return;

        foreach (FrameworkElement page in new FrameworkElement[] { GalleryPage, SettingsPage, GamesPage, UpdatesPage })
            _prewarmPages.Enqueue(page);

        _prewarmTimer = new DispatcherTimer(DispatcherPriority.Normal, Dispatcher) { Interval = TimeSpan.FromMilliseconds(350) };
        _prewarmTimer.Tick += (_, _) => PrewarmNextPage();
        _prewarmTimer.Start();
    }

    private void PrewarmNextPage()
    {
        if (_prewarmedPage != null)
        {
            if (!ReferenceEquals(_currentPage, _prewarmedPage))
            {
                _prewarmedPage.Visibility = Visibility.Collapsed;
                _prewarmedPage.Opacity = 1;
                _prewarmedPage.IsHitTestVisible = true;
            }
            _prewarmedPage = null;
        }

        if (_prewarmPages.Count == 0)
        {
            _prewarmTimer?.Stop();
            return;
        }

        FrameworkElement next = _prewarmPages.Dequeue();
        if (ReferenceEquals(next, _currentPage) || _isHidden) return;

        next.IsHitTestVisible = false;
        next.Opacity = 0;
        next.Visibility = Visibility.Visible;
        _prewarmedPage = next;
    }

    private void RefreshGalleryIfStale()
    {
        string dir = _settings.GetEffectiveSaveDestination();
        if (_galleryDirty || !string.Equals(dir, _galleryDir, StringComparison.OrdinalIgnoreCase))
            _ = RefreshGalleryAsync();
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

        Canvas.SetZIndex(newPage, 1);
        if (oldPage != null) Canvas.SetZIndex(oldPage, 0);

        newPage.IsHitTestVisible = true;
        newPage.Visibility = Visibility.Visible;
        _dq.TryEnqueue(() => { if (ReferenceEquals(_currentPage, newPage)) ResetPageScroll(newPage); });
        newPage.Opacity = 0;
        AnimateDouble(newPage, "Opacity", 0, 1, TimeSpan.FromMilliseconds(240), TimeSpan.Zero,
            new CubicEase { EasingMode = EasingMode.EaseOut });

        if (!fadeOnly) PlayPageEntrance(newPage);
        if (ReferenceEquals(newPage, GalleryPage)) RefreshGalleryIfStale();

        if (ReferenceEquals(newPage, ViewerPage)) FitViewerWindowToAspect();
        else FitWindowToPage(newPage);

        TransitionOut(oldPage);
    }

    private Rect GetWorkAreaDips()
    {
        System.Drawing.Rectangle wa = System.Windows.Forms.Screen.FromHandle(_hwnd).WorkingArea;
        double scale = VisualTreeHelper.GetDpi(this).DpiScaleX;
        if (scale <= 0) scale = 1.0;
        return new Rect(wa.X / scale, wa.Y / scale, wa.Width / scale, wa.Height / scale);
    }

    private void CenterOnWorkArea(Rect work)
    {
        if (_windowPlaced) return;
        _windowPlaced = true;
        Left = work.X + Math.Max(0, (work.Width - Width) / 2);
        Top = work.Y + Math.Max(0, (work.Height - Height) / 2);
    }

    private void FitWindowToPage(FrameworkElement page)
    {
        _dq.TryEnqueue(() =>
        {
            try
            {
                Rect work = GetWorkAreaDips();
                double w = Math.Min(DefaultWindowWidthDips, work.Width * 0.94);
                double h = Math.Min(DefaultWindowHeightDips, work.Height * 0.94);

                if (Math.Abs(Width - w) > 0.5 || Math.Abs(Height - h) > 0.5)
                {
                    Width = w;
                    Height = h;
                }

                CenterOnWorkArea(work);
            }
            catch (Exception ex)
            {
                Logger.Warn($"FitWindowToPage failed: {ex.Message}");
            }
        });
    }

    private void FitViewerWindowToAspect()
    {
        _dq.TryEnqueue(() =>
        {
            try
            {
                Rect work = GetWorkAreaDips();
                double maxClientW = Math.Min(DefaultWindowWidthDips, work.Width * 0.94);
                double maxClientH = Math.Min(DefaultWindowHeightDips, work.Height * 0.94);

                double clientW = ActualWidth, clientH = ActualHeight;
                double chromeW = clientW - PlayerViewport.ActualWidth;
                double chromeH = clientH - PlayerViewport.ActualHeight;
                if (chromeW < 160 || chromeH < 80)
                {
                    // First viewer open (or an unexpected layout): use the measured
                    // navigation/title/back-button allowance instead of a bad delta.
                    chromeW = 260;
                    chromeH = 104;
                }

                double availW = Math.Max(320, maxClientW - chromeW);
                double availH = Math.Max(240, maxClientH - chromeH);
                double aspect = _playerAspect > 0 ? _playerAspect : 16.0 / 9.0;

                double viewportW = Math.Min(availW, availH * aspect);
                double viewportH = viewportW / aspect;
                if (viewportH > availH)
                {
                    viewportH = availH;
                    viewportW = viewportH * aspect;
                }

                double w = Math.Round(Math.Min(maxClientW, viewportW + chromeW));
                double h = Math.Round(Math.Min(maxClientH, viewportH + chromeH));
                w = Compat.Clamp(w, Math.Min(480, maxClientW), maxClientW);
                h = Compat.Clamp(h, Math.Min(360, maxClientH), maxClientH);

                if (Math.Abs(clientW - w) > 1 || Math.Abs(clientH - h) > 1)
                {
                    Width = w;
                    Height = h;
                }

                CenterOnWorkArea(work);
            }
            catch (Exception ex)
            {
                Logger.Warn($"FitViewerWindowToAspect failed: {ex.Message}");
            }
        });
    }

    private IEnumerable<FrameworkElement> EntranceTargets(FrameworkElement page)
    {
        if (ReferenceEquals(page, HomePage))
            return new FrameworkElement[] { HeroArt, HeroTitle, HeroSubtitle, HeroActions };
        if (ReferenceEquals(page, SettingsPage))
            return Array.Empty<FrameworkElement>();
        if (ReferenceEquals(page, GalleryPage))
            return new FrameworkElement[] { GalleryHeader };
        if (ReferenceEquals(page, UpdatesPage))
            return UpdatesContent.Children.OfType<FrameworkElement>().ToList();
        return Array.Empty<FrameworkElement>();
    }

    private void PlayPageEntrance(FrameworkElement page)
    {
        var ease = new CubicEase { EasingMode = EasingMode.EaseOut };
        int index = 0;
        foreach (FrameworkElement el in EntranceTargets(page))
        {
            if (!(el.RenderTransform is TranslateTransform t) || t.IsFrozen)
            {
                t = new TranslateTransform();
                el.RenderTransform = t;
            }

            t.Y = 28;
            el.Opacity = 0;

            var begin = TimeSpan.FromMilliseconds(index * 70);
            AnimateDouble(el, "Opacity", 0, 1, TimeSpan.FromMilliseconds(380), begin, ease);
            AnimateDouble(t, "TranslateY", 28, 0, TimeSpan.FromMilliseconds(460), begin, ease);
            index++;
        }
    }

    private sealed class NavIconTransforms
    {
        public readonly ScaleTransform Scale = new ScaleTransform(1, 1);
        public readonly RotateTransform Rotate = new RotateTransform(0);
        public readonly TranslateTransform Translate = new TranslateTransform();

        public void Reset()
        {
            foreach (DependencyProperty p in new[] { ScaleTransform.ScaleXProperty, ScaleTransform.ScaleYProperty })
                Scale.BeginAnimation(p, null);
            Rotate.BeginAnimation(RotateTransform.AngleProperty, null);
            Translate.BeginAnimation(TranslateTransform.YProperty, null);
            Scale.ScaleX = Scale.ScaleY = 1;
            Rotate.Angle = 0;
            Translate.Y = 0;
        }
    }

    private readonly Dictionary<FrameworkElement, NavIconTransforms> _navIcons = new Dictionary<FrameworkElement, NavIconTransforms>();

    private void SetupSettingsGear()
    {
        try
        {
            RegisterNavIcon(HomePage, NavHomeItemIcon);
            RegisterNavIcon(GalleryPage, NavGalleryItemIcon);
            RegisterNavIcon(SettingsPage, NavSettingsItemIcon);
        }
        catch (Exception ex)
        {
            Logger.Warn($"Navigation icon setup failed: {ex.Message}");
        }
    }

    private void RegisterNavIcon(FrameworkElement page, FrameworkElement icon)
    {
        var transforms = new NavIconTransforms();
        var group = new TransformGroup();
        group.Children.Add(transforms.Scale);
        group.Children.Add(transforms.Rotate);
        group.Children.Add(transforms.Translate);
        icon.RenderTransformOrigin = new Point(0.5, 0.5);
        icon.RenderTransform = group;
        _navIcons[page] = transforms;
    }

    private static void AddKeys(Animatable target, DependencyProperty property, params (int Ms, double Value)[] keys)
    {
        var anim = new DoubleAnimationUsingKeyFrames();
        foreach (var key in keys)
        {
            anim.KeyFrames.Add(new EasingDoubleKeyFrame
            {
                KeyTime = KeyTime.FromTimeSpan(TimeSpan.FromMilliseconds(key.Ms)),
                Value = key.Value,
                EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut },
            });
        }
        target.BeginAnimation(property, anim);
    }

    private void PlayNavIconAnimation(FrameworkElement page)
    {
        if (!_navIcons.TryGetValue(page, out NavIconTransforms? t)) return;

        try
        {
            foreach (NavIconTransforms other in _navIcons.Values) other.Reset();

            if (ReferenceEquals(page, SettingsPage))
            {
                AddKeys(t.Rotate, RotateTransform.AngleProperty, (0, 0), (400, 360));
            }
            else if (ReferenceEquals(page, HomePage))
            {
                AddKeys(t.Scale, ScaleTransform.ScaleXProperty, (0, 1), (170, 1.3), (340, 1));
                AddKeys(t.Scale, ScaleTransform.ScaleYProperty, (0, 1), (170, 1.3), (340, 1));
                AddKeys(t.Translate, TranslateTransform.YProperty, (0, 0), (170, -3), (340, 0));
            }
            else
            {
                AddKeys(t.Scale, ScaleTransform.ScaleXProperty, (0, 1), (150, 0.1), (320, 1));
                AddKeys(t.Rotate, RotateTransform.AngleProperty, (0, 0), (150, -8), (320, 0));
            }
        }
        catch (Exception ex)
        {
            Logger.Warn($"Navigation icon animation failed: {ex.Message}");
        }
    }


    private void ApplyHomeAccent()
    {
        if (HeroCard is null) return;

        Color accent = GetSystemAccent();
        bool dark = _settings.DarkMode;

        Color plate = dark ? Lighten(accent, 0.16f) : accent;
        _accentPlateBrush = new SolidColorBrush(Color.FromArgb(0x32, plate.R, plate.G, plate.B));
        CpuIconBg.Background = _accentPlateBrush;
        RamIconBg.Background = _accentPlateBrush;

        var iconBrush = new SolidColorBrush(dark ? Lighten(accent, 0.34f) : Darken(accent, 0.06f));
        CpuIconGlyph.Foreground = iconBrush;
        RamIconGlyph.Foreground = iconBrush;

        _normalBarBrush = new SolidColorBrush(dark ? Lighten(accent, 0.12f) : accent);

        SaveReplayButton.Background = new SolidColorBrush(accent);
        SaveReplayButton.Foreground = new SolidColorBrush(ContrastForeground(accent));
    }

    private static Color Darken(Color c, float amount) => Color.FromArgb(255,
        (byte)Compat.Clamp(c.R * (1f - amount), 0f, 255f),
        (byte)Compat.Clamp(c.G * (1f - amount), 0f, 255f),
        (byte)Compat.Clamp(c.B * (1f - amount), 0f, 255f));

    private static Color Lighten(Color c, float amount) => Color.FromArgb(255,
        (byte)Compat.Clamp(c.R + (255f - c.R) * amount, 0f, 255f),
        (byte)Compat.Clamp(c.G + (255f - c.G) * amount, 0f, 255f),
        (byte)Compat.Clamp(c.B + (255f - c.B) * amount, 0f, 255f));

    private static Color ContrastForeground(Color c)
    {
        double luminance = (0.2126 * c.R + 0.7152 * c.G + 0.0722 * c.B) / 255.0;
        return luminance > 0.55 ? Color.FromArgb(255, 16, 16, 16) : Colors.White;
    }

    private static Color GetSystemAccent()
    {
        try
        {
            return AppTheme.GetSystemAccent();
        }
        catch
        {
            var (r, g, b) = WindowsTheme.GetAccentColor();
            return Color.FromArgb(255, r, g, b);
        }
    }


    private void SetStartStopLabel(string text, string? glyph = null)
    {
        StartStopText.Text = text;
        if (glyph != null) StartStopIcon.Glyph = glyph;
    }

    private void ApplyStartStopLook(bool running)
    {
        SaveReplayButton.Visibility = Visibility.Visible;
        SaveReplayButton.IsEnabled = running;

        StartStopButton.ClearValue(Control.BackgroundProperty);
        StartStopButton.ClearValue(Control.ForegroundProperty);

        if (running)
        {
            StartStopButton.Style = (Style)Application.Current.Resources["HomeDangerButtonStyle"];
            SetStartStopLabel("Stop", "\uE71A");
        }
        else
        {
            StartStopButton.Style = (Style)Application.Current.Resources["HomeNeutralButtonStyle"];
            SetStartStopLabel("Start Replay", "\uE768");
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
            string dir = _settings.GetEffectiveSaveDestination();
            _galleryDirty = false;
            _galleryDir = dir;
            await _library.RefreshAsync(dir);
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

    private void EmptyGoHomeButton_Click(object sender, RoutedEventArgs e) { SelectNav(NavHomeItem); NavigateTo(HomePage); }

    private void GalleryGrid_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (GalleryGrid.SelectedItem is not ReplayItem item) return;
        GalleryGrid.SelectedItem = null;
        OpenViewer(item);
    }

    private void CardRoot_Loaded(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement card && card.DataContext is ReplayItem item)
            _library.Queue(item, visible: true);
    }

    private ReplayItem? _cardMenuTarget;
    private ContextMenu? _cardMenu;

    private void CardMoreButton_Click(object sender, RoutedEventArgs e)
    {
        if (sender is not FrameworkElement element ||
            (element.Tag as ReplayItem ?? element.DataContext as ReplayItem) is not ReplayItem item)
            return;

        _cardMenuTarget = item;
        if (_cardMenu == null)
        {
            var delete = new MenuItem
            {
                Header = "Delete",
                Icon = new FontIcon { Glyph = "\uE74D", FontFamily = (FontFamily)Application.Current.Resources["SymbolThemeFontFamily"] }
            };
            delete.Click += (_, _) =>
            {
                if (_cardMenuTarget is { } target) _ = DeleteReplayAsync(target);
            };
            _cardMenu = new ContextMenu();
            _cardMenu.Items.Add(delete);
        }

        _cardMenu.PlacementTarget = element;
        _cardMenu.Placement = PlacementMode.Bottom;
        _cardMenu.IsOpen = true;
    }

    private void PopulateViewer(ReplayItem item)
    {
        ViewerPoster.Source = item.Thumbnail;
        ViewerPoster.Visibility = Visibility.Visible;
    }

    private async void OpenViewer(ReplayItem item)
    {
        StopViewerPlayback();
        _viewerItem = item;
        _playerTime = 0;
        _playerPlaying = true;
        PopulateViewer(item);
        NavigateTo(ViewerPage);
        int generation = _viewerGeneration;
        if (!item.MetadataLoaded) await _library.EnsureAsync(item);
        if (generation != _viewerGeneration || !ReferenceEquals(_currentPage, ViewerPage)) return;
        _playerAspect = item.VideoWidth > 0 && item.VideoHeight > 0 ? (double)item.VideoWidth / item.VideoHeight : 16.0 / 9.0;
        UpdateInlinePlayerBounds();
        FitViewerWindowToAspect();
        await OpenInlinePlayerAsync(item, generation);
    }

    private async Task OpenInlinePlayerAsync(ReplayItem item, int generation)
    {
        WebView2? web = null;
        try
        {
            if (!File.Exists(item.VideoPath)) throw new FileNotFoundException("The replay was moved or deleted.");
            web = new WebView2 { DefaultBackgroundColor = System.Drawing.Color.Transparent };
            _inlinePlayer = web;
            InlinePlayerHost.Children.Add(web);
            await PlayerWebViewShared.WaitForLoadedAsync(web, TimeSpan.FromSeconds(6));
            await web.EnsureCoreWebView2Async();
            if (generation != _viewerGeneration || !ReferenceEquals(web, _inlinePlayer)) return;
            var core = web.CoreWebView2!;
            core.Settings.IsZoomControlEnabled = false;
            core.Settings.AreBrowserAcceleratorKeysEnabled = false;
            core.WebMessageReceived += (_, e) =>
            {
                if (ReferenceEquals(web, _inlinePlayer) && _fsWindow == null && !_switchingPlayer)
                    HandlePlayerMessage(PlayerWebViewShared.ToPlainJson(e.WebMessageAsJson ?? ""), false);
            };
            core.NavigationCompleted += (_, e) =>
            {
                if (!ReferenceEquals(web, _inlinePlayer)) return;
                if (e.IsSuccess) { ViewerPoster.Visibility = Visibility.Collapsed; SendAccentInline(); }
                else ShowInfoBar(InfoBarSeverity.Error, "Couldn't play replay", "The player could not load this replay.", "play");
            };
            core.Navigate(BuildPlayerUrl(item.VideoPath, _playerTime, false));
            web.Focus();
        }
        catch (Exception ex)
        {
            Logger.Error("Inline player failed", ex);
            if (generation == _viewerGeneration)
                ShowInfoBar(InfoBarSeverity.Error, "Couldn't play replay", "The replay or WebView2 player could not be opened.", "play");
        }
    }

    private void PlayerViewport_SizeChanged(object sender, SizeChangedEventArgs e) => UpdateInlinePlayerBounds();
    private void UpdateInlinePlayerBounds()
    {
        if (PlayerViewport == null || PlayerCard == null || InlinePlayerHost == null) return;
        double availableWidth = PlayerViewport.ActualWidth;
        double availableHeight = PlayerViewport.ActualHeight;
        if (availableWidth <= 0 || availableHeight <= 0 || _playerAspect <= 0) return;
        double width = Math.Min(availableWidth, availableHeight * _playerAspect);
        double height = width / _playerAspect;
        PlayerCard.Width = width;
        PlayerCard.Height = height;
    }

    private void SendAccentInline()
    {
        string? accent = GetAccentHex();
        if (accent != null) SendScriptInline($"receiveCommand({{\"c\":\"accent\",\"v\":\"{accent}\"}})");
    }
    private void SendScriptInline(string script)
    {
        try { if (_inlinePlayer?.CoreWebView2 is { } core) _ = core.ExecuteScriptAsync(script); }
        catch (Exception ex) { Logger.Warn($"Inline player command failed: {ex.Message}"); }
    }
    private void ReadPlayerState(JsonElement state)
    {
        if (state.TryGetProperty("width", out var width) && state.TryGetProperty("height", out var height)
            && width.TryGetInt32(out int w) && height.TryGetInt32(out int h) && w > 0 && h > 0)
        {
            double aspect = (double)w / h;
            if (Math.Abs(aspect - _playerAspect) > 0.001)
            {
                _playerAspect = aspect;
                UpdateInlinePlayerBounds();
                if (_fsWindow == null && ReferenceEquals(_currentPage, ViewerPage)) FitViewerWindowToAspect();
            }
        }
        if (state.TryGetProperty("t", out var t) && t.TryGetDouble(out double seconds) && IsFiniteNumber(seconds)) _playerTime = Math.Max(seconds, 0);
        if (state.TryGetProperty("playing", out var playing)) _playerPlaying = playing.ValueKind == JsonValueKind.True;
        if (state.TryGetProperty("v", out var volume) && volume.TryGetDouble(out double value) && IsFiniteNumber(value)) _playerVolume = Compat.Clamp(value, 0, 1);
        if (state.TryGetProperty("m", out var mute)) _playerMuted = mute.ValueKind == JsonValueKind.True;
    }

    private void ViewerBackButton_Click(object sender, RoutedEventArgs e)
    {
        CloseViewer();
    }

    private void CloseViewer()
    {
        StopViewerPlayback();
        NavigateTo(GalleryPage, fadeOnly: true);
    }

    private void StopViewerPlayback()
    {
        _viewerGeneration++;
        CloseFullscreenPlayer(resumeInline: false);
        var web = _inlinePlayer;
        _inlinePlayer = null;
        if (web != null) { InlinePlayerHost.Children.Remove(web); try { web.Dispose(); } catch { } }
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

            _switchingPlayer = true;
            if (_inlinePlayer?.CoreWebView2 is { } inline)
            {
                string state = await inline.ExecuteScriptAsync("({t:v.currentTime,playing:!v.paused&&!v.ended,v:v.volume,m:v.muted})");
                using var parsed = JsonDocument.Parse(state);
                ReadPlayerState(parsed.RootElement);
                await inline.ExecuteScriptAsync("v.pause()");
            }
            Logger.Info("OpenViewer: opening fullscreen player");

            var fs = new PlayerFullScreenWindow { ReturnBounds = GetPlayerScreenBounds };
            fs.Closed += (_, _) =>
            {
                if (ReferenceEquals(_fsWindow, fs)) CloseFullscreenPlayer();
            };
            fs.NavigationCompleted += ok =>
            {
                if (ok) SendAccentFs();
            };
            _fsWindow = fs;
            EnableWindow(_hwnd, false);

            await fs.OpenAsync(BuildPlayerUrl(item.VideoPath, _playerTime, true), json => { if (ReferenceEquals(_fsWindow, fs)) HandlePlayerMessage(json, true); }, _hwnd, GetPlayerScreenBounds());
            _switchingPlayer = false;
            if (!fs.WebReady) CloseFullscreenPlayer();
        }
        catch (Exception ex)
        {
            Logger.Error($"Opening fullscreen player failed: {ex.Message}", ex);
            CloseFullscreenPlayer();
        }
        finally { _switchingPlayer = false; }
    }

    private void CloseFullscreenPlayer(bool resumeInline = true)
    {
        var fs = _fsWindow;
        if (fs == null) return;
        _fsWindow = null;

        try { EnableWindow(_hwnd, true); } catch { }
        try { fs.Shutdown(); } catch { }
        try { SetForegroundWindow(_hwnd); } catch { }
        try { Activate(); } catch { }
        if (resumeInline && _inlinePlayer != null && ReferenceEquals(_currentPage, ViewerPage))
            SendScriptInline($"receiveCommand({{\"c\":\"restore\",\"t\":{_playerTime.ToString(CultureInfo.InvariantCulture)},\"playing\":{(_playerPlaying ? "true" : "false")},\"v\":{_playerVolume.ToString(CultureInfo.InvariantCulture)},\"m\":{(_playerMuted ? "true" : "false")} }})");
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct ScreenPoint { public int X, Y; }

    [DllImport("user32.dll")]
    private static extern bool ClientToScreen(IntPtr hWnd, ref ScreenPoint point);

    private ScreenRect? GetPlayerScreenBounds()
    {
        try
        {
            if (_isHidden || !ReferenceEquals(_currentPage, ViewerPage) || !PlayerCard.IsLoaded) return null;
            if (PlayerCard.ActualWidth < 2 || PlayerCard.ActualHeight < 2) return null;

            double scale = VisualTreeHelper.GetDpi(this).DpiScaleX;
            if (scale <= 0) scale = 1.0;
            Point topLeft = PlayerCard.TransformToAncestor(this).Transform(new Point(0, 0));
            var client = new ScreenPoint();
            if (!ClientToScreen(_hwnd, ref client)) return null;

            return new ScreenRect(
                client.X + (int)Math.Round(topLeft.X * scale),
                client.Y + (int)Math.Round(topLeft.Y * scale),
                (int)Math.Round(PlayerCard.ActualWidth * scale),
                (int)Math.Round(PlayerCard.ActualHeight * scale));
        }
        catch
        {
            return null;
        }
    }

    private void SendScriptFs(string js) => _fsWindow?.SendScript(js);

    private async Task DeleteReplayAsync(ReplayItem item)
    {
        if (_fsWindow != null)
        {
            var fs = _fsWindow;
            _fsWindow = null;
            try { fs.Shutdown(); } catch { }
        }
        try { Show(); } catch { }
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
    private void CardOpenFolderButton_Click(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement element &&
            (element.Tag as ReplayItem ?? element.DataContext as ReplayItem) is ReplayItem item)
            OpenContainingFolder(item.VideoPath);
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

    private string BuildPlayerUrl(string videoPath, double startSeconds, bool fullscreen)
    {
        string page = new Uri(Path.Combine(AppContext.BaseDirectory, "mediaplayer", "player.html")).AbsoluteUri;
        string src = new Uri(videoPath).AbsoluteUri;
        string t = startSeconds.ToString("0.###", CultureInfo.InvariantCulture);
        return $"{page}?src={Uri.EscapeDataString(src)}&t={t}&mode={(fullscreen ? "fullscreen" : "inline")}&autoplay={(_playerPlaying ? 1 : 0)}&volume={_playerVolume.ToString(CultureInfo.InvariantCulture)}&muted={(_playerMuted ? 1 : 0)}";
    }

    private void HandlePlayerMessage(string json, bool fullscreen)
    {
        try
        {
            using var doc = JsonDocument.Parse(json);
            JsonElement root = doc.RootElement;
            string type = root.GetProperty("type").GetString() ?? "";

            switch (type)
            {
                case "state":
                    ReadPlayerState(root);
                    break;
                case "fs":
                    string act = root.TryGetProperty("act", out var ae) ? ae.GetString() ?? "" : "";
                    if (fullscreen && act == "exit") CloseFullscreenPlayer();
                    else if (!fullscreen && act == "enter" && _viewerItem is { } item && !_switchingPlayer)
                        _ = OpenFullscreenPlayerAsync(item);
                    break;

                case "close":
                    if (fullscreen) CloseFullscreenPlayer(); else CloseViewer();
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

    private static bool IsFiniteNumber(double value) => !double.IsNaN(value) && !double.IsInfinity(value);

    private static DependencyProperty? AnimationProperty(string property)
    {
        switch (property)
        {
            case "Opacity": return UIElement.OpacityProperty;
            case "TranslateY":
            case "Y": return TranslateTransform.YProperty;
            default: return null;
        }
    }

    private static void AnimateDouble(DependencyObject target, string property, double from, double to,
        TimeSpan duration, TimeSpan? beginTime = null, EasingFunctionBase? easing = null, Action? onCompleted = null)
    {
        DependencyProperty? dp = AnimationProperty(property);
        if (dp == null || !(target is IAnimatable animatable)) return;

        // FillBehavior.Stop + writing the final value locally keeps later direct assignments (Opacity = 1, ...)
        // effective; a held animation clock would otherwise override them.
        var anim = new DoubleAnimation { From = from, To = to, Duration = duration, EasingFunction = easing, FillBehavior = FillBehavior.Stop };
        if (beginTime.HasValue) anim.BeginTime = beginTime.Value;
        anim.Completed += (_, _) =>
        {
            target.SetValue(dp, to);
            onCompleted?.Invoke();
        };
        animatable.BeginAnimation(dp, anim);
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
            new() { Text = "Start with Windows", Glyph = "\uE7E8", IsCheckable = true,
                    IsChecked = StartupManager.IsEnabled(), OnClick = () =>
            {
                bool newState = !StartupManager.IsEnabled();
                StartupManager.SetEnabled(newState);
                _dq.TryEnqueue(() => StartupToggle.IsChecked = newState);
            }},
            TrayMenuItem.Separator(),
            new() { Text = "Save Replay", Glyph = "\uE74E", Shortcut = "Alt+S",
                    OnClick = () => _ = SaveReplayAsync() },
            new() { Text = "Open Window", Glyph = "\uE8A7", OnClick = ShowFromTray },
            TrayMenuItem.Separator(),
            new() { Text = "Restart App", Glyph = "\uE72C", OnClick = RestartApplication },
            TrayMenuItem.Separator(),
            new() { Text = "Exit", Glyph = "\uE8BB", OnClick = ExitApplicationAsync },
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

        Show();
        if (WindowState == WindowState.Minimized) WindowState = WindowState.Normal;
        _isHidden = false;
        _taskbarBadge?.SetTaskbarVisible(true);
        ApplyFixedWindowSize();
        Activate();
        RootGrid.InvalidateMeasure();
        RootGrid.UpdateLayout();
        RefreshStartupToggleState();
        RefreshAppBadge();
    }

    private void RefreshAppBadge() =>
        UpdateAppBadge(_hasErrored ? BadgeKind.Error
                   : _engine.IsRunning && !_engine.IsPaused ? BadgeKind.Recording
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
            string? exePath = Compat.ProcessPath;
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
        ShowStartFailure();
    }

    private void ShowStartFailure()
    {
        string detail = ReplayEngine.LastStartError;
        string message = detail.Length > 0
            ? detail + " The debug log contains the failure details."
            : "Capture could not start. Check that the target is available and try the GPU setting in Settings. The debug log contains the failure details.";
        ShowInfoBar(InfoBarSeverity.Error, "Recording could not start", message, "start");
        _infoBarTimer?.Stop();
    }

    private void SetRunningVisual(bool running)
    {
        StopPulse(RecDot, ref _recDotPulseTimer);
        _lastBufferHealthy = running ? true : null;

        if (running)
        {
            StatusHeadline.Text = _engine.TargetState;
            RecDot.Fill = GetThemedBrush("SuccessBrush");
            RecDot.Opacity = 1;
            var t = _recDotPulseTimer;
            if (!_engine.IsPaused) StartPulse(RecDot, ref t);
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

        if (_engine.IsPaused)
        {
            StopPulse(RecDot, ref _recDotPulseTimer);
            StatusHeadline.Text = _engine.TargetState;
            RecDot.Fill = GetThemedBrush("SuccessBrush");
            RecDot.Opacity = 1;
            ApplyStartStopLook(true);
            RefreshAppBadge();
            _lastBufferHealthy = null;
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
        var profile = VideoQualityProfile.FromPreset(_settings.QualityPreset, _settings.FrameRate);
        long estimatedMb = Math.Min(512L,
            (long)profile.BitrateKbps * _settings.ReplayDurationSeconds / 8 / 1024);
        Logger.Debug($"Buffer Status - Duration: {_settings.ReplayDurationSeconds}s, " +
                     $"Estimated buffer: ~{estimatedMb} MB (cap 512 MB)");
        _engine.LogDebugDiagnostics();
    }

    private void CheckForMemoryLeak()
    {
        if (!_engine.IsRunning)
        {
            _memoryObservationStartedUtc = DateTime.MinValue;
            _highMemStreak = 0;
            if (_leakWarningActive) StopLeakWarning();
            return;
        }

        int generation = _engine.CaptureGeneration;
        if (generation != _memoryGeneration)
        {
            _memoryGeneration = generation;
            _memoryObservationStartedUtc = DateTime.MinValue;
            _highMemStreak = 0;
            if (_leakWarningActive) StopLeakWarning();
        }

        long memMb = Environment.WorkingSet / (1024 * 1024);
        DateTime now = DateTime.UtcNow;
        if (_memoryObservationStartedUtc == DateTime.MinValue)
        {
            _memoryObservationStartedUtc = now;
            _memoryBaselineMb = memMb;
            _lastMemorySampleMb = memMb;
            return;
        }

        TimeSpan fillAndSettle = TimeSpan.FromSeconds(
            Math.Max(_settings.ReplayDurationSeconds, 1) + 60);
        if (now - _memoryObservationStartedUtc < fillAndSettle)
        {
            _memoryBaselineMb = memMb;
            _lastMemorySampleMb = memMb;
            _highMemStreak = 0;
            return;
        }

        _memoryBaselineMb = Math.Min(_memoryBaselineMb, memMb);
        long growthMb = memMb - _memoryBaselineMb;
        bool stillGrowing = memMb >= _lastMemorySampleMb + 4;
        _lastMemorySampleMb = memMb;
        if (growthMb >= LeakGrowthMb && stillGrowing) _highMemStreak++;
        else if (growthMb < LeakGrowthMb * 3 / 4) _highMemStreak = 0;

        if (_leakWarningActive && growthMb < LeakGrowthMb / 2)
        {
            StopLeakWarning();
            return;
        }

        if (_highMemStreak >= LeakConsecutiveSamplesRequired && !_leakWarningActive)
        {
            _leakWarningActive = true;
            Logger.Warn($"Memory usage kept growing after the replay ring stabilized " +
                        $"({_memoryBaselineMb} MB to {memMb} MB) — possible leak.");
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

        _dq.TryEnqueue(() => { if (!_isClosingForReal) ClipNotificationWindow.BeginEncoding(); });
        var progress = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(100) };
        progress.Tick += (_, _) => ClipNotificationWindow.ReportProgress(_engine.SaveProgress);
        progress.Start();

        SaveReplayResult result;
        try
        {
            result = await _engine.SaveReplayAsync(_tempDir, destDir).ConfigureAwait(true);
        }
        catch (Exception ex)
        {
            Logger.Error("Failed to clip replay", ex);
            result = new SaveReplayResult(SaveReplayOutcome.MergeFailed);
        }
        finally
        {
            progress.Stop();
        }

        _lastSavedReplayPath = result.Success ? result.Path : null;
        int clippedSeconds = result.Success
            ? await Task.Run(() => ClipNotificationWindow.RecordedSeconds(result.Path, _settings.ReplayDurationSeconds))
            : 0;

        _dq.TryEnqueue(() =>
        {
            _lastBalloonWasLeakWarning = false;
            ShowSaveResult(result, clippedSeconds);
        });
    }

    private void ShowSaveResult(SaveReplayResult result, int clippedSeconds)
    {
        _lastBalloonWasLeakWarning = false;

        if (!result.Success)
        {
            Logger.Warn($"Failed to clip replay: {result.UserMessage()}");
            if (!_isClosingForReal) ClipNotificationWindow.Fail();
            return;
        }

        if (!_isClosingForReal) ClipNotificationWindow.Complete(clippedSeconds);
    }

    private async void StartStopButton_Click(object sender, RoutedEventArgs e)
    {
        StartStopButton.IsEnabled = false;
        if (_engine.IsRunning)
        {
            SetStartStopLabel("Stopping…");
            await Task.Run(() => _engine.Stop());
            SetRunningVisual(false);
        }
        else
        {
            SetStartStopLabel("Starting…");
            bool ok = await Task.Run(StartCaptureCore);
            SetRunningVisual(ok);
            if (!ok) ShowStartFailure();
            else ClearInfoBar();
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
            try
            {
                var missingBrush = new SolidColorBrush(Color.FromArgb(255, 229, 72, 77));
                MicCombo.Foreground = missingBrush;
                MicCombo.Resources["ComboBoxForeground"] = missingBrush;
                MicCombo.Resources["ComboBoxForegroundPointerOver"] = missingBrush;
                MicCombo.Resources["ComboBoxForegroundPressed"] = missingBrush;
                MicCombo.Resources["ComboBoxForegroundSelected"] = missingBrush;
                MicCombo.Resources["ComboBoxForegroundFocused"] = missingBrush;
                MicCombo.Resources["TextFillColorPrimaryBrush"] = missingBrush;
                MicCombo.Resources["ComboBoxItemForeground"] = missingBrush;
                MicCombo.Resources["ComboBoxItemForegroundPointerOver"] = missingBrush;
                MicCombo.Resources["ComboBoxItemForegroundPressed"] = missingBrush;
                MicCombo.Resources["ComboBoxItemForegroundSelected"] = missingBrush;
            }
            catch { }
            try { MicCombo.IsEnabled = true; } catch { }

        }
        else
        {
            try { MicCombo.ClearValue(Control.ForegroundProperty); }
            catch { }
            MicCombo.Resources.Remove("ComboBoxForeground");
            MicCombo.Resources.Remove("ComboBoxForegroundPointerOver");
            MicCombo.Resources.Remove("ComboBoxForegroundPressed");
            MicCombo.Resources.Remove("ComboBoxForegroundSelected");
            MicCombo.Resources.Remove("ComboBoxForegroundFocused");
            MicCombo.Resources.Remove("TextFillColorPrimaryBrush");
            MicCombo.Resources.Remove("ComboBoxItemForeground");
            MicCombo.Resources.Remove("ComboBoxItemForegroundPointerOver");
            MicCombo.Resources.Remove("ComboBoxItemForegroundPressed");
            MicCombo.Resources.Remove("ComboBoxItemForegroundSelected");
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
        RefreshQualityDetails();
        CheckDirty();
    }

    private void InitQualitySegments()
    {
        QualityCombo.SelectedIndex = VideoQualityProfile.PresetIndex(_settings.QualityPreset);
        RefreshQualityDetails();
    }

    private void RefreshBitrateDisplay()
    {
        (int width, int height) = ReplayEngine.MonitorSize(_settings.CaptureMonitorIndex);
        int kbps = VideoQualityProfile.TargetBitrateKbps(
            _settings.QualityPreset, _settings.FrameRate, width, height);
        BitrateValueText.Text = $"{kbps / 1000.0:0.#} Mbps ({kbps:N0} kbps)";
    }

    private void RefreshQualityDetails()
    {
        string[] presets = VideoQualityProfile.PresetNames;
        for (int i = 0; i < presets.Length && i < QualityCombo.Items.Count; i++)
            if (QualityCombo.Items[i] is ComboBoxItem item)
                item.Content = VideoQualityProfile.FromPreset(presets[i], _settings.FrameRate).Label;
        RefreshBitrateDisplay();
    }

    private void QualityCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_initializing) return;
        _settings.QualityPreset = VideoQualityProfile.PresetNames[
            Compat.Clamp(QualityCombo.SelectedIndex, 0, VideoQualityProfile.PresetNames.Length - 1)];
        RefreshBitrateDisplay();
        CheckDirty();
    }

    private void InitAudioToggles()
    {
        _syncingAudioToggles = true;
        SysAudioToggle.IsChecked = _settings.RecordSystemAudio;
        MicToggle.IsChecked = _settings.RecordMicrophone;
        _syncingAudioToggles = false;
    }

    // Combo indices map straight to these arrays (keep in sync with MainWindow.xaml).
    private static readonly string[] CodecChoices = { "H264", "Hevc", "Av1" };
    private static readonly int[] AudioBitrates = { 96, 128, 160, 192 };

    private void InitEncoderSettings()
    {
        CodecCombo.SelectedIndex = Math.Max(0, Array.IndexOf(CodecChoices, _settings.VideoCodec));
        BufferMethodCombo.SelectedIndex = _settings.BufferMethod == "Segmented" ? 1 : 0;
        GpuEncoderCheck.IsChecked = _settings.GpuEncoder;
        GpuPreferenceCombo.SelectedIndex = _settings.GpuPreference == "Prefer iGPU" ? 0 : 1;
        UpdateGpuControls();
        RefreshBitrateDisplay();
    }

    private void BufferMethodCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_initializing) return;
        _settings.BufferMethod = BufferMethodCombo.SelectedIndex == 1 ? "Segmented" : "Continuous";
        CheckDirty();
    }

    private void UpdateGpuControls()
    {
        GpuPreferenceCombo.IsEnabled = GpuEncoderCheck.IsChecked == true;
    }

    private void GpuEncoder_Changed(object sender, RoutedEventArgs e)
    {
        if (_initializing) return;
        _settings.GpuEncoder = GpuEncoderCheck.IsChecked == true;
        UpdateGpuControls();
        CheckDirty();
    }

    private void GpuPreferenceCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_initializing) return;
        _settings.GpuPreference = GpuPreferenceCombo.SelectedIndex == 0 ? "Prefer iGPU" : "Prefer dGPU";
        CheckDirty();
    }

    private void CodecCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_initializing) return;
        int i = CodecCombo.SelectedIndex;
        _settings.VideoCodec = i >= 0 && i < CodecChoices.Length ? CodecChoices[i] : "H264";
        CheckDirty();
    }

    private void InitAudioSettings()
    {
        AudioCodecCombo.SelectedIndex = _settings.AudioCodec == "FLAC" ? 1 : 0;
        int index = Array.IndexOf(AudioBitrates, _settings.AudioBitrate);
        AudioBitrateCombo.SelectedIndex = index < 0 ? 2 : index;
        ApplicationLocalAudioToggle.IsChecked = _settings.ApplicationLocalAudio;
        UpdateAudioCodecControls();
    }

    private void UpdateAudioCodecControls()
    {
        AudioBitrateRow.IsEnabled = AudioCodecCombo.SelectedIndex == 0;
    }

    private void AudioCodecCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_initializing) return;
        _settings.AudioCodec = AudioCodecCombo.SelectedIndex == 1 ? "FLAC" : "AAC";
        UpdateAudioCodecControls();
        CheckDirty();
    }

    private void AudioBitrateCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_initializing) return;
        int i = AudioBitrateCombo.SelectedIndex;
        _settings.AudioBitrate = i >= 0 && i < AudioBitrates.Length ? AudioBitrates[i] : 160;
        CheckDirty();
    }

    private void ApplicationLocalAudioToggle_Changed(object sender, RoutedEventArgs e)
    {
        if (_initializing || _syncingAudioToggles) return;
        _settings.ApplicationLocalAudio = (ApplicationLocalAudioToggle.IsChecked == true);
        CheckDirty();
    }

    private List<(int Index, string Name)> _captureMonitors = new();
    private bool _syncingCaptureTargets;
    private void InitCaptureTargets()
    {
        _syncingCaptureTargets = true;
        try
        {
            CaptureTargetCombo.SelectedIndex = _settings.CaptureTarget == "SpecificWindows" ? 1 : 0;
            _captureMonitors = ReplayEngine.EnumerateMonitors();
            CaptureMonitorCombo.ItemsSource = _captureMonitors.Select(m => m.Name).ToList();
            int savedMonitor = _settings.CaptureMonitorIndex < 0 ? 0 : _captureMonitors.FindIndex(m => m.Index == _settings.CaptureMonitorIndex);
            CaptureMonitorCombo.SelectedIndex = savedMonitor < 0 ? 0 : savedMonitor;
            CaptureMethodCombo.SelectedIndex = _settings.CaptureMethod == "Wgc" ? 1 : 0;
            CaptureCursorCheck.IsChecked = _settings.CaptureCursor;
            CaptureClientAreaCheck.IsChecked = _settings.CaptureClientArea;
            CaptureBorderCheck.IsChecked = _settings.CaptureBorder;
            UpdateCaptureTargetControls();
        }
        finally { _syncingCaptureTargets = false; }
    }
    private void UpdateCaptureTargetControls()
    {
        bool window = _settings.CaptureTarget == "SpecificWindows";
        CaptureMonitorRow.Visibility = window ? Visibility.Collapsed : Visibility.Visible;
        CaptureMethodRow.Visibility = window ? Visibility.Collapsed : Visibility.Visible;
        NavGamesItem.Visibility = window ? Visibility.Visible : Visibility.Collapsed;
        RecordAudioRow.Label = window ? "Record app audio" : "Record system audio";
    }
    private void CaptureTargetCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_initializing || _syncingCaptureTargets) return;
        _settings.CaptureTarget = CaptureTargetCombo.SelectedIndex == 1 ? "SpecificWindows" : "Display";
        UpdateCaptureTargetControls();
        CheckDirty();
    }
    private void RefreshGamesPage()
    {
        CustomGamesList.ItemsSource = _settings.SpecificWindows.ToArray();
        GamesEmptyState.Visibility = _settings.SpecificWindows.Count == 0 ? Visibility.Visible : Visibility.Collapsed;
        GameInfoBar1.IsOpen = !_settings.DismissedGameInfoBar1;
        GameInfoBar2.IsOpen = !_settings.DismissedGameInfoBar2;
        GameInfoBar2.Message = $"The App Can Detect {GameDetector.TotalCatalogCount:N0} Games Offline From A Local Database Including Your Custom Games Or Apps Added Manually.";
    }

    private void DismissGameInfoBar1_Click(object sender, RoutedEventArgs e)
    {
        _settings.DismissedGameInfoBar1 = true;
        _settings.SaveFields(nameof(Settings.DismissedGameInfoBar1));
        GameInfoBar1.IsOpen = false;
    }

    private void DismissGameInfoBar2_Click(object sender, RoutedEventArgs e)
    {
        _settings.DismissedGameInfoBar2 = true;
        _settings.SaveFields(nameof(Settings.DismissedGameInfoBar2));
        GameInfoBar2.IsOpen = false;
    }

    private static Wpf.Ui.Controls.TextBox LabeledBox(StackPanel host, string header, string placeholder, int maxLength)
    {
        host.Children.Add(new TextBlock { Text = header, FontSize = 12, Margin = new Thickness(0, host.Children.Count == 0 ? 0 : 12, 0, 4) });
        var box = new Wpf.Ui.Controls.TextBox { PlaceholderText = placeholder, MaxLength = maxLength };
        host.Children.Add(box);
        return box;
    }

    private async void AddGame_Click(object sender, RoutedEventArgs e)
    {
        var panel = new StackPanel { MinWidth = 340 };
        var name = LabeledBox(panel, "App or game name", "My game", 120);
        var exe = LabeledBox(panel, "Executable name", "game.exe", 260);
        var browse = new Wpf.Ui.Controls.Button { Content = "Browse to executable…", HorizontalAlignment = HorizontalAlignment.Left, Margin = new Thickness(0, 12, 0, 0) };
        panel.Children.Add(browse);
        var keyword = LabeledBox(panel, "Title keyword (optional)", "For example: Minecraft", 120);
        var error = new TextBlock { TextWrapping = TextWrapping.Wrap, Foreground = GetThemedBrush("ErrorBrush"), Margin = new Thickness(0, 12, 0, 0) };
        panel.Children.Add(error);
        browse.Click += (_, _) =>
        {
            try
            {
                var picker = new Microsoft.Win32.OpenFileDialog { Filter = "Executable (*.exe)|*.exe", CheckFileExists = true };
                if (picker.ShowDialog(this) == true)
                {
                    string fileName = Path.GetFileName(picker.FileName);
                    exe.Text = fileName;
                    if (string.IsNullOrWhiteSpace(name.Text)) name.Text = Path.GetFileNameWithoutExtension(fileName);
                }
            }
            catch (Exception ex) { error.Text = ex.Message; }
        };

        while (true)
        {
            var dialog = new Wpf.Ui.Controls.MessageBox
            {
                Title = "Add a specific window",
                Content = panel,
                PrimaryButtonText = "Add",
                CloseButtonText = "Cancel",
                Owner = this,
            };
            Wpf.Ui.Controls.MessageBoxResult result = await dialog.ShowDialogAsync();
            dialog.Content = null;
            if (result != Wpf.Ui.Controls.MessageBoxResult.Primary) return;

            string executable = exe.Text.Trim();
            if (!executable.EndsWith(".exe", StringComparison.OrdinalIgnoreCase) || executable.IndexOfAny(Path.GetInvalidFileNameChars()) >= 0 || executable.IndexOf('/') >= 0 || executable.IndexOf('\\') >= 0)
            { error.Text = "Enter a filename such as game.exe, or use Browse."; continue; }
            if (_settings.SpecificWindows.Any(r => r.Executable.Equals(executable, StringComparison.OrdinalIgnoreCase) && r.Keyword.Equals(keyword.Text.Trim(), StringComparison.OrdinalIgnoreCase)))
            { error.Text = "That executable and keyword are already in your list."; continue; }
            break;
        }

        string filename = exe.Text.Trim();
        _settings.SpecificWindows = new List<GameRule>(_settings.SpecificWindows)
        {
            new GameRule(string.IsNullOrWhiteSpace(name.Text) ? Path.GetFileNameWithoutExtension(filename) : name.Text.Trim(), filename, keyword.Text.Trim())
        };
        _settings.Save();
        _engine.ReloadGameRules();
        RefreshGamesPage();
    }

    private void RemoveGame_Click(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { Tag: GameRule rule }) return;
        _settings.SpecificWindows = _settings.SpecificWindows.Where(r => r != rule).ToList();
        _settings.Save();
        _engine.ReloadGameRules();
        RefreshGamesPage();
    }

    private void CaptureMonitorCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_initializing || _syncingCaptureTargets) return;
        if (CaptureMonitorCombo.SelectedIndex >= 0 && CaptureMonitorCombo.SelectedIndex < _captureMonitors.Count)
            _settings.CaptureMonitorIndex = _captureMonitors[CaptureMonitorCombo.SelectedIndex].Index;
        CheckDirty();
    }
    private void CaptureMethodCombo_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_initializing || _syncingCaptureTargets) return;
        _settings.CaptureMethod = CaptureMethodCombo.SelectedIndex == 1 ? "Wgc" : "Dxgi";
        CheckDirty();
    }
    private void WindowCaptureOption_Changed(object sender, RoutedEventArgs e)
    {
        if (_initializing || _syncingCaptureTargets) return;
        _settings.CaptureCursor = CaptureCursorCheck.IsChecked == true;
        _settings.CaptureClientArea = CaptureClientAreaCheck.IsChecked == true;
        _settings.CaptureBorder = CaptureBorderCheck.IsChecked == true;
        CheckDirty();
    }

    private void DebugLoggingToggle_Changed(object sender, RoutedEventArgs e)
    {
        if (_initializing) return;
        _settings.DebugLogging = (DebugLoggingToggle.IsChecked == true);
        Logger.SetDebugMode(_settings.DebugLogging);
        CheckDirty();
    }

    private void SysAudioToggle_Changed(object sender, RoutedEventArgs e)
    {
        if (_initializing || _syncingAudioToggles) return;
        _settings.RecordSystemAudio = (SysAudioToggle.IsChecked == true);
        CheckDirty();
    }

    private void MicToggle_Changed(object sender, RoutedEventArgs e)
    {
        if (_initializing || _syncingAudioToggles) return;
        _settings.RecordMicrophone = (MicToggle.IsChecked == true);
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
        HotkeyCaptureButton.Focus();
    }

    private void EndHotkeyCapture()
    {
        _capturingHotkey = false;
        _pendingHotkeyModifier = 0;
        StopPulse(HotkeyCaptureButton, ref _hotkeyPulseTimer);
        HotkeyCaptureButton.Style = (Style)Application.Current.Resources["BaseButtonStyle"];
        UpdateHotkeyButtonDisplay();
    }

    private void UpdateHotkeyButtonDisplay()
    {
        HotkeyCaptureText.Text = HotkeyDisplay.Format(_settings.HotkeyModifiers, _settings.HotkeyVk);
        UpdateHomeHotkeyDisplay();
    }

    private void UpdateHomeHotkeyDisplay()
    {
        HotkeyKeys.Children.Clear();

        IReadOnlyList<string> parts = HotkeyDisplay.Parts(_settings.HotkeyModifiers, _settings.HotkeyVk);
        for (int i = 0; i < parts.Count; i++)
        {
            if (i > 0)
            {
                HotkeyKeys.Children.Add(new TextBlock
                {
                    Text = "+",
                    FontSize = 10,
                    Foreground = new SolidColorBrush(Color.FromArgb(0x99, 255, 255, 255)),
                    VerticalAlignment = VerticalAlignment.Center,
                });
            }

            HotkeyKeys.Children.Add(new Border
            {
                Padding = new Thickness(5, 1, 5, 2),
                CornerRadius = new CornerRadius(4),
                Background = new SolidColorBrush(Color.FromArgb(0x26, 255, 255, 255)),
                BorderBrush = new SolidColorBrush(Color.FromArgb(0x33, 255, 255, 255)),
                BorderThickness = new Thickness(1),
                VerticalAlignment = VerticalAlignment.Center,
                Child = new TextBlock
                {
                    Text = parts[i],
                    FontSize = 10,
                    FontWeight = FontWeights.SemiBold,
                    Foreground = new SolidColorBrush(Color.FromArgb(0xF0, 255, 255, 255)),
                },
            });
        }

        // 4 px between key pills (WinUI StackPanel.Spacing).
        for (int i = 1; i < HotkeyKeys.Children.Count; i++)
            ((FrameworkElement)HotkeyKeys.Children[i]).Margin = new Thickness(4, 0, 0, 0);
    }

    private void Window_PreviewKeyDown(object sender, KeyEventArgs e)
    {
        if (!_capturingHotkey) return;
        Key key = e.Key == Key.System ? e.SystemKey : e.Key;
        if (key == Key.ImeProcessed || key == Key.None) return;

        if (key == Key.Escape)
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

        uint vk = (uint)KeyInterop.VirtualKeyFromKey(key);
        e.Handled = true;
        FinalizeHotkey(_pendingHotkeyModifier, vk);
    }

    private void Window_PreviewKeyUp(object sender, KeyEventArgs e)
    {
        if (!_capturingHotkey || _pendingHotkeyModifier == 0) return;
        Key released = e.Key == Key.System ? e.SystemKey : e.Key;
        if (HotkeyDisplay.ModifierBitForKey(released) == _pendingHotkeyModifier)
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
        bool enabled = (StartupToggle.IsChecked == true);
        StartupManager.SetEnabled(enabled);
    }

    private void RefreshStartupToggleState()
    {
        bool actuallyEnabled = StartupManager.IsEnabled();
        if ((StartupToggle.IsChecked == true) == actuallyEnabled) return;

        _syncingStartupToggle = true;
        StartupToggle.IsChecked = actuallyEnabled;
        _syncingStartupToggle = false;
    }

    private void SaveLocationBox_TextChanged(object sender, TextChangedEventArgs e)
    {
        if (_initializing) return;
        _settings.SaveDestination = SaveLocationBox.Text;
        CheckDirty();
    }

    private void BrowseSaveLocationButton_Click(object sender, RoutedEventArgs e)
    {
        try
        {
            string? folder = FolderPicker.PickFolder(_hwnd, SaveLocationBox.Text);
            if (folder != null) SaveLocationBox.Text = folder;
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

    private void EnsureInfoBarTimer()
    {
        if (_infoBarTimer != null) return;

        _infoBarTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(3) };
        _infoBarTimer.Tick += (_, _) =>
        {
            _infoBarTimer!.Stop();
            ClearInfoBar();
        };

        StatusInfoBar.MouseEnter += (_, _) => _infoBarTimer?.Stop();
        StatusInfoBar.MouseLeave += (_, _) =>
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
        StatusInfoActionButton.Content = actionLabel ?? "";
        StatusInfoActionButton.Visibility = actionLabel is null
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
        StatusInfoActionButton.Visibility = Visibility.Collapsed;
    }

    private void UpdatePendingFlyout()
    {
        if (_dirty == _flyoutShown) return;
        _flyoutShown = _dirty;

        if (_dirty)
        {
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
        _settings.VideoCodec = previous.VideoCodec;
        _settings.GpuEncoder = previous.GpuEncoder;
        _settings.GpuPreference = previous.GpuPreference;
        _settings.BufferMethod = previous.BufferMethod;
        _settings.CaptureBorder = previous.CaptureBorder;
        _settings.CaptureCursor = previous.CaptureCursor;
        _settings.CaptureClientArea = previous.CaptureClientArea;
        _settings.CaptureMonitorIndex = previous.CaptureMonitorIndex;
        _settings.CaptureMethod = previous.CaptureMethod;
        _settings.CaptureTarget = previous.CaptureTarget;
        _settings.Microphone = previous.Microphone;
        _settings.SaveDestination = previous.SaveDestination;
        _settings.RecordSystemAudio = previous.RecordSystemAudio;
        _settings.RecordMicrophone = previous.RecordMicrophone;
        _settings.AudioCodec = previous.AudioCodec;
        _settings.AudioBitrate = previous.AudioBitrate;
        _settings.ApplicationLocalAudio = previous.ApplicationLocalAudio;
        _settings.DebugLogging = previous.DebugLogging;
        Logger.SetDebugMode(_settings.DebugLogging);

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

            MicToggle.IsChecked = _settings.RecordMicrophone;
            SysAudioToggle.IsChecked = _settings.RecordSystemAudio;
            InitDurationSegments();
            InitFpsSegments();
            InitQualitySegments();
            SaveLocationBox.Text = _settings.GetEffectiveSaveDestination();
            InitEncoderSettings();
            InitAudioSettings();
            InitCaptureTargets();
            DebugLoggingToggle.IsChecked = _settings.DebugLogging;
        }
        finally
        {
            _initializing = wasInitializing;
        }

    }

    private async void PendingSaveButton_Click(object sender, RoutedEventArgs e) => await SaveChangesAsync();

    private void PendingDiscardButton_Click(object sender, RoutedEventArgs e)
    {
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
        bool restartTimedOut = false;
        try
        {
            _settings.Save();

            if (_engine.IsRunning)
            {
                restarted = true;
                Task<bool> restart = Task.Run(() => _engine.Restart(_tempDir, wipeSegments: true));
                Task finished = await Task.WhenAny(restart, Task.Delay(TimeSpan.FromSeconds(15)));
                if (finished == restart)
                {
                    ok = await restart;
                }
                else
                {
                    // The settings are already on disk. Never leave the UI saying
                    // "Saving changes" forever if native cleanup or start hangs.
                    ok = false;
                    restartTimedOut = true;
                    Logger.Warn("Save changes: engine restart timed out after 15 seconds; continuing without waiting for it.");
                }
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
            else if (restartTimedOut)
            {
                ShowInfoBar(InfoBarSeverity.Warning, "Settings saved",
                    "Your settings were saved, but restarting the capture engine took too long. Check the debug log for details.");
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
            UpdateSpark(_cpuHistory, pct, CpuSparkHost, CpuSparkLine, CpuSparkArea);
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
                UpdateSpark(_ramHistory, m.dwMemoryLoad, RamSparkHost, RamSparkLine, RamSparkArea);
            }
        }
        catch (Exception ex) { Logger.Warn($"Memory stats failed: {ex.Message}"); }
    }

    private void UpdateSpark(List<double> history, double pct, FrameworkElement host,
        System.Windows.Shapes.Polyline line, System.Windows.Shapes.Polygon area)
    {
        history.Add(Compat.Clamp(pct, 0, 100));
        if (history.Count > SparkSamples) history.RemoveAt(0);

        Brush b = pct >= SparkAlertPct ? AlertBrush : _normalBarBrush;
        line.Stroke = b;
        area.Fill = b;

        RenderSpark(history, host, line, area);
    }

    private static void RenderSpark(List<double> history, FrameworkElement host,
        System.Windows.Shapes.Polyline line, System.Windows.Shapes.Polygon area)
    {
        double w = host.ActualWidth, h = host.ActualHeight;
        if (w <= 0 || h <= 0 || history.Count < 2)
        {
            line.Points = new PointCollection();
            area.Points = new PointCollection();
            return;
        }

        const double pad = 3;
        var pts = new PointCollection();
        double firstX = 0, lastX = 0;
        for (int i = 0; i < history.Count; i++)
        {
            double x = w * (SparkSamples - history.Count + i) / (SparkSamples - 1);
            double y = h - pad - history[i] / 100.0 * (h - 2 * pad);
            if (i == 0) firstX = x;
            lastX = x;
            pts.Add(new Point(x, y));
        }
        line.Points = pts;

        var fill = new PointCollection();
        foreach (var p in pts) fill.Add(p);
        fill.Add(new Point(lastX, h));
        fill.Add(new Point(firstX, h));
        area.Points = fill;
    }

    private void SparkHost_SizeChanged(object sender, SizeChangedEventArgs e)
    {
        if (ReferenceEquals(sender, CpuSparkHost))
            RenderSpark(_cpuHistory, CpuSparkHost, CpuSparkLine, CpuSparkArea);
        else if (ReferenceEquals(sender, RamSparkHost))
            RenderSpark(_ramHistory, RamSparkHost, RamSparkLine, RamSparkArea);
    }

    private void ExitApplicationAsync()
    {
        if (_isClosingForReal) return;

        if (_dirty) RevertPendingChanges();

        _isClosingForReal = true;
        _updateTimer?.Stop();
        _updateLifetime.Cancel();

        UnregisterHotKey(_hwnd, HotkeyId);
        RegisterRawKeyboard(false);

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
        // Native cleanup used to wait forever on capture threads. Bound it so
        // the tray Exit item always terminates the process.
        Task cleanup = Task.Run(() =>
        {
            try { _engine.Stop(); _engine.Dispose(); }
            catch (Exception ex) { Logger.Warn($"Engine cleanup during exit failed: {ex.Message}"); }
        });
        if (Task.WhenAny(cleanup, Task.Delay(TimeSpan.FromSeconds(5))).GetAwaiter().GetResult() != cleanup)
            Logger.Warn("Engine cleanup during exit timed out; forcing application exit.");
        ClipNotificationWindow.Shutdown();

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
        else _ = Task.Run(async () =>
        {
            await Task.Delay(TimeSpan.FromSeconds(2));
            Environment.Exit(0);
        });
    }
}

/// <summary>Marshals work onto the UI thread (stands in for WinUI's DispatcherQueue.TryEnqueue).</summary>
internal sealed class UiQueue
{
    private readonly Dispatcher _dispatcher;

    public UiQueue(Dispatcher dispatcher) => _dispatcher = dispatcher;

    public bool TryEnqueue(Action action)
    {
        try { _dispatcher.BeginInvoke(action); return true; }
        catch { return false; }
    }

    public bool TryEnqueue(Func<Task> action)
    {
        try
        {
            _dispatcher.BeginInvoke(new Action(async () =>
            {
                try { await action(); }
                catch (Exception ex) { Logger.Error("Unhandled error in queued UI work", ex); }
            }));
            return true;
        }
        catch { return false; }
    }
}

/// <summary>Vista-style folder picker (IFileOpenDialog with FOS_PICKFOLDERS); .NET Framework's FolderBrowserDialog is the old tree.</summary>
internal static class FolderPicker
{
    [ComImport, Guid("DC1C5A9C-E88A-4DDE-A5A1-60F82A20AEF7")]
    private class FileOpenDialogCom { }

    [ComImport, Guid("42F85136-DB7E-439C-85F1-E4075D135FC8"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IFileDialog
    {
        [PreserveSig] int Show(IntPtr parent);
        void SetFileTypes(uint count, IntPtr filterSpec);
        void SetFileTypeIndex(uint index);
        void GetFileTypeIndex(out uint index);
        void Advise(IntPtr events, out uint cookie);
        void Unadvise(uint cookie);
        void SetOptions(uint options);
        void GetOptions(out uint options);
        void SetDefaultFolder(IShellItem item);
        void SetFolder(IShellItem item);
        void GetFolder(out IShellItem item);
        void GetCurrentSelection(out IShellItem item);
        void SetFileName([MarshalAs(UnmanagedType.LPWStr)] string name);
        void GetFileName([MarshalAs(UnmanagedType.LPWStr)] out string name);
        void SetTitle([MarshalAs(UnmanagedType.LPWStr)] string title);
        void SetOkButtonLabel([MarshalAs(UnmanagedType.LPWStr)] string text);
        void SetFileNameLabel([MarshalAs(UnmanagedType.LPWStr)] string text);
        void GetResult(out IShellItem item);
        void AddPlace(IShellItem item, int placement);
        void SetDefaultExtension([MarshalAs(UnmanagedType.LPWStr)] string extension);
        void Close(int hr);
        void SetClientGuid(ref Guid guid);
        void ClearClientData();
        void SetFilter(IntPtr filter);
    }

    [ComImport, Guid("43826D1E-E718-42EE-BC55-A1E261C37BFE"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IShellItem
    {
        void BindToHandler(IntPtr bindContext, ref Guid handler, ref Guid iid, out IntPtr result);
        void GetParent(out IShellItem item);
        void GetDisplayName(uint sigdn, [MarshalAs(UnmanagedType.LPWStr)] out string name);
        void GetAttributes(uint mask, out uint attributes);
        void Compare(IShellItem other, uint hint, out int order);
    }

    [DllImport("shell32.dll", CharSet = CharSet.Unicode, PreserveSig = true)]
    private static extern int SHCreateItemFromParsingName(string path, IntPtr bindContext, ref Guid iid, out IShellItem item);

    private const uint FosPickFolders = 0x00000020;
    private const uint FosForceFileSystem = 0x00000040;
    private const uint SigdnFileSysPath = 0x80058000;
    private const int ErrorCancelled = unchecked((int)0x800704C7);

    public static string? PickFolder(IntPtr owner, string? initialFolder)
    {
        var dialog = (IFileDialog)new FileOpenDialogCom();
        try
        {
            dialog.GetOptions(out uint options);
            dialog.SetOptions(options | FosPickFolders | FosForceFileSystem);

            if (!string.IsNullOrWhiteSpace(initialFolder) && Directory.Exists(initialFolder))
            {
                Guid iid = typeof(IShellItem).GUID;
                if (SHCreateItemFromParsingName(initialFolder, IntPtr.Zero, ref iid, out IShellItem start) == 0)
                    dialog.SetFolder(start);
            }

            int hr = dialog.Show(owner);
            if (hr == ErrorCancelled) return null;
            if (hr != 0) Marshal.ThrowExceptionForHR(hr);

            dialog.GetResult(out IShellItem result);
            result.GetDisplayName(SigdnFileSysPath, out string path);
            return path;
        }
        finally
        {
            Marshal.ReleaseComObject(dialog);
        }
    }
}
