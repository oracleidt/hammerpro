# Native weighted model activities

Self-authored model, PCF and materials. Sources for the compiled model are in
source/. Native capture used owned game process2032, which was closed; the
three temporary Lua overrides were restored. No installed assets were changed.
The model has ACT_IDLE sequences1,2,3 with weights1,3,6. The installed engine
selects among all three rather than always picking the first matching activity.

native.lua creates32 separate renderer definitions and3 direct sequence
controls. It warms the model activity table before loading PCF definitions:
the first cold probe exposed lazy native activity registration, so that cold
capture is not the committed reference. All32 committed weighted PNGs exactly
match one of the three direct native sequence PNGs. selection-proof.json lists
those choices and the independent512 native Entity:SelectWeightedSequence calls.
The live game's global random state is not the editor's isolated preview state.

native-selection.json supplies3072 exact controlled choices from the installed
client's RVA167eb0 selector, with real installed vstdlib RandomInt, seed1.
The native hash/bucket lookup, ticket subtraction and tuple scan execute in
an owned offline emulator. Only the already-loaded model-cache validation is
supplied at the API boundary. Cases cover1:3:6, zero weights, negative weights,
all-zero weights and singleton activities. probe-selection.py regenerates this
reference given the installed win64 folder, requiring the recorded DLL hash.

Tests compare the combined editor RNG and selector to every native result.
The singleton RNG no-advance rule is separately captured from installed
vstdlib exports in hp-pcf/tests/data/native_random_degenerate.json. Model
metadata weights are read from the actual StudioMDL-produced MDL in the GMA.
The asset test runs the authored PCF, chooses once per definition/pass, then
compares both GPU paths to the native image for the controlled choice:70 whole
images, maximum RGB difference0. It also checks stability across entities/frames
and releases cached choices when their definitions die.

The live32 choices are not forced into the editor's RNG state; the geometry
comparison uses the appropriate direct native control. No particle attributes
or sequence picks are injected into production to match an uncontrolled game
random stream. fixture.rs regenerates PCF/GMA from an output folder containing
the existing owned GMA and a game folder containing the compiled owned model.

Primary public descriptions of native tuple weights and selection:
https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/public/studio.cpp
https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/game/shared/animation.cpp
The editor implementation is independently written; no SDK source is copied.
