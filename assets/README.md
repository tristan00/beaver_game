# Artwork assets

`ui/icons.png` is the lossless export of the former procedural icon atlas at
commit `7787ed1`. Its decoded RGBA pixels were compared byte-for-byte with the
original generator. Icon ordering remains in `crates/game/src/icons.rs`; each
cell is 64 pixels in a 16-column atlas. Runtime mip generation is unchanged.

These files contain artwork, not executable game logic. Keep asset names,
animation pivots, colors and dimensions stable during structural refactors.
