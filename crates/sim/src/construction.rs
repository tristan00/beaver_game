//! Building placement, paid construction, repairs and cancellation.
use crate::{SimError, SimState, agents, buildings, catalogue, dam_preview, maintenance, water, work};

impl SimState {
    pub fn check_site(&self, kind: buildings::BuildingKind, pos: (usize, usize)) -> Result<(), SimError> {
        if !self.can_build(kind) {
            return Err(SimError::InvalidCommand(format!("requires finished buildings: {:?}", self.missing_prerequisites(kind))));
        }
        self.check_off_pile(kind, pos)?;
        self.check_explored(kind, pos)?;
        let terrain = buildings::Terrain { height: &self.height, depth: &self.water.depth };
        self.buildings.can_place_site(kind, pos, terrain).map_err(|e| SimError::InvalidCommand(e.to_string()))
    }

    fn check_explored(&self, kind: buildings::BuildingKind, pos: (usize, usize)) -> Result<(), SimError> {
        let (w, d) = kind.spec().footprint;
        for y in pos.1..pos.1 + d {
            for x in pos.0..pos.0 + w {
                if x < self.fog.size && y < self.fog.size && !self.fog.explored((x, y)) {
                    return Err(SimError::InvalidCommand("Unexplored: the colony has not seen that ground".into()));
                }
            }
        }
        Ok(())
    }

    fn check_off_pile(&self, kind: buildings::BuildingKind, pos: (usize, usize)) -> Result<(), SimError> {
        let (w, d) = kind.spec().footprint;
        if let Some(p) = self.work.supplies {
            if (pos.0..pos.0 + w).contains(&p.0) && (pos.1..pos.1 + d).contains(&p.1) {
                return Err(SimError::InvalidCommand("Blocked: the supply pile is there".into()));
            }
        }
        Ok(())
    }

    pub fn place_site(&mut self, kind: buildings::BuildingKind, pos: (usize, usize)) -> Result<buildings::BuildingId, SimError> {
        if let Some(need) = self.missing_prerequisites(kind).first().filter(|_| !self.sandbox) {
            return Err(SimError::InvalidCommand(format!("needs a finished {} first", need.spec().name.to_lowercase())));
        }
        if !self.can_build(kind) {
            return Err(SimError::InvalidCommand(format!("requires finished buildings: {:?}", self.missing_prerequisites(kind))));
        }
        self.check_off_pile(kind, pos)?;
        self.check_explored(kind, pos)?;
        if kind == buildings::BuildingKind::Lock && !buildings::Buildings::footprint(kind, pos).iter().any(|c| self.water.has_dam(c.0, c.1)) {
            return Err(SimError::InvalidCommand("a lock is built on a dam, to let barges through it".into()));
        }
        let terrain = buildings::Terrain { height: &self.height, depth: &self.water.depth };
        let id = self
            .buildings
            .place_site(kind, pos, terrain)
            .map_err(|e| SimError::InvalidCommand(e.to_string()))?;
        let spec = kind.spec();
        let dam_cells = (kind == buildings::BuildingKind::Dam).then(|| dam_preview::spot_cells(pos));
        let (cost, labour) = match &dam_cells {
            Some(c) => (dam_preview::dam_cost(c.len()), spec.work * dam_preview::length_factor(c.len())),
            None => (spec.cost.to_vec(), spec.work),
        };
        if let Err(e) = self.work.add_site(id, kind, &cost, labour) {
            self.buildings.buildings.retain(|b| b.id != id);
            return Err(SimError::InvalidCommand(format!("{e:?}")));
        }
        if let Some(cells) = dam_cells {
            let heights = vec![dam_preview::DEFAULT_HEIGHT; cells.len()];
            let from = cells.iter().map(|c| self.water.dam_height.get(c.0, c.1).copied().unwrap_or(0.0)).collect();
            self.work.dam_plans.push(work::DamPlan { building: id, repair: false, cells, height: dam_preview::DEFAULT_HEIGHT, heights, from });
        }
        Ok(id)
    }

