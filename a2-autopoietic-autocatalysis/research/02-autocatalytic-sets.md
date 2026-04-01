# Autocatalytic Sets: Theory, Formalization, and Relevance to Self-Sustaining Systems

**Date:** 2026-04-01
**Status:** Research note

---

## 1. Kauffman's Autocatalytic Sets

### 1.1 Origin and Core Idea

Stuart Kauffman first proposed the concept of autocatalytic sets in 1971, with the full theory developed in his 1986 paper "Autocatalytic sets of proteins" (with Farmer) and expanded in *The Origins of Order* (1993) and *At Home in the Universe* (1995).

The central claim: a sufficiently diverse set of catalytic polymers will, with high probability, contain a subset in which every molecule's formation is catalyzed by at least one other molecule in the set. Life need not have begun with a single self-replicating molecule (the "replicator first" view); instead, **collective autocatalysis** could have bootstrapped metabolism before genetics.

### 1.2 The Binary Polymer Model

Kauffman introduced an abstract model of chemical reaction systems:

- **Molecules** are represented as binary strings (sequences of 0s and 1s).
- **Reactions** are ligation (concatenation of two strings) and cleavage (splitting a string into two).
- **Catalysis** is assigned randomly: each molecule has a fixed *a priori* probability *p* of catalyzing any given reaction.
- A **food set** *F* of short polymers is assumed freely available from the environment.

The key insight is combinatorial: as maximum polymer length *n* increases, the number of possible molecules grows as ~2^n, but the number of possible reactions grows faster (roughly ~n * 2^n for cleavage alone). Since each molecule has probability *p* of catalyzing each reaction, the expected number of catalyzed reactions per molecule grows superlinearly with molecular diversity.

### 1.3 The Phase Transition Argument

Kauffman drew an analogy to Erdos-Renyi random graph theory. In a random graph, when the ratio of edges to nodes exceeds a critical threshold, a giant connected component appears as a **phase transition**. Analogously:

- Nodes = molecule types
- Edges = catalytic relationships (molecule *A* catalyzes the formation of molecule *B*)

When molecular diversity crosses a threshold, the probability of a **collectively autocatalytic set** existing jumps from near-zero to near-one. The system undergoes a percolation transition on the reaction graph.

Kauffman's original argument suggested the critical catalysis probability scaled as ~1/M where M is the number of molecule types. Later formal work (Hordijk, Steel, and Kauffman, 2010) showed that only a **linear growth rate** in catalysis level with increasing maximum polymer length is required -- significantly more modest than Kauffman's original expectation, strengthening the plausibility argument.

### 1.4 Catalytic Closure

An autocatalytic set achieves **catalytic closure**: every reaction needed to produce the set's members from the food set is catalyzed by some member of the set. The set is self-sustaining -- given food, it can regenerate all of its components without external catalysts.

This is distinct from *stoichiometric closure* (mass balance) and *thermodynamic closure* (energy). Kauffman's claim is specifically about the catalytic network topology.

### 1.5 Relationship to the Origin of Life

Autocatalytic sets offer a "metabolism first" alternative to "RNA world" and "replicator first" hypotheses:

- No single self-replicating molecule is needed.
- The collective network is the unit of selection.
- Life begins as a phase transition in chemical complexity.
- Compartmentalization (e.g., lipid vesicles) can come later.

Experimental evidence has accumulated:
- **Ghadiri (1996):** A 32-residue alpha-helical peptide that autocatalytically templates its own synthesis.
- **Ashkenasy, Lehman, Otto, and colleagues:** Cross-catalytic peptide networks demonstrating collective autocatalysis.
- **2020 prebiotic chemistry studies:** Abundance of autocatalytic and hetero-catalytic reaction cycles in prebiotic chemical networks, with mutually linked cycles acting as collectively autocatalytic sets.
- **2025 RNA perspectives:** Multicomponent mixtures self-organizing via prebiotic evolution of autocatalytic RNAs, with crowded environments enabling heritable variation.

### 1.6 Key Criticisms

