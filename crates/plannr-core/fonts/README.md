# Fonts for 3D Text

`sans.json`, `sans_bold.json`, `serif.json` and `serif_bold.json` hold the letter outlines of **Droid Sans** and
**Droid Serif** (regular and bold), Copyright (C) 2008 The Android Open Source Project, licensed under the
Apache License, Version 2.0 (http://www.apache.org/licenses/LICENSE-2.0). See `NOTICE`.

They were made from the typeface.json conversions that ship with three.js (`examples/fonts/droid`), changed
only in form: each glyph keeps its advance width and its outline (`m` move, `l` line, `q` quadratic curve with
the end point first), the other fields are dropped.
