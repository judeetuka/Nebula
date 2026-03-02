import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../features/auth/data/datasources/auth_remote_source.dart';
import '../../features/auth/data/repositories/auth_repository_impl.dart';
import '../../features/auth/domain/repositories/auth_repository.dart';
import '../../features/auth/domain/usecases/sign_in.dart';
import '../../features/auth/domain/usecases/sign_out.dart';
import '../../features/cluster/data/datasources/cluster_remote_source.dart';
import '../../features/cluster/data/repositories/cluster_repository_impl.dart';
import '../../features/cluster/domain/repositories/cluster_repository.dart';
import '../../features/cluster/domain/usecases/create_cluster.dart';
import '../../features/cluster/domain/usecases/get_cluster_nodes.dart';
import '../../features/cluster/domain/usecases/get_clusters.dart';

// --- Auth ---

final authRemoteSourceProvider = Provider<AuthRemoteSource>(
  (ref) => AuthRemoteSourceStub(),
);

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

final clusterRemoteSourceProvider = Provider<ClusterRemoteSource>(
  (ref) => ClusterRemoteSourceStub(),
);

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
