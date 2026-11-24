using System;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Threading;

namespace InstantReplay.GUI;

public sealed partial class SupportView : UserControl
{
    public event EventHandler? CloseRequested;

    public SupportView()
    {
        InitializeComponent();
    }

    private void CloseDonateButton_Click(object sender, RoutedEventArgs e) => CloseRequested?.Invoke(this, EventArgs.Empty);

    private void CopyAddressButton_Click(object sender, RoutedEventArgs e)
    {
        try { Clipboard.SetText(WalletAddressText.Text); }
        catch (System.Runtime.InteropServices.COMException) { return; }

        CopyAddressButton.Content = "COPIED!";
        var revertTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(1.5) };
        revertTimer.Tick += (_, _) => { revertTimer.Stop(); CopyAddressButton.Content = "COPY ADDRESS"; };
        revertTimer.Start();
    }
}
