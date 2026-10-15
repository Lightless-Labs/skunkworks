# Note on `summary.json` classes

This summary was produced before the classifier boundary fix (2026-10-15, `stats::classify` tolerance; LOG "Roadmap close").

Two exploratory comparisons have a CI upper bound of exactly 10.0 pp (8/80), which floating point stored as 0.09999999999999998. They are classed `RuledOut` here and are **Inconclusive** under the fixed classifier:
- C:i1 vs D;
- C:i1 vs C:u1.

No registered decision depended on either. The E12 go/no-go used the point estimate C(H*) − D = −1.2 pp against a +10 pp bar.
