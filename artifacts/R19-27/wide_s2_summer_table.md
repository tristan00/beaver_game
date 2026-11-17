## wide_s2_summer: seed 2, --start 1 --cam iso-wide, summer

### whole frame, wide_s2_summer_noglow.png

| Terrain | STYLE hex | lit px | lit mean | lit within 12 of hex | shadow px | shadow mean (blend vs hex / vs lit mean) | target 35% / 40% / 45% | shadow px within 12 of a 35-45% blend | worst shadow px error | sample (x,y): measured vs nearest target (blend) |
|---|---|---|---|---|---|---|---|---|---|---|
| Meadow light | #A8D878 | 6456 | #A2D476 | 99.8% | 4974 | #8CA27E (43% / 41%) | #93AC7E / #90A67F / #8D9F80 | 100.0% | 6 | (743, 173): #8FA580 vs #8FA47F (41.0%)<br>(821, 473): #90A57F vs #90A67F (40.0%)<br>(896, 639): #90A67F vs #90A67F (39.5%) |
| Meadow | #85C468 | 13017 | #87C56A | 99.9% | 3762 | #7D9978 (40% / 41%) | #7C9F74 / #7B9A76 / #799477 | 100.0% | 7 | (635, 141): #749573 vs #799477 (45.0%)<br>(367, 477): #7B9579 vs #799477 (45.0%)<br>(477, 638): #7C9974 vs #7B9A75 (39.5%) |
| Meadow deep | #5FA85A | 3332 | #65B05F | 100.0% | 710 | #678D70 (38% / 41%) | #638D6B / #64896D / #648570 | 100.0% | 7 | (616, 137): #678E70 vs #648A6C (38.0%)<br>(571, 328): #678E70 vs #648A6C (38.0%)<br>(583, 601): #6A8B6F vs #64856F (44.5%) |
| Straw | #D9C27A | 0 | - | - | 0 | - (0% / -) | #B29E80 / #AD9880 / #A89381 | - | - | - |
| Stone | #B9AFA8 | 16236 | #B2A9A3 | 100.0% | 17257 | #958A98 (45% / 40%) | #9E919E / #9A8D9C / #96899A | 100.0% | 6 | (806, 153): #968A99 vs #96899A (45.0%)<br>(799, 394): #958B98 vs #96899A (45.0%)<br>(243, 494): #948D95 vs #96899A (45.0%) |

Penumbra (cyan, excluded) px: 5959. Verdict: PASS

### product frame outside the glow, wide_s2_summer.png

| Terrain | STYLE hex | lit px | lit mean | lit within 12 of hex | shadow px | shadow mean (blend vs hex / vs lit mean) | target 35% / 40% / 45% | shadow px within 12 of a 35-45% blend | worst shadow px error | sample (x,y): measured vs nearest target (blend) |
|---|---|---|---|---|---|---|---|---|---|---|
| Meadow light | #A8D878 | 3255 | #A1D475 | 100.0% | 1002 | #8DA37F (42% / 39%) | #93AC7E / #90A67F / #8D9F80 | 100.0% | 4 | (1003, 296): #8CA37F vs #8EA280 (43.0%)<br>(810, 479): #90A580 vs #8FA57F (40.5%)<br>(896, 639): #90A67F vs #90A67F (39.5%) |
| Meadow | #85C468 | 4565 | #86C469 | 100.0% | 1010 | #7E9B77 (39% / 39%) | #7C9F74 / #7B9A76 / #799477 | 100.0% | 7 | (940, 254): #819D78 vs #7C9F74 (35.0%)<br>(973, 453): #7B9976 vs #7A9976 (40.5%)<br>(909, 624): #7A9676 vs #7A9677 (43.0%) |
| Meadow deep | #5FA85A | 1143 | #64B05E | 100.0% | 158 | #678D70 (38% / 41%) | #638D6B / #64896D / #648570 | 100.0% | 7 | (1138, 380): #698573 vs #648570 (45.0%)<br>(992, 506): #678E70 vs #648A6C (38.0%)<br>(1026, 517): #678E6F vs #648B6C (37.5%) |
| Straw | #D9C27A | 0 | - | - | 0 | - (0% / -) | #B29E80 / #AD9880 / #A89381 | - | - | - |
| Stone | #B9AFA8 | 4773 | #B2A8A3 | 100.0% | 6655 | #968A99 (45% / 39%) | #9E919E / #9A8D9C / #96899A | 100.0% | 6 | (866, 327): #938798 vs #96899A (45.0%)<br>(1063, 403): #96899A vs #96899A (45.0%)<br>(1075, 452): #959095 vs #978A9B (43.5%) |

Penumbra (cyan, excluded) px: 1701. Verdict: PASS

