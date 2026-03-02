class NodeInfo {
  final String nodeId;
  final String role;
  final int batteryLevel;
  final double cpuLoad;
  final String status;
  final DateTime lastSeen;

  const NodeInfo({
    required this.nodeId,
    required this.role,
    required this.batteryLevel,
    required this.cpuLoad,
    required this.status,
    required this.lastSeen,
  });
}
