## wide_s2_summer: seed 2, --start 1 --cam iso-wide, summer

### whole frame, wide_s2_summer_noglow.png

| Terrain | STYLE hex | lit px | lit mean | lit within 12 of hex | shadow px | shadow mean (blend vs hex / vs lit mean) | target 35% / 40% / 45% | shadow px within 12 of a 35-45% blend | worst shadow px error | sample (x,y): measured vs nearest target (blend) |
|---|---|---|---|---|---|---|---|---|---|---|
| Meadow light | #A8D878 | 6028 | #A2D476 | 99.8% | 4922 | #8CA27E (43% / 41%) | #93AC7E / #90A67F / #8D9F80 | 100.0% | 6 | (743, 173): #8FA580 vs #8FA47F (41.0%)<br>(525, 473): #8A9F7F vs #8D9F80 (45.0%)<br>(896, 639): #90A67F vs #90A67F (39.5%) |
| Meadow | #85C468 | 12147 | #87C56A | 99.9% | 3733 | #7D9978 (40% / 41%) | #7C9F74 / #7B9A76 / #799477 | 100.0% | 7 | (635, 141): #749573 vs #799477 (45.0%)<br>(372, 477): #7D957C vs #799477 (45.0%)<br>(477, 638): #7C9974 vs #7B9A75 (39.5%) |
| Meadow deep | #5FA85A | 2916 | #65B05F | 100.0% | 653 | #678E70 (38% / 40%) | #638D6B / #64896D / #648570 | 100.0% | 7 | (616, 137): #678E70 vs #648A6C (38.0%)<br>(578, 327): #678E70 vs #648A6C (38.0%)<br>(584, 600): #698C6F vs #64876E (42.0%) |
| Straw | #D9C27A | 0 | - | - | 0 | - (0% / -) | #B29E80 / #AD9880 / #A89381 | - | - | - |
| Stone | #B9AFA8 | 16236 | #B2A9A3 | 100.0% | 17257 | #958A98 (45% / 40%) | #9E919E / #9A8D9C / #96899A | 100.0% | 6 | (806, 153): #968A99 vs #96899A (45.0%)<br>(799, 394): #958B98 vs #96899A (45.0%)<br>(243, 494): #948D95 vs #96899A (45.0%) |

Penumbra (cyan, excluded) px: 5904. Verdict: PASS

### product frame outside the glow, wide_s2_summer.png

| Terrain | STYLE hex | lit px | lit mean | lit within 12 of hex | shadow px | shadow mean (blend vs hex / vs lit mean) | target 35% / 40% / 45% | shadow px within 12 of a 35-45% blend | worst shadow px error | sample (x,y): measured vs nearest target (blend) |
|---|---|---|---|---|---|---|---|---|---|---|
| Meadow light | #A8D878 | 2936 | #A1D475 | 100.0% | 961 | #8DA37F (42% / 39%) | #93AC7E / #90A67F / #8D9F80 | 100.0% | 4 | (1003, 296): #8CA37F vs #8EA280 (43.0%)<br>(819, 478): #8FA57F vs #8FA57F (40.5%)<br>(896, 639): #90A67F vs #90A67F (39.5%) |
| Meadow | #85C468 | 4171 | #87C469 | 100.0% | 989 | #7D9B77 (39% / 39%) | #7C9F74 / #7B9A76 / #799477 | 100.0% | 7 | (940, 254): #819D78 vs #7C9F74 (35.0%)<br>(980, 452): #7F9C77 vs #7C9F74 (35.0%)<br>(878, 596): #7E9C77 vs #7C9D74 (36.5%) |
| Meadow deep | #5FA85A | 998 | #64B05E | 100.0% | 152 | #678D70 (38% / 41%) | #638D6B / #64896D / #648570 | 100.0% | 7 | (1138, 380): #698573 vs #648570 (45.0%)<br>(988, 506): #668E70 vs #648A6C (38.0%)<br>(792, 565): #5F856B vs #648A6D (39.0%) |
| Straw | #D9C27A | 0 | - | - | 0 | - (0% / -) | #B29E80 / #AD9880 / #A89381 | - | - | - |
| Stone | #B9AFA8 | 4773 | #B2A8A3 | 100.0% | 6655 | #968A99 (45% / 39%) | #9E919E / #9A8D9C / #96899A | 100.0% | 6 | (866, 327): #938798 vs #96899A (45.0%)<br>(1063, 403): #96899A vs #96899A (45.0%)<br>(1075, 452): #959095 vs #978A9B (43.5%) |

Penumbra (cyan, excluded) px: 1676. Verdict: PASS

