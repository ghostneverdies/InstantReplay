using System.Collections.ObjectModel;
using System.Drawing;
using System.Drawing.Imaging;
using System.Globalization;
using System.IO;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using System.Windows.Threading;
using InstantReplay.Models;
using InstantReplay.Services;
using Wpf.Ui.Controls;

namespace InstantReplay.Core;

public sealed class ReplayLibrary
{
    private const uint FO_DELETE = 3;
    private const ushort FOF_SILENT = 0x0004;
    private const ushort FOF_NOCONFIRMATION = 0x0010;
    private const ushort FOF_ALLOWUNDO = 0x0040;
    private const ushort FOF_NOERRORUI = 0x0400;
    private const ushort DeleteToRecycleBinFlags =
        FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI;

    private const uint SIIGBF_BIGGERSIZEOK = 0x00000001;
    private const uint SIIGBF_THUMBNAILONLY = 0x00000008;

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
        _debounce = new DispatcherTimer(DispatcherPriority.Normal, dispatcher)
        {
            Interval = TimeSpan.FromMilliseconds(600),
        };
        _debounce.Tick += (_, _) => FolderChanged?.Invoke();
        Dispatcher = dispatcher;
    }

    public ReplayLibrary()
        : this(Application.Current?.Dispatcher ?? System.Windows.Threading.Dispatcher.CurrentDispatcher)
    {
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
        using (SHA1 sha = SHA1.Create())
            hash = sha.ComputeHash(Encoding.UTF8.GetBytes(folderPath.ToLowerInvariant()));
        string suffix = BitConverter.ToString(hash).Replace("-", "").Substring(0, 8);
        return Path.Combine(ThumbnailRoot, $"{name}_{suffix}.jpg");
    }

    private static string LegacyThumbnailPathFor(string thumbPath)
    {
        return Path.Combine(Path.GetTempPath(), "thumbnails", Path.GetFileName(thumbPath));
    }


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
                _ = _signal.Release();
                return;
            }

            if (item.Failures >= MaxFailures) return;

            item.PrepareState = StateQueued;
            item.QueuedAsVisible = visible;
            if (visible) _visible.Push(item); else _background.Enqueue(item);
            _ = _signal.Release();
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
                    _ = item.Prepared.TrySetResult(true);
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

        DispatcherPriority priority = visible ? DispatcherPriority.Normal : DispatcherPriority.Background;
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
            _ = Directory.CreateDirectory(Path.GetDirectoryName(item.ThumbPath)!);
            File.Copy(legacy, item.ThumbPath, overwrite: false);
        }
        catch { }
    }

    private async Task<MetaCache?> ReadVideoMetaAsync(ReplayItem item)
    {
        return await Task.Run(() => ReadVideoMeta(item)).ConfigureAwait(false);
    }


    private static MetaCache? ReadVideoMeta(ReplayItem item)
    {
        try
        {
            var info = new FileInfo(item.VideoPath);
            using var reader = new ShellPropertyReader(item.VideoPath);
            TimeSpan duration = ParseShellDuration(reader.Get("System.Media.Duration"));
            int width = ToInt(reader.Get("System.Video.FrameWidth"));
            int height = ToInt(reader.Get("System.Video.FrameHeight"));
            double fps = ParseShellFrameRate(reader.Get("System.Video.FrameRate"));
            return new MetaCache(duration, width, height, fps, info.Length, info.LastWriteTimeUtc.Ticks);
        }
        catch (Exception ex)
        {
            Logger.Warn($"Gallery: reading video properties failed: {ex.Message}");
            return null;
        }
    }

    private static TimeSpan ParseShellDuration(object? value)
    {
        if (value is long or int or uint or ulong or short or ushort)
        {
            try { return TimeSpan.FromTicks(Convert.ToInt64(value, CultureInfo.InvariantCulture)); }
            catch { return TimeSpan.Zero; }
        }
        if (value is string text && TimeSpan.TryParse(text, CultureInfo.InvariantCulture, out TimeSpan parsed))
            return parsed;
        return TimeSpan.Zero;
    }

    private static int ToInt(object? value)
    {
        if (value == null) return 0;
        try { return Convert.ToInt32(value, CultureInfo.InvariantCulture); }
        catch { return 0; }
    }

    private static double ParseShellFrameRate(object? value)
    {
        if (value == null) return 0;
        try { return Convert.ToDouble(value, CultureInfo.InvariantCulture) / 1000.0; }
        catch { return 0; }
    }

    private static string MetaCachePathFor(string thumbPath)
    {
        return thumbPath + ".meta.json";
    }


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
            _ = Directory.CreateDirectory(Path.GetDirectoryName(path)!);
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
        _ = Directory.CreateDirectory(Path.GetDirectoryName(thumbPath)!);

        byte[]? bytes = await Task.Run(() =>
            GetShellThumbnailBytes(item.VideoPath) ?? GetIconThumbnailBytes(item.VideoPath)).ConfigureAwait(false);
        if (bytes == null) return;

        string temporary = thumbPath + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            using (var stream = new FileStream(temporary, FileMode.Create, FileAccess.Write, FileShare.None))
                await stream.WriteAsync(bytes, 0, bytes.Length).ConfigureAwait(false);
            if (File.Exists(thumbPath)) File.Delete(thumbPath);
            File.Move(temporary, thumbPath);
            Logger.Debug($"Gallery: thumbnail generated for '{Path.GetFileName(item.FolderPath)}'.");
        }
        finally
        {
            try { File.Delete(temporary); } catch { }
        }
    }

    private static byte[]? GetShellThumbnailBytes(string videoPath)
    {
        IShellItemImageFactory? factory = null;
        IntPtr hbm = IntPtr.Zero;
        try
        {
            Guid iid = typeof(IShellItemImageFactory).GUID;
            SHCreateItemFromParsingName(videoPath, IntPtr.Zero, ref iid, out factory);
            factory.GetImage(new ShellImageSize { Cx = 480, Cy = 480 },
                SIIGBF_THUMBNAILONLY | SIIGBF_BIGGERSIZEOK, out hbm);
            if (hbm == IntPtr.Zero) return null;

            using (Bitmap bitmap = Bitmap.FromHbitmap(hbm))
            using (var stream = new MemoryStream())
            {
                bitmap.Save(stream, ImageFormat.Jpeg);
                return stream.ToArray();
            }
        }
        catch (Exception ex)
        {
            Logger.Debug($"Gallery: shell thumbnail unavailable for '{videoPath}': {ex.Message}");
            return null;
        }
        finally
        {
            if (hbm != IntPtr.Zero) _ = DeleteObject(hbm);
            if (factory != null)
            {
                try { _ = Marshal.ReleaseComObject(factory); } catch { }
            }
        }
    }

    private static byte[]? GetIconThumbnailBytes(string videoPath)
    {
        try
        {
            using Icon icon = Icon.ExtractAssociatedIcon(videoPath);
            using Bitmap bitmap = icon.ToBitmap();
            using var stream = new MemoryStream();
            bitmap.Save(stream, ImageFormat.Png);
            return stream.ToArray();
        }
        catch
        {
            return null;
        }
    }

    private Task OnUiAsync(Action action, DispatcherPriority priority = DispatcherPriority.Normal)
    {
        var tcs = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        try
        {
            _ = Dispatcher.BeginInvoke(priority, (Action)(() =>
            {
                try { action(); _ = tcs.TrySetResult(true); }
                catch (Exception ex) { _ = tcs.TrySetException(ex); }
            }));
        }
        catch
        {
            // The dispatcher is shutting down; mirror the old TryEnqueue(false) behavior.
            _ = tcs.TrySetResult(true);
        }
        return tcs.Task;
    }

    private void PostToUi(Action action)
    {
        try { _ = Dispatcher.BeginInvoke(action); }
        catch { }
    }

    private static ImageSource CreateThumbnailSource(string thumbPath)
    {
        var image = new BitmapImage();
        image.BeginInit();
        image.CacheOption = BitmapCacheOption.OnLoad;
        image.DecodePixelWidth = 480;
        image.UriSource = new Uri(Path.GetFullPath(thumbPath), UriKind.Absolute);
        image.EndInit();
        image.Freeze();
        return image;
    }

    public async Task<bool> DeleteAsync(ReplayItem item)
    {
        try
        {
            await Task.Run(() => DeleteToRecycleBin(item.FolderPath));
            try { File.Delete(item.ThumbPath); } catch { }
            try { File.Delete(MetaCachePathFor(item.ThumbPath)); } catch { }
            _ = Items.Remove(item);
            return true;
        }
        catch (Exception ex)
        {
            Logger.Warn($"Gallery: deleting '{item.FolderPath}' failed: {ex.Message}");
            return false;
        }
    }

    private static void DeleteToRecycleBin(string path)
    {
        if (!Directory.Exists(path)) return;
        try
        {
            var op = new ShFileOpStruct
            {
                hwnd = IntPtr.Zero,
                wFunc = FO_DELETE,
                pFrom = path + "\0", // SHFileOperation expects a double NUL-terminated list.
                fFlags = DeleteToRecycleBinFlags,
            };
            if (SHFileOperation(ref op) == 0) return;
        }
        catch { }

        // Fallback when the shell refuses the operation (e.g. unavailable recycle bin).
        Directory.Delete(path, recursive: true);
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
            FileSystemEventHandler onChange = (_, _) => PostToUi(() => { _debounce.Stop(); _debounce.Start(); });
            watcher.Created += onChange;
            watcher.Deleted += onChange;
            watcher.Renamed += (_, _) => PostToUi(() => { _debounce.Stop(); _debounce.Start(); });
            watcher.EnableRaisingEvents = true;
            _watcher = watcher;
        }
        catch (Exception ex)
        {
            Logger.Warn($"Gallery: could not watch '{dir}': {ex.Message}");
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct ShellImageSize
    {
        public int Cx;
        public int Cy;
    }

    [ComImport]
    [Guid("BCC18B79-BA16-442F-80C4-8A59C30C463B")]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IShellItemImageFactory
    {
        void GetImage(ShellImageSize size, uint flags, out IntPtr hbitmap);
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct ShFileOpStruct
    {
        public IntPtr hwnd;
        public uint wFunc;
        [MarshalAs(UnmanagedType.LPWStr)] public string pFrom;
        [MarshalAs(UnmanagedType.LPWStr)] public string? pTo;
        public ushort fFlags;
        [MarshalAs(UnmanagedType.Bool)] public bool fAnyOperationsAborted;
        public IntPtr hNameMappings;
        [MarshalAs(UnmanagedType.LPWStr)] public string? lpszProgressTitle;
    }

    [DllImport("shell32.dll", CharSet = CharSet.Unicode, PreserveSig = false)]
    private static extern void SHCreateItemFromParsingName(
        [MarshalAs(UnmanagedType.LPWStr)] string path, IntPtr bindContext, ref Guid riid,
        out IShellItemImageFactory factory);

    [DllImport("shell32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern int SHFileOperation(ref ShFileOpStruct fileOp);

    [DllImport("gdi32.dll")]
    private static extern bool DeleteObject(IntPtr hObject);

    /// <summary>
    /// Minimal net48 replacement for WinRT StorageFile property retrieval. Reads the
    /// same shell properties used by the old implementation through Shell.Application.
    /// </summary>
    private sealed class ShellPropertyReader : IDisposable
    {
        private readonly object _shell;
        private readonly object? _item;

        public ShellPropertyReader(string videoPath)
        {
            Type? shellType = Type.GetTypeFromProgID("Shell.Application");
            if (shellType == null) throw new InvalidOperationException("Shell.Application is unavailable.");
            _shell = Activator.CreateInstance(shellType)!;
            _item = null;

            try
            {
                string folder = Path.GetDirectoryName(videoPath) ?? "";
                string file = Path.GetFileName(videoPath);
                object? ns = shellType.InvokeMember("NameSpace", BindingFlags.InvokeMethod, null, _shell,
                    new object[] { folder });
                _item = ns?.GetType().InvokeMember("ParseName", BindingFlags.InvokeMethod, null, ns,
                    new object[] { file });
                Release(ns);
            }
            catch
            {
                Release(_shell);
                throw;
            }
        }

        public object? Get(string canonicalName)
        {
            if (_item == null) return null;
            try
            {
                return _item.GetType().InvokeMember("ExtendedProperty", BindingFlags.InvokeMethod, null, _item,
                    new object[] { canonicalName });
            }
            catch
            {
                return null;
            }
        }

        public void Dispose()
        {
            Release(_item);
            Release(_shell);
        }

        private static void Release(object? comObject)
        {
            if (comObject == null) return;
            try { _ = Marshal.ReleaseComObject(comObject); } catch { }
        }
    }
}
