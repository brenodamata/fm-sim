//! Twenty universal attributes plus a five-attribute goalkeeping set.
//!
//! Groups are load-bearing, not cosmetic: knowledge accrues per group (ADR-010),
//! groups reveal at different rates, and decline curves are per group rather than
//! per player — which is why a keeper can still be excellent at thirty-eight
//! while a winger whose game was pace is finished at thirty-two.

/// Attribute values run 1..=20. Stored as `u8`; construction is clamped.
pub type Rating = u8;

pub const RATING_MIN: Rating = 1;
pub const RATING_MAX: Rating = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    Technical,
    Mental,
    Physical,
    Goalkeeping,
}

impl Group {
    pub const ALL: [Group; 4] = [
        Group::Technical,
        Group::Mental,
        Group::Physical,
        Group::Goalkeeping,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Group::Technical => "technical",
            Group::Mental => "mental",
            Group::Physical => "physical",
            Group::Goalkeeping => "goalkeeping",
        }
    }
}

/// The full attribute set. Order is frozen — it indexes [`Attributes`], and
/// changing it would silently reinterpret every stored player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Attribute {
    // Technical (7)
    Finishing,
    Passing,
    Crossing,
    FirstTouch,
    Dribbling,
    Tackling,
    Heading,
    // Mental (7)
    Decisions,
    OffTheBall,
    Positioning,
    Composure,
    Vision,
    WorkRate,
    Aggression,
    // Physical (6)
    Pace,
    Acceleration,
    Stamina,
    Strength,
    Agility,
    Jumping,
    // Goalkeeping (5)
    Handling,
    Reflexes,
    CommandOfArea,
    Distribution,
    OneOnOnes,
}

pub const ATTRIBUTE_COUNT: usize = 25;

impl Attribute {
    pub const ALL: [Attribute; ATTRIBUTE_COUNT] = [
        Attribute::Finishing,
        Attribute::Passing,
        Attribute::Crossing,
        Attribute::FirstTouch,
        Attribute::Dribbling,
        Attribute::Tackling,
        Attribute::Heading,
        Attribute::Decisions,
        Attribute::OffTheBall,
        Attribute::Positioning,
        Attribute::Composure,
        Attribute::Vision,
        Attribute::WorkRate,
        Attribute::Aggression,
        Attribute::Pace,
        Attribute::Acceleration,
        Attribute::Stamina,
        Attribute::Strength,
        Attribute::Agility,
        Attribute::Jumping,
        Attribute::Handling,
        Attribute::Reflexes,
        Attribute::CommandOfArea,
        Attribute::Distribution,
        Attribute::OneOnOnes,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn group(self) -> Group {
        match self {
            Attribute::Finishing
            | Attribute::Passing
            | Attribute::Crossing
            | Attribute::FirstTouch
            | Attribute::Dribbling
            | Attribute::Tackling
            | Attribute::Heading => Group::Technical,

            Attribute::Decisions
            | Attribute::OffTheBall
            | Attribute::Positioning
            | Attribute::Composure
            | Attribute::Vision
            | Attribute::WorkRate
            | Attribute::Aggression => Group::Mental,

            Attribute::Pace
            | Attribute::Acceleration
            | Attribute::Stamina
            | Attribute::Strength
            | Attribute::Agility
            | Attribute::Jumping => Group::Physical,

            Attribute::Handling
            | Attribute::Reflexes
            | Attribute::CommandOfArea
            | Attribute::Distribution
            | Attribute::OneOnOnes => Group::Goalkeeping,
        }
    }

    pub fn is_goalkeeping(self) -> bool {
        self.group() == Group::Goalkeeping
    }

    pub fn name(self) -> &'static str {
        match self {
            Attribute::Finishing => "finishing",
            Attribute::Passing => "passing",
            Attribute::Crossing => "crossing",
            Attribute::FirstTouch => "first_touch",
            Attribute::Dribbling => "dribbling",
            Attribute::Tackling => "tackling",
            Attribute::Heading => "heading",
            Attribute::Decisions => "decisions",
            Attribute::OffTheBall => "off_the_ball",
            Attribute::Positioning => "positioning",
            Attribute::Composure => "composure",
            Attribute::Vision => "vision",
            Attribute::WorkRate => "work_rate",
            Attribute::Aggression => "aggression",
            Attribute::Pace => "pace",
            Attribute::Acceleration => "acceleration",
            Attribute::Stamina => "stamina",
            Attribute::Strength => "strength",
            Attribute::Agility => "agility",
            Attribute::Jumping => "jumping",
            Attribute::Handling => "handling",
            Attribute::Reflexes => "reflexes",
            Attribute::CommandOfArea => "command_of_area",
            Attribute::Distribution => "distribution",
            Attribute::OneOnOnes => "one_on_ones",
        }
    }
}

