import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:nebula_node/main.dart';

void main() {
  testWidgets('NebulaNodeApp renders and shows welcome page',
      (WidgetTester tester) async {
    await tester.pumpWidget(
      const ProviderScope(child: NebulaNodeApp()),
    );

    // The app should render without crashing.
    // On first launch, the engine is not configured so we land on welcome.
    await tester.pumpAndSettle();

    // The welcome page should show the NEBULA branding.
    expect(find.text('NEBULA'), findsOneWidget);
    expect(find.text('Distributed Compute Node'), findsOneWidget);

    // The scan button should be visible.
    expect(find.widgetWithText(FilledButton, 'Scan QR Code'), findsOneWidget);
  });
}
