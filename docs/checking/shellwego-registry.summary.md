# shellwego-registry - Build Check Summary

**Date:** 2026-03-13  
**Status:** ❌ FAILED

## Errors (3)

### Import Errors
1. **E0432:** Unresolved import `bytes`
   - Missing `bytes` crate dependency

### Borrow Errors
2. **E0502:** Cannot borrow `index` as mutable because it is also borrowed as immutable
   - Simultaneous mutable and immutable borrows

### Type Annotations
3. **E0282:** Type annotations needed

## Warnings (7)
- Unused imports: `debug`, `error`
- Unused imports: `Descriptor`, `Platform`
- Unused import: `error`
- Unused imports: `ConfigDescriptor`, `LayerDescriptor`
- Unused import: `RegistryBackend`
- Unused import: `futures_util::stream::StreamExt`
- Unused variable: `is_first`

## Recommendations
1. Add `bytes` crate to dependencies
2. Fix borrow checker issues with `index`
3. Add type annotations where needed
4. Remove unused imports and variables
