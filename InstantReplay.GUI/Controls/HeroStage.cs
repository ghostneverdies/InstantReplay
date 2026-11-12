using System.Globalization;
using System.Numerics;
using Microsoft.UI.Composition;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Hosting;
using Windows.UI;

namespace InstantReplay.GUI.Controls;

/// <summary>
/// Animated backdrop for the home page hero. It fills the whole hero card (everything from the top of the
/// page down to the CPU/RAM row) and is tinted entirely from the user's Windows accent colour:
///  * slow orbiting aurora blobs (accent, a lighter and darker shade and a hue-shifted companion),
///  * a drifting field of glowing particles,
///  * an equalizer "skyline" rising from the bottom edge, a nod to what the app records,
///  * two pulse rings that radiate from the app icon, which itself floats gently.
/// Everything runs on the compositor: positions and scales are expression animations driven by two shared
/// clocks, so there is no per-frame work on the UI thread and the stage can be switched off completely
/// (all animations stopped, visuals hidden) when the page is not visible.
/// </summary>
public sealed class HeroStage : IDisposable
{
    private const float TwoPi = 6.2831853f;
    private const int ParticleCount = 28;
    private const int BarCount = 56;
    private const float RingSize = 240f;

    private readonly FrameworkElement _host;
    private readonly FrameworkElement _ringAnchor;
    private readonly FrameworkElement? _icon;
    private readonly Compositor _compositor;
    private readonly ContainerVisual _root;
    private readonly CompositionPropertySet _clock;
    private readonly CompositionRoundedRectangleGeometry _clipGeometry;
    private readonly CompositionEasingFunction _linear;
    private readonly CompositionEasingFunction _soft;

    private readonly List<BrushSpec> _brushes = new();
    private readonly List<SpriteVisual> _blobs = new();
    private readonly List<SpriteVisual> _particles = new();
    private readonly List<SpriteVisual> _bars = new();
    private readonly SpriteVisual[] _rings = new SpriteVisual[2];
    private readonly Random _rng = new(23);

    private Color _accent = Color.FromArgb(255, 0x8B, 0x5F, 0xBF);
    private bool _dark = true;
    private bool _running;
    private bool _disposed;

    private readonly record struct Palette(Color Accent, Color Light, Color Dark, Color Analog, float Alpha);
    private sealed record BrushSpec(CompositionGradientBrush Brush, Func<Palette, Color[]> Colors);

    public HeroStage(FrameworkElement host, FrameworkElement ringAnchor, FrameworkElement? icon)
    {
        _host = host;
        _ringAnchor = ringAnchor;
        _icon = icon;

        var hostVisual = ElementCompositionPreview.GetElementVisual(host);
        _compositor = hostVisual.Compositor;

        _root = _compositor.CreateContainerVisual();
        _root.RelativeSizeAdjustment = Vector2.One;
        _root.IsVisible = false;

        _clock = _compositor.CreatePropertySet();
        _clock.InsertScalar("Slow", 0f);
        _clock.InsertScalar("Fast", 0f);

        _linear = _compositor.CreateLinearEasingFunction();
        _soft = _compositor.CreateCubicBezierEasingFunction(new Vector2(0.45f, 0f), new Vector2(0.55f, 1f));

        // One rounded clip for the whole stage so the aurora respects the card's corner radius.
        _clipGeometry = _compositor.CreateRoundedRectangleGeometry();
        _clipGeometry.CornerRadius = new Vector2(18f, 18f);
        _root.Clip = _compositor.CreateGeometricClip(_clipGeometry);

        BuildVisuals();
        ElementCompositionPreview.SetElementChildVisual(host, _root);

        host.SizeChanged += OnHostSizeChanged;
        ringAnchor.SizeChanged += OnAnchorSizeChanged;
        ApplyPalette();
    }

    // ------------------------------------------------------------------ public surface

    /// <summary>Retints the whole stage. Safe to call at any time (accent or theme change).</summary>
    public void SetAccent(Color accent, bool darkTheme)
    {
        if (_disposed) return;
        _accent = accent;
        _dark = darkTheme;
        ApplyPalette();
    }

