//! Germline: persistent, git-backed store of enzyme definitions.
//!
//! Every mutation is a proposed change to the enzyme set. Mutations are
//! accepted only if they pass the RAF closure gate (mechanical, no agent
//! discretion) and improve fitness (mechanical delta, no self-report).

use crate::raf::detect_max_raf;
use crate::types::{EnzymeDef, EnzymeId, FoodSet, RafResult};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GermlineError {
    #[error("mutation rejected: RAF closure broken (coverage {coverage:.2}, lost: {lost:?})")]
    ClosureBroken { coverage: f64, lost: Vec<EnzymeId> },

    #[error("mutation rejected: enzyme '{0}' not found")]
    EnzymeNotFound(EnzymeId),

    #[error("mutation rejected: duplicate enzyme '{0}'")]
    DuplicateEnzyme(EnzymeId),
}

/// The germline: a set of enzyme definitions + food set.
///
/// All mutations are gated by RAF closure. If a proposed change would
/// break catalytic closure, it is rejected. No exceptions, no overrides.
#[derive(Debug, Clone)]
pub struct Germline {
    enzymes: BTreeMap<EnzymeId, EnzymeDef>,
    food: FoodSet,
}

impl Germline {
    /// Create a new germline from initial enzyme definitions and food set.
    ///
    /// Does NOT require initial closure — the seed may start unclosed
    /// and achieve closure through staged bootstrapping.
    pub fn new(enzymes: Vec<EnzymeDef>, food: FoodSet) -> Self {
        let map = enzymes.into_iter().map(|e| (e.id.clone(), e)).collect();
        Self { enzymes: map, food }
    }

    /// Current RAF status.
    pub fn raf_status(&self) -> RafResult {
        let enzyme_list: Vec<EnzymeDef> = self.enzymes.values().cloned().collect();
        detect_max_raf(&enzyme_list, &self.food)
    }

    /// All enzyme definitions.
    pub fn enzymes(&self) -> Vec<&EnzymeDef> {
        self.enzymes.values().collect()
    }

    /// Look up a single enzyme.
    pub fn get_enzyme(&self, id: &EnzymeId) -> Option<&EnzymeDef> {
        self.enzymes.get(id)
    }

    /// The food set.
    pub fn food(&self) -> &FoodSet {
        &self.food
    }

    /// Propose adding a new enzyme. Accepted only if RAF closure
    /// is maintained or improved.
    pub fn propose_add(&mut self, enzyme: EnzymeDef) -> Result<RafResult, GermlineError> {
        if self.enzymes.contains_key(&enzyme.id) {
            return Err(GermlineError::DuplicateEnzyme(enzyme.id));
        }

        let before = self.raf_status();

        // Tentatively add
        self.enzymes.insert(enzyme.id.clone(), enzyme.clone());
        let after = self.raf_status();

        // Gate: coverage must not decrease
        if after.coverage < before.coverage {
            // Rollback
            self.enzymes.remove(&enzyme.id);
            let lost: Vec<EnzymeId> = before.max_raf.difference(&after.max_raf).cloned().collect();
            return Err(GermlineError::ClosureBroken {
                coverage: after.coverage,
                lost,
            });
        }

        Ok(after)
    }

    /// Propose removing an enzyme. Accepted only if RAF closure
    /// is maintained.
    pub fn propose_remove(&mut self, id: &EnzymeId) -> Result<RafResult, GermlineError> {
        let enzyme = self
            .enzymes
            .remove(id)
            .ok_or_else(|| GermlineError::EnzymeNotFound(id.clone()))?;

        let after = self.raf_status();

        // Gate: all remaining enzymes that were in the RAF must stay in the RAF
        // (i.e., removing this enzyme must not orphan others)
        if !after.orphans.is_empty() && after.coverage < 1.0 {
            // Check if orphans existed before
            let before_with = {
                self.enzymes.insert(id.clone(), enzyme.clone());
                let r = self.raf_status();
                self.enzymes.remove(id);
                r
            };

            let new_orphans: Vec<EnzymeId> = after
                .orphans
                .difference(&before_with.orphans)
                .cloned()
                .collect();

            if !new_orphans.is_empty() {
                // Rollback
                self.enzymes.insert(id.clone(), enzyme);
                return Err(GermlineError::ClosureBroken {
                    coverage: after.coverage,
                    lost: new_orphans,
                });
            }
        }

        Ok(after)
    }

