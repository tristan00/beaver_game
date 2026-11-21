use super::panels::{status_line, stage_icon, Ctx, RIGHT_W};
use super::ui::*;
use super::words::*;
use super::HudAction;
use crate::flow::Selection;
use crate::icons::Icon;
use bevy::prelude::*;
use sim::buildings::{BuildingId, BuildingKind, Good};
use sim::colony::{BeaverId, Job};

fn shell(f: impl FnOnce(&mut Ui)) -> VNode {
    let mut ui = Ui::default();
    ui.node(Node { width: Val::Px(RIGHT_W), flex_direction: FlexDirection::Column, row_gap: Val::Px(6.0), ..default() }, f);
    let mut v = ui.nodes.pop().unwrap();
    panel(&mut v);
    v
}

fn header(c: &Ctx, sel: Selection, u: &mut Ui, icon: Icon, name: String, tip: Option<Tip>, extra: impl FnOnce(&mut Ui)) {
    let pinned = c.hud.pinned.contains(&sel);
    u.row(6.0, |r| {
        let t = r.row(6.0, |x| {
            x.icon(icon, 20.0, INK);
            x.bold(name, 14.0, INK);
        });
        if let Some(tip) = tip {
            t.tip(tip);
        }
        r.spacer();
        extra(r);
        r.icon_button(Icon::Pin, 11.0, HudAction::Pin(sel), FAINT, pinned).tip(Tip::name(if pinned { "Unpin" } else { "Pin" }));
        if pinned {
            r.icon_button(Icon::Cross, 11.0, HudAction::Pin(sel), FAINT, false).tip(Tip::name("Close"));
        } else {
            r.icon_button(Icon::Cross, 11.0, HudAction::CloseInspector, FAINT, false).tip(Tip::name("Close").key("Esc"));
        }
    });
}

pub fn inspector(c: &Ctx, sel: Selection) -> Option<VNode> {
    match sel {
        Selection::Site(id) => Some(match c.s.buildings.building(id).and_then(|b| b.dam.as_deref()) {
            Some(d) => super::dam_panel::dam_panel(c, id, d),
            None => building(c, id),
        }),
        Selection::Beaver(id) => Some(beaver(c, id)),
        Selection::Tree(rid) => Some(resource(c, rid)),
        Selection::Cell(cell) => Some(cell_panel(c, cell)),
        Selection::Lodge(id) => Some(lodge(c, id)),
        Selection::None => None,
    }
}

fn bar_row(u: &mut Ui, icon: Icon, col: Color, frac: f32, value: String, tip: Tip) {
    u.row(6.0, |r| {
        r.icon(icon, 15.0, col);
        r.bar(frac, 150.0, 6.0, col);
        r.spacer();
        r.bold(value, 12.0, INK);
    })
    .tip(tip);
}

