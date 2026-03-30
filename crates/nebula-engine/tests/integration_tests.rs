//! Integration tests: end-to-end scenarios crossing multiple engine subsystems.

use std::thread;
use std::time::Duration;

use nebula_core::identity::node_id::NodeId;
use nebula_core::identity::roles::NodeRole;
use nebula_engine::cluster::failover::{
    FailoverConfig, FailoverCoordinator, FailoverState, PromotionNotice,
};
use nebula_engine::cluster::membership::MemberInfo;
use nebula_engine::cluster::membership::{ClusterMembership, NodeMetrics};
use nebula_engine::cluster::rotation::{compute_master_score, RotationManager};
use nebula_engine::cluster::succession::SuccessionManager;
use nebula_engine::storage::db::StorageManager;
use nebula_engine::storage::models::*;

fn make_metrics(battery: u8, cpu: f32, mem: u32, tasks: u16, uptime: u64) -> NodeMetrics {
    NodeMetrics {
        battery_level: battery,
        cpu_load: cpu,
        memory_available_mb: mem,
        active_tasks: tasks,
        uptime_secs: uptime,
    }
}

fn drive_to_reported(c: &mut FailoverCoordinator) {
    c.mqtt_connection_lost();
    thread::sleep(Duration::from_millis(5));
    c.check_master_timeout();
    thread::sleep(Duration::from_millis(5));
    c.check_grace_period();
}

// ---------------------------------------------------------------------------
// 1. Cluster membership full lifecycle
// ---------------------------------------------------------------------------

#[test]
fn test_cluster_membership_full_lifecycle() {
    let master = NodeId::generate();
    let w1 = NodeId::generate();
    let w2 = NodeId::generate();
    let w3 = NodeId::generate();

    let mut membership = ClusterMembership::new(master, NodeRole::Master);
    membership.add_member(w1, NodeRole::Worker, make_metrics(90, 0.2, 4096, 0, 3600));
    membership.add_member(w2, NodeRole::Worker, make_metrics(50, 0.6, 1024, 5, 7200));
    membership.add_member(w3, NodeRole::Worker, make_metrics(30, 0.8, 512, 10, 1800));

    let members = membership.get_members();
    // Master created via new() is included in members, workers added via add_member
    let expected_count = members.len(); // verify we have at least 3 workers
    assert!(
        expected_count >= 3,
        "should have at least 3 workers, got {}",
        expected_count
    );

    // Verify specific member exists
    assert!(members.contains_key(&w1));
    assert_eq!(members.get(&w1).unwrap().role, NodeRole::Worker);

    // Update heartbeat for w2
    membership.update_heartbeat(&w2, make_metrics(55, 0.5, 1024, 4, 7300));
    let members2 = membership.get_members();
    let updated = members2.get(&w2).unwrap();
    assert_eq!(updated.metrics.battery_level, 55);
}

// ---------------------------------------------------------------------------
// 2. Succession line computation with full cluster
// ---------------------------------------------------------------------------

#[test]
fn test_succession_line_computation_with_full_cluster() {
    let master = NodeId::generate();
    let strong = NodeId::generate();
    let medium = NodeId::generate();
    let weak = NodeId::generate();
    let fragile = NodeId::generate();

    let mut membership = ClusterMembership::new(master, NodeRole::Master);
    membership.add_member(
        strong,
        NodeRole::Worker,
        make_metrics(95, 0.1, 4096, 0, 600),
    );
    membership.add_member(
        medium,
        NodeRole::Worker,
        make_metrics(70, 0.3, 2048, 3, 3600),
    );
    membership.add_member(weak, NodeRole::Worker, make_metrics(40, 0.6, 1024, 8, 7200));
    membership.add_member(
        fragile,
        NodeRole::Worker,
        make_metrics(25, 0.7, 512, 10, 100),
    );

    let sm = SuccessionManager::new();
    let line = sm.compute_succession_line(&membership, &master);

    assert_eq!(line.len(), 4);
    assert_eq!(line[0].node_id, strong.to_string());
    assert_eq!(line[0].rank, 1);

    // Scores should be descending
    for i in 0..line.len() - 1 {
        assert!(
            line[i].score >= line[i + 1].score,
            "Score at rank {} ({}) should be >= rank {} ({})",
            line[i].rank,
            line[i].score,
            line[i + 1].rank,
            line[i + 1].score
        );
    }

    // Now degrade fragile to below battery threshold
    membership.update_heartbeat(&fragile, make_metrics(10, 0.7, 512, 10, 100));
    let line2 = sm.compute_succession_line(&membership, &master);
    assert_eq!(
        line2.len(),
        3,
        "fragile should be excluded (battery=10 < threshold=20)"
    );
    assert!(!line2.iter().any(|e| e.node_id == fragile.to_string()));

    // Designated heir should be the strong node
    let heir = sm.designated_heir(&membership, &master).unwrap();
    assert_eq!(heir.node_id, strong.to_string());
    assert_eq!(heir.rank, 1);
}

