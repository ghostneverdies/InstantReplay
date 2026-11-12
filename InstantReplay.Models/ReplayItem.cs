using System.ComponentModel;
using System.Globalization;
using Microsoft.UI.Xaml.Media;
using InstantReplay.Core;

namespace InstantReplay.Models;

public sealed class ReplayItem : INotifyPropertyChanged
{
    // Cached event args: the gallery raises these for every card as its metadata arrives,
    // so allocating a fresh PropertyChangedEventArgs per notification showed up in profiles.
    private static readonly PropertyChangedEventArgs[] MetadataChanged =
    {
        new(nameof(Duration)), new(nameof(DurationText)),
        new(nameof(VideoWidth)), new(nameof(VideoHeight)), new(nameof(Fps)),
        new(nameof(QualityText)), new(nameof(InfoText)),
    };
    private static readonly PropertyChangedEventArgs ThumbnailChanged = new(nameof(Thumbnail));

    private long _sizeBytes;
    private TimeSpan _duration;
    private int _videoWidth;
    private int _videoHeight;
    private double _fps;
    private ImageSource? _thumbnail;

    public ReplayItem(string folderPath, string videoPath, DateTime created, long sizeBytes)
    {
        FolderPath = folderPath;
        VideoPath = videoPath;
        Created = created;
        _sizeBytes = sizeBytes;
        // Hashing the path on every access was measurable with hundreds of replays.
        ThumbPath = ReplayLibrary.ThumbnailPathFor(folderPath);
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    public string FolderPath { get; }
    public string VideoPath { get; }
    public DateTime Created { get; }
    public string ThumbPath { get; }

    // Preparation bookkeeping owned by ReplayLibrary (guarded by its lock).
    internal int PrepareState;          // 0 idle, 1 queued, 2 running
    internal bool QueuedAsVisible;
    internal int Failures;
    internal readonly TaskCompletionSource Prepared =
        new(TaskCreationOptions.RunContinuationsAsynchronously);

    public volatile bool MetadataLoaded;

    /// <summary>True once both the metadata and the thumbnail are in place.</summary>
    internal bool IsReady => MetadataLoaded && _thumbnail != null;

    public long SizeBytes
    {
        get => _sizeBytes;
        set
        {
            if (_sizeBytes == value) return;
            _sizeBytes = value;
            Raise(nameof(SizeBytes)); Raise(nameof(SizeText)); Raise(nameof(InfoText));
        }
    }

    public TimeSpan Duration => _duration;
    public int VideoWidth => _videoWidth;
    public int VideoHeight => _videoHeight;
    public double Fps => _fps;

    public ImageSource? Thumbnail => _thumbnail;

    /// <summary>
    /// Applies everything the background workers produced in one UI-thread pass and raises
    /// each change notification once, instead of one round of notifications per property.
    /// </summary>
    internal void ApplyPrepared(bool hasMetadata, TimeSpan duration, int width, int height, double fps,
        ImageSource? thumbnail)
    {
        if (hasMetadata)
        {
            _duration = duration;
            _videoWidth = width;
            _videoHeight = height;
            _fps = fps;
            MetadataLoaded = true;
            foreach (PropertyChangedEventArgs args in MetadataChanged) PropertyChanged?.Invoke(this, args);
        }

        if (thumbnail != null && !ReferenceEquals(_thumbnail, thumbnail))
        {
            _thumbnail = thumbnail;
            PropertyChanged?.Invoke(this, ThumbnailChanged);
        }
    }

    public string DateText => Created.ToString("MMM d, yyyy  ·  HH:mm", CultureInfo.CurrentCulture);

    public string DurationText => Duration > TimeSpan.Zero
        ? (Duration.TotalHours >= 1
            ? $"{(int)Duration.TotalHours}:{Duration.Minutes:00}:{Duration.Seconds:00}"
            : $"{(int)Duration.TotalMinutes}:{Duration.Seconds:00}")
        : "--:--";

    public string SizeText => FormatSize(SizeBytes);

    public string QualityText
    {
        get
        {
            if (VideoHeight <= 0) return "—";
            return Fps > 0 ? $"{VideoHeight}p  ·  {Fps:0} FPS" : $"{VideoHeight}p";
        }
    }

    public string InfoText => VideoHeight > 0 ? $"{QualityText}  ·  {SizeText}" : SizeText;

    private static string FormatSize(long bytes)
    {
        double mb = bytes / (1024.0 * 1024.0);
        return mb >= 1024 ? $"{mb / 1024.0:0.0} GB" : $"{mb:0.0} MB";
    }

    private void Raise(string name) => PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
}
