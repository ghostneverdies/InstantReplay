using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace InstantReplay.GUI.Controls;

public static class UiHover
{
    private static readonly HashSet<FrameworkElement> Wired = new();
    private static Microsoft.UI.Composition.Compositor? _compositor;
    private static Microsoft.UI.Composition.SpringVector3NaturalMotionAnimation? _grow;
    private static Microsoft.UI.Composition.SpringVector3NaturalMotionAnimation? _shrink;

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
        if (d is not Button button || e.NewValue is not true) return;

        button.Loaded += (_, _) => Wire(button);
        if (button.IsLoaded) Wire(button);
    }

    private static void Wire(Button button)
    {
        if (!GetIsEnabled(button)) return;
        if (!Wired.Add(button)) return;
        button.PointerEntered += (_, _) => Animate(button, true);
        button.PointerExited += (_, _) => Animate(button, false);
    }

    private static void Animate(FrameworkElement element, bool hover)
    {
        try
        {
            if (element is Button { IsEnabled: false }) return;
            if (!GetIsEnabled(element)) return;

            _compositor ??= Microsoft.UI.Xaml.Media.CompositionTarget.GetCompositorForCurrentThread();
            if (hover)
                _grow ??= CreateSpring(new System.Numerics.Vector3(1.04f, 1.04f, 1f));
            else
                _shrink ??= CreateSpring(System.Numerics.Vector3.One);

            var visual = Microsoft.UI.Xaml.Hosting.ElementCompositionPreview.GetElementVisual(element);
            visual.CenterPoint = new System.Numerics.Vector3(
                (float)element.ActualWidth / 2,
                (float)element.ActualHeight / 2,
                0);
            visual.StartAnimation("Scale", hover ? _grow! : _shrink!);
        }
        catch
        {
        }
    }

    private static Microsoft.UI.Composition.SpringVector3NaturalMotionAnimation CreateSpring(System.Numerics.Vector3 final)
    {
        var anim = _compositor!.CreateSpringVector3Animation();
        anim.Target = "Scale";
        anim.FinalValue = final;
        anim.DampingRatio = 0.9f;
        anim.Period = TimeSpan.FromMilliseconds(65);
        return anim;
    }
}
