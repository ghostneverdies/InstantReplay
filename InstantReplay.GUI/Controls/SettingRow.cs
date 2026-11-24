using System.Windows;
using System.Windows.Controls;

namespace InstantReplay.GUI.Controls;

public sealed class SettingRow : ContentControl
{
    public static readonly DependencyProperty LabelProperty =
        DependencyProperty.Register(nameof(Label), typeof(string), typeof(SettingRow), new PropertyMetadata(string.Empty));

    public string Label
    {
        get => (string)GetValue(LabelProperty);
        set => SetValue(LabelProperty, value);
    }

    static SettingRow()
    {
        DefaultStyleKeyProperty.OverrideMetadata(typeof(SettingRow), new FrameworkPropertyMetadata(typeof(SettingRow)));
    }
}
