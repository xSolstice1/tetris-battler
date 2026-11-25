# cjk-subset.ttf

A subset of **Noto Sans SC** (weight 600), cut down to only the Chinese characters the game
shows (currently 哥), so the web build stays small.

Noto Sans SC is © Google LLC and Adobe, licensed under the SIL Open Font License, Version 1.1
(https://openfontlicense.org). The subset is a Modified Version under that licence and is
distributed under the same terms; it is not sold on its own.

To add characters, re-run the subsetter with the new text (fontTools):

```sh
python3 - <<'PY'
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer
from fontTools import subset
f = instancer.instantiateVariableFont(TTFont("NotoSansSC-VF.ttf"), {"wght": 600})
o = subset.Options(); o.layout_features = []; o.name_IDs = ["*"]; o.notdef_outline = True
s = subset.Subsetter(o); s.populate(text="哥"); s.subset(f)
f.save("assets/fonts/cjk-subset.ttf")
PY
```
