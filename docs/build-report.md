# Build Report — ShellWeGo Backend Monorepo

**Date**: 2026-04-05
**Rust toolchain**: stable 1.94.1 (e408947bf 2026-03-25)
**Command**: `cargo check -p <crate>` + `RUSTFLAGS="-W warnings" cargo check -p <crate>`

---

## Summary

| # | Crate | Errors | Warnings | Status |
|---|-------|--------|----------|--------|
| 1 | `shellwego-schema` | 0 | 0 | ✅ Clean |
| 2 | `shellwego-observability` | 0 | 0 | ✅ Clean |
| 3 | `shellwego-storage` | 0 | 0 | ✅ Clean |
| 4 | `shellwego-registry` | 0 | 2 | ⚠️ Warnings |
| 5 | `shellwego-network` | 0 | 4 | ⚠️ Warnings |
| 6 | `shellwego-firecracker` | 0 | 0 | ✅ Clean |
| 7 | `shellwego-edge` | 0 | 2 | ⚠️ Warnings |
| 8 | `shellwego-billing` | 0 | 2 | ⚠️ Warnings |
| 9 | `shellwego-agent` | 0 | 6 | ⚠️ Warnings |
| 10 | `shellwego-control-plane` | 0 | 124 | 🔴 Heavy dead code |
| 11 | `shellwego-cli` | 0 | 5 | ⚠️ Warnings |
| | **TOTAL** | **0** | **145** | |

**Build status**: All 11 crates compile without errors. 145 warnings total (mostly dead code).

---

## Per-Crate Detail

### shellwego-schema — ✅ Clean
No issues.

### shellwego-observability — ✅ Clean
No issues.

### shellwego-storage — ✅ Clean
No issues.

### shellwego-registry — ⚠️ 2 warnings
- Unused import: `futures_util::stream::StreamExt`
- Field `pool` is never read

### shellwego-network — ⚠️ 4 warnings
- Field `manager` is never read (rate_limiter module)
- Fields `packets_per_sec`, `burst`, `action` are never read (XdpConfig)
- Field `manager` is never read (ebpf module)
- Fields `direction`, `burst_bytes`, `priority` are never read (TcConfig)

### shellwego-firecracker — ✅ Clean
No issues.

### shellwego-edge — ⚠️ 2 warnings
- Fields `tls_manager` and `config` are never read (WebSocketProxy)
- Field `auto_renewal` is never read (CertificateManager)

### shellwego-billing — ⚠️ 2 warnings
- Field `flush_interval_secs` is never read (MeteringConfig)
- Unused `Result` that must be used (unhandled error)

### shellwego-agent — ⚠️ 6 warnings (own) + 4 (from shellwego-network dep)
Own warnings:
- Value assigned to `should_resume` is never read (2 occurrences — migration.rs)
- Field `base_dataset` is never read
- Function `is_pvm_available` is never used
- Function `estimate_pvm_memory_overhead` is never used
- Function `validate_pvm_config` is never used

### shellwego-control-plane — 🔴 124 warnings
The vast majority are **dead code** — structs, enums, traits, methods, and fields that are defined but never used. Key categories:

| Category | Count | Examples |
|----------|-------|---------|
| Structs never constructed | ~35 | `ServiceContext`, `HealthSummary`, `RateLimitStats`, `WebhookRouter`, `FederationCoordinator`, etc. |
| Methods never used | ~25 | `generate_password`, `provision_instance`, `list_instances`, `update_heartbeat`, etc. |
| Fields never read | ~30 | Config structs (`log_level`, `federation`, `build`, `kms`, `secret`, `issuer`, etc.) |
| Variants never constructed | ~20 | Database types, backup types, SSL modes, etc. |
| Enums never used | ~5 | `BackupError`, `CertificateError`, `RateLimitError`, `GitProvider`, `ResourceType` |
| Functions never used | ~5 | `log_request`, `default_page`, `default_per_page`, `load_from_file` |
| Traits never used | ~1 | `DatabaseOperator` |

This confirms the audit finding: the control-plane crate contains a large amount of scaffolding/scaffolded code that is not wired into actual functionality.

### shellwego-cli — ⚠️ 5 warnings
- Multiple methods never used
- Function `create_table` never used
- Struct `ResizeMessage` never constructed
- Struct `SignalMessage` never constructed
- Associated function `path` never used

---

## Future Incompatibility Notes

- `sqlx-postgres v0.7.4` contains code that will be rejected by a future version of Rust (reported during billing check)

## Global Warning

- `profiles for the non root package will be ignored, specify profiles at the workspace root` — appears on all crates; suggests some crate-level `[profile.*]` sections should be moved to the workspace `Cargo.toml`
