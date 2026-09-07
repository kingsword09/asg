# Font fixtures

`AsgTestLayout-Regular.ttf` is generated from `AsgTestSans-Regular.ttf`.
It adds English and French legacy/typographic family names, a required
`AB` to `C` ligature, and an `AB` kerning pair. These exercise family selection,
GSUB glyph closure, and GPOS preservation without depending on installed fonts.

Regenerate the fixture with:

```sh
uv run --with fonttools==4.64.0 python asg/tests/fonts/generate_layout.py
```

The generated font is checked in; running the Rust tests does not require Python.
