class Cluster {
  final String id;
  final String name;
  final int nodeCount;
  final String serverUrl;
  final DateTime createdAt;

  const Cluster({
    required this.id,
    required this.name,
    required this.nodeCount,
    required this.serverUrl,
    required this.createdAt,
  });
}
