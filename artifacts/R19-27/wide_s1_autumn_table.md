## wide_s1_autumn: seed 1, --start 1 --cam iso-wide, autumn

### whole frame, wide_s1_autumn_noglow.png

| Terrain | STYLE hex | lit px | lit mean | lit within 12 of hex | shadow px | shadow mean (blend vs hex / vs lit mean) | target 35% / 40% / 45% | shadow px within 12 of a 35-45% blend | worst shadow px error | sample (x,y): measured vs nearest target (blend) |
|---|---|---|---|---|---|---|---|---|---|---|
| Meadow light | #A8D878 | 0 | - | - | 0 | - (0% / -) | #93AC7E / #90A67F / #8D9F80 | - | - | - |
| Meadow | #85C468 | 0 | - | - | 0 | - (0% / -) | #7C9F74 / #7B9A76 / #799477 | - | - | - |
| Meadow deep | #5FA85A | 0 | - | - | 13 | #5F856B (41% / -) | #638D6B / #64896D / #648570 | 100.0% | 6 | (558, 376): #5F846B vs #64896D (40.0%)<br>(881, 493): #5F856B vs #648A6D (39.0%)<br>(649, 635): #60856A vs #64896D (40.0%) |
| Straw | #D9C27A | 15830 | #D9C57D | 100.0% | 3484 | #AB9982 (41% / 41%) | #B29E80 / #AD9880 / #A89381 | 100.0% | 7 | (586, 157): #A6947B vs #AB9781 (41.5%)<br>(857, 441): #AD9C85 vs #AC9880 (40.5%)<br>(719, 638): #AC9981 vs #AD9880 (40.0%) |
| Stone | #B9AFA8 | 21587 | #B7ADA4 | 100.0% | 18706 | #998C99 (42% / 39%) | #9E919E / #9A8D9C / #96899A | 100.0% | 7 | (707, 120): #978A9A vs #978A9B (44.0%)<br>(874, 300): #9C8F98 vs #998C9C (41.5%)<br>(902, 630): #9D9198 vs #9A8D9C (40.0%) |

Penumbra (cyan, excluded) px: 8134. Verdict: PASS

### product frame outside the glow, wide_s1_autumn.png

| Terrain | STYLE hex | lit px | lit mean | lit within 12 of hex | shadow px | shadow mean (blend vs hex / vs lit mean) | target 35% / 40% / 45% | shadow px within 12 of a 35-45% blend | worst shadow px error | sample (x,y): measured vs nearest target (blend) |
|---|---|---|---|---|---|---|---|---|---|---|
| Meadow light | #A8D878 | 0 | - | - | 0 | - (0% / -) | #93AC7E / #90A67F / #8D9F80 | - | - | - |
| Meadow | #85C468 | 0 | - | - | 0 | - (0% / -) | #7C9F74 / #7B9A76 / #799477 | - | - | - |
| Meadow deep | #5FA85A | 0 | - | - | 4 | #5E846B (42% / -) | #638D6B / #64896D / #648570 | 100.0% | 6 | (880, 492): #5E846A vs #648A6D (39.0%)<br>(881, 492): #5D846B vs #648A6C (38.0%)<br>(881, 493): #5F856B vs #648A6D (39.0%) |
| Straw | #D9C27A | 3765 | #D9C57D | 100.0% | 749 | #AC9A82 (40% / 41%) | #B29E80 / #AD9880 / #A89381 | 100.0% | 7 | (905, 333): #AD9A82 vs #AD9880 (40.0%)<br>(862, 401): #AF9C84 vs #AD9880 (40.0%)<br>(719, 638): #AC9981 vs #AD9880 (40.0%) |
| Stone | #B9AFA8 | 9714 | #B7ADA4 | 100.0% | 7671 | #988C99 (42% / 40%) | #9E919E / #9A8D9C / #96899A | 100.0% | 7 | (1015, 241): #978B9A vs #978A9B (43.5%)<br>(1001, 292): #988B99 vs #978A9B (44.0%)<br>(902, 630): #9D9198 vs #9A8D9C (40.0%) |

Penumbra (cyan, excluded) px: 2946. Verdict: PASS

