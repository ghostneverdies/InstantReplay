using System.Collections.ObjectModel;
using System.ComponentModel;
using System.Globalization;
using System.Security.Cryptography;
using System.Text;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Microsoft.VisualBasic.FileIO;
using Windows.Storage;
using Windows.Storage.FileProperties;
using Windows.Storage.Streams;

namespace InstantReplay;

public sealed class ReplayItem : INotifyPropertyChanged
{
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
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    public string FolderPath { get; }
    public string VideoPath { get; }
    public DateTime Created { get; }
    public string ThumbPath => ReplayLibrary.ThumbnailPathFor(FolderPath);

    internal bool Busy;
    public bool MetadataLoaded { get; internal set; }

    public long SizeBytes
    {
        get => _sizeBytes;
        set { if (_sizeBytes == value) return; _sizeBytes = value; Raise(nameof(SizeBytes), nameof(SizeText), nameof(InfoText)); }
    }

    public TimeSpan Duration
    {
        get => _duration;
        set { if (_duration == value) return; _duration = value; Raise(nameof(Duration), nameof(DurationText)); }
    }

    public int VideoWidth
    {
        get => _videoWidth;
        set { if (_videoWidth == value) return; _videoWidth = value; Raise(nameof(VideoWidth), nameof(QualityText), nameof(InfoText)); }
    }

    public int VideoHeight
    {
        get => _videoHeight;
        set { if (_videoHeight == value) return; _videoHeight = value; Raise(nameof(VideoHeight), nameof(QualityText), nameof(InfoText)); }
    }

    public double Fps
    {
        get => _fps;
        set { if (Math.Abs(_fps - value) < 0.01) return; _fps = value; Raise(nameof(Fps), nameof(QualityText), nameof(InfoText)); }
    }

    public ImageSource? Thumbnail
    {
        get => _thumbnail;
        set { if (ReferenceEquals(_thumbnail, value)) return; _thumbnail = value; Raise(nameof(Thumbnail)); }
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

    private void Raise(params string[] names)
    {
        foreach (string n in names) PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(n));
    }
}

public sealed class ReplayLibrary
{
    private readonly record struct ScanEntry(string Folder, string Video, DateTime Created, long Size);

    private readonly DispatcherQueueTimer _debounce;
    private FileSystemWatcher? _watcher;
    private string _watchedDir = "";

    public ReplayLibrary(DispatcherQueue dq)
    {
        _debounce = dq.CreateTimer();
        _debounce.Interval = TimeSpan.FromMilliseconds(600);
        _debounce.IsRepeating = false;
        _debounce.Tick += (_, _) => FolderChanged?.Invoke();
        DispatcherQueue = dq;
    }

    public DispatcherQueue DispatcherQueue { get; }
    public ObservableCollection<ReplayItem> Items { get; } = new();
    public event Action? FolderChanged;

    public static string ThumbnailPathFor(string folderPath)
    {
        string name = Path.GetFileName(folderPath.TrimEnd('\\', '/'));
        byte[] hash = SHA1.HashData(Encoding.UTF8.GetBytes(folderPath.ToLowerInvariant()));
        string suffix = Convert.ToHexString(hash)[..8];
        return Path.Combine(Path.GetTempPath(), "thumbnails", $"{name}_{suffix}.jpg");
    }

    public async Task RefreshAsync(string dir)
    {
        Watch(dir);

        List<ScanEntry> found = await Task.Run(() => Scan(dir));
        var foundKeys = new HashSet<string>(found.Select(f => f.Folder), StringComparer.OrdinalIgnoreCase);

        for (int i = Items.Count - 1; i >= 0; i--)
        {
            if (!foundKeys.Contains(Items[i].FolderPath)) Items.RemoveAt(i);
        }

        var existing = Items.ToDictionary(x => x.FolderPath, StringComparer.OrdinalIgnoreCase);
        for (int i = 0; i < found.Count; i++)
        {
            ScanEntry f = found[i];
            if (!existing.TryGetValue(f.Folder, out ReplayItem? item))
            {
                item = new ReplayItem(f.Folder, f.Video, f.Created, f.Size);
                existing[f.Folder] = item;
                Items.Insert(Math.Min(i, Items.Count), item);
            }
            else
            {
                item.SizeBytes = f.Size;
                int cur = Items.IndexOf(item);
                if (cur >= 0 && cur != i) Items.Move(cur, i);
            }
        }

        _ = EnsureAllAsync(Items.ToList());
    }

    private static List<ScanEntry> Scan(string dir)
    {
        var list = new List<ScanEntry>();
        try
        {
            if (!Directory.Exists(dir)) return list;
            foreach (string folder in Directory.EnumerateDirectories(dir, "replay_*"))
            {
                string video = Path.Combine(folder, "replay.mp4");
                if (!File.Exists(video)) continue;

                var info = new FileInfo(video);
                DateTime created = TryParseFolderDate(Path.GetFileName(folder)) ?? info.LastWriteTime;
                list.Add(new ScanEntry(folder, video, created, info.Length));
            }
        }
        catch (Exception ex)
        {
            Logger.Warn($"Gallery: scanning '{dir}' failed: {ex.Message}");
        }

        list.Sort((a, b) => b.Created.CompareTo(a.Created));
        return list;
    }