    public void Start()
    {
        if (_disposed) return;
        Stop();
        BuildAnimations();
        UpdateClip();
        UpdateRingCenter();
        _root.IsVisible = true;
        _running = true;
    }

    public void Stop()
    {
        if (_disposed) return;
        _running = false;
        _root.IsVisible = false;

        _clock.StopAnimation("Slow");
        _clock.StopAnimation("Fast");
        foreach (var blob in _blobs) { blob.StopAnimation("Offset"); blob.StopAnimation("Scale"); blob.StopAnimation("Size"); blob.StopAnimation("CenterPoint"); }
        foreach (var dot in _particles) { dot.StopAnimation("Offset"); dot.StopAnimation("Opacity"); }
        foreach (var bar in _bars) { bar.StopAnimation("Scale"); bar.StopAnimation("Size"); bar.StopAnimation("Offset"); bar.StopAnimation("CenterPoint"); }
        foreach (var ring in _rings) { ring.StopAnimation("Scale"); ring.StopAnimation("Opacity"); ring.Opacity = 0f; }

        if (_icon != null)
        {
            var visual = ElementCompositionPreview.GetElementVisual(_icon);
            visual.StopAnimation("Translation.Y");
            visual.StopAnimation("RotationAngleInDegrees");
            visual.StopAnimation("Scale");
            visual.Scale = Vector3.One;
            visual.RotationAngleInDegrees = 0f;
        }
    }

    public void Dispose()
    {
        if (_disposed) return;
        Stop();
        _disposed = true;
        _host.SizeChanged -= OnHostSizeChanged;
        _ringAnchor.SizeChanged -= OnAnchorSizeChanged;
        ElementCompositionPreview.SetElementChildVisual(_host, null);
        _root.Dispose();
    }

    // ------------------------------------------------------------------ construction

    private void BuildVisuals()
    {
        // Soft accent wash behind everything.
        var wash = _compositor.CreateSpriteVisual();
        wash.RelativeSizeAdjustment = Vector2.One;
        var washBrush = Linear(new[] { 0f, 1f }, new Vector2(0f, 0f), new Vector2(1f, 1f),
            p => new[] { With(p.Accent, 0.20f * p.Alpha), With(p.Dark, 0f) });
        wash.Brush = washBrush;
        _root.Children.InsertAtTop(wash);

        // Aurora blobs.
        (Func<Palette, Color> Color, float Alpha, float Cx, float Cy, float Rx, float Ry, int K, float Phase, float Size)[] blobs =
        {
            (p => p.Accent, 0.55f, 0.20f, 0.30f, 0.16f, 0.22f, 1, 0.00f, 0.85f),
            (p => p.Light,  0.34f, 0.80f, 0.34f, 0.14f, 0.26f, 2, 0.35f, 0.75f),
            (p => p.Analog, 0.32f, 0.52f, 0.88f, 0.30f, 0.10f, 1, 0.60f, 0.95f),
            (p => p.Dark,   0.65f, 0.62f, 0.12f, 0.22f, 0.12f, 3, 0.15f, 0.80f),
        };
        foreach (var b in blobs)
        {
            var brush = Radial(new[] { 0f, 0.5f, 1f }, p =>
            {
                Color c = b.Color(p);
                return new[] { With(c, b.Alpha * p.Alpha), With(c, b.Alpha * 0.38f * p.Alpha), With(c, 0f) };
            });
            var blob = _compositor.CreateSpriteVisual();
            blob.Brush = brush;
            blob.Size = new Vector2(420f, 420f);
            _blobs.Add(blob);
            _root.Children.InsertAtTop(blob);
        }

        // Particles.
        var dotAccent = Radial(new[] { 0f, 0.4f, 1f },
            p => new[] { With(p.Light, 0.95f * p.Alpha), With(p.Accent, 0.38f * p.Alpha), With(p.Accent, 0f) });
        var dotAnalog = Radial(new[] { 0f, 0.4f, 1f },
            p => new[] { With(p.Analog, 0.9f * p.Alpha), With(p.Analog, 0.32f * p.Alpha), With(p.Analog, 0f) });
        for (int i = 0; i < ParticleCount; i++)
        {
            var dot = _compositor.CreateSpriteVisual();
            float size = 6f + (float)_rng.NextDouble() * 12f;
            dot.Size = new Vector2(size, size);
            dot.Brush = i % 4 == 3 ? dotAnalog : dotAccent;
            dot.Opacity = 0f;
            _particles.Add(dot);
            _root.Children.InsertAtTop(dot);
        }

        // Equalizer skyline along the bottom edge.
        var barBrush = Linear(new[] { 0f, 1f }, new Vector2(0.5f, 0f), new Vector2(0.5f, 1f),
            p => new[] { With(p.Accent, 0f), With(p.Light, 0.42f * p.Alpha) });
        for (int i = 0; i < BarCount; i++)
        {
            var bar = _compositor.CreateSpriteVisual();
            bar.Brush = barBrush;
            bar.AnchorPoint = new Vector2(0f, 1f);
            _bars.Add(bar);
            _root.Children.InsertAtTop(bar);
        }

        // Pulse rings radiating from the icon.
        var ringBrush = Radial(new[] { 0f, 0.70f, 0.88f, 1f },
            p => new[] { With(p.Accent, 0f), With(p.Accent, 0f), With(p.Light, 0.75f * p.Alpha), With(p.Accent, 0f) });
        for (int i = 0; i < _rings.Length; i++)
        {
            var ring = _compositor.CreateSpriteVisual();
            ring.Brush = ringBrush;
            ring.Size = new Vector2(RingSize, RingSize);
            ring.CenterPoint = new Vector3(RingSize / 2f, RingSize / 2f, 0f);
            ring.Opacity = 0f;
            _rings[i] = ring;
            _root.Children.InsertAtTop(ring);
        }
    }