    pub fn missing_prerequisites(&self, kind: buildings::BuildingKind) -> Vec<buildings::BuildingKind> {
        catalogue::prerequisites(kind)
            .iter()
            .copied()
            .filter(|&p| !self.buildings.buildings.iter().any(|b| b.kind == p && b.is_complete()))
            .collect()
    }

    pub fn has_building(&self, kind: buildings::BuildingKind) -> bool {
        self.buildings.buildings.iter().any(|b| b.kind == kind && b.is_complete())
    }

    pub fn can_build(&self, kind: buildings::BuildingKind) -> bool {
        self.sandbox || (self.features.allows_building(kind) && self.missing_prerequisites(kind).is_empty())
    }

    pub fn build_gaps(&self, kind: buildings::BuildingKind) -> Vec<buildings::BuildingKind> {
        self.missing_prerequisites(kind)
    }

    pub fn dam_site(&self, cells: &[water::Cell]) -> Result<water::Cell, buildings::BuildError> {
        let terrain = buildings::Terrain { height: &self.height, depth: &self.water.depth };
        let mid = cells.len() / 2;
        let mut order: Vec<usize> = (0..cells.len()).collect();
        order.sort_by_key(|&i| (i.abs_diff(mid), i));
        let mut first = None;
        for i in order {
            match self.buildings.can_place_site(buildings::BuildingKind::Dam, cells[i], terrain) {
                Ok(()) => return Ok(cells[i]),
                Err(e) => {
                    first.get_or_insert(e);
                }
            }
        }
        Err(first.unwrap_or(buildings::BuildError::OutOfBounds))
    }

    pub const MAX_CANAL_CELLS: usize = 150;
    pub const CANAL_DEPTH: f32 = 0.5;

    pub fn order_canal(&mut self, cells: &[water::Cell]) -> Result<buildings::BuildingId, SimError> {
        use buildings::BuildingKind::Canal;
        let invalid = |m: &str| SimError::InvalidCommand(m.to_string());
        if cells.is_empty() || cells.len() > Self::MAX_CANAL_CELLS {
            return Err(invalid("not a valid canal line (empty or too long)"));
        }
        if cells.iter().any(|&(x, y)| !self.water.depth.in_bounds(x, y)) {
            return Err(invalid("out of bounds"));
        }
        if cells.windows(2).any(|w| w[0].0.abs_diff(w[1].0).max(w[0].1.abs_diff(w[1].1)) != 1) {
            return Err(invalid("not a valid canal line (cells must touch)"));
        }
        if let Some(c) = cells.iter().find(|c| !self.fog.explored(**c)) {
            return Err(invalid(&format!("Unexplored at {c:?}")));
        }
        if !self.can_build(Canal) {
            return Err(invalid("Locked: Canal digging"));
        }
        let terrain = buildings::Terrain { height: &self.height, depth: &self.water.depth };
        let pos = cells
            .iter()
            .copied()
            .find(|&c| self.buildings.can_place_site(Canal, c, terrain).is_ok())
            .ok_or_else(|| invalid("needs water: a canal starts from water"))?;
        let id = self.buildings.place_site(Canal, pos, terrain).map_err(|e| SimError::InvalidCommand(e.to_string()))?;
        let n = cells.len() as f32;
        let cost: Vec<buildings::Amount> = Canal.spec().cost.iter().map(|&(g, a)| (g, a * n)).collect();
        let labour = Canal.spec().work * n;
        if let Err(e) = self.work.add_site(id, Canal, &cost, labour) {
            self.buildings.buildings.retain(|b| b.id != id);
            return Err(SimError::InvalidCommand(format!("{e:?}")));
        }
        self.work.canal_plans.push(work::CanalPlan { building: id, cells: cells.to_vec(), depth: Self::CANAL_DEPTH });
        Ok(id)
    }

