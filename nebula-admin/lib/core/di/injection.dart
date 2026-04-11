import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:http/http.dart' as http;

import '../../features/auth/data/datasources/auth_api_source.dart';
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

final localStorageProvider = Provider<LocalStorage>(
  (ref) => throw UnimplementedError(
    'localStorageProvider must be overridden with a pre-initialized instance',
  ),
);

// --- Server configuration ---

final serverUrlProvider = StateProvider<String>((ref) {
  final storage = ref.read(localStorageProvider);
  return storage.serverUrl;
});

// --- Authorized HTTP client ---

final authorizedClientProvider = Provider<http.Client>((ref) {
  final storage = ref.watch(localStorageProvider);
  return AuthorizedClient(storage: storage);
});

// --- Auth (JWT only, no Firebase) ---

final authRemoteSourceProvider = Provider<AuthRemoteSource>((ref) {
  final baseUrl = ref.watch(serverUrlProvider);
  final storage = ref.watch(localStorageProvider);
  return AuthApiSource(baseUrl: baseUrl, storage: storage);
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
