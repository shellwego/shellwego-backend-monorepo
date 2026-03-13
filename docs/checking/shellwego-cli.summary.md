# shellwego-cli - Build Check Summary

**Date:** 2026-03-13  
**Status:** ❌ FAILED

## Errors (96)

### Module Resolution
- **E0433:** Failed to resolve: use of unresolved module or unlinked crate `uuid` (multiple occurrences)
  - The `uuid` crate is not directly imported but used in code

## Warnings (11)
- Multiple warnings related to unresolved `uuid` crate usage

## Recommendations
1. Add `uuid` to direct dependencies in `Cargo.toml`
2. Or import via `shellwego_schema::uuid` if re-exported
