# R19-27 plum sun shadows: measurements

Renders: `sh tools/render_shadow.sh artifacts/R19-27` (release build, 1280x720). Every view is `beaver_colony --start 1 --seed S --cam iso-close|iso-wide` with `BEAVER_SEASON` set; `iso-close` is the iso angle at 55 units from the map centre, `iso-wide` the same at 150.
Per view: `NAME.png` product frame, `NAME_noglow.png` the same without the warm top-left UI glow overlay, `NAME_lit.png` without sun shadows (classifies terrain), `NAME_mask.png` terrain painted by sun visibility (magenta shadow, cyan penumbra, yellow lit), `NAME_table.md`.

Method (`tools/shadow_check.py`): only pixels whose 3x3 block is one mask colour count. The terrain colour of a pixel is the STYLE hex its `_lit` pixel is within 12 of. A **shadow** pixel passes if it is within 12 per channel of its hex blended somewhere in 35-45% toward Plum `#6B5A8A`. A **lit** pixel passes if it is within 12 of the hex. "Blend vs hex" projects the shadow mean onto hex->Plum. "vs lit mean" does the same against the lit pixels of that class, which is the blend the shader applied (40%).
Each view is checked twice: whole frame on `_noglow.png`, then the product `NAME.png` outside the glow (r < 0.72 x width from the top-left corner; glow alpha measured 0 beyond ~900 px).
Straw is the autumn base grass and Meadow light, Meadow and Meadow deep the summer ones, so the summer and autumn renders of each view together cover all five terrains.

## Summary: shadow pixels (product frame outside the glow; whole no-glow frame in brackets)

| View | Terrain | shadow px | within 12 of a 35-45% blend | worst px error | mean | blend vs hex / vs lit mean | lit px within 12 of hex |
|---|---|---|---|---|---|---|---|
| close_s1_summer | Meadow light | 710 (6159) | 100.0% (100.0%) | 5 (5) | #8CA17F | 44% / 41% | 99.8% (100.0%) |
| close_s1_summer | Meadow | 1883 (5988) | 100.0% (100.0%) | 7 (7) | #7A9776 | 42% / 42% | 100.0% (100.0%) |
| close_s1_summer | Meadow deep | 728 (6095) | 100.0% (100.0%) | 7 (7) | #678D70 | 38% / 40% | 100.0% (100.0%) |
| close_s1_summer | Stone | 35017 (69972) | 100.0% (100.0%) | 6 (6) | #958998 | 46% / 41% | 100.0% (100.0%) |
| close_s1_autumn | Meadow deep | 56 (190) | 100.0% (100.0%) | 7 (7) | #5E846B | 42% / 40% | 100.0% (100.0%) |
| close_s1_autumn | Straw | 2965 (9060) | 100.0% (100.0%) | 7 (7) | #AA9882 | 41% / 41% | 100.0% (100.0%) |
| close_s1_autumn | Stone | 39153 (83129) | 100.0% (100.0%) | 7 (7) | #978B99 | 44% / 40% | 100.0% (100.0%) |
| wide_s1_summer | Meadow light | 435 (8488) | 100.0% (100.0%) | 5 (5) | #8EA47F | 41% / 39% | 100.0% (100.0%) |
| wide_s1_summer | Meadow | 440 (1985) | 100.0% (100.0%) | 7 (7) | #7A9978 | 41% / 41% | 100.0% (100.0%) |
| wide_s1_summer | Meadow deep | 194 (884) | 100.0% (100.0%) | 7 (7) | #678D70 | 38% / 40% | 96.0% (99.8%) |
| wide_s1_summer | Stone | 6518 (13825) | 100.0% (100.0%) | 6 (7) | #968A98 | 45% / 41% | 100.0% (100.0%) |
| wide_s1_autumn | Meadow deep | 4 (13) | 100.0% (100.0%) | 6 (6) | #5E846B | 42% / - | - (-) |
| wide_s1_autumn | Straw | 749 (3484) | 100.0% (100.0%) | 7 (7) | #AC9A82 | 40% / 41% | 100.0% (100.0%) |
| wide_s1_autumn | Stone | 7671 (18706) | 100.0% (100.0%) | 7 (7) | #988C99 | 42% / 40% | 100.0% (100.0%) |
| wide_s2_summer | Meadow light | 1002 (4974) | 100.0% (100.0%) | 4 (6) | #8DA37F | 42% / 39% | 100.0% (99.8%) |
| wide_s2_summer | Meadow | 1010 (3762) | 100.0% (100.0%) | 7 (7) | #7E9B77 | 39% / 39% | 100.0% (99.9%) |
| wide_s2_summer | Meadow deep | 158 (710) | 100.0% (100.0%) | 7 (7) | #678D70 | 38% / 41% | 100.0% (100.0%) |
| wide_s2_summer | Stone | 6655 (17257) | 100.0% (100.0%) | 6 (6) | #968A99 | 45% / 39% | 100.0% (100.0%) |
| wide_s2_autumn | Meadow deep | 3 (19) | 100.0% (100.0%) | 5 (6) | #60866B | 41% / - | - (-) |
| wide_s2_autumn | Straw | 1268 (5375) | 100.0% (100.0%) | 7 (7) | #AE9A83 | 39% / 40% | 100.0% (99.9%) |
| wide_s2_autumn | Stone | 7959 (23346) | 100.0% (100.0%) | 7 (7) | #988C99 | 42% / 38% | 100.0% (100.0%) |

