using System.Runtime.InteropServices;
using Microsoft.Win32;
using Microsoft.Windows.AppNotifications;
using Microsoft.Windows.AppNotifications.Builder;
using InstantReplay.Services;

namespace InstantReplay.GUI.Shell;

public static class ToastCenter
{
    public const string AumId = "InstantReplay.InstantReplay";

    private static readonly object s_lock = new();
    private static bool _registered;

    public static bool IsAvailable => _registered;

    public static bool EnsureRegistered()
    {
        lock (s_lock)
        {
            if (_registered) return true;
            try
            {
                EnsureShortcut();
                EnsureRegistryEntry();
                SetCurrentProcessExplicitAppUserModelID(AumId);
                AppNotificationManager.Default.Register();
                _registered = true;
                Logger.Info($"Toast notifications registered (AUMID {AumId}).");
            }
            catch (Exception ex)
            {
                _registered = false;
                Logger.Warn($"Toast registration failed, falling back to balloon: {ex.Message}");
            }
            return _registered;
        }
    }

    public static bool Show(string title, string message)
    {
        try
        {
            if (!EnsureRegistered()) return false;

            var builder = new AppNotificationBuilder().AddText(title).AddText(message);
            AppNotificationManager.Default.Show(builder.BuildNotification());
            return true;
        }
        catch (Exception ex)
        {
            Logger.Warn($"Toast show failed, falling back to balloon: {ex.Message}");
            return false;
        }
    }

    private static void EnsureShortcut()
    {
        string shortcutPath = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
            "Microsoft", "Windows", "Start Menu", "Programs", "Instant Replay.lnk");
        if (File.Exists(shortcutPath)) return;

        try
        {
            var link = (IShellLinkW)new CShellLink();
            link.SetPath(Environment.ProcessPath!);
            link.SetArguments("");
            link.SetWorkingDirectory(Path.GetDirectoryName(Environment.ProcessPath!)!);
            link.SetIconLocation(Environment.ProcessPath!, 0);

            var propertyStore = (IPropertyStore)link;
            var key = new PropertyKey(new Guid("9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3"), 5);
            using var value = PropVariant.FromString(AumId);
            propertyStore.SetValue(in key, in value);
            propertyStore.Commit();

            var persistFile = (IPersistFile)link;
            persistFile.Save(shortcutPath, false);
        }
        catch (Exception ex)
        {
            Logger.Warn($"Toast shortcut creation failed: {ex.Message}");
            throw;
        }
    }

    private static void EnsureRegistryEntry()
    {
        using var key = Registry.CurrentUser.CreateSubKey($@"Software\Classes\AppUserModelId\{AumId}");
        key.SetValue("DisplayName", "Instant Replay");
        key.SetValue("IconUri", Environment.ProcessPath!, RegistryValueKind.String);
    }

    [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
    private static extern int SetCurrentProcessExplicitAppUserModelID([MarshalAs(UnmanagedType.LPWStr)] string appId);

    [ComImport]
    [Guid("00021401-0000-0000-C000-000000000046")]
    private class CShellLink { }

    [ComImport]
    [Guid("000214F9-0000-0000-C000-000000000046")]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IShellLinkW
    {
        void GetPath([Out, MarshalAs(UnmanagedType.LPWStr)] System.Text.StringBuilder pszFile, int cch, IntPtr pfd, int fFlags);
        void GetIDList(out IntPtr ppidl);
        void SetIDList(IntPtr pidl);
        void GetDescription([Out, MarshalAs(UnmanagedType.LPWStr)] System.Text.StringBuilder pszName, int cch);
        void SetDescription([MarshalAs(UnmanagedType.LPWStr)] string pszName);
        void GetWorkingDirectory([Out, MarshalAs(UnmanagedType.LPWStr)] System.Text.StringBuilder pszDir, int cch);
        void SetWorkingDirectory([MarshalAs(UnmanagedType.LPWStr)] string pszDir);
        void GetArguments([Out, MarshalAs(UnmanagedType.LPWStr)] System.Text.StringBuilder pszArgs, int cch);
        void SetArguments([MarshalAs(UnmanagedType.LPWStr)] string pszArgs);
        void GetHotkey(out short pwHotkey);
        void SetHotkey(short wHotkey);
        void GetShowCmd(out int piShowCmd);
        void SetShowCmd(int iShowCmd);
        void GetIconLocation([Out, MarshalAs(UnmanagedType.LPWStr)] System.Text.StringBuilder pszIconPath, int cch, out int piIcon);
        void SetIconLocation([MarshalAs(UnmanagedType.LPWStr)] string pszIconPath, int iIcon);
        void SetRelativePath([MarshalAs(UnmanagedType.LPWStr)] string pszPathRel, int dwReserved);
        void Resolve(IntPtr hwnd, int fFlags);
        void SetPath([MarshalAs(UnmanagedType.LPWStr)] string pszFile);
    }

    [ComImport]
    [Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99")]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IPropertyStore
    {
        void GetCount(out uint cProps);
        void GetAt(uint iProp, out PropertyKey pkey);
        void GetValue(ref PropertyKey key, out PropVariant pv);
        void SetValue(in PropertyKey key, in PropVariant pv);
        void Commit();
    }

    [ComImport]
    [Guid("0000010B-0000-0000-C000-000000000046")]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IPersistFile
    {
        void GetClassID(out Guid pClassID);
        int IsDirty();
        void Load([MarshalAs(UnmanagedType.LPWStr)] string pszFileName, int dwMode);
        void Save([MarshalAs(UnmanagedType.LPWStr)] string pszFileName, bool fRemember);
        void SaveCompleted([MarshalAs(UnmanagedType.LPWStr)] string pszFileName);
        void GetCurFile([MarshalAs(UnmanagedType.LPWStr)] out string ppszFileName);
    }

    [StructLayout(LayoutKind.Sequential)]
    private readonly struct PropertyKey
    {
        public readonly Guid fmtid;
        public readonly uint pid;

        public PropertyKey(Guid fmtid, uint pid)
        {
            this.fmtid = fmtid;
            this.pid = pid;
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    private readonly struct PropVariant : IDisposable
    {
        private const ushort VT_LPWSTR = 31;

        public readonly ushort vt;
        public readonly ushort wReserved1;
        public readonly ushort wReserved2;
        public readonly ushort wReserved3;
        public readonly IntPtr pData;
        public readonly IntPtr rawValue2;
        public readonly IntPtr rawValue3;

        public static PropVariant FromString(string value)
        {
            return new PropVariant(VT_LPWSTR, Marshal.StringToCoTaskMemUni(value));
        }

        private PropVariant(ushort vt, IntPtr data)
        {
            this.vt = vt;
            wReserved1 = 0;
            wReserved2 = 0;
            wReserved3 = 0;
            pData = data;
            rawValue2 = IntPtr.Zero;
            rawValue3 = IntPtr.Zero;
        }

        public void Dispose()
        {
            if (vt == VT_LPWSTR && pData != IntPtr.Zero)
                Marshal.FreeCoTaskMem(pData);
        }
    }
}