    private CompositionRadialGradientBrush Radial(float[] offsets, Func<Palette, Color[]> colors)
    {
        var brush = _compositor.CreateRadialGradientBrush();
        brush.MappingMode = CompositionMappingMode.Relative;
        brush.EllipseCenter = new Vector2(0.5f, 0.5f);
        brush.EllipseRadius = new Vector2(0.5f, 0.5f);
        foreach (float offset in offsets)
            brush.ColorStops.Add(_compositor.CreateColorGradientStop(offset, Color.FromArgb(0, 0, 0, 0)));
        _brushes.Add(new BrushSpec(brush, colors));
        return brush;
    }

    private CompositionLinearGradientBrush Linear(float[] offsets, Vector2 start, Vector2 end,
        Func<Palette, Color[]> colors)
    {
        var brush = _compositor.CreateLinearGradientBrush();
        brush.MappingMode = CompositionMappingMode.Relative;
        brush.StartPoint = start;
        brush.EndPoint = end;
        foreach (float offset in offsets)
            brush.ColorStops.Add(_compositor.CreateColorGradientStop(offset, Color.FromArgb(0, 0, 0, 0)));
        _brushes.Add(new BrushSpec(brush, colors));
        return brush;
    }

    // ------------------------------------------------------------------ animation

    private void BuildAnimations()
    {
        // Shared clocks: everything below is a pure function of these two numbers.
        var slow = _compositor.CreateScalarKeyFrameAnimation();
        slow.InsertKeyFrame(0f, 0f);
        slow.InsertKeyFrame(1f, 1f, _linear);
        slow.Duration = TimeSpan.FromSeconds(54);
        slow.IterationBehavior = AnimationIterationBehavior.Forever;
        _clock.StartAnimation("Slow", slow);

        var fast = _compositor.CreateScalarKeyFrameAnimation();
        fast.InsertKeyFrame(0f, 0f);
        fast.InsertKeyFrame(1f, 1f, _linear);
        fast.Duration = TimeSpan.FromSeconds(4);
        fast.IterationBehavior = AnimationIterationBehavior.Forever;
        _clock.StartAnimation("Fast", fast);

        AnimateBlobs();
        AnimateParticles();
        AnimateBars();
        AnimateRings();
        AnimateIcon();
    }

    private ExpressionAnimation Expr(string expression)
    {
        var animation = _compositor.CreateExpressionAnimation(expression);
        animation.SetReferenceParameter("host", _root);
        animation.SetReferenceParameter("clock", _clock);
        return animation;
    }

