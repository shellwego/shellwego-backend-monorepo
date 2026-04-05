//! Role-Based Access Control (RBAC)
//!
//! Provides functions for checking user permissions on protected endpoints.
//! Permissions follow the pattern: `resource:action` (e.g., `apps:read`).
//! Wildcard permissions like `admin:*` grant access to all resources.

use crate::auth::{has_permission, AuthError, CurrentUser};

/// Check if a user has a specific permission
///
/// This function is used by handlers and middleware to verify permissions.
pub fn check_permission(user: &CurrentUser, required: &str) -> Result<(), AuthError> {
    if has_permission(&user.permissions, required) {
        Ok(())
    } else {
        Err(AuthError::InsufficientPermissions {
            required: required.to_string(),
            have: user.permissions.join(", "),
        })
    }
}

/// Check if user has ANY of the required permissions
pub fn check_any_permission(user: &CurrentUser, required: &[&str]) -> Result<(), AuthError> {
    for perm in required {
        if has_permission(&user.permissions, perm) {
            return Ok(());
        }
    }
    Err(AuthError::InsufficientPermissions {
        required: required.join(" OR "),
        have: user.permissions.join(", "),
    })
}

/// Check if user has ALL of the required permissions
pub fn check_all_permissions(user: &CurrentUser, required: &[&str]) -> Result<(), AuthError> {
    for perm in required {
        check_permission(user, perm)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::UserRole;
    use uuid::Uuid;

    fn test_user(permissions: Vec<String>) -> CurrentUser {
        CurrentUser {
            user_id: Uuid::new_v4(),
            username: "testuser".to_string(),
            role: UserRole::Member,
            permissions,
            organization_id: None,
        }
    }

    #[test]
    fn test_check_permission_granted() {
        let user = test_user(vec!["apps:read".to_string(), "apps:write".to_string()]);
        assert!(check_permission(&user, "apps:read").is_ok());
        assert!(check_permission(&user, "apps:write").is_ok());
    }

    #[test]
    fn test_check_permission_denied() {
        let user = test_user(vec!["apps:read".to_string()]);
        let result = check_permission(&user, "apps:write");
        assert!(matches!(result, Err(AuthError::InsufficientPermissions { .. })));
    }

    #[test]
    fn test_admin_wildcard() {
        let user = test_user(vec!["admin:*".to_string()]);
        assert!(check_permission(&user, "apps:read").is_ok());
        assert!(check_permission(&user, "apps:write").is_ok());
        assert!(check_permission(&user, "nodes:delete").is_ok());
    }

    #[test]
    fn test_resource_wildcard() {
        let user = test_user(vec!["apps:*".to_string()]);
        assert!(check_permission(&user, "apps:read").is_ok());
        assert!(check_permission(&user, "apps:write").is_ok());
        assert!(check_permission(&user, "apps:delete").is_ok());
        assert!(check_permission(&user, "nodes:read").is_err());
    }

    #[test]
    fn test_check_any_permission() {
        let user = test_user(vec!["apps:read".to_string()]);
        assert!(check_any_permission(&user, &["apps:read", "apps:write"]).is_ok());
        assert!(check_any_permission(&user, &["nodes:read", "apps:read"]).is_ok());
        assert!(check_any_permission(&user, &["nodes:read", "nodes:write"]).is_err());
    }

    #[test]
    fn test_check_all_permissions() {
        let user = test_user(vec![
            "apps:read".to_string(),
            "apps:write".to_string(),
        ]);
        assert!(check_all_permissions(&user, &["apps:read", "apps:write"]).is_ok());
        assert!(check_all_permissions(&user, &["apps:read", "apps:delete"]).is_err());
    }
}