    private static DateTime? TryParseFolderDate(string folderName)
    {
        const string prefix = "replay_";
        if (!folderName.StartsWith(prefix, StringComparison.OrdinalIgnoreCase)) return null;
        return DateTime.TryParseExact(folderName[prefix.Length..], "yyyyMMdd_HHmmss",
            CultureInfo.InvariantCulture, DateTimeStyles.None, out DateTime dt) ? dt : null;
    }

    private async Task EnsureAllAsync(List<ReplayItem> items)
    {
        foreach (ReplayItem item in items) await EnsureAsync(item);
    }

    public async Task EnsureAsync(ReplayItem item)
    {
        if (item.Busy) return;
        bool thumbOk = item.Thumbnail != null && File.Exists(item.ThumbPath);
        if (item.MetadataLoaded && thumbOk) return;

        item.Busy = true;
        try
        {
            StorageFile file = await StorageFile.GetFileFromPathAsync(item.VideoPath);
            if (!item.MetadataLoaded) await LoadMetadataAsync(item, file);
            if (!thumbOk) await EnsureThumbnailAsync(item, file);
        }
        catch (Exception ex)
        {
            Logger.Warn($"Gallery: could not prepare '{item.VideoPath}': {ex.Message}");
        }
        finally
        {
            item.Busy = false;
        }
    }

    private static async Task LoadMetadataAsync(ReplayItem item, StorageFile file)
    {
        try
        {
            VideoProperties vp = await file.Properties.GetVideoPropertiesAsync();
            item.Duration = vp.Duration;
            item.VideoWidth = (int)vp.Width;
            item.VideoHeight = (int)vp.Height;

            IDictionary<string, object> props =
                await file.Properties.RetrievePropertiesAsync(new[] { "System.Video.FrameRate" });
            if (props.TryGetValue("System.Video.FrameRate", out object? v) && v is uint fr && fr > 0)
                item.Fps = fr / 1000.0;
        }
        catch (Exception ex)
        {
            Logger.Warn($"Gallery: reading video properties failed: {ex.Message}");
        }
        item.MetadataLoaded = true;
    }

    private static async Task EnsureThumbnailAsync(ReplayItem item, StorageFile video)
    {
        string thumbPath = item.ThumbPath;
        if (!File.Exists(thumbPath))
        {
            Directory.CreateDirectory(Path.GetDirectoryName(thumbPath)!);

            using StorageItemThumbnail thumb =
                await video.GetThumbnailAsync(ThumbnailMode.VideosView, 480, ThumbnailOptions.UseCurrentScale);
            if (thumb == null || thumb.Type != ThumbnailType.Image || thumb.Size == 0) return;

            var bytes = new byte[thumb.Size];
            using (var reader = new DataReader(thumb.GetInputStreamAt(0)))
            {
                await reader.LoadAsync((uint)thumb.Size);
                reader.ReadBytes(bytes);
            }
            await File.WriteAllBytesAsync(thumbPath, bytes);
        }

        StorageFile thumbFile = await StorageFile.GetFileFromPathAsync(thumbPath);
        using IRandomAccessStream stream = await thumbFile.OpenReadAsync();
        var bmp = new BitmapImage { DecodePixelWidth = 480 };
        await bmp.SetSourceAsync(stream);
        item.Thumbnail = bmp;
    }

    public async Task<bool> DeleteAsync(ReplayItem item)
    {
        try
        {
            await Task.Run(() => FileSystem.DeleteDirectory(item.FolderPath,
                UIOption.OnlyErrorDialogs, RecycleOption.SendToRecycleBin));
            try { File.Delete(item.ThumbPath); } catch { }
            Items.Remove(item);
            return true;
        }
        catch (Exception ex)
        {
            Logger.Warn($"Gallery: deleting '{item.FolderPath}' failed: {ex.Message}");
            return false;
        }
    }

    private void Watch(string dir)
    {
        if (string.Equals(_watchedDir, dir, StringComparison.OrdinalIgnoreCase) && _watcher != null) return;

        _watcher?.Dispose();
        _watcher = null;
        _watchedDir = dir;

        try
        {
            if (!Directory.Exists(dir)) return;

            var watcher = new FileSystemWatcher(dir)
            {
                IncludeSubdirectories = true,
                NotifyFilter = NotifyFilters.FileName | NotifyFilters.DirectoryName,
            };
            FileSystemEventHandler onChange = (_, _) => DispatcherQueue.TryEnqueue(() => { _debounce.Stop(); _debounce.Start(); });
            watcher.Created += onChange;
            watcher.Deleted += onChange;
            watcher.Renamed += (_, _) => DispatcherQueue.TryEnqueue(() => { _debounce.Stop(); _debounce.Start(); });
            watcher.EnableRaisingEvents = true;
            _watcher = watcher;
        }
        catch (Exception ex)
        {
            Logger.Warn($"Gallery: could not watch '{dir}': {ex.Message}");
        }
    }
}
