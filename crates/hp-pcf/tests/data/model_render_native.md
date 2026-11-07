# Native model-particle transform and animation arithmetic

`model_transform_native.json` records forty runs of unmodified client.dll instructions at RVA 0x451234..0x45177b, including native normalization, AngleMatrix and matrix concatenation. Input normals include zero and an oblique vector; both orientation modes and all three individual angles are exercised. The JSON records the source SHA-256. No arithmetic helpers were substituted. The returned 3x4 matrix is captured before the renderer assigns particle position to its translation column. Float comparisons permit 2e-6 for the portable trigonometry implementation.

Attribute 4 is roll, 12 is yaw and 20 is pitch. The native temporary pointers at frame +0x50 and +0x58 refer to attributes 20 and 12 respectively. The normal is attribute 21, default (0,0,1). Orient-Z selects a different ordering/sign of the normal-derived basis; the default is not an identity basis.

`model_animation_native.json` records 1,200 runs of actual instructions at RVA 0x52fd7e..0x52fdb8. The inputs are collection time, animation rate and the integer Studio_MaxFrame result. Cycle is the signed fractional part of time*rate/max(max_frame,1), computed with native truncation toward zero. It is independent of particle creation time and sequence looping flags. Studio_MaxFrame uses frame count minus one when frame count exceeds one; that reduction was separately verified in RVA 0x191310. The test compares every output float bit, including negative rates and times. It does not substitute particle age for collection time.

The live model/PCF/GMA references and full-image comparisons are in hp-ui-lab/tests/data/native_model_particles.*. Model skin/bodygroup/sequence/activity captures are in hp-assets/tests/data/native_particle_model_variants.*.
