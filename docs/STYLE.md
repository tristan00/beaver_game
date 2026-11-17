# Beaver Colony - World Style Guide

**One sentence:** a soft, cute, finished picture-book meadow seen from above, built from rounded plush shapes in warm light, where every beaver looks like a toy and nothing is sharp, dark or grey.

This guide covers the **world only**: land, trees, water, structures, beavers, seasons and camera. Interface styling (panels, buttons, text, icons, navigation) lives in `docs/UI_SPEC.md`; nothing about it belongs here.

Audience: people who want a calm, low-pressure colony toy (curious, pet-sized, cosy). When unsure, make it rounder, lighter, warmer, and finish it: a small detail done well beats a bare surface.

## How to use this guide

- Every section below has a **render file**: a drawn sample sheet of target colours and silhouettes in `docs/style/`. They are targets, not game screenshots. Match the game to them.
- Regenerate, never repaint: `python tools/style_sheets.py` redraws all seven sheets; add `--check` to confirm every hex written in this file exists in `crates/game/src/palette.rs`.
- Colours live in code: `crates/game/src/palette.rs` is the single source (`MEADOW`, `FUR`, `WATER_CLEAR`...). The sheets read it, and game code must use those names, never raw digits. A new colour goes into `palette.rs` and this guide first, then into use.

## Shared rules

- **Shape language:** everything is a blob. Hills are overlapping ellipses, canopy is 3 to 4 overlapping circles, the lodge is a dome. Corners are bevelled or rounded; no sharp spikes.
- **Proportions and scale:** beaver is 1 grid cell wide, canopy about 2x a beaver, lodge about 4x. Never realistic scale.
- **Materials:** matte and soft. Wood has rounded ends with visible cut rings. No gloss, no brush noise, no per-cell grain.
- **Lighting and mood:** late-morning sun from the upper left (`#FFF5E0`), ambient high (`#CCE6FF`) so nothing is black. Every cast and contact shadow is plum `#6B5A8A` at 35 to 45%. Darkest terrain value is Meadow deep `#5FA85A` (grass) or Stone shade `#8C8290` (rock).
- **Outlines:** none on terrain. A 2 px darker-fur outline only on small wooden pieces that must read at distance.
- **Accent rule:** one saturated accent per screen area (a flower, a lily); the rest stays soft.

| Name | Hex | Name | Hex |
|---|---|---|---|
| Meadow light | `#A8D878` | Fur | `#B9783F` |
| Meadow | `#85C468` | Fur light | `#C98B52` |
| Meadow deep | `#5FA85A` | Fur dark / tail | `#8F5A2E` |
| Leaf highlight | `#8CD070` | Ink (eyes) | `#4D3321` |
| Straw | `#D9C27A` | Bark | `#7A4A2B` |
| Mud | `#A07F60` | Cut wood | `#F2D9A8` |
| Stone | `#B9AFA8` | Stick | `#9C6A3E` |
| Stone shade | `#8C8290` | Stick light | `#C79A62` |
| Plum shadow | `#6B5A8A` | Dam mud | `#8F6E52` |

## 1. Trees - `docs/style/trees.png`

- Trees stand in **clusters of 3 to 9** with **clearings** of open meadow between them. Never a grid, never a lone straight row, never one tree per cell.
- Canopy: 3 to 4 round blobs, Meadow deep `#5FA85A` base with Leaf highlight `#8CD070` blobs; trunk Bark `#7A4A2B`, short and stubby. Young trees use Meadow `#85C468`. A tall poplar is a stretched oval, still rounded.
- Beavers fell trees from the cluster edge: **stumps** (Bark side, Cut wood `#F2D9A8` top with a ring) mark the felled edge and stay for seasons. Felled logs lie as rounded cylinders with cut ends.
- Every tree has a plum contact shadow. No dark conifers, no spiky silhouettes.

## 2. Ground - `docs/style/ground.png`

