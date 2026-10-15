#[test]
fn spec_r1_no_inner_attribute_or_documentation_behavior() {
    // R1 is a source-file compilation constraint; no runtime behavior to assert.
}

#[test]
fn spec_r2_public_types_and_constants() {
    assert_eq!(MAX_VERTICES, 4096);
    assert_eq!(MAX_EDGES, 32768);

    let error = GraphError::TooManyEdges { limit: MAX_EDGES };
    assert_eq!(error, GraphError::TooManyEdges { limit: 32768 });

    let graph = Graph {
        components: vec![vec![0]],
        component_of: vec![0],
        cyclic: vec![false],
    };
    assert_eq!(graph.components, vec![vec![0]]);
}

#[test]
fn spec_r3_outputs_are_owned_and_calls_have_no_persistent_state() {
    let edges = vec![(0, 1), (1, 0)];
    let first = analyze(2, &edges).unwrap();
    let second = analyze(2, &[]).unwrap();

    assert_eq!(first.components, vec![vec![0, 1]]);
    assert_eq!(second.components, vec![vec![0], vec![1]]);
    assert_eq!(first.cyclic, vec![true]);
    assert_eq!(second.cyclic, vec![false, false]);
}

#[test]
fn spec_r4_vertex_limit_and_precedence() {
    assert_eq!(
        analyze(MAX_VERTICES + 1, &vec![(usize::MAX, usize::MAX); MAX_EDGES + 1]),
        Err(GraphError::TooManyVertices {
            count: MAX_VERTICES + 1,
            limit: MAX_VERTICES,
        })
    );

    let graph = analyze(MAX_VERTICES, &[]).unwrap();
    assert_eq!(graph.components.len(), MAX_VERTICES);
    assert_eq!(graph.component_of.len(), MAX_VERTICES);
    assert_eq!(graph.cyclic, vec![false; MAX_VERTICES]);
}

#[test]
fn spec_r5_edge_limit_and_precedence() {
    assert_eq!(
        analyze(0, &vec![(usize::MAX, usize::MAX); MAX_EDGES + 1]),
        Err(GraphError::TooManyEdges { limit: MAX_EDGES })
    );

    let graph = analyze(1, &vec![(0, 0); MAX_EDGES]).unwrap();
    assert_eq!(graph.components, vec![vec![0]]);
    assert_eq!(graph.cyclic, vec![true]);
}

#[test]
fn spec_r6_first_bad_endpoint_and_source_before_target() {
    assert_eq!(
        analyze(3, &[(0, 1), (3, 3), (usize::MAX, 0)]),
        Err(GraphError::BadEndpoint {
            edge: 1,
            side: 0,
            vertex: 3,
        })
    );

    assert_eq!(
        analyze(3, &[(1, 3)]),
        Err(GraphError::BadEndpoint {
            edge: 0,
            side: 1,
            vertex: 3,
        })
    );
}

#[test]
fn spec_r7_endpoint_positions_duplicates_and_usize_max() {
    assert_eq!(
        analyze(2, &[(0, 0), (0, 0), (1, 1), (usize::MAX, 1)]),
        Err(GraphError::BadEndpoint {
            edge: 3,
            side: 0,
            vertex: usize::MAX,
        })
    );

    assert_eq!(
        analyze(2, &[(0, 1), (1, usize::MAX)]),
        Err(GraphError::BadEndpoint {
            edge: 1,
            side: 1,
            vertex: usize::MAX,
        })
    );
}

#[test]
fn spec_r8_zero_length_path_makes_single_vertex_reachable_from_itself() {
    let graph = analyze(1, &[]).unwrap();
    assert_eq!(graph.components, vec![vec![0]]);
    assert_eq!(graph.component_of, vec![0]);
    assert_eq!(graph.cyclic, vec![false]);
}

#[test]
fn spec_r9_directed_mutual_reachability_and_isolated_vertices() {
    let graph = analyze(5, &[(0, 1), (1, 0), (1, 2), (3, 4)]).unwrap();
    assert_eq!(graph.components, vec![vec![0, 1], vec![2], vec![3], vec![4]]);
    assert_eq!(graph.component_of, vec![0, 0, 1, 2, 3]);
}

#[test]
fn spec_r10_component_members_are_distinct_and_sorted() {
    let graph = analyze(3, &[(2, 0), (0, 2), (2, 0), (1, 1)]).unwrap();
    assert_eq!(graph.components, vec![vec![0, 2], vec![1]]);
    assert_eq!(graph.components[0], vec![0, 2]);
}