fn building(c: &Ctx, id: BuildingId) -> VNode {
    let s = c.s;
    let Some(b) = s.buildings.building(id) else {
        return shell(|u| {
            u.text("-", 12.0, MUTED);
        });
    };
    let place = s.work.place(id);
    shell(|u| {
        header(c, Selection::Site(id), u, building_icon(b.kind), b.kind.spec().name.to_string(), None, |r| {
            if let Some(pl) = place.filter(|pl| !pl.slots.is_empty()) {
                use sim::work::Priority;
                let (icon, next, name) = match pl.priority {
                    Priority::Low => (Icon::PriorityDown, Priority::Normal, "Low priority"),
                    Priority::Normal => (Icon::Dot, Priority::High, "Normal priority"),
                    Priority::High => (Icon::PriorityUp, Priority::Low, "High priority"),
                };
                r.icon_button(icon, 11.0, HudAction::Priority(id, next), FAINT, pl.priority != Priority::Normal).tip(Tip::name(name).line("Click", "change"));
                r.icon_button(Icon::Pause, 11.0, HudAction::Paused(id, !pl.paused), FAINT, pl.paused).tip(Tip::name(if pl.paused { "Paused" } else { "Pause" }));
            }
            r.icon_button(Icon::Focus, 11.0, HudAction::Focus(b.pos), FAINT, false).tip(Tip::name("Show"));
        });
        if let Some(home) = s.work.homes.get(&id).and_then(|h| s.colony.lodges.iter().find(|l| l.id == *h)) {
            let residents: Vec<_> = s.colony.beavers.iter().filter(|b| b.lodge == Some(home.id)).collect();
            let name = s.colony.family(home.family).map_or("Empty family home", |f| f.name.as_str());
            u.row(6.0, |r| { r.icon(Icon::Home, 14.0, INK); r.text(name, 12.0, INK); r.spacer(); r.bold(format!("{}", residents.len()), 12.0, INK); });
            for resident in residents {
                u.row(6.0, |r| { r.icon(stage_icon(resident.stage), 12.0, MUTED); r.text(resident.name.clone(), 11.0, INK).action(HudAction::SelectBeaver(resident.id)); });
            }
        }
        if let Some((icon, words, goods, col)) = status_line(s, id) {
            let line = u.row(5.0, |r| {
                r.icon(if col == GOOD { Icon::Dot } else { Icon::Warn }, 9.0, col);
                if let Some(i) = icon {
                    r.icon(i, 15.0, col);
                }
                r.bold(words, 12.5, INK);
                for g in goods {
                    r.icon(good_icon(g), 15.0, good_color(g));
                }
            });
            line.node.padding = UiRect::axes(Val::Px(6.0), Val::Px(4.0));
            line.node.border_radius = BorderRadius::all(Val::Px(5.0));
            line.bg = col.with_alpha(0.12);
        }
        if b.is_complete() {
            u.rule();
            bar_row(u,if b.reinforced {Icon::Mud} else {Icon::Hammer},if b.condition<0.55{WARN}else{GOOD},b.condition,format!("{:.0}%",b.condition*100.0),Tip::name("Condition").line(if b.reinforced {"Clay reinforced"} else {"Timber"},""));
            if !s.maintenance.jobs.contains_key(&id) {
                u.row(6.0,|r| {
                    if b.condition<0.98 {
                        let mut tip=Tip::name("Repair");
                        for (g,a) in sim::maintenance::quoted_cost(s,b,sim::maintenance::Refit::Repair).0 {tip.push(TipLine::new(good_name(g),num(a)).icon(good_icon(g),good_color(g)));}
                        r.icon_button(Icon::Hammer,14.0,HudAction::RepairBuilding(id),ACCENT,false).tip(tip);
                    }
                    if !b.reinforced && sim::maintenance::can_reinforce(b.kind) {
                        let mut tip=Tip::name("Reinforce with clay").line("Durability",if b.kind==BuildingKind::Shelter{"1.75×"}else{"1.5×"});
                        for (g,a) in sim::maintenance::cost(b,sim::maintenance::Refit::Reinforce).0 {tip.push(TipLine::new(good_name(g),num(a)).icon(good_icon(g),good_color(g)));}
                        r.icon_button(good_icon(Good::Clay),14.0,HudAction::ReinforceBuilding(id),ACCENT,false).tip(tip);
                    }
                });
            }
        }
        if let Some(site) = place.and_then(|pl| pl.site.as_ref()) {
            u.rule();
            if let Some(job)=s.maintenance.jobs.get(&id) {
                u.text(if job.kind==sim::maintenance::Refit::Reinforce{"Clay reinforcement in progress"}else{"Repairs in progress"},12.0,INK);
                u.icon_label_button(Icon::Undo,"Cancel maintenance",12.0,HudAction::CancelMaintenance(id),FAINT,false).tip(Tip::name("Return delivered materials; preserve the existing structure"));
            }
            u.row(10.0, |r| {
                for a in &site.needed {
                    let got = a.1 - site.missing(a.0);
                    r.row(3.0, |x| {
                        x.icon(good_icon(a.0), 15.0, good_color(a.0));
                        x.bold(format!("{}/{}", got.round() as i64, a.1.round() as i64), 12.0, if site.missing(a.0) > 0.0 { WARN } else { GOOD });
                    })
                    .tip(Tip::name(good_name(a.0)));
                }
            });
            bar_row(u, Icon::Hammer, INK, site.fraction(), format!("{:.0}%", site.fraction() * 100.0), Tip::name("Built").line("Labour", format!("{}/{}", num(site.labour_done), num(site.labour_needed))));
        }
        if let Some(pl) = place.filter(|pl| !pl.slots.is_empty()) {
            u.rule();
            let role = pl.slots[0].role;
            let filled = pl.slots.iter().filter(|sl| sl.worker.is_some()).count();
            u.row(5.0, |r| {
                r.icon(role_icon(role), 18.0, INK).tip(Tip::name(role_name(role)));
                for sl in &pl.slots {
                    match sl.worker.and_then(|w| s.colony.beaver(w)) {
                        Some(bv) => {
                            let tip = if sl.locked { Tip::name(bv.name.clone()).row(TipLine::new("Placed by hand", "").icon(Icon::HandLock, MUTED)) } else { Tip::name(bv.name.clone()) };
                            r.icon(if sl.locked { Icon::HandLock } else { Icon::Dot }, 16.0, GOOD).tip(tip).action(HudAction::SelectBeaver(bv.id));
                        }
                        None => {
                            r.icon(Icon::DotEmpty, 16.0, WARN).tip(Tip::name("Empty"));
                        }
                    }
                }
                r.spacer();
                r.icon_button(Icon::Minus, 11.0, HudAction::RemoveWorker(id), FAINT, false).tip(Tip::name("Free a worker"));
                let add_tip = super::add_worker_tip(s, id);
                let _ = filled;
                r.icon_button(Icon::Plus, 11.0, HudAction::AddWorker(id), if add_tip.lines.is_empty() { Color::NONE } else { FAINT }, false).tip(add_tip);
            });
        }
        if let Some(rep) = sim::hud::building_report(s, id) {
            u.rule();
            u.row(8.0, |r| {
                if rep.measured.is_empty() {
                    r.bold("-", 13.0, MUTED);
                }
                for (g, rate) in &rep.measured {
                    r.row(2.0, |x| {
                        x.icon(good_icon(*g), 16.0, good_color(*g));
                        x.bold(format!("{}{}", if rep.steady { "" } else { "~" }, num(*rate)), 13.0, GOOD);
                    });
                }
                r.spacer();
                if let Some((g, max)) = rep.ceiling {
                    r.row(3.0, |x| {
                        x.text("Max", 11.5, MUTED);
                        x.icon(good_icon(g), 13.0, good_color(g));
                        x.bold(num(max), 12.0, MUTED);
                    })
                    .tip(Tip::name("All slots, no walking").line("Per day", num(max)));
                }
            })
            .tip(Tip::name("Made per day").line(if rep.steady { "" } else { "Settling" }, ""));
            let shown: Vec<&sim::hud::Factor> = rep.factors.iter().filter(|f| !matches!(f.label.as_str(), "Workers" | "Crafters")).collect();
            let limiting: Vec<&sim::hud::Factor> = shown.iter().copied().filter(|f| f.ok == Some(false)).collect();
            if let Some(f) = limiting.first() {
                factor_row(u, f);
            }
            let open = c.hud.details_open;
            u.row(4.0, |r| {
                r.icon(if open { Icon::Collapse } else { Icon::Expand }, 11.0, MUTED);
                r.icon(Icon::Log, 12.0, MUTED);
            })
            .action(HudAction::ToggleDetails)
            .tip(Tip::name("Details"));
            if open {
                for f in shown.iter().copied().filter(|f| f.ok != Some(false) || limiting.len() > 1) {
                    factor_row(u, f);
                }
            }
        }
        if b.kind == BuildingKind::Warehouse && b.is_complete() {
            u.rule();
            u.row(10.0, |r| {
                for g in Good::ALL {
                    let have = s.work.stock_of(id, g);
                    if have > 0.05 {
                        r.row(3.0, |x| {
                            x.icon(good_icon(g), 15.0, good_color(g));
                            x.bold(format!("{}", have.round() as i64), 12.0, INK);
                        })
                        .tip(Tip::name(good_name(g)));
                    }
                }
                r.spacer();
                let on = s.work.balanced.contains(&id);
                r.icon_button(Icon::Balance, 14.0, HudAction::Balance(id, !on), FAINT, on)
                    .tip(Tip::name("Balance").line(if on { "On" } else { "Off" }, "").line("Haulers share goods with balanced stores", ""));
            });
        }
        if place.is_some_and(|pl| pl.area.is_some()) && b.is_complete() {
            let editing = c.hud.area_edit == Some(id);
            u.row(6.0, |r| {
                r.icon_button(Icon::Area, 14.0, HudAction::EditArea(id), FAINT, editing).tip(Tip::name("Work area").line("Drag on the map", ""));
            });
        }
    })
}

