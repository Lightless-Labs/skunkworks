#[test]
fn spec_r1_single_source_compilation() {
    let result = analyze(0, &[]);
    assert_eq!(result, Ok(Graph {
        components: vec![],
        component_of: vec![],
        cyclic: vec![],
        internal_edges: vec![],
        arcs: vec![],
    }));
}

#[test]
fn spec_r2_public_types_and_fields() {
    let arc = Arc { from: 0, to: 1, multiplicity: 1, first_edge: 0 };
    let graph = Graph {
        components: vec![vec![0], vec![1]],
        component_of: vec![0, 1],
        cyclic: vec![false, false],
        internal_edges: vec![0, 0],
        arcs: vec![arc.clone()],
    };
    let cloned = graph.clone();
    assert_eq!(graph, cloned);
    assert_eq!(arc, cloned.arcs[0]);
    let error = GraphError::BadEndpoint { edge: 0, side: 1, vertex: 2 };
    assert_eq!(error.clone(), error);
}

#[test]
fn spec_r3_independent_owned_results() {
    let edges = vec![(0, 1), (1, 0)];
    let first = analyze(2, &edges).unwrap();
    let second = analyze(2, &[]);
    assert_eq!(first.components, vec![vec![0, 1]]);
    assert_eq!(second.unwrap().components, vec![vec![0], vec![1]]);
    assert_eq!(edges, vec![(0, 1), (1, 0)]);
}

#[test]
fn spec_r4_vertex_limit_precedes_edge_limit_and_endpoints() {
    assert_eq!(
        analyze(MAX_VERTICES + 1, &vec![(usize::MAX, usize::MAX); MAX_EDGES + 1]),
        Err(GraphError::TooManyVertices { count: MAX_VERTICES + 1, limit: MAX_VERTICES })
    );
}

#[test]
fn spec_r4_maximum_vertex_count_is_allowed() {
    let graph = analyze(MAX_VERTICES, &[]).unwrap();
    assert_eq!(graph.components.len(), MAX_VERTICES);
    assert_eq!(graph.component_of.len(), MAX_VERTICES);
    assert_eq!(graph.cyclic, vec![false; MAX_VERTICES]);
}

#[test]
fn spec_r5_edge_limit_precedes_endpoint_validation() {
    assert_eq!(
        analyze(0, &vec![(usize::MAX, usize::MAX); MAX_EDGES + 1]),
        Err(GraphError::TooManyEdges { limit: MAX_EDGES })
    );
}

#[test]
fn spec_r5_maximum_edge_count_is_allowed() {
    let edges = vec![(0, 0); MAX_EDGES];
    let graph = analyze(1, &edges).unwrap();
    assert_eq!(graph.internal_edges, vec![MAX_EDGES]);
    assert!(graph.cyclic[0]);
}

#[test]
fn spec_r6_reports_first_bad_edge_and_checks_source_first() {
    assert_eq!(
        analyze(3, &[(0, 1), (3, 3), (usize::MAX, 0)]),
        Err(GraphError::BadEndpoint { edge: 1, side: 0, vertex: 3 })
    );
    assert_eq!(
        analyze(3, &[(0, 3)]),
        Err(GraphError::BadEndpoint { edge: 0, side: 1, vertex: 3 })
    );
}

#[test]
fn spec_r7_endpoint_positions_include_duplicates_and_loops() {
    assert_eq!(
        analyze(2, &[(0, 0), (0, 1), (0, 1), (1, usize::MAX)]),
        Err(GraphError::BadEndpoint { edge: 3, side: 1, vertex: usize::MAX })
    );
}

#[test]
fn spec_r8_zero_length_path_keeps_isolates_as_components() {
    let graph = analyze(3, &[]).unwrap();
    assert_eq!(graph.components, vec![vec![0], vec![1], vec![2]]);
}

#[test]
fn spec_r9_sccs_follow_directed_mutual_reachability() {
    let graph = analyze(4, &[(0, 1), (1, 2), (2, 0), (2, 3)]).unwrap();
    assert_eq!(graph.components, vec![vec![0, 1, 2], vec![3]]);
}

