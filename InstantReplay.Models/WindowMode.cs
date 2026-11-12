namespace InstantReplay.Models;

/// <summary>How the game presents itself on screen.</summary>
internal enum WindowMode
{
    /// <summary>Normal window (title bar / frame), including maximized windows.</summary>
    Windowed,
    /// <summary>Frameless window that covers its monitor but is still composed by DWM.</summary>
    BorderlessFullscreen,
    /// <summary>Exclusive fullscreen: the game owns the display and bypasses normal composition.</summary>
    ExclusiveFullscreen,
}
