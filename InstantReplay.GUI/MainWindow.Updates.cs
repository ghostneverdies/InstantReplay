using System.Runtime.InteropServices;
using System.Text.Json;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media.Animation;
using InstantReplay.Models;
using InstantReplay.Services;

namespace InstantReplay.GUI;

public partial class MainWindow
{
    private UpdateService _updates = null!;
    private DispatcherTimer? _updateTimer;
    private readonly CancellationTokenSource _updateLifetime = new();
    private CancellationTokenSource? _downloadCancellation;
    private bool _updateBusy;
    private bool _updateReminderOpen;
    private string? _updateMessage;
    private bool _updateMessageIsError;
    private Storyboard? _updateSpinStoryboard;

    private void SetUpdateSpinner(bool spinning)
    {
        if (!spinning)
        {
            _updateSpinStoryboard?.Stop();
            UpdateIconRotation.Angle = 0;
            return;
        }
        _updateSpinStoryboard ??= new Storyboard();
        if (_updateSpinStoryboard.Children.Count == 0)
        {
            var animation = new DoubleAnimation
            {
                From = 0,
                To = 360,
                Duration = TimeSpan.FromMilliseconds(750),
                RepeatBehavior = RepeatBehavior.Forever,
                EnableDependentAnimation = true,
            };
            Storyboard.SetTarget(animation, UpdateIconRotation);
            Storyboard.SetTargetProperty(animation, "Angle");
            _updateSpinStoryboard.Children.Add(animation);
        }
        _updateSpinStoryboard.Begin();
    }

    private void InitializeUpdates()
    {
        _updates = new UpdateService();
        try
        {
            if (File.Exists(UpdateService.ResultPath))
            {
                var result = System.Text.Json.JsonSerializer.Deserialize<UpdateInstallResult>(File.ReadAllText(UpdateService.ResultPath));
                if (result is not null)
                {
                    _updateMessage = result.Success ? $"Updated to version {result.Version}." : $"Update was not completed: {result.Message}";
                    _updateMessageIsError = !result.Success;
                    if (!result.Success) Logger.Warn(_updateMessage);
                }
                File.Delete(UpdateService.ResultPath);
            }
        }
        catch (Exception ex) { Logger.Warn($"Update result could not be read: {ex.Message}"); }
        RefreshUpdatesPage();
        _updateTimer = new DispatcherTimer { Interval = TimeSpan.FromMinutes(1) };
        _updateTimer.Tick += async (_, _) => await CheckUpdatesAsync(false);
        _updateTimer.Start();
        RootGrid.Loaded += (_, _) => _dq.TryEnqueue(async () => await CheckUpdatesAsync(false));
    }

    private void RefreshUpdatesPage()
    {
        if (_updates is null) return;
        InstalledVersionLabel.Text = $"Version {_updates.CurrentVersion.ToString(3)}";
        UpdateCheck state = _updates.State;
        bool available = _updates.HasUpdate;
        UpdateHeading.Text = available ? "An update is available" : state.Release is null ? "Keep Instant Replay up to date" : "You Are Using Latest Version Of The App";
        UpdateDescription.Text = available ? $"Version {state.Release!.Version} is ready to download." : state.Release is null
            ? "Check for the latest stable release on GitHub." : $"Latest release: {state.Release.Version}.";
        CheckUpdatesButton.IsEnabled = !_updateBusy;
        InstallUpdateButton.Visibility = available ? Visibility.Visible : Visibility.Collapsed;
        InstallUpdateButton.IsEnabled = !_updateBusy && state.Error is null;
        ReleaseNotesCard.Visibility = state.Release is not null && available ? Visibility.Visible : Visibility.Collapsed;
        UpdateReleaseNotes.Text = string.IsNullOrWhiteSpace(state.Release?.Notes) ? "No release notes were provided." : state.Release.Notes;
        LastUpdateCheckLabel.Text = state.LastAttemptUtc == DateTimeOffset.MinValue ? "Not checked yet" : $"Last checked {state.LastAttemptUtc.ToLocalTime():g}";
        UpdateStatusBar.IsOpen = _updateMessage is not null || state.Error is not null;
        UpdateStatusBar.Severity = _updateMessage is not null ? (_updateMessageIsError ? InfoBarSeverity.Error : InfoBarSeverity.Informational) : InfoBarSeverity.Error;
        UpdateStatusBar.Message = _updateMessage ?? state.Error ?? "";
    }

    private async Task CheckUpdatesAsync(bool manual)
    {
        if (_updateBusy || _updateReminderOpen || _isClosingForReal) return;
        if (!manual && !_updates.IsDue(DateTimeOffset.UtcNow))
        {
            await ShowUpdateReminderAsync();
            return;
        }
        _updateBusy = true;
        SetUpdateSpinner(true);
        _updateMessage = "Checking for updates…";
        _updateMessageIsError = false;
        UpdateProgress.Visibility = Visibility.Visible;
        UpdateProgress.IsIndeterminate = true;
        RefreshUpdatesPage();
        try
        {
            await _updates.CheckAsync(_updateLifetime.Token);
            try
            {
                await _updates.UpdateGameCatalogAsync(_updateLifetime.Token);
                _engine.ReloadGameRules();
                RefreshGamesPage();
            }
            catch (Exception ex)
            {
                Logger.Warn($"Game database update failed: {ex.Message}");
            }
            _updateMessage = null;
            if (!manual && _updates.HasUpdate) NavUpdatesItem.InfoBadge = new InfoBadge { Value = 1 };
            else NavUpdatesItem.InfoBadge = null;
        }
        catch (OperationCanceledException) { }
        finally
        {
            _updateBusy = false;
            SetUpdateSpinner(false);
            UpdateProgress.Visibility = Visibility.Collapsed;
            if (!_isClosingForReal) RefreshUpdatesPage();
        }
        await ShowUpdateReminderAsync();
    }

