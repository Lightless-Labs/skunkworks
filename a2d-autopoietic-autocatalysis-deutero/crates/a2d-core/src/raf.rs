//! RAF (Reflexively Autocatalytic and Food-generated) detection algorithm.
//!
//! Implements Hordijk-Steel iterative pruning: start with all enzymes,
//! repeatedly remove any enzyme whose catalysts or reactants cannot be
//! produced by the remaining set + food. The fixed point is the maxRAF.

use crate::types::{ArtifactType, EnzymeDef, EnzymeId, FoodSet, RafResult};
use std::collections::BTreeSet;

/// Detect the maximal RAF in a catalytic reaction system.
///
/// This is the primary organizational health metric for A²D.
/// A coverage ratio of 1.0 means full catalytic closure.
pub fn detect_max_raf(enzymes: &[EnzymeDef], food: &FoodSet) -> RafResult {
    let all_ids: BTreeSet<EnzymeId> = enzymes.iter().map(|e| e.id.clone()).collect();

    if enzymes.is_empty() {
        return RafResult {
            max_raf: BTreeSet::new(),
            orphans: all_ids,
            coverage: 0.0,
            iterations: 0,
        };
    }

    let mut active: BTreeSet<EnzymeId> = all_ids.clone();
    let mut iterations = 0;

    loop {
        let producible = producible_artifacts(enzymes, &active, food);
        let mut next_active = BTreeSet::new();

        for enzyme in enzymes {
            if !active.contains(&enzyme.id) {
                continue;
            }

            // All catalysts must be producible (by other enzymes or in food)
            let catalysts_available = enzyme.catalysts.iter().all(|c| producible.contains(c));

            // All reactants must be producible (by other enzymes or in food)
            let reactants_available = enzyme.reactants.iter().all(|r| producible.contains(r));

            if catalysts_available && reactants_available {
                next_active.insert(enzyme.id.clone());
            }
        }

        iterations += 1;

        if next_active == active {
            break;
        }
        active = next_active;
    }

    let total = all_ids.len() as f64;
    let coverage = if total == 0.0 {
        0.0
    } else {
        active.len() as f64 / total
    };

    let orphans: BTreeSet<EnzymeId> = all_ids.difference(&active).cloned().collect();

    RafResult {
        max_raf: active,
        orphans,
        coverage,
        iterations,
    }
}

