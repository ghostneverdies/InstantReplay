using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using System.Collections.ObjectModel;
using System.Globalization;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Runtime.InteropServices;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using System.Windows.Threading;
using Microsoft.VisualBasic.FileIO;
using InstantReplay.Models;
using InstantReplay.Services;

namespace InstantReplay.Core;

public sealed class ReplayLibrary
{
    private readonly record struct ScanEntry(string Folder, string Video, DateTime Created, long Size);

    private sealed record MetaCache(TimeSpan Duration, int Width, int Height, double Fps, long Length, long Ticks);

    private const int StateIdle = 0, StateQueued = 1, StateRunning = 2;
    private const int MaxFailures = 2;

    private readonly DispatcherTimer _debounce;

    private readonly object _gate = new();
    private readonly Stack<ReplayItem> _visible = new();
    private readonly Queue<ReplayItem> _background = new();
    private readonly SemaphoreSlim _signal = new(0);
    private FileSystemWatcher? _watcher;
    private string _watchedDir = "";
    private bool _workersStarted;

    public ReplayLibrary(Dispatcher dispatcher)
    {
        Dispatcher = dispatcher;
        _debounce = new DispatcherTimer(DispatcherPriority.Normal, dispatcher) { Interval = TimeSpan.FromMilliseconds(600) };
        _debounce.Tick += (_, _) => { _debounce.Stop(); FolderChanged?.Invoke(); };
    }

    public Dispatcher Dispatcher { get; }
    public ObservableCollection<ReplayItem> Items { get; } = new();
    public event Action? FolderChanged;

    private static readonly string ThumbnailRoot = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "InstantReplay", "cache", "thumbnails");

    public static string ThumbnailPathFor(string folderPath)
    {
        string name = Path.GetFileName(folderPath.TrimEnd('\\', '/'));
        byte[] hash;
        using (var sha1 = SHA1.Create())
            hash = sha1.ComputeHash(Encoding.UTF8.GetBytes(folderPath.ToLowerInvariant()));
        string suffix = Compat.ToHexString(hash).Substring(0, 8);
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
        return DateTime.TryParseExact(folderName.Substring(prefix.Length), "yyyyMMdd_HHmmss",
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
                    item.Prepared.TrySetResult(true);
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

        var priority = visible ? DispatcherPriority.Normal : DispatcherPriority.Background;
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

    private Task<MetaCache?> ReadVideoMetaAsync(ReplayItem item)
    {
        return Task.Run<MetaCache?>(() =>
        {
            try
            {
                var info = new FileInfo(item.VideoPath);
                ShellVideoInfo video = ShellMedia.ReadVideoInfo(item.VideoPath);
                return new MetaCache(video.Duration, video.Width, video.Height, video.Fps,
                    info.Length, info.LastWriteTimeUtc.Ticks);
            }
            catch (Exception ex)
            {
                Logger.Warn($"Gallery: reading video properties failed: {ex.Message}");
                return null;
            }
        });
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
            bytes = await Task.Run(() => ShellMedia.CreateThumbnailJpeg(item.VideoPath, 480)).ConfigureAwait(false);
        }
        catch (Exception ex)
        {
            Logger.Debug($"Gallery: shell thumbnail unavailable for '{item.VideoPath}': {ex.Message}");
        }

        if (bytes == null || bytes.Length == 0) return;

        string temporary = thumbPath + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            await Compat.WriteAllBytesAsync(temporary, bytes).ConfigureAwait(false);
            Compat.MoveOverwrite(temporary, thumbPath);
            Logger.Debug($"Gallery: thumbnail generated for '{Path.GetFileName(item.FolderPath)}'.");
        }
        finally
        {
            try { File.Delete(temporary); } catch { }
        }
    }

    private Task OnUiAsync(Action action, DispatcherPriority priority = DispatcherPriority.Normal)
    {
        return Dispatcher.InvokeAsync(action, priority).Task;
    }

    private static ImageSource CreateThumbnailSource(string thumbPath)
    {
        var image = new BitmapImage();
        image.BeginInit();
        image.DecodePixelWidth = 480;
        image.CacheOption = BitmapCacheOption.OnLoad;
        image.CreateOptions = BitmapCreateOptions.IgnoreColorProfile;
        image.UriSource = new Uri(thumbPath);
        image.EndInit();
        image.Freeze();
        return image;
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
            FileSystemEventHandler onChange = (_, _) => Dispatcher.BeginInvoke(new Action(() => { _debounce.Stop(); _debounce.Start(); }));
            watcher.Created += onChange;
            watcher.Deleted += onChange;
            watcher.Renamed += (_, _) => Dispatcher.BeginInvoke(new Action(() => { _debounce.Stop(); _debounce.Start(); }));
            watcher.EnableRaisingEvents = true;
            _watcher = watcher;
        }
        catch (Exception ex)
        {
            Logger.Warn($"Gallery: could not watch '{dir}': {ex.Message}");
        }
    }
}


