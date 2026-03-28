import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:nebula_admin/features/cluster/domain/entities/node_info.dart';
import 'package:nebula_admin/features/cluster/presentation/widgets/node_list_tile.dart';

void main() {
  /// Helper to create a [NodeInfo] with sensible defaults.
  NodeInfo makeNode({
    String nodeId = 'abcdef123456',
    String role = 'worker',
    int batteryLevel = 75,
    double cpuLoad = 0.45,
    String status = 'online',
  }) {
    return NodeInfo(
      nodeId: nodeId,
      role: role,
      batteryLevel: batteryLevel,
      cpuLoad: cpuLoad,
      status: status,
      lastSeen: DateTime(2025, 6, 15),
    );
  }

  Widget buildSubject(NodeInfo node) {
    return MaterialApp(
      home: Scaffold(
        body: ListView(children: [NodeListTile(node: node)]),
      ),
    );
  }

  group('NodeListTile', () {
    testWidgets('displays truncated node ID for long IDs', (tester) async {
      // "abcdef123456" is 12 chars → truncated to first 10 + "..."
      final node = makeNode(nodeId: 'abcdef123456');
      await tester.pumpWidget(buildSubject(node));

      expect(find.text('abcdef1234...'), findsOneWidget);
    });

    testWidgets('displays full node ID when short enough', (tester) async {
      final node = makeNode(nodeId: 'short-id');
      await tester.pumpWidget(buildSubject(node));

      expect(find.text('short-id'), findsOneWidget);
    });

    testWidgets('displays role badge', (tester) async {
      final node = makeNode(role: 'coordinator');
      await tester.pumpWidget(buildSubject(node));

      expect(find.text('coordinator'), findsOneWidget);
    });

    testWidgets('displays battery level percentage', (tester) async {
      final node = makeNode(batteryLevel: 92);
      await tester.pumpWidget(buildSubject(node));

      expect(find.text('92%'), findsOneWidget);
    });

    testWidgets('displays CPU load percentage', (tester) async {
      final node = makeNode(cpuLoad: 0.63);
      await tester.pumpWidget(buildSubject(node));

      expect(find.text('CPU 63%'), findsOneWidget);
    });

    testWidgets('shows full battery icon when level > 80', (tester) async {
      final node = makeNode(batteryLevel: 95);
      await tester.pumpWidget(buildSubject(node));

      expect(find.byIcon(Icons.battery_full), findsOneWidget);
    });

    testWidgets('shows low battery icon when level <= 20', (tester) async {
      final node = makeNode(batteryLevel: 15);
      await tester.pumpWidget(buildSubject(node));

      expect(find.byIcon(Icons.battery_1_bar), findsOneWidget);
    });

    testWidgets('shows CPU progress bar', (tester) async {
      final node = makeNode(cpuLoad: 0.5);
      await tester.pumpWidget(buildSubject(node));

      expect(find.byType(LinearProgressIndicator), findsOneWidget);
    });
  });
}
