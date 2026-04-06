//! Integration tests for registry mirror chain.
//!
//! Tests priority ordering, circuit breaking, health checks, and failover.

#[cfg(test)]
mod tests {
    use shellwego_registry::mirror::MirrorChain;
    use shellwego_schema::oci::{MirrorConfig, MirrorHealth, MirrorList, MirrorPriority};

    fn make_mirror(id: &str, priority: MirrorPriority) -> MirrorConfig {
        MirrorConfig {
            id: id.to_string(),
            endpoint: format!("https://{}.example.com", id),
            priority,
            enabled: true,
            registry_override: None,
            auth: None,
            health_check_interval_secs: 30,
            circuit_breaker_threshold: 3,
            timeout_secs: 60,
        }
    }

    #[test]
    fn test_empty_mirror_list() {
        let list = MirrorList::new();
        assert!(list.is_empty());
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn test_mirror_list_sorts_by_priority() {
        let list = MirrorList::new()
            .add_mirror(make_mirror("low-priority", MirrorPriority::Low))
            .add_mirror(make_mirror("critical", MirrorPriority::Critical))
            .add_mirror(make_mirror("high", MirrorPriority::High));

        assert_eq!(list.mirrors[0].id, "critical");
        assert_eq!(list.mirrors[1].id, "high");
        assert_eq!(list.mirrors[2].id, "low-priority");
    }

    #[tokio::test]
    async fn test_chain_creation() {
        let list = MirrorList::new()
            .add_mirror(make_mirror("m1", MirrorPriority::High))
            .add_mirror(make_mirror("m2", MirrorPriority::Normal));

        let chain = MirrorChain::new(list);
        assert!(!chain.is_empty());
        assert_eq!(chain.config().len(), 2);
    }

    #[tokio::test]
    async fn test_chain_returns_mirror_in_order() {
        let list = MirrorList::new()
            .add_mirror(make_mirror("m1", MirrorPriority::High))
            .add_mirror(make_mirror("m2", MirrorPriority::Normal))
            .add_mirror(make_mirror("m3", MirrorPriority::Low));

        let chain = MirrorChain::new(list);

        let (endpoint, _) = chain.next_mirror("docker.io", &[]).await.unwrap();
        assert!(endpoint.contains("m1"));

        let (endpoint, _) = chain
            .next_mirror("docker.io", &["m1".to_string()])
            .await
            .unwrap();
        assert!(endpoint.contains("m2"));

        let (endpoint, _) = chain
            .next_mirror(
                "docker.io",
                &["m1".to_string(), "m2".to_string()],
            )
            .await
            .unwrap();
        assert!(endpoint.contains("m3"));
    }

    #[tokio::test]
    async fn test_circuit_breaker() {
        let list = MirrorList::new().add_mirror(make_mirror("m1", MirrorPriority::High));
        let chain = MirrorChain::new(list);

        // 3 failures should trip the circuit breaker
        for _ in 0..3 {
            chain.record_failure("m1", 3).await;
        }

        let result = chain.next_mirror("docker.io", &[]).await;
        assert!(result.is_none(), "Circuit breaker should block mirror");
    }

    #[tokio::test]
    async fn test_success_resets_failure_count() {
        let list = MirrorList::new().add_mirror(make_mirror("m1", MirrorPriority::High));
        let chain = MirrorChain::new(list);

        // 2 failures (below threshold)
        chain.record_failure("m1", 3).await;
        chain.record_failure("m1", 3).await;

        // Success resets
        chain.record_success("m1").await;

        // One more failure shouldn't trip the breaker
        chain.record_failure("m1", 3).await;
        let result = chain.next_mirror("docker.io", &[]).await;
        assert!(
            result.is_some(),
            "Circuit breaker should NOT trip after reset"
        );
    }

    #[tokio::test]
    async fn test_empty_chain_next_returns_none() {
        let chain = MirrorChain::new(MirrorList::new());
        let result = chain.next_mirror("docker.io", &[]).await;
        assert!(result.is_none());
    }

    #[test]
    fn test_registry_override_filtering() {
        let list = MirrorList::new()
            .add_mirror(make_mirror("generic", MirrorPriority::Normal))
            .add_mirror({
                let mut m = make_mirror("docker-specific", MirrorPriority::High);
                m.registry_override = Some("registry-1.docker.io".to_string());
                m
            });

        // For docker registry, both match
        let docker_mirrors = list.for_registry("registry-1.docker.io");
        assert_eq!(docker_mirrors.len(), 2);

        // For gcr, only generic matches
        let gcr_mirrors = list.for_registry("gcr.io");
        assert_eq!(gcr_mirrors.len(), 1);
        assert_eq!(gcr_mirrors[0].id, "generic");
    }
}
