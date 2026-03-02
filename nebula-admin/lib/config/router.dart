import 'package:flutter/material.dart';

import '../features/auth/presentation/pages/login_page.dart';
import '../features/cluster/presentation/pages/cluster_detail_page.dart';
import '../features/cluster/presentation/pages/create_cluster_page.dart';
import '../features/dashboard/presentation/pages/shell_page.dart';

class AppRoutes {
  AppRoutes._();

  static const String login = '/login';
  static const String dashboard = '/dashboard';
  static const String clusterCreate = '/cluster/create';

  static String clusterDetail(String id) => '/cluster/$id';
}

Route<dynamic>? generateRoute(RouteSettings settings) {
  final uri = Uri.parse(settings.name ?? '');

  // Match /cluster/create first (before the parameterized route)
  if (settings.name == AppRoutes.clusterCreate) {
    return MaterialPageRoute<void>(
      builder: (_) => const CreateClusterPage(),
      settings: settings,
    );
  }

  // Match /cluster/:id
  if (uri.pathSegments.length == 2 && uri.pathSegments[0] == 'cluster') {
    final clusterId = uri.pathSegments[1];
    return MaterialPageRoute<void>(
      builder: (_) => ClusterDetailPage(clusterId: clusterId),
      settings: settings,
    );
  }

  switch (settings.name) {
    case AppRoutes.login:
      return MaterialPageRoute<void>(
        builder: (_) => const LoginPage(),
        settings: settings,
      );
    case AppRoutes.dashboard:
      return MaterialPageRoute<void>(
        builder: (_) => const ShellPage(),
        settings: settings,
      );
    default:
      return MaterialPageRoute<void>(
        builder: (_) => const LoginPage(),
        settings: settings,
      );
  }
}