    private static string F(float value) => value.ToString("0.####", CultureInfo.InvariantCulture);

    private void AnimateBlobs()
    {
        (float Cx, float Cy, float Rx, float Ry, int K, float Phase, float Size)[] paths =
        {
            (0.20f, 0.30f, 0.16f, 0.22f, 1, 0.00f, 0.85f),
            (0.80f, 0.34f, 0.14f, 0.26f, 2, 0.35f, 0.75f),
            (0.52f, 0.88f, 0.30f, 0.10f, 1, 0.60f, 0.95f),
            (0.62f, 0.12f, 0.22f, 0.12f, 3, 0.15f, 0.80f),
        };

        for (int i = 0; i < _blobs.Count && i < paths.Length; i++)
        {
            var p = paths[i];
            var blob = _blobs[i];

            // The blob is sized from the stage so it scales with the window.
            blob.StartAnimation("Size", Expr(
                $"Vector2(Max(host.Size.X, host.Size.Y) * {F(p.Size)}, Max(host.Size.X, host.Size.Y) * {F(p.Size)})"));

            string angle = $"{F(TwoPi)} * (clock.Slow * {p.K} + {F(p.Phase)})";
            blob.StartAnimation("Offset", Expr(
                $"Vector3(host.Size.X * ({F(p.Cx)} + {F(p.Rx)} * Cos({angle})) - this.Target.Size.X * 0.5, " +
                $"host.Size.Y * ({F(p.Cy)} + {F(p.Ry)} * Sin({angle})) - this.Target.Size.Y * 0.5, 0)"));

            blob.StartAnimation("CenterPoint", Expr("Vector3(this.Target.Size.X * 0.5, this.Target.Size.Y * 0.5, 0)"));
            string breathe = $"1 + 0.14 * Sin({F(TwoPi)} * (clock.Slow * {p.K * 2} + {F(p.Phase)}))";
            blob.StartAnimation("Scale", Expr($"Vector3({breathe}, {breathe}, 1)"));
        }
    }

    private void AnimateParticles()
    {
        foreach (var dot in _particles)
        {
            float x = 0.04f + (float)_rng.NextDouble() * 0.92f;
            float sway = ((float)_rng.NextDouble() - 0.5f) * 0.10f;
            double seconds = 9 + _rng.NextDouble() * 13;
            var delay = TimeSpan.FromSeconds(_rng.NextDouble() * seconds);

            var move = _compositor.CreateVector3KeyFrameAnimation();
            move.SetReferenceParameter("host", _root);
            move.InsertExpressionKeyFrame(0f, $"Vector3(host.Size.X * {F(x)}, host.Size.Y + 24, 0)", _linear);
            move.InsertExpressionKeyFrame(0.5f, $"Vector3(host.Size.X * {F(x + sway)}, host.Size.Y * 0.48, 0)", _linear);
            move.InsertExpressionKeyFrame(1f, $"Vector3(host.Size.X * {F(x)}, -24, 0)", _linear);
            move.Duration = TimeSpan.FromSeconds(seconds);
            move.DelayTime = delay;
            move.DelayBehavior = AnimationDelayBehavior.SetInitialValueBeforeDelay;
            move.IterationBehavior = AnimationIterationBehavior.Forever;

            var fade = _compositor.CreateScalarKeyFrameAnimation();
            fade.InsertKeyFrame(0f, 0f);
            fade.InsertKeyFrame(0.15f, 1f, _soft);
            fade.InsertKeyFrame(0.82f, 1f);
            fade.InsertKeyFrame(1f, 0f, _soft);
            fade.Duration = TimeSpan.FromSeconds(seconds);
            fade.DelayTime = delay;
            fade.DelayBehavior = AnimationDelayBehavior.SetInitialValueBeforeDelay;
            fade.IterationBehavior = AnimationIterationBehavior.Forever;

            dot.StartAnimation("Offset", move);
            dot.StartAnimation("Opacity", fade);
        }
    }

