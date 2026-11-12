namespace InstantReplay.Models;

public readonly struct SaveReplayResult
{
    public SaveReplayOutcome Outcome { get; }
    public string? Path { get; }
    public bool Success => Outcome == SaveReplayOutcome.Success;

    public SaveReplayResult(SaveReplayOutcome outcome, string? path = null)
    {
        Outcome = outcome;
        Path = path;
    }

    public string UserMessage() => Outcome switch
    {
        SaveReplayOutcome.Success => $"Replay saved to {Path}",
        SaveReplayOutcome.NotRecording => "Recorder isn't running — try starting capture first.",
        SaveReplayOutcome.NoSegments => "Still buffering — nothing to save yet. Try again in a few seconds.",
        SaveReplayOutcome.WriteBlocked =>
            "Save blocked by Windows security. Open Windows Security → Virus & threat protection → " +
            "Manage ransomware protection → Controlled folder access, and allow Instant Replay.exe " +
            "through it, then try again.",
        SaveReplayOutcome.MergeFailed => "Failed to save replay (see log for details).",
        SaveReplayOutcome.LowMemory => "Replay save skipped: Windows is running low on committed memory. Close other apps or enable a system-managed page file before trying again.",
        _ => "Failed to save replay (see log for details).",
    };
}
