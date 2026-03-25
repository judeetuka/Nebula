/// A real-time event pushed from the NEBULA server via WebSocket.
class ServerEvent {
  final String type;
  final String? clusterId;
  final String? nodeId;
  final Map<String, dynamic> data;
  final DateTime timestamp;

  const ServerEvent({
    required this.type,
    this.clusterId,
    this.nodeId,
    this.data = const {},
    required this.timestamp,
  });

  factory ServerEvent.fromJson(Map<String, dynamic> json) {
    return ServerEvent(
      type: (json['type'] ?? 'unknown') as String,
      clusterId: json['cluster_id'] as String?,
      nodeId: json['node_id'] as String?,
      data: (json['data'] as Map<String, dynamic>?) ?? const {},
      timestamp:
          DateTime.tryParse((json['timestamp'] ?? '') as String) ??
          DateTime.now(),
    );
  }

  /// Common event types emitted by nebula-server.
  static const String nodeJoined = 'node_joined';
  static const String nodeLeft = 'node_left';
  static const String nodeStatusChanged = 'node_status_changed';
  static const String clusterCreated = 'cluster_created';
  static const String clusterDeleted = 'cluster_deleted';
  static const String metricsUpdate = 'metrics_update';
}
