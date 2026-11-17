## close_s1_summer: seed 1, --start 1 --cam iso-close, summer

### whole frame, close_s1_summer_noglow.png

| Terrain | STYLE hex | lit px | lit mean | lit within 12 of hex | shadow px | shadow mean (blend vs hex / vs lit mean) | target 35% / 40% / 45% | shadow px within 12 of a 35-45% blend | worst shadow px error | sample (x,y): measured vs nearest target (blend) |
|---|---|---|---|---|---|---|---|---|---|---|
| Meadow light | #A8D878 | 7553 | #A3D476 | 100.0% | 6052 | #8CA27F (43% / 41%) | #93AC7E / #90A67F / #8D9F80 | 100.0% | 5 | (167, 51): #8EA182 vs #8D9F80 (45.0%)<br>(1185, 308): #8EA082 vs #8D9F80 (45.0%)<br>(1228, 547): #89A17D vs #8D9F80 (45.0%) |
| Meadow | #85C468 | 23975 | #86C369 | 100.0% | 5903 | #7B9977 (41% / 41%) | #7C9F74 / #7B9A76 / #799477 | 100.0% | 7 | (352, 50): #749473 vs #799477 (45.0%)<br>(1194, 358): #7A9677 vs #7A9677 (43.5%)<br>(618, 638): #779473 vs #7A9776 (42.5%) |
| Meadow deep | #5FA85A | 11070 | #66B05F | 100.0% | 5991 | #678E70 (38% / 40%) | #638D6B / #64896D / #648570 | 100.0% | 7 | (979, 62): #668E70 vs #648A6C (38.0%)<br>(651, 342): #678E70 vs #648A6C (38.0%)<br>(625, 592): #6B8F72 vs #64886E (41.0%) |
| Straw | #D9C27A | 0 | - | - | 0 | - (0% / -) | #B29E80 / #AD9880 / #A89381 | - | - | - |
| Stone | #B9AFA8 | 40620 | #B2AAA2 | 100.0% | 69972 | #958998 (46% / 41%) | #9E919E / #9A8D9C / #96899A | 100.0% | 6 | (23, 50): #968A99 vs #96899A (45.0%)<br>(875, 550): #938798 vs #96899A (45.0%)<br>(1169, 639): #968F96 vs #978A9B (43.5%) |

Penumbra (cyan, excluded) px: 9291. Verdict: PASS

### product frame outside the glow, close_s1_summer.png

| Terrain | STYLE hex | lit px | lit mean | lit within 12 of hex | shadow px | shadow mean (blend vs hex / vs lit mean) | target 35% / 40% / 45% | shadow px within 12 of a 35-45% blend | worst shadow px error | sample (x,y): measured vs nearest target (blend) |
|---|---|---|---|---|---|---|---|---|---|---|
| Meadow light | #A8D878 | 1497 | #A1D275 | 99.8% | 711 | #8CA17F (44% / 41%) | #93AC7E / #90A67F / #8D9F80 | 100.0% | 5 | (1192, 275): #8E9E81 vs #8D9F80 (45.0%)<br>(1143, 397): #8B9E7F vs #8D9F80 (45.0%)<br>(1228, 547): #89A17D vs #8D9F80 (45.0%) |
| Meadow | #85C468 | 9465 | #87C36A | 100.0% | 1812 | #7A9776 (42% / 42%) | #7C9F74 / #7B9A76 / #799477 | 100.0% | 7 | (1108, 50): #77987C vs #799477 (45.0%)<br>(1215, 342): #7B9A76 vs #7B9A76 (40.0%)<br>(1231, 551): #759274 vs #799477 (45.0%) |
| Meadow deep | #5FA85A | 2858 | #67B15F | 100.0% | 699 | #678D70 (38% / 40%) | #638D6B / #64896D / #648570 | 100.0% | 7 | (979, 62): #668E70 vs #648A6C (38.0%)<br>(1018, 75): #5E846B vs #648A6D (39.0%)<br>(1231, 550): #5D816A vs #64886E (41.5%) |
| Straw | #D9C27A | 0 | - | - | 0 | - (0% / -) | #B29E80 / #AD9880 / #A89381 | - | - | - |
| Stone | #B9AFA8 | 26133 | #B2AAA3 | 100.0% | 35017 | #958998 (46% / 41%) | #9E919E / #9A8D9C / #96899A | 100.0% | 6 | (1259, 88): #968999 vs #96899A (45.0%)<br>(924, 553): #948899 vs #96899A (45.0%)<br>(1169, 639): #968F96 vs #978A9B (43.5%) |

Penumbra (cyan, excluded) px: 3836. Verdict: PASS

