import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:nebula_ui/nebula_ui.dart';

import 'config/router.dart';
import 'features/engine/presentation/providers/engine_provider.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  runApp(const ProviderScope(child: NebulaNodeApp()));
}

class NebulaNodeApp extends ConsumerWidget {
  const NebulaNodeApp({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final isConfigured = ref.watch(isConfiguredProvider);

    final initialRoute = isConfigured.when(
      data: (configured) =>
          configured ? AppRoutes.status : AppRoutes.welcome,
      loading: () => AppRoutes.welcome,
      error: (_, _) => AppRoutes.welcome,
    );

    return MaterialApp(
      title: 'NEBULA Node',
      debugShowCheckedModeBanner: false,
      theme: NebulaTheme.lightTheme,
      darkTheme: NebulaTheme.darkTheme,
      themeMode: ThemeMode.dark,
      initialRoute: initialRoute,
      onGenerateRoute: onGenerateRoute,
    );
  }
}
