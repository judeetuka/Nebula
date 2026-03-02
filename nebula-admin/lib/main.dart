import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:nebula_ui/nebula_ui.dart';

import 'config/router.dart';

void main() {
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
