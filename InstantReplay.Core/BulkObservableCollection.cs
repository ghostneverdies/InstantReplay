using System.Collections.ObjectModel;
using System.Collections.Specialized;
using System.ComponentModel;

namespace InstantReplay.Core;

/// <summary>
/// An <see cref="ObservableCollection{T}"/> that can be swapped in one shot.
/// <para>
/// Rebuilding the gallery used to raise one Insert/Move notification per replay. Every one of
/// those made the GridView re-measure and re-realize its containers, which is what froze the
/// gallery for seconds on first open. <see cref="ReplaceAll"/> mutates the backing list with
/// notifications suppressed and then raises a single <c>Reset</c>.
/// </para>
/// </summary>
public sealed class BulkObservableCollection<T> : ObservableCollection<T>
{
    /// <summary>Replaces every entry, raising exactly one <c>Reset</c> notification.</summary>
    public void ReplaceAll(IReadOnlyList<T> next)
    {
        ArgumentNullException.ThrowIfNull(next);

        CheckReentrancy();

        Items.Clear();
        for (int i = 0; i < next.Count; i++)
            Items.Add(next[i]);

        OnPropertyChanged(new PropertyChangedEventArgs(nameof(Count)));
        OnPropertyChanged(new PropertyChangedEventArgs("Item[]"));
        OnCollectionChanged(new NotifyCollectionChangedEventArgs(NotifyCollectionChangedAction.Reset));
    }
}