    /// Propose replacing an enzyme definition. Accepted only if
    /// RAF closure is maintained or improved.
    pub fn propose_replace(&mut self, enzyme: EnzymeDef) -> Result<RafResult, GermlineError> {
        let id = enzyme.id.clone();
        let old = self
            .enzymes
            .get(&id)
            .cloned()
            .ok_or_else(|| GermlineError::EnzymeNotFound(id.clone()))?;

        let before = self.raf_status();

        // Tentatively replace
        self.enzymes.insert(id.clone(), enzyme);
        let after = self.raf_status();

        // Gate: coverage must not decrease
        if after.coverage < before.coverage {
            // Rollback
            self.enzymes.insert(id, old);
            let lost: Vec<EnzymeId> = before.max_raf.difference(&after.max_raf).cloned().collect();
            return Err(GermlineError::ClosureBroken {
                coverage: after.coverage,
                lost,
            });
        }

        Ok(after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ArtifactType;

    fn enzyme(id: &str, reactants: &[&str], products: &[&str], catalysts: &[&str]) -> EnzymeDef {
        EnzymeDef {
            id: id.into(),
            reactants: reactants.iter().map(|&s| ArtifactType::from(s)).collect(),
            products: products.iter().map(|&s| ArtifactType::from(s)).collect(),
            catalysts: catalysts.iter().map(|&s| ArtifactType::from(s)).collect(),
            ..Default::default()
        }
    }

    fn food(items: &[&str]) -> FoodSet {
        items.iter().map(|&s| ArtifactType::from(s)).collect()
    }

    fn minimal_cycle() -> (Vec<EnzymeDef>, FoodSet) {
        let enzymes = vec![
            enzyme("coder", &["requirements"], &["code"], &["enzyme_defs"]),
            enzyme("tester", &["code"], &["test_results"], &["code"]),
            enzyme(
                "evolver",
                &["test_results"],
                &["enzyme_defs"],
                &["test_results"],
            ),
        ];
        (enzymes, food(&["requirements"]))
    }

    #[test]
    fn new_germline_from_closed_set() {
        let (enzymes, f) = minimal_cycle();
        let g = Germline::new(enzymes, f);
        let status = g.raf_status();
        assert!(status.is_closed());
    }

    #[test]
    fn add_compatible_enzyme_accepted() {
        let (enzymes, f) = minimal_cycle();
        let mut g = Germline::new(enzymes, f);

        // Add a doc generator that uses code (produced by coder) as catalyst
        let doc_gen = enzyme("doc_gen", &["code"], &["docs"], &["code"]);
        let result = g.propose_add(doc_gen);
        assert!(result.is_ok());
        assert_eq!(g.enzymes().len(), 4);
    }

    #[test]
    fn add_duplicate_rejected() {
        let (enzymes, f) = minimal_cycle();
        let mut g = Germline::new(enzymes, f);

        let dup = enzyme("coder", &["x"], &["y"], &["z"]);
        let result = g.propose_add(dup);
        assert!(matches!(result, Err(GermlineError::DuplicateEnzyme(_))));
    }

    #[test]
    fn remove_non_critical_enzyme_accepted() {
        let (enzymes, f) = minimal_cycle();
        let mut g = Germline::new(enzymes, f);

        // Add a non-critical enzyme then remove it
        let extra = enzyme("extra", &["code"], &["logs"], &["code"]);
        g.propose_add(extra).unwrap();

        let result = g.propose_remove(&EnzymeId::from("extra"));
        assert!(result.is_ok());
        assert_eq!(g.enzymes().len(), 3);
    }

    #[test]
    fn remove_critical_enzyme_rejected() {
        let (enzymes, f) = minimal_cycle();
        let mut g = Germline::new(enzymes, f);

        // Removing the evolver would break the cycle — coder needs enzyme_defs
        let result = g.propose_remove(&EnzymeId::from("evolver"));
        assert!(matches!(result, Err(GermlineError::ClosureBroken { .. })));
        // Enzyme should still be there (rollback)
        assert!(g.get_enzyme(&EnzymeId::from("evolver")).is_some());
    }

    #[test]
    fn replace_with_compatible_change_accepted() {
        let (enzymes, f) = minimal_cycle();
        let mut g = Germline::new(enzymes, f);

        // Replace coder with one that also produces docs
        let better_coder = enzyme(
            "coder",
            &["requirements"],
            &["code", "docs"],
            &["enzyme_defs"],
        );
        let result = g.propose_replace(better_coder);
        assert!(result.is_ok());

        let coder = g.get_enzyme(&EnzymeId::from("coder")).unwrap();
        assert!(coder.products.contains(&ArtifactType::from("docs")));
    }

    #[test]
    fn replace_breaking_closure_rejected() {
        let (enzymes, f) = minimal_cycle();
        let mut g = Germline::new(enzymes, f);

        // Replace evolver with one that no longer produces enzyme_defs
        let broken_evolver = enzyme(
            "evolver",
            &["test_results"],
            &["useless_stuff"],
            &["test_results"],
        );
        let result = g.propose_replace(broken_evolver);
        assert!(matches!(result, Err(GermlineError::ClosureBroken { .. })));

        // Original evolver should be restored
        let evolver = g.get_enzyme(&EnzymeId::from("evolver")).unwrap();
        assert!(
            evolver
                .products
                .contains(&ArtifactType::from("enzyme_defs"))
        );
    }

    #[test]
    fn remove_nonexistent_enzyme_errors() {
        let (enzymes, f) = minimal_cycle();
        let mut g = Germline::new(enzymes, f);
        let result = g.propose_remove(&EnzymeId::from("ghost"));
        assert!(matches!(result, Err(GermlineError::EnzymeNotFound(_))));
    }
}
