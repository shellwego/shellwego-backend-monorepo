# shellwego-control-plane - Build Check Summary

**Date:** 2026-03-13  
**Status:** ❌ FAILED

## Errors (22)

### API/Method Errors
1. **E0599:** No method `with_env_filter` found for `SubscriberBuilder`
   - Tracing subscriber API mismatch

### Type Mismatch
2. **E0308:** Mismatched types

### Ownership Errors
3. **E0382:** Borrow of moved value: `hostname`
4. **E0382:** Use of moved value: `queue.completed`
5. **E0382:** Use of partially moved value: `queue`

## Warnings (33)
- Unused variable: `app_id`
- Unused variable: `params`
- Unused variable: `cached`
- Unused variable: `instance`
- Unused variable: `secret`
- Unused variable: `encrypted`
- Unused variable: `timestamp`
- Unused variable: `merkle_tree`
- Additional warnings (25 more)

## Recommendations
1. Update tracing-subscriber to compatible version or fix API usage
2. Fix ownership issues with `hostname` and `queue`
3. Remove unused variables
