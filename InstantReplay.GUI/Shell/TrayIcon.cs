using System.Numerics;
using System.Runtime.InteropServices;
using Microsoft.UI.Composition;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Hosting;
using Microsoft.UI.Xaml.Media;
using Windows.Graphics;
using Windows.UI;
using InstantReplay.Services;
using NotifyContextMenuMode = H.NotifyIcon.ContextMenuMode;
using NotifyMouseEvent = H.NotifyIcon.Core.MouseEvent;
using NotifyNotificationIcon = H.NotifyIcon.Core.NotificationIcon;
using NotifyPopupActivation = H.NotifyIcon.Core.PopupActivationMode;
using TrayIconControl = H.NotifyIcon.TaskbarIcon;

namespace InstantReplay.GUI.Shell;

/// <summary>
/// System tray icon backed by H.NotifyIcon.WinUI. The context menu is a real WinUI
/// <see cref="MenuFlyout"/> hosted in the library's transparent helper window:
///  * the flyout surface uses a true desktop-acrylic system backdrop (not Mica and not
///    the in-app acrylic brush, which has nothing to sample inside a transparent helper window),
///  * the popup opens with a composition "drop-down" reveal: an inset clip that grows away from
///    the taskbar, a soft scale/fade of the surface and a staggered slide-in of every row,
///  * rows are rounded, accent-tinted and highlighted like the Windows 11 shell menus.
/// All animations run on the compositor thread, so they stay smooth while the app is busy.
/// </summary>
public sealed class TrayIcon : IDisposable
{
    public const int IconInfo = 1, IconWarning = 2, IconError = 3;

    /// <summary>Retained so older window-message plumbing still compiles.</summary>
    public const uint WM_TRAYICON = 0x8001;

    // Motion tokens (Fluent "decelerate" curve, same family WPF-UI uses for flyouts).
    private static readonly Vector2 EaseA = new(0.10f, 0.90f);
    private static readonly Vector2 EaseB = new(0.20f, 1.00f);
    private const int SurfaceMs = 240;
    private const int RevealMs = 280;
    private const int RowMs = 220;
    private const int RowStaggerMs = 26;
    private const int RowDelayMs = 40;
    // Long menus must not feel sluggish: rows past this index all share the last delay slot.
    private const int RowStaggerCap = 6;

    private readonly TrayIconControl _icon;
    private readonly System.Drawing.Icon _baseIcon;
    private readonly MenuFlyout _menu;
    private bool _disposed;

    public event Action? DoubleClicked;
    public event Action? BalloonClicked;

    public Func<List<TrayMenuItem>>? MenuItemsProvider { get; set; }

    [StructLayout(LayoutKind.Sequential)]
    private struct NativePoint { public int X, Y; }

    [DllImport("user32.dll")]
    private static extern bool GetCursorPos(out NativePoint point);

    public TrayIcon(IntPtr hwnd, string iconPath, string tooltip)
    {
        _baseIcon = new System.Drawing.Icon(iconPath);

        _menu = new MenuFlyout
        {
            MenuFlyoutPresenterStyle = CreatePresenterStyle(),
            AreOpenCloseAnimationsEnabled = true,
            ShouldConstrainToRootBounds = false,
        };

        try
        {
            // Real, behind-the-window acrylic. Falls back to the presenter's own tint if the
            // runtime refuses a backdrop on this popup.
            _menu.SystemBackdrop = new DesktopAcrylicBackdrop();
        }
        catch (Exception ex)
        {
            Logger.Warn($"Tray menu backdrop unavailable: {ex.Message}");
        }

        _menu.Opening += (_, _) => RebuildMenu();
        _menu.Opened += (_, _) => PlayOpenAnimation();

        _icon = new TrayIconControl
        {
            Icon = _baseIcon,
            ToolTipText = tooltip,
            MenuActivation = NotifyPopupActivation.RightClick,
            // A dedicated transparent window gives real WinUI acrylic, rounded corners and
            // animations even for unpackaged apps; PopupMenu mode falls back to a Win32 menu.
            ContextMenuMode = NotifyContextMenuMode.SecondWindow,
            ContextFlyout = _menu,
        };

        _icon.ForceCreate(enablesEfficiencyMode: false);

        try
        {
            // WinRT events are disabled in this library build, but the message
            // window still surfaces raw mouse events, including balloon clicks.
            var window = _icon.TrayIcon.MessageWindow;
            if (window != null)
            {
                window.MouseEventReceived += (_, args) =>
                {
                    switch (args.MouseEvent)
                    {
                        case NotifyMouseEvent.IconDoubleClick:
                        case NotifyMouseEvent.IconLeftDoubleClick:
                            DoubleClicked?.Invoke();
                            break;
                        case NotifyMouseEvent.BalloonToolTipClicked:
                            BalloonClicked?.Invoke();
                            break;
                    }
                };
            }
        }
        catch (Exception ex)
        {
            Logger.Warn($"Tray icon message hook failed: {ex.Message}");
        }

        Logger.Info("Tray icon added (H.NotifyIcon).");
    }

