# shellwego-billing - Build Check Summary

**Date:** 2026-03-13  
**Status:** ❌ FAILED

## Errors (4)

### Ownership/Lifetime Issues
1. **E0382:** Borrow of moved value: `events`
   - Value was moved then borrowed

2. **E0515:** Cannot return value referencing function parameter `entry` (2 occurrences)
   - Cannot return reference to function parameter

3. **E0624:** Method `load_from_glob` is private
   - Attempting to call private method

## Warnings (3)
1. Unused import: `warn`
2. Unused imports: `InvoiceStatus`, `LineItem`
3. Unused variable: `body`

## Recommendations
1. Fix ownership issues with `events` - clone or reorder operations
2. Refactor code returning references to `entry` - return owned values instead
3. Make `load_from_glob` public or use alternative approach
4. Remove unused imports and variables
