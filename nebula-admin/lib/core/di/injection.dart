import 'package:firebase_auth/firebase_auth.dart' as fb;
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:http/http.dart' as http;

import '../../features/auth/data/datasources/auth_api_source.dart';
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
import '../network/authorized_client.dart';
import '../storage/local_storage.dart';

// --- Local storage ---

/// Singleton [LocalStorage] instance. Must be initialized in main() before
/// the ProviderScope is created, then set via override.
final localStorageProvider = Provider<LocalStorage>(
  (ref) => throw UnimplementedError(
    'localStorageProvider must be overridden with a pre-initialized instance',
  ),
);

// --- Server configuration ---

/// Base URL for the nebula-server REST API.
///
/// Reads the persisted value from Hive on first access. Runtime changes are
/// written through to Hive by the settings page.
final serverUrlProvider = StateProvider<String>((ref) {
  final storage = ref.read(localStorageProvider);
  return storage.serverUrl;
});

// --- Authorized HTTP client ---

/// An [http.Client] that attaches `Authorization: Bearer <token>` to every
/// outbound request when a JWT is stored in [LocalStorage].
final authorizedClientProvider = Provider<http.Client>((ref) {
  final storage = ref.watch(localStorageProvider);
  return AuthorizedClient(storage: storage);
});

// --- Auth ---

/// Provides the auth data source.
///
/// Priority order:
///   1. JWT API source (talks to nebula-server /api/auth/*)
///   2. Firebase Auth (if initialized)
///   3. Stub fallback for local development
final authRemoteSourceProvider = Provider<AuthRemoteSource>((ref) {
  final baseUrl = ref.watch(serverUrlProvider);
  final storage = ref.watch(localStorageProvider);

  // If a JWT token already exists, or the server URL is set,
  // prefer the API auth source.
  return AuthApiSource(baseUrl: baseUrl, storage: storage);
});

/// Firebase-backed auth source. Used as fallback if needed.
final firebaseAuthSourceProvider = Provider<AuthRemoteSource?>((ref) {
  try {
    fb.FirebaseAuth.instance;
    return AuthFirebaseSource();
  } catch (_) {
    return null;
  }
});

final authRepositoryProvider = Provider<AuthRepository>((ref) {
  final source = ref.watch(authRemoteSourceProvider);
  final storage = ref.watch(localStorageProvider);
  return AuthRepositoryImpl(source, storage);
});

final signInUseCaseProvider = Provider<SignIn>(
  (ref) => SignIn(ref.watch(authRepositoryProvider)),
);

final signOutUseCaseProvider = Provider<SignOut>(
  (ref) => SignOut(ref.watch(authRepositoryProvider)),
);

// --- Cluster ---

/// Provides the cluster data source backed by real HTTP calls.
///
/// Uses the [authorizedClientProvider] so every request carries JWT auth.
final clusterRemoteSourceProvider = Provider<ClusterRemoteSource>((ref) {
  final baseUrl = ref.watch(serverUrlProvider);
  final client = ref.watch(authorizedClientProvider);
  return ClusterApiSource(baseUrl: baseUrl, client: client);
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
