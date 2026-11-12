namespace InstantReplay.GUI.Shell;

public sealed class TrayMenuItem
{
    public string Text { get; init; } = "";
    public string Glyph { get; init; } = "";
    public string Shortcut { get; init; } = "";
    public Action? OnClick { get; init; }
    public bool IsSeparator { get; init; }
    public bool IsCheckable { get; init; }
    public bool IsChecked { get; init; }
    public bool IsEnabled { get; init; } = true;

    public static TrayMenuItem Separator() => new() { IsSeparator = true };
}
