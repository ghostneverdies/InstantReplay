using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.ApplicationModel.DataTransfer;

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
        var package = new DataPackage();
        package.SetText(WalletAddressText.Text);
        Clipboard.SetContent(package);
        CopyAddressButton.Content = "COPIED!";
        var revertTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(1.5) };
        revertTimer.Tick += (_, _) => { revertTimer.Stop(); CopyAddressButton.Content = "COPY ADDRESS"; };
        revertTimer.Start();
    }
}