1. **Parasites and side reactions:** Real chemistry produces many parasitic side-reactions that could poison autocatalytic sets. Response: compartmentalization and spatial structure can contain parasites.
2. **Specificity of catalysis:** Random catalysis assumptions may not hold -- real enzymes are highly specific. Response: ribozymes and short peptides show broader catalytic repertoires; the binary polymer model is intentionally abstract.
3. **Thermodynamic feasibility:** Catalytic closure says nothing about energy. Response: the food set implicitly carries free energy; coupling to energy sources is a separate but compatible requirement.
4. **Evolvability:** Can autocatalytic sets evolve by natural selection? Response: RAF theory shows hierarchical substructure (irrRAFs) that can provide heritable variation.

### Key References
- Kauffman, S. (1986). "Autocatalytic sets of proteins." *Journal of Theoretical Biology*, 119, 1-24.
- Kauffman, S. (1993). *The Origins of Order*. Oxford University Press.
- Kauffman, S. (1995). *At Home in the Universe*. Oxford University Press.

---

## 2. RAF Theory (Reflexively Autocatalytic and Food-generated Sets)

### 2.1 Motivation

Kauffman's original arguments were informal and relied on analogies to random graphs. Wim Hordijk and Mike Steel developed **RAF theory** as a rigorous mathematical framework to formalize, test, and extend Kauffman's claims. Their foundational paper appeared in 2004.

### 2.2 Formal Definitions

A **Catalytic Reaction System (CRS)** is a tuple Q = (X, R, C, F) where:

- **X** = set of molecule types
- **R** = set of reactions, where each reaction r in R has a set of reactants rho(r) subset X and a set of products pi(r) subset X
- **C** = catalysis assignment: for each reaction r, C(r) subset X is the set of molecule types that catalyze r
- **F** subset X = the **food set** of molecule types freely available from the environment

A subset R' of R (together with the molecules involved in R') is a **RAF set** if it is:

1. **Reflexively Autocatalytic (RA):** Every reaction r in R' is catalyzed by at least one molecule type that is either in F or is a product of some reaction in R'.
2. **Food-generated (F):** Every reactant of every reaction in R' can be constructed from the food set F by successive application of reactions from R' itself.

In other words: the set is self-sufficient. Every reaction has an internal catalyst, and every molecule can be built up from food using only internal reactions.

### 2.3 maxRAF, irrRAF, and Hierarchical Structure

**maxRAF:** The unique maximal RAF set -- the union of all RAF subsets within a CRS. The RAF algorithm returns the maxRAF in polynomial time, or determines that no RAF exists.

**subRAF:** Any subset of the maxRAF that itself satisfies the RAF property.

**irrRAF (irreducible RAF):** A RAF set that cannot be reduced further -- removing any single reaction destroys the RAF property. These are the minimal self-sustaining units within a maxRAF.

**Key structural result:** The maxRAF typically contains multiple irrRAFs, giving it a **hierarchical, decomposable structure**. This is significant for evolvability: different irrRAFs can be gained, lost, or combined, providing a mechanism for heritable variation and selection in autocatalytic networks.

### 2.4 CAF Sets (Constructively Autocatalytic and Food-generated)

A **CAF** is a RAF with the additional constraint that reactions can be **totally ordered** such that for each reaction, all its reactants and at least one catalyst are either in F or produced by an earlier reaction in the ordering. CAFs are strictly more restrictive than RAFs -- every CAF is a RAF, but not every RAF is a CAF. CAFs avoid circular dependencies entirely.

### 2.5 The RAF Algorithm

Hordijk and Steel proved that detecting whether a maxRAF exists in a CRS, and finding it, can be done in **polynomial time** (specifically O(|R|^2 * |X|) in the straightforward implementation). This is notable because many related problems on reaction networks are NP-hard in general.

The algorithm works by iterative pruning:
1. Start with all reactions R.
2. Remove any reaction whose reactants cannot all be produced from F using the current reaction set, or that has no catalyst producible from F using the current set.
3. Repeat until no more reactions can be removed.
4. If a non-empty set remains, it is the maxRAF.

Finding irrRAFs is computationally harder but still tractable for practical system sizes. Polynomial-time algorithms for analyzing minimal RAFs were developed in later work (Hordijk et al., 2015; Steel et al., 2024).

### 2.6 Conditions for Emergence

In random instances of the binary polymer model with maximum string length *n*:

- Each molecule catalyzes each reaction independently with probability *p*.
- The expected number of reactions catalyzed per molecule is f(n) ~ p * |R(n)|.

