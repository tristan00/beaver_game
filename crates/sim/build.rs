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
    let roles = ["Forager","Logger","MudDigger","Builder","Hauler","Forester","Maintainer","Guard","PebbleCollector"];
    let mut order = Vec::new();
    for row in db.prepare("SELECT role,priority FROM role_policy ORDER BY priority")?.query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,u32>(1)?)))? {
        let (role,priority) = row?;
        assert!(roles.contains(&role.as_str()));
        assert_eq!(priority as usize,order.len());
        order.push(format!("RoleKind::{role}"));
    }
    assert_eq!(order.len(),roles.len());
    constants.entry("jobs".into()).or_default().push_str(&format!("const DEFAULT_ROLE_ORDER: [RoleKind; {}] = [{}];\n",order.len(),order.join(",")));
    trees(&db, constants.entry("trees".into()).or_default())?;
    for (module, text) in constants { fs::write(out.join(format!("{module}_constants.rs")), text)?; }

    let mut stmt = db.prepare("SELECT id,kind,name,width,depth,site,labour,capacity,area,requires FROM building ORDER BY id")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_,u32>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,u32>(3)?,r.get::<_,u32>(4)?,r.get::<_,String>(5)?,r.get::<_,f64>(6)?,r.get::<_,f64>(7)?,r.get::<_,Option<u32>>(8)?,r.get::<_,Option<String>>(9)?)))?.collect::<Result<Vec<_>,_>>()?;
    let source = fs::read_to_string("src/buildings.rs")?;
    let kinds: Vec<_> = source.split("pub enum BuildingKind {").nth(1).unwrap().split('}').next().unwrap().split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    assert_eq!(kinds.len(), rows.len(), "content must define every supported building");
    let mut specs = String::from("const SPECS: &[Spec] = &[\n");
    for (index, (id, kind, name, width, depth, site, labour, capacity, area, requires)) in rows.iter().enumerate() {
        assert_eq!(*id, index as u32, "building IDs must follow BuildingKind order");
        assert_eq!(kind, kinds[index]);
        assert!(*width > 0 && *depth > 0 && *width <= 64 && *depth <= 64);
        assert!(matches!(site.as_str(), "TouchesWater"));
        let costs = amounts(&db, "SELECT good,amount FROM cost WHERE building=? ORDER BY position", *id)?;
        let mut slots = Vec::new();
        for role in db.prepare("SELECT role FROM slot WHERE building=? ORDER BY position")?.query_map([id], |r| r.get::<_,String>(0))? {
            let role = role?;
            assert!(matches!(role.as_str(), "Build" | "Haul" | "Forest" | "Guard" | "Collect(ResourceKind::Tree)" | "Collect(ResourceKind::Plant)" | "Collect(ResourceKind::MudBank)" | "Collect(ResourceKind::Pebbles)"), "invalid role {role}");
            slots.push(format!("Role::{role}"));
        }
        let area = area.map_or("None".into(), |a| { assert!(a > 0 && a <= 1024); format!("Some(({a},{a}))") });
        let requires = requires.as_ref().map_or("None".into(), |r| {
            assert!(kinds.contains(&r.as_str()) && r != kind, "{kind} requires unknown building {r}");
            format!("Some(BuildingKind::{r})")
        });
        specs.push_str(&format!("Spec {{ name: {name:?}, footprint: ({width},{depth}), site: Site::{site}, cost: &[{costs}], work: {}, slots: &[{}], area: {area}, capacity: {}, requires: {requires} }},\n",number(*labour),slots.join(","),number(*capacity)));
    }
    specs.push_str("];\n");
    fs::write(out.join("catalogue.rs"), specs)?;
    Ok(())
}

/// Species stats follow the `Tree` enum order; every shipped map and `default` need a flora.
fn trees(db: &Connection, out: &mut String) -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=src/trees.rs");
    println!("cargo:rerun-if-changed=maps");
    let source = fs::read_to_string("src/trees.rs")?;
    let kinds: Vec<String> = source.split("pub enum Tree {").nth(1).unwrap().split('}').next().unwrap().split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    let rows = db.prepare("SELECT position,species,growth,logs,branches,bark,fell_work,flood,max_depth FROM tree_species ORDER BY position")?
        .query_map([], |r| Ok((r.get::<_,u32>(0)?,r.get::<_,String>(1)?,[r.get::<_,f64>(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?])))?.collect::<Result<Vec<_>,_>>()?;
    assert_eq!(rows.iter().map(|r| r.1.clone()).collect::<Vec<_>>(), kinds, "tree_species must follow the Tree enum");
    let mut specs = Vec::new();
    for (position, species, v) in &rows {
        assert_eq!(*position as usize, specs.len());
        assert!(v[0] > 0.0 && v[4] > 0.0 && v[5] > 0.0 && v[6] > 0.0, "{species} needs positive growth, felling work, flood tolerance and water depth");
        specs.push(format!("TreeSpec {{ growth: {}, logs: {}, branches: {}, bark: {}, fell_work: {}, flood: {}, max_depth: {} }}", number(v[0]), number(v[1]), number(v[2]), number(v[3]), number(v[4]), number(v[5]), number(v[6])));
    }
    out.push_str(&format!("pub const SPECIES: [TreeSpec; {}] = [{}];\n", specs.len(), specs.join(",")));
    let mut flora: BTreeMap<String, Vec<[f64; 4]>> = BTreeMap::new();
    for row in db.prepare("SELECT map,species,dry,wet,upland,open FROM tree_flora")?.query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,[r.get::<_,f64>(2)?,r.get(3)?,r.get(4)?,r.get(5)?])))? {
        let (map, species, w) = row?;
        assert!(identifier(&map));
        let i = kinds.iter().position(|k| *k == species).expect("flora names a known species");
        flora.entry(map).or_insert_with(|| vec![[0.0; 4]; kinds.len()])[i] = w;
    }
    let mut maps: Vec<String> = fs::read_dir("maps")?.filter_map(|e| e.ok()?.file_name().to_str()?.strip_suffix(".map").map(str::to_string)).collect();
    maps.push("default".into());
    for m in &maps { assert!(flora.contains_key(m), "map {m} needs a tree_flora"); }
    let mut entries = Vec::new();
    for (map, mix) in flora {
        assert!(maps.contains(&map), "tree_flora for unknown map {map}");
        assert!(mix.iter().map(|w| w[0]).sum::<f64>() > 0.0 && mix.iter().map(|w| w[1]).sum::<f64>() > 0.0, "{map} needs dry and wet trees");
        let mix: Vec<String> = mix.iter().map(|w| format!("Mix {{ dry: {}, wet: {}, upland: {}, open: {} }}", number(w[0]), number(w[1]), number(w[2]), number(w[3]))).collect();
        entries.push(format!("({map:?}, [{}])", mix.join(",")));
    }
    out.push_str(&format!("pub const FLORA: [(&str, [Mix; {}]); {}] = [{}];\n", kinds.len(), entries.len(), entries.join(",")));
    Ok(())
}

fn amounts(db: &Connection, sql: &str, key: u32) -> rusqlite::Result<String> {
    let mut values = Vec::new();
    for row in db.prepare(sql)?.query_map([key], |r| Ok((r.get::<_,String>(0)?,r.get::<_,f64>(1)?)))? {
        let (good, amount) = row?;
        assert!(matches!(good.as_str(), "Food" | "Sticks" | "Clay" | "Pebbles" | "Saplings"));
        values.push(format!("(Good::{good},{})", number(amount)));
    }
    Ok(values.join(","))
}
