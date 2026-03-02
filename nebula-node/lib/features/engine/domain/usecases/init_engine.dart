import '../../../../core/usecases/usecase.dart';
import '../repositories/engine_repository.dart';

class InitEngine extends UseCase<String, String> {
  final EngineRepository repository;

  InitEngine(this.repository);

  @override
  Future<String> call(String storagePath) {
    return repository.initEngine(storagePath);
  }
}
