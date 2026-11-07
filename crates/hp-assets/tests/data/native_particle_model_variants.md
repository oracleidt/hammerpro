# Native model-particle variants

The owned QC/SMD sources produce a model with two bodygroups, two skins and two sequences (one animated five-frame sequence with ACT_IDLE). The GMA includes only these owned compiled model/material/texture files. Seven PCFs/captures exercise default bodygroup selection, alternate and invalid group indices, skin 1, sequence 1, invalid activity override and ACT_IDLE. Animation rate is zero for these live references, isolating pose, selection and material semantics from capture timing. Independent native instruction samples validate nonzero animation cycle arithmetic.

Native `sequence_number` selects a bodygroup and sets its value to 1. `sequence_number 1` selects animation independently. The model renderer resolves `activity override` as an activity, not as a sequence label: `slide` is invalid and falls back, while ACT_IDLE selects the authored sequence.

The asset test loads actual GMA, MDL/VVD/VTX, VMT and VTF files, runs the production model preparation/pose/material paths, and compares viewport and preview output. It also verifies identical-pose reuse, expiry of unused GPU poses, and preservation of ordinary scene-model instances. Model bind geometry and animation data are prepared by the asset worker; animated pose keys include source identity, skin, bodygroup, sequence, frame and interpolation fraction.

Recompile QC/SMD in an owned game directory. The generator accepts output directory and compiled-model game directory. Use the Lua in an isolated owned native test mod to regenerate captures.

Six additional native skinned poses are stored in `native_particle_model_pose.*`. Owned ClientsideModel entities are frozen at cycle 0, .125, .25, .5, .875 and .999 with SetupBones and the same normal basis. Cycle 0 was calibrated against the actual PCF sequence-1 capture: maximum RGB delta 0. These references test native interpolation and loop closure independently of wall-clock timing. The asset test uses its production animation clock and both draw paths: 26 variant/pose images passed with maximum RGB delta 0/255. Independent instruction samples validate clock arithmetic.