fn factor_row(u: &mut Ui, f: &sim::hud::Factor) {
    let l = f.label.to_lowercase();
    let icon = if l.contains("tree") {
        Icon::Tree
    } else if l.contains("plant") {
        Icon::Plant
    } else if l.contains("mud") || l.contains("clay") {
        Icon::MudBank
    } else if l.contains("walk") {
        Icon::Walk
    } else if l.contains("store for") {
        Icon::NoStore
    } else if l.contains("worker") || l.contains("crafter") {
        Icon::Beaver
    } else if l.contains("wheel") {
        Icon::Wheel
    } else if l.contains("room") {
        Icon::Full
    } else if l.contains("recipe") {
        Icon::Tools
    } else {
        Icon::Dot
    };
    let col = match f.ok {
        Some(true) => GOOD,
        Some(false) => WARN,
        None => INK,
    };
    let v = f.value.split([' ', ',', ':']).next().unwrap_or("").to_string();
    let value = if v == "none" { "0".to_string() } else { v };
    u.row(6.0, |r| {
        r.icon(icon, 15.0, col);
        r.spacer();
        if f.ok == Some(false) {
            r.icon(Icon::Warn, 9.0, WARN);
        }
        r.bold(value, 12.5, col);
    })
    .tip(Tip::name(f.label.clone()).with_value(f.value.clone()));
}

