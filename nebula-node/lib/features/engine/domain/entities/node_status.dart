class NodeStatus {
  final String state;
  final String nodeId;
  final String? clusterId;
  final bool isConfigured;
  final bool isActive;

  const NodeStatus({
    required this.state,
    required this.nodeId,
    this.clusterId,
    required this.isConfigured,
    required this.isActive,
  });

  factory NodeStatus.fromJson(Map<String, dynamic> json) {
    return NodeStatus(
      state: json['state'] as String,
      nodeId: json['node_id'] as String,
      clusterId: json['cluster_id'] as String?,
      isConfigured: json['is_configured'] as bool,
      isActive: json['is_active'] as bool,
    );
  }
}
