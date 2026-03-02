/// Builds MQTT topic strings following the `nebula/{cluster_id}/{path}` convention.
///
/// All cluster communication is namespaced under the cluster ID to support
/// multi-tenant broker deployments.
pub struct TopicBuilder;

impl TopicBuilder {
    /// Heartbeat topic for a specific node: `nebula/{cluster_id}/heartbeat/{node_id}`
    pub fn heartbeat(cluster_id: &str, node_id: &str) -> String {
        format!("nebula/{cluster_id}/heartbeat/{node_id}")
    }

    /// Task dispatch to a specific node: `nebula/{cluster_id}/tasks/dispatch/{node_id}`
    pub fn task_dispatch(cluster_id: &str, node_id: &str) -> String {
        format!("nebula/{cluster_id}/tasks/dispatch/{node_id}")
    }

    /// Broadcast tasks to all workers: `nebula/{cluster_id}/tasks/broadcast`
    pub fn task_broadcast(cluster_id: &str) -> String {
        format!("nebula/{cluster_id}/tasks/broadcast")
    }

    /// Task result for a specific task: `nebula/{cluster_id}/tasks/result/{task_id}`
    pub fn task_result(cluster_id: &str, task_id: &str) -> String {
        format!("nebula/{cluster_id}/tasks/result/{task_id}")
    }

    /// Plugin install command: `nebula/{cluster_id}/control/plugin/install`
    pub fn control_plugin_install(cluster_id: &str) -> String {
        format!("nebula/{cluster_id}/control/plugin/install")
    }

    /// Plugin uninstall command: `nebula/{cluster_id}/control/plugin/uninstall`
    pub fn control_plugin_uninstall(cluster_id: &str) -> String {
        format!("nebula/{cluster_id}/control/plugin/uninstall")
    }

    /// Master rotation announcement: `nebula/{cluster_id}/control/rotation/announce`
    pub fn control_rotation_announce(cluster_id: &str) -> String {
        format!("nebula/{cluster_id}/control/rotation/announce")
    }

    /// Master rotation ready signal: `nebula/{cluster_id}/control/rotation/ready`
    pub fn control_rotation_ready(cluster_id: &str) -> String {
        format!("nebula/{cluster_id}/control/rotation/ready")
    }

    /// Configuration update broadcast: `nebula/{cluster_id}/control/config/update`
    pub fn control_config_update(cluster_id: &str) -> String {
        format!("nebula/{cluster_id}/control/config/update")
    }

    /// Node registration join request: `nebula/{cluster_id}/registration/join`
    pub fn registration_join(cluster_id: &str) -> String {
        format!("nebula/{cluster_id}/registration/join")
    }

    /// Registration acknowledgement for a specific node: `nebula/{cluster_id}/registration/ack/{node_id}`
    pub fn registration_ack(cluster_id: &str, node_id: &str) -> String {
        format!("nebula/{cluster_id}/registration/ack/{node_id}")
    }

