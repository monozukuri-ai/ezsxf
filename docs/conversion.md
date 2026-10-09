# DXF conversion and plotting

## DXF output

Convert a file or an already parsed result:

```python
import ezsxf

ezsxf.to_dxf("drawing.sfc", "drawing.dxf")

parsed = ezsxf.parse_sfc("drawing.sfc")
dxf_text = ezsxf.to_dxf(parsed)
```

## Plotting

Install the optional plotting dependency with `python -m pip install "ezsxf[plot]"`.
Draw with matplotlib and save through the returned `Axes`:

```python
ax = ezsxf.plot("drawing.sfc")
ax.figure.savefig("drawing.png", dpi=200, bbox_inches="tight")
```

## Conversion scope

Both backends share the same hierarchy, placement, layer, color, line type,
line width, text, dimension, and hatch conversion. Curves are converted to
polylines; use `curve_segments` to control the approximation resolution.

Converters that need true curves can read `PathPrimitive.curve`
(`CurveGeometry`): for circle, arc, ellipse and elliptical-arc sources it holds
the exact curve behind the sampled `points`, already transformed through
compound-figure placements. The curve is `center + axis_u*cos(t) + axis_v*sin(t)`
for `t` from `start_param` to `end_param`; `axis_u`/`axis_v` are conjugate
semi-diameters, so a circle placed with unequal X/Y ratios is reported as the
ellipse it becomes.

P21 `TRIMMED_CURVE` on an `ELLIPSE` retains an `ellipse_arc` curve with local
parameter trims, rotation and clockwise/counterclockwise sense. Nested and
unequal X/Y placements transform both the exact curve and its sampled points.

Conversion accepts SFC and supported P21 geometry. P21 parsing exposes generic
entities and [common sheet/subfigure metadata](usage.md#common-p21-sheet-and-subfigure-metadata);
conversion does not implement the entire AP202 schema. Inspect
`Drawing.warnings` for conversion limits. Externally defined symbols are
shown as insertion markers, while externally defined and tiled hatch patterns
retain only boundaries marked visible by the SXF data.