    pub fn order_dam(&mut self, cells: &[water::Cell], height: f32) -> Result<buildings::BuildingId, SimError> {
        self.order_dam_heights(cells, &vec![height; cells.len()])
    }

    pub fn order_dam_to(&mut self, cells: &[water::Cell], crest: f32) -> Result<buildings::BuildingId, SimError> {
        let (cells, heights) = dam_preview::heights_to(self, cells, crest);
        if cells.is_empty() {
            return Err(SimError::InvalidCommand("the ground is already that high".into()));
        }
        self.order_dam_heights(&cells, &heights)
    }

    pub fn repair_dam(&mut self, cell: water::Cell) -> Result<buildings::BuildingId, SimError> {
        let recorded=self.buildings.buildings.iter().find(|b| b.kind==buildings::BuildingKind::Dam && b.dam_profile.iter().any(|v|v.0==cell)).cloned();
        let mut cells = recorded.as_ref().map(|b|b.dam_profile.iter().map(|v|v.0).collect::<Vec<_>>()).unwrap_or_else(||self.water.dam_line_at(cell));
        cells.sort();
        if cells.is_empty() {
            return Err(SimError::InvalidCommand("there is no dam here to repair".into()));
        }
        // Repeated clicks must not create competing work orders for the same dam.
        if let Some(plan) = self.work.dam_plans.iter().find(|p| p.cells.iter().any(|c| cells.binary_search(c).is_ok())) {
            return Ok(plan.building);
        }
        let heights: Vec<f32> = cells.iter().map(|&(x,y)| recorded.as_ref().and_then(|b|b.dam_profile.iter().find(|v|v.0==(x,y)).map(|v|v.1)).unwrap_or_else(||*self.water.dam_height.get(x,y).expect("dam cell"))).collect();
        let (mut cost, labour) = dam_preview::cost_for(self, &cells, &heights);
        if labour <= 1e-3 {
            return Err(SimError::InvalidCommand("the dam does not need repairs".into()));
        }
        let existing = recorded.as_ref().map(|b|b.id).or_else(||self.buildings.buildings.iter().find(|b| b.kind == buildings::BuildingKind::Dam
            && b.is_complete() && b.cells().any(|c| cells.binary_search(&c).is_ok())).map(|b| b.id));
        let Some(id) = existing else { return self.order_dam_heights(&cells, &heights) };
        if self.maintenance.jobs.contains_key(&id) {return Err(SimError::InvalidCommand("this dam already has maintenance queued".into()));}
        if self.buildings.building(id).is_some_and(|b|b.reinforced) {
            let damage=cells.iter().map(|&(x,y)|1.0-self.water.dam_integrity.get(x,y).copied().unwrap_or(0.0)).sum::<f32>();
            cost.push((buildings::Good::Clay,(damage*1.2).ceil().max(1.0)));
        }
        // A dam's crown can be dry. Reuse its original work site instead of
        // requiring the water/placement checks for a brand-new dam.
        let old = self.work.places.iter().position(|p| p.building == id).map(|i| self.work.places.remove(i));
        if let Err(e) = self.work.add_site(id, buildings::BuildingKind::Dam, &cost, labour) {
            if let Some(p) = old { self.work.places.push(p); }
            return Err(SimError::InvalidCommand(format!("{e:?}")));
        }
        self.buildings.buildings.iter_mut().find(|b| b.id == id).expect("existing dam").status = buildings::Status::Site;
        let height = heights.iter().copied().fold(0.0, f32::max);
        self.work.dam_plans.push(work::DamPlan { building: id, repair: true, cells, height, from: heights.clone(), heights });
        Ok(id)
    }

