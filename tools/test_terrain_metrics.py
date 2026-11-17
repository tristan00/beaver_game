"""Unit tests for tools/terrain_metrics.py and the projection in tools/real_landscape.py.

  python -m unittest tools/test_terrain_metrics.py
"""
import os, sys, unittest
import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import terrain_metrics as tm  # noqa: E402


def grids(n, river_cells, flow):
    mr = np.zeros((n, n), np.uint8); ft = -np.ones(n * n, int)
    for x, y in river_cells: mr[y, x] = 1
    for a, b in flow.items(): ft[a[1] * n + a[0]] = b[1] * n + b[0]
    return {"main_river": mr, "flow_to": ft.reshape(n, n)}


class MainPath(unittest.TestCase):
    def test_longest_chain_wins_over_first_headless_cell(self):
        n = 8
        # a 1-cell side arm at (0,0) flowing out of the mask, listed first; the real river runs (1,3)->(6,3)
        river = [(0, 0)] + [(x, 3) for x in range(1, 7)]
        flow = {(x, 3): (x + 1, 3) for x in range(1, 6)}
        flow[(0, 0)] = (0, 1)  # leaves the mask
        path = tm.main_path(grids(n, river, flow), n)
        self.assertEqual(path, [(x, 3) for x in range(1, 7)])

    def test_cycle_terminates(self):
        n = 4
        river = [(0, 0), (1, 0), (2, 0)]
        flow = {(0, 0): (1, 0), (1, 0): (2, 0), (2, 0): (1, 0)}
        self.assertEqual(tm.main_path(grids(n, river, flow), n), [(0, 0), (1, 0), (2, 0)])


class Sinuosity(unittest.TestCase):
    def test_straight_line_is_one(self):
        self.assertTrue(all(abs(v - 1) < 1e-9 for v in tm.sinuosity([(i, 0) for i in range(100)])))


class Clearings(unittest.TestCase):
    def test_patches_below_threshold_ignored(self):
        n = 20
        g = {k: np.zeros((n, n), np.uint8) for k in ("river", "lake", "main_river")}
        g["forest"] = np.ones((n, n), np.uint8)
        g["forest"][2:7, 2:7] = 0  # 25 cells = 0.25 ha at 10 m
        g["forest"][15, 15] = 0    # 1 cell = 0.01 ha, below CLEARING_MIN_HA
        g["height"] = np.zeros((n, n)); g["slope"] = np.zeros((n, n)); g["flow_to"] = -np.ones((n, n), int)
        m = tm.measure(g, n, 10.0, 1.0)
        self.assertAlmostEqual(m["clearing_median_area"], 0.25)
        self.assertEqual(m["_counts"]["clearings"], 1)


class Projection(unittest.TestCase):
    def test_lcc_3979_origin_and_axis(self):
        import real_landscape as rl
        x, y = rl.lcc_3979(np.array(49.0), np.array(-95.0))
        self.assertAlmostEqual(float(x), 0.0, places=3); self.assertAlmostEqual(float(y), 0.0, places=3)
        x, _ = rl.lcc_3979(np.array(49.0), np.array(-94.0))
        self.assertTrue(72000 < float(x) < 74000)  # one degree of longitude at 49 N is about 73 km


if __name__ == "__main__":
    unittest.main()
