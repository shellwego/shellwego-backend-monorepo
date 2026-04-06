# ShellWeGo Gap Remediation — Master Index

**Date**: 2026-04-06
**Based on**: `shellwego-gap-analysis-report.md` (post-merge `main`, commit `f1d9f99`)
**Overall Parity**: ~38%

---

## Plan Overview

| # | Plan File | Title | Complexity | Crates Touched | Status |
|---|-----------|-------|------------|----------------|--------|
| 01 | `01-security-hardening.plan.md` | Security Hardening (JWT, RBAC, KMS, Audit, Supply Chain) | XL | control-plane | ✅ Done |
| 02 | `02-scheduler-deploy-guardian.plan.md` | Scheduler, Deploy Pipeline & Guardian | XL | control-plane, schema | ✅ Done |
| 03 | `03-quic-message-bus.plan.md` | QUIC Message Bus | L | network, schema | ⏳ Pending |
| 04 | `04-agent-activation.plan.md` | Agent Runtime Activation | XL | agent, network, schema, storage, registry | ⏳ Pending |
| 05 | `05-edge-proxy-enhancements.plan.md` | Edge Proxy Enhancements | L | edge | ⏳ Pending |
| 06 | `06-cli-completion.plan.md` | CLI Tool Completion | L | cli | ⏳ Pending |
| 07 | `07-billing-real-integration.plan.md` | Billing & Real Payment Integration | XL | billing, schema | ⏳ Pending |
| 08 | `08-observability-dashboards.plan.md` | Observability Enhancement (Dashboards, Alerts, Metrics) | L | observability, agent | ⏳ Pending |
| 09 | `09-registry-dragonfly.plan.md` | Registry & Dragonfly Distribution | L | registry, storage, schema | ⏳ Pending |
| 10 | `10-storage-provisioning.plan.md` | Storage Volume Provisioning & Encryption | L | storage, control-plane, agent | ⏳ Pending |
| 11 | `11-infrastructure-deployment.plan.md` | Infrastructure, Deployment & Dashboard | XXL | root, charts, scripts, new frontend | ⏳ Pending |

---

## Dependency Graph

```
                    ┌──────────────────────────────┐
                    │  Plan 01: Security Hardening  │
                    │  (should run FIRST)           │
                    └──────────┬───────────────────┘
                               │
              ┌────────────────┼────────────────┐
              │                │                │
              ▼                ▼                ▼
   ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐
   │  Plan 02:    │  │  Plan 03:    │  │  Plan 11:        │
   │  Scheduler   │──│  QUIC Bus    │  │  Infra/Deploy    │
   │  & Guardian  │  │  (parallel)  │  │  & Dashboard     │
   └──────┬───────┘  └──────┬───────┘  └──────────────────┘
          │                 │
          ▼                 ▼
   ┌──────────────┐  ┌──────────────┐
   │  Plan 04:    │  │  Plan 08:    │
   │  Agent       │  │  Observab.   │
   │  Activation  │  │  (parallel)  │
   └──┬───────┬───┘  └──────────────┘
      │       │
      ▼       ▼
┌──────────┐ ┌──────────────────┐
│ Plan 09: │ │  Plan 10:        │
│ Registry │ │  Storage         │
│ Dragonfly│ │  Provisioning    │
└──────────┘ └──────────────────┘

   ┌──────────────┐  ┌──────────────┐
   │  Plan 05:    │  │  Plan 06:    │  ┌──────────────┐
   │  Edge Proxy  │  │  CLI Tool    │  │  Plan 07:    │
   │  (parallel)  │  │  (parallel)  │  │  Billing     │
   └──────────────┘  └──────────────┘  │  (parallel)  │
                                       └──────────────┘
```

---

## Dependency Matrix