    public void SetIcon(IntPtr icon)
    {
        if (_disposed) return;
        try
        {
            if (icon == IntPtr.Zero)
            {
                _icon.Icon = _baseIcon;
                return;
            }

            using var borrowed = System.Drawing.Icon.FromHandle(icon);
            _icon.Icon = (System.Drawing.Icon)borrowed.Clone();
        }
        catch (Exception ex)
        {
            Logger.Warn($"Tray icon update failed: {ex.Message}");
        }
    }

    public void ShowBalloonTip(string title, string message, int iconFlag)
    {
        if (_disposed) return;
        NotifyNotificationIcon icon = iconFlag switch
        {
            IconWarning => NotifyNotificationIcon.Warning,
            IconError => NotifyNotificationIcon.Error,
            _ => NotifyNotificationIcon.Info,
        };
        try
        {
            _icon.ShowNotification(title, message, icon);
        }
        catch (Exception ex)
        {
            Logger.Warn($"Tray notification failed: {ex.Message}");
        }
    }

    /// <summary>The old Win32 path received tray messages in the window proc;
    /// H.NotifyIcon owns its own message window now, so this is a no-op.</summary>
    public void HandleMessage(IntPtr lParam) { }

    // ------------------------------------------------------------------ menu content

    private void RebuildMenu()
    {
        _menu.Items.Clear();

        var items = MenuItemsProvider?.Invoke();
        if (items == null) return;

        Brush? accent = null;
        try { accent = Application.Current.Resources["AccentBrush"] as Brush; } catch { }

        foreach (var item in items)
        {
            if (item.IsSeparator)
            {
                var separator = new MenuFlyoutSeparator { Margin = new Thickness(6, 3, 6, 3) };
                _menu.Items.Add(separator);
                HideForReveal(separator);
                continue;
            }

            MenuFlyoutItemBase entry;
            if (item.IsCheckable)
            {
                entry = new ToggleMenuFlyoutItem
                {
                    Text = item.Text,
                    IsChecked = item.IsChecked,
                    IsEnabled = item.IsEnabled,
                };
            }
            else
            {
                entry = new MenuFlyoutItem
                {
                    Text = item.Text,
                    IsEnabled = item.IsEnabled,
                    KeyboardAcceleratorTextOverride = item.Shortcut,
                };
            }

            // Rounded, roomy rows in the style of the Windows 11 shell menus.
            entry.Padding = new Thickness(11, 9, 11, 9);
            entry.CornerRadius = new CornerRadius(8);
            entry.Margin = new Thickness(0, 1, 0, 1);

            if (entry is MenuFlyoutItem clickable)
            {
                clickable.Click += (_, _) => item.OnClick?.Invoke();
            }

            if (!string.IsNullOrEmpty(item.Glyph))
            {
                var icon = new FontIcon
                {
                    Glyph = item.Glyph,
                    FontSize = 16,
                    Foreground = accent,
                };
                if (entry is MenuFlyoutItem menuItem)
                    menuItem.Icon = icon;
            }

            _menu.Items.Add(entry);
            HideForReveal(entry);
        }
    }

