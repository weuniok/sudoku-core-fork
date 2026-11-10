//! Sudoku Core Engine
//!
//! A cross-platform Sudoku engine supporting puzzle generation, solving, hints,
//! and variant rules (X-Sudoku, Killer Sudoku, etc.)

mod bitset;
mod cell;
mod constraint;
mod diversity;
mod generator;
mod grid;
mod hash;
mod position;
mod puzzle_id;
mod solver;

pub use bitset::BitSet;
pub use cell::Cell;
pub use constraint::{
    BoxConstraint, ColumnConstraint, Constraint, ConstraintBox, DiagonalConstraint,
    KillerCageConstraint, RowConstraint, ThermoConstraint,
};
pub use diversity::{DifficultyStats, DiversityAnalyzer, DiversityReport, TheoreticalEstimates};
pub use generator::{Generator, GeneratorConfig, SymmetryType};
pub use grid::{Grid, MoveError, ValidationResult};
pub use hash::{canonical_puzzle_hash, canonical_puzzle_hash_str};
pub use position::Position;
pub use puzzle_id::PuzzleId;
pub use solver::{
    search_teaching_hint, search_teaching_hint_with_options, AlsProofDescriptor, ArithmeticCheck,
    ArithmeticProof, ArithmeticReplay, ArithmeticRequirement, ArithmeticSearchOptions,
    ArithmeticSearchResult, ArithmeticState, ArithmeticTerm, ArithmeticTerminal, Difficulty,
    ForcingSource, Hint, HintType, LinkType, Polarity, ProofCertificate, Solver, TeachingBranch,
    TeachingCandidate, TeachingConjugate, TeachingContradiction, TeachingDeduction,
    TeachingEvidence, TeachingGroup, TeachingHint, TeachingHouse, TeachingLink, TeachingNode,
    TeachingProof, TeachingRelation, TeachingSearchResult, TeachingUniqueness, Technique,
    TEACHING_TECHNIQUES,
};