    private void AnimateBars()
    {
        for (int i = 0; i < _bars.Count; i++)
        {
            var bar = _bars[i];
            // Taller in the middle, quieter toward the edges.
            float envelope = 0.40f + 0.60f * MathF.Sin(MathF.PI * (i + 0.5f) / BarCount);

            bar.StartAnimation("Size", Expr(
                $"Vector2(host.Size.X / {BarCount} - 3, host.Size.Y * 0.30)"));
            bar.StartAnimation("Offset", Expr(
                $"Vector3(host.Size.X * {F(i / (float)BarCount)} + 1.5, host.Size.Y, 0)"));
            bar.StartAnimation("CenterPoint", Expr("Vector3(0, this.Target.Size.Y, 0)"));

            string wave = $"Abs(Sin({F(TwoPi)} * (clock.Fast * 3 + {F(i * 0.137f)})) * " +
                          $"Cos({F(TwoPi)} * (clock.Fast * 2 + {F(i * 0.071f)})))";
            bar.StartAnimation("Scale", Expr($"Vector3(1, 0.05 + {F(envelope)} * {wave}, 1)"));
        }
    }

    private void AnimateRings()
    {
        for (int i = 0; i < _rings.Length; i++)
        {
            var ring = _rings[i];
            var delay = TimeSpan.FromSeconds(1.8 * i);

            var grow = _compositor.CreateVector3KeyFrameAnimation();
            grow.InsertKeyFrame(0f, new Vector3(0.50f, 0.50f, 1f));
            grow.InsertKeyFrame(1f, new Vector3(1.90f, 1.90f, 1f), _soft);
            grow.Duration = TimeSpan.FromSeconds(3.6);
            grow.DelayTime = delay;
            grow.DelayBehavior = AnimationDelayBehavior.SetInitialValueBeforeDelay;
            grow.IterationBehavior = AnimationIterationBehavior.Forever;

            var fade = _compositor.CreateScalarKeyFrameAnimation();
            fade.InsertKeyFrame(0f, 0f);
            fade.InsertKeyFrame(0.15f, 0.9f, _soft);
            fade.InsertKeyFrame(1f, 0f, _soft);
            fade.Duration = TimeSpan.FromSeconds(3.6);
            fade.DelayTime = delay;
            fade.DelayBehavior = AnimationDelayBehavior.SetInitialValueBeforeDelay;
            fade.IterationBehavior = AnimationIterationBehavior.Forever;

            ring.StartAnimation("Scale", grow);
            ring.StartAnimation("Opacity", fade);
        }
    }

    private void AnimateIcon()
    {
        if (_icon == null) return;

        var visual = ElementCompositionPreview.GetElementVisual(_icon);
        ElementCompositionPreview.SetIsTranslationEnabled(_icon, true);
        visual.Properties.InsertVector3("Translation", Vector3.Zero);
        visual.CenterPoint = new Vector3((float)_icon.ActualWidth / 2f, (float)_icon.ActualHeight / 2f, 0f);

        var bob = _compositor.CreateScalarKeyFrameAnimation();
        bob.InsertKeyFrame(0f, 0f);
        bob.InsertKeyFrame(0.5f, -9f, _soft);
        bob.InsertKeyFrame(1f, 0f, _soft);
        bob.Duration = TimeSpan.FromSeconds(3.4);
        bob.IterationBehavior = AnimationIterationBehavior.Forever;

        var tilt = _compositor.CreateScalarKeyFrameAnimation();
        tilt.InsertKeyFrame(0f, -2.5f);
        tilt.InsertKeyFrame(0.5f, 2.5f, _soft);
        tilt.InsertKeyFrame(1f, -2.5f, _soft);
        tilt.Duration = TimeSpan.FromSeconds(4.6);
        tilt.IterationBehavior = AnimationIterationBehavior.Forever;

        var pulse = _compositor.CreateVector3KeyFrameAnimation();
        pulse.InsertKeyFrame(0f, Vector3.One);
        pulse.InsertKeyFrame(0.5f, new Vector3(1.05f, 1.05f, 1f), _soft);
        pulse.InsertKeyFrame(1f, Vector3.One, _soft);
        pulse.Duration = TimeSpan.FromSeconds(2.4);
        pulse.IterationBehavior = AnimationIterationBehavior.Forever;

        visual.StartAnimation("Translation.Y", bob);
        visual.StartAnimation("RotationAngleInDegrees", tilt);
        visual.StartAnimation("Scale", pulse);
    }