    /// <summary>Rows start invisible on the compositor so they never flash before the reveal.</summary>
    private static void HideForReveal(UIElement element)
    {
        try { ElementCompositionPreview.GetElementVisual(element).Opacity = 0f; }
        catch { /* the open animation falls back to making everything visible */ }
    }

    // ------------------------------------------------------------------ animation

    private void PlayOpenAnimation()
    {
        var elements = new List<UIElement>();
        foreach (var item in _menu.Items)
            if (item is UIElement element) elements.Add(element);

        try
        {
            if (elements.Count == 0) return;

            var presenter = FindPresenter(elements[0]);
            var compositor = ElementCompositionPreview.GetElementVisual(elements[0]).Compositor;
            var ease = compositor.CreateCubicBezierEasingFunction(EaseA, EaseB);

            bool opensUpward = IsInLowerHalfOfScreen();

            if (presenter != null)
                AnimateSurface(presenter, compositor, ease, opensUpward);

            for (int i = 0; i < elements.Count; i++)
            {
                try
                {
                    AnimateRow(elements[i], compositor, ease, i, opensUpward);
                }
                catch
                {
                    // Never leave a single row stranded at opacity 0.
                    try { ElementCompositionPreview.GetElementVisual(elements[i]).Opacity = 1f; } catch { }
                }
            }
        }
        catch (Exception ex)
        {
            Logger.Debug($"Tray menu animation skipped: {ex.Message}");
            ShowAllRows(elements);
        }
    }

    private static void AnimateSurface(FrameworkElement presenter, Compositor compositor,
        CubicBezierEasingFunction ease, bool opensUpward)
    {
        var visual = ElementCompositionPreview.GetElementVisual(presenter);
        float width = (float)presenter.ActualWidth;
        float height = (float)presenter.ActualHeight;
        if (width <= 0 || height <= 0) return;
        // A previous open may still hold its reveal clip if it was reopened quickly.
        visual.StopAnimation("Opacity");
        visual.StopAnimation("Scale");
        visual.Opacity = 0f;
        visual.Scale = new Vector3(0.94f, 0.90f, 1f);

        // Grow away from the taskbar edge.
        visual.CenterPoint = new Vector3(width / 2f, opensUpward ? height : 0f, 0f);

        var fade = compositor.CreateScalarKeyFrameAnimation();
        fade.InsertKeyFrame(0f, 0f);
        fade.InsertKeyFrame(1f, 1f, ease);
        fade.Duration = TimeSpan.FromMilliseconds(SurfaceMs * 0.7);

        var scale = compositor.CreateVector3KeyFrameAnimation();
        scale.InsertKeyFrame(0f, new Vector3(0.94f, 0.90f, 1f));
        scale.InsertKeyFrame(1f, Vector3.One, ease);
        scale.Duration = TimeSpan.FromMilliseconds(SurfaceMs);

        // Drop-down reveal: the visible window of the surface expands from the anchor edge.
        var clip = compositor.CreateInsetClip();
        visual.Clip = clip;
        var reveal = compositor.CreateScalarKeyFrameAnimation();
        reveal.InsertKeyFrame(0f, height * 0.55f);
        reveal.InsertKeyFrame(1f, 0f, ease);
        reveal.Duration = TimeSpan.FromMilliseconds(RevealMs);

        var batch = compositor.CreateScopedBatch(CompositionBatchTypes.Animation);
        visual.StartAnimation("Opacity", fade);
        visual.StartAnimation("Scale", scale);
        clip.StartAnimation(opensUpward ? "TopInset" : "BottomInset", reveal);
        batch.End();
        batch.Completed += (_, _) =>
        {
            try { visual.Clip = null; visual.Opacity = 1f; visual.Scale = Vector3.One; } catch { }
        };
    }