**Key result (Mossel & Steel, 2005; Hordijk, Steel & Kauffman, 2010):** RAF sets exist with high probability when f(n) grows **at least linearly** in n. Specifically, when f(n) >= c*n for a modest constant c, a RAF almost certainly exists. The critical transition already occurs at very modest levels of catalysis: **between one and two reactions catalyzed per molecule type** for moderate-sized networks.

This is dramatically better than Kauffman's original expectation (which suggested quadratic or higher growth might be needed) and makes the spontaneous emergence of autocatalytic sets far more plausible.

### 2.7 Experimental Validation

RAF sets have been detected in:
- The metabolic network of *Escherichia coli*
- Reconstructed metabolic networks of ancient anaerobic autotrophs
- Experimental peptide cross-catalytic networks

### Key References
- Hordijk, W. & Steel, M. (2004). "Detecting autocatalytic, self-sustaining sets in chemical reaction systems." *Journal of Theoretical Biology*, 227(4), 451-461.
- Mossel, E. & Steel, M. (2005). "Random biochemical networks: the probability of self-sustaining autocatalysis." *Journal of Theoretical Biology*, 233(3), 327-336.
- Hordijk, W., Steel, M., & Kauffman, S. (2010). "Required levels of catalysis for emergence of autocatalytic sets in models of chemical reaction systems." *International Journal of Molecular Sciences*, 12(5), 3085-3101.
- Hordijk, W. (2019). "A history of autocatalytic sets." *Biological Theory*, 14, 224-246.
- Hordijk, W. (2023). "A concise and formal definition of RAF sets and the RAF algorithm." arXiv:2303.01809.
- Steel, M. et al. (2024). "Self-generating autocatalytic networks: structural results, algorithms and their relevance to early biochemistry." *Journal of the Royal Society Interface*, 21(214).

---

## 3. Closure to Efficient Causation (Rosen) and Autopoiesis

### 3.1 Rosen's (M,R)-Systems

Robert Rosen, in *Life Itself* (1991), proposed that the essential characteristic distinguishing living from non-living systems is **closure to efficient causation**. His framework uses Aristotle's four causes:

- **Material cause:** the substrates
- **Formal cause:** the structure/pattern
- **Final cause:** the function/purpose
- **Efficient cause:** the agent/catalyst that makes something happen

In machines, efficient causes are always external (the engineer, the designer). In organisms, **efficient causes are generated internally** -- the system produces its own catalysts.

### 3.2 The (M,R) Formalism

An **(M,R)-system** (Metabolism-Repair system) consists of three components forming a closed causal loop:

1. **Metabolism (f):** Transforms input materials A into output materials B. Formally, f: A -> B.
2. **Repair (Phi_f):** Uses outputs B to repair/regenerate the metabolic component f. Formally, Phi_f: B -> Hom(A,B) (maps outputs to metabolic functions).
3. **Replication (beta_b):** Uses f to regenerate the repair component Phi_f. Formally, beta_b: Hom(A,B) -> Hom(B, Hom(A,B)).

The key feature is the **closed loop of efficient causation**: f -> beta_b -> Phi_f -> f. Each component is the efficient cause of the next. No external "builder" is needed.

### 3.3 Category Theory Foundation

Rosen's **relational biology** (building on Rashevsky's earlier work) uses category theory to express these relationships abstractly, focusing on the *organization* of causal relationships rather than the material substrate. This is fundamentally different from mechanistic (reductionist) biology:

- **Mechanistic view:** A system is described by its parts and their interactions. The whole is reducible to parts.
- **Relational view:** A system is described by its functional organization. The whole has properties (closure to efficient causation) that no part possesses alone.

Rosen argued that any system with closure to efficient causation is **non-computable** by a Turing machine -- a controversial claim. Letelier, Soto-Andrade, Guinez, Cornish-Bowden, and Cardenas (2006) explored this computability question and showed that while (M,R)-systems have self-referential structure, the non-computability claim depends on specific interpretations of the formalism.

### 3.4 Autopoiesis (Maturana and Varela)

Humberto Maturana and Francisco Varela introduced **autopoiesis** (Greek: self-production) in 1972:

> An autopoietic system is a network of inter-related component-producing processes such that the components in interaction generate the same network that produced them, and constitute it as a unity in the space in which they exist by specifying the topological domain of its realization.