| Plan | Depends On | Blocks |
|------|-----------|--------|
| **01** Security | None | 02, 04, 06, 11 |
| **02** Scheduler | 01, 03 | 04 |
| **03** QUIC Bus | None | 02, 04, 06 |
| **04** Agent | 01, 02, 03, 09, 10 | — |
| **05** Edge Proxy | None | — |
| **06** CLI | 01, 03 | — |
| **07** Billing | None | — |
| **08** Observability | None | — |
| **09** Registry | None | 04 |
| **10** Storage | None | 04 |
| **11** Infra/Deploy | 01 | — |

---

## Recommended Execution Waves

### Wave 0 — Foundation (sequential, must go first)
1. **Plan 01** — Security Hardening (JWT RS256, RBAC enforcement, real AES-256-GCM KMS, audit logging)

### Wave 1 — Core Infrastructure (parallel after Wave 0)
2. **Plan 03** — QUIC Message Bus (no dependencies)
3. **Plan 05** — Edge Proxy Enhancements (no dependencies)
4. **Plan 07** — Billing & Real Payment Integration (no dependencies)
5. **Plan 08** — Observability Enhancement (no dependencies)
6. **Plan 09** — Registry & Dragonfly Distribution (no dependencies)
7. **Plan 10** — Storage Volume Provisioning (no dependencies)
8. **Plan 11** — Infrastructure & Deployment (depends only on Plan 01)

### Wave 2 — Platform Core (sequential, after Wave 1 completes)
9. **Plan 02** — Scheduler, Deploy Pipeline & Guardian (needs 01 + 03)

### Wave 3 — Runtime (after Wave 2)
10. **Plan 04** — Agent Runtime Activation (needs 01 + 02 + 03 + 09 + 10)

### Wave 4 — Polish (anytime)
11. **Plan 06** — CLI Tool Completion (needs 01 + 03 for end-to-end testing)

---

## Build Status Reference

| Crate | Build Status | Errors | Warnings |
|-------|-------------|--------|----------|
| shellwego-control-plane | ❌ FAILED | 22 | 33 |
| shellwego-agent | ❌ FAILED | 5 | 6 |
| shellwego-edge | ❌ FAILED | 72 | 16 |
| shellwego-billing | ❌ FAILED | 4 | 3 |
| shellwego-observability | ❌ FAILED | 17 | 2 |
| shellwego-cli | ❌ FAILED | 96 | 11 |
| shellwego-registry | ❌ FAILED (old) / ✅ (new) | 3→0 | 7→2 |
| shellwego-network | ⚠️ PASSED | 0 | 7 |
| shellwego-storage | ✅ PASSED | 0 | 0 |
| shellwego-firecracker | ✅ PASSED | 0 | 0 |
| shellwego-schema | ✅ PASSED | 0 | 0 |

**Note**: Each plan includes a "Phase 0" or "Prerequisites" section to fix the build errors in its target crate before making changes.

---

## Estimated Total Effort

| Plan | Production LOC | Test LOC | Config/JSON LOC |
|------|---------------|----------|-----------------|
| 01 | ~490 | ~150 | — |
| 02 | ~1,100 | ~200 | — |
| 03 | ~1,350 | ~550 | — |
| 04 | ~820 | ~200 | — |
| 05 | ~1,520 | ~300 | — |
| 06 | ~1,130 | ~80 | — |
| 07 | ~2,880 | ~400 | — |
| 08 | ~210 | ~100 | ~1,700 (YAML+JSON) |
| 09 | ~1,400 | ~350 | — |
| 10 | ~990 | ~150 | — |
| 11 | ~1,430 | ~100 | ~200 |
| **Total** | **~13,320** | **~2,580** | **~1,900** |

---

## How to Use These Plans

1. Each `.plan.md` file is **self-contained** — an AI agent can execute it independently.
2. Follow the **dependency graph** and **execution waves** for correct ordering.
3. Each plan has a **Prerequisites** section that must be completed before implementation.
4. Each plan has **Acceptance Criteria** — verify completion before moving on.
5. Run `cargo check -p <crate>` after each plan to verify no regressions.
6. Run `cargo test -p <crate>` where applicable.
