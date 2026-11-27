use std::{collections::BTreeMap, env, fs, path::PathBuf};

fn number(v: &toml::Value) -> String {
    let x = v.as_float().or_else(|| v.as_integer().map(|i| i as f64)).expect("tuning values are numbers");
    assert!(x.is_finite() && x >= 0.0, "tuning values must be finite and nonnegative");
    format!("{x:?}")
}

fn main() {
    println!("cargo:rerun-if-changed=tuning.toml");
    let tuning: toml::Table = fs::read_to_string("tuning.toml").unwrap().parse().unwrap();
    let mut files: BTreeMap<String, String> = BTreeMap::new();
    for (module, types) in &tuning {
        let text = files.entry(module.clone()).or_default();
        for (ty, consts) in types.as_table().expect("[module.type] sections") {
            for (name, v) in consts.as_table().expect("constants") {
                let line = match (ty.as_str(), v) {
                    ("range", toml::Value::Array(a)) => format!("std::ops::RangeInclusive<usize> = {}..={}", a[0].as_integer().unwrap(), a[1].as_integer().unwrap()),
                    ("usize" | "u32", toml::Value::Integer(i)) => format!("{ty} = {}", u32::try_from(*i).expect("u32 range")),
                    ("f32" | "f64", toml::Value::Array(a)) => format!("[{ty}; {}] = [{}]", a.len(), a.iter().map(number).collect::<Vec<_>>().join(",")),
                    ("f32" | "f64", toml::Value::String(r)) => {
                        let (factor, target) = r.split_once(" * ").expect("<factor> * <module>::<NAME>");
                        let (m, n) = target.split_once("::").expect("<module>::<NAME>");
                        assert!(tuning.get(m).and_then(|t| t.as_table()).is_some_and(|t| t.values().any(|c| c.get(n).is_some())), "unknown constant {target}");
                        let path = if matches!(m, "colony" | "ecology" | "buildings") { format!("crate::tuning::{m}") } else { format!("crate::{m}") };
                        format!("{ty} = {:?} * ({path}::{n} as {ty})", factor.parse::<f64>().expect("factor"))
                    }
                    ("f32" | "f64", v) => format!("{ty} = {}", number(v)),
                    _ => panic!("tuning {module}.{ty}.{name}: unsupported"),
                };
                text.push_str(&format!("pub const {name}: {line};\n"));
            }
        }
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    for (module, text) in files {
        fs::write(out.join(format!("{module}_constants.rs")), text).unwrap();
    }
}
