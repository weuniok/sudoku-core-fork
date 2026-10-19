//! Finding -> Hint conversion and explanation generation.
//!
//! Engines return `Finding` structs. This module converts them to `Hint`
//! with human-readable explanation strings.

use super::arithmetic::ArithmeticProof;
use super::fabric::idx_to_pos;
use super::types::{Hint, HintType, Technique};
use crate::Position;

/// What the engine found: either place a value or eliminate candidates.
#[derive(Debug, Clone)]
pub enum InferenceResult {
    /// Place a value in a cell
    Placement { cell: usize, value: u8 },
    /// Eliminate candidates from a cell
    Elimination { cell: usize, values: Vec<u8> },
}

// ==================== Proof Certificates ====================
//
// Each deduction carries a proof certificate identifying which of the
// four grid spaces (Cell, Candidate, Sector, Link) it operates in and
// the structural evidence that justifies the inference.

/// Polarity in the AIC bipartite graph: candidate is ON (true) or OFF (false).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polarity {
    On,
    Off,
}

/// Link type in an alternating inference chain.
///
/// Community terminology (StrmCkr / Players Forum):
/// - `Strong` = structural link (XOR): exactly one of the connected nodes is true.
/// - `WeakInference` = weak inference (NAND): at most one of the connected nodes is true.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkType {
    /// Strong link (XOR): exactly one of the connected nodes is true.
    Strong,
    /// Weak inference (NAND): at most one of the connected nodes is true.
    WeakInference,
}

/// Source of a forcing chain deduction.
#[derive(Debug, Clone)]
pub enum ForcingSource {
    /// All candidates of a cell lead to the same conclusion.
    Cell(usize),
    /// All positions of a digit in a sector lead to the same conclusion.
    Region { sector: usize, digit: u8 },
    /// A single candidate assumption leads to contradiction (Nishio).
    Nishio { cell: usize, digit: u8 },
}

/// Descriptor for an ALS (Almost Locked Set) in a proof certificate.
#[derive(Debug, Clone)]
pub struct AlsProofDescriptor {
    /// Linear cell indices in this ALS.
    pub cells: Vec<usize>,
    /// Candidate digits in this ALS (N+1 values for N cells).
    pub candidates: Vec<u8>,
    /// Sector this ALS was found in.
    pub sector: usize,
}

/// Proof certificate: structural evidence justifying a deduction.
///
/// Each variant records the evidence supplied by an engine:
///
/// - **Basic**: Direct constraint propagation in Cell×Candidate space.
/// - **Fish**: Rank deficiency in the Sector×Candidate incidence matrix.
/// - **Als**: Degree-of-freedom chain in the ALS subset graph.
/// - **Aic**: Path in the bipartite ON/OFF polarity graph (Link space).
/// - **Arithmetic**: A checked integer combination of native requirements,
///   with an interval or divisibility contradiction for the opposite candidate.
/// - **Uniqueness**: Relies on the meta-constraint that the puzzle has one solution.
/// - **Forcing**: Multi-branch propagation proving a common conclusion.
/// - **Backtracking**: No human technique found; trial-and-error.
#[derive(Debug, Clone)]
pub enum ProofCertificate {
    /// Direct deduction from cell/sector constraints (singles, locked sets).
    Basic { kind: &'static str },
    /// Rank deficiency in the sector-candidate incidence matrix.
    Fish {
        digit: u8,
        base_sectors: Vec<usize>,
        cover_sectors: Vec<usize>,
        fins: Vec<usize>,
    },
    /// ALS chain: sequence of Almost Locked Sets linked by RCC values.
    Als {
        als_chain: Vec<AlsProofDescriptor>,
        rcc_values: Vec<u8>,
        z_value: Option<u8>,
    },
    /// Alternating inference chain in the bipartite polarity graph.
    Aic {
        /// Chain nodes: (cell, digit, polarity).
        chain: Vec<(usize, u8, Polarity)>,
        /// Link types between consecutive nodes.
        link_types: Vec<LinkType>,
    },
    /// Recomputable arithmetic certificate bound to the exact candidate state.
    Arithmetic(ArithmeticProof),
    /// Uniqueness assumption: the puzzle has exactly one solution.
    Uniqueness {
        pattern: String,
        /// Floor cells (cells with exactly 2 candidates in the UR).
        floor_cells: Vec<usize>,
        /// Roof cells (cells with extra candidates).
        roof_cells: Vec<usize>,
    },
    /// Forcing: all branches from a source lead to the same conclusion.
    Forcing {
        source: ForcingSource,
        branches: usize,
    },
    /// Backtracking: no human technique found.
    Backtracking,
}

/// Engine-specific explanation data for generating human-readable strings.
#[derive(Debug, Clone)]
pub enum ExplanationData {
    /// Naked single: cell has only one candidate
    NakedSingle { cell: usize, value: u8 },
    /// Hidden single: digit can only go in one cell in a sector
    HiddenSingle {
        cell: usize,
        value: u8,
        sector: usize,
        sector_name: String,
    },
    /// Locked set (naked/hidden pair/triple/quad)
    LockedSet {
        sector: usize,
        kind: &'static str, // "Naked" or "Hidden"
        size: usize,
        #[allow(dead_code)]
        cells: Vec<usize>,
        #[allow(dead_code)]
        values: Vec<u8>,
        sector_name: String,
    },
    /// Intersection (pointing pair / box-line reduction)
    Intersection {
        kind: &'static str,
        digit: u8,
        from_sector: String,
        to_sector: String,
    },
    /// Fish pattern (X-Wing through Mutant Fish)
    Fish {
        size: usize,
        digit: u8,
        base_sectors: Vec<String>,
        cover_sectors: Vec<String>,
        fins: Vec<usize>,
        variant: String, // "Basic", "Finned", "Franken", "Siamese", "Mutant"
    },
    /// ALS-based pattern
    Als {
        variant: String,
        chain_length: usize,
        shared_value: Option<u8>,
    },
    /// AIC/chain pattern
    Chain {
        variant: String,
        chain_length: usize,
        #[allow(dead_code)]
        values: Vec<u8>,
    },
    /// Uniqueness-based pattern
    Uniqueness { variant: String },
    /// Forcing chain
    ForcingChain { variant: String, source_cell: usize },
    /// Backtracking (last resort)
    Backtracking { cell: usize, value: u8 },
    /// Explanation rendered by the engine.
    Raw(String),
}

/// A finding from an engine, ready to be converted to a Hint.
#[derive(Debug, Clone)]
pub struct Finding {
    pub technique: Technique,
    pub inference: InferenceResult,
    pub involved_cells: Vec<usize>,
    pub explanation: ExplanationData,
    /// Structural proof certificate identifying the grid space and evidence.
    pub proof: Option<ProofCertificate>,
}

impl Finding {
    /// Convert this Finding into a public Hint.
    pub fn to_hint(&self) -> Hint {
        let hint_type = match &self.inference {
            InferenceResult::Placement { cell, value } => HintType::SetValue {
                pos: idx_to_pos(*cell),
                value: *value,
            },
            InferenceResult::Elimination { cell, values } => HintType::EliminateCandidates {
                pos: idx_to_pos(*cell),
                values: values.clone(),
            },
        };

        let involved_cells: Vec<Position> = self
            .involved_cells
            .iter()
            .map(|&idx| idx_to_pos(idx))
            .collect();

        let explanation = self.render_explanation();

        Hint {
            technique: self.technique,
            hint_type,
            explanation,
            involved_cells,
            proof: self.proof.clone(),
        }
    }