- Natural ground carries the map: Meadow light to Meadow deep gradient with large soft patches (60 to 140 px), Straw `#D9C27A` where dry, Mud `#A07F60` banks at water, Sand `#E6D3A3` shore. No brush noise, no hard facets: slopes are rounded.
- Detail is sparse and small: tufts, flowers (`#FFFAEE`, `#F59AA6`, `#F2C94C`), pebbles; about 1 item per 40x40 px of open grass, never on water.
- **Rock only where terrain calls for it**: ridges, cliffs and the odd pebble. Stone `#B9AFA8` lit, Stone shade `#8C8290` shaded (warm lilac, never blue-grey), bevelled and rounded. No rock as filler in open meadow.

## 3. Water - `docs/style/water.png`

- **Nearly clear.** Water is a thin Water clear `#BFEFF0` tint (about 40 to 60% over the bed) so the mud, stones and weed beneath show through. No opaque blue slab, no grey.
- Depth reads by gently stronger tint, from Water shallow `#6FD3E0` to Water mid glow `#8FE0E6`; Water deep `#1F8AB8` only as a thin tint at the deepest centre, never a solid fill.
- Smooth curved edge with a 5 px Foam `#F4FBF6` ring, short Foam ripple arcs, a few Lily `#4E9A52` pads and Reed `#B7C97A` at the banks. Swimming beavers leave Foam arcs, not white discs.

## 4. Structures - `docs/style/structures.png`

- **Stick-and-mud, grown by paws, in three stages.** Lodge: (1) small mound of sticks, (2) low dome with a dark door, (3) full dome with a vent gap and a winter store of logs. Dam: (1) a few sticks, (2) a staggered row, (3) a thick two-layer wall plastered with mud.
- Materials: Stick `#9C6A3E`, Stick light `#C79A62` for new work, Dam mud `#8F6E52` plaster, Fur dark `#8F5A2E` for logs. Later stages add sticks, mud and detail only; never a new material or hard geometry.
- Sticks cross at loose angles with rounded ends. Silhouette is always a soft mound or low wall, with a plum contact shadow.

## 5. Beaver poses - `docs/style/beaver_poses.png`

- Chibi: head about 60% of body width, flat oval tail in Fur dark, small round ear, dot eyes (Ink), two Cut wood teeth. Plush and rounded, never sharp.
- Six base poses, one clear silhouette per job: **stand**, **carry** (log held over the back), **gnaw** (log in teeth), **build** (stick and mud in paws), **swim** (body low in a Foam ring, head up), **sleep** (curled, eyes as a closed arc).
- Fur uses only Fur / Fur light (belly and face) / Fur dark (tail, ear). No other browns.

## 6. Seasons - `docs/style/seasons.png`

- Same shapes in every season; hue and a few props change, brightness stays high.
- **Spring:** fresh Meadow with Meadow light patches, Blossom `#F5B8C4` petals. **Summer:** saturated Meadow / Meadow deep. **Autumn:** Straw ground, canopies Autumn gold `#E8B04A` with Autumn rust `#D9814E` highlights, fallen leaves, sun `#FFDBB3`. **Winter:** Snow `#F6F4FA` ground, Snow shade `#CFC8E0` canopies with snow caps, sky `#C8D4E8`, water keeps its clear tint with a pale rim.
- Sky `#9ED1F5` (spring, summer). Night is a plum `#6B5A8A` overlay at 30%: dusk, not dark.

## 7. Camera at every zoom - `docs/style/camera.png`

- **Far:** colour and mass only. Clusters read as soft green blobs, water as a pale shape, the lodge as one brown dot. Hide tufts, flowers and ripples.
- **Mid:** shapes read: trees, stumps, lodge and dam stages, beaver poses at small size. Shadows soft, plum.
- **Close:** detail and faces: fur, teeth, cut rings, flowers, tufts, ripples, stick texture.
- Scale holds at every zoom (beaver 1 cell, canopy 2x, lodge 4x). Zoom adds detail, never new colours, and the light and shadow tint never change.

## Checking the running game

Take the same camera views the sheets show, in each season, and compare to the render files. Name each place that breaks a rule: terrain darker than Meadow deep or Stone shade, any grey or blue-grey rock, trees in a grid or without clearings, opaque or grey water, a structure that stays one stage, fur outside the three fur colours, a black shadow, a detail that appears at only one zoom. Fix the code that owns the colour (`palette.rs`), not the screenshot.