    private async void CheckUpdates_Click(object sender, RoutedEventArgs e) => await CheckUpdatesAsync(true);

    private async Task ShowUpdateReminderAsync()
    {
        if (_updates is null || _updateBusy || _updateReminderOpen || _isClosingForReal || _isHidden
            || RootGrid.XamlRoot is null || GetForegroundWindow() != _hwnd || !_updates.IsReminderDue(DateTimeOffset.UtcNow)) return;
        _updateReminderOpen = true;
        bool accepted = false;
        try
        {
            var dialog = new ContentDialog
            {
                Title = "An update is available",
                Content = $"Instant Replay {_updates.State.Release!.Version} is available. Would you like to update now?\n\nThe app will download the update, close, install it, and reopen automatically. Save any replay you want to keep first: restarting clears the current buffer. Windows may ask for administrator approval.",
                PrimaryButtonText = "Update now",
                CloseButtonText = "Not now",
                DefaultButton = ContentDialogButton.Close,
                XamlRoot = RootGrid.XamlRoot,
                RequestedTheme = RootGrid.RequestedTheme
            };
            dialog.Opened += (_, _) => _updates.MarkReminderShown(DateTimeOffset.UtcNow);
            accepted = await dialog.ShowAsync() == ContentDialogResult.Primary;
        }
        catch (Exception ex) { Logger.Debug($"Update reminder could not be shown: {ex.Message}"); }
        finally { _updateReminderOpen = false; }
        if (accepted && !_isClosingForReal)
        {
            NavView.SelectedItem = NavUpdatesItem;
            await InstallUpdateAsync(true);
        }
    }

    private async void InstallUpdate_Click(object sender, RoutedEventArgs e) => await InstallUpdateAsync(false);

    private async Task InstallUpdateAsync(bool confirmed)
    {
        if (_updateBusy || !_updates.HasUpdate || _isClosingForReal) return;
        _updateBusy = true;
        RefreshUpdatesPage();
        string? directory = null;
        bool handoffStarted = false;
        try
        {
            if (!confirmed)
            {
                var dialog = new ContentDialog
                {
                    Title = "Install update?",
                    Content = "Instant Replay will download the update, close, install it, and reopen automatically. The current replay buffer will be cleared. Save any replay you want to keep before continuing. Windows may ask for administrator approval.",
                    PrimaryButtonText = "Download and install",
                    CloseButtonText = "Cancel",
                    DefaultButton = ContentDialogButton.Close,
                    XamlRoot = RootGrid.XamlRoot,
                    RequestedTheme = RootGrid.RequestedTheme
                };
                if (await dialog.ShowAsync() != ContentDialogResult.Primary) return;
            }
            _downloadCancellation = CancellationTokenSource.CreateLinkedTokenSource(_updateLifetime.Token);
            _updateMessage = "Checking the release before downloading…";
            _updateMessageIsError = false;
            UpdateProgress.Visibility = Visibility.Visible;
            UpdateProgress.IsIndeterminate = true;
            CancelUpdateButton.Visibility = Visibility.Visible;
            RefreshUpdatesPage();
            await _updates.CheckAsync(_downloadCancellation.Token);
            if (_updates.State.Error is { } error) throw new InvalidOperationException(error);
            if (!_updates.HasUpdate) { _updateMessage = "You're already up to date."; return; }
            UpdateRelease release = _updates.State.Release!;
            directory = Path.Combine(Path.GetTempPath(), "InstantReplay-Updates", Guid.NewGuid().ToString("N"));
            UpdateProgress.IsIndeterminate = false;
            UpdateProgress.Value = 0;
            var progress = new Progress<double>(value =>
            {
                UpdateProgress.Value = value;
                UpdateStatusBar.Message = $"Downloading version {release.Version}… {value:0}%";
            });
            _updateMessage = $"Downloading version {release.Version}…";
            RefreshUpdatesPage();
            string installer = await _updates.DownloadAsync(release, directory, progress, _downloadCancellation.Token);
            CancelUpdateButton.Visibility = Visibility.Collapsed;
            _updateMessage = "Installer verified. Preparing to restart…";
            RefreshUpdatesPage();
            await UpdateService.StartInstallerAsync(installer, release, _downloadCancellation.Token);
            handoffStarted = true;
            ExitApplicationAsync();
        }
        catch (OperationCanceledException)
        {
            _updateMessage = "Update download cancelled. Nothing was installed.";
            _updateMessageIsError = false;
        }
        catch (Exception ex)
        {
            _updateMessage = $"Update could not be installed: {ex.Message}";
            _updateMessageIsError = true;
            Logger.Error("Update installation failed", ex);
        }
        finally
        {
            _downloadCancellation?.Dispose();
            _downloadCancellation = null;
            _updateBusy = false;
            CancelUpdateButton.Visibility = Visibility.Collapsed;
            UpdateProgress.Visibility = Visibility.Collapsed;
            if (!handoffStarted && directory is not null)
            {
                try
                {
                    foreach (string name in new[] { UpdateService.InstallerName, UpdateService.InstallerName + ".partial", "install-update.ps1", "install-update.json", "ready" })
                        File.Delete(Path.Combine(directory, name));
                    Directory.Delete(directory);
                }
                catch (Exception ex) { Logger.Debug($"Update cleanup: {ex.Message}"); }
            }
            if (!_isClosingForReal) RefreshUpdatesPage();
        }
    }

    private void CancelUpdate_Click(object sender, RoutedEventArgs e) => _downloadCancellation?.Cancel();
}