Key features:
- **Self-production:** The system produces all its own components.
- **Boundary production:** The system produces its own boundary (membrane), distinguishing self from non-self.
- **Organizational closure:** The network of processes is closed -- every component is produced by processes within the network.
- **Structural coupling:** The system interacts with its environment through perturbations that trigger internal structural changes, without the environment specifying those changes.

For Maturana and Varela, autopoiesis is both necessary and sufficient for life.

### 3.5 Comparing the Three Frameworks

| Property | Autocatalytic Sets (Kauffman) | (M,R)-Systems (Rosen) | Autopoiesis (Maturana & Varela) |
|---|---|---|---|
| **Closure type** | Catalytic closure | Closure to efficient causation | Organizational closure + boundary production |
| **Self-referentiality** | Implicit (mutual catalysis) | Explicit (causal loop f -> beta -> Phi -> f) | Explicit (self-producing network) |
| **Boundary** | Not addressed | Not required (organizational, not spatial) | Required (the system produces its own boundary) |
| **Material substrate** | Chemistry-specific (polymers, reactions) | Abstract (relational, category-theoretic) | Physical (molecular, spatial) |
| **Computability** | Computable (RAF algorithm is polynomial) | Claimed non-computable (controversial) | Not addressed formally |
| **Evolvability** | Yes (via irrRAF structure) | Not directly addressed | Not directly addressed |

### 3.6 Formal Relationships

**Letelier et al. (2003)** argued that autopoietic systems are a **proper subset** of (M,R)-systems: all autopoietic systems are (M,R)-systems, but not all (M,R)-systems are autopoietic, because autopoiesis additionally requires boundary (membrane) generation and internal spatial topology. This interpretation was contested by McMullin (2004) and Razeto-Barry (2012).

**Cornish-Bowden and Cardenas (2020)** compared all major theories of life -- (M,R)-systems, the hypercycle (Eigen), the chemoton (Ganti), autopoiesis, and autocatalytic sets -- and found they all invoke some form of closure but differ in:
- What is closed (catalysis, causation, organization, physical boundary)
- Level of abstraction (chemical vs. relational vs. spatial)
- Whether boundary production is required

**Key finding:** When (M,R)-systems and autopoiesis are compared with other characterizations of operational closure, they are the only two that are equivalent in their **self-referentiality** -- both require that the system produce the very components that produce the system, including the "repair" or "regeneration" function itself.

Autocatalytic sets (in the RAF sense) achieve catalytic closure but are less demanding: they require that every reaction has a catalyst from the set, but do not require the deeper self-referential loop where the system produces its own "repair machinery." An autocatalytic set *can* be a (M,R)-system if it additionally achieves closure at the repair/replication level.

### Key References
- Rosen, R. (1991). *Life Itself: A Comprehensive Inquiry into the Nature, Origin, and Fabrication of Life*. Columbia University Press.
- Maturana, H. & Varela, F. (1972/1980). *Autopoiesis and Cognition: The Realization of the Living*. Reidel.
- Letelier, J.C., Marin, G., & Mpodozis, J. (2003). "Autopoietic and (M,R) systems." *Journal of Theoretical Biology*, 222(2), 261-272.
- Cornish-Bowden, A. & Cardenas, M.L. (2020). "Contrasting theories of life: historical context, current theories." *Biosystems*, 188, 104063.
- Letelier, J.C., Soto-Andrade, J., Guinez, F., Cornish-Bowden, A., & Cardenas, M.L. (2006). "Organizational invariance and metabolic closure." *Journal of Theoretical Biology*, 238(4), 949-961.

---

## 4. Computational Autocatalysis

### 4.1 Fontana's AlChemy (Algorithmic Chemistry)

The most theoretically significant computational model of autocatalytic organization is Walter Fontana and Leo Buss's **AlChemy** (1994, 1996), which uses the lambda calculus as a substrate for artificial chemistry.

**Setup:**
- "Molecules" are lambda expressions (programs in the lambda calculus).
- A "reaction" consists of one expression being *applied* to another (function application), with the result reduced to normal form (beta-reduction).
- The system is a **"Turing Gas"**: a well-stirred reactor of lambda expressions that randomly collide. Each collision applies one expression to another, producing a new expression.
- Expressions that produce no normal form (non-terminating computations) are discarded. A flow reactor maintains population size by removing random expressions when new ones are added.

**Key Results:**

