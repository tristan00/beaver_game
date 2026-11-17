## wide_s1_summer: seed 1, --start 1 --cam iso-wide, summer

### whole frame, wide_s1_summer_noglow.png

| Terrain | STYLE hex | lit px | lit mean | lit within 12 of hex | shadow px | shadow mean (blend vs hex / vs lit mean) | target 35% / 40% / 45% | shadow px within 12 of a 35-45% blend | worst shadow px error | sample (x,y): measured vs nearest target (blend) |
|---|---|---|---|---|---|---|---|---|---|---|
| Meadow light | #A8D878 | 11123 | #A4D477 | 100.0% | 8325 | #8DA27E (43% / 41%) | #93AC7E / #90A67F / #8D9F80 | 100.0% | 5 | (756, 174): #8BA37D vs #8EA280 (43.0%)<br>(454, 496): #8CA17D vs #8EA380 (42.0%)<br>(862, 630): #8EA180 vs #8DA180 (43.5%) |
| Meadow | #85C468 | 10503 | #87C46A | 100.0% | 1971 | #7A9977 (41% / 41%) | #7C9F74 / #7B9A76 / #799477 | 100.0% | 7 | (585, 158): #739473 vs #799477 (45.0%)<br>(866, 404): #819E79 vs #7C9F74 (35.0%)<br>(719, 638): #7D9977 vs #7B9B75 (39.0%) |
| Meadow deep | #5FA85A | 2404 | #66B05F | 99.8% | 837 | #678D70 (38% / 41%) | #638D6B / #64896D / #648570 | 100.0% | 7 | (605, 138): #678E70 vs #648A6C (38.0%)<br>(640, 351): #698E73 vs #64886D (40.5%)<br>(649, 636): #698B6E vs #64866F (43.0%) |
| Straw | #D9C27A | 0 | - | - | 0 | - (0% / -) | #B29E80 / #AD9880 / #A89381 | - | - | - |
| Stone | #B9AFA8 | 17721 | #B3ABA3 | 100.0% | 13824 | #958A98 (45% / 41%) | #9E919E / #9A8D9C / #96899A | 100.0% | 7 | (707, 120): #978A99 vs #96899A (45.0%)<br>(937, 293): #968B98 vs #96899A (45.0%)<br>(290, 540): #958E94 vs #96899A (45.0%) |

Penumbra (cyan, excluded) px: 8109. Verdict: PASS

### product frame outside the glow, wide_s1_summer.png

| Terrain | STYLE hex | lit px | lit mean | lit within 12 of hex | shadow px | shadow mean (blend vs hex / vs lit mean) | target 35% / 40% / 45% | shadow px within 12 of a 35-45% blend | worst shadow px error | sample (x,y): measured vs nearest target (blend) |
|---|---|---|---|---|---|---|---|---|---|---|
| Meadow light | #A8D878 | 4719 | #A4D377 | 100.0% | 411 | #8EA47F (41% / 39%) | #93AC7E / #90A67F / #8D9F80 | 100.0% | 5 | (1044, 321): #8F9E85 vs #8D9F80 (45.0%)<br>(1061, 465): #8FA580 vs #8FA47F (41.0%)<br>(862, 630): #8EA180 vs #8DA180 (43.5%) |
| Meadow | #85C468 | 2387 | #86C469 | 100.0% | 438 | #7A9978 (41% / 41%) | #7C9F74 / #7B9A76 / #799477 | 100.0% | 7 | (905, 333): #7D9B78 vs #7B9A76 (40.0%)<br>(954, 364): #7D9B77 vs #7B9B75 (38.5%)<br>(719, 638): #7D9977 vs #7B9B75 (39.0%) |
| Meadow deep | #5FA85A | 95 | #66B161 | 95.8% | 172 | #678E70 (38% / 40%) | #638D6B / #64896D / #648570 | 100.0% | 7 | (1132, 385): #698E70 vs #64896D (40.0%)<br>(940, 535): #668D6F vs #648A6C (38.0%)<br>(973, 588): #648269 vs #64876E (42.0%) |
| Straw | #D9C27A | 0 | - | - | 0 | - (0% / -) | #B29E80 / #AD9880 / #A89381 | - | - | - |
| Stone | #B9AFA8 | 8618 | #B3ABA3 | 100.0% | 6518 | #968A98 (45% / 41%) | #9E919E / #9A8D9C / #96899A | 100.0% | 6 | (1015, 241): #968A99 vs #96899A (45.0%)<br>(1057, 288): #968A99 vs #96899A (45.0%)<br>(827, 539): #968C97 vs #96899A (45.0%) |

Penumbra (cyan, excluded) px: 2939. Verdict: PASS

