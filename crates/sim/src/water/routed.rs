use super::{Water, edge_bed, neighbours4};
use std::{cmp::Reverse, collections::BinaryHeap};

const NONE: usize = usize::MAX;
const SINGLE_PATH_REACHES: bool = true;

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct Flow {
    version: Option<((u64, u64), (u64, u64), (u64, u64), (u64, u64), bool)>,
    nodes: Vec<Node>,
    owner: Vec<usize>,
    wet: Vec<usize>,
    published: Vec<usize>,
    source_key: Option<u64>,
    source_nodes: Vec<usize>,
    queued: Vec<u64>,
    stamp: u64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct Node {
    cells: Vec<usize>,
    beds: Vec<f64>,
    prefix: Vec<f64>,
    links: Vec<Link>,
    rank: usize,
    volume: f64,
    level: f64,
    source: f64,
    fixed: Option<f64>,
    reservoir: bool,
    span: (usize, usize),
    flow: (f64, f64),
    incoming: (f64, f64),
    cached_input: (f64, f64, f64),
    rates: Vec<f64>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct Link {
    to: usize,
    crest: f64,
    slope: f64,
    direction: (f64, f64),
    weir: bool,
}

impl Node {
    fn level_for(&self, volume: f64) -> f64 {
        let (mut low, mut high) = (1, self.beds.len());
        while low < high {
            let k = (low + high) / 2;
            if (volume + self.prefix[k]) / k as f64 > self.beds[k] {
                low = k + 1;
            } else {
                high = k;
            }
        }
        let k = low;
        (volume + self.prefix[k]) / k as f64
    }
    fn storage(&self, level: f64) -> (f64, f64) {
        let k = self.beds.partition_point(|&z| z < level).max(1);
        ((level * k as f64 - self.prefix[k]).max(0.0), k as f64)
    }
}

impl Flow {
    pub(super) fn total_volume(&self) -> Option<f64> {
        self.version
            .map(|_| self.wet.iter().map(|&i| self.nodes[i].volume).sum())
    }

    pub(super) fn prepare(&mut self, w: &mut Water, mut sync: bool) {
        let (terrain, dam) = w.bed_version.unwrap();
        let version = (
            terrain,
            dam,
            w.closed_edge.version(),
            w.lake_level.version(),
            w.config.open_edges,
        );
        let rebuilt = self.version != Some(version);
        let correction = if rebuilt {
            self.total_volume()
                .map(|volume| volume - w.h.iter().map(|&h| h as f64).sum::<f64>())
                .unwrap_or(0.0)
        } else {
            0.0
        };
        if rebuilt {
            self.rebuild(w);
            self.version = Some(version);
            sync = true;
        }
        if sync {
            if rebuilt {
                self.wet = (0..self.nodes.len()).collect();
            }
            let receiver = if correction != 0.0 {
                w.h.iter()
                    .enumerate()
                    .max_by(|a, b| a.1.total_cmp(b.1))
                    .map(|(i, _)| self.owner[i])
            } else {
                None
            };
            for &i in &self.wet {
                let node = &mut self.nodes[i];
                node.volume = node.cells.iter().map(|&i| w.h[i] as f64).sum();
                if receiver == Some(i) {
                    node.volume = (node.volume + correction).max(0.0);
                }
                if let Some(level) = node.fixed {
                    let target = node.storage(level).0;
                    let change = target - node.volume;
                    if change >= 0.0 {
                        w.books.sources += change;
                        w.added_volume += change;
                    } else {
                        w.books.edges -= change;
                        w.lost_volume -= change;
                    }
                    node.volume = target;
                }
                node.level = node.fixed.unwrap_or_else(|| node.level_for(node.volume));
            }
            self.wet
                .retain(|&i| self.nodes[i].volume > 0.0 || self.nodes[i].fixed.is_some());
            if rebuilt {
                self.published.clone_from(&self.wet);
            }
        }
        if self.source_key != w.source_key {
            for &id in &self.source_nodes {
                self.nodes[id].source = 0.0;
            }
            self.source_nodes.clear();
            for &i in &w.source_cells {
                let owner = self.owner[i];
                self.nodes[owner].source += w.src[i] as f64;
                self.source_nodes.push(owner);
            }
            self.source_nodes.sort_unstable();
            self.source_nodes.dedup();
            self.source_key = w.source_key;
        }
    }

    fn rebuild(&mut self, w: &Water) {
        let n = w.n;
        let count = n * n;
        let key = crate::water::ordered_f32;
        let mut queue = BinaryHeap::new();
        let mut spill = vec![f32::INFINITY; count];
        let mut rank = vec![NONE; count];
        let mut seen = vec![false; count];
        for i in 0..count {
            let boundary = i % n == 0 || i % n + 1 == n || i / n == 0 || i / n + 1 == n;
            let lake = w.lake_level.as_slice()[i];
            if lake.is_finite() || boundary && w.config.open_edges && !w.closed_edge.as_slice()[i] {
                let level = if lake.is_finite() {
                    lake.max(w.z[i])
                } else {
                    w.z[i]
                };
                seen[i] = true;
                spill[i] = level;
                queue.push(Reverse((key(level), i)));
            }
        }
        let mut visited = 0;
        while let Some(Reverse((_, i))) = queue.pop() {
            rank[i] = visited;
            visited += 1;
            for j in neighbours4(n, i) {
                if seen[j] {
                    continue;
                }
                seen[j] = true;
                spill[j] = spill[i].max(w.z[j]);
                queue.push(Reverse((key(spill[j]), j)));
            }
        }
        self.nodes.clear();
        self.owner = vec![NONE; count];
        self.published.clear();
        for seed in 0..count {
            if self.owner[seed] != NONE {
                continue;
            }
            let id = self.nodes.len();
            let mut cells = vec![seed];
            self.owner[seed] = id;
            let reservoir = spill[seed] > w.z[seed] + 1.0e-5;
            {
                let mut k = 0;
                while k < cells.len() {
                    let i = cells[k];
                    k += 1;
                    for j in neighbours4(n, i) {
                        let connected = if reservoir {
                            spill[j] == spill[seed] && spill[j] > w.z[j] + 1.0e-5
                        } else {
                            w.z[j] == w.z[seed] && spill[j] <= w.z[j] + 1.0e-5
                        };
                        if self.owner[j] == NONE && connected {
                            self.owner[j] = id;
                            cells.push(j);
                        }
                    }
                }
            }
            let mut beds: Vec<f64> = cells.iter().map(|&i| w.z[i] as f64).collect();
            beds.sort_unstable_by(f64::total_cmp);
            let mut prefix = vec![0.0];
            for &bed in &beds {
                prefix.push(prefix.last().unwrap() + bed);
            }
            let fixed = cells
                .iter()
                .map(|&i| w.lake_level.as_slice()[i])
                .find(|v| v.is_finite())
                .map(|v| v as f64);
            let rank = cells.iter().map(|&i| rank[i]).min().unwrap();
            let span = (
                cells.iter().map(|i| i % n).max().unwrap()
                    - cells.iter().map(|i| i % n).min().unwrap()
                    + 1,
                cells.iter().map(|i| i / n).max().unwrap()
                    - cells.iter().map(|i| i / n).min().unwrap()
                    + 1,
            );
            self.nodes.push(Node {
                cells,
                beds,
                prefix,
                links: vec![],
                rank,
                volume: 0.0,
                level: 0.0,
                source: 0.0,
                fixed,
                reservoir,
                span,
                flow: (0.0, 0.0),
                incoming: (0.0, 0.0),
                cached_input: (f64::NAN, 0.0, 0.0),
                rates: vec![],
            });
        }
        for i in 0..count {
            let a = self.owner[i];
            for j in neighbours4(n, i) {
                let b = self.owner[j];
                if a == b || self.nodes[b].rank >= self.nodes[a].rank {
                    continue;
                }
                let dx = (j % n) as f64 - (i % n) as f64;
                let dy = (j / n) as f64 - (i / n) as f64;
                let crest = w.z[i].max(w.z[j]) as f64;
                let weir = self.nodes[a].reservoir
                    || w.dam_height.as_slice()[i] > 0.0
                    || w.dam_height.as_slice()[j] > 0.0;
                self.nodes[a].links.push(Link {
                    to: b,
                    crest,
                    slope: ((w.z[i] - w.z[j]) as f64 / w.cell_m as f64).max(0.001),
                    direction: (dx, dy),
                    weir,
                });
            }
            let (x, y) = (i % n, i / n);
            if w.config.open_edges && !w.closed_edge.as_slice()[i] {
                for (yes, inner, direction) in [
                    (x == 0, i + 1, (-1.0, 0.0)),
                    (x + 1 == n, i.wrapping_sub(1), (1.0, 0.0)),
                    (y == 0, i + n, (0.0, -1.0)),
                    (y + 1 == n, i.wrapping_sub(n), (0.0, 1.0)),
                ] {
                    if !yes {
                        continue;
                    }
                    self.nodes[a].links.push(Link {
                        to: NONE,
                        crest: w.z[i] as f64,
                        slope: ((w.z[i] - edge_bed(&w.z, i, inner, w.cell_m)) / w.cell_m) as f64,
                        direction,
                        weir: false,
                    });
                }
            }
        }
        if SINGLE_PATH_REACHES {
            for node in self.nodes.iter_mut().filter(|n| !n.reservoir && n.fixed.is_none()) {
                let inner: Vec<usize> = (0..node.links.len()).filter(|&k| node.links[k].to != NONE).collect();
                if inner.len() > 1 {
                    let best = *inner.iter().max_by(|&&a, &&b| node.links[a].slope.total_cmp(&node.links[b].slope).then(b.cmp(&a))).unwrap();
                    let mut k = 0;
                    node.links.retain(|l| {
                        let keep = l.to == NONE || k == best;
                        k += 1;
                        keep
                    });
                }
            }
        }
        self.source_key = None;
        self.source_nodes.clear();
        self.queued = vec![0; self.nodes.len()];
        for node in &mut self.nodes {
            node.rates = vec![0.0; node.links.len()];
        }
    }

    pub(super) fn advance(&mut self, w: &mut Water, seconds: f64) {
        let mut left = seconds;
        while left > 1.0e-6 {
            let dt = if left <= 900.001 { left } else { 900.0 };
            self.stamp += 1;
            let stamp = self.stamp;
            let mut queue = BinaryHeap::with_capacity(self.wet.len() + self.source_nodes.len());
            for &id in self.wet.iter().chain(&self.source_nodes) {
                if self.queued[id] == stamp {
                    continue;
                }
                self.queued[id] = stamp;
                self.nodes[id].incoming = (0.0, 0.0);
                queue.push((self.nodes[id].rank, id));
            }
            self.wet.clear();
            while let Some((_, id)) = queue.pop() {
                let node = &mut self.nodes[id];
                let input = node.source * dt;
                node.volume += input;
                w.books.sources += input;
                w.added_volume += input;
                if node.fixed.is_some() {
                    w.books.edges += input;
                    w.lost_volume += input;
                    node.volume -= input;
                }
                if node.volume <= 1.0e-10 && node.fixed.is_none() {
                    w.books.seepage += node.volume;
                    w.lost_volume += node.volume;
                    node.volume = 0.0;
                    continue;
                }
                let available = node.volume;
                let manning = w.config.manning as f64;
                let cached = (available - node.cached_input.0).abs() < 1.0e-6
                    && (dt - node.cached_input.1).abs() < 1.0e-3
                    && manning == node.cached_input.2;
                let mut level = node.fixed.unwrap_or_else(|| node.level_for(available));
                let (mut low, mut high) = (node.beds[0], level);
                if !cached && node.fixed.is_none() && !node.links.is_empty() {
                    let crest = node
                        .links
                        .iter()
                        .map(|l| l.crest)
                        .fold(f64::INFINITY, f64::min);
                    if high > crest {
                        let capacity = node.storage(crest).0;
                        let rate: f64 = node
                            .links
                            .iter()
                            .map(|l| discharge(l, crest + 1.0, manning).0)
                            .sum();
                        level = (crest
                            + ((available - capacity) / (dt * rate).max(1.0e-30))
                                .max(0.0)
                                .powf(0.6))
                        .min(high);
                    }
                    for _ in 0..12 {
                        let (mut q, mut derivative) = (0.0, 0.0);
                        for link in &node.links {
                            let f = discharge(link, level, w.config.manning as f64);
                            q += f.0;
                            derivative += f.1;
                        }
                        let (volume, area) = node.storage(level);
                        let residual = volume + q * dt - available;
                        if residual.abs() < 1.0e-8 {
                            break;
                        }
                        if residual > 0.0 {
                            high = level;
                        } else {
                            low = level;
                        }
                        let next = level - residual / (area + dt * derivative);
                        level = if next > low && next < high {
                            next
                        } else {
                            0.5 * (low + high)
                        };
                    }
                }
                if !cached {
                    for (rate, link) in node.rates.iter_mut().zip(&node.links) {
                        *rate = discharge(link, level, manning).0;
                    }
                    node.cached_input = (available, dt, manning);
                }
                let total: f64 = node.rates.iter().sum();
                let scale = if node.fixed.is_some() {
                    1.0
                } else {
                    (available / (total * dt).max(1.0e-30)).min(1.0)
                };
                let fixed = node.fixed.is_some();
                let mut moved = 0.0;
                let mut vector = (0.0, 0.0);
                for li in 0..self.nodes[id].links.len() {
                    let link = &self.nodes[id].links[li];
                    if fixed && (link.to == NONE || self.nodes[link.to].fixed.is_some()) {
                        continue;
                    }
                    let rate = self.nodes[id].rates[li] * scale;
                    let amount = rate * dt;
                    let (to, direction) = (link.to, link.direction);
                    moved += amount;
                    vector.0 += rate * direction.0;
                    vector.1 += rate * direction.1;
                    if fixed {
                        w.books.sources += amount;
                        w.added_volume += amount;
                    }
                    if to == NONE || self.nodes[to].fixed.is_some() {
                        w.books.edges += amount;
                        w.lost_volume += amount;
                    } else if amount > 0.0 {
                        let next = &mut self.nodes[to];
                        if self.queued[to] != stamp {
                            self.queued[to] = stamp;
                            next.incoming = (0.0, 0.0);
                            queue.push((next.rank, to));
                        }
                        next.volume += amount;
                        next.incoming.0 += rate * direction.0;
                        next.incoming.1 += rate * direction.1;
                    }
                }
                let node = &mut self.nodes[id];
                if !fixed {
                    node.volume = (available - moved).max(0.0);
                }
                node.level = node.fixed.unwrap_or_else(|| node.level_for(node.volume));
                node.flow = (
                    (vector.0 + node.incoming.0) * 0.5,
                    (vector.1 + node.incoming.1) * 0.5,
                );
                if node.volume > 0.0 || node.fixed.is_some() {
                    self.wet.push(id);
                }
                w.timings.cell_updates += 1;
            }
            left -= dt;
            w.physics_seconds += dt;
            w.steps_taken += 1;
            w.timings.physics_s += dt;
            w.timings.substeps += 1;
        }
    }

    pub(super) fn environment(&mut self, w: &mut Water, days: f64) {
        w.update_riverbed(days as f32);
        let forcing = w.forcing();
        let r = forcing.rain as f64 * days;
        let e = w.config.evaporation as f64 * forcing.evaporation as f64 * days;
        let s = w.config.seepage as f64 * days;
        let (mut rain, mut evaporation, mut seepage) = (0.0, 0.0, 0.0);
        for &id in &self.wet {
            let node = &mut self.nodes[id];
            if node.fixed.is_some() {
                continue;
            }
            let mut change = 0.0;
            for &i in &node.cells {
                let depth = (node.level - w.z[i] as f64).max(0.0);
                if depth == 0.0 {
                    continue;
                }
                let ev = e.min(depth + r);
                let infiltration = if w.riverbed.as_slice()[i] < 0.5 && !w.canal.as_slice()[i] {
                    4.0 * s
                } else {
                    s
                };
                let sp = infiltration.min((depth + r - ev).max(0.0));
                change += r - ev - sp;
                rain += r;
                evaporation += ev;
                seepage += sp;
            }
            node.volume = (node.volume + change).max(0.0);
            node.level = node.level_for(node.volume);
        }
        w.books.rain += rain;
        w.books.evaporation += evaporation;
        w.books.seepage += seepage;
        w.added_volume += rain;
        w.lost_volume += evaporation + seepage;
    }

    pub(super) fn publish(&mut self, w: &mut Water) {
        for &id in &self.published {
            for &i in &self.nodes[id].cells {
                w.h[i] = 0.0;
            }
        }
        for row in &mut w.rows {
            row.clear();
        }
        w.qx.fill(0.0);
        w.qy.fill(0.0);
        let n = w.n;
        for &id in &self.wet {
            let node = &self.nodes[id];
            let (dx, dy) = node.flow;
            for &i in &node.cells {
                let (x, y) = (i % n, i / n);
                w.h[i] = (node.level - w.z[i] as f64).max(0.0) as f32;
                w.rows[y].push((x as u32, x as u32 + 1));
                let qx = (dx / node.span.1 as f64) as f32;
                let qy = (dy / node.span.0 as f64) as f32;
                w.qx[y * (n + 1) + x] += 0.5 * qx;
                w.qx[y * (n + 1) + x + 1] += 0.5 * qx;
                w.qy[i] += 0.5 * qy;
                w.qy[i + n] += 0.5 * qy;
            }
        }
        for row in &mut w.rows {
            row.sort_unstable();
            merge_runs_in_place(row, 0);
        }
        self.published.clone_from(&self.wet);
    }
}

fn discharge(link: &Link, level: f64, manning: f64) -> (f64, f64) {
    let h = (level - link.crest).max(0.0);
    if h <= 0.0 {
        return (0.0, 0.0);
    }
    let weir = 1.7 * h * h.sqrt();
    if link.weir {
        return (weir, 1.5 * weir / h);
    }
    let normal = h.powf(5.0 / 3.0) * link.slope.sqrt() / manning;
    if weir < normal {
        (weir, 1.5 * weir / h)
    } else {
        (normal, (5.0 / 3.0) * normal / h)
    }
}

fn merge_runs_in_place(runs:&mut Vec<(u32,u32)>,gap:u32) {
    let mut count=0;
    for read in 0..runs.len() {
        let (a,b)=runs[read];
        if count>0 && a<=runs[count-1].1+gap {runs[count-1].1=runs[count-1].1.max(b);}
        else {runs[count]=(a,b);count+=1;}
    }
    runs.truncate(count);
}

impl super::WaterEngine for Flow {
    fn prepare(&mut self, w: &mut Water, sync: bool) { Flow::prepare(self, w, sync) }
    fn advance(&mut self, w: &mut Water, seconds: f64) { Flow::advance(self, w, seconds) }
    fn environment(&mut self, w: &mut Water, days: f64) { Flow::environment(self, w, days) }
    fn publish(&mut self, w: &mut Water) { Flow::publish(self, w) }
    fn total_volume(&self) -> Option<f64> { Flow::total_volume(self) }
}
