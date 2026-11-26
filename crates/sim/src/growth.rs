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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SimState;

    struct DryLand;
    impl crate::colony::Landscape for DryLand {
        fn is_water(&self, _: f32, _: f32) -> bool {
            false
        }
    }

    fn founder() -> Beaver {
        let mut sim = SimState::new(1);
        sim.colony.found((10.0, 10.0), &DryLand).expect("found");
        let mut b = sim.colony.beavers[0].clone();
        b.experience = 0.0;
        for s in b.skills.values_mut() {
            *s = 0.0;
        }
        b
    }

    #[test]
    fn a_novice_is_an_ordinary_beaver() {
        let b = founder();
        assert_eq!((move_x(&b), load_x(&b), tire_x(&b), hunger_x(&b), cold_x(&b)), (1.0, 1.0, 1.0, 1.0, 1.0));
        assert_eq!((sight_x(&b), heal_x(&b), load_pace(&b, 0.5), rest_x(&b, 0.6)), (1.0, 1.0, 0.5, 0.6));
    }

    #[test]
    fn full_experience_is_a_fifth_better_at_everything() {
        let mut b = founder();
        b.experience = 1.0;
        for (x, want) in [(move_x(&b), 1.2), (load_x(&b), 1.2), (tire_x(&b), 0.8), (hunger_x(&b), 0.8), (cold_x(&b), 0.8)] {
            assert!((x - want).abs() < 1e-6, "{x} != {want}");
        }
    }

    #[test]
    fn experience_grows_from_any_work_and_never_passes_full() {
        let mut b = founder();
        learn(&mut b, 10.0);
        let once = b.experience;
        assert!(once > 0.0);
        for _ in 0..100_000 {
            learn(&mut b, 10.0);
        }
        assert!(b.experience > once && b.experience <= 1.0);
    }

    #[test]
    fn each_trait_comes_from_its_own_skill() {
        let mut b = founder();
        b.skills[Job::of("Hauler")] = 1.0;
        assert!(load_pace(&b, 0.5) > 0.5 && sight_x(&b) == 1.0);
        b.skills[Job::of("Sentry")] = 1.0;
        assert!(sight_x(&b) > 1.0);
        b.skills[Job::of("Builder")] = 1.0;
        assert!(rest_x(&b, 0.6) > 0.6);
        b.skills[Job::of("Planter")] = 1.0;
        assert!(heal_x(&b) > 1.0);
    }

    #[test]
    fn experience_grows_at_a_quarter_of_a_skills_pace() {
        // The same work that teaches a job's skill teaches general experience, four times slower.
        let mut b = founder();
        learn(&mut b, 1.0);
        let skill_gain = crate::colony::SKILL_GAIN;
        assert!((b.experience * 4.0 - skill_gain).abs() < 1e-7, "{} vs {skill_gain}", b.experience);
    }

    #[test]
    fn bonuses_sum_each_source_and_are_zero_for_a_novice() {
        let mut b = founder();
        assert!(bonuses(&b).iter().all(|x| x.change == 0.0));
        b.experience = 1.0;
        b.skills[Job::of("Sentry")] = 1.0;
        let all = bonuses(&b);
        let of = |stat: &str| all.iter().find(|x| x.stat == stat).expect("stat").change;
        assert!((of("Walking and swimming") - 0.2).abs() < 1e-6);
        assert!((of("Hunger") + 0.2).abs() < 1e-6);
        assert!((of("Wolf sight") - (sight_x(&b) - 1.0)).abs() < 1e-6 && of("Wolf sight") > 0.0);
        assert_eq!(of("Healing"), 0.0);
        // Every non-zero bonus helps: up where more is better, down where less is.
        assert!(all.iter().all(|x| x.change == 0.0 || (x.change < 0.0) == x.less_is_better));
    }

    #[test]
    fn only_speed_skills_change_work_pace() {
        let mut b = founder();
        for s in b.skills.values_mut() {
            *s = 1.0;
        }
        for j in Job::all() {
            let x = crate::agents::work_factors(&b, j).skill_x;
            assert_eq!(x > 1.0, j.skill_speeds(), "{}", j.row().key);
        }
    }
}
