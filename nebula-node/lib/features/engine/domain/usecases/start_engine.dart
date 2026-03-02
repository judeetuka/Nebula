import '../../../../core/usecases/usecase.dart';
import '../repositories/engine_repository.dart';

class StartEngine extends UseCase<void, NoParams> {
  final EngineRepository repository;

  StartEngine(this.repository);

  @override
  Future<void> call(NoParams params) {
    return repository.startEngine();
  }
}
