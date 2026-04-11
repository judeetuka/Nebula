import 'package:firebase_core/firebase_core.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:manny_ui/manny_ui.dart';

import 'config/router.dart';
import 'core/di/injection.dart';
import 'core/storage/local_storage.dart';

void main() async {
  WidgetsFlutterBinding.ensureInitialized();

  // Initialize Hive-backed local storage before anything else.
  final storage = LocalStorage();
  await storage.init();

  // Attempt Firebase initialization. If firebase_options.dart is missing or
  // Firebase is not configured for this environment, the app falls back to
  // JWT API auth (see injection.dart).
  try {
    await Firebase.initializeApp();
  } catch (e) {
    debugPrint('Firebase not configured: $e');
  }

  runApp(
    ProviderScope(
      overrides: [localStorageProvider.overrideWithValue(storage)],
      child: const NebulaAdminApp(),
    ),
  );
}

class NebulaAdminApp extends StatelessWidget {
  const NebulaAdminApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'NEBULA Admin',
      debugShowCheckedModeBanner: false,
      theme: MannyTheme.lightTheme,
      darkTheme: MannyTheme.darkTheme,
      themeMode: ThemeMode.system,
      initialRoute: AppRoutes.login,
      onGenerateRoute: generateRoute,
    );
  }
}
