import '../../../../core/usecases/usecase.dart';
import '../entities/node_status.dart';
import '../repositories/engine_repository.dart';

class GetNodeStatus extends UseCase<NodeStatus, NoParams> {
  final EngineRepository repository;

  GetNodeStatus(this.repository);

  @override
  Future<NodeStatus> call(NoParams params) {
    return repository.getNodeStatus();
  }
}
