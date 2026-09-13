//! Players, positions, and the values the manager never sees as numbers.

use super::attributes::{Attribute, Attributes, Group, Rating};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlayerId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClubId(pub u32);

/// Position families, in the Brazilian shape rather than the English one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Position {
    Goleiro,
    Zagueiro,
    Lateral,
    Volante,
    Meia,
    Ponta,
    Centroavante,
}

impl Position {
    pub const ALL: [Position; 7] = [
        Position::Goleiro,
        Position::Zagueiro,
        Position::Lateral,
        Position::Volante,
        Position::Meia,
        Position::Ponta,
        Position::Centroavante,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Position::Goleiro => "goleiro",
            Position::Zagueiro => "zagueiro",
            Position::Lateral => "lateral",
            Position::Volante => "volante",
            Position::Meia => "meia",
            Position::Ponta => "ponta",
            Position::Centroavante => "centroavante",
        }
    }

    pub fn is_keeper(self) -> bool {
        matches!(self, Position::Goleiro)
    }

    /// How many of this position a squad wants, as a share. Used by the
    /// generator; the numbers are deliberately boring and live here rather than
    /// in tuning because squad shape is structural, not a balance lever.
    pub fn squad_share(self) -> f32 {
        match self {
            Position::Goleiro => 0.10,
            Position::Zagueiro => 0.18,
            Position::Lateral => 0.14,
            Position::Volante => 0.16,
            Position::Meia => 0.16,
            Position::Ponta => 0.14,
            Position::Centroavante => 0.12,
        }
    }

    /// Weight applied to each attribute when generating a player of this
    /// position: 1.0 means "central to the job", lower means incidental.
    ///
    /// This is generation only. The match engine weights attributes by *role*
    /// (M1b), not by position — a role is a point in the five-axis space, and
    /// two players in the same position can weight completely differently.
    pub fn generation_weight(self, attr: Attribute) -> f32 {
        use Attribute::*;
        if self.is_keeper() {
            return if attr.is_goalkeeping() {
                1.0
            } else {
                match attr {
                    Composure | Decisions | Positioning | Jumping | Agility => 0.55,
                    _ => 0.25,
                }
            };
        }
        if attr.is_goalkeeping() {
            return 0.0; // outfielders have no keeping ability worth modelling
        }
        match (self, attr) {
            (Position::Zagueiro, Tackling | Heading | Strength | Positioning) => 1.0,
            (Position::Zagueiro, Jumping | Decisions | Aggression) => 0.85,
            (Position::Zagueiro, Finishing | Crossing | Dribbling) => 0.3,

            (Position::Lateral, Stamina | Crossing | Pace | Acceleration) => 1.0,
            (Position::Lateral, Tackling | WorkRate | Positioning) => 0.8,
            (Position::Lateral, Finishing | Heading) => 0.35,

            (Position::Volante, Tackling | Positioning | WorkRate | Decisions) => 1.0,
            (Position::Volante, Passing | Stamina | Aggression | Strength) => 0.85,
            (Position::Volante, Finishing | Crossing) => 0.4,

            (Position::Meia, Passing | Vision | FirstTouch | Decisions) => 1.0,
            (Position::Meia, Dribbling | Composure | OffTheBall) => 0.85,
            (Position::Meia, Tackling | Heading | Strength) => 0.4,

            (Position::Ponta, Dribbling | Pace | Acceleration | Crossing) => 1.0,
            (Position::Ponta, Agility | FirstTouch | OffTheBall) => 0.85,
            (Position::Ponta, Tackling | Heading | Strength) => 0.35,

            (Position::Centroavante, Finishing | OffTheBall | Composure) => 1.0,
            (Position::Centroavante, Heading | Strength | FirstTouch) => 0.85,
            (Position::Centroavante, Tackling | Crossing) => 0.3,

            _ => 0.6,
        }
    }
}

/// Values the manager never sees as a number.
///
/// Stored as truth. Nothing above the engine may serialise these directly — the
/// application service is responsible for turning them into estimates or
/// omitting them entirely (ADR-010). Keeping them in one struct makes that
/// boundary auditable: if a DTO contains a `Hidden`, the filter was skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hidden {
    /// Soft target, not a ceiling. Growth is pulled toward it and can overshoot
    /// or fall short, so two players with identical potential finish apart.
    pub potential: Rating,
    /// Scales physical decline only. High longevity is why a keeper can be
    /// excellent at thirty-eight.
    pub longevity: Rating,
    /// Professionalism, ambition, resilience, greed. Expressed through the
    /// drain — negotiation, agitation, reaction to a broken promise — not
    /// through development.
    pub temperament: Temperament,
    /// Innate and fixed. Estimable by medical staff before signing, sharpened by
    /// injury history, never shown.
    pub proneness: Rating,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Temperament {
    pub professionalism: Rating,
    pub ambition: Rating,
    pub resilience: Rating,
    pub greed: Rating,
    pub loyalty: Rating,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub birth_year: i32,
    pub position: Position,
    pub attributes: Attributes,
    pub hidden: Hidden,
}

impl Player {
    pub fn age(&self, current_year: i32) -> i32 {
        current_year - self.birth_year
    }

    /// A crude single number for generation and harness reporting only.
    ///
    /// Deliberately *not* used by the match engine, which resolves contested
    /// checks between named players on role-weighted attributes. Any use of this
    /// in simulation logic is a bug.
    pub fn coarse_ability(&self) -> f32 {
        if self.position.is_keeper() {
            self.attributes.group_mean(Group::Goalkeeping) * 0.7
                + self.attributes.group_mean(Group::Mental) * 0.2
                + self.attributes.group_mean(Group::Physical) * 0.1
        } else {
            let mut total = 0.0;
            let mut weight = 0.0;
            for (attr, value) in self.attributes.iter() {
                let w = self.position.generation_weight(attr);
                if w > 0.0 {
                    total += value as f32 * w;
                    weight += w;
                }
            }
            if weight == 0.0 {
                0.0
            } else {
                total / weight
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squad_shares_sum_to_one() {
        let total: f32 = Position::ALL.iter().map(|p| p.squad_share()).sum();
        assert!((total - 1.0).abs() < 1e-5, "shares sum to {total}");
    }

    #[test]
    fn outfielders_have_no_keeping_weight() {
        for pos in Position::ALL.iter().filter(|p| !p.is_keeper()) {
            for attr in Attribute::ALL.iter().filter(|a| a.is_goalkeeping()) {
                assert_eq!(
                    pos.generation_weight(*attr),
                    0.0,
                    "{} should not weight {}",
                    pos.name(),
                    attr.name()
                );
            }
        }
    }

    #[test]
    fn keepers_weight_keeping_attributes_fully() {
        for attr in Attribute::ALL.iter().filter(|a| a.is_goalkeeping()) {
            assert_eq!(Position::Goleiro.generation_weight(*attr), 1.0);
        }
    }

    #[test]
    fn position_names_are_unique() {
        let mut names: Vec<&str> = Position::ALL.iter().map(|p| p.name()).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len());
    }
}
