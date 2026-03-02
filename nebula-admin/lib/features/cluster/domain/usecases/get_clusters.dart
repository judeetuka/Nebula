import '../../../../core/usecases/usecase.dart';
import '../entities/cluster.dart';
import '../repositories/cluster_repository.dart';

class GetClusters implements UseCase<List<Cluster>, NoParams> {
  final ClusterRepository repository;

  const GetClusters(this.repository);

  @override
  Future<List<Cluster>> call(NoParams params) {
    return repository.getClusters();
  }
}
