using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;
using WinRT.Interop;
using InstantReplay.Services;

namespace InstantReplay.GUI.Helpers;

internal static class Chrome
{
    public static void ApplyBackdrop(Window window)
    {
        ApplyBackdrop(window, acrylic: false);
    }

    public static void ApplyAcrylicBackdrop(Window window)
    {
        ApplyBackdrop(window, acrylic: true);
    }

    private static void ApplyBackdrop(Window window, bool acrylic)
    {
        try
        {
            window.SystemBackdrop = acrylic
                ? new DesktopAcrylicBackdrop()
                : new MicaBackdrop();
        }
        catch (Exception ex)
        {
            Logger.Error($"Could not apply {(acrylic ? "Acrylic" : "Mica")} backdrop", ex);
            window.SystemBackdrop = null;
        }

        try
        {
            TitleBarButtons.SetCornerPreference(WindowNative.GetWindowHandle(window), round: true);
            TitleBarButtons.SetAccentBorder(WindowNative.GetWindowHandle(window));
        }
        catch (Exception ex)
        {
            Logger.Warn($"Could not set corner preference: {ex.Message}");
        }
    }
}
