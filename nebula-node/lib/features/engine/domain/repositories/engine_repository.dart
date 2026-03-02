import '../entities/node_status.dart';

abstract class EngineRepository {
  Future<String> initEngine(String storagePath);
  Future<NodeStatus> getNodeStatus();
  Future<void> startEngine();
  Future<void> shutdownEngine();
  Future<void> configureCluster(
    String clusterId,
    String serverUrl,
    String authToken,
  );
  Future<bool> isConfigured();
}
