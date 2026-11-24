using System;
using System.Collections.Generic;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Media.Animation;

namespace InstantReplay.GUI.Controls;

/// <summary>
/// Opt-in hover "grow" for buttons (controls:UiHover.IsEnabled="True"). The WinUI build ran a spring on the
/// composition visual; WPF has no composition layer, so a ScaleTransform is eased instead.
/// </summary>
public static class UiHover
{
    private const double HoverScale = 1.04;
    private static readonly HashSet<FrameworkElement> Wired = new();

    public static readonly DependencyProperty IsEnabledProperty =
        DependencyProperty.RegisterAttached(
            "IsEnabled",
            typeof(bool),
            typeof(UiHover),
            new PropertyMetadata(false, OnIsEnabledChanged));

    public static bool GetIsEnabled(DependencyObject obj) => (bool)obj.GetValue(IsEnabledProperty);
    public static void SetIsEnabled(DependencyObject obj, bool value) => obj.SetValue(IsEnabledProperty, value);

    private static void OnIsEnabledChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
    {
        if (!(d is FrameworkElement element) || !(e.NewValue is bool enabled) || !enabled) return;
        element.Loaded += (_, _) => Wire(element);
        if (element.IsLoaded) Wire(element);
    }

    private static void Wire(FrameworkElement element)
    {
        if (!GetIsEnabled(element)) return;
        if (!Wired.Add(element)) return;
        element.MouseEnter += (_, _) => Animate(element, true);
        element.MouseLeave += (_, _) => Animate(element, false);
    }

    private static void Animate(FrameworkElement element, bool hover)
    {
        try
        {
            if (element is Button button && !button.IsEnabled) return;
            if (!GetIsEnabled(element)) return;

            if (!(element.RenderTransform is ScaleTransform scale) || scale.IsFrozen)
            {
                scale = new ScaleTransform(1, 1);
                element.RenderTransform = scale;
                element.RenderTransformOrigin = new Point(0.5, 0.5);
            }

            double target = hover ? HoverScale : 1.0;
            var ease = new CubicEase { EasingMode = EasingMode.EaseOut };
            var animation = new DoubleAnimation(target, TimeSpan.FromMilliseconds(120)) { EasingFunction = ease };
            scale.BeginAnimation(ScaleTransform.ScaleXProperty, animation);
            scale.BeginAnimation(ScaleTransform.ScaleYProperty, animation);
        }
        catch
        {
        }
    }
}
