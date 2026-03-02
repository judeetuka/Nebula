import '../../../../core/usecases/usecase.dart';
import '../entities/cluster.dart';
import '../repositories/cluster_repository.dart';

class CreateCluster implements UseCase<Cluster, CreateClusterParams> {
  final ClusterRepository repository;

  const CreateCluster(this.repository);

  @override
  Future<Cluster> call(CreateClusterParams params) {
    return repository.createCluster(name: params.name);
  }
}

class CreateClusterParams {
  final String name;

  const CreateClusterParams({required this.name});
}
