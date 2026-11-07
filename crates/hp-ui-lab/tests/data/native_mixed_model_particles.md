# Native mixed model/sprite renderer order

The owned model is shifted along local Z to lie entirely behind a blue sprite. Native PCFs contain Model-only, Sprite-only, Model-then-Sprite and Sprite-then-Model passes. Radius is 4; the model has its own white material, and the collection sprite is blue. QC/SMD sources and all assets are owned fixtures.

The unmodified native client preserves authored order. Model-then-Sprite has nine blue pixels (center RGB 2,2,255), while Sprite-then-Model has white at the center: sprites do not write depth and the later opaque model overwrites them. Always drawing all models first or last is incorrect.

The regression runs the actual PCF plan and simulation without attribute injection, loads the owned MDL, moves a sprite into the entity-FX queue while retaining its collection/pass identity, and runs the production viewport merge and ordered preparation. Model-ID/material-ID resolution is mocked; actual model/skin/material resolution has independent native image tests in hp-assets. A model-unavailable stage verifies that loading/missing models produce no bogus ranges. Both viewport and preview GPU paths are compared with all four native captures.

Explicit batches select ranges in separate sprite/model buffers. Ordered sprite uploads retain instance order instead of globally regrouping blend modes across boundaries. Existing callers without model batches retain their prior behavior. Native children were closed and Lua backups restored after capture.

All eight native comparisons passed after ordered preparation and range drawing, maximum RGB delta 2/255. The residual is confined to the native blue-over-white sprite blend; model-only, sprite-only and the opposite order retain their independent controls.