**Level 0 Organizations:** Self-copying expressions (fixed points under application) rapidly dominate the reactor. These are analogous to selfish replicators -- trivial and uninteresting. They represent identity functions and similar copiers that, when applied to anything, reproduce themselves.

**Level 1 Organizations:** When self-copiers are explicitly prohibited (forbidden from the reactor), the system produces **algebraically closed, self-maintaining organizations**. A Level 1 organization is a set of lambda expressions such that applying any two expressions in the set yields another expression in the set. This is precisely an autocatalytic set: the set collectively produces itself through mutual interaction. These organizations are stable under perturbation -- removing members triggers regeneration from remaining members.

**Level 2 Organizations:** Mixing distinct Level 1 organizations can produce higher-order interactions: one organization can "invade" or "merge" with another, producing composite organizations. This provides a primitive form of ecology and evolution at the level of organizations rather than individual molecules.

**Significance:** AlChemy demonstrates that autocatalytic closure is not specific to chemistry -- it arises naturally in any sufficiently rich constructive system. Lambda calculus, being Turing-complete, suggests that autocatalytic organization is a **universal feature of complex constructive systems**.

**2024 Reimplementation:** Mathis et al. (2024) reimplemented AlChemy with modern computing resources ("Return to AlChemy"), reproducing the original results and discovering several unanticipated features, including a surprising mix of dynamical robustness and fragility. An open-source implementation (LambdaReactor) is available on GitHub (gbaydin/LambdaReactor).

### 4.2 Chemical Organization Theory (Dittrich and colleagues)

Peter Dittrich, Jens Ziegler, and Wolfgang Banzhaf developed **Chemical Organization Theory (COT)** (2001-2007), which provides a general mathematical framework for analyzing artificial chemistries.

**Core concept:** An **organization** is a set of molecule types that is simultaneously:
- **Closed:** The set does not produce anything outside itself (all products of reactions among members are themselves members).
- **Self-maintaining:** Every molecule in the set can be regenerated by reactions within the set (no member is consumed without being replenished).

**Key insight:** Every stationary state of a chemical dynamical system (fixed point of the ODE system) corresponds to an organization. This bridges the gap between the structural/topological view (what *can* happen) and the dynamical view (what *does* happen).

**Relationship to RAF theory:** Organizations in COT are related to but distinct from RAF sets. COT requires stoichiometric self-maintenance (mass balance), while RAF theory requires only catalytic closure. COT organizations are "stronger" in that they account for consumption and production rates, not just catalytic topology.

### 4.3 Bagley and Farmer

Richard Bagley and J. Doyne Farmer (1991) conducted some of the earliest computational investigations of autocatalytic sets using a simplified polymer chemistry:

- Reactions restricted to catalyzed cleavage and condensation.
- Studied spontaneous emergence of autocatalytic sets from an initial set of simple building blocks.
- Demonstrated that when the initial set exceeds a critical diversity, autocatalytic reactions generate large molecular species in abundance.
- The critical diversity threshold was found to be modest, consistent with Kauffman's theoretical predictions.

### 4.4 Hutton's Artificial Chemistry

Tim Hutton (2002, 2007) developed **Squirm3**, a spatial artificial chemistry:

- 2D grid with simple physics and chemistry rules.
- Molecules are chains of atoms with bonding and reaction rules.
- Self-replicating molecules (similar to DNA) emerge spontaneously from a random soup given appropriate conditions.
- By 2007: self-reproducing cells with genetic material, enzymes, and physical division, demonstrating that autocatalytic organization can give rise to cell-like entities in a purely computational substrate.

### 4.5 Other Notable Computational Models

- **Virgo et al. (2021):** "Emergence of Self-Reproducing Metabolisms as Recursive Algorithms in an Artificial Chemistry" -- demonstrated that self-reproducing metabolisms can emerge as recursive algorithms, directly connecting computational recursion to metabolic autocatalysis.
- **Prebiotic Functional Programs (2025):** Explored endogenous selection in artificial chemistries, showing how functional programs emerge and self-maintain without external fitness criteria.
- **Hordijk et al. (2014-2015):** Evolution of autocatalytic sets in computational models of chemical reaction networks, studying how RAF sets grow, shrink, and evolve over time in dynamic simulations.

