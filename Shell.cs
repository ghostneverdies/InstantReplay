namespace InstantReplay
{
    using System;
    using System.Collections.Generic;
    using System.IO;
    using System.Runtime.InteropServices;
    using Microsoft.UI;
    using Microsoft.UI.Windowing;
    using Microsoft.UI.Xaml;
    using Microsoft.UI.Xaml.Controls;
    using Microsoft.UI.Xaml.Media;
    using Microsoft.Win32;
    using Microsoft.Windows.AppNotifications;
    using Microsoft.Windows.AppNotifications.Builder;
    using Windows.Graphics;
    using Windows.System;
    using Windows.UI;
    using WinRT.Interop;

    internal static class Chrome
    {
        public static void ApplyBackdrop(Window window, BackdropKind kind)
        {
            try
            {
                window.SystemBackdrop = kind switch
                {
                    BackdropKind.Mica => new MicaBackdrop(),
                    _ => new DesktopAcrylicBackdrop(),
                };
            }
            catch (Exception ex)
            {
                Logger.Error($"Could not apply {kind} backdrop", ex);
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

    public sealed class SettingRow : ContentControl
    {
        public static readonly DependencyProperty LabelProperty =
            DependencyProperty.Register(nameof(Label), typeof(string), typeof(SettingRow), new PropertyMetadata(string.Empty));

        public string Label
        {
            get => (string)GetValue(LabelProperty);
            set => SetValue(LabelProperty, value);
        }

        public SettingRow()
        {
            DefaultStyleKey = typeof(SettingRow);
        }
    }

    public static class HotkeyDisplay
    {
        private const uint VkOemComma = 0xBC, VkOemPeriod = 0xBE, VkOemQuestion = 0xBF,
                            VkOemSemicolon = 0xBA, VkOemQuotes = 0xDE, VkOemPlus = 0xBB, VkOemMinus = 0xBD,
                            VkMultiply = 0x6A, VkAdd = 0x6B, VkSubtract = 0x6D, VkDivide = 0x6F, VkDecimal = 0x6E,
                            VkNumPad0 = 0x60, VkNumPad9 = 0x69, VkD0 = 0x30, VkD9 = 0x39, VkSpace = 0x20;

        public static string Format(uint modifiers, uint vk)
        {
            var parts = new List<string>();
            if ((modifiers & MainWindow.ModControl) != 0) parts.Add("CTRL");
            if ((modifiers & MainWindow.ModAlt) != 0) parts.Add("ALT");
            if ((modifiers & MainWindow.ModShift) != 0) parts.Add("SHIFT");
            if ((modifiers & MainWindow.ModWin) != 0) parts.Add("WIN");

            if (vk != 0) parts.Add(VkName(vk));
            else if (parts.Count == 0) parts.Add("â€¦");

            return string.Join(" + ", parts);
        }

        public static string VkName(uint vk)
        {
            switch (vk)
            {
                case VkMultiply: return "*";
                case VkAdd: return "NUM +";
                case VkSubtract: return "NUM -";
                case VkDivide: return "NUM /";
                case VkDecimal: return "NUM .";
                case VkOemComma: return ",";
                case VkOemPeriod: return ".";
                case VkOemQuestion: return "/";
                case VkOemSemicolon: return ";";
                case VkOemQuotes: return "'";
                case VkOemPlus: return "+";
                case VkOemMinus: return "-";
                case VkSpace: return "SPACE";
            }
            if (vk is >= VkNumPad0 and <= VkNumPad9) return "NUM " + (vk - VkNumPad0);
            if (vk is >= VkD0 and <= VkD9) return (vk - VkD0).ToString();

            return ((VirtualKey)vk).ToString().ToUpperInvariant();
        }

        public static uint? ModifierBitForKey(VirtualKey key) => key switch
        {
            VirtualKey.Control or VirtualKey.LeftControl or VirtualKey.RightControl => MainWindow.ModControl,
            VirtualKey.Menu or VirtualKey.LeftMenu or VirtualKey.RightMenu => MainWindow.ModAlt,
            VirtualKey.Shift or VirtualKey.LeftShift or VirtualKey.RightShift => MainWindow.ModShift,
            VirtualKey.LeftWindows or VirtualKey.RightWindows => MainWindow.ModWin,
            _ => null,
        };
    }

    internal static class TitleBarButtons
    {
        private delegate IntPtr SubclassProc(IntPtr hWnd, uint uMsg, IntPtr wParam, IntPtr lParam, IntPtr uIdSubclass, IntPtr dwRefData);

        [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW", CharSet = CharSet.Unicode)]
        private static extern IntPtr GetWindowLongPtr(IntPtr hWnd, int nIndex);

        [DllImport("user32.dll", EntryPoint = "SetWindowLongPtrW", CharSet = CharSet.Unicode)]
        private static extern IntPtr SetWindowLongPtr(IntPtr hWnd, int nIndex, IntPtr dwNewLong);

        [DllImport("user32.dll")]
        private static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter, int x, int y, int cx, int cy, uint uFlags);

        [DllImport("comctl32.dll")]
        private static extern bool SetWindowSubclass(IntPtr hWnd, SubclassProc pfnSubclass, uint uIdSubclass, IntPtr dwRefData);

        [DllImport("comctl32.dll")]
        private static extern IntPtr DefSubclassProc(IntPtr hWnd, uint uMsg, IntPtr wParam, IntPtr lParam);

        [DllImport("dwmapi.dll")]
        private static extern int DwmSetWindowAttribute(IntPtr hwnd, int dwAttribute, ref int pvAttribute, int cbAttribute);

        private const int GwlStyle = -16;
        private const long WsCaption = 0x00C00000L;
        private const long WsThickFrame = 0x00040000L;
        private const long WsMinimizeBox = 0x00020000L;
        private const long WsMaximizeBox = 0x00010000L;
        private const long WsPopup = 0x80000000L;
        private const uint WmNcCalcSize = 0x0083;
        private const uint SwpNoSize = 0x0001;
        private const uint SwpNoMove = 0x0002;
        private const uint SwpNoZOrder = 0x0004;
        private const uint SwpNoActivate = 0x0010;
        private const uint SwpFrameChanged = 0x0020;

        private const int DwmwaTransitionsForceDisabled = 3;
        private const long WsSysMenu = 0x00080000L;
        private const int DwmwaWindowCornerPreference = 33;
        private const int DwmwcpRound = 2;
        private const int DwmwcpDoNotRound = 1;

        private static SubclassProc? _subclassProc;

        private static IntPtr WindowSubclass(IntPtr hWnd, uint uMsg, IntPtr wParam, IntPtr lParam, IntPtr uIdSubclass, IntPtr dwRefData)
        {
            if (uMsg == WmNcCalcSize)
                return IntPtr.Zero;
            return DefSubclassProc(hWnd, uMsg, wParam, lParam);
        }

        public static void InstallNoCaptionFrame(IntPtr hwnd)
        {
            try
            {
                int allowTransitions = 0;
                DwmSetWindowAttribute(hwnd, DwmwaTransitionsForceDisabled, ref allowTransitions, sizeof(int));

                long style = GetWindowLongPtr(hwnd, GwlStyle).ToInt64();
                long want = WsCaption | WsThickFrame | WsSysMenu;
                if ((style & want) != want)
                {
                    SetWindowLongPtr(hwnd, GwlStyle, new IntPtr(style | want));
                    SetWindowPos(hwnd, IntPtr.Zero, 0, 0, 0, 0, SwpNoSize | SwpNoMove | SwpNoZOrder | SwpNoActivate | SwpFrameChanged);
                }

                if (_subclassProc == null)
                {
                    _subclassProc = WindowSubclass;
                    SetWindowSubclass(hwnd, _subclassProc, 0, IntPtr.Zero);
                    SetWindowPos(hwnd, IntPtr.Zero, 0, 0, 0, 0, SwpNoSize | SwpNoMove | SwpNoZOrder | SwpNoActivate | SwpFrameChanged);
                }
            }
            catch (Exception ex)
            {
                Logger.Warn($"Could not install custom window frame: {ex.Message}");
            }
        }

        private const int DwmwaBorderColor = 34;

        public static void SetAccentBorder(IntPtr hwnd)
        {
            try
            {
                var accent = new Windows.UI.ViewManagement.UISettings()
                    .GetColorValue(Windows.UI.ViewManagement.UIColorType.Accent);
                int colorRef = accent.R | (accent.G << 8) | (accent.B << 16);
                DwmSetWindowAttribute(hwnd, DwmwaBorderColor, ref colorRef, sizeof(int));
            }
            catch (Exception ex)
            {
                Logger.Warn($"Could not set accent window border: {ex.Message}");
            }
        }

        public static void SetCornerPreference(IntPtr hwnd, bool round)
        {
            try
            {
                int preference = round ? DwmwcpRound : DwmwcpDoNotRound;
                DwmSetWindowAttribute(hwnd, DwmwaWindowCornerPreference, ref preference, sizeof(int));
            }
            catch (Exception ex)
            {
                Logger.Warn($"Could not set corner preference: {ex.Message}");
            }
        }

        public static void StripWindowFrame(IntPtr hwnd)
        {
            try
            {
                long style = GetWindowLongPtr(hwnd, GwlStyle).ToInt64();
                long next = (style & ~(WsCaption | WsThickFrame | WsSysMenu | WsMinimizeBox | WsMaximizeBox)) | WsPopup;
                SetWindowLongPtr(hwnd, GwlStyle, new IntPtr(next));

                int noBorder = unchecked((int)0xFFFFFFFE);
                DwmSetWindowAttribute(hwnd, DwmwaBorderColor, ref noBorder, sizeof(int));

                int corner = DwmwcpDoNotRound;
                DwmSetWindowAttribute(hwnd, DwmwaWindowCornerPreference, ref corner, sizeof(int));

                SetWindowPos(hwnd, IntPtr.Zero, 0, 0, 0, 0, SwpNoSize | SwpNoMove | SwpNoZOrder | SwpNoActivate | SwpFrameChanged);
            }
            catch (Exception ex)
            {
                Logger.Warn($"Could not strip window frame: {ex.Message}");
            }
        }

        public static void SetTopmost(IntPtr hwnd, bool topmost)
        {
            try
            {
                IntPtr insert = new IntPtr(topmost ? -1 : -2);
                SetWindowPos(hwnd, insert, 0, 0, 0, 0, SwpNoSize | SwpNoMove | SwpNoActivate);
            }
            catch (Exception ex)
            {
                Logger.Warn($"Could not set window z-order: {ex.Message}");
            }
        }

        public static void Apply(AppWindowTitleBar? tb, bool dark)
        {
            if (tb == null) return;

            tb.ButtonBackgroundColor = Color.FromArgb(0, 0, 0, 0);

            if (dark)
            {
                tb.ButtonForegroundColor = Color.FromArgb(255, 255, 255, 255);
                tb.ButtonHoverBackgroundColor = Color.FromArgb(255, 76, 76, 76);
                tb.ButtonHoverForegroundColor = Color.FromArgb(255, 255, 255, 255);
                tb.ButtonPressedBackgroundColor = Color.FromArgb(255, 64, 64, 64);
                tb.ButtonPressedForegroundColor = Color.FromArgb(255, 255, 255, 255);
                tb.InactiveBackgroundColor = Color.FromArgb(0, 0, 0, 0);
                tb.InactiveForegroundColor = Color.FromArgb(140, 255, 255, 255);
            }
            else
            {
                tb.ButtonForegroundColor = Color.FromArgb(255, 24, 24, 24);
                tb.ButtonHoverBackgroundColor = Color.FromArgb(255, 228, 228, 228);
                tb.ButtonHoverForegroundColor = Color.FromArgb(255, 0, 0, 0);
                tb.ButtonPressedBackgroundColor = Color.FromArgb(255, 214, 214, 214);
                tb.ButtonPressedForegroundColor = Color.FromArgb(255, 0, 0, 0);
                tb.InactiveBackgroundColor = Color.FromArgb(0, 0, 0, 0);
                tb.InactiveForegroundColor = Color.FromArgb(130, 0, 0, 0);
            }
        }
    }

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

    public enum BadgeKind
    {
        None,
        Recording,
        Paused,
        Error,
    }

    public static class BadgeGlyphs
    {
        private const uint ImageIconTrueColor = 0x00000002;
        private const uint DibRgbColors = 0x00;

        [StructLayout(LayoutKind.Sequential)]
        private struct BITMAPINFOHEADER
        {
            public int biSize, biWidth, biHeight;
            public short biPlanes, biBitCount;
            public int biCompression, biSizeImage;
            public int biXPelsPerMeter, biYPelsPerMeter, biClrUsed, biClrImportant;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct ICONINFO
        {
            public bool fIcon;
            public int xHotspot, yHotspot;
            public IntPtr hbmMask, hbmColor;
        }

        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        private static extern IntPtr LoadImageW(IntPtr hinst, string lpszName, uint uType, int cxDesired, int cyDesired, uint fuLoad);

        [DllImport("gdi32.dll", SetLastError = true)]
        private static extern IntPtr CreateCompatibleBitmap(IntPtr hdc, int cx, int cy);

        [DllImport("gdi32.dll", SetLastError = true)]
        private static extern IntPtr CreateCompatibleDC(IntPtr hdc);

        [DllImport("gdi32.dll")]
        private static extern bool DeleteDC(IntPtr hdc);

        [DllImport("gdi32.dll", SetLastError = true)]
        private static extern IntPtr CreateDIBSection(IntPtr hdc, ref BITMAPINFOHEADER pbmi, uint usage, out IntPtr ppvBits, IntPtr hSection, uint offset);

        [DllImport("gdi32.dll")]
        private static extern IntPtr SelectObject(IntPtr hdc, IntPtr hgdiobj);

        [DllImport("gdi32.dll")]
        private static extern bool DeleteObject(IntPtr hObject);

        [DllImport("gdi32.dll")]
        private static extern bool BitBlt(IntPtr hdcDest, int xDest, int yDest, int w, int h, IntPtr hdcSrc, int xSrc, int ySrc, int rop);

        [DllImport("gdi32.dll")]
        private static extern int SetBkMode(IntPtr hdc, int mode);

        [DllImport("gdi32.dll")]
        private static extern uint GdiFlush();

        [DllImport("user32.dll")]
        private static extern IntPtr GetDC(IntPtr hWnd);

        [DllImport("user32.dll")]
        private static extern int ReleaseDC(IntPtr hWnd, IntPtr hDC);

        [DllImport("user32.dll", SetLastError = true)]
        private static extern IntPtr CreateIconIndirect(ref ICONINFO piconinfo);

        [DllImport("user32.dll", SetLastError = true)]
        private static extern bool GetIconInfo(IntPtr hIcon, out ICONINFO piconinfo);

        [DllImport("user32.dll")]
        private static extern bool DestroyIcon(IntPtr hIcon);

        [DllImport("gdi32.dll")]
        private static extern IntPtr CreateFont(int cHeight, int cWidth, int cEscapement, int cOrientation,
            int cWeight, uint fdwItalic, uint fdwUnderline, uint fdwStrikeOut, uint iCharSet,
            uint fdwOutputPrecision, uint fdwClipPrecision, uint fdwQuality, uint fdwPitchAndFamily, string face);

        [DllImport("gdi32.dll", CharSet = CharSet.Unicode)]
        private static extern bool TextOutW(IntPtr hdc, int x, int y, string s, int c);

        [DllImport("gdi32.dll", CharSet = CharSet.Unicode)]
        private static extern bool GetTextExtentPoint32W(IntPtr hdc, string s, int c, out SIZE extent);

        [StructLayout(LayoutKind.Sequential)]
        private struct SIZE { public int CX, CY; }

        private const uint DefaultQuality = 0;
        private const uint ClipDefaultPrecision = 0;
        private const uint OutDefaultPrecision = 0;
        private const uint DefaultPitchAndFamily = 0;
        private const uint DefaultCharSet = 1;
        private const uint NoItalic = 0;
        private const uint NoUnderline = 0;
        private const uint NoStrikeOut = 0;
        private const int TransParentBk = 1;

        [DllImport("gdi32.dll")]
        private static extern IntPtr CreateBitmap(int nWidth, int nHeight, uint nPlanes, uint nBitCount, IntPtr lpBits);

        [DllImport("gdi32.dll")]
        private static extern int GetDIBits(IntPtr hdc, IntPtr hbm, uint start, uint cLines, IntPtr lpvBits, ref BITMAPINFOHEADER lpbmi, uint usage);

        private const uint AntialiasedQuality = 4;

        private static BITMAPINFOHEADER TopDownBgra(int width, int height) => new()
        {
            biSize = Marshal.SizeOf<BITMAPINFOHEADER>(),
            biWidth = width,
            biHeight = -height,
            biPlanes = 1,
            biBitCount = 32,
            biCompression = 0,
        };

        public static bool TryRenderGlyphBgra(int codepoint, Color color, int size, out byte[] bgra)
        {
            bgra = new byte[Math.Max(size, 0) * Math.Max(size, 0) * 4];
            if (codepoint == 0 || size <= 0) return false;

            foreach (string face in new[] { "Segoe Fluent Icons", "Segoe MDL2 Assets" })
            {
                if (TryRenderGlyphWithFace(face, codepoint, color, size, bgra)) return true;
            }
            return false;
        }

        private static bool TryRenderGlyphWithFace(string face, int codepoint, Color color, int size, byte[] bgra)
        {
            IntPtr screen = GetDC(IntPtr.Zero);
            IntPtr dc = IntPtr.Zero, dib = IntPtr.Zero, font = IntPtr.Zero;
            IntPtr oldBmp = IntPtr.Zero, oldFont = IntPtr.Zero;

            try
            {
                dc = CreateCompatibleDC(screen);
                BITMAPINFOHEADER bi = TopDownBgra(size, size);
                dib = CreateDIBSection(screen, ref bi, DibRgbColors, out IntPtr bits, IntPtr.Zero, 0);
                if (dc == IntPtr.Zero || dib == IntPtr.Zero || bits == IntPtr.Zero) return false;

                oldBmp = SelectObject(dc, dib);

                var white = new byte[size * size * 4];
                Array.Fill(white, (byte)0xFF);
                Marshal.Copy(white, 0, bits, white.Length);

                font = CreateFont(-size, 0, 0, 0, 400, NoItalic, NoUnderline, NoStrikeOut,
                    DefaultCharSet, OutDefaultPrecision, ClipDefaultPrecision, AntialiasedQuality, DefaultPitchAndFamily, face);
                if (font == IntPtr.Zero) return false;

                oldFont = SelectObject(dc, font);
                SetBkMode(dc, TransParentBk);

                string glyph = char.ConvertFromUtf32(codepoint);
                if (!GetTextExtentPoint32W(dc, glyph, glyph.Length, out SIZE extent)) return false;
                if (!TextOutW(dc, (size - extent.CX) / 2, (size - extent.CY) / 2, glyph, glyph.Length)) return false;
                GdiFlush();

                var raw = new byte[size * size * 4];
                Marshal.Copy(bits, raw, 0, raw.Length);

                bool anyInk = false;
                for (int k = 0; k < size * size; k++)
                {
                    byte lum = Math.Min(raw[k * 4], Math.Min(raw[k * 4 + 1], raw[k * 4 + 2]));
                    byte alpha = (byte)(255 - lum);
                    if (alpha > 8) anyInk = true;
                    bgra[k * 4 + 0] = color.B;
                    bgra[k * 4 + 1] = color.G;
                    bgra[k * 4 + 2] = color.R;
                    bgra[k * 4 + 3] = alpha;
                }
                return anyInk;
            }
            catch (Exception ex)
            {
                Logger.Warn($"Glyph render failed ({face}): {ex.Message}");
                return false;
            }
            finally
            {
                if (oldFont != IntPtr.Zero && dc != IntPtr.Zero) SelectObject(dc, oldFont);
                if (oldBmp != IntPtr.Zero && dc != IntPtr.Zero) SelectObject(dc, oldBmp);
                if (font != IntPtr.Zero) DeleteObject(font);
                if (dib != IntPtr.Zero) DeleteObject(dib);
                if (dc != IntPtr.Zero) DeleteDC(dc);
                if (screen != IntPtr.Zero) ReleaseDC(IntPtr.Zero, screen);
            }
        }

        private static IntPtr CreateIconFromBgra(byte[] bgra, int width, int height)
        {
            IntPtr screen = GetDC(IntPtr.Zero);
            IntPtr dib = IntPtr.Zero, mask = IntPtr.Zero;
            GCHandle maskPin = default;

            try
            {
                BITMAPINFOHEADER bi = TopDownBgra(width, height);
                dib = CreateDIBSection(screen, ref bi, DibRgbColors, out IntPtr bits, IntPtr.Zero, 0);
                if (dib == IntPtr.Zero || bits == IntPtr.Zero) return IntPtr.Zero;
                Marshal.Copy(bgra, 0, bits, width * height * 4);

                var maskBits = new byte[((width + 15) / 16) * 2 * height];
                maskPin = GCHandle.Alloc(maskBits, GCHandleType.Pinned);
                mask = CreateBitmap(width, height, 1, 1, maskPin.AddrOfPinnedObject());
                if (mask == IntPtr.Zero) return IntPtr.Zero;

                var ii = new ICONINFO { fIcon = true, xHotspot = 0, yHotspot = 0, hbmMask = mask, hbmColor = dib };
                return CreateIconIndirect(ref ii);
            }
            catch (Exception ex)
            {
                Logger.Warn($"CreateIconFromBgra failed: {ex.Message}");
                return IntPtr.Zero;
            }
            finally
            {
                if (maskPin.IsAllocated) maskPin.Free();
                if (mask != IntPtr.Zero) DeleteObject(mask);
                if (dib != IntPtr.Zero) DeleteObject(dib);
                if (screen != IntPtr.Zero) ReleaseDC(IntPtr.Zero, screen);
            }
        }

        private static IntPtr CreateGlyphIconFromFile(string path, int size)
        {
            IntPtr icon = LoadImageW(IntPtr.Zero, path, ImageIconTrueColor, size, size, 0x10);
            return icon;
        }

        public static IntPtr LoadAppIcon(int size)
        {
            string[] candidates =
            {
                Path.Combine(AppContext.BaseDirectory, "assets", "icons", "icon.ico"),
                Path.Combine(AppContext.BaseDirectory, "assets", "icon.ico"),
                Path.Combine(AppContext.BaseDirectory, "icon.ico"),
            };

            foreach (string candidate in candidates)
            {
                if (!File.Exists(candidate)) continue;
                IntPtr icon = CreateGlyphIconFromFile(candidate, size);
                if (icon != IntPtr.Zero) return icon;
            }

            return CreateGlyphIconFromFile("ms-appx:///assets/icons/icon.ico", size);
        }

        private static void PutPixel(byte[] bgra, int size, int x, int y, byte r, byte g, byte b, byte a)
        {
            if (x < 0 || y < 0 || x >= size || y >= size) return;
            int o = (y * size + x) * 4;
            bgra[o] = b;
            bgra[o + 1] = g;
            bgra[o + 2] = r;
            bgra[o + 3] = a;
        }

        private static void FillCircleAa(byte[] bgra, int size, Color color)
        {
            double c = size / 2.0;
            double radius = size / 2.0 - 0.5;
            for (int y = 0; y < size; y++)
                for (int x = 0; x < size; x++)
                {
                    double d = Math.Sqrt((x + 0.5 - c) * (x + 0.5 - c) + (y + 0.5 - c) * (y + 0.5 - c));
                    double coverage = Math.Clamp(radius - d + 0.5, 0, 1);
                    if (coverage > 0) PutPixel(bgra, size, x, y, color.R, color.G, color.B, (byte)Math.Round(coverage * 255));
                }
        }

        private static void FillRect(byte[] bgra, int size, int x0, int y0, int w, int h, byte r, byte g, byte b)
        {
            for (int y = y0; y < y0 + h; y++)
                for (int x = x0; x < x0 + w; x++)
                    PutPixel(bgra, size, x, y, r, g, b, 255);
        }

        private static byte[] RenderBadgeBgra(int glyphCode, Color color, BadgeKind kind, int size)
        {
            var bgra = new byte[size * size * 4];
            FillCircleAa(bgra, size, color);

            int glyphSize = Math.Max(6, (int)Math.Round(size * 0.62));
            int offset = (size - glyphSize) / 2;
            if (TryRenderGlyphBgra(glyphCode, Color.FromArgb(255, 255, 255, 255), glyphSize, out byte[] glyph))
            {
                OverlayBgra(bgra, size, glyph, glyphSize, offset, offset);
            }
            else
            {
                Logger.Info($"Badge glyph U+{glyphCode:X4} unavailable; drawing a simple badge shape instead.");
                if (kind == BadgeKind.Error)
                {
                    int stem = Math.Max(1, size / 8);
                    int stemHeight = Math.Max(2, size * 2 / 5);
                    FillRect(bgra, size, size / 2 - stem / 2, size / 2 - stemHeight / 2 - stem / 2, stem, stemHeight, 0xFF, 0xFF, 0xFF);
                    FillRect(bgra, size, size / 2 - stem / 2, size / 2 + stemHeight / 2 + stem / 2, stem, stem, 0xFF, 0xFF, 0xFF);
                }
                else
                {
                    FillRect(bgra, size, size / 2 - size / 5, size / 2 - size / 6, size * 2 / 5, size / 3, 0xFF, 0xFF, 0xFF);
                }
            }
            return bgra;
        }

        public static IntPtr CreateOverlayIcon(int glyphCode, Color color, BadgeKind kind, int size)
        {
            try
            {
                return CreateIconFromBgra(RenderBadgeBgra(glyphCode, color, kind, size), size, size);
            }
            catch (Exception ex)
            {
                Logger.Warn($"Overlay icon creation failed: {ex.Message}");
                return IntPtr.Zero;
            }
        }

        public static IntPtr CreateBadgeIcon(int glyphCode, Color color, BadgeKind kind, int size)
        {
            IntPtr appIcon = LoadAppIcon(size);
            if (appIcon == IntPtr.Zero) return IntPtr.Zero;

            try
            {
                if (!TryReadIconBgra(appIcon, size, out byte[] baseBgra)) return IntPtr.Zero;

                int badgeSize = Math.Max(8, (int)Math.Round(size * 0.62));
                byte[] badgeBgra = RenderBadgeBgra(glyphCode, color, kind, badgeSize);

                OverlayBgra(baseBgra, size, badgeBgra, badgeSize, size - badgeSize, size - badgeSize);
                return CreateIconFromBgra(baseBgra, size, size);
            }
            catch (Exception ex)
            {
                Logger.Warn($"Badge composition failed: {ex.Message}");
                return IntPtr.Zero;
            }
            finally
            {
                DestroyIcon(appIcon);
            }
        }

        private static void OverlayBgra(byte[] baseBgra, int baseSize, byte[] badgeBgra, int badgeSize, int offsetX, int offsetY)
        {
            for (int y = 0; y < badgeSize; y++)
            {
                for (int x = 0; x < badgeSize; x++)
                {
                    int src = y * badgeSize + x;
                    byte a = badgeBgra[src * 4 + 3];
                    if (a == 0) continue;

                    int dx = offsetX + x;
                    int dy = offsetY + y;
                    if (dx < 0 || dy < 0 || dx >= baseSize || dy >= baseSize) continue;

                    int dst = (dy * baseSize + dx) * 4;
                    float t = a / 255f;
                    byte baseAlpha = baseBgra[dst + 3];
                    baseBgra[dst + 0] = (byte)(badgeBgra[src * 4 + 0] * t + baseBgra[dst + 0] * (1 - t));
                    baseBgra[dst + 1] = (byte)(badgeBgra[src * 4 + 1] * t + baseBgra[dst + 1] * (1 - t));
                    baseBgra[dst + 2] = (byte)(badgeBgra[src * 4 + 2] * t + baseBgra[dst + 2] * (1 - t));
                    baseBgra[dst + 3] = (byte)Math.Min(255, a + baseAlpha * (1 - t));
                }
            }
        }

        private static bool TryReadIconBgra(IntPtr icon, int size, out byte[] bgra)
        {
            bgra = new byte[size * size * 4];
            if (!GetIconInfo(icon, out ICONINFO info)) return false;

            IntPtr screen = GetDC(IntPtr.Zero);
            IntPtr buffer = IntPtr.Zero;
            try
            {
                if (info.hbmColor == IntPtr.Zero) return false;

                BITMAPINFOHEADER bi = TopDownBgra(size, size);
                buffer = Marshal.AllocHGlobal(bgra.Length);
                if (GetDIBits(screen, info.hbmColor, 0, (uint)size, buffer, ref bi, DibRgbColors) == 0) return false;
                Marshal.Copy(buffer, bgra, 0, bgra.Length);

                bool anyAlpha = false;
                for (int k = 3; k < bgra.Length; k += 4)
                {
                    if (bgra[k] != 0) { anyAlpha = true; break; }
                }
                if (!anyAlpha)
                {
                    for (int k = 3; k < bgra.Length; k += 4) bgra[k] = 255;
                }
                return true;
            }
            catch (Exception ex)
            {
                Logger.Warn($"Reading the app icon failed: {ex.Message}");
                return false;
            }
            finally
            {
                if (buffer != IntPtr.Zero) Marshal.FreeHGlobal(buffer);
                if (info.hbmColor != IntPtr.Zero) DeleteObject(info.hbmColor);
                if (info.hbmMask != IntPtr.Zero) DeleteObject(info.hbmMask);
                if (screen != IntPtr.Zero) ReleaseDC(IntPtr.Zero, screen);
            }
        }
    }

    public sealed class TaskbarBadge : IDisposable
    {
        [ComImport]
        [Guid("56FDF344-FD6D-11d0-958A-006097C9A090")]
        private class TaskbarInstance
        {
        }

        [ComImport]
        [Guid("ea1afb91-9e28-4b86-90e9-9e9f8a5eefaf")]
        [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
        private interface ITaskbarList3
        {
            void HrInit();
            void AddTab(IntPtr hwnd);
            void DeleteTab(IntPtr hwnd);
            void ActivateTab(IntPtr hwnd);
            void SetActiveAlt(IntPtr hwnd);
            void MarkFullscreenWindow(IntPtr hwnd, [MarshalAs(UnmanagedType.Bool)] bool fullscreen);
            void SetProgressValue(IntPtr hwnd, ulong completed, ulong total);
            void SetProgressState(IntPtr hwnd, int flags);
            void RegisterTab(IntPtr hwnd, IntPtr tab);
            void UnregisterTab(IntPtr hwnd);
            void SetTabOrder(IntPtr hwndTab, IntPtr hwndInsertBefore);
            void SetTabActive(IntPtr hwndTab, IntPtr hwndMDI, uint dwReserved);
            void ThumbBarAddButtons(IntPtr hwnd, uint cButtons, IntPtr pButton);
            void ThumbBarUpdateButtons(IntPtr hwnd, uint cButtons, IntPtr pButton);
            void ThumbBarSetImageList(IntPtr hwnd, IntPtr himl);
            void SetOverlayIcon(IntPtr hwnd, IntPtr icon, [MarshalAs(UnmanagedType.LPWStr)] string? description);
            void SetThumbnailTooltip(IntPtr hwnd, [MarshalAs(UnmanagedType.LPWStr)] string? tip);
            void SetThumbnailClip(IntPtr hwnd, IntPtr rect);
        }

        [DllImport("user32.dll", SetLastError = true)]
        private static extern bool FlashWindowEx(IntPtr hwnd, ref FLASHWINFO pwfi);

        [StructLayout(LayoutKind.Sequential)]
        private struct FLASHWINFO
        {
            public int cbSize;
            public uint dwFlags;
            public uint uCount;
            public uint dwTimeout;
        }

        private const uint FlashwTray = 0x00000002;
        private const uint FlashwTimer = 0x00000004;
        private const uint FlashwStop = 0x00000000;

        private readonly IntPtr _hwnd;
        private ITaskbarList3? _taskbar;
        private BadgeKind _kind = BadgeKind.None;
        private bool _taskbarVisible = true;

        public TaskbarBadge(IntPtr hwnd)
        {
            _hwnd = hwnd;
            try
            {
                var instance = (ITaskbarList3)new TaskbarInstance();
                instance.HrInit();
                _taskbar = instance;
            }
            catch (Exception ex)
            {
                Logger.Warn($"TaskbarList3 unavailable: {ex.Message}");
            }
        }

        public void SetTaskbarVisible(bool visible)
        {
            if (_taskbarVisible == visible) return;
            _taskbarVisible = visible;
            if (!visible) StopFlashing();
        }

        public void SetOverlay(IntPtr icon, BadgeKind kind)
        {
            if (_kind == kind) return;

            if (_taskbar == null || !_taskbarVisible) return;

            _kind = kind;

            try
            {
                _taskbar.SetOverlayIcon(_hwnd, icon, kind switch
                {
                    BadgeKind.Recording => "Recording",
                    BadgeKind.Paused => "Not recording",
                    BadgeKind.Error => "Something went wrong",
                    _ => null,
                });
            }
            catch (Exception ex)
            {
                Logger.Warn($"SetOverlayIcon failed: {ex.Message}");
            }
        }

        public void Clear() => SetOverlay(IntPtr.Zero, BadgeKind.None);

        public void Reset() => _kind = BadgeKind.None;

        public void Flash()
        {
            if (_taskbar == null || !_taskbarVisible) return;

            try
            {
                var info = new FLASHWINFO
                {
                    cbSize = Marshal.SizeOf<FLASHWINFO>(),
                    dwFlags = (int)(FlashwTray | FlashwTimer),
                    uCount = uint.MaxValue,
                    dwTimeout = 0,
                };
                FlashWindowEx(_hwnd, ref info);
            }
            catch (Exception ex)
            {
                Logger.Warn($"FlashWindowEx failed: {ex.Message}");
            }
        }

        public void StopFlashing()
        {
            if (_taskbar == null) return;
            try
            {
                var info = new FLASHWINFO
                {
                    cbSize = Marshal.SizeOf<FLASHWINFO>(),
                    dwFlags = (int)FlashwStop,
                };
                FlashWindowEx(_hwnd, ref info);
            }
            catch (Exception ex)
            {
                Logger.Warn($"FlashWindowEx stop failed: {ex.Message}");
            }
        }

        public void Dispose()
        {
            try
            {
                _taskbar?.SetOverlayIcon(_hwnd, IntPtr.Zero, null);
            }
            catch (Exception ex)
            {
                Logger.Warn($"Taskbar overlay cleanup failed: {ex.Message}");
            }
            if (_taskbar != null && Marshal.IsComObject(_taskbar)) Marshal.ReleaseComObject(_taskbar);
            _taskbar = null;
        }
    }

    public sealed class TrayIcon : IDisposable
    {
        [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
        private struct NOTIFYICONDATA
        {
            public int cbSize;
            public IntPtr hWnd;
            public int uID;
            public int uFlags;
            public int uCallbackMessage;
            public IntPtr hIcon;
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string szTip;
            public int dwState;
            public int dwStateMask;
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)] public string szInfo;
            public int uTimeoutOrVersion;
            [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 64)] public string szInfoTitle;
            public int dwInfoFlags;
            public IntPtr hBalloonIcon;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct POINT { public int X; public int Y; }

        private const int NimAdd = 0, NimModify = 1, NimDelete = 2, NimSetVersion = 4;
        private const int NifMessage = 0x1, NifIcon = 0x2, NifTip = 0x4, NifInfo = 0x10;
        private const uint NotifyIconVersion4 = 4;

        public const int IconInfo = 0x1, IconWarning = 0x2, IconError = 0x3;

        public const uint WM_TRAYICON = 0x8001;
        private const int WM_LBUTTONDBLCLK = 0x0203;
        private const int WM_RBUTTONUP = 0x0205;
        private const int WM_CONTEXTMENU = 0x007B;
        private const int NIN_BALLOONUSERCLICK = 0x0405;

        private const uint MfString = 0x0, MfSeparator = 0x800, MfChecked = 0x08, MfGrayed = 0x01;
        private const uint TpmRightButton = 0x0002, TpmReturnCmd = 0x0100;

        [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
        private static extern bool Shell_NotifyIcon(int dwMessage, ref NOTIFYICONDATA data);

        [DllImport("kernel32.dll")]
        private static extern uint GetLastError();

        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        private static extern IntPtr LoadImageW(IntPtr hinst, string lpszName, uint uType, int cxDesired, int cyDesired, uint fuLoad);

        [DllImport("user32.dll")] private static extern bool GetCursorPos(out POINT lpPoint);
        [DllImport("user32.dll")] private static extern IntPtr CreatePopupMenu();
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern bool AppendMenuW(IntPtr hMenu, uint uFlags, uint uIDNewItem, string lpNewItem);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern bool SetForegroundWindow(IntPtr hWnd);
        [DllImport("user32.dll")] private static extern int TrackPopupMenuEx(IntPtr hMenu, uint uFlags, int x, int y, IntPtr hWnd, IntPtr lptpm);
        [DllImport("user32.dll")] private static extern bool DestroyMenu(IntPtr hMenu);
        [DllImport("user32.dll")] private static extern bool PostMessage(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);

        private readonly IntPtr _hwnd;
        private const int Uid = 1;
        private readonly IntPtr _hIcon;

        public event Action? DoubleClicked;
        public event Action? BalloonClicked;

        public Func<List<TrayMenuItem>>? MenuItemsProvider { get; set; }

        public TrayIcon(IntPtr hwnd, string iconPath, string tooltip)
        {
            _hwnd = hwnd;
            _hIcon = LoadImageW(IntPtr.Zero, iconPath, 2, 16, 16, 0x10);

            var data = new NOTIFYICONDATA
            {
                cbSize = Marshal.SizeOf<NOTIFYICONDATA>(),
                hWnd = hwnd,
                uID = Uid,
                uFlags = NifMessage | NifIcon | NifTip,
                uCallbackMessage = (int)WM_TRAYICON,
                hIcon = _hIcon,
                szTip = tooltip,
            };
            if (!Shell_NotifyIcon(NimAdd, ref data))
                Logger.Warn($"Tray icon add failed (last error 0x{GetLastError():X8}).");
            else
                Logger.Info("Tray icon added.");

            var version = new NOTIFYICONDATA
            {
                cbSize = Marshal.SizeOf<NOTIFYICONDATA>(),
                hWnd = hwnd,
                uID = Uid,
                uTimeoutOrVersion = (int)NotifyIconVersion4,
            };
            if (!Shell_NotifyIcon(NimSetVersion, ref version))
                Logger.Warn($"Tray icon version negotiation failed (last error 0x{GetLastError():X8}).");
        }

        public void SetIcon(IntPtr icon)
        {
            IntPtr handle = icon == IntPtr.Zero ? _hIcon : icon;
            if (handle == IntPtr.Zero) return;

            var data = new NOTIFYICONDATA
            {
                cbSize = Marshal.SizeOf<NOTIFYICONDATA>(),
                hWnd = _hwnd,
                uID = Uid,
                uFlags = NifIcon,
                hIcon = handle,
            };
            if (!Shell_NotifyIcon(NimModify, ref data))
                Logger.Warn($"Tray icon update failed (last error 0x{GetLastError():X8}).");
        }

        public void ShowBalloonTip(string title, string message, int iconFlag)
        {
            var data = new NOTIFYICONDATA
            {
                cbSize = Marshal.SizeOf<NOTIFYICONDATA>(),
                hWnd = _hwnd,
                uID = Uid,
                uFlags = NifInfo,
                szInfo = message,
                szInfoTitle = title,
                dwInfoFlags = iconFlag,
            };
            if (!Shell_NotifyIcon(NimModify, ref data))
                Logger.Warn($"Balloon modify failed (last error 0x{GetLastError():X8}).");
        }

        public void HandleMessage(IntPtr lParam)
        {
            int evt = (int)lParam & 0xFFFF;
            if (evt == WM_LBUTTONDBLCLK) DoubleClicked?.Invoke();
            else if (evt == NIN_BALLOONUSERCLICK) BalloonClicked?.Invoke();
            else if (evt == WM_RBUTTONUP || evt == WM_CONTEXTMENU) ShowContextMenu();
        }

        private void ShowContextMenu()
        {
            var items = MenuItemsProvider?.Invoke();
            if (items == null || items.Count == 0) return;

            IntPtr menu = CreatePopupMenu();
            var actions = new Dictionary<uint, Action?>();
            uint id = 1000;
            foreach (var item in items)
            {
                if (item.IsSeparator) { AppendMenuW(menu, MfSeparator, 0, ""); continue; }
                uint flags = MfString | (item.IsChecked ? MfChecked : 0) | (item.IsEnabled ? 0 : MfGrayed);
                AppendMenuW(menu, flags, id, item.Text);
                actions[id] = item.OnClick;
                id++;
            }

            GetCursorPos(out var pt);
            SetForegroundWindow(_hwnd);
            int cmd = TrackPopupMenuEx(menu, TpmRightButton | TpmReturnCmd, pt.X, pt.Y, _hwnd, IntPtr.Zero);
            PostMessage(_hwnd, 0, IntPtr.Zero, IntPtr.Zero);
            DestroyMenu(menu);

            if (cmd != 0 && actions.TryGetValue((uint)cmd, out var action)) action?.Invoke();
        }

        public void Dispose()
        {
            var data = new NOTIFYICONDATA { cbSize = Marshal.SizeOf<NOTIFYICONDATA>(), hWnd = _hwnd, uID = Uid };
            Shell_NotifyIcon(NimDelete, ref data);
        }
    }

    public sealed class TrayMenuItem
    {
        public string Text { get; init; } = "";
        public Action? OnClick { get; init; }
        public bool IsSeparator { get; init; }
        public bool IsChecked { get; init; }
        public bool IsEnabled { get; init; } = true;

        public static TrayMenuItem Separator() => new() { IsSeparator = true };
    }

    public sealed class DonateWindow : Window
    {
        [StructLayout(LayoutKind.Sequential)]
        private struct Rect
        {
            public int Left, Top, Right, Bottom;
        }

        [DllImport("user32.dll")] private static extern bool GetWindowRect(IntPtr hWnd, out Rect rect);
        [DllImport("user32.dll")] private static extern uint GetDpiForWindow(IntPtr hWnd);
        [DllImport("user32.dll")] private static extern int GetSystemMetrics(int nIndex);

        private const int SmCyFullscreen = 17;
        private const double WindowWidthDips = 480;
        private const double WindowHeightDips = 540;

        private static DonateWindow? _instance;

        public static void Show(IntPtr parentHwnd, Settings settings)
        {
            if (_instance != null)
            {
                try { _instance.Activate(); } catch { }
                return;
            }

            _instance = new DonateWindow(parentHwnd, settings);
            _instance.Closed += (_, _) => _instance = null;
            _instance.DispatcherQueue.TryEnqueue(() =>
            {
                try { _instance?.Activate(); }
                catch { }
            });
        }

        private DonateWindow(IntPtr parentHwnd, Settings settings)
        {
            Title = "Support Instant Replay";

            var view = new DonationView
            {
                RequestedTheme = settings.DarkMode ? ElementTheme.Dark : ElementTheme.Light,
            };
            view.CloseRequested += (_, _) => CloseWithFade(view);
            Content = view;

            Chrome.ApplyBackdrop(this, settings.Backdrop);

            IntPtr hwnd = WindowNative.GetWindowHandle(this);
            double scale = GetDpiForWindow(hwnd) / 96.0;
            if (scale <= 0) scale = 1.0;

            var appWindow = AppWindow.GetFromWindowId(Win32Interop.GetWindowIdFromWindow(hwnd));

            try { appWindow.SetIcon(ResolveIconPath()); }
            catch (Exception ex) { Logger.Warn($"Could not set donate window icon: {ex.Message}"); }

            if (appWindow.Presenter is OverlappedPresenter presenter)
            {
                presenter.IsResizable = false;
                presenter.IsMaximizable = false;
                presenter.IsMinimizable = false;
            }

            PositionOverParent(appWindow, parentHwnd, scale);
            SetupCustomTitleBar(appWindow, hwnd);
            TitleBarButtons.Apply(appWindow.TitleBar, settings.DarkMode);
        }

        private void CloseWithFade(DonationView view)
        {
            try
            {
                var appWindow = AppWindow.GetFromWindowId(
                    Win32Interop.GetWindowIdFromWindow(WindowNative.GetWindowHandle(this)));
                appWindow.Hide();
            }
            catch (Exception ex)
            {
                Logger.Warn($"Could not hide donate window before closing: {ex.Message}");
            }

            DispatcherQueue.TryEnqueue(() =>
            {
                try { Close(); }
                catch (Exception ex) { Logger.Warn($"Could not close donate window: {ex.Message}"); }
            });
        }

        private static void SetupCustomTitleBar(AppWindow appWindow, IntPtr hwnd)
        {
            var tb = appWindow.TitleBar;
            if (tb == null) return;

            try { tb.ExtendsContentIntoTitleBar = true; }
            catch (Exception ex) { Logger.Warn($"Could not extend donate title bar: {ex.Message}"); return; }

            try { tb.PreferredHeightOption = TitleBarHeightOption.Collapsed; }
            catch (Exception ex) { Logger.Warn($"Could not collapse donate title bar: {ex.Message}"); }

            double scale = GetDpiForWindow(hwnd) / 96.0;
            if (scale <= 0) scale = 1.0;

            int stripPx = (int)Math.Round((tb.Height > 0 ? tb.Height : 32) * scale);
            int captionPx = (int)Math.Round(46.0 * scale);
            int dragW = appWindow.ClientSize.Width - captionPx;

            try
            {
                if (dragW > 0 && stripPx > 0)
                    tb.SetDragRectangles(new[] { new RectInt32(0, 0, dragW, stripPx) });
            }
            catch (Exception ex)
            {
                Logger.Warn($"Could not set donate title bar drag region: {ex.Message}");
            }

            TitleBarButtons.InstallNoCaptionFrame(hwnd);
        }

        private static void PositionOverParent(AppWindow appWindow, IntPtr parentHwnd, double scale)
        {
            int width = (int)Math.Round(WindowWidthDips * scale);
            int height = (int)Math.Round(WindowHeightDips * scale);
            int workH = GetSystemMetrics(SmCyFullscreen);
            if (height > workH - 12) height = Math.Max(480, workH - 12);

            int x = 0, y = 0;
            if (GetWindowRect(parentHwnd, out Rect parent))
            {
                x = parent.Left + ((parent.Right - parent.Left) - width) / 2;
                y = parent.Top + ((parent.Bottom - parent.Top) - height) / 3;
            }
            appWindow.MoveAndResize(new RectInt32(x, y, width, height));
        }

        private static string ResolveIconPath()
        {
            string[] candidates =
            {
                Path.Combine(AppContext.BaseDirectory, "assets", "icons", "icon.ico"),
                Path.Combine(AppContext.BaseDirectory, "assets", "icon.ico"),
                Path.Combine(AppContext.BaseDirectory, "icon.ico"),
            };
            foreach (string candidate in candidates)
            {
                if (File.Exists(candidate)) return candidate;
            }
            return candidates[0];
        }
    }


    public static class UiHover
    {
        private static readonly HashSet<FrameworkElement> Wired = new();
        private static Microsoft.UI.Composition.Compositor? _compositor;
        private static Microsoft.UI.Composition.SpringVector3NaturalMotionAnimation? _grow;
        private static Microsoft.UI.Composition.SpringVector3NaturalMotionAnimation? _shrink;

        public static readonly DependencyProperty IsEnabledProperty =
            DependencyProperty.RegisterAttached(
                "IsEnabled",
                typeof(bool),
                typeof(UiHover),
                new PropertyMetadata(false, OnIsEnabledChanged));

        public static bool GetIsEnabled(DependencyObject obj) => (bool)obj.GetValue(IsEnabledProperty);
        public static void SetIsEnabled(DependencyObject obj, bool value) => obj.SetValue(IsEnabledProperty, value);

        private static void OnIsEnabledChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
        {
            if (d is not Button button || e.NewValue is not true) return;

            button.Loaded += (_, _) => Wire(button);
            if (button.IsLoaded) Wire(button);
        }

        private static void Wire(Button button)
        {
            if (!Wired.Add(button)) return;
            button.PointerEntered += (_, _) => Animate(button, true);
            button.PointerExited += (_, _) => Animate(button, false);
        }

        private static void Animate(FrameworkElement element, bool hover)
        {
            try
            {
                if (element is Button { IsEnabled: false }) return;

                _compositor ??= Microsoft.UI.Xaml.Media.CompositionTarget.GetCompositorForCurrentThread();
                if (hover)
                    _grow ??= CreateSpring(new System.Numerics.Vector3(1.04f, 1.04f, 1f));
                else
                    _shrink ??= CreateSpring(System.Numerics.Vector3.One);

                Microsoft.UI.Xaml.Hosting.ElementCompositionPreview.GetElementVisual(element)
                    .StartAnimation("Scale", hover ? _grow! : _shrink!);
            }
            catch
            {
            }
        }

        private static Microsoft.UI.Composition.SpringVector3NaturalMotionAnimation CreateSpring(System.Numerics.Vector3 final)
        {
            var anim = _compositor!.CreateSpringVector3Animation();
            anim.Target = "Scale";
            anim.FinalValue = final;
            anim.DampingRatio = 0.8f;
            anim.Period = TimeSpan.FromMilliseconds(90);
            return anim;
        }
    }


    internal static class PlayerWebViewShared
    {
        public static string ToPlainJson(string message)
        {
            string s = message;
            if (s.Length >= 2 && s[0] == '"' && s[^1] == '"')
            {
                try { s = System.Text.Json.JsonSerializer.Deserialize<string>(s) ?? s; } catch { }
            }
            return s;
        }

        public static async Task WaitForLoadedAsync(FrameworkElement element, TimeSpan timeout)
        {
            if (element.IsLoaded) return;
            var tcs = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            RoutedEventHandler handler = (_, _) => tcs.TrySetResult();
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


    public sealed class PlayerFullScreenWindow : Window
    {
        private readonly WebView2 _web = new();

        public PlayerFullScreenWindow()
        {
            Title = "Instant Replay";
            _web.DefaultBackgroundColor = Microsoft.UI.Colors.Black;
            Content = _web;
        }

        public bool WebReady { get; private set; }

        public event Action<bool>? NavigationCompleted;

        private const int GwlpHwndParent = -8;

        [DllImport("user32.dll", EntryPoint = "SetWindowLongPtrW")]
        private static extern IntPtr SetOwnerLongPtr(IntPtr hWnd, int nIndex, IntPtr dwNewLong);

        public async Task OpenAsync(string url, Action<string> onMessage, IntPtr ownerHwnd = default)
        {
            try
            {
                var hwnd = WindowNative.GetWindowHandle(this);
                if (ownerHwnd != IntPtr.Zero)
                    SetOwnerLongPtr(hwnd, GwlpHwndParent, ownerHwnd);
                var appWindow = AppWindow.GetFromWindowId(Win32Interop.GetWindowIdFromWindow(hwnd));
                if (appWindow == null)
                    throw new InvalidOperationException("AppWindow not available for the fullscreen player.");
                appWindow.SetPresenter(AppWindowPresenterKind.FullScreen);
                Activate();

                await PlayerWebViewShared.WaitForLoadedAsync(_web, TimeSpan.FromSeconds(6));
                await _web.EnsureCoreWebView2Async();
                var cwv = _web.CoreWebView2!;
                cwv.Settings.IsZoomControlEnabled = false;
                cwv.Settings.AreBrowserAcceleratorKeysEnabled = false;
                cwv.WebMessageReceived += (_, e) => onMessage(PlayerWebViewShared.ToPlainJson(e.WebMessageAsJson ?? ""));
                cwv.NavigationCompleted += (_, e) =>
                {
                    try { _web.Focus(FocusState.Programmatic); } catch { }
                    NavigationCompleted?.Invoke(e.IsSuccess);
                };
                cwv.Navigate(url);
                try { _web.Focus(FocusState.Programmatic); } catch { }
                WebReady = true;
            }
            catch (Exception ex)
            {
                Logger.Error($"Fullscreen WebView2 init failed: {ex.Message}", ex);
            }
        }

        public void SendScript(string js)
        {
            try
            {
                if (_web.CoreWebView2 != null) _ = _web.CoreWebView2.ExecuteScriptAsync(js);
            }
            catch { }
        }

        public void Shutdown()
        {
            try { _web.Close(); } catch { }
            try { Close(); } catch { }
        }
    }
}