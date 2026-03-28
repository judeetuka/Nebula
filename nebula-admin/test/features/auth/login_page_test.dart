import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:nebula_admin/features/auth/presentation/pages/login_page.dart';
import 'package:nebula_admin/features/auth/presentation/providers/auth_provider.dart';

void main() {
  /// Wraps [LoginPage] in the minimum tree required for widget tests:
  /// MaterialApp (for theming / navigation) and ProviderScope (for Riverpod).
  ///
  /// We override [authProvider] so the widget never reaches into Firebase or
  /// the network layer – it just sees a default [AuthState].
  Widget buildSubject({AuthState? initialState}) {
    return ProviderScope(
      overrides: [
        authProvider.overrideWith(
          (ref) => _StubAuthNotifier(initialState ?? const AuthState()),
        ),
      ],
      child: const MaterialApp(home: LoginPage()),
    );
  }

  group('LoginPage', () {
    testWidgets('renders email and password fields', (tester) async {
      await tester.pumpWidget(buildSubject());
      await tester.pump();

      // Email field
      expect(find.widgetWithText(TextFormField, 'Email'), findsOneWidget);

      // Password field
      expect(find.widgetWithText(TextFormField, 'Password'), findsOneWidget);
    });

    testWidgets('shows validation errors on empty submit', (tester) async {
      await tester.pumpWidget(buildSubject());
      await tester.pump();

      // Tap Sign In without filling any fields
      await tester.tap(find.text('Sign In'));
      await tester.pumpAndSettle();

      // The form validators should fire
      expect(find.text('Email is required'), findsOneWidget);
      expect(find.text('Password is required'), findsOneWidget);
    });

    testWidgets('Sign In button is present and enabled', (tester) async {
      await tester.pumpWidget(buildSubject());
      await tester.pump();

      final signInFinder = find.widgetWithText(FilledButton, 'Sign In');
      expect(signInFinder, findsOneWidget);

      // Button should be enabled (authState.isLoading == false)
      final FilledButton button = tester.widget(signInFinder);
      expect(button.onPressed, isNotNull);
    });

    testWidgets('Sign In button is disabled while loading', (tester) async {
      await tester.pumpWidget(
        buildSubject(initialState: const AuthState(isLoading: true)),
      );
      await tester.pump();

      // When loading, the button text is replaced by a progress indicator
      expect(find.byType(CircularProgressIndicator), findsOneWidget);
      expect(find.text('Sign In'), findsNothing);
    });

    testWidgets('displays app title and subtitle', (tester) async {
      await tester.pumpWidget(buildSubject());
      await tester.pump();

      expect(find.text('NEBULA Admin'), findsOneWidget);
      expect(
        find.text('Sign in to manage your compute clusters'),
        findsOneWidget,
      );
    });
  });
}

/// Minimal stub of [AuthNotifier] that never touches real services.
class _StubAuthNotifier extends AuthNotifier {
  _StubAuthNotifier(AuthState initial) : super(_FakeRef()) {
    state = initial;
  }

  @override
  Future<bool> signIn({required String email, required String password}) async {
    return false;
  }

  @override
  Future<void> signOut() async {}
}

/// Bare-minimum [Ref] so we can construct [_StubAuthNotifier] without a
/// real Riverpod container.
class _FakeRef implements Ref {
  @override
  dynamic noSuchMethod(Invocation invocation) => throw UnimplementedError();
}
