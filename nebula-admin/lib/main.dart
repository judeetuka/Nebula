import 'package:firebase_core/firebase_core.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:nebula_ui/nebula_ui.dart';

import 'config/router.dart';

void main() async {
  WidgetsFlutterBinding.ensureInitialized();

  // Attempt Firebase initialization. If firebase_options.dart is missing or
  // Firebase is not configured for this environment, the app falls back to
  // stub auth (see injection.dart).
  try {
    await Firebase.initializeApp();
  } catch (e) {
    debugPrint('Firebase not configured: $e');
  }

  runApp(const ProviderScope(child: NebulaAdminApp()));
}

class NebulaAdminApp extends StatelessWidget {
  const NebulaAdminApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'NEBULA Admin',
      debugShowCheckedModeBanner: false,
      theme: NebulaTheme.lightTheme,
      darkTheme: NebulaTheme.darkTheme,
      themeMode: ThemeMode.system,
      initialRoute: AppRoutes.login,
      onGenerateRoute: generateRoute,
    );
  }
}
