import 'dart:convert';

import '../../../../src/rust/api/config_api.dart' as config_api;
import '../../../../src/rust/api/node_api.dart' as node_api;

/// Data source that bridges to the Rust engine via FFI.
///
/// All methods delegate to the generated flutter_rust_bridge bindings.
/// [RustLib.init()] must be called before any method on this class.
class EngineNativeSource {
  Future<String> initEngine(String storagePath) async {
    return node_api.initEngine(storagePath: storagePath);
  }

  Future<Map<String, dynamic>> getNodeStatus() async {
    final json = node_api.getNodeStatus();
    return jsonDecode(json) as Map<String, dynamic>;
  }

  Future<void> startEngine() async {
    await node_api.startEngine();
  }

  Future<void> shutdownEngine() async {
    await node_api.shutdownEngine();
  }

  Future<void> configureCluster(
    String clusterId,
    String serverUrl,
    String authToken,
  ) async {
    config_api.configureCluster(
      clusterId: clusterId,
      serverUrl: serverUrl,
      authToken: authToken,
    );
  }

  Future<bool> isConfigured() async {
    return config_api.isConfigured();
  }
}