fn job_icon(j: Job) -> Icon {
    match j {
        Job::Forager => Icon::RoleForager,
        Job::Logger => Icon::RoleLogger,
        Job::Builder => Icon::RoleBuilder,
        Job::Digger => Icon::RoleMudDigger,
        Job::Hauler => Icon::RoleHauler,
        Job::Guard => Icon::RoleGuard,
        Job::Crafter => Icon::RoleCrafter,
        Job::Scout => Icon::Walk,
    }
}

pub fn mood_icon(m: f32) -> (Icon, Color) {
    if m >= 0.66 {
        (Icon::MoodGood, GOOD)
    } else if m >= 0.4 {
        (Icon::MoodOk, WARN)
    } else {
        (Icon::MoodBad, BAD)
    }
}

pub fn mood_tip(s: &sim::SimState, b: &sim::colony::Beaver) -> Tip {
    let (i, c) = mood_icon(b.mood);
    let mut t = Tip::titled(Some((i, c)), "Mood").with_value(format!("{:.0}", b.mood * 100.0));
    for r in sim::hud::mood_breakdown(s, b.id).rows {
        t.push(TipLine::new(r.label, format!("{:.0}", r.value * 100.0)));
    }
    t
}

fn health_tip(s: &sim::SimState, b: &sim::colony::Beaver) -> Tip {
    let mut t = Tip::titled(Some((Icon::Health, GOOD)), "Health").with_value(format!("{:.0}", b.health * 100.0));
    for r in sim::hud::health_breakdown(s, b.id).rows {
        t.push(TipLine::new(r.label, format!("{:+.0}", r.value * 100.0)).color(if r.value < 0.0 { BAD } else { INK }));
    }
    t.line("Recovery", "Food and warmth")
}

