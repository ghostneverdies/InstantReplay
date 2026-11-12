namespace InstantReplay.Models;

/// <summary>Capture backend that suits a <see cref="WindowMode"/>.</summary>
internal enum CaptureSource
{
    /// <summary>Windows Graphics Capture of the game window.</summary>
    Wgc,
    /// <summary>DXGI desktop duplication of the whole display the game is on.</summary>
    Dxgi,
}
