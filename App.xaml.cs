using System.Diagnostics;
using Microsoft.UI.Xaml;

namespace InstantReplay;

public partial class App : Application
{
    private const string SingleInstanceMutexName = "Global\\InstantReplay_SingleInstance_Mutex";
    private const string ShowWindowEventName = "Global\\InstantReplay_ShowWindow_Event";

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
        _singleInstanceMutex = new Mutex(initiallyOwned: false, SingleInstanceMutexName);
        bool acquired;
        try { acquired = _singleInstanceMutex.WaitOne(0); }
        catch (AbandonedMutexException) { acquired = true; }

        if (!acquired)
        {
            Logger.Info("Another Instant Replay instance is already running; signaling it and exiting silently.");
            try
            {
                for (int attempt = 0; attempt < 40; attempt++)
                {
                    try
                    {
                        using var existingShowEvent = EventWaitHandle.OpenExisting(ShowWindowEventName);
                        existingShowEvent.Set();
                        break;
                    }
                    catch (WaitHandleCannotBeOpenedException) when (attempt < 39)
                    {
                        Thread.Sleep(50);
                    }
                }
            }
            catch (Exception ex)
            {
                Logger.Warn($"Could not signal existing instance: {ex.Message}");
            }

            Environment.Exit(0);
            return;
        }

        _showWindowEvent = new EventWaitHandle(initialState: false, EventResetMode.AutoReset, ShowWindowEventName);

        bool startMinimized = System.Environment.GetCommandLineArgs()
            .Any(a => string.Equals(a, "--tray", System.StringComparison.OrdinalIgnoreCase));

        _window = new MainWindow(startMinimized);
        _window.Activate();

        StartShowWindowListener();
    }

    private void StartShowWindowListener()
    {
        var listenerThread = new Thread(() =>
        {
            while (true)
            {
                _showWindowEvent!.WaitOne();

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

}
