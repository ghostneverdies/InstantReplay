using System.Collections.Generic;
using Windows.System;

namespace InstantReplay;

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
        else if (parts.Count == 0) parts.Add("…");

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
