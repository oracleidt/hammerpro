# Native collection and child traversal

These ten Garry's Mod captures use authored red/blue child collections. Red is
at (10,0,64), blue at (10,32,64); each has one 24-unit-radius particle with alpha
128/255, a white SpriteCard texture, and a 100-second lifetime. The camera is
(10,-128,64), yaw 90, FOV 60, rendered at 128×128. Parents without their own
particles have no draw renderer. The included PCF/GMA, generator and Lua probe
reproduce these cases without changing installed game assets or binaries.

| Case | Authored drawing order | Native center RGB |
|---|---|---|
| rb | red, blue | 137, 0, 188 |
| br | blue, red | 188, 0, 137 |
| rbr | red, blue, red | 207, 0, 137 |
| brr | blue, red, red | 225, 0, 99 |
| r2b | red collection with two renderers, then blue | 165, 0, 188 |
| br2 | blue, then red collection with two renderers | 225, 0, 99 |
| nested_rb | child subtree (red, blue), then red sibling | 207, 0, 137 |
| nested_br | child subtree (blue, red), then red sibling | 225, 0, 99 |
| drawn_rb | parent's red renderer, then blue child | 137, 0, 188 |
| drawn_rbr | parent's red renderer, blue child, red child | 207, 0, 137 |

The two nested cases distinguish depth-first traversal from breadth-first
storage/simulation order. The drawn-parent cases distinguish drawing a parent
before its children. Reversing child links changes the native image, even when
global camera-distance sorting would choose the same order in every case.

The regression parses the actual PCF, simulates the effect/children, reads the
authored material look, calls the UI adapter and sorter, then compares every
GPU RGB pixel against the native PNG (tolerance 3/255). All ten use supported
position/lifetime initializers; no particle attributes are injected in these
child tests. Simulation storage remains in its existing parent-before-child
order; a separate draw traversal visits each collection's entire subtree.

Native PNGs use point-sampled RGBA16161616F, tone mapping 1 and sRGB readback.
Coverage is one effect tree with these static children. Priority between
independent effects, visibility gating, endcaps and other child behavior require
separate references; those are not established by these images.