internal readonly struct ShellVideoInfo
{
    public ShellVideoInfo(TimeSpan duration, int width, int height, double fps)
    {
        Duration = duration; Width = width; Height = height; Fps = fps;
    }

    public TimeSpan Duration { get; }
    public int Width { get; }
    public int Height { get; }
    public double Fps { get; }
}

/// <summary>
/// Video metadata and thumbnails straight from the Windows shell (the same providers Explorer uses).
/// Replaces the WinRT StorageFile/VideoProperties/GetThumbnailAsync calls, which .NET Framework 4.8 cannot use.
/// </summary>
internal static class ShellMedia
{
    private static readonly Guid IidShellItem = new Guid("43826D1E-E718-42EE-BC55-A1E261C37BFE");
    private static readonly Guid IidShellItemImageFactory = new Guid("BCC18B79-BA16-442F-80C4-8A59C30C463B");
    private static readonly Guid IidPropertyStore = new Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99");

    private static readonly Guid FmtidMedia = new Guid("64440490-4C8B-11D1-8B70-080036B11A03");
    private static readonly Guid FmtidVideo = new Guid("64440491-4C8B-11D1-8B70-080036B11A03");

    private const uint SiigbfBiggerSizeOk = 0x01;
    private const uint SiigbfThumbnailOnly = 0x08;
    private const uint GpsDefault = 0;

    [StructLayout(LayoutKind.Sequential)]
    private struct Size { public int Cx, Cy; }

    [StructLayout(LayoutKind.Sequential)]
    private struct PropertyKeyNative
    {
        public Guid Fmtid;
        public uint Pid;
    }

    // PROPVARIANT: 2-byte type, 6 reserved bytes, then a union (8 bytes on x86, 16 on x64).
    [StructLayout(LayoutKind.Sequential)]
    private struct PropVariantNative
    {
        public ushort Vt;
        public ushort Reserved1, Reserved2, Reserved3;
        public ulong Value;
        public ulong Value2;
    }

