# Water model: audit against how streams work, and the model that replaces it

Maps are 512 m windows of 1 m cells from 1 m lidar, heights in metres. A game day is a real day
(seasons are 30 days); at 1x speed one game day passes in about 5 real minutes (300x real time), the
sim steps one game minute at a time, and beavers walk 0.5 m/s and swim 1 m/s.

## Audit of the old engine (2D "virtual pipe" model) and the maps it ran on

Each line: what the engine did, what a real stream does.

### Flow and speed
1. **Flow law.** A face passed flow in proportion to the surface drop times the depth (linear in
   slope and depth). Real open-channel flow follows Manning: discharge per width grows with depth
   to the 5/3 power and slope to the 1/2. Consequence: depth fell as 1/slope, so steep reaches
   became films of a few millimetres and flat reaches ran metres deep. Real depth varies only as
   slope^-0.3 (a 50x steeper reach is ~3x shallower, not 50x).
2. **Capacity cap.** Each face moved at most 0.24 of the surface drop per substep, whatever the
   depth: a deeper river could not carry more. Real channels carry far more as they deepen.
3. **Speed.** Water crept a few cells per substep-day budget; a river took days to cross 500 m.
   Real stream water moves 0.2-1 m/s and crosses a 512 m map in 10-40 minutes, i.e. at game
   time scale a river is always in equilibrium with its inflow.
4. **Discharge.** Rivers carried 1-8 "units"/day (16-130 m3/day). A 1 km2 catchment in these
   regions yields about 0.01 m3/s mean (about 315 mm of runoff a year), 860 m3/day: the game ran
   10-50x too little water, chosen only so the slow pipe model would not overflow.
5. **Seasons.** Inflow followed 10% of the snowmelt swing. Real boreal/alpine streams run about 3x
   the mean in the spring freshet and a third of it in late summer and under winter ice.

### Channels
6. **Width.** Every channel was a whole 4 m cell. Real streams of 0.2-25 km2 are 0.5-6 m wide,
   growing as the square root of their bankfull flow (hydraulic geometry).
7. **Wetted width.** The water filled the cell or nothing. A real stream at low flow occupies part
   of its bed (wetted width grows as flow^0.26 at a station); the rest of the bed is wet gravel.
8. **Depth.** Channels were cut 0.3-0.7 m whatever the stream, then steepened through flat ground
   so the slow model could pass its flow (trenches up to 2 m). Real bankfull depth is 0.2-0.6 m for
   these streams and follows the landscape.
9. **Diagonal flow.** Water could only cross cell faces, so diagonal reaches ran through lowered
   corner cells, doubling path length and halving capacity there.

### What a player sees
10. **Thin water drawn as ground.** Cells under 1 cm drew no water and no bed: a stream entering
    over a steep reach was invisible and appeared to start mid-slope ("a river from nowhere").
    A real stream always shows: a bed of wet stones or mud and a ribbon of moving water, narrow
    and fast in riffles, wider and slower in pools.
11. **No stream bed.** Nothing marks where a channel runs when the flow is low.

### Ponds, dams, floods
12. **Pond filling.** A beaver pond of a few hundred cubic metres took weeks; on real inflow it
    fills in one to three days.
13. **Spill and wear.** Dam wear compared the crest with any neighbouring surface, dry banks
    included, and treated a few centimetres of ordinary spill as erosion. Real dams wear from
    overtopping floods (head over the crest from the weir law), not from their own trickle.
14. **Leakage.** Dams were watertight. Real beaver dams leak; fine for now (documented).
15. **Floods.** Overbank flow came from the pipe model spreading water, not from flow exceeding
    the channel's bankfull capacity.

### Maps
16. **Scale** (fixed earlier): 20 m cells with a 40-cell river threshold made rivers of 1.6 ha
    gullies; now 4 m cells and rivers only from real catchments entering across the edge.
17. **No growth downstream.** A river carried only what entered at the edge. Real streams gain
    water from the land they drain on the way (lateral inflow), without new streams starting.

## The model that replaces it

At game time scales river flow is in equilibrium, so rivers are computed, not crept:

* **Routing** over the bed (terrain + dams) with the inflow edges closed: every cell drains to
  one neighbour (8 directions), depressions are found as basins.
* **Discharge** (m3/s) at every cell, upstream to downstream: inflows from beyond the map (their
  catchment x unit runoff x season) plus lateral inflow from the land draining to the river.
* **Channel geometry** from the cell's bankfull flow: width 3.0 Q^0.5 m, depth 0.35 Q^0.4 m.
* **Flow depth** by Manning (n = 0.04) on the wetted width (bankfull width x (Q/Qbf)^0.26) and the
  river's local slope; above bankfull the water spreads over the floodplain cells of the cross
  section until it carries the flow.
* **Basins** (ponds behind dams, lakes, any hollow on a river) store water: volume rises by inflow
  minus outflow; outflow is the weir flow over the spill (or the inflow, once full). A breach
  drains the stored water through the gap as a flood wave.
* **Depth grid** (what the rest of the game reads): basin water level minus bed, channel flow
  depth, floodplain depth. Every river cell with flow reports its depth and wetted width; the
  renderer draws the bed and the water ribbon at their real widths, so a stream is always seen.
* **Books:** inflows, lateral inflow, outflow at the map edge and storage balance exactly.
