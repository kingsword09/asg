from pathlib import Path

from fontTools.ttLib import TTFont


directory = Path(__file__).parent
font = TTFont(directory / "AsgTestSans-Regular.ttf", recalcTimestamp=False)
names = font["name"]
for name_id in (1, 16):
    names.setName("Asg Test Narrow", name_id, 3, 1, 0x0409)

# A 0.55em advance: the Consolas-like case from asg#19 that breaks a fixed
# 0.6em cell grid. Exercised by the metrics-derived cell width tests.
for glyph in font.getGlyphOrder():
    advance, lsb = font["hmtx"][glyph]
    font["hmtx"][glyph] = (550, lsb)

font.save(directory / "AsgTestNarrow-Regular.ttf")
