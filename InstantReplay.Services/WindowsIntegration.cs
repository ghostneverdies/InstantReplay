using System;

namespace InstantReplay.Services
{

public static class WindowsTheme
{
    public static (byte R, byte G, byte B) GetAccentColor()
    {
        try
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.OpenSubKey(@"Software\Microsoft\Windows\DWM");
            if (key?.GetValue("AccentColor") is int argb)
            {
                uint v = unchecked((uint)argb);
                byte r = (byte)(v & 0xFF);
                byte g = (byte)((v >> 8) & 0xFF);
                byte b = (byte)((v >> 16) & 0xFF);
                return (r, g, b);
            }
        }
        catch (Exception ex)
        {
            Logger.Warn($"WindowsTheme: could not read system accent color, using default: {ex.Message}");
        }
        return (0x8B, 0x5F, 0xBF);
    }

    public static bool IsSystemDarkMode()
    {
        try
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.OpenSubKey(
                @"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
            if (key?.GetValue("AppsUseLightTheme") is int lightMode)
                return lightMode == 0;
        }
        catch (Exception ex)
        {
            Logger.Warn($"WindowsTheme: could not read system light/dark mode, defaulting to dark: {ex.Message}");
        }
        return true;
    }
}

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
        string exe = Compat.ProcessPath ?? "InstantReplay.exe";
        return $"\"{exe}\" --tray";
    }
}
}

namespace System.Runtime.CompilerServices
{
    internal static class IsExternalInit { }
}

namespace InstantReplay.Services
{
    public static class Compat
    {
        private static readonly System.Diagnostics.Stopwatch Clock = System.Diagnostics.Stopwatch.StartNew();

        public static long TickCount64 => Clock.ElapsedMilliseconds;

        public static string? ProcessPath
        {
            get
            {
                try
                {
                    using (var process = System.Diagnostics.Process.GetCurrentProcess())
                        return process.MainModule?.FileName;
                }
                catch { return null; }
            }
        }

        public static int Clamp(int value, int min, int max) => value < min ? min : value > max ? max : value;
        public static long Clamp(long value, long min, long max) => value < min ? min : value > max ? max : value;
        public static double Clamp(double value, double min, double max) => value < min ? min : value > max ? max : value;
        public static float Clamp(float value, float min, float max) => value < min ? min : value > max ? max : value;

        public static void MoveOverwrite(string source, string destination)
        {
            if (System.IO.File.Exists(destination)) System.IO.File.Replace(source, destination, null);
            else System.IO.File.Move(source, destination);
        }

        public static System.Threading.Tasks.Task WriteAllBytesAsync(string path, byte[] bytes) =>
            System.Threading.Tasks.Task.Run(() => System.IO.File.WriteAllBytes(path, bytes));

        public static string ToHexString(byte[] bytes) => BitConverter.ToString(bytes).Replace("-", "");

        public static byte[] FromHexString(string hex)
        {
            if (hex.Length % 2 != 0) throw new FormatException("Odd-length hex string.");
            var bytes = new byte[hex.Length / 2];
            for (int i = 0; i < bytes.Length; i++)
                bytes[i] = Convert.ToByte(hex.Substring(i * 2, 2), 16);
            return bytes;
        }

        public static bool FixedTimeEquals(byte[] left, byte[] right)
        {
            if (left.Length != right.Length) return false;
            int diff = 0;
            for (int i = 0; i < left.Length; i++) diff |= left[i] ^ right[i];
            return diff == 0;
        }
    }
}
