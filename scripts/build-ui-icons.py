import json
import sys
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.ttLib import TTFont
from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.recordingPen import DecomposingRecordingPen
from fontTools.pens.transformPen import TransformPen
from fontTools.pens.ttGlyphPen import TTGlyphPen
root = Path(__file__).resolve().parents[1]
work = Path(sys.argv[1])
source = TTFont(work / "fluent.ttf")
catalog = json.loads((work / "fluent.json").read_text())
tabler = TTFont(work / "tabler.ttf")
fonts = {"fluent": source, "tabler": tabler}
groups = {
"document":"🗋", "folder_open":"🗁📂", "folder":"🗀", "save":"💾", "copy":"🗐", "clipboard":"📋", "cut":"✂", "delete":"🗑",
"arrow_undo":"⟲", "arrow_redo":"⟳", "arrow_sync":"🔄", "settings":"⚙", "search":"🔍", "link":"🔗⛓", "connector":"🔌",
"warning":"⚠", "dismiss":"✖❌", "checkmark":"✔", "add":"➕✚", "subtract":"➖", "play":"▶⏵", "previous":"◀⏮", "stop":"⏹", "pause":"⏸",
"chevron_down":"⏷▾▼🔻", "chevron_up":"⏶▲🔺", "chevron_right":"▸", "eye":"👁", "box":"📦", "lightbulb":"💡", "target":"🎯⌖◎",
"color":"🎨", "drop":"💧", "pin":"📌", "location":"📍", "globe":"🌐", "arrow_down":"⬇", "arrow_up":"⬆", "full_screen_maximize":"⛶",
"xbox_controller":"🎮", "weather_sunny":"☀", "cloud":"☁", "weather_fog":"🌁", "weather_moon":"🌙", "weather_rain":"☔", "keyboard":"⌨",
"square_multiple":"🗗▣", "map":"🗺", "image":"🌄🖼", "mesh":"🗻", "sparkle":"✨🎆", "lock_closed":"🔒🔐", "desktop":"🖵", "camera":"📷", "video":"🎥",
"edit":"✏", "archive":"🗄", "cube":"🎲🗿", "arrow_shuffle":"🔀", "scales":"⚖", "prohibited":"🚫", "door":"🚪", "bug":"🐛", "radar":"📡",
"book_open":"📖", "branch":"🐙", "box_edit":"🔳", "square":"🔲⬛", "cursor":"⬉", "toolbox":"⛏🔨", "pentagon":"⬟", "hexagon":"⬣",
"draw_shape":"➰", "person_walking":"🚶", "arrow_up_right":"⬈", "arrow_down_left":"⬋", "arrow_down_right":"⬊", "arrow_swap":"⬌",
"arrow_download":"📥", "arrow_upload":"📤", "speaker_2":"🔊", "grid":"🕸▦", "wrench":"🔧", "warning_shield":"🚧", "leaf_two":"🌿",
"chat":"💬", "hourglass":"⏳", "line_horizontal_3":"☰", "panel_left":"◫", "circle_half_fill":"◔◑◕", "circle":"○", "square_hint":"◻", "diamond":"◆", "grid_dots":"▓", "keyboard_layout_float":"⌘"
}
resolved = {}
missing = []
for name, chars in groups.items():
    if name == "mesh":
        # Tabler Icons 3.50.0: the warped surface is distinct from Texture's image.
        for char in chars:
            resolved[char] = (name, "tabler", tabler.getBestCmap()[0x10201])
        continue
    key = next((f"ic_fluent_{name}_{size}_regular" for size in [20,24,16,28,32,48] if f"ic_fluent_{name}_{size}_regular" in catalog), None)
    if key is None:
        missing.append(name)
    else:
        for char in chars:
            resolved[char] = (name, "fluent", source.getBestCmap()[catalog[key]])
if missing:
    print(json.dumps({"missing":missing,"suggestions":{n:[k for k in catalog if n.split('_')[0] in k][:10] for n in missing}}))
    sys.exit(1)
font = FontBuilder(1024, isTTF=True)
order = [".notdef"] + sorted(set(name for name, _, _ in resolved.values()))
font.setupGlyphOrder(order)
font.setupCharacterMap({ord(char):name for char,(name,_,_) in resolved.items()})
outlines = {".notdef":TTGlyphPen(None).glyph()}
for name, font_name, source_name in set(resolved.values()):
    glyphs = fonts[font_name].getGlyphSet()
    glyph = glyphs[source_name]
    bounds = BoundsPen(glyphs)
    glyph.draw(bounds)
    x0,y0,x1,y1 = bounds.bounds
    scale = 832 / max(x1-x0,y1-y0)
    transform = (scale,0,0,scale,512-(x0+x1)*scale/2,360-(y0+y1)*scale/2)
    recorded = DecomposingRecordingPen(glyphs)
    glyph.draw(recorded)
    pen = TTGlyphPen(None)
    recorded.replay(TransformPen(pen,transform))
    outlines[name] = pen.glyph()
font.setupGlyf(outlines)
font.setupHorizontalMetrics({name:(1024,getattr(outlines[name],"xMin",0)) for name in order})
font.setupHorizontalHeader(ascent=872,descent=-152)
license_text = ((work/"fluent-license.txt").read_text(encoding="utf-8").rstrip()
    + "\n\nTabler Icons (mesh):\n" + (work/"tabler-license.txt").read_text(encoding="utf-8"))
font.setupNameTable({"familyName":"Hammer Pro Icons","styleName":"Regular","uniqueFontIdentifier":"HammerProIcons-1.1.1","fullName":"Hammer Pro Icons Regular","psName":"HammerProIcons-Regular","version":"Version 1.1.1","licenseDescription":license_text,"description":"Normalized subsets of Microsoft Fluent UI System Icons (https://github.com/microsoft/fluentui-system-icons) and Tabler Icons 3.50.0 mesh (https://github.com/tabler/tabler-icons)."})
font.setupOS2(sTypoAscender=872,sTypoDescender=-152,usWinAscent=872,usWinDescent=152)
font.setupPost()
font.setupMaxp()
out=root/"assets"/"fonts"
out.mkdir(parents=True,exist_ok=True)
font.save(out/"hammerpro-icons.ttf")
(out/"LICENSE.txt").write_text(license_text,encoding="utf-8")
(out/"icons.json").write_text(json.dumps({char:name for char,(name,_,_) in resolved.items()},ensure_ascii=False,indent=2)+"\n",encoding="utf-8")
print(f"Built {len(resolved)} symbols, {len(order)-1} icons, {(out/'hammerpro-icons.ttf').stat().st_size} bytes")


