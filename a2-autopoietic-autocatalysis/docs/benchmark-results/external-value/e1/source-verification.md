# E1 source verification: SWE-Bench Pro public split on this machine

**Date:** 2026-10-14
**Scope:** primary sources only: HF dataset card, files and API; upstream GitHub harness; GHCR and Docker Hub registry APIs (manifests and config blobs only, no layers pulled); read-only local checks. No images were pulled and no model CLIs were run.

## Pinned revisions examined

| Artifact | Revision |
|---|---|
| HF dataset `ScaleAI/SWE-bench_Pro` | commit `2d52cb3df914a3fcf80c7f66738b3a88ae37fc50` (lastModified 2026-09-22T18:44:38Z) |
| V2 parquet `data/default/test-00000-of-00001.parquet` | 6,511,689 B, sha256 `1067f72c1f43bbc578276888625db3b836cebb535546c9eaf824c5d5dd104a7f` (matches the LFS oid) |
| Harness `scaleapi/SWE-bench_Pro-os` | `main` @ `66f92766bba642462d4bbe5479e83f91f9211862` (2026-09-22, "SWE-bench Pro V2 (v2.0.0)") |
| Harbor (`laude-institute/harbor`, now redirects to `harbor-framework/harbor`) | `main` @ `2821487da7415a9bc328d8cc930e7b3181df4e61` |

URLs:
- https://huggingface.co/datasets/ScaleAI/SWE-bench_Pro (card), https://huggingface.co/api/datasets/ScaleAI/SWE-bench_Pro (API)
- https://huggingface.co/datasets/ScaleAI/SWE-bench_Pro/resolve/2d52cb3df914a3fcf80c7f66738b3a88ae37fc50/README.md
- https://github.com/scaleapi/SWE-bench_Pro-os/blob/66f92766bba642462d4bbe5479e83f91f9211862/v2/README.md
- https://github.com/scaleapi/SWE-bench_Pro-os/blob/66f92766bba642462d4bbe5479e83f91f9211862/v2/GATE.md
- https://scale.com/leaderboard/swe_bench_pro_public, https://scaleapi.github.io/SWE-bench_Pro-os/
- https://ghcr.io/v2/scaleapi/swe-bench_pro-v2/manifests/<instance_id> (anonymous token from `https://ghcr.io/token?scope=repository:scaleapi/swe-bench_pro-v2:pull`)
- https://hub.docker.com/v2/repositories/jefzda/sweap-images/tags?page_size=5 (legacy v1 images only)

## Headline: what changed since the brief was written

The public split now has a **V2** (released 2026-09-22), and it is the HF `default` config. It has **642 tasks**, down from 731; the `hard` config is a 51-task subset; v1 (731) survives as config `v1` / git tag `v1.0`. V2 images moved from Docker Hub (`jefzda/sweap-images`) to **GHCR** (`ghcr.io/scaleapi/swe-bench_pro-v2:<instance_id>`). The official V2 runner is **Harbor**, and its documented examples use Modal (`-e modal`). Harbor also ships a local `docker` environment, but this has **not been verified** for this benchmark. Calibration numbers published so far are **all v1**.

## 1. Dataset

| Claim | Source | Verified? |
|---|---|---|
| HF id `ScaleAI/SWE-bench_Pro`, not gated (`gated:false`) | HF API | Yes |
| Configs: `default` = V2, 642 rows; `hard` = 51; `v1` = 731 | card YAML + parquet row count | Yes (642 counted locally) |
| Languages (V2): go 256, python 237, js 145, ts 4 | parquet | Yes |
| Fields: `repo, instance_id, base_commit, patch, test_patch, problem_statement, requirements, interface, repo_language, fail_to_pass, pass_to_pass, issue_specificity, issue_categories, before_repo_set_cmd, selected_test_files_to_run, dockerhub_tag, docker_image, hard, version`; list fields are strict JSON strings | card + parquet | Yes |
| License: harness/tooling MIT; task content "subject to [the 11 upstream repos'] respective licenses". There is no separate dataset license field. | card "License" section; GitHub license = MIT | Yes. Upstream repos sampled: ansible GPL-3.0, navidrome GPL-3.0, vuls GPL-3.0, teleport AGPL-3.0, flipt NOASSERTION (GitHub API) |
| Dates: HF says `v1.0` was published 2026-02-23. Other sources point to Aug/Sep 2025: HF repo `_id` timestamp 2025-08-26, Docker Hub `jefzda/sweap-images` registered 2025-08-27, GitHub repo created 2025-09-05, arXiv 2509.16941. | various | **Discrepancy, unresolved.** Most likely the 2026-02-23 "v1.0" tag froze a later revision of the 731 set (README news 2/9: "removed some unit tests which were outdated"). Not confirmed. |

