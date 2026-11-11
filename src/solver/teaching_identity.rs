//! Persisted teaching identity v1. Field labels, order, delimiters, and the
//! two proof layouts are frozen to preserve keys already saved by the app.
//! Keep this encoding independent of the teaching wire format.

use std::fmt::Write;

use super::teaching_contract::{
    TeachingBranch, TeachingCandidate, TeachingConjugate, TeachingContradiction, TeachingDeduction,
    TeachingEvidence, TeachingGroup, TeachingHouse, TeachingLink, TeachingNode, TeachingRelation,
    TeachingUniqueness,
};
use sha2::{Digest, Sha256};

pub(super) fn hint_key(
    position: &str,
    deduction: &TeachingDeduction,
    branches: &[TeachingBranch],
) -> String {
    let technique = &deduction.technique_id;
    let mut input = String::new();
    deduction.source_cells.encode(&mut input);
    input.push(':');
    deduction.candidates.encode(&mut input);
    input.push(':');
    deduction.placements.encode(&mut input);
    input.push(':');
    deduction.eliminations.encode(&mut input);
    input.push(':');
    let evidence = &deduction.proof;
    if matches!(
        technique.as_str(),
        "naked_single"
            | "hidden_single"
            | "locked_candidates"
            | "naked_pair"
            | "x_wing"
            | "xy_wing"
            | "wxyz_wing"
            | "aic"
    ) {
        record(
            &mut input,
            "TeachingProof",
            &[
                ("kind", &evidence.kind),
                ("nodes", &evidence.nodes),
                ("links", &evidence.links),
                ("groups", &evidence.groups),
                ("rcc_digits", &evidence.rcc_digits),
            ],
        );
    } else {
        input.push_str("TeachingProof { evidence: ");
        evidence.encode(&mut input);
        input.push_str(", branches: [] }");
    }
    let key = format!("{position}:{technique}:{:x}", Sha256::digest(input));
    if branches.is_empty() {
        return key;
    }

    input = key;
    input.push(':');
    branches.encode(&mut input);
    format!("{position}:{technique}:{:x}", Sha256::digest(input))
}

trait EncodeV1 {
    fn encode(&self, out: &mut String);
}

impl EncodeV1 for u8 {
    fn encode(&self, out: &mut String) {
        write!(out, "{self}").unwrap();
    }
}

impl EncodeV1 for u16 {
    fn encode(&self, out: &mut String) {
        write!(out, "{self}").unwrap();
    }
}

impl EncodeV1 for String {
    fn encode(&self, out: &mut String) {
        out.push('"');
        for character in self.chars() {
            match character {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '\0' => out.push_str("\\0"),
                other if other.is_control() => {
                    write!(out, "\\u{{{:x}}}", u32::from(other)).unwrap();
                }
                other => out.push(other),
            }
        }
        out.push('"');
    }
}

impl<T: EncodeV1> EncodeV1 for [T] {
    fn encode(&self, out: &mut String) {
        out.push('[');
        for (index, value) in self.iter().enumerate() {
            if index != 0 {
                out.push_str(", ");
            }
            value.encode(out);
        }
        out.push(']');
    }
}

impl<T: EncodeV1> EncodeV1 for Vec<T> {
    fn encode(&self, out: &mut String) {
        self.as_slice().encode(out);
    }
}

impl<T: EncodeV1> EncodeV1 for Option<T> {
    fn encode(&self, out: &mut String) {
        match self {
            Some(value) => {
                out.push_str("Some(");
                value.encode(out);
                out.push(')');
            }
            None => out.push_str("None"),
        }
    }
}

fn record(out: &mut String, label: &str, fields: &[(&str, &dyn EncodeV1)]) {
    out.push_str(label);
    out.push_str(" { ");
    for (index, (label, value)) in fields.iter().enumerate() {
        if index != 0 {
            out.push_str(", ");
        }
        out.push_str(label);
        out.push_str(": ");
        value.encode(out);
    }
    out.push_str(" }");
}

// The explicit labels are identity data even if a Rust type or field is renamed.
macro_rules! encode_record {
    ($record:ty, $label:literal, $($key:literal => $field:ident),+ $(,)?) => {
        impl EncodeV1 for $record {
            fn encode(&self, out: &mut String) {
                record(out, $label, &[$(($key, &self.$field)),+]);
            }
        }
    };
}

encode_record!(TeachingCandidate, "TeachingCandidate", "cell" => cell, "digit" => digit);
encode_record!(TeachingHouse, "TeachingHouse", "kind" => kind, "index" => index, "role" => role);
encode_record!(
    TeachingNode, "TeachingNode",
    "id" => id, "kind" => kind, "members" => members, "polarity" => polarity, "meaning" => meaning,
);
encode_record!(TeachingLink, "TeachingLink", "from" => from, "to" => to, "kind" => kind);
encode_record!(TeachingGroup, "TeachingGroup", "cells" => cells, "digits" => digits, "house" => house);
encode_record!(
    TeachingEvidence, "TeachingEvidence",
    "kind" => kind, "nodes" => nodes, "links" => links, "groups" => groups,
    "rcc_digits" => rcc_digits, "fins" => fins, "group_roles" => group_roles,
    "relations" => relations, "uniqueness" => uniqueness,
);
encode_record!(
    TeachingRelation, "TeachingRelation", "from" => from, "to" => to, "digit" => digit, "kind" => kind,
);
encode_record!(
    TeachingUniqueness, "TeachingUniqueness",
    "rule" => rule, "floor_cells" => floor_cells, "roof_cells" => roof_cells,
    "digits" => digits, "auxiliary_cells" => auxiliary_cells, "conjugates" => conjugates,
);
encode_record!(
    TeachingConjugate, "TeachingConjugate", "house" => house, "digit" => digit, "cells" => cells,
);
encode_record!(
    TeachingDeduction, "TeachingDeduction",
    "technique_id" => technique_id, "source_cells" => source_cells, "candidates" => candidates,
    "placements" => placements, "eliminations" => eliminations, "houses" => houses, "proof" => proof,
);
encode_record!(
    TeachingBranch, "TeachingBranch",
    "assumption" => assumption, "steps" => steps, "contradiction" => contradiction,
);
encode_record!(
    TeachingContradiction, "TeachingContradiction",
    "kind" => kind, "cells" => cells, "house" => house, "digit" => digit,
);