/// A player's attributes.
///
/// A fixed array rather than a map — deliberately. Map iteration order would
/// leak into any draw that walks attributes, which breaks the determinism
/// contract in a way that is very hard to find later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attributes {
    values: [Rating; ATTRIBUTE_COUNT],
}

impl Attributes {
    pub fn uniform(value: Rating) -> Self {
        Attributes {
            values: [clamp(value); ATTRIBUTE_COUNT],
        }
    }

    pub fn get(&self, attr: Attribute) -> Rating {
        self.values[attr.index()]
    }

    pub fn set(&mut self, attr: Attribute, value: Rating) {
        self.values[attr.index()] = clamp(value);
    }

    /// Iterates in declaration order, which is frozen — safe to draw from.
    pub fn iter(&self) -> impl Iterator<Item = (Attribute, Rating)> + '_ {
        Attribute::ALL.iter().map(move |&a| (a, self.get(a)))
    }

    /// Mean of one group, rounded. Used for coarse displays and for the
    /// club-strength summaries the harness reports on.
    pub fn group_mean(&self, group: Group) -> f32 {
        let vals: Vec<Rating> = Attribute::ALL
            .iter()
            .filter(|a| a.group() == group)
            .map(|&a| self.get(a))
            .collect();
        if vals.is_empty() {
            return 0.0;
        }
        vals.iter().map(|&v| v as f32).sum::<f32>() / vals.len() as f32
    }

    /// Mean across the outfield groups only. A crude overall used by the M0
    /// generator and harness; the match engine never uses it.
    pub fn outfield_mean(&self) -> f32 {
        let vals: Vec<Rating> = Attribute::ALL
            .iter()
            .filter(|a| !a.is_goalkeeping())
            .map(|&a| self.get(a))
            .collect();
        vals.iter().map(|&v| v as f32).sum::<f32>() / vals.len() as f32
    }
}

impl Default for Attributes {
    fn default() -> Self {
        Attributes::uniform(RATING_MIN)
    }
}

pub fn clamp(value: Rating) -> Rating {
    value.clamp(RATING_MIN, RATING_MAX)
}

/// Clamp from a float, for generation and development arithmetic.
pub fn clamp_f32(value: f32) -> Rating {
    if value.is_nan() {
        return RATING_MIN;
    }
    let rounded = value.round();
    if rounded <= RATING_MIN as f32 {
        RATING_MIN
    } else if rounded >= RATING_MAX as f32 {
        RATING_MAX
    } else {
        rounded as Rating
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_attribute_has_exactly_one_group() {
        let mut counts = [0usize; 4];
        for a in Attribute::ALL {
            let idx = Group::ALL.iter().position(|&g| g == a.group()).unwrap();
            counts[idx] += 1;
        }
        assert_eq!(counts, [7, 7, 6, 5], "group sizes drifted from the design");
    }

    #[test]
    fn all_array_matches_declaration_order() {
        for (i, a) in Attribute::ALL.iter().enumerate() {
            assert_eq!(a.index(), i, "{} is out of order", a.name());
        }
    }

    #[test]
    fn attribute_names_are_unique() {
        let mut names: Vec<&str> = Attribute::ALL.iter().map(|a| a.name()).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "duplicate attribute name");
    }

    #[test]
    fn ratings_are_clamped() {
        let mut a = Attributes::uniform(10);
        a.set(Attribute::Pace, 200);
        assert_eq!(a.get(Attribute::Pace), RATING_MAX);
        a.set(Attribute::Pace, 0);
        assert_eq!(a.get(Attribute::Pace), RATING_MIN);
    }

    #[test]
    fn clamp_f32_handles_edges() {
        assert_eq!(clamp_f32(f32::NAN), RATING_MIN);
        assert_eq!(clamp_f32(-5.0), RATING_MIN);
        assert_eq!(clamp_f32(99.0), RATING_MAX);
        assert_eq!(clamp_f32(12.4), 12);
        assert_eq!(clamp_f32(12.6), 13);
    }
}
