//! How a beaver grows. General experience comes from every day it works, in any job, and makes it
//! a little stronger at everything: quicker on its feet, carrying more, tiring and hungering
//! less, and hurt less by cold. Each job's skill gives one more thing (`content::SkillTrait`):
//! most make the beaver faster at that job, and a few go with it into whatever job it works.
include!(concat!(env!("OUT_DIR"), "/growth_constants.rs"));

use crate::colony::Beaver;
use crate::content::{Job, SkillTrait};

/// How strong a trait is in a beaver, 0 to 1: its skill in the job that gives it.
pub fn level(b: &Beaver, t: SkillTrait) -> f32 {
    Job::with_trait(t).map_or(0.0, |j| b.skills[j])
}

/// Gains experience from work done (`units`, as skills do), slowing as it nears full.
pub fn learn(b: &mut Beaver, units: f32) {
    b.experience = (b.experience + units * EXPERIENCE_GAIN * (1.0 - b.experience)).min(1.0);
}

/// Walking and swimming pace.
pub fn move_x(b: &Beaver) -> f32 {
    1.0 + EXPERIENCE_MOVE * b.experience
}

/// How much it carries at once, of a full load.
pub fn load_x(b: &Beaver) -> f32 {
    1.0 + EXPERIENCE_LOAD * b.experience
}

/// How fast work tires it.
pub fn tire_x(b: &Beaver) -> f32 {
    1.0 - EXPERIENCE_TIRE * b.experience
}

/// How fast it gets hungry.
pub fn hunger_x(b: &Beaver) -> f32 {
    1.0 - EXPERIENCE_HUNGER * b.experience
}

/// How much cold harms it.
pub fn cold_x(b: &Beaver) -> f32 {
    1.0 - EXPERIENCE_COLD * b.experience
}

/// Its pace carrying a load that slows a beaver to `pace`: heavy loads slow it down less.
pub fn load_pace(b: &Beaver, pace: f32) -> f32 {
    pace + (1.0 - pace.min(1.0)) * HEAVY_LOAD_EASE * level(b, SkillTrait::HeavyLoads)
}

/// How far it sees a wolf, of a beaver's usual sight.
pub fn sight_x(b: &Beaver) -> f32 {
    1.0 + SIGHT_GAIN * level(b, SkillTrait::Sight)
}

/// What tiredness leaves of its work (`rest_x` for an ordinary beaver): steady beavers lose less.
pub fn rest_x(b: &Beaver, rest_x: f32) -> f32 {
    1.0 - (1.0 - rest_x) * (1.0 - STEADY_EASE * level(b, SkillTrait::Steady))
}

/// How fast it heals.
pub fn heal_x(b: &Beaver) -> f32 {
    1.0 + HEALING_GAIN * level(b, SkillTrait::Healing)
}

/// What a trait does at a beaver's level now, in words for its page: "+18% wolf sight".
pub fn effect_now(b: &Beaver, job: Job) -> String {
    let s = b.skills[job];
    let pct = |x: f32| format!("{:+.0}%", x * 100.0);
    match job.row().skill_trait {
        SkillTrait::Speed => format!("×{:.2} pace", crate::agents::skill_pace(s)),
        SkillTrait::HeavyLoads => format!("{} load slowdown", pct(-HEAVY_LOAD_EASE * s)),
        SkillTrait::Sight => format!("{} wolf sight", pct(SIGHT_GAIN * s)),
        SkillTrait::Steady => format!("{} tiredness on work", pct(-STEADY_EASE * s)),
        SkillTrait::Healing => format!("{} healing", pct(HEALING_GAIN * s)),
    }
}

/// What general experience does at a beaver's level now, each line a stat and its change.
pub fn experience_lines(b: &Beaver) -> [(&'static str, String); 5] {
    let pct = |x: f32| format!("{:+.0}%", x * 100.0);
    let e = b.experience;
    [
        ("Walking and swimming", pct(EXPERIENCE_MOVE * e)),
        ("Load carried", pct(EXPERIENCE_LOAD * e)),
        ("Tiredness", pct(-EXPERIENCE_TIRE * e)),
        ("Hunger", pct(-EXPERIENCE_HUNGER * e)),
        ("Harm from cold", pct(-EXPERIENCE_COLD * e)),
    ]
}

/// One stat a beaver's growth changes, all its sources together: the change as a fraction
/// (+0.12 is 12% more), whether less is better (tiredness, hunger), and where it comes from.
pub struct Bonus {
    pub stat: &'static str,
    pub change: f32,
    pub less_is_better: bool,
    pub from: &'static str,
}

/// Every bonus a beaver has from experience and its skills' traits, summed per stat, for its
/// page. Work pace at each job is the skill's own row, so it is not repeated here.
pub fn bonuses(b: &Beaver) -> [Bonus; 9] {
    let trait_from = |t: SkillTrait| Job::with_trait(t).map_or("", |j| j.row().skill.as_str());
    let bonus = |stat, change, less_is_better, from| Bonus { stat, change, less_is_better, from };
    [
        bonus("Walking and swimming", move_x(b) - 1.0, false, "Experience"),
        bonus("Load carried", load_x(b) - 1.0, false, "Experience"),
        bonus("Slowdown under a load", -HEAVY_LOAD_EASE * level(b, SkillTrait::HeavyLoads), true, trait_from(SkillTrait::HeavyLoads)),
        bonus("Tiredness", tire_x(b) - 1.0, true, "Experience"),
        bonus("Tiredness's drag on work", -STEADY_EASE * level(b, SkillTrait::Steady), true, trait_from(SkillTrait::Steady)),
        bonus("Hunger", hunger_x(b) - 1.0, true, "Experience"),
        bonus("Harm from cold", cold_x(b) - 1.0, true, "Experience"),
        bonus("Wolf sight", sight_x(b) - 1.0, false, trait_from(SkillTrait::Sight)),
        bonus("Healing", heal_x(b) - 1.0, false, trait_from(SkillTrait::Healing)),
    ]
}
