import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:web_socket_channel/web_socket_channel.dart';

import '../di/injection.dart';
import '../storage/local_storage.dart';
import 'server_event.dart';

/// Real-time event service.
///
/// **Disabled by default.** The admin app uses pull-to-refresh and periodic
/// auto-refresh (30s) for all data. Live mode is an opt-in toggle in settings
/// that enables WebSocket on web or faster polling on native.
///
/// Why: The admin app is a management dashboard, not a real-time control
/// panel. Persistent connections waste battery on mobile and add complexity
/// for no practical benefit in most use cases.
class EventService {
  final String serverUrl;
  final LocalStorage storage;

  final StreamController<ServerEvent> _controller =
      StreamController<ServerEvent>.broadcast();
  bool _disposed = false;
  bool _liveMode = false;

  WebSocketChannel? _channel;
  Timer? _reconnectTimer;
  int _reconnectAttempts = 0;

  EventService({required this.serverUrl, required this.storage});

  Stream<ServerEvent> get events => _controller.stream;
  bool get isLive => _liveMode && !_disposed;

  /// Enable live mode (WebSocket on web). No-op on native (use polling provider instead).
  void enableLiveMode() {
    if (_liveMode || _disposed) return;
    _liveMode = true;
    if (kIsWeb) {
      _connectWebSocket();
    }
    debugPrint('Live mode enabled');
  }

  /// Disable live mode and close any connections.
  void disableLiveMode() {
    _liveMode = false;
    _reconnectTimer?.cancel();
    _channel?.sink.close();
    _channel = null;
    debugPrint('Live mode disabled');
  }

  void _connectWebSocket() {
    if (_disposed || !_liveMode) return;

    final wsUrl = serverUrl
        .replaceFirst('http://', 'ws://')
        .replaceFirst('https://', 'wss://');
    final token = storage.jwtToken;
    final uri = token != null
        ? '$wsUrl/api/ws/events?token=$token'
        : '$wsUrl/api/ws/events';

    try {
      _channel = WebSocketChannel.connect(Uri.parse(uri));
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
        onDone: () {
          if (_liveMode) _scheduleReconnect();
        },
        onError: (error) {
          debugPrint('WebSocket error: $error');
          if (_liveMode) _scheduleReconnect();
        },
      );
    } catch (e) {
      debugPrint('WebSocket connection failed: $e');
      if (_liveMode) _scheduleReconnect();
    }
  }

  void _scheduleReconnect() {
    if (_disposed || !_liveMode) return;
    _reconnectAttempts++;
    final delaySecs = (_reconnectAttempts * 2).clamp(1, 30);
    _reconnectTimer?.cancel();
    _reconnectTimer = Timer(Duration(seconds: delaySecs), _connectWebSocket);
  }

  void dispose() {
    _disposed = true;
    disableLiveMode();
    _controller.close();
  }
}

// --- Riverpod providers ---

final eventServiceProvider = Provider<EventService>((ref) {
  final serverUrl = ref.watch(serverUrlProvider);
  final storage = ref.watch(localStorageProvider);
  final service = EventService(serverUrl: serverUrl, storage: storage);
  // NOT connected by default -- user opts in via settings
  ref.onDispose(() => service.dispose());
  return service;
});

/// Stream of server events. Only emits when live mode is enabled.
final serverEventsProvider = StreamProvider<ServerEvent>((ref) {
  final service = ref.watch(eventServiceProvider);
  return service.events;
});