    private static void AnimateRow(UIElement row, Compositor compositor, CubicBezierEasingFunction ease,
        int index, bool opensUpward)
    {
        var visual = ElementCompositionPreview.GetElementVisual(row);
        ElementCompositionPreview.SetIsTranslationEnabled(row, true);
        // Restart from a clean slate: a row revealed in an earlier open keeps the old
        // animation's final values otherwise.
        visual.StopAnimation("Opacity");
        visual.StopAnimation("Translation.Y");
        visual.Opacity = 0f;
        visual.Properties.InsertVector3("Translation",
            new Vector3(0f, opensUpward ? 12f : -12f, 0f));

        var delay = TimeSpan.FromMilliseconds(
            RowDelayMs + RowStaggerMs * Math.Min(index, RowStaggerCap));

        var fade = compositor.CreateScalarKeyFrameAnimation();
        fade.InsertKeyFrame(0f, 0f);
        fade.InsertKeyFrame(1f, 1f, ease);
        fade.Duration = TimeSpan.FromMilliseconds(RowMs);
        fade.DelayTime = delay;
        fade.DelayBehavior = AnimationDelayBehavior.SetInitialValueBeforeDelay;

        // Rows slide toward their resting place from the side the menu grows from.
        var slide = compositor.CreateScalarKeyFrameAnimation();
        slide.InsertKeyFrame(0f, opensUpward ? 12f : -12f);
        slide.InsertKeyFrame(1f, 0f, ease);
        slide.Duration = TimeSpan.FromMilliseconds(RowMs + 40);
        slide.DelayTime = delay;
        slide.DelayBehavior = AnimationDelayBehavior.SetInitialValueBeforeDelay;

        visual.StartAnimation("Opacity", fade);
        visual.StartAnimation("Translation.Y", slide);
    }

    private static void ShowAllRows(IEnumerable<UIElement> elements)
    {
        foreach (var element in elements)
        {
            try { ElementCompositionPreview.GetElementVisual(element).Opacity = 1f; } catch { }
        }
    }

    private static MenuFlyoutPresenter? FindPresenter(DependencyObject start)
    {
        DependencyObject? current = VisualTreeHelper.GetParent(start);
        while (current != null)
        {
            if (current is MenuFlyoutPresenter presenter) return presenter;
            current = VisualTreeHelper.GetParent(current);
        }
        return null;
    }

    /// <summary>Tray menus open upward when the taskbar is at the bottom of the screen.</summary>
    private static bool IsInLowerHalfOfScreen()
    {
        try
        {
            if (!GetCursorPos(out var point)) return true;
            var area = DisplayArea.GetFromPoint(new PointInt32(point.X, point.Y), DisplayAreaFallback.Nearest);
            var work = area.WorkArea;
            return point.Y > work.Y + work.Height / 2;
        }
        catch
        {
            return true;
        }
    }

    // ------------------------------------------------------------------ styling

    /// <summary>
    /// The presenter is transparent so the flyout's desktop-acrylic backdrop shows through;
    /// a hairline light border and generous rounding give it the Windows 11 flyout look.
    /// </summary>
    private static Style CreatePresenterStyle()
    {
        var style = new Style(typeof(MenuFlyoutPresenter));
        style.Setters.Add(new Setter(Control.BackgroundProperty,
            new SolidColorBrush(Color.FromArgb(0x38, 0x1C, 0x1C, 0x22))));
        style.Setters.Add(new Setter(Control.BorderBrushProperty,
            new SolidColorBrush(Color.FromArgb(0x30, 0xFF, 0xFF, 0xFF))));
        style.Setters.Add(new Setter(Control.BorderThicknessProperty, new Thickness(1)));
        style.Setters.Add(new Setter(Control.PaddingProperty, new Thickness(6, 7, 6, 7)));
        style.Setters.Add(new Setter(Control.CornerRadiusProperty, new CornerRadius(12)));
        style.Setters.Add(new Setter(FrameworkElement.MinWidthProperty, 252.0));
        style.Setters.Add(new Setter(FrameworkElement.MaxWidthProperty, 340.0));
        return style;
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        try { _icon.Dispose(); } catch { }
        try { _baseIcon.Dispose(); } catch { }
    }
}
