/// Data source that bridges to the Rust engine via FFI.
///
/// Currently uses stub implementations that return mock data.
/// The real FFI bridge will be connected in Phase 7.
class EngineNativeSource {
  bool _initialized = false;
  bool _running = false;
  String? _clusterId;
  String? _serverUrl;
  String? _authToken;

  static const _mockNodeId = 'node-a1b2c3d4-e5f6-7890-abcd-ef1234567890';

  Future<String> initEngine(String storagePath) async {
    await Future<void>.delayed(const Duration(milliseconds: 200));
    _initialized = true;
    return _mockNodeId;
  }

  Future<Map<String, dynamic>> getNodeStatus() async {
    await Future<void>.delayed(const Duration(milliseconds: 100));
    final String state;
    if (!_initialized) {
      state = 'uninitialized';
    } else if (_clusterId == null) {
      state = 'idle';
    } else if (_running) {
      state = 'active';
    } else {
      state = 'configured';
    }

    return {
      'state': state,
      'node_id': _mockNodeId,
      'cluster_id': _clusterId,
      'is_configured': _clusterId != null,
      'is_active': _running,
    };
  }

  Future<void> startEngine() async {
    await Future<void>.delayed(const Duration(milliseconds: 300));
    _running = true;
  }

  Future<void> shutdownEngine() async {
    await Future<void>.delayed(const Duration(milliseconds: 200));
    _running = false;
  }

  Future<void> configureCluster(
    String clusterId,
    String serverUrl,
    String authToken,
  ) async {
    await Future<void>.delayed(const Duration(milliseconds: 250));
    _clusterId = clusterId;
    _serverUrl = serverUrl;
    _authToken = authToken;
  }

  Future<bool> isConfigured() async {
    await Future<void>.delayed(const Duration(milliseconds: 50));
    return _clusterId != null;
  }

  // Expose for potential debugging; suppress unused field warnings.
  String? get serverUrl => _serverUrl;
  String? get authToken => _authToken;
}
