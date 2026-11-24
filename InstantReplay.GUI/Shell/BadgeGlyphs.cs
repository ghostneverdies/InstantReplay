using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using System.Runtime.InteropServices;
using System.Windows.Media;
using InstantReplay.Services;

namespace InstantReplay.GUI.Shell;

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

        return IntPtr.Zero;
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
                double coverage = Compat.Clamp(radius - d + 0.5, 0, 1);
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
