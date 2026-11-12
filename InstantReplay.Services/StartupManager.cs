namespace InstantReplay.Services;

public static class StartupManager
{
    private const string RunKeyPath = @"Software\Microsoft\Windows\CurrentVersion\Run";

    private const string ValueName = "Instant Replay";

    private const string LegacyValueName = "InstantReplay";

    public static bool IsEnabled()
    {
        CleanupLegacyKey();
        try
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.OpenSubKey(RunKeyPath, writable: false);
            return key?.GetValue(ValueName) is string existing &&
                   existing.Equals(BuildCommand(), StringComparison.OrdinalIgnoreCase);
        }
        catch (Exception ex)
        {
            Logger.Warn($"StartupManager.IsEnabled check failed: {ex.Message}");
            return false;
        }
    }

    public static void SetEnabled(bool enabled)
    {
        CleanupLegacyKey();
        try
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.OpenSubKey(RunKeyPath, writable: true)
                             ?? Microsoft.Win32.Registry.CurrentUser.CreateSubKey(RunKeyPath);
            if (enabled)
                key.SetValue(ValueName, BuildCommand());
            else
                key.DeleteValue(ValueName, throwOnMissingValue: false);
            Logger.Info($"Start with Windows {(enabled ? "enabled" : "disabled")}.");
        }
        catch (Exception ex)
        {
            Logger.Error("StartupManager.SetEnabled failed", ex);
        }
    }

    private static void CleanupLegacyKey()
    {
        try
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.OpenSubKey(RunKeyPath, writable: true);
            if (key?.GetValue(LegacyValueName) != null)
            {
                key.DeleteValue(LegacyValueName, throwOnMissingValue: false);
                Logger.Info($"Removed legacy duplicate startup registry value \"{LegacyValueName}\".");
            }
        }
        catch (Exception ex)
        {
            Logger.Warn($"StartupManager.CleanupLegacyKey failed: {ex.Message}");
        }
    }

    private static string BuildCommand()
    {
        string exe = Environment.ProcessPath ?? "InstantReplay.exe";
        return $"\"{exe}\" --tray";
    }
}
