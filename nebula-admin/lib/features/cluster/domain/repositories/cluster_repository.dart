import '../entities/cluster.dart';
import '../entities/node_info.dart';

abstract class ClusterRepository {
  Future<List<Cluster>> getClusters();
  Future<Cluster> createCluster({required String name});
  Future<List<NodeInfo>> getClusterNodes({required String clusterId});
}