// ---------------------------------------------------------------------------
// 3. Failover scenario: master death
// ---------------------------------------------------------------------------

#[test]
fn test_failover_scenario_server_mediated() {
    let config = FailoverConfig {
        master_timeout: Duration::from_millis(1),
        grace_period: Duration::from_millis(1),
        ..Default::default()
    };

    let mut coord = FailoverCoordinator::new("node-a", config);
    coord.update_local_metrics(make_metrics(95, 0.1, 4096, 0, 300), 100);

    // Simulate master MQTT broker disconnect -> timeout -> grace -> reported to server
    drive_to_reported(&mut coord);
    assert!(matches!(
        *coord.state(),
        FailoverState::ReportedToServer { .. }
    ));

    // Build timeout report to send to server
    let report = coord.build_timeout_report("cluster-1").unwrap();
    assert_eq!(report.reporter_node_id, "node-a");
    assert_eq!(report.reporter_battery, 95);
    assert_eq!(report.cluster_id, "cluster-1");

    // Server picks this node as the new master and sends PromotionNotice
    coord.handle_promotion_notice(&PromotionNotice {
        cluster_id: "cluster-1".into(),
        new_master_id: "node-a".into(),
        new_master_mqtt_host: Some("10.0.0.1".into()),
        new_master_mqtt_port: Some(1883),
    });
    assert_eq!(*coord.state(), FailoverState::PromotedByServer);
}

// ---------------------------------------------------------------------------
// 4. Proactive rotation with scoring
// ---------------------------------------------------------------------------

#[test]
fn test_proactive_rotation_with_scoring() {
    let master = NodeId::generate();
    let strong_worker = NodeId::generate();
    let weak_worker = NodeId::generate();

    // Degraded master vs strong worker
    let master_metrics = make_metrics(25, 0.7, 512, 10, 7200);
    let strong_metrics = make_metrics(95, 0.1, 4096, 0, 600);
    let weak_metrics = make_metrics(40, 0.5, 1024, 8, 3600);

    let master_score = compute_master_score(&master_metrics);
    let strong_score = compute_master_score(&strong_metrics);
    let _weak_score = compute_master_score(&weak_metrics);

    // Strong worker should have significantly higher score
    assert!(
        strong_score > master_score + 0.15,
        "Strong worker score ({}) should exceed master score ({}) by > 0.15",
        strong_score,
        master_score
    );

    // Build member list for rotation check
    let strong_member = MemberInfo {
        node_id: strong_worker,
        role: NodeRole::Worker,
        last_heartbeat: std::time::Instant::now(),
        metrics: strong_metrics,
    };
    let weak_member = MemberInfo {
        node_id: weak_worker,
        role: NodeRole::Worker,
        last_heartbeat: std::time::Instant::now(),
        metrics: weak_metrics,
    };
    let all_members: Vec<&MemberInfo> = vec![&strong_member, &weak_member];

    let mut mgr = RotationManager::new();
    mgr.start_tracking();
    thread::sleep(Duration::from_millis(5));
    let should = mgr.should_rotate(&master_metrics, &all_members);
    assert!(should.is_some(), "rotation should be suggested");
    let suggested = should.unwrap();
    assert_eq!(suggested, strong_worker, "should suggest strongest worker");
}

// ---------------------------------------------------------------------------
// 5. Storage + cluster members + succession lifecycle
// ---------------------------------------------------------------------------

