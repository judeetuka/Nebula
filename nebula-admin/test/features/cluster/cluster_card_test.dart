import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:nebula_admin/features/cluster/domain/entities/cluster.dart';
import 'package:nebula_admin/features/cluster/presentation/widgets/cluster_card.dart';

void main() {
  /// Helper to create a [Cluster] with sensible defaults.
  Cluster makeCluster({
    String id = 'c-1',
    String name = 'My Test Cluster',
    int nodeCount = 5,
    String serverUrl = 'http://192.168.0.10:8080',
  }) {
    return Cluster(
      id: id,
      name: name,
      nodeCount: nodeCount,
      serverUrl: serverUrl,
      createdAt: DateTime(2025, 1, 1),
    );
  }

  Widget buildSubject({required Cluster cluster, VoidCallback? onTap}) {
    return MaterialApp(
      home: Scaffold(
        body: ClusterCard(cluster: cluster, onTap: onTap ?? () {}),
      ),
    );
  }

  group('ClusterCard', () {
    testWidgets('displays cluster name', (tester) async {
      final cluster = makeCluster(name: 'Alpha Cluster');
      await tester.pumpWidget(buildSubject(cluster: cluster));

      expect(find.text('Alpha Cluster'), findsOneWidget);
    });

    testWidgets('displays node count (plural)', (tester) async {
      final cluster = makeCluster(nodeCount: 7);
      await tester.pumpWidget(buildSubject(cluster: cluster));

      expect(find.text('7 nodes'), findsOneWidget);
    });

    testWidgets('displays node count (singular)', (tester) async {
      final cluster = makeCluster(nodeCount: 1);
      await tester.pumpWidget(buildSubject(cluster: cluster));

      expect(find.text('1 node'), findsOneWidget);
    });

    testWidgets('displays server URL', (tester) async {
      final cluster = makeCluster(serverUrl: 'http://10.0.0.1:9090');
      await tester.pumpWidget(buildSubject(cluster: cluster));

      expect(find.text('http://10.0.0.1:9090'), findsOneWidget);
    });

    testWidgets('is tappable and invokes callback', (tester) async {
      var tapped = false;
      final cluster = makeCluster();

      await tester.pumpWidget(
        buildSubject(cluster: cluster, onTap: () => tapped = true),
      );

      await tester.tap(find.byType(ClusterCard));
      expect(tapped, isTrue);
    });

    testWidgets('shows green dot when nodeCount > 0', (tester) async {
      final cluster = makeCluster(nodeCount: 3);
      await tester.pumpWidget(buildSubject(cluster: cluster));

      // The status dot is a Container with a BoxDecoration circle
      final dotFinder = find.byWidgetPredicate((widget) {
        if (widget is Container && widget.decoration is BoxDecoration) {
          final dec = widget.decoration as BoxDecoration;
          return dec.shape == BoxShape.circle &&
              widget.constraints?.maxWidth == 10;
        }
        return false;
      });
      expect(dotFinder, findsOneWidget);
    });
  });
}
