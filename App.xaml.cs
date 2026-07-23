using System.Linq;
using Microsoft.UI.Xaml;

namespace InstantReplay;

public partial class App : Application
{
    private Window? _window;

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
        bool startMinimized = System.Environment.GetCommandLineArgs()
            .Any(a => string.Equals(a, "--tray", System.StringComparison.OrdinalIgnoreCase));

        _window = new MainWindow(startMinimized);
        _window.Activate();
    }
}