### Key References
- Fontana, W. & Buss, L. (1994). "The arrival of the fittest: Toward a theory of biological organization." *Bulletin of Mathematical Biology*, 56, 1-64.
- Fontana, W. & Buss, L. (1996). "The barrier of objects: From dynamical systems to bounded organizations." *Boundaries and Barriers*, Addison-Wesley.
- Dittrich, P., Ziegler, J., & Banzhaf, W. (2001). "Artificial chemistries -- a review." *Artificial Life*, 7(3), 225-275.
- Dittrich, P. & Speroni di Fenizio, P. (2007). "Chemical organisation theory." *Bulletin of Mathematical Biology*, 69(4), 1199-1231.
- Bagley, R. & Farmer, J.D. (1991). "Spontaneous emergence of a metabolism." *Artificial Life II*, Addison-Wesley.
- Hutton, T. (2002). "Evolvable self-replicating molecules in an artificial chemistry." *Artificial Life*, 8(4), 341-356.
- Mathis, C., Patarroyo, K., et al. (2024). "Self-organization in computation and chemistry: Return to AlChemy." *Chaos*, 34(9), 093142.

---

## 5. Relevance to Self-Improving AI Systems

### 5.1 The Core Analogy

The analogy between autocatalytic chemical networks and networks of AI agents is structurally precise:

| Chemical System | AI Agent System |
|---|---|
| Molecule types | Agent types / capabilities |
| Reactions | Transformation processes (training, fine-tuning, code generation) |
| Catalysis | One agent facilitating the production/improvement of another |
| Food set | Base resources (compute, data, base models, human-provided seed) |
| Catalytic closure | Every agent's production/improvement is facilitated by some agent in the system |
| RAF set | A self-sustaining ecosystem of agents that collectively produce and maintain all members |

### 5.2 AERA: Autocatalytic Endogenous Reflective Architecture

The most explicit application of autocatalytic set theory to AI architecture is **AERA** (Nivel and Thorisson, 2013), which defines recursive self-improvement systems as autocatalytic sets. AERA operates on three foundational principles:

1. **Autocatalysis:** The system creates the conditions for its own advancement. New models and capabilities catalyze the creation of further models and capabilities.
2. **Endogeny:** The system acts based on its own objectives, not just external instructions. Goals are internally generated and maintained.
3. **Reflectivity:** The system models itself, enabling meta-cognition and self-modification.

AERA implements value-driven dynamic priority scheduling controlling parallel execution of reasoning threads, achieving recursive self-improvement "after it leaves the lab, within the boundaries imposed by its designers."

**Key formalization:** Nivel and Thorisson propose that RSI (Recursively Self-Improving) systems *are* autocatalytic sets -- the system self-maintains and self-updates, with autonomy and reflectivity as essential properties for purposeful, goal-oriented operation.

### 5.3 The Darwin Godel Machine

Sakana AI's **Darwin Godel Machine (DGM)** (2025) is a concrete implementation of self-improving AI agents using evolutionary principles:

- The system iteratively modifies its own source code and validates changes against benchmarks.
- An archive of diverse agent variants grows over time (analogous to an expanding autocatalytic set).
- Agents are sampled from the archive and self-modify to create new variants (analogous to catalyzed reactions producing new molecules).
- Performance improved from 20% to 50% on SWE-bench through open-ended self-improvement.

While the DGM does not explicitly cite autocatalytic set theory, its architecture is structurally analogous: a population of agents collectively enables the production of improved agents, with the archive serving as a self-sustaining reservoir.

### 5.4 Conditions for Emergence (Translating RAF Theory)

RAF theory's conditions for autocatalytic set emergence translate to AI system design constraints:

1. **Sufficient diversity (the phase transition):** The system needs enough distinct agent types/capabilities that the catalytic network becomes dense enough for closure. In RAF theory, the critical threshold is modest (linear in system complexity). Implication: a relatively small number of well-chosen agent specializations may suffice for self-sustaining collective improvement.

2. **Food set availability:** Base resources must be continuously available -- compute, training data, base model weights, human-provided objectives. These are the "food molecules" from which all agent capabilities are ultimately constructed.

3. **Catalytic connectivity:** Each agent must be "catalyzable" by at least one other agent -- every capability must be improvable by some existing capability. This is a design constraint: avoid orphan capabilities that no other agent can enhance.

4. **The maxRAF/irrRAF structure:** A self-improving AI ecosystem will likely have hierarchical structure -- large self-sustaining clusters containing smaller irreducible cores. The irrRAFs represent minimal viable self-improving subsystems. Design implication: start with a minimal irrRAF (the smallest self-sustaining set of mutually improving agents) and grow from there.

