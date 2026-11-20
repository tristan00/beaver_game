# Artwork assets

`ui/icons.png` is the lossless export of the former procedural icon atlas at
commit `7787ed1`. Its decoded RGBA pixels were compared byte-for-byte with the
original generator. Icon ordering remains in `crates/game/src/icons.rs`; each
cell is 64 pixels in a 16-column atlas. Runtime mip generation is unchanged.

These files contain artwork, not executable game logic. Keep asset names,
animation pivots, colors and dimensions stable during structural refactors.

`models/colony.glb` contains 419 named meshes exported from commit `7787ed1`:
seasonal trees and their LODs, beaver rig parts, loads, props, static buildings
and reinforcements. Every position, normal, linear color and triangle index
was compared byte-for-byte after loading the GLB through the runtime loader.
Parameterized dams, canals, docks, barges and pond surfaces remain procedural.
Edit this standard glTF binary with a mesh editor; preserve mesh names and
local origins, since the renderer's animation pivots rely on them.
