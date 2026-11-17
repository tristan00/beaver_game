## close_s1_summer: seed 1, --start 1 --cam iso-close, summer

### whole frame, close_s1_summer_noglow.png

| Terrain | STYLE hex | lit px | lit mean | lit within 12 of hex | shadow px | shadow mean (blend vs hex / vs lit mean) | target 35% / 40% / 45% | shadow px within 12 of a 35-45% blend | worst shadow px error | sample (x,y): measured vs nearest target (blend) |
|---|---|---|---|---|---|---|---|---|---|---|
| Meadow light | #A8D878 | 7803 | #A3D476 | 100.0% | 6159 | #8DA27F (43% / 41%) | #93AC7E / #90A67F / #8D9F80 | 100.0% | 5 | (167, 51): #8EA182 vs #8D9F80 (45.0%)<br>(180, 309): #8D9E81 vs #8D9F80 (45.0%)<br>(1226, 546): #89A17D vs #8D9F80 (45.0%) |
| Meadow | #85C468 | 24915 | #86C369 | 100.0% | 5988 | #7B9977 (41% / 41%) | #7C9F74 / #7B9A76 / #799477 | 100.0% | 7 | (352, 50): #749473 vs #799477 (45.0%)<br>(940, 357): #7A9975 vs #7B9A76 (40.0%)<br>(618, 638): #779473 vs #7A9776 (42.5%) |
| Meadow deep | #5FA85A | 12034 | #65B05F | 100.0% | 6095 | #678D70 (38% / 40%) | #638D6B / #64896D / #648570 | 100.0% | 7 | (1113, 51): #64886D vs #64886D (40.5%)<br>(620, 344): #678E70 vs #648A6C (38.0%)<br>(625, 592): #6B8F72 vs #64886E (41.0%) |
| Straw | #D9C27A | 0 | - | - | 0 | - (0% / -) | #B29E80 / #AD9880 / #A89381 | - | - | - |
| Stone | #B9AFA8 | 40620 | #B2AAA2 | 100.0% | 69972 | #958998 (46% / 41%) | #9E919E / #9A8D9C / #96899A | 100.0% | 6 | (23, 50): #968A99 vs #96899A (45.0%)<br>(875, 550): #938798 vs #96899A (45.0%)<br>(1169, 639): #968F96 vs #978A9B (43.5%) |

Penumbra (cyan, excluded) px: 9529. Verdict: PASS

### product frame outside the glow, close_s1_summer.png

| Terrain | STYLE hex | lit px | lit mean | lit within 12 of hex | shadow px | shadow mean (blend vs hex / vs lit mean) | target 35% / 40% / 45% | shadow px within 12 of a 35-45% blend | worst shadow px error | sample (x,y): measured vs nearest target (blend) |
|---|---|---|---|---|---|---|---|---|---|---|
| Meadow light | #A8D878 | 1517 | #A1D275 | 99.8% | 710 | #8CA17F (44% / 41%) | #93AC7E / #90A67F / #8D9F80 | 100.0% | 5 | (1192, 275): #8E9E81 vs #8D9F80 (45.0%)<br>(1136, 397): #8A9E7E vs #8D9F80 (45.0%)<br>(1226, 546): #89A17D vs #8D9F80 (45.0%) |
| Meadow | #85C468 | 9808 | #87C36A | 100.0% | 1883 | #7A9776 (42% / 42%) | #7C9F74 / #7B9A76 / #799477 | 100.0% | 7 | (1108, 50): #77987C vs #799477 (45.0%)<br>(891, 340): #7B9A76 vs #7B9A76 (40.0%)<br>(1236, 544): #7D9979 vs #7A9876 (41.5%) |
| Meadow deep | #5FA85A | 3095 | #66B05F | 100.0% | 728 | #678D70 (38% / 40%) | #638D6B / #64896D / #648570 | 100.0% | 7 | (1113, 51): #64886D vs #64886D (40.5%)<br>(1021, 75): #668E70 vs #648A6C (38.0%)<br>(1232, 551): #6A8B6F vs #64856F (44.5%) |
| Straw | #D9C27A | 0 | - | - | 0 | - (0% / -) | #B29E80 / #AD9880 / #A89381 | - | - | - |
| Stone | #B9AFA8 | 26133 | #B2AAA3 | 100.0% | 35017 | #958998 (46% / 41%) | #9E919E / #9A8D9C / #96899A | 100.0% | 6 | (1259, 88): #968999 vs #96899A (45.0%)<br>(924, 553): #948899 vs #96899A (45.0%)<br>(1169, 639): #968F96 vs #978A9B (43.5%) |

Penumbra (cyan, excluded) px: 3938. Verdict: PASS

