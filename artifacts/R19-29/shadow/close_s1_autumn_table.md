## close_s1_autumn: seed 1, --start 1 --cam iso-close, autumn

### whole frame, close_s1_autumn_noglow.png

| Terrain | STYLE hex | lit px | lit mean | lit within 12 of hex | shadow px | shadow mean (blend vs hex / vs lit mean) | target 35% / 40% / 45% | shadow px within 12 of a 35-45% blend | worst shadow px error | sample (x,y): measured vs nearest target (blend) |
|---|---|---|---|---|---|---|---|---|---|---|
| Meadow light | #A8D878 | 0 | - | - | 0 | - (0% / -) | #93AC7E / #90A67F / #8D9F80 | - | - | - |
| Meadow | #85C468 | 0 | - | - | 0 | - (0% / -) | #7C9F74 / #7B9A76 / #799477 | - | - | - |
| Meadow deep | #5FA85A | 81 | #55A055 | 100.0% | 134 | #5E846B (42% / 40%) | #638D6B / #64896D / #648570 | 100.0% | 7 | (308, 72): #5F866B vs #648A6C (38.0%)<br>(889, 240): #5D846B vs #648A6C (38.0%)<br>(1231, 550): #5D826B vs #64896D (40.0%) |
| Straw | #D9C27A | 28589 | #D8C37C | 100.0% | 8978 | #AB9983 (41% / 40%) | #B29E80 / #AD9880 / #A89381 | 100.0% | 7 | (352, 50): #A7947D vs #AB9681 (42.0%)<br>(1224, 357): #A6947C vs #AB9681 (42.0%)<br>(618, 638): #A8947E vs #AB9681 (42.0%) |
| Stone | #B9AFA8 | 50040 | #B5ABA3 | 100.0% | 83129 | #978B99 (43% / 40%) | #9E919E / #9A8D9C / #96899A | 100.0% | 7 | (23, 50): #968B9B vs #978A9B (43.5%)<br>(1071, 332): #A09297 vs #9B8E9C (39.0%)<br>(1169, 639): #9D8F99 vs #9A8D9C (39.5%) |

Penumbra (cyan, excluded) px: 9291. Verdict: PASS

### product frame outside the glow, close_s1_autumn.png

| Terrain | STYLE hex | lit px | lit mean | lit within 12 of hex | shadow px | shadow mean (blend vs hex / vs lit mean) | target 35% / 40% / 45% | shadow px within 12 of a 35-45% blend | worst shadow px error | sample (x,y): measured vs nearest target (blend) |
|---|---|---|---|---|---|---|---|---|---|---|
| Meadow light | #A8D878 | 0 | - | - | 0 | - (0% / -) | #93AC7E / #90A67F / #8D9F80 | - | - | - |
| Meadow | #85C468 | 0 | - | - | 0 | - (0% / -) | #7C9F74 / #7B9A76 / #799477 | - | - | - |
| Meadow deep | #5FA85A | 24 | #55A055 | 100.0% | 28 | #5F856B (42% / 40%) | #638D6B / #64896D / #648570 | 100.0% | 6 | (1018, 74): #5E846B vs #648A6D (39.0%)<br>(870, 321): #5F866B vs #648A6C (38.0%)<br>(1231, 550): #5D826B vs #64896D (40.0%) |
| Straw | #D9C27A | 10573 | #D7C37C | 100.0% | 2888 | #AA9882 (41% / 41%) | #B29E80 / #AD9880 / #A89381 | 100.0% | 7 | (1108, 50): #A89680 vs #A99581 (43.5%)<br>(1194, 361): #A89581 vs #A99481 (44.0%)<br>(1278, 581): #A89883 vs #AA9681 (42.5%) |
| Stone | #B9AFA8 | 30062 | #B5ABA4 | 100.0% | 39153 | #978B99 (44% / 40%) | #9E919E / #9A8D9C / #96899A | 100.0% | 7 | (1259, 88): #968A9A vs #978A9B (44.0%)<br>(883, 547): #948799 vs #96899A (45.0%)<br>(1169, 639): #9D8F99 vs #9A8D9C (39.5%) |

Penumbra (cyan, excluded) px: 3836. Verdict: PASS