    [ComImport, Guid("BCC18B79-BA16-442F-80C4-8A59C30C463B"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IShellItemImageFactory
    {
        [PreserveSig]
        int GetImage(Size size, uint flags, out IntPtr phbm);
    }

    [ComImport, Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IPropertyStoreNative
    {
        [PreserveSig] int GetCount(out uint count);
        [PreserveSig] int GetAt(uint index, out PropertyKeyNative key);
        [PreserveSig] int GetValue(ref PropertyKeyNative key, out PropVariantNative value);
        [PreserveSig] int SetValue(ref PropertyKeyNative key, ref PropVariantNative value);
        [PreserveSig] int Commit();
    }

    [DllImport("shell32.dll", CharSet = CharSet.Unicode, PreserveSig = true)]
    private static extern int SHCreateItemFromParsingName(string path, IntPtr bindContext, ref Guid riid,
        [MarshalAs(UnmanagedType.Interface)] out object shellItem);

    [DllImport("shell32.dll", CharSet = CharSet.Unicode, PreserveSig = true)]
    private static extern int SHGetPropertyStoreFromParsingName(string path, IntPtr bindContext, uint flags, ref Guid riid,
        [MarshalAs(UnmanagedType.Interface)] out IPropertyStoreNative store);

    [DllImport("ole32.dll")]
    private static extern int PropVariantClear(ref PropVariantNative value);

    [DllImport("gdi32.dll")]
    private static extern bool DeleteObject(IntPtr handle);

    private static ulong ReadUInt64(IPropertyStoreNative store, Guid fmtid, uint pid)
    {
        var key = new PropertyKeyNative { Fmtid = fmtid, Pid = pid };
        if (store.GetValue(ref key, out PropVariantNative value) != 0) return 0;
        try
        {
            switch (value.Vt)
            {
                case 21: // VT_UI8
                case 20: // VT_I8
                    return value.Value;
                case 19: // VT_UI4
                case 3:  // VT_I4
                    return (uint)(value.Value & 0xFFFFFFFF);
                default:
                    return 0;
            }
        }
        finally { PropVariantClear(ref value); }
    }

    public static ShellVideoInfo ReadVideoInfo(string path)
    {
        Guid iid = IidPropertyStore;
        int hr = SHGetPropertyStoreFromParsingName(path, IntPtr.Zero, GpsDefault, ref iid, out IPropertyStoreNative store);
        if (hr != 0) throw new COMException("The shell could not read the video properties.", hr);
        try
        {
            ulong duration100ns = ReadUInt64(store, FmtidMedia, 3);   // System.Media.Duration
            ulong width = ReadUInt64(store, FmtidVideo, 3);           // System.Video.FrameWidth
            ulong height = ReadUInt64(store, FmtidVideo, 4);          // System.Video.FrameHeight
            ulong frameRate = ReadUInt64(store, FmtidVideo, 6);       // System.Video.FrameRate (frames per 1000 s)
            return new ShellVideoInfo(
                TimeSpan.FromTicks((long)Math.Min(duration100ns, (ulong)long.MaxValue)),
                (int)Math.Min(width, int.MaxValue), (int)Math.Min(height, int.MaxValue),
                frameRate > 0 ? frameRate / 1000.0 : 0);
        }
        finally { Marshal.ReleaseComObject(store); }
    }

    public static byte[]? CreateThumbnailJpeg(string path, int width)
    {
        Guid iid = IidShellItem;
        int hr = SHCreateItemFromParsingName(path, IntPtr.Zero, ref iid, out object item);
        if (hr != 0) throw new COMException("The shell could not open the video.", hr);
        IntPtr bitmap = IntPtr.Zero;
        try
        {
            var factory = (IShellItemImageFactory)item;
            hr = factory.GetImage(new Size { Cx = width, Cy = width * 9 / 16 }, SiigbfBiggerSizeOk | SiigbfThumbnailOnly, out bitmap);
            if (hr != 0 || bitmap == IntPtr.Zero) throw new COMException("The shell has no thumbnail for this video.", hr);

            BitmapSource source = System.Windows.Interop.Imaging.CreateBitmapSourceFromHBitmap(
                bitmap, IntPtr.Zero, System.Windows.Int32Rect.Empty, BitmapSizeOptions.FromEmptyOptions());
            var encoder = new JpegBitmapEncoder { QualityLevel = 90 };
            encoder.Frames.Add(BitmapFrame.Create(source));
            using (var stream = new MemoryStream())
            {
                encoder.Save(stream);
                return stream.ToArray();
            }
        }
        finally
        {
            if (bitmap != IntPtr.Zero) DeleteObject(bitmap);
            Marshal.ReleaseComObject(item);
        }
    }
}