#[test]
fn spec_r10_component_members_are_distinct_and_sorted() {
    let graph = analyze(4, &[(3, 1), (1, 3), (3, 1)]).unwrap();
    assert!(graph.components.iter().all(|component| !component.is_empty()));
    assert_eq!(graph.components[0], vec![1, 3]);
    assert_eq!(graph.components[1], vec![0]);
    assert_eq!(graph.components[2], vec![2]);
}

#[test]
fn spec_r11_components_sort_by_size_then_smallest_member() {
    let graph = analyze(7, &[
        (0, 1), (1, 0),
        (2, 3), (3, 2),
        (4, 5), (5, 4), (5, 6), (6, 5),
    ]).unwrap();
    assert_eq!(graph.components, vec![vec![4, 5, 6], vec![0, 1], vec![2, 3]]);
    assert_eq!(graph.component_of, vec![1, 1, 2, 2, 0, 0, 0]);
}

#[test]
fn spec_r12_component_ids_match_sorted_component_vector() {
    let graph = analyze(5, &[(0, 1), (1, 0), (2, 3), (3, 2)]).unwrap();
    assert_eq!(graph.components, vec![vec![0, 1], vec![2, 3], vec![4]]);
    assert_eq!(graph.component_of, vec![0, 0, 1, 1, 2]);
}

#[test]
fn spec_r13_only_multi_vertex_components_or_singletons_with_loops_are_cyclic() {
    let graph = analyze(4, &[(0, 1), (1, 0), (2, 2), (0, 3)]).unwrap();
    assert_eq!(graph.components, vec![vec![0, 1], vec![2], vec![3]]);
    assert_eq!(graph.cyclic, vec![true, true, false]);
}

#[test]
fn spec_r14_duplicate_edges_count_and_do_not_change_sccs() {
    let graph = analyze(2, &[(0, 0), (0, 0), (0, 1), (0, 1), (1, 0)]).unwrap();
    assert_eq!(graph.components, vec![vec![0, 1]]);
    assert_eq!(graph.cyclic, vec![true]);
    assert_eq!(graph.internal_edges, vec![5]);
}

#[test]
fn spec_r15_empty_graph_and_nonempty_edges_at_zero_vertices() {
    assert_eq!(analyze(0, &[]), Ok(Graph {
        components: vec![],
        component_of: vec![],
        cyclic: vec![],
        internal_edges: vec![],
        arcs: vec![],
    }));
    assert_eq!(
        analyze(0, &[(0, 0), (usize::MAX, 0)]),
        Err(GraphError::BadEndpoint { edge: 0, side: 0, vertex: 0 })
    );
}

#[test]
fn spec_r16_no_edges_yield_ordered_acyclic_singletons() {
    let graph = analyze(4, &[]).unwrap();
    assert_eq!(graph.components, vec![vec![0], vec![1], vec![2], vec![3]]);
    assert_eq!(graph.component_of, vec![0, 1, 2, 3]);
    assert_eq!(graph.cyclic, vec![false; 4]);
}

#[test]
fn spec_r17_edge_permutation_preserves_sccs_and_cyclic_flags() {
    let first = analyze(4, &[(0, 1), (1, 0), (1, 2), (2, 3)]).unwrap();
    let second = analyze(4, &[(2, 3), (1, 2), (1, 0), (0, 1)]).unwrap();
    assert_eq!(first.components, second.components);
    assert_eq!(first.component_of, second.component_of);
    assert_eq!(first.cyclic, second.cyclic);
}

#[test]
fn spec_r18_long_path_and_single_component_are_supported() {
    let n = MAX_VERTICES;
    let path: Vec<(usize, usize)> = (0..n - 1).map(|v| (v, v + 1)).collect();
    let graph = analyze(n, &path).unwrap();
    assert_eq!(graph.components.len(), n);
    assert_eq!(graph.components[0], vec![0]);
    assert_eq!(graph.components[n - 1], vec![n - 1]);

    let cycle: Vec<(usize, usize)> = (0..n).map(|v| (v, (v + 1) % n)).collect();
    let graph = analyze(n, &cycle).unwrap();
    assert_eq!(graph.components, vec![(0..n).collect::<Vec<_>>()]);
    assert_eq!(graph.cyclic, vec![true]);
}