    fn render_explanation(&self) -> String {
        match &self.explanation {
            ExplanationData::NakedSingle { cell, value } => {
                let pos = idx_to_pos(*cell);
                format!(
                    "Cell ({}, {}) can only be {} - it's the only candidate left.",
                    pos.row + 1,
                    pos.col + 1,
                    value
                )
            }
            ExplanationData::HiddenSingle {
                cell,
                value,
                sector_name,
                ..
            } => {
                let pos = idx_to_pos(*cell);
                format!(
                    "{} can only go in cell ({}, {}) in {}.",
                    value,
                    pos.row + 1,
                    pos.col + 1,
                    sector_name
                )
            }
            ExplanationData::LockedSet {
                kind,
                size,
                cells: _,
                values,
                sector_name,
                ..
            } => {
                let size_name = match size {
                    2 => "Pair",
                    3 => "Triple",
                    4 => "Quad",
                    _ => "Set",
                };
                format!("{} {} on {:?} in {}.", kind, size_name, values, sector_name)
            }
            ExplanationData::Intersection {
                kind,
                digit,
                from_sector,
                to_sector,
            } => {
                format!(
                    "{}: in {}, {} is confined to {}, eliminating from rest of {}.",
                    kind, from_sector, digit, to_sector, to_sector
                )
            }
            ExplanationData::Fish {
                size,
                digit,
                base_sectors,
                cover_sectors,
                fins,
                variant,
            } => {
                let name = match (*size, variant.as_str()) {
                    (2, "Basic") => "X-Wing",
                    (3, "Basic") => "Swordfish",
                    (4, "Basic") => "Jellyfish",
                    (2, "Finned") => "Finned X-Wing",
                    (3, "Finned") => "Finned Swordfish",
                    (4, "Finned") => "Finned Jellyfish",
                    (_, "Franken") => "Franken Fish",
                    (_, "Siamese") => "Siamese Fish",
                    (_, "Mutant") => "Mutant Fish",
                    _ => "Fish",
                };
                if fins.is_empty() {
                    format!(
                        "{} on {} in bases {:?}, covers {:?}.",
                        name, digit, base_sectors, cover_sectors
                    )
                } else {
                    format!(
                        "{} on {} in bases {:?}, covers {:?} (fins present).",
                        name, digit, base_sectors, cover_sectors
                    )
                }
            }
            ExplanationData::Als {
                variant,
                chain_length,
                shared_value,
            } => {
                if let Some(z) = shared_value {
                    format!(
                        "{}: chain of {} ALS linked by shared value {}.",
                        variant, chain_length, z
                    )
                } else {
                    format!("{}: chain of {} ALS.", variant, chain_length)
                }
            }
            ExplanationData::Chain {
                variant,
                chain_length,
                values: _,
            } => {
                format!("{}: chain of length {}.", variant, chain_length)
            }
            ExplanationData::Uniqueness { variant } => {
                format!("{} found.", variant)
            }
            ExplanationData::ForcingChain {
                variant,
                source_cell,
            } => {
                let pos = idx_to_pos(*source_cell);
                format!(
                    "{}: all candidates of ({}, {}) lead to same conclusion.",
                    variant,
                    pos.row + 1,
                    pos.col + 1
                )
            }
            ExplanationData::Backtracking { cell, value } => {
                let pos = idx_to_pos(*cell);
                format!(
                    "The cell at ({}, {}) must be {}.",
                    pos.row + 1,
                    pos.col + 1,
                    value
                )
            }
            ExplanationData::Raw(s) => s.clone(),
        }
    }
}
