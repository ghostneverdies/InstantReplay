using System;
using System.Collections.Generic;
using System.Windows.Input;

namespace InstantReplay.GUI.Helpers;

public static class HotkeyDisplay
{
    private const uint VkOemComma = 0xBC, VkOemPeriod = 0xBE, VkOemQuestion = 0xBF,
                        VkOemSemicolon = 0xBA, VkOemQuotes = 0xDE, VkOemPlus = 0xBB, VkOemMinus = 0xBD,
                        VkMultiply = 0x6A, VkAdd = 0x6B, VkSubtract = 0x6D, VkDivide = 0x6F, VkDecimal = 0x6E,
                        VkNumPad0 = 0x60, VkNumPad9 = 0x69, VkD0 = 0x30, VkD9 = 0x39, VkSpace = 0x20;

    public static string Format(uint modifiers, uint vk) =>
        string.Join(" + ", Parts(modifiers, vk));

    public static IReadOnlyList<string> Parts(uint modifiers, uint vk)
    {
        var parts = new List<string>();
        if ((modifiers & MainWindow.ModControl) != 0) parts.Add("CTRL");
        if ((modifiers & MainWindow.ModAlt) != 0) parts.Add("ALT");
        if ((modifiers & MainWindow.ModShift) != 0) parts.Add("SHIFT");
        if ((modifiers & MainWindow.ModWin) != 0) parts.Add("WIN");

        if (vk != 0) parts.Add(VkName(vk));
        else if (parts.Count == 0) parts.Add("…");

        return parts;
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

        Key key = KeyInterop.KeyFromVirtualKey((int)vk);
        switch (key)
        {
            case Key.Return: return "ENTER";
            case Key.Prior: return "PAGEUP";
            case Key.Next: return "PAGEDOWN";
            case Key.Capital: return "CAPSLOCK";
            case Key.None: return "0X" + vk.ToString("X2");
        }
        return key.ToString().ToUpperInvariant();
    }

    public static uint? ModifierBitForKey(Key key)
    {
        switch (key)
        {
            case Key.LeftCtrl:
            case Key.RightCtrl:
                return MainWindow.ModControl;
            case Key.LeftAlt:
            case Key.RightAlt:
                return MainWindow.ModAlt;
            case Key.LeftShift:
            case Key.RightShift:
                return MainWindow.ModShift;
            case Key.LWin:
            case Key.RWin:
                return MainWindow.ModWin;
            default:
                return null;
        }
    }
}
