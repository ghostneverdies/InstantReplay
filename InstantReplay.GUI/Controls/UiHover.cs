namespace InstantReplay.GUI.Controls;

/// <summary>
/// Lightweight hover-scale behavior used by the gallery cards. The original
/// WinUI composition animation is replaced with a WPF ScaleTransform so the
/// project no longer depends on Microsoft.UI.Composition.
/// </summary>
public static class UiHover
{
    private static readonly HashSet<FrameworkElement> Wired = new();

    public static readonly DependencyProperty IsEnabledProperty =
        DependencyProperty.RegisterAttached(
            "IsEnabled",
            typeof(bool),
            typeof(UiHover),
            new PropertyMetadata(false, OnIsEnabledChanged));

    public static bool GetIsEnabled(DependencyObject obj)
    {
        return (bool)obj.GetValue(IsEnabledProperty);
    }

    public static void SetIsEnabled(DependencyObject obj, bool value)
    {
        obj.SetValue(IsEnabledProperty, value);
    }


    private static void OnIsEnabledChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
    {
        if (d is not FrameworkElement element || e.NewValue is not true) return;

        element.Loaded += (_, _) => Wire(element);
        if (element.IsLoaded) Wire(element);
    }

    private static void Wire(FrameworkElement element)
    {
        if (!GetIsEnabled(element)) return;
        if (!Wired.Add(element)) return;

        element.MouseEnter += (_, _) => Animate(element, 1.04);
        element.MouseLeave += (_, _) => Animate(element, 1.0);
        element.IsVisibleChanged += (_, args) =>
        {
            if (args.NewValue is not true) Animate(element, 1.0);
        };
    }

    private static void Animate(FrameworkElement element, double scale)
    {
        try
        {
            if (element is Button { IsEnabled: false }) return;
            if (!GetIsEnabled(element)) return;

            var transform = element.RenderTransform as ScaleTransform;
            if (transform == null)
            {
                transform = new ScaleTransform(1, 1);
                element.RenderTransformOrigin = new Point(0.5, 0.5);
                element.RenderTransform = transform;
            }

            var animation = new DoubleAnimation(scale, TimeSpan.FromMilliseconds(110))
            {
                EasingFunction = new QuadraticEase { EasingMode = EasingMode.EaseOut },
            };

            transform.BeginAnimation(ScaleTransform.ScaleXProperty, animation);
            transform.BeginAnimation(ScaleTransform.ScaleYProperty, animation);
        }
        catch
        {
            // Hover feedback is cosmetic; never let it break layout.
        }
    }
}
