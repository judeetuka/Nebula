import 'package:flutter/material.dart';

import '../features/browser/presentation/pages/browser_page.dart';
import '../features/engine/presentation/pages/status_page.dart';
import '../features/onboarding/presentation/pages/qr_scan_page.dart';
import '../features/onboarding/presentation/pages/welcome_page.dart';

class AppRoutes {
  AppRoutes._();

  static const welcome = '/welcome';
  static const scan = '/scan';
  static const status = '/status';
  static const browser = '/browser';
}

Route<dynamic>? onGenerateRoute(RouteSettings settings) {
  switch (settings.name) {
    case AppRoutes.welcome:
      return MaterialPageRoute(
        builder: (_) => const WelcomePage(),
        settings: settings,
      );
    case AppRoutes.scan:
      return MaterialPageRoute(
        builder: (_) => const QrScanPage(),
        settings: settings,
      );
    case AppRoutes.status:
      return MaterialPageRoute(
        builder: (_) => const StatusPage(),
        settings: settings,
      );
    case AppRoutes.browser:
      final initialUrl = settings.arguments as String?;
      return MaterialPageRoute(
        builder: (_) => BrowserPage(initialUrl: initialUrl),
        settings: settings,
      );
    default:
      return MaterialPageRoute(
        builder: (_) => const WelcomePage(),
        settings: settings,
      );
  }
}
