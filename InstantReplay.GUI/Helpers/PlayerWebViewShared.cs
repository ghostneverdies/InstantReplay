using System;
using System.Threading.Tasks;
using System.Windows;

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
        var tcs = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        RoutedEventHandler handler = (_, _) => tcs.TrySetResult(true);
        element.Loaded += handler;
        try
        {
            await Task.WhenAny(tcs.Task, Task.Delay(timeout));
        }
        finally
        {
            element.Loaded -= handler;
        }
    }
}