    /// Status topic for a specific node: `nebula/{cluster_id}/status/{node_id}`
    pub fn status(cluster_id: &str, node_id: &str) -> String {
        format!("nebula/{cluster_id}/status/{node_id}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLUSTER: &str = "test-cluster";
    const NODE: &str = "node-abc-123";
    const TASK: &str = "task-xyz-789";

    #[test]
    fn test_heartbeat_topic() {
        assert_eq!(
            TopicBuilder::heartbeat(CLUSTER, NODE),
            "nebula/test-cluster/heartbeat/node-abc-123"
        );
    }

    #[test]
    fn test_task_dispatch_topic() {
        assert_eq!(
            TopicBuilder::task_dispatch(CLUSTER, NODE),
            "nebula/test-cluster/tasks/dispatch/node-abc-123"
        );
    }

    #[test]
    fn test_task_broadcast_topic() {
        assert_eq!(
            TopicBuilder::task_broadcast(CLUSTER),
            "nebula/test-cluster/tasks/broadcast"
        );
    }

    #[test]
    fn test_task_result_topic() {
        assert_eq!(
            TopicBuilder::task_result(CLUSTER, TASK),
            "nebula/test-cluster/tasks/result/task-xyz-789"
        );
    }

    #[test]
    fn test_control_plugin_install_topic() {
        assert_eq!(
            TopicBuilder::control_plugin_install(CLUSTER),
            "nebula/test-cluster/control/plugin/install"
        );
    }

    #[test]
    fn test_control_plugin_uninstall_topic() {
        assert_eq!(
            TopicBuilder::control_plugin_uninstall(CLUSTER),
            "nebula/test-cluster/control/plugin/uninstall"
        );
    }

    #[test]
    fn test_control_rotation_announce_topic() {
        assert_eq!(
            TopicBuilder::control_rotation_announce(CLUSTER),
            "nebula/test-cluster/control/rotation/announce"
        );
    }

    #[test]
    fn test_control_rotation_ready_topic() {
        assert_eq!(
            TopicBuilder::control_rotation_ready(CLUSTER),
            "nebula/test-cluster/control/rotation/ready"
        );
    }

    #[test]
    fn test_control_config_update_topic() {
        assert_eq!(
            TopicBuilder::control_config_update(CLUSTER),
            "nebula/test-cluster/control/config/update"
        );
    }

    #[test]
    fn test_registration_join_topic() {
        assert_eq!(
            TopicBuilder::registration_join(CLUSTER),
            "nebula/test-cluster/registration/join"
        );
    }

    #[test]
    fn test_registration_ack_topic() {
        assert_eq!(
            TopicBuilder::registration_ack(CLUSTER, NODE),
            "nebula/test-cluster/registration/ack/node-abc-123"
        );
    }

    #[test]
    fn test_status_topic() {
        assert_eq!(
            TopicBuilder::status(CLUSTER, NODE),
            "nebula/test-cluster/status/node-abc-123"
        );
    }

    #[test]
    fn test_all_topics_start_with_nebula_prefix() {
        let topics = vec![
            TopicBuilder::heartbeat(CLUSTER, NODE),
            TopicBuilder::task_dispatch(CLUSTER, NODE),
            TopicBuilder::task_broadcast(CLUSTER),
            TopicBuilder::task_result(CLUSTER, TASK),
            TopicBuilder::control_plugin_install(CLUSTER),
            TopicBuilder::control_plugin_uninstall(CLUSTER),
            TopicBuilder::control_rotation_announce(CLUSTER),
            TopicBuilder::control_rotation_ready(CLUSTER),
            TopicBuilder::control_config_update(CLUSTER),
            TopicBuilder::registration_join(CLUSTER),
            TopicBuilder::registration_ack(CLUSTER, NODE),
            TopicBuilder::status(CLUSTER, NODE),
        ];

        for topic in topics {
            assert!(
                topic.starts_with("nebula/test-cluster/"),
                "Topic should start with nebula/{{cluster_id}}/: {}",
                topic
            );
        }
    }

    #[test]
    fn test_topics_contain_no_double_slashes() {
        let topics = vec![
            TopicBuilder::heartbeat(CLUSTER, NODE),
            TopicBuilder::task_dispatch(CLUSTER, NODE),
            TopicBuilder::task_broadcast(CLUSTER),
            TopicBuilder::task_result(CLUSTER, TASK),
            TopicBuilder::control_plugin_install(CLUSTER),
            TopicBuilder::control_plugin_uninstall(CLUSTER),
            TopicBuilder::control_rotation_announce(CLUSTER),
            TopicBuilder::control_rotation_ready(CLUSTER),
            TopicBuilder::control_config_update(CLUSTER),
            TopicBuilder::registration_join(CLUSTER),
            TopicBuilder::registration_ack(CLUSTER, NODE),
            TopicBuilder::status(CLUSTER, NODE),
        ];

        for topic in topics {
            assert!(
                !topic.contains("//"),
                "Topic should not contain double slashes: {}",
                topic
            );
        }
    }
}