All 12 checks (6 views x 2 frames): PASS.

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



## wide_s1_summer: seed 1, --start 1 --cam iso-wide, summer

### whole frame, wide_s1_summer_noglow.png

| Terrain | STYLE hex | lit px | lit mean | lit within 12 of hex | shadow px | shadow mean (blend vs hex / vs lit mean) | target 35% / 40% / 45% | shadow px within 12 of a 35-45% blend | worst shadow px error | sample (x,y): measured vs nearest target (blend) |
|---|---|---|---|---|---|---|---|---|---|---|
| Meadow light | #A8D878 | 12095 | #A4D477 | 100.0% | 8488 | #8DA27E (43% / 41%) | #93AC7E / #90A67F / #8D9F80 | 100.0% | 5 | (756, 174): #8BA37D vs #8EA280 (43.0%)<br>(441, 497): #8CA27D vs #8EA380 (42.0%)<br>(862, 630): #8EA180 vs #8DA180 (43.5%) |
| Meadow | #85C468 | 11555 | #87C46A | 100.0% | 1985 | #7A9977 (41% / 41%) | #7C9F74 / #7B9A76 / #799477 | 100.0% | 7 | (585, 158): #739473 vs #799477 (45.0%)<br>(871, 403): #7C9B77 vs #7B9A76 (40.0%)<br>(719, 638): #7D9977 vs #7B9B75 (39.0%) |
| Meadow deep | #5FA85A | 2508 | #66B05F | 99.8% | 884 | #678D70 (38% / 41%) | #638D6B / #64896D / #648570 | 100.0% | 7 | (605, 138): #678E70 vs #648A6C (38.0%)<br>(637, 353): #678E73 vs #64886D (40.5%)<br>(650, 636): #678A6D vs #64876E (42.0%) |
| Straw | #D9C27A | 0 | - | - | 0 | - (0% / -) | #B29E80 / #AD9880 / #A89381 | - | - | - |
| Stone | #B9AFA8 | 17721 | #B3ABA3 | 100.0% | 13825 | #958A98 (45% / 41%) | #9E919E / #9A8D9C / #96899A | 100.0% | 7 | (707, 120): #978A99 vs #96899A (45.0%)<br>(937, 293): #968B98 vs #96899A (45.0%)<br>(290, 540): #958E94 vs #96899A (45.0%) |

Penumbra (cyan, excluded) px: 8134. Verdict: PASS

### product frame outside the glow, wide_s1_summer.png

| Terrain | STYLE hex | lit px | lit mean | lit within 12 of hex | shadow px | shadow mean (blend vs hex / vs lit mean) | target 35% / 40% / 45% | shadow px within 12 of a 35-45% blend | worst shadow px error | sample (x,y): measured vs nearest target (blend) |
|---|---|---|---|---|---|---|---|---|---|---|
| Meadow light | #A8D878 | 5032 | #A4D377 | 100.0% | 435 | #8EA47F (41% / 39%) | #93AC7E / #90A67F / #8D9F80 | 100.0% | 5 | (1044, 321): #8F9E85 vs #8D9F80 (45.0%)<br>(1040, 466): #90A67F vs #90A67F (39.5%)<br>(862, 630): #8EA180 vs #8DA180 (43.5%) |
| Meadow | #85C468 | 2669 | #86C469 | 100.0% | 440 | #7A9978 (41% / 41%) | #7C9F74 / #7B9A76 / #799477 | 100.0% | 7 | (905, 333): #7D9B78 vs #7B9A76 (40.0%)<br>(954, 364): #7D9B77 vs #7B9B75 (38.5%)<br>(719, 638): #7D9977 vs #7B9B75 (39.0%) |
| Meadow deep | #5FA85A | 99 | #66B161 | 96.0% | 194 | #678D70 (38% / 40%) | #638D6B / #64896D / #648570 | 100.0% | 7 | (1132, 385): #698E70 vs #64896D (40.0%)<br>(931, 536): #678E72 vs #64896D (39.5%)<br>(973, 588): #648269 vs #64876E (42.0%) |
| Straw | #D9C27A | 0 | - | - | 0 | - (0% / -) | #B29E80 / #AD9880 / #A89381 | - | - | - |
| Stone | #B9AFA8 | 8618 | #B3ABA3 | 100.0% | 6518 | #968A98 (45% / 41%) | #9E919E / #9A8D9C / #96899A | 100.0% | 6 | (1015, 241): #968A99 vs #96899A (45.0%)<br>(1057, 288): #968A99 vs #96899A (45.0%)<br>(827, 539): #968C97 vs #96899A (45.0%) |

Penumbra (cyan, excluded) px: 2946. Verdict: PASS



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

