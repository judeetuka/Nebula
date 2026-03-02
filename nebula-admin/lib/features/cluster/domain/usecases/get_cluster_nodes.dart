import '../../../../core/usecases/usecase.dart';
import '../entities/node_info.dart';
import '../repositories/cluster_repository.dart';

class GetClusterNodes implements UseCase<List<NodeInfo>, GetClusterNodesParams> {
  final ClusterRepository repository;

  const GetClusterNodes(this.repository);

  @override
  Future<List<NodeInfo>> call(GetClusterNodesParams params) {
    return repository.getClusterNodes(clusterId: params.clusterId);
  }
}

class GetClusterNodesParams {
  final String clusterId;

  const GetClusterNodesParams({required this.clusterId});
}