V2 per-repo breakdown (from parquet; P2P = tasks with a non-empty `pass_to_pass`):

| repo | lang | n | with P2P | HARD-51 |
|---|---|---|---|---|
| ansible/ansible | python | 82 | 62 | 8 |
| internetarchive/openlibrary | python | 82 | 59 | 8 |
| qutebrowser/qutebrowser | python | 73 | 67 | 4 |
| flipt-io/flipt | go | 78 | **2** | 7 |
| gravitational/teleport | go | 68 | 12 | 6 |
| future-architect/vuls | go | 58 | 11 | 4 |
| navidrome/navidrome | go | 52 | **1** | 3 |
| protonmail/webclients | js | 55 | 32 | 5 |
| element-hq/element-web | js | 52 | 46 | 4 |
| NodeBB/NodeBB | js | 38 | 36 | 1 |
| tutao/tutanota | ts | 4 | 1 | 1 |

Median F2P / P2P counts: python 4 / 14, go 3 / 0, js 5 / 30. **Go tasks are graded almost entirely on F2P, with no regression check.** That changes what "resolved" means for each language.

## 2. Harness and images

- **Harness:** https://github.com/scaleapi/SWE-bench_Pro-os. V2 ships 642 Harbor task directories at `v2/tasks/<instance_id>/` (`instruction.md`, `task.toml`, `environment/Dockerfile` = `FROM ghcr.io/scaleapi/swe-bench_pro-v2:<id>`, `solution/`, `tests/{test.sh,run_script.sh,parser.py,config.json,test_patch.patch}`), plus `v2/tooling/` (locked agents for Claude Code, Codex and mini-swe-agent; `patch_replay.py` re-grader; network and git-history probes) and `v2/SHA256SUMS`. The v1 pipeline (`swe_bench_pro_eval.py`, which has a `--use_local_docker` mode, plus Docker Hub images) is kept for v1 only.
- **Documented run:** `harbor run -p v2/tasks -e modal ...` (Harbor >= 0.22, Modal >= 1.5.1). Harbor's tree has `src/harbor/environments/docker/` with `docker-compose-no-network.yaml` and an egress-control sidecar (allowlist). That sidecar is enabled only if `_egress_control_kernel_support()` passes. **Not verified on Colima.**
- **Image naming:** `ghcr.io/scaleapi/swe-bench_pro-v2:<instance_id>`, e.g. `...:instance_ansible__ansible-0ea40e09d1b35bcb69ff4d9cecf3d0defa4b36e8-v30a923fb5c164d6cd18280c02422f75e611e8fb2`. Anonymous pull worked (token request + manifest fetch).
- **Architecture: amd64 only. Verified for all 642** by fetching every config blob, which gave `architecture: amd64` ×642. 329 tags are OCI indexes containing only `linux/amd64` plus an attestation manifest; 313 are single manifests. There is no arm64 variant.
- **Per-task resources (task.toml):** 1 CPU, 4096 MB RAM, 10240 MB storage, agent timeout 3000 s, verifier timeout 3000 s, build timeout 1800 s.
- **Compressed sizes (all 642 V2 manifests, sum of layer sizes):**