#[test]
fn spec_r11_component_order_size_then_smallest_member() {
    let graph = analyze(
        8,
        &[(0, 1), (1, 0), (2, 3), (3, 2), (4, 5), (5, 4), (6, 7), (7, 6)],
    )
    .unwrap();

    assert_eq!(
        graph.components,
        vec![vec![0, 1], vec![2, 3], vec![4, 5], vec![6, 7]]
    );
    assert_eq!(graph.component_of, vec![0, 0, 1, 1, 2, 2, 3, 3]);
}

#[test]
fn spec_r12_component_of_uses_canonical_sorted_component_ids() {
    let graph = analyze(4, &[(2, 3), (3, 2)]).unwrap();
    assert_eq!(graph.components, vec![vec![2, 3], vec![0], vec![1]]);
    assert_eq!(graph.component_of, vec![1, 2, 0, 0]);
}

#[test]
fn spec_r13_cyclic_flags_for_multivertex_and_singleton_self_loop_components() {
    let graph = analyze(4, &[(0, 1), (1, 0), (2, 2), (3, 0)]).unwrap();
    assert_eq!(graph.components, vec![vec![0, 1], vec![2], vec![3]]);
    assert_eq!(graph.cyclic, vec![true, true, false]);
}

#[test]
fn spec_r14_duplicates_and_self_loops_do_not_change_partition_or_flags() {
    let base = analyze(3, &[(0, 1), (1, 0), (2, 2)]).unwrap();
    let repeated = analyze(
        3,
        &[(0, 1), (0, 1), (1, 0), (1, 0), (2, 2), (2, 2)],
    )
    .unwrap();

    assert_eq!(repeated.components, base.components);
    assert_eq!(repeated.component_of, base.component_of);
    assert_eq!(repeated.cyclic, base.cyclic);
}

#[test]
fn spec_r15_empty_graph_and_nonempty_edges_at_zero_vertices() {
    let empty = analyze(0, &[]).unwrap();
    assert_eq!(empty.components, Vec::<Vec<usize>>::new());
    assert_eq!(empty.component_of, Vec::<usize>::new());
    assert_eq!(empty.cyclic, Vec::<bool>::new());

    assert_eq!(
        analyze(0, &[(0, 0), (usize::MAX, 0)]),
        Err(GraphError::BadEndpoint {
            edge: 0,
            side: 0,
            vertex: 0,
        })
    );
}

#[test]
fn spec_r16_no_edges_produce_singletons_in_vertex_order() {
    let graph = analyze(4, &[]).unwrap();
    assert_eq!(graph.components, vec![vec![0], vec![1], vec![2], vec![3]]);
    assert_eq!(graph.component_of, vec![0, 1, 2, 3]);
    assert_eq!(graph.cyclic, vec![false, false, false, false]);
}

#[test]
fn spec_r17_edge_permutation_preserves_graph_fields() {
    let first = analyze(5, &[(0, 1), (1, 0), (1, 2), (2, 3), (3, 2)]).unwrap();
    let permuted = analyze(5, &[(3, 2), (1, 2), (0, 1), (2, 3), (1, 0)]).unwrap();

    assert_eq!(permuted.components, first.components);
    assert_eq!(permuted.component_of, first.component_of);
    assert_eq!(permuted.cyclic, first.cyclic);
}

#[test]
fn spec_r18_long_path_and_all_vertices_in_one_component() {
    let n = MAX_VERTICES;
    let path: Vec<(usize, usize)> = (0..n - 1).map(|v| (v, v + 1)).collect();
    let path_graph = analyze(n, &path).unwrap();
    assert_eq!(path_graph.components.len(), n);
    assert_eq!(path_graph.components[0], vec![0]);
    assert_eq!(path_graph.components[n - 1], vec![n - 1]);

    let cycle: Vec<(usize, usize)> = (0..n).map(|v| (v, (v + 1) % n)).collect();
    let cycle_graph = analyze(n, &cycle).unwrap();
    assert_eq!(cycle_graph.components, vec![(0..n).collect::<Vec<_>>()]);
    assert_eq!(cycle_graph.component_of, vec![0; n]);
    assert_eq!(cycle_graph.cyclic, vec![true]);
}
