//! Number provenance (owner: `ui_shell`). Every number a view-model shows is an `Explained`:
//! its value, unit and the terms that add up to it. Hover shows `parts`; a part may itself be
//! explained further (`Part::source`). Tests assert `sums()` for every number on every screen.

/// Where a term comes from, so the hover can link onwards.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// A plain fact (count, level) with no further breakdown.
    Fact,
    /// A sim system or entity that can be selected (building, family, tech).
    Entity(String),
    /// A multiplier or modifier, e.g. "winter x0.7".
    Modifier,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub label: String,
    pub value: f32,
    pub source: Source,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Explained {
    pub label: String,
    pub value: f32,
    pub unit: &'static str,
    pub parts: Vec<Part>,
}

impl Explained {
    /// A number whose parts are exactly its terms.
    pub fn sum(label: &str, unit: &'static str, parts: Vec<Part>) -> Self {
        let value = parts.iter().map(|p| p.value).sum();
        Self { label: label.to_string(), value, unit, parts }
    }
    /// A raw fact: one part equal to the value.
    pub fn fact(label: &str, unit: &'static str, value: f32) -> Self {
        Self { label: label.to_string(), value, unit, parts: vec![Part { label: label.to_string(), value, source: Source::Fact }] }
    }
    /// True if the parts add up to the value (within rounding).
    pub fn sums(&self) -> bool {
        let t: f32 = self.parts.iter().map(|p| p.value).sum();
        (t - self.value).abs() <= 0.01 * self.value.abs().max(1.0)
    }
}

/// Implemented by every view-model so one test can walk all numbers on a screen.
pub trait Numbers {
    fn numbers(&self) -> Vec<&Explained>;
}

/// Checks a screen: every number sums, and no label+value appears twice. Returns problems.
pub fn audit(screen: &dyn Numbers) -> Vec<String> {
    let nums = screen.numbers();
    let mut bad = Vec::new();
    for (i, n) in nums.iter().enumerate() {
        if !n.sums() {
            bad.push(format!("{} does not add up", n.label));
        }
        if nums[..i].iter().any(|m| m.label == n.label && m.value == n.value) {
            bad.push(format!("{} shown twice", n.label));
        }
    }
    bad
}
