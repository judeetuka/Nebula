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
    // Server sends timestamp as either unix int or ISO string
    DateTime ts;
    final rawTs = json['timestamp'];
    if (rawTs is int) {
      ts = DateTime.fromMillisecondsSinceEpoch(rawTs * 1000);
    } else if (rawTs is String) {
      ts = DateTime.tryParse(rawTs) ?? DateTime.now();
    } else {
      ts = DateTime.now();
    }

    // Extract data fields — server events use flat structure (no nested "data")
    final knownKeys = {'type', 'cluster_id', 'node_id', 'timestamp'};
    final extraData = Map<String, dynamic>.from(json)
      ..removeWhere((k, _) => knownKeys.contains(k));

    return ServerEvent(
      type: (json['type'] ?? 'unknown') as String,
      clusterId: json['cluster_id']?.toString(),
      nodeId: json['node_id']?.toString(),
      data: extraData,
      timestamp: ts,
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
