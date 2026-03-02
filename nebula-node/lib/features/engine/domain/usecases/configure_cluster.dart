import '../../../../core/usecases/usecase.dart';
import '../repositories/engine_repository.dart';

class ConfigureClusterParams {
  final String clusterId;
  final String serverUrl;
  final String authToken;

  const ConfigureClusterParams({
    required this.clusterId,
    required this.serverUrl,
    required this.authToken,
  });
}

class ConfigureCluster extends UseCase<void, ConfigureClusterParams> {
  final EngineRepository repository;

  ConfigureCluster(this.repository);

  @override
  Future<void> call(ConfigureClusterParams params) {
    return repository.configureCluster(
      params.clusterId,
      params.serverUrl,
      params.authToken,
    );
  }
}
