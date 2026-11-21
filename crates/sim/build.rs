use rusqlite::{Connection, OpenFlags};
use std::{collections::{BTreeMap, BTreeSet}, env, fs, path::PathBuf};

fn identifier(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

fn number(value: f64) -> String {
    assert!(value.is_finite() && value >= 0.0, "content values must be finite and nonnegative");
    format!("{value:?}")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = "content/game.sqlite3";
    println!("cargo:rerun-if-changed={path}");
    println!("cargo:rerun-if-changed=src/buildings.rs");
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let version: u32 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    assert_eq!(version, 1, "unsupported content schema");
    let integrity: String = db.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    assert_eq!(integrity, "ok");
    assert!(!db.prepare("PRAGMA foreign_key_check")?.exists([])?, "content has dangling references");

    let mut constants: BTreeMap<String, String> = BTreeMap::new();
    let mut stmt = db.prepare("SELECT module,name,type,value FROM gameplay_constant ORDER BY module,name")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,f64>(3)?)))? {
        let (module, name, ty, value) = row?;
        assert!(identifier(&module), "invalid constants module");
        assert!(identifier(&name), "invalid tuning name");
        let literal = match ty.as_str() {
            "f32" => { assert!(value <= f32::MAX as f64); number(value) },
            "f64" => number(value),
            "usize" | "u32" => {
                assert!(value.is_finite() && value >= 0.0 && value.fract() == 0.0 && value <= u32::MAX as f64);
                format!("{}", value as u64)
            }
            _ => panic!("unsupported tuning type {ty}"),
        };
        constants.entry(module).or_default().push_str(&format!("pub const {name}: {ty} = {literal};\n"));
    }

    let references = db.prepare("SELECT module,name,type,target_module,target_name,factor FROM constant_reference ORDER BY module,name")?
        .query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,f64>(5)?)))?.collect::<Result<Vec<_>,_>>()?;
    let mut known = db.prepare("SELECT module,name FROM gameplay_constant")?.query_map([], |r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?.collect::<Result<BTreeSet<_>,_>>()?;
    for (module,name,..) in &references { assert!(known.insert((module.clone(),name.clone())), "duplicate constant"); }
    for (module,name,ty,target_module,target_name,factor) in references {
        assert!([&module,&name,&target_module,&target_name].into_iter().all(|s|identifier(s)));
        assert!(matches!(ty.as_str(),"f32"|"f64") && factor.is_finite() && factor > 0.0);
        assert!(known.contains(&(target_module.clone(),target_name.clone())),"unknown constant reference");
        let path = if matches!(target_module.as_str(),"colony"|"ecology"|"buildings") { format!("crate::tuning::{target_module}") } else { format!("crate::{target_module}") };
        constants.entry(module).or_default().push_str(&format!("pub const {name}: {ty} = {} * ({path}::{target_name} as {ty});\n",number(factor)));
    }
    let mut pools: BTreeMap<String,Vec<String>> = BTreeMap::new();
    for row in db.prepare("SELECT pool,position,name FROM name_pool ORDER BY pool,position")?.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,u32>(1)?,r.get::<_,String>(2)?)))? {
        let (pool,position,name) = row?;
        assert!(matches!(pool.as_str(),"FIRST_NAMES"|"FAMILY_NAMES") && !name.is_empty());
        let entries=pools.entry(pool).or_default();assert_eq!(position as usize,entries.len());entries.push(format!("{name:?}"));
    }
    for (pool,names) in pools { constants.entry("colony".into()).or_default().push_str(&format!("pub const {pool}: [&str; {}] = [{}];\n",names.len(),names.join(","))); }

    let mut arrays: BTreeMap<(String,String,String), Vec<String>> = BTreeMap::new();
    for row in db.prepare("SELECT module,name,type,position,value FROM constant_array ORDER BY module,name,position")?.query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,u32>(3)?,r.get::<_,f64>(4)?)))? {
        let (module,name,ty,position,value) = row?;
        assert!(identifier(&module) && identifier(&name) && ty == "f32");
        let array = arrays.entry((module,name,ty)).or_default();
        assert_eq!(position as usize,array.len(),"array positions must be contiguous");
        array.push(number(value));
    }
    for ((module,name,ty),array) in arrays {
        constants.entry(module).or_default().push_str(&format!("pub const {name}: [{ty}; {}] = [{}];\n",array.len(),array.join(",")));
    }
    for row in db.prepare("SELECT module,name,type,minimum,maximum FROM constant_range")?.query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,f64>(3)?,r.get::<_,f64>(4)?)))? {
        let (module,name,ty,minimum,maximum) = row?;
        assert!(identifier(&module) && identifier(&name) && ty == "usize" && minimum >= 0.0 && minimum <= maximum
            && minimum.fract() == 0.0 && maximum.fract() == 0.0 && maximum <= u32::MAX as f64);
        let (minimum,maximum) = (minimum as u32,maximum as u32);
        constants.entry(module).or_default().push_str(&format!("pub const {name}: std::ops::RangeInclusive<usize> = {minimum}..={maximum};\n"));
    }
    let mut starting = Vec::new();
    for row in db.prepare("SELECT position,good,amount FROM starting_good ORDER BY position")?.query_map([], |r| Ok((r.get::<_,u32>(0)?,r.get::<_,String>(1)?,r.get::<_,f64>(2)?)))? {
        let (position,good,amount) = row?;
        assert_eq!(position as usize,starting.len());
        assert!(matches!(good.as_str(),"Food"|"Sticks"|"Clay"));
        starting.push(format!("(crate::buildings::Good::{good},{})",number(amount)));
    }
    constants.entry("starts".into()).or_default().push_str(&format!("pub const START_BUNDLE: [(crate::buildings::Good,f32); {}] = [{}];\n",starting.len(),starting.join(",")));
    let roles = ["Forager","Logger","MudDigger","Builder","Hauler","Crafter","Forester","Guard","Scout"];
    let mut order = Vec::new();
    let mut weights = BTreeMap::new();
    for row in db.prepare("SELECT role,priority,weight FROM role_policy ORDER BY priority")?.query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,u32>(1)?,r.get::<_,f64>(2)?)))? {
        let (role,priority,weight) = row?;
        assert!(roles.contains(&role.as_str()) && weight.is_finite() && weight > 0.0);
        assert_eq!(priority as usize,order.len());
        order.push(format!("RoleKind::{role}"));
        weights.insert(role,weight);
    }
    assert_eq!(order.len(),roles.len());
    constants.entry("jobs".into()).or_default().push_str(&format!("const DEFAULT_ROLE_ORDER: [RoleKind; {}] = [{}];\n",order.len(),order.join(",")));
    constants.entry("work".into()).or_default().push_str(&format!("const DEFAULT_ROLE_WEIGHTS: [f32; {}] = [{}];\n",roles.len(),roles.iter().map(|r|number(weights[*r])).collect::<Vec<_>>().join(",")));
    for (module, text) in constants { fs::write(out.join(format!("{module}_constants.rs")), text)?; }

    let mut stmt = db.prepare("SELECT id,kind,name,width,depth,site,labour,capacity,area FROM building ORDER BY id")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_,u32>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,u32>(3)?,r.get::<_,u32>(4)?,r.get::<_,String>(5)?,r.get::<_,f64>(6)?,r.get::<_,f64>(7)?,r.get::<_,Option<u32>>(8)?)))?.collect::<Result<Vec<_>,_>>()?;
    let source = fs::read_to_string("src/buildings.rs")?;
    let kinds: Vec<_> = source.split("pub enum BuildingKind {").nth(1).unwrap().split('}').next().unwrap().split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    assert_eq!(kinds.len(), rows.len(), "content must define every supported building");
    let mut graph = BTreeMap::<u32, Vec<u32>>::new();
    let mut specs = String::from("const SPECS: &[Spec] = &[\n");
    for (index, (id, kind, name, width, depth, site, labour, capacity, area)) in rows.iter().enumerate() {
        assert_eq!(*id, index as u32, "building IDs must follow BuildingKind order");
        assert_eq!(kind, kinds[index]);
        assert!(*width > 0 && *depth > 0 && *width <= 64 && *depth <= 64);
        assert!(matches!(site.as_str(), "Pond" | "Dry" | "TouchesWater" | "WaterByDock"));
        let costs = amounts(&db, "SELECT good,amount FROM cost WHERE building=? ORDER BY position", *id)?;
        let mut prereqs = Vec::new();
        for req in db.prepare("SELECT requires FROM prerequisite WHERE building=? ORDER BY requires")?.query_map([id], |r| r.get::<_,u32>(0))? {
            let req = req?;
            graph.entry(*id).or_default().push(req);
            prereqs.push(format!("BuildingKind::{}", rows[req as usize].1));
        }
        let mut slots = Vec::new();
        for role in db.prepare("SELECT role FROM slot WHERE building=? ORDER BY position")?.query_map([id], |r| r.get::<_,String>(0))? {
            let role = role?;
            assert!(matches!(role.as_str(), "Build" | "Haul" | "Craft" | "Forester" | "Guard" | "Scout" | "Collect(ResourceKind::Tree)" | "Collect(ResourceKind::Plant)" | "Collect(ResourceKind::MudBank)"), "invalid role {role}");
            slots.push(format!("Role::{role}"));
        }
        let area = area.map_or("None".into(), |a| { assert!(a > 0 && a <= 1024); format!("Some(({a},{a}))") });
        specs.push_str(&format!("Spec {{ name: {name:?}, footprint: ({width},{depth}), site: Site::{site}, cost: &[{costs}], work: {}, prerequisites: &[{}], slots: &[{}], area: {area}, capacity: {} }},\n",number(*labour),prereqs.join(","),slots.join(","),number(*capacity)));
    }
    let mut reachable = BTreeSet::new();
    for _ in 0..rows.len() {
        for (id, ..) in &rows { if graph.get(id).is_none_or(|p|p.iter().all(|r|reachable.contains(r))) { reachable.insert(*id); } }
    }
    assert_eq!(reachable.len(), rows.len(), "building prerequisites contain a cycle");
    specs.push_str("];\n");
    fs::write(out.join("catalogue.rs"), specs)?;
    Ok(())
}

fn amounts(db: &Connection, sql: &str, key: u32) -> rusqlite::Result<String> {
    let mut values = Vec::new();
    for row in db.prepare(sql)?.query_map([key], |r| Ok((r.get::<_,String>(0)?,r.get::<_,f64>(1)?)))? {
        let (good, amount) = row?;
        assert!(matches!(good.as_str(), "Food" | "Sticks" | "Clay"));
        values.push(format!("(Good::{good},{})", number(amount)));
    }
    Ok(values.join(","))
}
