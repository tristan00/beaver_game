# Real study area for the beaver game map: Lunde river, Telemark, Norway

Researched 2026-11-17. Support ratings: **strong** = read in primary text; **moderate** = seen in a
secondary quote or a search summary, not the primary text; **weak/unverified** = not confirmed.

## Recommendation

Use the lowland Lunde river / Telemark Canal corridor around **Lunde, Nome municipality, Telemark,
Norway** (about 59.298 N, 9.103 E). It is a slow, low-gradient, forested-and-farmland river corridor
in the core of the Norwegian beaver range, and the literature calls its colony density high.

## Claim 1: high beaver density (acceptance 1)

| Claim | Source | Support |
|---|---|---|
| On the Lunde river there is "one colony per 1.4 km" and on the nearby Gvarv river "one colony per 1.2 km", described as high density. Cited there to Rosell & Hovde (2001). | Haarberg & Rosell, "Selective foraging ..." (Telemark study), USN Open Archive https://openarchive.usn.no/usn-xmlui/handle/11250/2438063 (PDF: https://openarchive.usn.no/usn-xmlui/bitstream/11250/2438063/2/Selective_foraging.pdf) | **Moderate.** Quote came via search summaries of this paper. The archive has since been retired (redirects to https://nva.sikt.no) and I could not re-read the PDF. Journal, year and DOI are **unverified**, and so is the original Rosell & Hovde (2001) reference (not located). |
| Same paper places the Lunde in Nome and Sauherad municipalities; the studied section is bounded by canal locks and weirs; slow-flowing, mostly clear water. | Same | Moderate |
| Beaver colony density is highest along low-gradient rivers and farmland, lowest in alpine areas. | "Beaver Management in Norway - A Review of Recent Literature and Current Problems", https://openarchive.usn.no/usn-xmlui/handle/11250/2439204 (now retired) | Moderate (search summary only) |
| Landscape-scale (>100 km2) density in south-central Scandinavia averages about 1 occupied site per 4 km2; in Bø (Telemark), occupied sites track utilized shoreline length, not habitat area. 13 management units of 794-2200 ha. | Parker, Steifetten, Uren & Rosell 2013, "Use of linear and areal habitat models to establish and distribute beaver Castor fiber harvest quotas in Norway", *Fauna norvegica* 33: 29-34, https://doi.org/10.5324/fn.v33i0.1591 | **Strong** (abstract read: https://ntnu.no/ojs/index.php/fauna_norvegica/article/view/1591). It is per km2, a different unit from per-km-of-river. |
| Other published per-river-km figures for context: Bø 0.64, Lunde 1.4, Saua 0.53 colonies/km (1998); Mordovia, Russia 0.2-0.94 colonies/km (avg 0.46-0.52); eastern Germany 1.15 beavers/km shoreline. | Search summaries of Rosell & Bjørkøyli; Forestry Studies 2017 (https://doi.org/10.1515/fsmu-2017-0016); Brugger et al. 2020 https://iucnosgbull.org/Volume37/Brugger_et_al_2020.pdf | Weak-moderate. Only the Mordovia/Germany ones were seen in a search summary; not read in full. |

**Disagreement/ambiguity.** The density figures use different units (colonies/km river, colonies/km2,
beavers/km shoreline), so they do not compare directly. Note that "1 colony per 1.4 km" is 0.71 colonies/km,
while a different summary quotes "1.4 colonies/km" for the Lunde in 1998 (likely a misreading of the same
figure; **unresolved** until the primary text is read). Either way the Lunde sits at the top of the Norwegian range.
Also, Wikipedia places the settlement of Lunde "along the river Straumen" and the Telemark Canal, not the
Lunde river by name; the study river section was not located on a map.

## Claim 2: bounds sized to the game map (acceptance 2)

The sim world is a square grid, `WorldParams::DEFAULT_SIZE = 128` cells, minimum 64
(`crates/sim/src/worldgen.rs:52,212`). The repo defines no metres-per-cell scale, so this report **proposes
25 m per cell**: 128 x 25 m = **3.2 km x 3.2 km**. That holds about 2 beaver territories on a river at 1.4 km per
colony, with several km of channel. (Any other scale rescales the box; for 64 cells, use 1.6 km.)

Centre: Kartverket place-name entry "Lunde" (tettsted, Nome), 59.29832 N, 9.10268 E
(https://stadnamn.kartverket.no/fakta/609164). Wikipedia gives 59.29839 N, 9.1057 E (180 m east); the
sources disagree slightly, Kartverket is used.

| | min | max |
|---|---|---|
| Latitude (WGS84) | 59.28393 | 59.31271 |
| Longitude (WGS84) | 9.07452 | 9.13084 |
| UTM 32N easting (m) | 504 250 | 507 450 |
| UTM 32N northing (m) | 6 571 670 | 6 574 870 |

Derived with 1 deg lat = 111.2 km and 1 deg lon = 56.8 km at 59.3 N; the UTM values are a hand conversion
(about +-10 m) and should be recomputed with a projection tool before cutting data. Box = 3.2 km square.
**Not verified:** that the Lunde river itself, rather than the Straumen/canal reach, runs through the box
centre; check on Norgeskart before cutting data, and shift the box to the river if needed (the size stays).

## Datasets (acceptance 3)

| Layer | Dataset and download | Licence | Derived data may ship? | Support |
|---|---|---|---|---|
| Elevation | **Nasjonal høydemodell DTM1** ("DTM 1 Høydedata", Kartverket's national 1 m laser-scan terrain model, GeoTIFF in 15 km tiles, EPSG:25833). Catalogue record: Geonorge **fd96e6c3-eff6-437c-8fd6-e7c4718b1bf8** ("DTM1 Atom feed-tjeneste", https://kartkatalog.geonorge.no/api/getdata/fd96e6c3-eff6-437c-8fd6-e7c4718b1bf8), the download record for the dataset. Direct download: the whole 3.2 km box (lat 59.28393-59.31271, lon 9.07452-9.13084) lies inside one tile, **https://nedlasting.geonorge.no/hoydedata/DTM1/33-117-110.tif** (tile extent lat 59.183-59.329, lon 8.941-9.226; municipalities Midt-Telemark, Nome, Drangedal; laser projects "NDH Skien-Nome-Sauherad 5pkt 2016", "Drangedal Nome 2022"; HTTP 200, 1.19 GB, checked 2026-11-17). Tile index: https://nedlasting.geonorge.no/geonorge/ATOM/hoydedata/datasett/DTM1.atom. Not chosen: the WCS record 05821c51-2f5b-411f-9098-924d13dbea9a ("Nasjonal høydemodell Digital terrengmodell 25832 WCS") is a service record whose licence field says only "Lisens" plus the INSPIRE code "No conditions apply to access and use", with no named licence. Also not chosen: historic DTM 10 (e.g. fd851873-f363-46f9-9fc6-bb1b403575df), whose record says **CC BY-NC 4.0**, so it cannot ship in a commercial game. | Record fd96e6c3 `Constraints`: `"AccessConstraints": "Åpne data"`, `"OtherConstraintsLinkText": "Creative Commons BY 4.0 (CC BY 4.0)"`, `"OtherConstraintsLink": "https://creativecommons.org/licenses/by/4.0/"`, `"UseLimitations": "Ingen begrensninger"` (no restrictions). Kartverket's terms page (https://www.kartverket.no/api-og-data/vilkar-for-bruk, updated 21.07.26) agrees: free products are "frigitt for gratis bruk til både kommersielle og ikkje-kommersielle føremål" under "Creative Commons Navngivelse 4.0 international (CC BY 4.0)", and the name must be shown wherever the products "eller uttrekk av produkta" (or extracts) are used, as "© Kartverket". **Disagreement:** the Atom feed files themselves carry `<rights>` NLOD 2.0 (https://data.norge.no/nlod/no/2.0/), not CC BY 4.0. Both licences allow commercial use, adaptation and redistribution with attribution, so the conclusion does not change. | **Yes.** CC BY 4.0 allows sharing and adapting for any purpose, commercial included, with attribution. Credit "© Kartverket" (and, to satisfy NLOD too, name Kartverket as the source). | **Strong** for the record, its CC BY 4.0 tag, the tile covering the box and the download URL (all read directly from the Geonorge API, the Atom feed and an HTTP HEAD request on 2026-11-17). Moderate on which licence governs if CC BY 4.0 and NLOD 2.0 were ever read differently; both permit shipping. The dataset-level record 0442c622-6639-4024-86be-846de8c15bb2 that the feed points to now returns 404 in the catalogue API, so the licence is quoted from the download record. |
| Rivers | **NVE Elvenett / ELVIS** (river network lines, themes "Hovedelv" and "Elvenett", each segment has id `strekn_lnr`). Download: NVE's GIS download page http://nedlasting.nve.no/gis (the Geonorge record's own distribution link). Catalogue record: Geonorge UUID 3f95a194-0968-4457-a500-912958de3d39 (https://register.geonorge.no/inspire-statusregister/elvis-elvenett/3f95a194-0968-4457-a500-912958de3d39); product sheet https://register.geonorge.no/data/documents/Elvenett - ELVIS_v2_produktark-elvis-elvenett_.pdf. (Earlier draft cited api.nve.no, which is the hydrology gauge API and does not serve river geometry; that was wrong.) Fallback: Kartverket N50 water (v1 of ELVIS was derived from N50). | **NLOD 1.0** (http://data.norge.no/nlod/no/1.0), read in the Geonorge catalogue API ("Norsk lisens for offentlige data (NLOD)", "Ingen begrensninger på bruk er oppgitt") and in the product sheet ("distribueres i henhold til NLOD"). Required credit: "Kilde: NVE". | Yes: NLOD permits copying, adapting and redistributing with attribution (NLOD text itself still not read) | **Strong** for dataset, download location and licence (two primary NVE/Geonorge sources agree); moderate for NLOD terms |
| Forest cover | ESA WorldCover 10 m (2020 v100, 2021 v200): https://esa-worldcover.org/en/data-access ; AWS `s3://esa-worldcover` (no sign-in); Zenodo 10.5281/zenodo.5571936 and 10.5281/zenodo.7254221 | CC BY 4.0 ("free of charge, without restriction of use"; attribution "Contains modified Copernicus Sentinel data") | Yes, with attribution | **Strong** (page read) |
| Forest cover (alternatives) | Copernicus HRL Tree Cover Density; NIBIO SR16/AR5 (kilden.nibio.no) | **Not verified** (pages were JavaScript shells or truncated) | Unknown | Weak; do not ship derived data from these until checked |

Disagreement: NVE uses NLOD and Kartverket's DTM1 elevation is CC BY 4.0 in its catalogue record (NLOD 2.0 in its Atom feed), so a combined derived map needs both attributions: "Kilde: NVE" and "© Kartverket".
NLOD is described by its publisher as compatible with CC BY, which was not confirmed from the text.

## Open items for dependent work

1. Read the primary Rosell & Hovde (2001) / Haarberg & Rosell text for the Lunde figure and its method.
2. Confirm the 25 m/cell scale (nothing in the repo sets one) and re-cut the box if it changes.
3. Recompute UTM bounds and confirm the river runs through the box.
4. Read the NLOD text (river data). The elevation licence is settled: DTM1, CC BY 4.0 per Geonorge record fd96e6c3-eff6-437c-8fd6-e7c4718b1bf8; credit "© Kartverket". The 1.19 GB tile 33-117-110 is in EPSG:25833 (UTM 33), not UTM 32 like the bounds table, so reproject or clip with a projection tool.
