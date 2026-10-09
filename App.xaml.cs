using System.Diagnostics;
using Microsoft.UI.Xaml;

namespace InstantReplay;

public partial class App : Application
{
    private const string SingleInstanceMutexName = "Global\\InstantReplay_SingleInstance_Mutex";
    private const string ShowWindowEventName = "Global\\InstantReplay_ShowWindow_Event";

    private static readonly TimeSpan WatchdogInterval = TimeSpan.FromSeconds(15);

    private Window? _window;
    private Mutex? _singleInstanceMutex;
    private EventWaitHandle? _showWindowEvent;

    public App()
    {
        InitializeComponent();
        UnhandledException += (_, e) =>
        {
            Logger.Error($"Unhandled exception: {e.Exception}", e.Exception);
            e.Handled = true;
        };
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        _singleInstanceMutex = new Mutex(initiallyOwned: true, SingleInstanceMutexName, out bool createdNew);

        if (!createdNew)
        {
            Logger.Info("Another Instant Replay instance is already running; signaling it and exiting silently.");
            try
            {
                using var existingShowEvent = EventWaitHandle.OpenExisting(ShowWindowEventName);
                existingShowEvent.Set();
            }
            catch (Exception ex)
            {
                Logger.Warn($"Could not signal existing instance: {ex.Message}");
            }

            Environment.Exit(0);
            return;
        }

        bool startMinimized = System.Environment.GetCommandLineArgs()
            .Any(a => string.Equals(a, "--tray", System.StringComparison.OrdinalIgnoreCase));

        _window = new MainWindow(startMinimized);
        _window.Activate();

        StartShowWindowListener();
        StartWatchdog();
    }

    private void StartShowWindowListener()
    {
        _showWindowEvent = new EventWaitHandle(initialState: false, EventResetMode.AutoReset, ShowWindowEventName);

        var listenerThread = new Thread(() =>
        {
            while (true)
            {
                _showWindowEvent.WaitOne();

                var dq = (_window as MainWindow)?.DispatcherQueue;
                dq?.TryEnqueue(() => (_window as MainWindow)?.BringToFront());
            }
        })
        {
            IsBackground = true,
            Name = "InstantReplay-ShowWindowListener",
        };
        listenerThread.Start();
    }

    private static void StartWatchdog()
    {
        int currentPid = Environment.ProcessId;
        string processName = Process.GetCurrentProcess().ProcessName;

        var watchdogThread = new Thread(() =>
        {
            while (true)
            {
                Thread.Sleep(WatchdogInterval);
                try
                {
                    var processes = Process.GetProcessesByName(processName);
                    if (processes.Length > 1)
                    {
                        Logger.Warn($"Watchdog detected {processes.Length} Instant Replay processes running; terminating {processes.Length - 1} extra instance(s).");
                    }

                    foreach (var proc in processes)
                    {
                        if (proc.Id != currentPid)
                        {
                            try
                            {
                                proc.Kill();
                                Logger.Warn($"Watchdog terminated duplicate Instant Replay process (PID {proc.Id}).");
                            }
                            catch (Exception ex)
                            {
                                Logger.Error($"Watchdog failed to terminate duplicate process (PID {proc.Id}).", ex);
                            }
                        }
                        proc.Dispose();
                    }
                }
                catch (Exception ex)
                {
                    Logger.Warn($"Watchdog check failed: {ex.Message}");
                }
            }
        })
        {
            IsBackground = true,
            Name = "InstantReplay-Watchdog",
        };
        watchdogThread.Start();
    }
}