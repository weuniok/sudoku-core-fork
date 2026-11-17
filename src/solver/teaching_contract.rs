//! Versioned teaching wire records shared by search, proof adaptation, and the FFI.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeachingCandidate {
    pub cell: u8,
    pub digit: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeachingHouse {
    pub kind: String,
    pub index: u8,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeachingNode {
    pub id: u16,
    pub kind: String,
    pub members: Vec<TeachingCandidate>,
    pub polarity: String,
    pub meaning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeachingLink {
    pub from: u16,
    pub to: u16,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeachingGroup {
    pub cells: Vec<u8>,
    pub digits: Vec<u8>,
    pub house: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeachingEvidence {
    pub kind: String,
    pub nodes: Vec<TeachingNode>,
    pub links: Vec<TeachingLink>,
    pub groups: Vec<TeachingGroup>,
    pub rcc_digits: Vec<u8>,
    pub fins: Vec<TeachingCandidate>,
    pub group_roles: Vec<String>,
    pub relations: Vec<TeachingRelation>,
    pub uniqueness: Option<TeachingUniqueness>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeachingRelation {
    pub from: u16,
    pub to: u16,
    pub digit: u8,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeachingUniqueness {
    pub rule: String,
    pub floor_cells: Vec<u8>,
    pub roof_cells: Vec<u8>,
    pub digits: Vec<u8>,
    pub auxiliary_cells: Vec<u8>,
    pub conjugates: Vec<TeachingConjugate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeachingConjugate {
    pub house: u8,
    pub digit: u8,
    pub cells: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeachingProof {
    #[serde(flatten)]
    pub evidence: TeachingEvidence,
    pub branches: Vec<TeachingBranch>,
}
impl std::ops::Deref for TeachingProof {
    type Target = TeachingEvidence;
    fn deref(&self) -> &Self::Target {
        &self.evidence
    }
}
impl std::ops::DerefMut for TeachingProof {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.evidence
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeachingDeduction {
    pub technique_id: String,
    pub source_cells: Vec<u8>,
    pub candidates: Vec<TeachingCandidate>,
    pub placements: Vec<TeachingCandidate>,
    pub eliminations: Vec<TeachingCandidate>,
    pub houses: Vec<TeachingHouse>,
    pub proof: TeachingEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeachingBranch {
    pub assumption: TeachingCandidate,
    pub steps: Vec<TeachingDeduction>,
    pub contradiction: Option<TeachingContradiction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeachingContradiction {
    pub kind: String,
    pub cells: Vec<u8>,
    pub house: Option<u8>,
    pub digit: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeachingHint {
    pub schema_version: u32,
    pub technique_id: String,
    pub variant: String,
    pub position_key: String,
    pub key: String,
    pub source_cells: Vec<u8>,
    pub candidates: Vec<TeachingCandidate>,
    pub placements: Vec<TeachingCandidate>,
    pub eliminations: Vec<TeachingCandidate>,
    pub houses: Vec<TeachingHouse>,
    pub proof: TeachingProof,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeachingSearchResult {
    pub status: String,
    pub hints: Vec<TeachingHint>,
    pub work_used: u32,
}