| repo | median MB | p10 | p90 | sum GB (no dedup) | unique-layer GB |
|---|---|---|---|---|---|
| qutebrowser | 717 | 601 | 984 | 52.4 | 27.7 |
| ansible | 743 | 689 | 757 | 60.2 | 25.9 |
| NodeBB | 946 | 868 | 993 | 35.6 | 18.8 |
| navidrome | 1046 | 948 | 1150 | 54.5 | 29.9 |
| openlibrary | 1062 | 860 | 1292 | 87.1 | 47.9 |
| element-web | 1337 | 1249 | 1513 | 71.3 | 27.2 |
| vuls | 1432 | 992 | 1555 | 78.7 | 53.0 |
| tutanota | 1466 | 1210 | 1580 | 5.7 | 4.6 |
| flipt | 1801 | 609 | 2089 | 117.7 | 85.8 |
| teleport | 2706 | 2452 | 3413 | 191.8 | 82.5 |
| protonmail/webclients | 5247 | 4862 | 5929 | 296.2 | 142.3 |
| **total** | | | | **1051** | |

  Layer sharing across a whole repo is small (at most about 3 GB of layers are common to at least half of a repo's images), so caching base layers saves little. Each image is effectively its own pull.
- **Uncompressed size: NOT verified** (that would need a pull). No multiplier is asserted here.
- **Legacy v1 Docker Hub** (`jefzda/sweap-images`, 1002 tags, amd64 only). `full_size` (compressed) for 5 tags: element-web 1.02 GB and 1.44 GB, teleport 2.34 GB, ansible 0.53 GB, flipt 1.77 GB. These are **not** the V2 images.

## 3. How one instance is graded

`tests/test.sh` (identical template across tasks; read in full for ansible) does the following:
1. `cd /app` (or `/testbed`), purge `__pycache__`/`.pyc`, remove `.git/index.lock`.
2. **Revert every agent edit to test code**: anything under `tests/ test/ __tests__/ spec/`, `conftest.py`, `*_test.*`, `test_*.py`, `*.test|spec.*`. It also reverts edits under fixture/mock/snapshot directories and deletes files the test patch would create.
3. `git apply --verbose /tests/test_patch.patch`.
4. `bash /tests/run_script.sh "<selected_test_files_to_run joined by ,>"` → `parser.py` → `/tmp/output.json`.
5. **Resolved** iff `fail_to_pass ∪ pass_to_pass ⊆ {tests with status PASSED}`. The outcome is written to `/logs/verifier/reward.txt` (1/0).

Examples: Go runs `go test -v -run '^(T1|T2)$' ./pkg...`; qutebrowser runs pytest under Xvfb + dbus; ansible runs `bin/ansible-test units --requirements`; NodeBB runs `npm install` + redis + mocha.

**Network:** `task.toml` sets agent `network_mode = "no-network"` and verifier/environment `network_mode = "public"`. The V2 README says "a few Go tasks fetch modules at test time". Confirmed needing network at verify time: **NodeBB** (`npm install` in `run_script.sh`). Ansible probably does **not** need it, despite `ansible-test --requirements`: the image's `build.sh` sets `pip config set global.index-url http://127.0.0.1:9876/` (pypi-timemachine), nothing starts that server at verify time, and oracle passed 642/642, so the deps must already be present. This is unverified without running. All other repos are **unconfirmed**.

**Visible/public test subset: none exists.** The agent sees only `instruction.md` (PR description + requirements + interface). The F2P/P2P names, `test_patch` and the gold `patch` are all **in the public HF dataset**, so "hidden" means hidden from the agent, not from the world. A self-built harness must keep those fields out of agent context and keep the agent offline, since HF is one fetch away. The only tests the agent can see are those already in the repo at `base_commit` (including the files behind P2P), and `test.sh` reverts any agent edits to them before grading. A "visible" signal would have to be constructed, e.g. by exposing P2P names. That does not work for Go: flipt has P2P on 2/78 tasks and navidrome on 1/52.

## 4. Contamination

- The fixing commits are public upstream commits. Spot-checked through the GitHub API (all exist): ansible `0ea40e09` (2023-09-19), flipt `02e21636` (2024-05-24), NodeBB `00c70ce7` (2024-06-10), qutebrowser `01d1d149` (2020-12-19), navidrome `0130c6dc` (2022-12-28). In a random sample of 60 V2 tasks, fix dates run from 2019-12 to 2025-05 (2019:1, 2020:3, 2021:11, 2022:8, 2023:21, 2024:9, 2025:7). **Nearly all predate current model cutoffs.** Copyleft licensing is the only contamination defence Scale claims (leaderboard page).
- **Git history inside the image:** the V2 sanitisation layer (seen in the image config history for ansible and qutebrowser) bundles only `HEAD`'s ancestry, re-inits `.git`, runs gc, and asserts one ref, no remotes and `log --all == log HEAD`. The fixing commit, being a descendant, is absent from the visible filesystem. Caveat: the lower layers (pulled 2025-08-06 from Scale's ECR) still contain the pre-sanitisation `.git` in the layer tarballs. Those are whited out in the merged filesystem, so they are invisible to an agent inside the container but recoverable by anyone with the image tarball. Python deps were installed via `pypi-timemachine` pinned to the commit date.
- v1 images: the V2 README lists sanitised history ("no fixing commit, stray refs, stashes or hooks") as a V2 change. That implies v1 images were not sanitised; this has not been inspected directly. Use V2.

