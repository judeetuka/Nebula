import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:web_socket_channel/web_socket_channel.dart';

import '../di/injection.dart';
import '../storage/local_storage.dart';
import 'server_event.dart';

/// Manages a WebSocket connection to the NEBULA server's event stream.
///
/// Automatically reconnects with exponential backoff when the connection drops.
/// Exposes events as a broadcast [Stream<ServerEvent>].
class WebSocketService {
  final String serverUrl;
  final LocalStorage storage;

  WebSocketChannel? _channel;
  final StreamController<ServerEvent> _controller =
      StreamController<ServerEvent>.broadcast();
  Timer? _reconnectTimer;
  int _reconnectAttempts = 0;
  bool _disposed = false;

  static const int _maxReconnectDelay = 30; // seconds

  WebSocketService({required this.serverUrl, required this.storage});

  /// Broadcast stream of server events.
  Stream<ServerEvent> get events => _controller.stream;

  /// Open the WebSocket connection.
  void connect() {
    if (_disposed) return;
    _reconnectTimer?.cancel();

    final wsUrl = serverUrl
        .replaceFirst('https://', 'wss://')
        .replaceFirst('http://', 'ws://');
    final token = storage.jwtToken ?? '';

    final uri = Uri.parse(
      '$wsUrl/api/ws/events',
    ).replace(queryParameters: token.isNotEmpty ? {'token': token} : null);

    debugPrint('WebSocket connecting to $uri');

    try {
      _channel = WebSocketChannel.connect(uri);
      _reconnectAttempts = 0;

      _channel!.stream.listen(
        (message) {
          try {
            final json = jsonDecode(message as String) as Map<String, dynamic>;
            _controller.add(ServerEvent.fromJson(json));
          } catch (e) {
            debugPrint('WebSocket parse error: $e');
          }
        },
        onError: (Object error) {
          debugPrint('WebSocket error: $error');
          _scheduleReconnect();
        },
        onDone: () {
          debugPrint('WebSocket closed');
          _scheduleReconnect();
        },
      );
    } catch (e) {
      debugPrint('WebSocket connect failed: $e');
      _scheduleReconnect();
    }
  }

  void _scheduleReconnect() {
    if (_disposed) return;
    _channel = null;
    _reconnectAttempts++;
    final delay = Duration(
      seconds: _reconnectAttempts.clamp(1, _maxReconnectDelay),
    );
    debugPrint('WebSocket reconnecting in ${delay.inSeconds}s');
    _reconnectTimer = Timer(delay, connect);
  }

  /// Close the connection and release resources.
  void dispose() {
    _disposed = true;
    _reconnectTimer?.cancel();
    _channel?.sink.close();
    _controller.close();
  }
}

// --- Riverpod providers ---

/// Provides the singleton [WebSocketService] tied to the current server URL.
final webSocketServiceProvider = Provider<WebSocketService>((ref) {
  final serverUrl = ref.watch(serverUrlProvider);
  final storage = ref.watch(localStorageProvider);
  final service = WebSocketService(serverUrl: serverUrl, storage: storage);
  service.connect();
  ref.onDispose(() => service.dispose());
  return service;
});

/// Stream of all server events from the WebSocket.
final serverEventsProvider = StreamProvider<ServerEvent>((ref) {
  final service = ref.watch(webSocketServiceProvider);
  return service.events;
});
