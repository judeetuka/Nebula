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

  runApp(
    ProviderScope(
      overrides: [localStorageProvider.overrideWithValue(storage)],
      child: const NebulaAdminApp(),
    ),
  );
}

class NebulaAdminApp extends StatefulWidget {
  const NebulaAdminApp({super.key});

  /// Access from anywhere to toggle theme.
  static _NebulaAdminAppState of(BuildContext context) =>
      context.findAncestorStateOfType<_NebulaAdminAppState>()!;

  @override
  State<NebulaAdminApp> createState() => _NebulaAdminAppState();
}

class _NebulaAdminAppState extends State<NebulaAdminApp> {
  ThemeMode _themeMode = ThemeMode.dark;

  void toggleTheme() {
    setState(() {
      _themeMode =
          _themeMode == ThemeMode.dark ? ThemeMode.light : ThemeMode.dark;
    });
  }

  bool get isDark => _themeMode == ThemeMode.dark;

  // ── HUD Theme Palette ──
  static final _lightTheme = MannyTheme.lightTheme.copyWith(
    colorScheme: MannyTheme.lightTheme.colorScheme.copyWith(
      primary: const Color(0xFF059669),
      secondary: const Color(0xFF2563EB),
      tertiary: const Color(0xFFD97706),
      error: const Color(0xFFDC2626),
      surface: const Color(0xFFF4F6F8),
      primaryContainer: const Color(0xFFD1FAE5),
      secondaryContainer: const Color(0xFFDBEAFE),
    ),
    scaffoldBackgroundColor: const Color(0xFFF4F6F8),
    splashFactory: FrostedInkSplash.splashFactory,
  );

  static final _darkTheme = MannyTheme.darkTheme.copyWith(
    colorScheme: MannyTheme.darkTheme.colorScheme.copyWith(
      primary: const Color(0xFF6EE7B7),
      secondary: const Color(0xFF60A5FA),
      tertiary: const Color(0xFFFBBF24),
      error: const Color(0xFFF87171),
      surface: const Color(0xFF0C0F14),
      primaryContainer: const Color(0xFF14332A),
      secondaryContainer: const Color(0xFF1A2740),
    ),
    scaffoldBackgroundColor: const Color(0xFF0C0F14),
    splashFactory: FrostedInkSplash.splashFactory,
  );

  @override
  Widget build(BuildContext context) {
    return MannyConfig(
      neumorphic: true,
      child: MaterialApp(
        title: 'NEBULA Admin',
        debugShowCheckedModeBanner: false,
        theme: _lightTheme,
        darkTheme: _darkTheme,
        themeMode: _themeMode,
        scrollBehavior: MannyScrollBehavior().copyWith(
          physics: const ClampingScrollPhysics(),
        ),
        initialRoute: AppRoutes.login,
        onGenerateRoute: generateRoute,
      ),
    );
  }
}
