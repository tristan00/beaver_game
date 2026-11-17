# R19-23 screenshots (Start 2, seed 1, release build)

- `start2-join-request.png`: `BC_DEMO_ALERTS=1 beaver_colony --start 2 --seed 1 --frames 60 --screenshot ...`
  A wanderer's join request toast with **Welcome** / **Turn away**. Every toast also puts a pin
  (stem + rings) on the map at its place.
- `start2-alert-on-building.png`: `BC_DEMO_ALERTS=jump beaver_colony --start 2 --seed 1 --frames 60 --screenshot ...`
  The "Lodge site has no workers" toast is activated through the same `panels::apply(Do::Jump)` a
  click runs. The camera flies to the site's centre cell, then zooms in (distance 38). The capture
  waits until the camera arrives. The pin and the selection ring sit on the unbuilt lodge frame by the river.
