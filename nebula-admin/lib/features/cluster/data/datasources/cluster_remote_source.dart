import '../../domain/entities/cluster.dart';
import '../../domain/entities/node_info.dart';

abstract class ClusterRemoteSource {
  Future<List<Cluster>> getClusters();
  Future<Cluster> createCluster({required String name});
  Future<List<NodeInfo>> getClusterNodes({required String clusterId});
}

class ClusterRemoteSourceStub implements ClusterRemoteSource {
  final List<Cluster> _clusters = [
    Cluster(
      id: 'cl_001',
      name: 'Production Alpha',
      nodeCount: 5,
      serverUrl: 'https://alpha.nebula.io:8443',
      createdAt: DateTime(2026, 1, 15),
    ),
    Cluster(
      id: 'cl_002',
      name: 'Staging Beta',
      nodeCount: 3,
      serverUrl: 'https://beta.nebula.io:8443',
      createdAt: DateTime(2026, 2, 1),
    ),
    Cluster(
      id: 'cl_003',
      name: 'Dev Gamma',
      nodeCount: 2,
      serverUrl: 'https://gamma.nebula.io:8443',
      createdAt: DateTime(2026, 2, 20),
    ),
  ];

  int _nextId = 4;

  @override
  Future<List<Cluster>> getClusters() async {
    await Future<void>.delayed(const Duration(milliseconds: 300));
    return List.unmodifiable(_clusters);
  }

  @override
  Future<Cluster> createCluster({required String name}) async {
    await Future<void>.delayed(const Duration(milliseconds: 400));
    final cluster = Cluster(
      id: 'cl_${_nextId.toString().padLeft(3, '0')}',
      name: name,
      nodeCount: 0,
      serverUrl: 'https://${name.toLowerCase().replaceAll(' ', '-')}.nebula.io:8443',
      createdAt: DateTime.now(),
    );
    _clusters.add(cluster);
    _nextId++;
    return cluster;
  }

  @override
  Future<List<NodeInfo>> getClusterNodes({required String clusterId}) async {
    await Future<void>.delayed(const Duration(milliseconds: 300));

    final now = DateTime.now();
    return [
      NodeInfo(
        nodeId: 'nd_a1b2c3d4',
        role: 'coordinator',
        batteryLevel: 87,
        cpuLoad: 0.42,
        status: 'online',
        lastSeen: now.subtract(const Duration(seconds: 15)),
      ),
      NodeInfo(
        nodeId: 'nd_e5f6g7h8',
        role: 'worker',
        batteryLevel: 63,
        cpuLoad: 0.78,
        status: 'online',
        lastSeen: now.subtract(const Duration(seconds: 30)),
      ),
      NodeInfo(
        nodeId: 'nd_i9j0k1l2',
        role: 'worker',
        batteryLevel: 45,
        cpuLoad: 0.91,
        status: 'busy',
        lastSeen: now.subtract(const Duration(seconds: 5)),
      ),
      NodeInfo(
        nodeId: 'nd_m3n4o5p6',
        role: 'worker',
        batteryLevel: 12,
        cpuLoad: 0.05,
        status: 'offline',
        lastSeen: now.subtract(const Duration(minutes: 45)),
      ),
      NodeInfo(
        nodeId: 'nd_q7r8s9t0',
        role: 'observer',
        batteryLevel: 95,
        cpuLoad: 0.15,
        status: 'online',
        lastSeen: now.subtract(const Duration(seconds: 8)),
      ),
    ];
  }
}
