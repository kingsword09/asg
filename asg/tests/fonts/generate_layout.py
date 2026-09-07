from pathlib import Path

from fontTools.feaLib.builder import addOpenTypeFeaturesFromString
from fontTools.ttLib import TTFont


directory = Path(__file__).parent
font = TTFont(directory / "AsgTestSans-Regular.ttf", recalcTimestamp=False)
names = font["name"]
for name_id, english, french in [
    (1, "Asg Test Legacy", "Police de test historique"),
    (16, "Asg Test Layout", "Police de test ASG"),
]:
    names.removeNames(nameID=name_id)
    names.setName(english, name_id, 3, 1, 0x0409)
    names.setName(french, name_id, 3, 1, 0x040C)

addOpenTypeFeaturesFromString(
    font,
    """
    languagesystem DFLT dflt;
    languagesystem arab dflt;
    feature rlig {
        sub uni0041 uni0042 by uni0043;
    } rlig;
    feature kern {
        pos uni0041 uni0042 -80;
    } kern;
    """,
)
font.save(directory / "AsgTestLayout-Regular.ttf")
