namespace InstantReplay.Models;

public enum SaveReplayOutcome
{
    Success,
    NotRecording,
    NoSegments,
    WriteBlocked,
    MergeFailed,
    LowMemory,
}
