import '../../domain/entities/node_status.dart';
import '../../domain/repositories/engine_repository.dart';
import '../datasources/engine_native_source.dart';

class EngineRepositoryImpl implements EngineRepository {
  final EngineNativeSource nativeSource;

  EngineRepositoryImpl(this.nativeSource);

  @override
  Future<String> initEngine(String storagePath) {
    return nativeSource.initEngine(storagePath);
  }

  @override
  Future<NodeStatus> getNodeStatus() async {
    final json = await nativeSource.getNodeStatus();
    return NodeStatus.fromJson(json);
  }

  @override
  Future<void> startEngine() {
    return nativeSource.startEngine();
  }

  @override
  Future<void> shutdownEngine() {
    return nativeSource.shutdownEngine();
  }

  @override
  Future<void> configureCluster(
    String clusterId,
    String serverUrl,
    String authToken,
  ) {
    return nativeSource.configureCluster(clusterId, serverUrl, authToken);
  }

  @override
  Future<bool> isConfigured() {
    return nativeSource.isConfigured();
  }
}
