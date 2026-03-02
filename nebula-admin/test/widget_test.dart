import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:nebula_admin/main.dart';

void main() {
  testWidgets('App renders login page on startup', (WidgetTester tester) async {
    await tester.pumpWidget(
      const ProviderScope(child: NebulaAdminApp()),
    );
    await tester.pumpAndSettle();

    expect(find.text('NEBULA Admin'), findsOneWidget);
    expect(find.text('Sign In'), findsOneWidget);
  });
}
