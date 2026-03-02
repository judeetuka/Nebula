import 'package:firebase_auth/firebase_auth.dart' as fb;
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../features/auth/data/datasources/auth_firebase_source.dart';
import '../../features/auth/data/datasources/auth_remote_source.dart';
import '../../features/auth/data/repositories/auth_repository_impl.dart';
import '../../features/auth/domain/repositories/auth_repository.dart';
import '../../features/auth/domain/usecases/sign_in.dart';
import '../../features/auth/domain/usecases/sign_out.dart';
import '../../features/cluster/data/datasources/cluster_api_source.dart';
import '../../features/cluster/data/datasources/cluster_remote_source.dart';
import '../../features/cluster/data/repositories/cluster_repository_impl.dart';
import '../../features/cluster/domain/repositories/cluster_repository.dart';
import '../../features/cluster/domain/usecases/create_cluster.dart';
import '../../features/cluster/domain/usecases/get_cluster_nodes.dart';
import '../../features/cluster/domain/usecases/get_clusters.dart';

// --- Server configuration ---

/// Base URL for the nebula-server REST API.
///
/// Override this provider to change the server endpoint at runtime (e.g.
/// from settings page). Defaults to localhost for development.
final serverUrlProvider = StateProvider<String>(
  (ref) => 'http://localhost:8080',
);

// --- Auth ---

/// Provides the auth data source.
///
/// If Firebase Auth is initialized and available, uses [AuthFirebaseSource].
/// Otherwise falls back to [AuthRemoteSourceStub] so the app can still run
/// during development without Firebase config.
final authRemoteSourceProvider = Provider<AuthRemoteSource>((ref) {
  try {
    // Attempt to access Firebase Auth -- throws if Firebase is not initialized.
    fb.FirebaseAuth.instance;
    return AuthFirebaseSource();
  } catch (_) {
    return AuthRemoteSourceStub();
  }
});

final authRepositoryProvider = Provider<AuthRepository>(
  (ref) => AuthRepositoryImpl(ref.watch(authRemoteSourceProvider)),
);

final signInUseCaseProvider = Provider<SignIn>(
  (ref) => SignIn(ref.watch(authRepositoryProvider)),
);

final signOutUseCaseProvider = Provider<SignOut>(
  (ref) => SignOut(ref.watch(authRepositoryProvider)),
);

// --- Cluster ---

/// Provides the cluster data source backed by real HTTP calls.
///
/// Reads the current server URL from [serverUrlProvider].
final clusterRemoteSourceProvider = Provider<ClusterRemoteSource>((ref) {
  final baseUrl = ref.watch(serverUrlProvider);
  return ClusterApiSource(baseUrl: baseUrl);
});

final clusterRepositoryProvider = Provider<ClusterRepository>(
  (ref) => ClusterRepositoryImpl(ref.watch(clusterRemoteSourceProvider)),
);

final getClustersUseCaseProvider = Provider<GetClusters>(
  (ref) => GetClusters(ref.watch(clusterRepositoryProvider)),
);

final createClusterUseCaseProvider = Provider<CreateCluster>(
  (ref) => CreateCluster(ref.watch(clusterRepositoryProvider)),
);

final getClusterNodesUseCaseProvider = Provider<GetClusterNodes>(
  (ref) => GetClusterNodes(ref.watch(clusterRepositoryProvider)),
);
