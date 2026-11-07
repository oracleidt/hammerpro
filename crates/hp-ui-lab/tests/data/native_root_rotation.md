# Native system rotation constants

Nine authored Garry's Mod captures exercise the particle-system `rotation`
constant, initializer precedence and DMX value types. Each emits one stationary,
opaque, white particle with radius 24 at (10,0,64), using the four-quadrant 8×8
SpriteCard texture. The camera is (10,-128,64), yaw 90, FOV 60, at 128×128.

Native system constants are **raw radians**. Rotation Random's parameters are
**degrees**. A root value 30 consequently differs from an initializer value 30;
they must not pass through the same degree conversion.

| Case | Authored input | Native behavior |
|---|---|---|
| none | Root constant absent | Zero rotation |
| r30 | Root float 30 | 30 radians |
| r90 | Root float 90 | 90 radians |
| rm90 | Root float -90 | -90 radians |
| r30_init0 | Root float 30, Rotation Random fixed 0 | Initializer replaces the constant |
| r0_init30 | Root float 0, Rotation Random fixed 30 | 30 degrees |
| r30_int | Root int 30 | Wrong type: native default 0 |
| r30_string | Root string "30" | Wrong type: native default 0 |
| r1_bool | Root bool true | Wrong type: native default 0 |

The three wrong-type captures and r30_init0 match the default image exactly.
The green-to-yellow texture-axis angles for the float constants are +81.125°,
-116.565° and +116.565°, matching the projected raw-radian rotations modulo a
full turn. The initializer's 30-degree case measures -29.975°. These centroid
measurements identify units; the regression compares every RGB pixel, rather
than using the coarser angle measurement as its rendering tolerance.

The PCF/GMA, generator, Lua probe, JSON and PNGs are included. Capture uses a
point-sampled RGBA16161616F target, tone mapping 1 and UnlitGeneric sRGB readback.
The test runs the actual PCF parser, simulation, UI sprite adapter/sorter and
GPU, with a whole-image RGB tolerance of 3/255. No attributes are injected in
these nine cases. Installed assets and binaries remain unchanged.

Before the fix the system constant never reached the simulation; r30/r90/rm90
differed by 255 RGB levels. Other system constants, general root parameter
typing and arbitrary PCF behavior are outside this fixture's scope.
