# Command-line interface

The `ezsxf` executable and `python -m ezsxf` expose the same commands.
Use `--help` after a command to see its options.

```bash
# smoke test
python -m ezsxf

# parse to JSON
python -m ezsxf parse sfc drawing.sfc --pretty
python -m ezsxf parse p21 drawing.p21 --lenient

# convert SFC to DXF
python -m ezsxf to-dxf drawing.sfc drawing.dxf

# validate and resave SFC (input and output can be the same file)
python -m ezsxf resave-sfc drawing.sfc copy.sfc
# preserve external references; arrange SAF/image files separately
python -m ezsxf resave-sfc drawing.sfc copy.sfc --allow-external-references

# export the supported SFC subset to AP202 P21
python -m ezsxf to-p21 drawing.sfc drawing.p21

# convert SFC with SAF/images to a new P21 bundle or P2Z archive
python -m ezsxf bundle-p21 drawing.sfc delivery --file-name result.p21
python -m ezsxf to-p2z drawing.sfc result.p2z

# deliver SFC/SAF/images together into a new directory
python -m ezsxf bundle-sfc drawing.sfc delivery --file-name renamed.sfc

# save or interactively display a matplotlib drawing
python -m ezsxf plot drawing.sfc drawing.png --dpi 200
python -m ezsxf plot drawing.sfc
```

Plotting requires the `plot` extra. See [conversion](conversion.md) and
[SFC saving](sfc-writing.md) and [P21 writing](p21-writing.md) for behavior and limits.