#[test]
fn spec_r19_internal_edges_count_occurrences_per_component() {
    let graph = analyze(3, &[(0, 1), (1, 0), (0, 0), (2, 2)]).unwrap();
    assert_eq!(graph.components, vec![vec![0, 1], vec![2]]);
    assert_eq!(graph.internal_edges, vec![3, 1]);
}

#[test]
fn spec_r20_self_loops_and_duplicates_each_count_as_internal_edges() {
    let graph = analyze(2, &[(0, 0), (0, 0), (1, 0), (0, 1)]).unwrap();
    assert_eq!(graph.internal_edges, vec![4]);
    let singleton = analyze(1, &[]).unwrap();
    assert_eq!(singleton.internal_edges, vec![0]);
}

#[test]
fn spec_r21_condensation_never_contains_internal_arcs() {
    let graph = analyze(3, &[(0, 1), (1, 0), (1, 2), (2, 2)]).unwrap();
    assert!(graph.arcs.iter().all(|arc| arc.from != arc.to));
    assert_eq!(graph.arcs, vec![Arc { from: 0, to: 1, multiplicity: 1, first_edge: 2 }]);
}

#[test]
fn spec_r22_one_arc_per_ordered_component_pair_across_vertex_pairs() {
    let graph = analyze(4, &[(0, 1), (1, 0), (2, 3), (3, 2), (0, 2), (1, 3)]).unwrap();
    assert_eq!(graph.arcs.len(), 1);
    assert_eq!(graph.arcs[0].from, 0);
    assert_eq!(graph.arcs[0].to, 1);
}

#[test]
fn spec_r23_arc_multiplicity_counts_all_cross_component_occurrences() {
    let graph = analyze(3, &[(0, 1), (0, 1), (0, 2), (1, 2)]).unwrap();
    assert_eq!(graph.arcs, vec![
        Arc { from: 0, to: 1, multiplicity: 2, first_edge: 0 },
        Arc { from: 0, to: 2, multiplicity: 1, first_edge: 2 },
        Arc { from: 1, to: 2, multiplicity: 1, first_edge: 3 },
    ]);
}

#[test]
fn spec_r24_arc_witness_is_smallest_original_input_index() {
    let graph = analyze(3, &[(0, 0), (1, 1), (0, 2), (0, 2), (1, 2)]).unwrap();
    assert_eq!(graph.arcs, vec![
        Arc { from: 0, to: 2, multiplicity: 2, first_edge: 2 },
        Arc { from: 1, to: 2, multiplicity: 1, first_edge: 4 },
    ]);
}

#[test]
fn spec_r25_arcs_sort_by_source_then_descending_target() {
    let graph = analyze(6, &[(0, 1), (1, 0), (2, 3), (3, 2), (4, 5), (5, 4),
                            (0, 2), (0, 4), (2, 4), (2, 0), (4, 0)]).unwrap();
    assert_eq!(graph.components, vec![vec![0, 1], vec![2, 3], vec![4, 5]]);
    assert_eq!(
        graph.arcs.iter().map(|arc| (arc.from, arc.to)).collect::<Vec<_>>(),
        vec![(0, 2), (0, 1), (1, 2), (1, 0), (2, 0)]
    );
}

#[test]
fn spec_r26_each_occurrence_is_accounted_for_with_original_positions() {
    let graph = analyze(3, &[(0, 0), (0, 1), (0, 1), (1, 0), (1, 2), (2, 2)]).unwrap();
    assert_eq!(graph.components, vec![vec![0, 1], vec![2]]);
    assert_eq!(graph.internal_edges, vec![4, 1]);
    assert_eq!(graph.arcs, vec![Arc { from: 0, to: 1, multiplicity: 1, first_edge: 4 }]);
    let accounted: usize = graph.internal_edges.iter().sum::<usize>()
        + graph.arcs.iter().map(|arc| arc.multiplicity).sum::<usize>();
    assert_eq!(accounted, 6);
}
