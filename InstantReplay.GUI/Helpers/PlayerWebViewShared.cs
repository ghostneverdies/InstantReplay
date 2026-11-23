using Microsoft.Web.WebView2.Core;

namespace InstantReplay.GUI.Helpers;

internal static class PlayerWebViewShared
{
    public static string ToPlainJson(string message)
    {
        string s = message;
        if (s.Length >= 2 && s[0] == '"' && s[s.Length - 1] == '"')
        {
            try { s = System.Text.Json.JsonSerializer.Deserialize<string>(s) ?? s; } catch { }
        }
        return s;
    }

    public static async Task WaitForLoadedAsync(FrameworkElement element, TimeSpan timeout)
    {
        if (element.IsLoaded) return;

        // TaskCompletionSource (non-generic) does not exist on .NET Framework 4.8.
        var tcs = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        RoutedEventHandler handler = (_, _) => tcs.TrySetResult(true);
        element.Loaded += handler;
        try
        {
            _ = await Task.WhenAny(tcs.Task, Task.Delay(timeout));
        }
        finally
        {
            element.Loaded -= handler;
        }
    }

    public static bool IsRuntimeMissing(Exception ex)
    {
        string text = ex.ToString();
        return ex is WebView2RuntimeNotFoundException
            || text.IndexOf("WebView2", StringComparison.OrdinalIgnoreCase) >= 0
            || text.IndexOf("Edge WebView", StringComparison.OrdinalIgnoreCase) >= 0;
    }

    public static string DescribeLoadFailure(Exception ex)
    {
        return IsRuntimeMissing(ex)
        ? "The WebView2 runtime is not installed. Install it to play replays."
        : $"The player could not be opened: {ex.Message}";
    }

}
