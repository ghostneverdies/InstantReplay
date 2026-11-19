using System.Collections.ObjectModel;
using System.Globalization;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Microsoft.VisualBasic.FileIO;
using Windows.Media.Editing;
using Windows.Storage;
using Windows.Storage.FileProperties;
using Windows.Storage.Streams;
using InstantReplay.Models;
using InstantReplay.Services;

namespace InstantReplay.Core;

public sealed class ReplayLibrary
{
    private readonly record struct ScanEntry(string Folder, string Video, DateTime Created, long Size);

    private sealed record MetaCache(TimeSpan Duration, int Width, int Height, double Fps, long Length, long Ticks);

    private const int StateIdle = 0, StateQueued = 1, StateRunning = 2;
    private const int MaxFailures = 2;

    private readonly DispatcherQueueTimer _debounce;

    private readonly object _gate = new();
    private readonly Stack<ReplayItem> _visible = new();
    private readonly Queue<ReplayItem> _background = new();
    private readonly SemaphoreSlim _signal = new(0);
    private FileSystemWatcher? _watcher;
    private string _watchedDir = "";
    private bool _workersStarted;

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

    private static readonly string ThumbnailRoot = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "InstantReplay", "cache", "thumbnails");

    public static string ThumbnailPathFor(string folderPath)
    {
        string name = Path.GetFileName(folderPath.TrimEnd('\\', '/'));
        byte[] hash = SHA1.HashData(Encoding.UTF8.GetBytes(folderPath.ToLowerInvariant()));
        string suffix = Convert.ToHexString(hash)[..8];
        return Path.Combine(ThumbnailRoot, $"{name}_{suffix}.jpg");
    }

    private static string LegacyThumbnailPathFor(string thumbPath) =>
        Path.Combine(Path.GetTempPath(), "thumbnails", Path.GetFileName(thumbPath));

    public async Task WarmUpAsync(string dir)
    {
        await RefreshAsync(dir);
        foreach (ReplayItem item in Items.ToList()) Queue(item, visible: false);
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

        var existing = new Dictionary<string, ReplayItem>(Items.Count, StringComparer.OrdinalIgnoreCase);
        foreach (ReplayItem current in Items) existing[current.FolderPath] = current;

        for (int i = 0; i < found.Count; i++)
        {
            ScanEntry f = found[i];
            if (!existing.TryGetValue(f.Folder, out ReplayItem? item))
            {
                item = new ReplayItem(f.Folder, f.Video, f.Created, f.Size);
                existing[f.Folder] = item;
                Items.Insert(Math.Min(i, Items.Count), item);
                Queue(item, visible: false);
                continue;
            }

            if (item.SizeBytes != f.Size)
            {
                item.SizeBytes = f.Size;
                item.Failures = 0;
                Queue(item, visible: false);
            }

            if (i < Items.Count && !ReferenceEquals(Items[i], item))
            {
                int cur = Items.IndexOf(item);
                if (cur >= 0 && cur != i) Items.Move(cur, i);
            }
        }

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

    public void Queue(ReplayItem item, bool visible = false)
    {
        if (item.IsReady) return;
        EnsureWorkers();

        lock (_gate)
        {
            if (item.PrepareState == StateRunning) return;

            if (item.PrepareState == StateQueued)
            {
                if (!visible || item.QueuedAsVisible) return;
                item.QueuedAsVisible = true;
                _visible.Push(item);
                _signal.Release();
                return;
            }

            if (item.Failures >= MaxFailures) return;

            item.PrepareState = StateQueued;
            item.QueuedAsVisible = visible;
            if (visible) _visible.Push(item); else _background.Enqueue(item);
            _signal.Release();
        }
    }

    public Task EnsureAsync(ReplayItem item)
    {
        if (item.IsReady) return Task.CompletedTask;
        Queue(item, visible: true);
        return item.Prepared.Task;
    }

    private void EnsureWorkers()
    {
        if (Volatile.Read(ref _workersStarted)) return;
        lock (_gate)
        {
            if (_workersStarted) return;
            _workersStarted = true;
        }

        for (int i = 0; i < 2; i++) _ = Task.Run(WorkerLoopAsync);
    }

    private ReplayItem? TakeNext(out bool visible)
    {
        lock (_gate)
        {
            while (_visible.Count > 0 || _background.Count > 0)
            {
                bool fromVisible = _visible.Count > 0;
                ReplayItem candidate = fromVisible ? _visible.Pop() : _background.Dequeue();

                if (candidate.PrepareState != StateQueued) continue;

                candidate.PrepareState = StateRunning;
                visible = candidate.QueuedAsVisible;
                return candidate;
            }
        }

        visible = false;
        return null;
    }

    private async Task WorkerLoopAsync()
    {
        try
        {
            while (true)
            {
                await _signal.WaitAsync().ConfigureAwait(false);

                ReplayItem? item = TakeNext(out bool visible);
                if (item == null) continue;

                bool produced = false;
                try
                {
                    produced = await PrepareAsync(item, visible).ConfigureAwait(false);
                }
                catch (Exception ex)
                {
                    Logger.Warn($"Gallery: could not prepare '{item.VideoPath}': {ex.Message}");
                }
                finally
                {
                    lock (_gate)
                    {
                        item.PrepareState = StateIdle;
                        item.QueuedAsVisible = false;
                        if (!produced) item.Failures++;
                    }
                    item.Prepared.TrySetResult();
                }
            }
        }
        catch (Exception ex)
        {
            Logger.Warn($"Gallery: thumbnail worker stopped: {ex.Message}");
        }
    }

    private async Task<bool> PrepareAsync(ReplayItem item, bool visible)
    {
        MetaCache? meta = TryReadMetaCache(item);
        if (meta == null)
        {
            meta = await ReadVideoMetaAsync(item).ConfigureAwait(false);
            if (meta != null) TryWriteMetaCache(item, meta);
        }

        if (!File.Exists(item.ThumbPath))
        {
            TryRestoreLegacyThumbnail(item);
            if (!File.Exists(item.ThumbPath))
                await GenerateThumbnailAsync(item).ConfigureAwait(false);
        }

        bool hasThumbnail = File.Exists(item.ThumbPath);
        if (meta == null && !hasThumbnail) return false;

        var priority = visible ? DispatcherQueuePriority.Normal : DispatcherQueuePriority.Low;
        await OnUiAsync(() =>
        {
            ImageSource? source = hasThumbnail && item.Thumbnail == null
                ? CreateThumbnailSource(item.ThumbPath)
                : null;
            item.ApplyPrepared(meta != null,
                meta?.Duration ?? TimeSpan.Zero, meta?.Width ?? 0, meta?.Height ?? 0, meta?.Fps ?? 0,
                source);
        }, priority).ConfigureAwait(false);

        return hasThumbnail;
    }

    private static void TryRestoreLegacyThumbnail(ReplayItem item)
    {
        try
        {
            string legacy = LegacyThumbnailPathFor(item.ThumbPath);
            if (!File.Exists(legacy)) return;
            Directory.CreateDirectory(Path.GetDirectoryName(item.ThumbPath)!);
            File.Copy(legacy, item.ThumbPath, overwrite: false);
        }
        catch { }
    }

    private async Task<MetaCache?> ReadVideoMetaAsync(ReplayItem item)
    {
        try
        {
            var info = new FileInfo(item.VideoPath);
            StorageFile file = await StorageFile.GetFileFromPathAsync(item.VideoPath).AsTask().ConfigureAwait(false);
            VideoProperties vp = await file.Properties.GetVideoPropertiesAsync().AsTask().ConfigureAwait(false);

            double fps = 0;
            IDictionary<string, object> props = await file.Properties
                .RetrievePropertiesAsync(new[] { "System.Video.FrameRate" })
                .AsTask().ConfigureAwait(false);
            if (props.TryGetValue("System.Video.FrameRate", out object? v) && v is uint fr && fr > 0)
                fps = fr / 1000.0;

            return new MetaCache(vp.Duration, (int)vp.Width, (int)vp.Height, fps,
                info.Length, info.LastWriteTimeUtc.Ticks);
        }
        catch (Exception ex)
        {
            Logger.Warn($"Gallery: reading video properties failed: {ex.Message}");
            return null;
        }
    }

    private static string MetaCachePathFor(string thumbPath) => thumbPath + ".meta.json";

    private static MetaCache? TryReadMetaCache(ReplayItem item)
    {
        try
        {
            string path = MetaCachePathFor(item.ThumbPath);
            if (!File.Exists(path)) return null;
            MetaCache? meta = JsonSerializer.Deserialize<MetaCache>(File.ReadAllText(path));
            if (meta == null) return null;

            var info = new FileInfo(item.VideoPath);
            if (meta.Length != info.Length || meta.Ticks != info.LastWriteTimeUtc.Ticks) return null;
            return meta;
        }
        catch
        {
            return null;
        }
    }

    private static void TryWriteMetaCache(ReplayItem item, MetaCache meta)
    {
        try
        {
            string path = MetaCachePathFor(item.ThumbPath);
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            File.WriteAllText(path, JsonSerializer.Serialize(meta));
        }
        catch (Exception ex)
        {
            Logger.Debug($"Gallery: metadata cache write failed: {ex.Message}");
        }
    }

    private static async Task GenerateThumbnailAsync(ReplayItem item)
    {
        string thumbPath = item.ThumbPath;
        Directory.CreateDirectory(Path.GetDirectoryName(thumbPath)!);

        byte[]? bytes = null;
        try
        {
            StorageFile video = await StorageFile.GetFileFromPathAsync(item.VideoPath).AsTask().ConfigureAwait(false);
            using StorageItemThumbnail thumb = await video
                .GetThumbnailAsync(ThumbnailMode.VideosView, 480, ThumbnailOptions.UseCurrentScale)
                .AsTask()
                .ConfigureAwait(false);
            if (thumb != null && thumb.Type == ThumbnailType.Image && thumb.Size > 0)
                bytes = await ReadThumbnailBytesAsync(thumb).ConfigureAwait(false);
        }
        catch (Exception ex)
        {
            Logger.Debug($"Gallery: shell thumbnail unavailable for '{item.VideoPath}': {ex.Message}");
        }

        if (bytes == null)
        {
            try
            {
                using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(20));
                StorageFile video = await StorageFile.GetFileFromPathAsync(item.VideoPath).AsTask().ConfigureAwait(false);
                var clip = await MediaClip.CreateFromFileAsync(video).AsTask(timeout.Token)
                    .ConfigureAwait(false);
                var composition = new MediaComposition();
                composition.Clips.Add(clip);
                TimeSpan position = TimeSpan.FromSeconds(Math.Min(1, clip.OriginalDuration.TotalSeconds / 2));
                using var image = await composition.GetThumbnailAsync(position, 480, 0,
                        VideoFramePrecision.NearestKeyFrame)
                    .AsTask(timeout.Token)
                    .ConfigureAwait(false);
                bytes = await ReadThumbnailBytesAsync(image).ConfigureAwait(false);
            }
            catch (Exception ex)
            {
                Logger.Debug($"Gallery: video thumbnail unavailable for '{item.VideoPath}': {ex.Message}");
                return;
            }
        }

        string temporary = thumbPath + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            await File.WriteAllBytesAsync(temporary, bytes).ConfigureAwait(false);
            File.Move(temporary, thumbPath, overwrite: true);
            Logger.Debug($"Gallery: thumbnail generated for '{Path.GetFileName(item.FolderPath)}'.");
        }
        finally
        {
            try { File.Delete(temporary); } catch { }
        }
    }

    private Task OnUiAsync(Action action, DispatcherQueuePriority priority = DispatcherQueuePriority.Normal)
    {
        var tcs = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        if (!DispatcherQueue.TryEnqueue(priority, () =>
            {
                try { action(); tcs.TrySetResult(); }
                catch (Exception ex) { tcs.TrySetException(ex); }
            }))
        {
            tcs.TrySetResult();
        }
        return tcs.Task;
    }

    private static ImageSource CreateThumbnailSource(string thumbPath)
    {
        return new BitmapImage
        {
            DecodePixelWidth = 480,
            UriSource = new Uri(thumbPath),
        };
    }

    private static async Task<byte[]> ReadThumbnailBytesAsync(IRandomAccessStream stream)
    {
        if (stream.Size == 0 || stream.Size > 16 * 1024 * 1024)
            throw new InvalidDataException("The thumbnail image is empty or too large.");
        var bytes = new byte[(int)stream.Size];
        using var reader = new DataReader(stream.GetInputStreamAt(0));
        uint loaded = await reader.LoadAsync((uint)bytes.Length).AsTask().ConfigureAwait(false);
        if (loaded != bytes.Length) throw new InvalidDataException("The thumbnail image is incomplete.");
        reader.ReadBytes(bytes);
        return bytes;
    }

    public async Task<bool> DeleteAsync(ReplayItem item)
    {
        try
        {
            await Task.Run(() => FileSystem.DeleteDirectory(item.FolderPath,
                UIOption.OnlyErrorDialogs, RecycleOption.SendToRecycleBin));
            try { File.Delete(item.ThumbPath); } catch { }
            try { File.Delete(MetaCachePathFor(item.ThumbPath)); } catch { }
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