fn beaver(c: &Ctx, id: BeaverId) -> VNode {
    let s = c.s;
    let Some(b) = s.colony.beaver(id) else {
        return shell(|u| {
            u.text("-", 12.0, MUTED);
        });
    };
    shell(|u| {
        let tip = Tip::titled(Some((stage_icon(b.stage), INK)), b.name.clone()).line("Age", format!("{:.1}", b.age_years())).line("Life", format!("{:.0}", b.lifespan_days / 360.0));
        header(c, Selection::Beaver(id), u, stage_icon(b.stage), b.name.clone(), Some(tip), |r| {
            r.icon_button(Icon::Follow, 12.0, HudAction::Follow(id), FAINT, c.hud.follow == Some(id)).tip(Tip::name("Follow"));
        });
        if let Some(f) = s.colony.family(b.family) {
            u.row(6.0, |r| { r.icon(Icon::Home, 14.0, MUTED); r.text(f.name.clone(), 12.0, INK); });
        }
        for (label, relative) in [("Mate", b.mate), ("Mother", b.mother), ("Father", b.father)] {
            if let Some(relative) = relative {
                u.row(6.0, |r| {
                    r.text(label, 11.0, MUTED);
                    if let Some(other) = s.colony.beaver(relative) {
                        r.text(other.name.clone(), 12.0, INK).action(HudAction::SelectBeaver(relative));
                    } else { r.text("Deceased / departed", 11.0, MUTED); }
                });
            }
        }
        if let Some(p) = &b.pregnancy {
            u.row(6.0, |r| { r.icon(Icon::Kit, 14.0, INK); r.text(format!("{} kits expected in {:.0}d", p.litter, (p.due_day - s.time_days).max(0.0)), 12.0, INK); });
        }
        u.row(6.0, |r| {
            match s.work.slot_of(id) {
                Some((bid, _, role)) => {
                    r.icon(role_icon(role), 18.0, INK).tip(Tip::name(role_name(role)));
                    if let Some(bb) = s.buildings.building(bid) {
                        r.icon_label_button(building_icon(bb.kind), "", 14.0, HudAction::SelectBuilding(bid), FAINT, false).tip(Tip::name(bb.kind.spec().name));
                    }
                }
                None => {
                    let ordered = s.agents.get(id).and_then(|a| a.assignment).is_some();
                    r.icon(if ordered { Icon::Hammer } else { Icon::Idle }, 18.0, if ordered { INK } else { WARN }).tip(Tip::name(if ordered { "By order" } else { "Free" }));
                }
            }
            r.spacer();
            let (mi, mc) = mood_icon(b.mood);
            r.row(3.0, |x| {
                x.icon(mi, 16.0, mc);
                x.bold(format!("{:.0}", b.mood * 100.0), 12.0, INK);
            })
            .tip(mood_tip(s, b));
            r.row(3.0, |x| {
                x.icon(Icon::Health, 16.0, if b.health < 0.5 { BAD } else { GOOD });
                x.bold(format!("{:.0}", b.health * 100.0), 12.0, INK);
            })
            .tip(health_tip(s, b));
        });
        u.rule();
        let n = &b.needs;
        for (name, v) in [("Food", n.food), ("Rest", n.rest), ("Warmth", n.warmth), ("Safety", n.safety), ("Social", n.social)] {
            u.row(6.0, |r| {
                r.text(name, 11.5, MUTED).style(|nn| nn.width = Val::Px(56.0));
                r.bar(v, 180.0, 6.0, if v < 0.35 { BAD } else if v < 0.6 { WARN } else { GOOD });
                r.spacer();
                r.bold(format!("{:.0}", v * 100.0), 11.5, INK);
            });
        }
        u.rule();
        u.node(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(10.0), row_gap: Val::Px(4.0), ..default() }, |r| {
            for j in Job::ALL {
                let v = b.skills[j.index()];
                r.row(3.0, |x| {
                    x.icon(job_icon(j), 14.0, MUTED);
                    x.bar(v, 44.0, 5.0, INK);
                })
                .tip(Tip::name(format!("{j:?}")).with_value(format!("{:.0}", v * 100.0)));
            }
        });
        let locked = s.work.is_locked(id) || s.work.player_orders.contains(&id);
        if s.work.slot_of(id).is_some() || s.agents.get(id).and_then(|a| a.assignment).is_some() {
            u.row(6.0, |r| {
                r.spacer();
                if locked {
                    r.icon_button(Icon::HandLock, 11.0, HudAction::Unlock(id), FAINT, true).tip(Tip::name("Placed by hand").line("Click", "back to auto"));
                }
                r.icon_button(Icon::Minus, 11.0, HudAction::FreeBeaver(id), FAINT, false).tip(Tip::name("Free"));
            });
        }
    })
}

