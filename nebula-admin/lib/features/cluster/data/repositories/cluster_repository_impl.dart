import '../../domain/entities/cluster.dart';
import '../../domain/entities/node_info.dart';
import '../../domain/repositories/cluster_repository.dart';
import '../datasources/cluster_remote_source.dart';

class ClusterRepositoryImpl implements ClusterRepository {
  final ClusterRemoteSource remoteSource;

  const ClusterRepositoryImpl(this.remoteSource);

  @override
  Future<List<Cluster>> getClusters() {
    return remoteSource.getClusters();
  }

  @override
  Future<Cluster> createCluster({required String name}) {
    return remoteSource.createCluster(name: name);
  }

  @override
  Future<List<NodeInfo>> getClusterNodes({required String clusterId}) {
    return remoteSource.getClusterNodes(clusterId: clusterId);
  }
}