    // ------------------------------------------------------------------ layout

    private void OnHostSizeChanged(object sender, SizeChangedEventArgs e)
    {
        UpdateClip();
        QueueRingUpdate();
    }

    private void OnAnchorSizeChanged(object sender, SizeChangedEventArgs e) => QueueRingUpdate();

    private void UpdateClip()
    {
        _clipGeometry.Size = new Vector2((float)_host.ActualWidth, (float)_host.ActualHeight);
    }

    private void QueueRingUpdate()
    {
        // Nothing to reposition while the page is hidden, and the anchor is often mid-rearrange
        // during navigation. Start() does one explicit update once the page is visible.
        if (!_running) return;
        // The icon is re-arranged after the host resizes, so measure on the next dispatcher turn.
        _host.DispatcherQueue?.TryEnqueue(UpdateRingCenter);
    }

    private void UpdateRingCenter()
    {
        try
        {
            if (_disposed || _ringAnchor.ActualWidth <= 0 || _host.ActualWidth <= 0) return;
            var origin = _ringAnchor.TransformToVisual(_host).TransformPoint(
                new Windows.Foundation.Point(_ringAnchor.ActualWidth / 2, _ringAnchor.ActualHeight / 2));
            var offset = new Vector3((float)origin.X - RingSize / 2f, (float)origin.Y - RingSize / 2f, 0f);
            foreach (var ring in _rings) ring.Offset = offset;
        }
        catch
        {
            // The anchor is briefly detached while pages swap; the next size change fixes it.
        }
    }

    // ------------------------------------------------------------------ colour

    private void ApplyPalette()
    {
        Color accent = _accent;
        var palette = new Palette(
            accent,
            Lerp(accent, Color.FromArgb(255, 255, 255, 255), 0.38f),
            Lerp(accent, Color.FromArgb(255, 0, 0, 0), 0.35f),
            ShiftHue(accent, 32f),
            _dark ? 1f : 0.62f);

        foreach (var spec in _brushes)
        {
            Color[] colors = spec.Colors(palette);
            for (int i = 0; i < colors.Length && i < spec.Brush.ColorStops.Count; i++)
                spec.Brush.ColorStops[i].Color = colors[i];
        }
    }

    private static Color With(Color c, float alpha) =>
        Color.FromArgb((byte)Math.Clamp((int)MathF.Round(alpha * 255f), 0, 255), c.R, c.G, c.B);

    private static Color Lerp(Color a, Color b, float t) => Color.FromArgb(255,
        (byte)(a.R + (b.R - a.R) * t), (byte)(a.G + (b.G - a.G) * t), (byte)(a.B + (b.B - a.B) * t));

    /// <summary>Rotates the hue so the secondary glow is a harmonious neighbour of the accent.</summary>
    private static Color ShiftHue(Color c, float degrees)
    {
        float r = c.R / 255f, g = c.G / 255f, b = c.B / 255f;
        float max = MathF.Max(r, MathF.Max(g, b)), min = MathF.Min(r, MathF.Min(g, b));
        float delta = max - min;

        float h = 0f;
        if (delta > 0f)
        {
            if (max == r) h = 60f * (((g - b) / delta) % 6f);
            else if (max == g) h = 60f * (((b - r) / delta) + 2f);
            else h = 60f * (((r - g) / delta) + 4f);
        }
        if (h < 0f) h += 360f;

        float s = max <= 0f ? 0f : delta / max;
        float v = max;
        h = (h + degrees) % 360f;

        float chroma = v * s;
        float x = chroma * (1f - MathF.Abs((h / 60f) % 2f - 1f));
        float m = v - chroma;
        (float rr, float gg, float bb) = h switch
        {
            < 60f => (chroma, x, 0f),
            < 120f => (x, chroma, 0f),
            < 180f => (0f, chroma, x),
            < 240f => (0f, x, chroma),
            < 300f => (x, 0f, chroma),
            _ => (chroma, 0f, x),
        };

        return Color.FromArgb(255, (byte)((rr + m) * 255f), (byte)((gg + m) * 255f), (byte)((bb + m) * 255f));
    }
}
