"""Unit tests for the Lunde map asset and tools/build_map_asset.py (no network, no big inputs).

  python -m unittest tools/test_map_asset.py
"""
import json, os, sys, unittest
import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import build_map_asset as bm  # noqa: E402

ASSET = os.path.join(os.path.dirname(HERE), "assets", "maps", "lunde", "map.json")


def load():
    with open(ASSET, encoding="utf-8") as f:
        return json.load(f)


def flow_edges(asset):
    """Directed cell edges of the river network stored in the asset (downstream = from a to b)."""
    rivers = {r["id"]: r["cells"] for r in asset["rivers"]}
    edges = []
    for cells in rivers.values():
        edges += [(tuple(a), tuple(b)) for a, b in zip(cells[:-1], cells[1:])]
    for up, down in asset["river_joins"]:
        edges.append((tuple(rivers[up][-1]), tuple(rivers[down][0])))
    return edges


class AssetTest(unittest.TestCase):
    def test_elevation_never_rises_downstream_along_every_river(self):
        a = load(); z = a["elevation_cm"]
        edges = flow_edges(a)
        self.assertGreater(len(edges), 500)
        bad = [(p, q, z[p[0]][p[1]], z[q[0]][q[1]]) for p, q in edges if z[q[0]][q[1]] > z[p[0]][p[1]]]
        self.assertEqual(bad, [])

    def test_every_river_has_a_downstream_drop_or_is_level(self):
        a = load(); z = a["elevation_cm"]
        for r in a["rivers"]:
            h = [z[c[0]][c[1]] for c in r["cells"]]
            self.assertTrue(all(y <= x for x, y in zip(h, h[1:])), r["id"])
            self.assertGreaterEqual(h[0], h[-1])

    def test_grid_layers_are_128_by_128_and_in_range(self):
        a = load(); n = 128
        self.assertEqual((a["size"], a["cell_m"]), (n, 25.0))
        for k in ("elevation_cm", "forest_pct", "river_mask"):
            self.assertEqual(len(a[k]), n); self.assertTrue(all(len(r) == n for r in a[k]), k)
        f = np.array(a["forest_pct"]); self.assertTrue(f.min() >= 0 and f.max() <= 100)
        z = np.array(a["elevation_cm"]) / 100.0; self.assertTrue(30 < z.min() and z.max() < 400)
        b = a["bounds_m"]; self.assertEqual((b["xmax"] - b["xmin"], b["ymax"] - b["ymin"]), (3200.0, 3200.0))

    def test_river_mask_is_union_of_reach_cells_and_has_forest_and_open_land(self):
        a = load()
        cells = {tuple(c) for r in a["rivers"] for c in r["cells"]}
        mask = {(i, j) for i, row in enumerate(a["river_mask"]) for j, ch in enumerate(row) if ch == "#"}
        self.assertEqual(cells, mask)
        f = np.array(a["forest_pct"]); self.assertTrue(0.2 < (f >= 50).mean() < 0.8)

    def test_bounds_match_the_tool(self):
        a = load(); b = a["bounds_m"]
        self.assertEqual((b["xmin"], b["ymin"], b["xmax"], b["ymax"]), bm.bounds())


class MonotoneTest(unittest.TestCase):
    def test_fixes_bump_and_pit_and_leaves_falling_terrain_alone(self):
        e = np.zeros((1, 6)); e[0] = [10, 9, 9.5, 8, 5, 4]       # embankment bump at index 2
        edges = [((0, i), (0, i + 1)) for i in range(5)]
        out, st = bm.make_monotone(e, edges)
        self.assertTrue(all(out[0, i + 1] <= out[0, i] for i in range(5)))
        self.assertEqual(out[0, 0], 10); self.assertEqual(out[0, 5], 4)
        e2 = np.array([[10.0, 8, 6, 4]]); out2, st2 = bm.make_monotone(e2, [((0, i), (0, i + 1)) for i in range(3)])
        self.assertTrue(np.array_equal(out2, e2)); self.assertEqual(st2["max_change_m"], 0)

    def test_junction_and_cycle_are_handled_and_other_cells_untouched(self):
        e = np.array([[9.0, 5, 7], [3, 8, 2]])
        edges = [((0, 0), (0, 1)), ((0, 2), (0, 1)), ((0, 1), (1, 1)), ((1, 1), (0, 1))]  # (0,1)<->(1,1) cycle
        out, st = bm.make_monotone(e, edges)
        for a, b in edges:
            self.assertLessEqual(out[b], out[a])
        self.assertEqual(out[0, 1], out[1, 1]); self.assertEqual(st["cells_in_cycles"], 2)
        self.assertEqual(out[1, 0], 3); self.assertEqual(out[1, 2], 2)

    def test_trace_gives_adjacent_ordered_cells_clipped_to_box(self):
        b = (0.0, 0.0, 3200.0, 3200.0)
        cells = bm.trace([(-100.0, 3190.0), (60.0, 3190.0)], b)   # enters from outside, along the north row
        self.assertEqual(cells, [(0, 0), (0, 1), (0, 2)])

    def test_cell_samples_cover_the_cell_centres_north_up(self):
        X, Y = bm.cell_samples((1000.0, 2000.0, 4200.0, 5200.0))
        self.assertEqual(X.shape, (128, 128, 5, 5))
        self.assertAlmostEqual(float(X[0, 0].mean()), 1012.5); self.assertAlmostEqual(float(Y[0, 0].mean()), 5187.5)
        self.assertAlmostEqual(float(Y[127, 0].mean()), 2012.5)


if __name__ == "__main__":
    unittest.main()