## 5. Calibration numbers (all v1, 730/731 tasks, not V2)

Scale public leaderboard (fetched 2026-10-14; 250-turn uncapped unless "grayed out"; `*` = mini-swe-agent, otherwise SWE-Agent): minimax-2.1 **36.81 ±3.55**, kimi-k2-instruct **27.67 ±3.25**, deepseek-v3p2 **15.56 ±2.63**, glm-4.6 **9.67 ±2.15**, qwen3-235b-a22b 21.41, gpt-oss-120b 16.20; top is Muse Spark 1.1* 61.50. GitHub Pages "initial runs" (SWE-Agent, 250 turns, uncapped): **glm-4.5 35.52**, kimi-k2-instruct 27.67.
- **Cannot confirm which rows were the grayed-out 50-turn capped runs.** The styling did not survive HTML extraction. glm-4.6 at 9.67 vs glm-4.5 at 35.52 suggests glm-4.6 may be a capped run. Unverified.
- README news (05/18): "We have identified some issues with the leaderboard and are currently working on addressing them."
- **No V2 scores found** for GLM/Kimi/MiniMax/DeepSeek. V2 README names GLM-5.3 and Kimi-K3 only as HARD-51 selector models, with no numbers.

## 6. Local machine reality (read-only checks, 2026-10-14)

| Check | Value | Implication |
|---|---|---|
| Free disk (`diskutil`, Data volume) | **13.0 GB** (df: 11 GiB, 98% full) | **Contradicts the "~58 GB free" premise.** `~/.colima` already uses 14 GB of a 20 GiB cap, so VM growth room is about 6 GB. |
| Host | `Apple M4 (Virtual)`, 6 CPU, 11 GB RAM | This macOS is itself a VM, so Colima vz is nested virtualisation |
| Colima `default` | **Stopped**; 2 CPU / 2 GiB / 20 GiB; `vmType: vz`, `rosetta: false`, `binfmt: true` | amd64 would run under QEMU user-mode emulation (slow; Go/Node test binaries are risky). 2 GiB is below task.toml's 4096 MB per task. 20 GiB VM disk with 13 GB host free. |
| Rosetta-for-Linux under nested virt | unknown | **Cannot verify read-only.** Gate #1 for any experiment. |

## 7. Subset recommendation (if gates pass)

- **Python, ansible** (80/82 images < 800 MB, 62/82 have P2P, no display stack; verifier probably offline-capable, see section 3) is the best fit. Next is **qutebrowser** (65/73 < 800 MB, 67/73 have P2P; needs Xvfb + dbus in container, which is fine). Together they give about 145 tasks at about 0.7 GB compressed each. **Headroom caveat:** the Colima disk is capped at 20 GiB and `~/.colima` already occupies 14 GB, with 13 GB free on the host. Effective room for the VM to grow is therefore **about 6 GB**, unless the existing VM disk holds prunable images; that could not be inspected because Colima is stopped. Gate #1 must include `colima start` + `docker system df` + prune (and possibly a resize to ≥4 GiB RAM). Then pull → run → `docker rmi` + prune per task, one image at a time.
- **Go:** flipt is bimodal (24 images < 800 MB), but it has almost no P2P and Go builds under QEMU are slow. navidrome is about 1 GB, with P2P on only 1/52 tasks. Avoid teleport (2.7 GB). JS/TS: avoid protonmail (5.2 GB); NodeBB (about 0.95 GB) needs network + redis at verify time.
- **Unmeasured:** per-instance wall-clock under emulation. Gate #2: run `-a oracle` and `-a nop` on 3 ansible + 3 qutebrowser tasks locally and confirm 1/0 rewards before pre-registering.

## Cannot confirm from primary sources

- Uncompressed image sizes; Harbor `-e docker` behaviour and egress control on Colima; Rosetta under nested virt; per-task runtime.
- Which leaderboard rows are capped runs; any V2 cheap-model numbers.
- Which repos besides NodeBB (and probably ansible) need verifier-phase network.
- Why HF dates v1.0 to 2026-02-23 when other sources date the release to Aug/Sep 2025.
