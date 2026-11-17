## wide_s2_autumn: seed 2, --start 1 --cam iso-wide, autumn

### whole frame, wide_s2_autumn_noglow.png

| Terrain | STYLE hex | lit px | lit mean | lit within 12 of hex | shadow px | shadow mean (blend vs hex / vs lit mean) | target 35% / 40% / 45% | shadow px within 12 of a 35-45% blend | worst shadow px error | sample (x,y): measured vs nearest target (blend) |
|---|---|---|---|---|---|---|---|---|---|---|
| Meadow light | #A8D878 | 0 | - | - | 0 | - (0% / -) | #93AC7E / #90A67F / #8D9F80 | - | - | - |
| Meadow | #85C468 | 0 | - | - | 0 | - (0% / -) | #7C9F74 / #7B9A76 / #799477 | - | - | - |
| Meadow deep | #5FA85A | 0 | - | - | 19 | #5F856B (42% / -) | #638D6B / #64896D / #648570 | 100.0% | 6 | (564, 326): #5E846B vs #648A6D (39.0%)<br>(480, 542): #5F856A vs #648A6D (39.0%)<br>(584, 600): #60866C vs #648A6D (39.0%) |
| Straw | #D9C27A | 18818 | #D9C57D | 99.9% | 5375 | #AC9A84 (40% / 40%) | #B29E80 / #AD9880 / #A89381 | 100.0% | 7 | (635, 141): #A7957D vs #AB9681 (42.0%)<br>(376, 475): #A89785 vs #A89381 (45.0%)<br>(477, 638): #AC977F vs #AD9880 (40.0%) |
| Stone | #B9AFA8 | 20663 | #B4AAA3 | 100.0% | 23346 | #988B99 (43% / 39%) | #9E919E / #9A8D9C / #96899A | 100.0% | 7 | (806, 153): #968A9A vs #978A9B (44.0%)<br>(963, 397): #968A9A vs #978A9B (44.0%)<br>(248, 502): #A19395 vs #9A8D9C (39.5%) |

Penumbra (cyan, excluded) px: 5959. Verdict: PASS

### product frame outside the glow, wide_s2_autumn.png

| Terrain | STYLE hex | lit px | lit mean | lit within 12 of hex | shadow px | shadow mean (blend vs hex / vs lit mean) | target 35% / 40% / 45% | shadow px within 12 of a 35-45% blend | worst shadow px error | sample (x,y): measured vs nearest target (blend) |
|---|---|---|---|---|---|---|---|---|---|---|
| Meadow light | #A8D878 | 0 | - | - | 0 | - (0% / -) | #93AC7E / #90A67F / #8D9F80 | - | - | - |
| Meadow | #85C468 | 0 | - | - | 0 | - (0% / -) | #7C9F74 / #7B9A76 / #799477 | - | - | - |
| Meadow deep | #5FA85A | 0 | - | - | 3 | #60866B (41% / -) | #638D6B / #64896D / #648570 | 100.0% | 5 | (960, 449): #61876C vs #648A6D (39.0%)<br>(961, 449): #61876C vs #648A6D (39.0%)<br>(805, 477): #5E846A vs #648A6D (39.0%) |
| Straw | #D9C27A | 6390 | #D9C57D | 100.0% | 1268 | #AE9A83 (39% / 40%) | #B29E80 / #AD9880 / #A89381 | 100.0% | 7 | (940, 254): #AF9C84 vs #AD9880 (40.0%)<br>(967, 451): #AD9880 vs #AD9880 (40.0%)<br>(878, 596): #AE9B83 vs #AD9880 (40.0%) |
| Stone | #B9AFA8 | 4892 | #B3A9A4 | 100.0% | 7959 | #988C99 (42% / 38%) | #9E919E / #9A8D9C / #96899A | 100.0% | 7 | (866, 327): #938799 vs #96899A (45.0%)<br>(983, 406): #968A9B vs #978A9B (44.0%)<br>(1058, 460): #A19496 vs #9B8E9C (39.0%) |

Penumbra (cyan, excluded) px: 1701. Verdict: PASS