/// Compute all artifact types producible from the food set + active enzymes.
///
/// Iteratively expands: start with food, then add products of any enzyme
/// whose reactants are all available. Repeat until stable.
fn producible_artifacts(
    enzymes: &[EnzymeDef],
    active: &BTreeSet<EnzymeId>,
    food: &FoodSet,
) -> BTreeSet<ArtifactType> {
    let mut available: BTreeSet<ArtifactType> = food.clone();
    let active_enzymes: Vec<&EnzymeDef> =
        enzymes.iter().filter(|e| active.contains(&e.id)).collect();

    loop {
        let mut expanded = false;

        for enzyme in &active_enzymes {
            let reactants_met = enzyme.reactants.iter().all(|r| available.contains(r));
            if reactants_met {
                for product in &enzyme.products {
                    if available.insert(product.clone()) {
                        expanded = true;
                    }
                }
            }
        }

        if !expanded {
            break;
        }
    }

    available
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn empty_system_has_zero_coverage() {
        let result = detect_max_raf(&[], &food(&[]));
        assert_eq!(result.coverage, 0.0);
        assert!(result.max_raf.is_empty());
        assert!(!result.is_closed());
    }

    #[test]
    fn single_enzyme_with_food_catalysts_is_raf() {
        // Enzyme A: consumes food_x, produces art_y, catalyzed by food_z
        let enzymes = vec![enzyme("A", &["food_x"], &["art_y"], &["food_z"])];
        let f = food(&["food_x", "food_z"]);

        let result = detect_max_raf(&enzymes, &f);
        assert!(result.is_closed());
        assert_eq!(result.max_raf.len(), 1);
        assert!(result.orphans.is_empty());
    }

    #[test]
    fn enzyme_with_unproducible_catalyst_is_pruned() {
        // Enzyme A needs catalyst "magic" which nobody produces and isn't food
        let enzymes = vec![enzyme("A", &["food_x"], &["art_y"], &["magic"])];
        let f = food(&["food_x"]);

        let result = detect_max_raf(&enzymes, &f);
        assert_eq!(result.coverage, 0.0);
        assert!(result.max_raf.is_empty());
        assert_eq!(result.orphans.len(), 1);
    }

    #[test]
    fn minimal_autocatalytic_cycle() {
        // The A²D minimal irrRAF:
        // Coder: consumes requirements (food), produces code, catalyzed by enzyme_defs
        // Tester: consumes code, produces test_results, catalyzed by code
        // Evolver: consumes test_results, produces enzyme_defs, catalyzed by test_results
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
        let f = food(&["requirements"]);

        let result = detect_max_raf(&enzymes, &f);
        assert!(
            result.is_closed(),
            "Minimal cycle should be closed. Coverage: {}, RAF: {:?}, Orphans: {:?}",
            result.coverage,
            result.max_raf,
            result.orphans
        );
        assert_eq!(result.max_raf.len(), 3);
    }

    #[test]
    fn orphan_enzyme_detected_alongside_valid_raf() {
        // Valid cycle + one orphan that needs "unicorn" catalyst
        let enzymes = vec![
            enzyme("coder", &["requirements"], &["code"], &["enzyme_defs"]),
            enzyme("tester", &["code"], &["test_results"], &["code"]),
            enzyme(
                "evolver",
                &["test_results"],
                &["enzyme_defs"],
                &["test_results"],
            ),
            enzyme("orphan", &["code"], &["docs"], &["unicorn"]),
        ];
        let f = food(&["requirements"]);

        let result = detect_max_raf(&enzymes, &f);
        assert_eq!(result.max_raf.len(), 3);
        assert_eq!(result.orphans.len(), 1);
        assert!(result.orphans.contains(&EnzymeId::from("orphan")));
        assert!((result.coverage - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn cascade_pruning_removes_dependent_enzymes() {
        // B depends on A's product as catalyst, A has unproducible catalyst
        // Both should be pruned
        let enzymes = vec![
            enzyme("A", &["food_x"], &["art_a"], &["missing"]),
            enzyme("B", &["food_y"], &["art_b"], &["art_a"]),
        ];
        let f = food(&["food_x", "food_y"]);

        let result = detect_max_raf(&enzymes, &f);
        assert_eq!(result.coverage, 0.0);
        assert_eq!(result.orphans.len(), 2);
    }

    #[test]
    fn self_catalyzing_enzyme_with_food_reactants() {
        // Enzyme produces its own catalyst (self-catalyzing)
        // This needs bootstrapping: the first invocation needs catalyst from somewhere
        // If catalyst isn't in food, it can't start → should be pruned
        let enzymes = vec![enzyme(
            "self",
            &["food_x"],
            &["art_y", "cat_self"],
            &["cat_self"],
        )];
        let f = food(&["food_x"]);

        let result = detect_max_raf(&enzymes, &f);
        // self produces cat_self, but needs cat_self to run.
        // cat_self isn't in food and can't be produced without running self first.
        // The producible_artifacts function checks reactants not catalysts for production,
        // but the RAF check requires catalysts to be producible.
        // cat_self IS produced by self, and self's reactants (food_x) are available.
        // So producible_artifacts will include cat_self. Then the RAF check passes.
        assert!(
            result.is_closed(),
            "Self-catalyzing enzyme should be in RAF when its products include its catalyst"
        );
    }

    #[test]
    fn food_set_only_no_enzymes_yields_empty_raf() {
        let result = detect_max_raf(&[], &food(&["abundant_food"]));
        assert_eq!(result.coverage, 0.0);
        assert!(result.max_raf.is_empty());
    }

    #[test]
    fn two_independent_rafs_both_detected() {
        // Two independent cycles, both valid
        let enzymes = vec![
            // Cycle 1
            enzyme("A1", &["food_1"], &["art_1"], &["art_2"]),
            enzyme("A2", &["art_1"], &["art_2"], &["art_1"]),
            // Cycle 2
            enzyme("B1", &["food_2"], &["art_3"], &["art_4"]),
            enzyme("B2", &["art_3"], &["art_4"], &["art_3"]),
        ];
        let f = food(&["food_1", "food_2"]);

        let result = detect_max_raf(&enzymes, &f);
        assert!(result.is_closed());
        assert_eq!(result.max_raf.len(), 4);
    }

    #[test]
    fn reactant_chain_from_food() {
        // A produces B's reactant, B produces C's reactant
        // All catalyzed by food
        let enzymes = vec![
            enzyme("A", &["food"], &["art_a"], &["food"]),
            enzyme("B", &["art_a"], &["art_b"], &["food"]),
            enzyme("C", &["art_b"], &["art_c"], &["food"]),
        ];
        let f = food(&["food"]);

        let result = detect_max_raf(&enzymes, &f);
        assert!(result.is_closed());
        assert_eq!(result.max_raf.len(), 3);
    }
}
