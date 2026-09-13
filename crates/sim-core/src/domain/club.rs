//! Clubs and the competition structure they sit in.

use super::player::{ClubId, PlayerId};

/// Where a club sits in the pyramid. Division 0 is the top flight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DivisionId(pub u8);

#[derive(Debug, Clone, PartialEq)]
pub struct Club {
    pub id: ClubId,
    pub name: String,
    pub division: DivisionId,
    /// Club reputation, 1..=20. Slow-moving and largely inherited; drives
    /// sponsorship, youth stream quality, and who will consider signing.
    /// Distinct from manager reputation, which travels with the manager.
    pub reputation: u8,
    /// Squad in a stable order. A `Vec` rather than a set, deliberately: any
    /// iteration that feeds a draw must be order-stable (ADR-005).
    pub squad: Vec<PlayerId>,
}

impl Club {
    pub fn squad_size(&self) -> usize {
        self.squad.len()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Division {
    pub id: DivisionId,
    pub name: String,
    pub clubs: Vec<ClubId>,
}
