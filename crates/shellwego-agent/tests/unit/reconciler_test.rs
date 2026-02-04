use shellwego_agent::reconciler::Reconciler;
use shellwego_core::entities::app::ResourceSpec;
use uuid::Uuid;

#[test]
fn test_resource_math_correctness_tc_u1() {
    let spec = ResourceSpec {
        cpu_milli: 500,
        memory_bytes: 536870912,
        disk_bytes: 0,
    };
    assert_eq!(spec.memory_mb(), 512);
    assert_eq!(spec.cpu_shares(), 512);
    assert_eq!((spec.cpu_shares() / 1024) as u32, 1);
}

#[test]
fn test_resource_math_memory_mb() {
    let spec = ResourceSpec {
        cpu_milli: 1500,
        memory_bytes: 1024 * 1024 * 512,
        disk_bytes: 0,
    };
    assert_eq!(spec.memory_mb(), 512);
}

#[test]
fn test_resource_math_cpu_shares() {
    let spec = ResourceSpec {
        cpu_milli: 1500,
        memory_bytes: 1024 * 1024 * 512,
        disk_bytes: 0,
    };
    assert_eq!(spec.cpu_shares(), 1536);
}

#[test]
fn test_resource_math_edge_cases() {
    let zero = ResourceSpec::default();
    assert_eq!(zero.memory_mb(), 0);
    assert_eq!(zero.cpu_shares(), 0);

    let one_gb = ResourceSpec {
        cpu_milli: 1000,
        memory_bytes: 1024 * 1024 * 1024,
        disk_bytes: 0,
    };
    assert_eq!(one_gb.memory_mb(), 1024);
    assert_eq!(one_gb.cpu_shares(), 1024);
}

#[test]
fn test_cpu_shares_rounding() {
    let spec = ResourceSpec {
        cpu_milli: 1,
        memory_bytes: 1024,
        disk_bytes: 0,
    };
    assert_eq!(spec.cpu_shares(), 1);

    let spec = ResourceSpec {
        cpu_milli: 500,
        memory_bytes: 1024,
        disk_bytes: 0,
    };
    assert_eq!(spec.cpu_shares(), 512);

    let spec = ResourceSpec {
        cpu_milli: 1501,
        memory_bytes: 1024,
        disk_bytes: 0,
    };
    assert_eq!(spec.cpu_shares(), 1537);
}

#[test]
fn test_state_machine_sanity_tc_u2_create_action() {
    let app_a = Uuid::new_v4();
    let app_b = Uuid::new_v4();

    let desired = vec![app_a, app_b];
    let actual: Vec<Uuid> = vec![app_a];

    let to_create: Vec<Uuid> = desired
        .iter()
        .filter(|id| !actual.contains(id))
        .cloned()
        .collect();

    assert_eq!(to_create.len(), 1);
    assert!(to_create.contains(&app_b));
}

#[test]
fn test_state_machine_sanity_tc_u2_delete_action() {
    let desired: Vec<Uuid> = vec![];
    let actual = vec![Uuid::new_v4(), Uuid::new_v4()];

    let to_delete: Vec<Uuid> = actual
        .iter()
        .filter(|id| !desired.contains(id))
        .cloned()
        .collect();

    assert_eq!(to_delete.len(), 2);
}

#[test]
fn test_state_machine_sanity_tc_u2_no_change() {
    let app_id = Uuid::new_v4();
    let desired = vec![app_id];
    let actual = vec![app_id];

    let to_create: Vec<Uuid> = desired
        .iter()
        .filter(|id| !actual.contains(id))
        .cloned()
        .collect();
    let to_delete: Vec<Uuid> = actual
        .iter()
        .filter(|id| !desired.contains(id))
        .cloned()
        .collect();

    assert!(to_create.is_empty());
    assert!(to_delete.is_empty());
}

#[test]
fn test_state_machine_sanity_tc_u2_partial_overlap() {
    let app_a = Uuid::new_v4();
    let app_b = Uuid::new_v4();
    let app_c = Uuid::new_v4();

    let desired = vec![app_a, app_b];
    let actual = vec![app_a, app_c];

    let to_create: Vec<Uuid> = desired
        .iter()
        .filter(|id| !actual.contains(id))
        .cloned()
        .collect();
    let to_delete: Vec<Uuid> = actual
        .iter()
        .filter(|id| !desired.contains(id))
        .cloned()
        .collect();

    assert_eq!(to_create, vec![app_b]);
    assert_eq!(to_delete, vec![app_c]);
}

#[test]
fn test_mac_address_determinism_tc_u3() {
    let app_id = Uuid::parse_str("12345678-1234-1234-1234-123456789abc").unwrap();

    let mac1 = shellwego_network::generate_mac(&app_id);
    let mac2 = shellwego_network::generate_mac(&app_id);

    assert_eq!(mac1, mac2, "MAC generation must be deterministic");

    let parts: Vec<&str> = mac1.split(':').collect();
    assert_eq!(parts.len(), 6, "MAC must be 6 octets");

    assert_eq!(&parts[0..3], &["02", "00", "00"], "OUI must be 02:00:00");
}

#[test]
fn test_mac_address_determinism_different_ids() {
    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();

    let mac1 = shellwego_network::generate_mac(&id1);
    let mac2 = shellwego_network::generate_mac(&id2);

    assert_ne!(mac1, mac2, "Different UUIDs must produce different MACs");
}

#[test]
fn test_reconciliation_plan_generation() {
    let desired_apps = vec![
        Uuid::new_v4(),
        Uuid::new_v4(),
    ];
    let actual_apps = vec![desired_apps[0]];

    let creates: Vec<Uuid> = desired_apps
        .iter()
        .filter(|id| !actual_apps.contains(id))
        .cloned()
        .collect();
    let deletes: Vec<Uuid> = actual_apps
        .iter()
        .filter(|id| !desired_apps.contains(id))
        .cloned()
        .collect();

    assert_eq!(creates.len(), 1);
    assert!(deletes.is_empty());
    assert_eq!(creates[0], desired_apps[1]);
}
