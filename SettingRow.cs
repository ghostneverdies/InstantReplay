using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace InstantReplay;

public sealed class SettingRow : ContentControl
{
    public static readonly DependencyProperty LabelProperty =
        DependencyProperty.Register(nameof(Label), typeof(string), typeof(SettingRow), new PropertyMetadata(string.Empty));

    public string Label
    {
        get => (string)GetValue(LabelProperty);
        set => SetValue(LabelProperty, value);
    }

    public SettingRow()
    {
        DefaultStyleKey = typeof(SettingRow);
    }
}