    pub fn order_dam_heights(&mut self, cells: &[water::Cell], heights: &[f32]) -> Result<buildings::BuildingId, SimError> {
        if dam_preview::preview_heights(self, cells, heights).is_none() {
            return Err(SimError::InvalidCommand("not a valid dam line (cells empty, repeated or off the map, or height not positive)".into()));
        }
        if let Some(h) = heights.iter().copied().find(|&h| h > dam_preview::MAX_DAM_HEIGHT + 1e-3) {
            return Err(SimError::InvalidCommand(format!("{h:.1} is too high: a dam stands at most {} above the ground", dam_preview::MAX_DAM_HEIGHT)));
        }
        let (cost, labour) = dam_preview::cost_for(self, cells, heights);
        if labour <= 1e-3 {
            return Err(SimError::InvalidCommand("the dam is already that high".into()));
        }
        let pos = self.dam_site(cells).map_err(|e| SimError::InvalidCommand(e.to_string()))?;
        let terrain = buildings::Terrain { height: &self.height, depth: &self.water.depth };
        let id = self
            .buildings
            .place_site(buildings::BuildingKind::Dam, pos, terrain)
            .map_err(|e| SimError::InvalidCommand(e.to_string()))?;
        let labour = labour;
        if let Err(e) = self.work.add_site(id, buildings::BuildingKind::Dam, &cost, labour) {
            self.buildings.buildings.retain(|b| b.id != id);
            return Err(SimError::InvalidCommand(format!("{e:?}")));
        }
        let height = heights.iter().copied().fold(0.0, f32::max);
        let from = cells.iter().map(|c| self.water.dam_height.get(c.0, c.1).copied().unwrap_or(0.0)).collect();
        self.work.dam_plans.push(work::DamPlan { building: id, repair: false, cells: cells.to_vec(), height, heights: heights.to_vec(), from });
        Ok(id)
    }

    pub fn cancel_site(&mut self, id: buildings::BuildingId) -> Result<Vec<buildings::Amount>, SimError> {
        if self.maintenance.jobs.contains_key(&id) { return maintenance::cancel(self, id); }
        let b = self.buildings.building(id).ok_or_else(|| SimError::InvalidCommand(format!("no building {}", id.0)))?;
        if b.is_complete() {
            return Err(SimError::InvalidCommand(format!("{} is finished; only unfinished sites can be cancelled", b.kind.spec().name)));
        }
        let mut refunded = Vec::new();
        let is_dam = self.work.dam_plans.iter().any(|d| d.building == id);
        let is_repair = self.work.dam_plans.iter().any(|d| d.building == id && d.repair);
        if let Some(i) = self.work.places.iter().position(|p| p.building == id) {
            let place = self.work.places.remove(i);
            let site = place.site.as_ref();
            let built = if is_dam && !is_repair { site.map_or(0.0, |s| s.labour_fraction()) } else { 0.0 };
            for &(g, a) in site.map(|s| s.delivered.as_slice()).unwrap_or(&[]) {
                let used: f32 = site.map_or(0.0, |s| s.needed.iter().filter(|n| n.0 == g).map(|n| n.1).sum::<f32>()) * built;
                let a = (a - used).max(0.0);
                if a > 0.0 {
                    let put = self.buildings.stores.refund(g, a).map_err(|e| SimError::InvalidCommand(e.to_string()))?;
                    refunded.push((g, put));
                }
            }
        }
        self.work.dam_plans.retain(|d| d.building != id);
        self.work.canal_plans.retain(|d| d.building != id);
        self.work.managed.retain(|_, duty| !matches!(duty, work::ManagedDuty::Slot(b, _) if *b == id));
        for ag in self.agents.by_id.values_mut() {
            if matches!(ag.assignment, Some(agents::Assignment::Slot(b, _)) if b == id) {
                ag.assignment = None;
            }
        }
        if is_repair {
            let b = self.buildings.buildings.iter_mut().find(|b| b.id == id).expect("repair site");
            b.status = buildings::Status::Complete;
            self.work.add_complete(b).map_err(|e| SimError::InvalidCommand(format!("{e:?}")))?;
        } else {
            self.buildings.buildings.retain(|b| b.id != id);
        }
        self.output.forget(id);
        Ok(refunded)
    }
}