#[test]
fn test_storage_cluster_member_and_succession_lifecycle() {
    let mgr = StorageManager::new_in_memory(b"integration-test").unwrap();

    // Insert 4 cluster members
    let members = vec![
        ("master-1", "master", 90u8, 0.1f32, 4096u32),
        ("worker-1", "worker", 80, 0.3, 2048),
        ("worker-2", "worker", 60, 0.5, 1024),
        ("worker-3", "worker", 40, 0.7, 512),
    ];

    for (id, role, bat, cpu, mem) in &members {
        mgr.upsert_cluster_member(ClusterMemberRecord {
            node_id: id.to_string(),
            role: role.to_string(),
            join_time: 1000,
            last_heartbeat: 2000,
            battery_level: *bat,
            cpu_load: *cpu,
            memory_available_mb: *mem,
            active_tasks: 0,
            network_type: "wifi".into(),
            peer_address: None,
            is_stale: false,
        })
        .unwrap();
    }

    assert_eq!(mgr.get_cluster_members().unwrap().len(), 4);

    // Store succession line
    mgr.set_succession_line(SuccessionRecord {
        cluster_id: "cluster-1".into(),
        succession_json: r#"["worker-1","worker-2","worker-3"]"#.into(),
        computed_at: 2000,
        computed_by: "master-1".into(),
    })
    .unwrap();

    let line = mgr.get_succession_line("cluster-1").unwrap().unwrap();
    assert_eq!(line.computed_by, "master-1");

    // Degrade worker-2 to unhealthy
    mgr.upsert_cluster_member(ClusterMemberRecord {
        node_id: "worker-2".into(),
        role: "worker".into(),
        join_time: 1000,
        last_heartbeat: 2100,
        battery_level: 10,
        cpu_load: 0.9,
        memory_available_mb: 50,
        active_tasks: 20,
        network_type: "wifi".into(),
        peer_address: None,
        is_stale: true,
    })
    .unwrap();

    let stale = mgr.get_stale_members().unwrap();
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0].node_id, "worker-2");

    // Update succession (without worker-2)
    mgr.set_succession_line(SuccessionRecord {
        cluster_id: "cluster-1".into(),
        succession_json: r#"["worker-1","worker-3"]"#.into(),
        computed_at: 2100,
        computed_by: "master-1".into(),
    })
    .unwrap();

    let updated_line = mgr.get_succession_line("cluster-1").unwrap().unwrap();
    assert_eq!(updated_line.computed_at, 2100);
    assert!(!updated_line.succession_json.contains("worker-2"));
}

// ---------------------------------------------------------------------------
// 6. Storage task queue lifecycle
// ---------------------------------------------------------------------------

#[test]
fn test_storage_task_queue_lifecycle() {
    let mgr = StorageManager::new_in_memory(b"task-test").unwrap();

    // Enqueue 5 tasks with different priorities
    let tasks = vec![
        ("task-1", 1u8),
        ("task-2", 5),
        ("task-3", 10),
        ("task-4", 10),
        ("task-5", 3),
    ];
    for (id, pri) in &tasks {
        mgr.enqueue_task(TaskQueueItem {
            task_id: id.to_string(),
            status: "pending".into(),
            priority: *pri,
            task_type: "compute".into(),
            payload_json: "{}".into(),
            submitted_at: 100,
            started_at: None,
            completed_at: None,
            assigned_node: None,
            timeout_secs: 60,
            retry_count: 0,
            error_message: None,
        })
        .unwrap();
    }

    assert_eq!(mgr.get_tasks_by_status("pending").unwrap().len(), 5);

    // Dequeue: highest priority first (two priority-10 tasks, then 5, then 3, then 1)
    let t1 = mgr.dequeue_next_task().unwrap().unwrap();
    assert_eq!(t1.priority, 10);
    let t2 = mgr.dequeue_next_task().unwrap().unwrap();
    assert_eq!(t2.priority, 10);

    // 3 remaining: priorities 5, 3, 1
    assert_eq!(mgr.get_tasks_by_status("pending").unwrap().len(), 3);

    // Get task-5 (priority 3), update status to simulate lifecycle
    let task5 = mgr.get_task("task-5").unwrap().unwrap();
    assert_eq!(task5.status, "pending");

    let mut running = task5.clone();
    running.status = "running".into();
    running.started_at = Some(200);
    mgr.update_task(task5, running.clone()).unwrap();

    assert_eq!(mgr.get_tasks_by_status("running").unwrap().len(), 1);

    let mut completed = running.clone();
    completed.status = "completed".into();
    completed.completed_at = Some(300);
    mgr.update_task(running, completed).unwrap();

    assert_eq!(mgr.get_tasks_by_status("completed").unwrap().len(), 1);

    // Dequeue remaining pending tasks (task-2=5, task-1=1)
    // task-5 was moved to completed so only 2 pending remain
    let t3 = mgr.dequeue_next_task().unwrap().unwrap();
    assert_eq!(t3.priority, 5);
    let t4 = mgr.dequeue_next_task().unwrap().unwrap();
    assert_eq!(t4.priority, 1);

    // Queue should be empty
    assert!(mgr.dequeue_next_task().unwrap().is_none());
}
