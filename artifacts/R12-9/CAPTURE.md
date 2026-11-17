Captured from binary built (`cargo build --release -p game`) at commit b46d1f423d5959c09c58d68d21acec0ed7bd6aea (clean tree).
Env: BEAVER_DEMO=1 (required for lake + test object). Seed 1.
- world_seed1.png: `beaver_colony --seed 1 --cam iso --screenshot ... --frames 30`
- close_seed1.png: BEAVER_DEMO_CAM=close, `--seed 1 --screenshot ... --frames 30`
- frame_iso.txt: BEAVER_FRAMELOG=... `--seed 1 --cam iso` (no --screenshot; screenshot mode exits before sampling ends and leaves the log unwritten). avg_ms=1.93
Without BEAVER_DEMO=1 there is no water or object (screenshots show bare terrain).
Not regenerated: *_seed2/3, iso/top/low/autumn/winter, frame_top.
