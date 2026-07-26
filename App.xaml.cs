using System;
using System.Diagnostics;
using System.Linq;
using System.Threading;
using Microsoft.UI.Xaml;

namespace InstantReplay;

public partial class App : Application
{
    // Single, well-known names for the mutex/event used to detect and coordinate with a second
    // launch attempt. "Global\" makes these visible across user sessions on the same machine.
    private const string SingleInstanceMutexName = "Global\\InstantReplay_SingleInstance_Mutex";
    private const string ShowWindowEventName = "Global\\InstantReplay_ShowWindow_Event";

    // How often the watchdog re-checks for stray duplicate processes.
    private static readonly TimeSpan WatchdogInterval = TimeSpan.FromSeconds(15);

    private Window? _window;
    private Mutex? _singleInstanceMutex;
    private EventWaitHandle? _showWindowEvent;

    public App()
    {
        InitializeComponent();
        UnhandledException += (_, e) =>
        {
            Logger.Warn($"Unhandled exception: {e.Exception}");
        };
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        // --- Single-instance guard -------------------------------------------------------
        // If another Instant Replay process already holds the mutex, this is a duplicate
        // launch (stale/duplicate startup registry entry, user double-clicking the exe while
        // it's already running in the tray, etc). Signal the existing instance to show its
        // window, then quietly exit instead of creating a second window.
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

    /// <summary>
    /// Listens for a second launch attempt signaling us (via a named event) and brings the
    /// existing window to front instead of the duplicate process creating its own window.
    /// </summary>
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

    /// <summary>
    /// Belt-and-suspenders safety net: even with the mutex guard above, a narrow race at startup
    /// (e.g. two processes launched within the same instant, before either claims the mutex) could
    /// theoretically let more than one instance slip through. This periodically checks for stray
    /// duplicate processes and terminates every extra one, keeping only the current process alive.
    /// </summary>
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
