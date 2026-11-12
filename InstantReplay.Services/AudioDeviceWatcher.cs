using System.Runtime.InteropServices;
using InstantReplay.Native;

namespace InstantReplay.Services;

public sealed class AudioDeviceWatcher : IMMNotificationClient, IDisposable
{
    public event Action? DevicesChanged;

    private IMMDeviceEnumerator? _enumerator;
    private bool _registered;

    public AudioDeviceWatcher()
    {
        try
        {
            _enumerator = (IMMDeviceEnumerator)new MMDeviceEnumeratorComObject();
            _enumerator.RegisterEndpointNotificationCallback(this);
            _registered = true;
        }
        catch (Exception ex)
        {
            Logger.Warn($"AudioDeviceWatcher: could not register for device-change notifications, falling back to periodic polling only: {ex.Message}");
        }
    }

    void IMMNotificationClient.OnDeviceStateChanged(string deviceId, int newState) => DevicesChanged?.Invoke();
    void IMMNotificationClient.OnDeviceAdded(string deviceId) => DevicesChanged?.Invoke();
    void IMMNotificationClient.OnDeviceRemoved(string deviceId) => DevicesChanged?.Invoke();
    void IMMNotificationClient.OnDefaultDeviceChanged(int flow, int role, string deviceId) { }
    void IMMNotificationClient.OnPropertyValueChanged(string deviceId, PropertyKey key) { }

    public void Dispose()
    {
        try
        {
            if (_registered && _enumerator != null)
                _enumerator.UnregisterEndpointNotificationCallback(this);
        }
        catch (Exception ex)
        {
            Logger.Warn($"AudioDeviceWatcher: could not unregister cleanly: {ex.Message}");
        }
        if (_enumerator != null && Marshal.IsComObject(_enumerator))
            Marshal.ReleaseComObject(_enumerator);
        _enumerator = null;
    }
}
