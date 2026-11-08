# Native particle hierarchy and capacity reference

Self-authored PCF and GMA, captured in installed Garrys Mod on 2026-11-08.
report.json records the client DLL hash, native bounds and capture process.
Owned process6320 closed; temporary Lua overrides restored. No installed game
assets were modified. native.lua is the capture source. fixture.rs regenerates
the PCF and owned GMA in the given output folder, using its existing owned white
texture. It does not inject simulated particle attributes.

The children effect has21 separately positioned collections, including its root.
The deep effect contains7 collections with2 drawable leaves. Their native
whole images contain216 and24 lit pixels. The previous editor truncated at16
collections and depth4. Tests run the unchanged authored PCF through simulation,
UI assembly and both production GPU paths, comparing the complete images.

Counts6000 and10000 both reach5000 active particles in the native game. Count
is mapped to alpha, then alpha to position, so the largest X is4999. These
captures explicitly set CP0 orientation to the same identity frame used by
the test. A preceding default-frame capture produced4998.99951171875 because
the game creation-time orientation differs by float rounding; explicitly
setting the frame restores4999. No simulation arithmetic was changed to fit
that transform difference. Both hierarchy PNGs are byte-identical between the
default-frame and explicit-frame captures. The per-collection5000 cap remains.

Preview hierarchy expansion now retains up to256 collections and detects
ancestor cycles, while preserving repeated siblings. This is a resource guard,
not a native hierarchy limit. The existing80000-particle shared editing budget
is independent of this guard. The browser and particle editor report an
incomplete hierarchy rather than silently omitting it at the guard.