### 5.5 Connections to Fixed-Point Theory and Self-Reference

The mathematical connections run deep:

- **Fixed points:** A self-sustaining autocatalytic set is a fixed point of the "closure operator" on reaction sets. A self-improving AI system that maintains its own improvement capability is a fixed point of a meta-learning operator.
- **Kleene's recursion theorem:** Any sufficiently powerful computational system contains programs that can compute their own description. This is the computational analog of self-reference in (M,R)-systems.
- **Fontana's Level 1 organizations** (algebraically closed sets of lambda expressions) are literally fixed points of a closure operator in a Turing-complete space -- suggesting that autocatalytic closure is an attractor in any sufficiently rich computational system.

### 5.6 Open-Ended Evolution and Artificial Life

The connection to open-ended evolution research is direct:

- **Open-ended evolution** seeks systems that continually produce novel, increasingly complex entities without reaching a fixed point. Autocatalytic sets provide the *minimal self-sustaining structure* from which open-ended evolution can launch.
- **Gabora's cognitive RAF models** (2017, 2020, 2022) show that RAF theory can model the origin of cumulative cultural innovation -- mental representations as "molecules," creative associations as "reactions," and existing knowledge as "catalysts." An AI system whose agents' outputs catalyze new agent creation follows the same formal structure.
- **The economy as autocatalytic set:** Hordijk et al. showed that economic innovation networks can be modeled as autocatalytic sets, where technologies catalyze the creation of new technologies. An AI agent ecosystem is a technology-producing network and should exhibit similar dynamics.

### 5.7 Design Principles for Autocatalytic AI Systems

Synthesizing across the theoretical landscape:

1. **Achieve catalytic closure first.** Before pursuing open-ended improvement, ensure that every agent's production/improvement is catalyzed by some existing agent. This is the minimal viability condition (the irrRAF).

2. **Maintain a food set.** Continuously provide base resources. The system cannot bootstrap from nothing -- it needs a substrate of compute, data, and seed capabilities.

3. **Engineer for the phase transition.** RAF theory shows that modest catalytic connectivity suffices. Focus on ensuring each agent capability can improve at least 1-2 others, and the collective will likely achieve closure.

4. **Exploit hierarchical structure.** The maxRAF/irrRAF decomposition suggests building modular, hierarchically organized agent ecosystems where subsystems are independently self-sustaining.

5. **Distinguish catalytic closure from autopoietic closure.** Catalytic closure (RAF) is necessary but may not be sufficient. For a truly autonomous system, closure to efficient causation (Rosen) -- where the system produces its own "repair machinery" -- may be needed. This means agents that not only improve each other but improve the improvement process itself.

6. **Beware Level 0 attractors.** Fontana's AlChemy shows that trivial self-copiers (Level 0) dominate unless actively suppressed. In AI systems, this corresponds to degenerate solutions (agents that simply copy themselves without improvement). Active selection pressure against degeneracy is needed to reach Level 1 (genuine collective autocatalysis).

7. **Build in reflectivity.** Following AERA: the system must model itself. Without self-models, the system cannot identify which components need repair/improvement, and closure to efficient causation cannot be achieved.

### Key References
- Nivel, E. & Thorisson, K.R. (2013). "Bounded recursive self-improvement." arXiv:1312.6764.
- Sakana AI (2025). "Darwin Godel Machine: Open-Ended Evolution of Self-Improving Agents." arXiv:2505.22954.
- Gabora, L. & Steel, M. (2017). "Autocatalytic networks in cognition and the origin of culture." *Journal of Theoretical Biology*, 431, 87-95.
- Gabora, L. & Steel, M. (2020). "Modeling a cognitive transition at the origin of cultural evolution using autocatalytic networks." *Cognitive Science*, 44(9), e12878.
- Hordijk, W., Kauffman, S., & Steel, M. (2011). "Required levels of catalysis for emergence of autocatalytic sets." *International Journal of Molecular Sciences*, 12(5), 3085-3101.

---

## 6. Synthesis: Key Takeaways

1. **Autocatalytic sets are a general organizational principle**, not specific to chemistry. They emerge in any constructive system (chemical, computational, economic, cognitive) when catalytic connectivity exceeds a modest threshold.