fn resource(c: &Ctx, rid: sim::resources::ResourceId) -> VNode {
    let s = c.s;
    let Some(r) = s.resources.get(rid) else {
        return shell(|u| {
            u.text("-", 12.0, MUTED);
        });
    };
    shell(|u| {
        header(c, Selection::Tree(rid), u, resource_icon(r.kind), resource_name(r.kind).to_string(), None, |_| {});
        if r.kind == sim::resources::ResourceKind::RiverClay {
            u.text(if r.wet {"River sediment: slowly replenishing"} else {"River sediment: recovery paused"},12.0,MUTED);
            if !r.wet {u.text("Needs flowing, unfrozen water",11.0,MUTED);}
            u.text(format!("+{:.2} clay/day, up to {:.0}",sim::resources::RIVER_CLAY_PER_DAY,r.max_amount),11.0,MUTED);
        }
        let frac = if r.max_amount > 0.0 { r.amount / r.max_amount } else { 0.0 };
        bar_row(u, resource_icon(r.kind), GOOD, frac, num(r.amount), Tip::name("Left").with_value(format!("{}/{}", num(r.amount), num(r.max_amount))));
        let areas: Vec<BuildingId> = s.work.places.iter().filter(|p| p.area.is_some_and(|a| a.contains(r.cell))).map(|p| p.building).collect();
        if !areas.is_empty() {
            u.row(4.0, |row| {
                row.icon(Icon::Area, 14.0, MUTED);
                for bid in areas {
                    if let Some(bb) = s.buildings.building(bid) {
                        row.icon_label_button(building_icon(bb.kind), "", 13.0, HudAction::SelectBuilding(bid), FAINT, false).tip(Tip::name(bb.kind.spec().name));
                    }
                }
            });
        }
    })
}

pub fn cell_rows(s: &sim::SimState, cell: crate::flow::Cell) -> Vec<(Icon, Color, &'static str, String)> {
    let (x, y) = cell;
    let h = s.height.get(x, y).copied().unwrap_or(0.0);
    let d = s.water.depth.get(x, y).copied().unwrap_or(0.0);
    let discharge = s.water.discharge.get(x, y).copied().unwrap_or(0.0);
    let veg = s.ecology.vegetation.get(x, y).copied().unwrap_or(0.0);
    let flood = s.ecology.flood_days.get(x, y).copied().unwrap_or(0.0);
    let mut rows = vec![(Icon::Profile, MUTED, "Height", num(h))];
    if d > 0.01 {
        rows.push((Icon::Drop, WATER, "Depth", format!("{:.2}", d)));
        rows.push((Icon::Push, WATER, "Flow", format!("{discharge:.3} m3/s")));
    }
    rows.push((Icon::Plant, GOOD, "Cover", format!("{:.0}%", veg * 100.0)));
    if flood > 0.5 {
        rows.push((Icon::Flood, WATER, "Flooded", days(flood)));
    }
    rows
}

fn cell_panel(c: &Ctx, cell: crate::flow::Cell) -> VNode {
    shell(|u| {
        header(c, Selection::Cell(cell), u, Icon::Profile, String::new(), None, |_| {});
        for (i, col, name, v) in cell_rows(c.s, cell) {
            u.row(6.0, |r| {
                r.icon(i, 15.0, col);
                r.spacer();
                r.bold(v, 12.5, INK);
            })
            .tip(Tip::name(name));
        }
        if let Some(b) = c.s.buildings.buildings.iter().find(|b| b.dam.is_some() && b.cells().any(|x| x == cell)) {
            u.rule();
            u.row(6.0, |r| {
                r.spacer();
                r.icon_button(Icon::Dam, 15.0, HudAction::SelectBuilding(b.id), ACCENT, false).tip(Tip::name("Open the dam"));
            });
        }
    })
}

fn lodge(c: &Ctx, id: sim::colony::LodgeId) -> VNode {
    let s = c.s;
    shell(|u| {
        header(c, Selection::Lodge(id), u, Icon::Lodge, String::new(), Some(Tip::name("Lodge")), |_| {});
        let residents: Vec<&sim::colony::Beaver> = s.colony.beavers.iter().filter(|b| b.lodge == Some(id)).collect();
        u.row(6.0, |r| {
            r.icon(Icon::Beaver, 16.0, INK);
            r.bold(format!("{}", residents.len()), 13.0, INK);
        })
        .tip({
            let mut t = Tip::name("Residents");
            for b in residents.iter().take(10) {
                t.push(TipLine::new(b.name.clone(), "").icon(stage_icon(b.stage), INK));
            }
            t
        });
        if let Some(l) = s.colony.lodges.iter().find(|l| l.id == id) {
            bar_row(u, Icon::Hammer, INK, l.condition, format!("{:.0}%", l.condition * 100.0), Tip::name("Condition"));
        }
    })
}
