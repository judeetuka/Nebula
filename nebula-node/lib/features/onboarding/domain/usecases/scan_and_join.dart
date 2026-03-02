import '../../../../core/usecases/usecase.dart';
import '../../../engine/domain/repositories/engine_repository.dart';
import '../entities/qr_payload.dart';

class ScanAndJoin extends UseCase<void, QrPayload> {
  final EngineRepository repository;

  ScanAndJoin(this.repository);

  @override
  Future<void> call(QrPayload payload) {
    return repository.configureCluster(
      payload.clusterId,
      payload.serverUrl,
      payload.authToken,
    );
  }
}
