//! Number provenance (owner: `ui_shell`). Every number a view-model shows is an `Explained`:
//! its value, unit and the terms that add up to it. Hover shows `parts`. Tests run `audit()` over
//! every number visible at once (navigation bar, time controls and the open screen).
//!
//! Two kinds of number exist:
//! * derived numbers ([`Explained::sum`]): the value *is* the sum of its parts, and each part is
//!   a different, smaller quantity than the number itself;
//! * readings ([`Explained::reading`]): a quantity the sim stores as one figure (a stock level, a
//!   catalogue constant). Its one part names where it is read from, never the number itself, so a
//!   derived number cannot pose as a reading by explaining itself.

/// Where a term comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// A term computed from the sim (a count, a weighted need, a rate).
    Fact,
    /// The single stored figure a reading comes from (e.g. "colony larder", "tech catalogue").
    Reading,
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
    /// Decimal places shown.
    pub dec: u8,
    /// What the number measures, over which things (e.g. `"head count:3,4,7"`). Two numbers with the
    /// same key are the same figure, whatever their labels say.
    pub key: Option<String>,
}

impl Explained {
    /// A number whose parts are exactly its terms.
    pub fn sum(label: &str, unit: &'static str, parts: Vec<Part>) -> Self {
        let value = parts.iter().map(|p| p.value).sum();
        Self { label: label.to_string(), value, unit, parts, dec: 0, key: None }
    }
    /// A figure the sim stores as one value, read from `origin` (which must not be the label).
    pub fn reading(label: &str, unit: &'static str, value: f32, origin: &str) -> Self {
        let part = Part { label: origin.to_string(), value, source: Source::Reading };
        Self { label: label.to_string(), value, unit, parts: vec![part], dec: 0, key: None }
    }
    /// Decimal places to show.
    pub fn dp(mut self, dec: u8) -> Self {
        self.dec = dec;
        self
    }
    /// Names what the number measures and over which things (see [`Explained::key`]).
    pub fn of(mut self, quantity: &str, things: impl IntoIterator<Item = u64>) -> Self {
        let mut ids: Vec<u64> = things.into_iter().collect();
        ids.sort_unstable();
        let ids: Vec<String> = ids.iter().map(|i| i.to_string()).collect();
        self.key = Some(format!("{quantity}:{}", ids.join(",")));
        self
    }
    /// The number as shown, e.g. `12` or `4.5%`.
    pub fn show(&self) -> String {
        format!("{:.*}{}", self.dec as usize, self.value, self.unit)
    }
    /// A term of the breakdown.
    pub fn part(label: impl Into<String>, value: f32) -> Part {
        Part { label: label.into(), value, source: Source::Fact }
    }
    /// A sum of `parts`, or `0` explained as "none" when there are no parts.
    pub fn sum_or_zero(label: &str, unit: &'static str, parts: Vec<Part>) -> Self {
        if parts.is_empty() { Self::sum(label, unit, vec![Self::part("None", 0.0)]) } else { Self::sum(label, unit, parts) }
    }
    /// True if the parts add up to the value (within rounding).
    pub fn sums(&self) -> bool {
        let t: f32 = self.parts.iter().map(|p| p.value).sum();
        (t - self.value).abs() <= 0.01 * self.value.abs().max(1.0)
    }
    pub fn is_reading(&self) -> bool {
        matches!(self.parts.as_slice(), [p] if p.source == Source::Reading)
    }
    /// Parts as shown (label and rounded value), sorted: equal breakdowns compare equal.
    fn shape(&self) -> Vec<(String, i64)> {
        let mut v: Vec<(String, i64)> = self.parts.iter().map(|p| (p.label.clone(), (p.value * 100.0).round() as i64)).collect();
        v.sort();
        v
    }
}

/// Implemented by every view-model so one test can walk all numbers on a screen.
pub trait Numbers {
    fn numbers(&self) -> Vec<&Explained>;
}

/// Checks the numbers on one screen; see [`audit_all`].
#[cfg_attr(not(test), allow(dead_code))]
pub fn audit(screen: &dyn Numbers) -> Vec<String> {
    audit_all(&screen.numbers())
}

/// Checks numbers that are visible together. Returns problems:
/// * a number with no breakdown, or whose parts do not add up to it;
/// * a number explained by a part carrying its own label (it explains nothing);
/// * a reading mixed into a sum;
/// * the same figure twice: same label and value, same key, or same value with the same breakdown
///   (readings, and numbers whose keys say they measure different things, are exempt from the last).
pub fn audit_all(nums: &[&Explained]) -> Vec<String> {
    let mut bad = Vec::new();
    for (i, n) in nums.iter().enumerate() {
        if n.parts.is_empty() {
            bad.push(format!("{} has no breakdown", n.label));
        }
        if !n.sums() {
            bad.push(format!("{} does not add up", n.label));
        }
        if n.parts.iter().any(|p| p.label.eq_ignore_ascii_case(&n.label)) {
            bad.push(format!("{} is explained by itself", n.label));
        }
        if n.parts.iter().any(|p| p.source == Source::Reading) && n.parts.len() != 1 {
            bad.push(format!("{} mixes a reading into a sum", n.label));
        }
        for m in &nums[..i] {
            let same_value = m.show() == n.show();
            if m.label == n.label && same_value {
                bad.push(format!("{} shown twice", n.label));
            } else if m.key.is_some() && m.key == n.key && !(m.value == 0.0 && n.value == 0.0) {
                bad.push(format!("'{}' and '{}' are the same figure", m.label, n.label));
            } else if same_value && m.value != 0.0 && !m.is_reading() && !n.is_reading() && !(m.key.is_some() && n.key.is_some()) && m.shape() == n.shape() {
                bad.push(format!("'{}' and '{}' are the same figure ({})", m.label, n.label, n.show()));
            }
        }
    }
    bad
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(v: Vec<Explained>) -> Vec<String> {
        audit_all(&v.iter().collect::<Vec<_>>())
    }

    #[test]
    fn audit_rejects_a_number_explained_by_itself() {
        let selfish = Explained::sum("Lodge condition", "%", vec![Explained::part("Lodge condition", 80.0)]);
        assert!(!n(vec![selfish]).is_empty());
    }

    #[test]
    fn audit_rejects_parts_that_do_not_add_up() {
        let mut wrong = Explained::sum("Logs", "", vec![Explained::part("Made", 3.0)]);
        wrong.value = 4.0;
        assert!(!n(vec![wrong]).is_empty());
    }

    #[test]
    fn audit_rejects_the_same_figure_under_two_labels() {
        let stages = || vec![Explained::part("Adults", 2.0), Explained::part("Kits", 2.0)];
        assert!(!n(vec![Explained::sum("Beavers", "", stages()), Explained::sum("Sedgefield family size", "", stages())]).is_empty());
        let a = Explained::sum("Colony mood", "%", vec![Explained::part("Sedgefield family", 70.0)]).of("mood", [1, 2]);
        let b = Explained::sum("Sedgefield family mood", "%", vec![Explained::part("Ada", 35.0), Explained::part("Bo", 35.0)]).of("mood", [2, 1]);
        assert!(!n(vec![a, b]).is_empty());
    }

    #[test]
    fn audit_accepts_different_figures_that_happen_to_be_equal() {
        let ok = vec![
            Explained::reading("Larder stock", "", 12.0, "Colony larder"),
            Explained::sum("Logs capacity", "", vec![Explained::part("Base store", 12.0), Explained::part("Warehouses", 0.0)]),
        ];
        assert!(n(ok).is_empty());
    }
}
