import 'package:flutter_test/flutter_test.dart';

import 'package:nebula_admin/core/services/server_event.dart';

void main() {
  group('ServerEvent.fromJson', () {
    test('parses node_status_changed event', () {
      final json = <String, dynamic>{
        'type': 'node_status_changed',
        'cluster_id': 'cluster-42',
        'node_id': 'node-7',
        'data': <String, dynamic>{'old_status': 'online', 'new_status': 'busy'},
        'timestamp': '2025-06-15T10:30:00Z',
      };

      final event = ServerEvent.fromJson(json);

      expect(event.type, ServerEvent.nodeStatusChanged);
      expect(event.clusterId, 'cluster-42');
      expect(event.nodeId, 'node-7');
      expect(event.data['old_status'], 'online');
      expect(event.data['new_status'], 'busy');
      expect(event.timestamp, DateTime.utc(2025, 6, 15, 10, 30));
    });

    test('parses node_joined event', () {
      final json = <String, dynamic>{
        'type': 'node_joined',
        'cluster_id': 'c-1',
        'node_id': 'n-99',
        'timestamp': '2025-01-01T00:00:00Z',
      };

      final event = ServerEvent.fromJson(json);

      expect(event.type, ServerEvent.nodeJoined);
      expect(event.clusterId, 'c-1');
      expect(event.nodeId, 'n-99');
      expect(event.data, isEmpty);
    });

    test('parses metrics_update event with nested data', () {
      final json = <String, dynamic>{
        'type': 'metrics_update',
        'node_id': 'n-5',
        'data': <String, dynamic>{'cpu': 0.72, 'memory': 0.55, 'battery': 88},
        'timestamp': '2025-03-20T14:00:00Z',
      };

      final event = ServerEvent.fromJson(json);

      expect(event.type, ServerEvent.metricsUpdate);
      expect(event.clusterId, isNull);
      expect(event.nodeId, 'n-5');
      expect(event.data['cpu'], 0.72);
      expect(event.data['battery'], 88);
    });

    test('handles unknown type gracefully', () {
      final json = <String, dynamic>{
        'type': 'some_future_event',
        'timestamp': '2025-06-15T12:00:00Z',
      };

      final event = ServerEvent.fromJson(json);

      expect(event.type, 'some_future_event');
      expect(event.clusterId, isNull);
      expect(event.nodeId, isNull);
      expect(event.data, isEmpty);
    });

    test('defaults type to "unknown" when missing', () {
      final json = <String, dynamic>{'timestamp': '2025-06-15T12:00:00Z'};

      final event = ServerEvent.fromJson(json);

      expect(event.type, 'unknown');
    });

    test('defaults timestamp to now when missing or invalid', () {
      final before = DateTime.now();

      final event = ServerEvent.fromJson(<String, dynamic>{
        'type': 'node_left',
      });

      final after = DateTime.now();

      // Timestamp should be roughly "now" since no valid timestamp was given.
      expect(
        event.timestamp.isAfter(before.subtract(const Duration(seconds: 1))),
        isTrue,
      );
      expect(
        event.timestamp.isBefore(after.add(const Duration(seconds: 1))),
        isTrue,
      );
    });

    test('defaults data to empty map when null', () {
      final json = <String, dynamic>{
        'type': 'cluster_created',
        'cluster_id': 'c-new',
        'data': null,
        'timestamp': '2025-06-15T12:00:00Z',
      };

      final event = ServerEvent.fromJson(json);

      expect(event.data, isEmpty);
    });
  });

  group('ServerEvent type constants', () {
    test('all constants have expected values', () {
      expect(ServerEvent.nodeJoined, 'node_joined');
      expect(ServerEvent.nodeLeft, 'node_left');
      expect(ServerEvent.nodeStatusChanged, 'node_status_changed');
      expect(ServerEvent.clusterCreated, 'cluster_created');
      expect(ServerEvent.clusterDeleted, 'cluster_deleted');
      expect(ServerEvent.metricsUpdate, 'metrics_update');
    });
  });
}