2. **RAF theory provides rigorous, algorithmic tools** for detecting and analyzing autocatalytic closure. The polynomial-time RAF algorithm, the maxRAF/irrRAF decomposition, and the phase transition results are directly applicable to designing self-sustaining agent systems.

3. **Three levels of closure** form a hierarchy of self-sufficiency:
   - **Catalytic closure (RAF):** Every process has an internal catalyst. Necessary for self-sustenance.
   - **Organizational closure (autopoiesis):** The system produces its own boundary and all components. Necessary for autonomy.
   - **Closure to efficient causation (Rosen):** The system produces its own "builders." Necessary for genuine self-improvement.

4. **Fontana's AlChemy proves** that autocatalytic closure arises spontaneously in Turing-complete constructive systems, suggesting it may be an inevitable attractor in sufficiently rich AI agent ecosystems.

5. **The critical design challenge** for self-improving AI is not achieving any single form of closure but achieving *nested closure*: agents that improve agents that improve the improvement process -- the computational analog of Rosen's closed causal loop f -> beta -> Phi -> f.

---

## Sources

- [Autocatalytic set - Wikipedia](https://en.wikipedia.org/wiki/Autocatalytic_set)
- [A History of Autocatalytic Sets (Hordijk, 2019)](https://link.springer.com/article/10.1007/s13752-019-00330-w)
- [Collectively autocatalytic sets (Cell Reports, 2023)](https://www.sciencedirect.com/science/article/pii/S2666386423004022)
- [Autocatalytic Networks at the Basis of Life's Origin (MDPI, 2018)](https://www.mdpi.com/2075-1729/8/4/62)
- [Exploring the origins of life with autocatalytic sets (Research Outreach)](https://researchoutreach.org/articles/exploring-origins-life-autocatalytic-sets/)
- [A Concise and Formal Definition of RAF Sets (Hordijk, 2023)](https://arxiv.org/pdf/2303.01809)
- [Autocatalytic networks in biology: structural theory and algorithms (Royal Society, 2019)](https://royalsocietypublishing.org/doi/10.1098/rsif.2018.0808)
- [Self-generating autocatalytic networks (Royal Society, 2024)](https://royalsocietypublishing.org/rsif/article/21/214/20230732/90515/Self-generating-autocatalytic-networks-structural)
- [Closure to efficient causation, computability and artificial life (Letelier et al.)](https://www.sciencedirect.com/science/article/abs/pii/S0022519309005360)
- [Robert Rosen (biologist) - Wikipedia](https://en.wikipedia.org/wiki/Robert_Rosen_(biologist))
- [Autopoietic and (M,R) systems (Letelier et al., 2003)](https://www.ias-research.net/wp-content/uploads/2018/02/letelier_autopoietic_mr.pdf)
- [Formal autopoiesis (ScienceDirect, 2023)](https://www.sciencedirect.com/science/article/abs/pii/S0303264723000473)
- [Self-Organization in Computation & Chemistry: Return to AlChemy (2024)](https://arxiv.org/html/2408.12137v1)
- [Chemical Organisation Theory (Dittrich, 2007)](https://link.springer.com/article/10.1007/s11538-006-9130-8)
- [Artificial Chemistries - A Review (Dittrich et al., 2001)](https://direct.mit.edu/artl/article-abstract/7/3/225/2373/Artificial-Chemistries-A-Review)
- [Bounded Recursive Self-Improvement (Nivel & Thorisson, 2013)](https://arxiv.org/abs/1312.6764)
- [Darwin Godel Machine (Sakana AI, 2025)](https://sakana.ai/dgm/)
- [Autocatalytic networks in cognition and the origin of culture (Gabora & Steel, 2017)](https://www.sciencedirect.com/science/article/pii/S0022519317303533)
- [Modeling a Cognitive Transition (Gabora & Steel, 2020)](https://onlinelibrary.wiley.com/doi/10.1111/cogs.12878)
- [Autopoiesis - Wikipedia](https://en.wikipedia.org/wiki/Autopoiesis)
- [Autocatalytic Sets and the Origin of Life (MDPI, 2010)](https://www.mdpi.com/1099-4300/12/7/1733)
- [Autocatalytic Sets: From the Origin of Life to the Economy (BioScience, 2013)](https://academic.oup.com/bioscience/article/63/11/877/2389920)
