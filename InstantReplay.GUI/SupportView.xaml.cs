
namespace InstantReplay.GUI;

public sealed partial class SupportView : System.Windows.Controls.UserControl
{
    public event EventHandler? CloseRequested;

    public SupportView()
    {
        InitializeComponent();
    }

    private void CloseDonateButton_Click(object sender, RoutedEventArgs e)
    {
        CloseRequested?.Invoke(this, EventArgs.Empty);
    }


    private void CopyAddressButton_Click(object sender, RoutedEventArgs e)
    {
        System.Windows.Clipboard.SetText(WalletAddressText.Text);
        CopyAddressButton.Content = "COPIED!";
        var revertTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(1.5) };
        revertTimer.Tick += (_, _) => { revertTimer.Stop(); CopyAddressButton.Content = "COPY ADDRESS"; };
        revertTimer.Start();
    }
}
