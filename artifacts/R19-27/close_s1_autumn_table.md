## close_s1_autumn: seed 1, --start 1 --cam iso-close, autumn

### whole frame, close_s1_autumn_noglow.png

| Terrain | STYLE hex | lit px | lit mean | lit within 12 of hex | shadow px | shadow mean (blend vs hex / vs lit mean) | target 35% / 40% / 45% | shadow px within 12 of a 35-45% blend | worst shadow px error | sample (x,y): measured vs nearest target (blend) |
|---|---|---|---|---|---|---|---|---|---|---|
| Meadow light | #A8D878 | 0 | - | - | 0 | - (0% / -) | #93AC7E / #90A67F / #8D9F80 | - | - | - |
| Meadow | #85C468 | 0 | - | - | 0 | - (0% / -) | #7C9F74 / #7B9A76 / #799477 | - | - | - |
| Meadow deep | #5FA85A | 241 | #56A056 | 100.0% | 190 | #5F846B (42% / 40%) | #638D6B / #64896D / #648570 | 100.0% | 7 | (1112, 52): #5D846A vs #648A6C (38.0%)<br>(440, 417): #5E846B vs #648A6D (39.0%)<br>(1231, 551): #5E836A vs #64896D (40.0%) |
| Straw | #D9C27A | 29728 | #D8C37C | 100.0% | 9060 | #AB9983 (41% / 40%) | #B29E80 / #AD9880 / #A89381 | 100.0% | 7 | (352, 50): #A7947D vs #AB9681 (42.0%)<br>(1196, 355): #A89681 vs #A99581 (43.5%)<br>(618, 638): #A8947E vs #AB9681 (42.0%) |
| Stone | #B9AFA8 | 50040 | #B5ABA3 | 100.0% | 83129 | #978B99 (43% / 40%) | #9E919E / #9A8D9C / #96899A | 100.0% | 7 | (23, 50): #968B9B vs #978A9B (43.5%)<br>(1071, 332): #A09297 vs #9B8E9C (39.0%)<br>(1169, 639): #9D8F99 vs #9A8D9C (39.5%) |

Penumbra (cyan, excluded) px: 9529. Verdict: PASS

### product frame outside the glow, close_s1_autumn.png

| Terrain | STYLE hex | lit px | lit mean | lit within 12 of hex | shadow px | shadow mean (blend vs hex / vs lit mean) | target 35% / 40% / 45% | shadow px within 12 of a 35-45% blend | worst shadow px error | sample (x,y): measured vs nearest target (blend) |
|---|---|---|---|---|---|---|---|---|---|---|
| Meadow light | #A8D878 | 0 | - | - | 0 | - (0% / -) | #93AC7E / #90A67F / #8D9F80 | - | - | - |
| Meadow | #85C468 | 0 | - | - | 0 | - (0% / -) | #7C9F74 / #7B9A76 / #799477 | - | - | - |
| Meadow deep | #5FA85A | 93 | #55A056 | 100.0% | 56 | #5E846B (42% / 40%) | #638D6B / #64896D / #648570 | 100.0% | 7 | (1112, 52): #5D846A vs #648A6C (38.0%)<br>(900, 226): #61856C vs #64886E (41.5%)<br>(1231, 551): #5E836A vs #64896D (40.0%) |
| Straw | #D9C27A | 10930 | #D7C37C | 100.0% | 2965 | #AA9882 (41% / 41%) | #B29E80 / #AD9880 / #A89381 | 100.0% | 7 | (1108, 50): #A89680 vs #A99581 (43.5%)<br>(1229, 360): #A6947D vs #AA9581 (43.0%)<br>(1278, 581): #A89883 vs #AA9681 (42.5%) |
| Stone | #B9AFA8 | 30062 | #B5ABA4 | 100.0% | 39153 | #978B99 (44% / 40%) | #9E919E / #9A8D9C / #96899A | 100.0% | 7 | (1259, 88): #968A9A vs #978A9B (44.0%)<br>(883, 547): #948799 vs #96899A (45.0%)<br>(1169, 639): #9D8F99 vs #9A8D9C (39.5%) |

Penumbra (cyan, excluded) px: 3938. Verdict: PASS

