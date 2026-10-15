"""P2 sensitivity (descriptive; the registered pooled result stays primary).
Grouping below was written before computing anything."""
import json, random, sys
GROUPS = {
    "codecs-parsers": ["csv-quote", "json-pointer", "json-lexer", "toml-subset", "ini-config", "url-parse",
                       "http-headers", "email-address", "varint-codec", "base32-codec", "checksums",
                       "iso-duration", "numeral-convert"],
    "text-format": ["text-wrap", "line-diff", "template-engine", "pretty-printer", "glob-match", "row-filter"],
    "data-structures": ["bitset-ops", "indexed-heap", "union-groups", "rope-text", "sparse-matrix", "rational-matrix"],
    "graphs-algorithms": ["graph-paths", "scc-graph", "knapsack-dp", "key-maze", "life-rules"],
    "state-sim-finance": ["bank-ledger", "money-format", "bowling-score", "vending-fsm", "bank-queue-sim",
                          "cron-next", "fs-sim"],
}
group_of = {f: g for g, fs in GROUPS.items() for f in fs}
def load(path):
    out = {}
    for line in open(path):
        r = json.loads(line)
        if r["kind"] == "score":
            out[(r["arm"], r["task"])] = (r["family"], r["result"]["all_passed"])
    return out
pairs = []
for rep, path in enumerate(sys.argv[1:3]):
    d = load(path)
    for (arm, task), (fam, ok) in d.items():
        if arm == "C":
            pairs.append((fam, task, rep, ok, d[("D", task)][1]))
assert set(group_of) >= {p[0] for p in pairs}, {p[0] for p in pairs} - set(group_of)
def boot(pairs, key, iters=10000, seed=1):
    clusters = {}
    for p in pairs:
        clusters.setdefault(key(p), []).append(p)
    ids = list(clusters); rng = random.Random(seed); stats = []
    for _ in range(iters):
        n = x = y = 0
        for _ in ids:
            c = clusters[ids[rng.randrange(len(ids))]]
            n += len(c); x += sum(p[3] for p in c); y += sum(p[4] for p in c)
        stats.append((x - y) / n)
    stats.sort()
    lift = (sum(p[3] for p in pairs) - sum(p[4] for p in pairs)) / len(pairs)
    return lift, stats[int(0.025 * (iters - 1))], stats[-(-int(0.975 * (iters - 1)) // 1)]
lift, lo, hi = boot(pairs, lambda p: p[0])
print(f"family clusters (37): lift {lift:+.3f} CI [{lo:+.3f}, {hi:+.3f}]")
lift, lo, hi = boot(pairs, lambda p: group_of[p[0]])
print(f"domain clusters (5):  lift {lift:+.3f} CI [{lo:+.3f}, {hi:+.3f}]")
fams = sorted({p[0] for p in pairs}); lofo = []
for f in fams:
    rest = [p for p in pairs if p[0] != f]
    lofo.append(((sum(p[3] for p in rest) - sum(p[4] for p in rest)) / len(rest), f))
lofo.sort()
print("leave-one-family-out lift range:", f"{lofo[0][0]:+.3f} (without {lofo[0][1]})", "to", f"{lofo[-1][0]:+.3f} (without {lofo[-1][1]})")
