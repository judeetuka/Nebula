import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../../core/di/injection.dart';
import '../../../../core/usecases/usecase.dart';
import '../../domain/entities/cluster.dart';
import '../../domain/entities/node_info.dart';
import '../../domain/usecases/create_cluster.dart';
import '../../domain/usecases/get_cluster_nodes.dart';

// --- Clusters list ---

class ClustersState {
  final List<Cluster> clusters;
  final bool isLoading;
  final String? error;

  const ClustersState({
    this.clusters = const [],
    this.isLoading = false,
    this.error,
  });

  ClustersState copyWith({
    List<Cluster>? clusters,
    bool? isLoading,
    String? error,
  }) {
    return ClustersState(
      clusters: clusters ?? this.clusters,
      isLoading: isLoading ?? this.isLoading,
      error: error,
    );
  }
}

class ClustersNotifier extends StateNotifier<ClustersState> {
  final Ref _ref;

  ClustersNotifier(this._ref) : super(const ClustersState());

  Future<void> loadClusters() async {
    state = state.copyWith(isLoading: true, error: null);
    try {
      final clusters =
          await _ref.read(getClustersUseCaseProvider).call(const NoParams());
      state = ClustersState(clusters: clusters);
    } on Exception catch (e) {
      state = state.copyWith(isLoading: false, error: e.toString());
    }
  }

  Future<bool> createCluster({required String name}) async {
    try {
      await _ref
          .read(createClusterUseCaseProvider)
          .call(CreateClusterParams(name: name));
      await loadClusters();
      return true;
    } on Exception catch (e) {
      state = state.copyWith(error: e.toString());
      return false;
    }
  }
}

final clustersProvider =
    StateNotifierProvider<ClustersNotifier, ClustersState>(
  (ref) => ClustersNotifier(ref),
);

// --- Cluster nodes ---

class ClusterNodesState {
  final List<NodeInfo> nodes;
  final bool isLoading;
  final String? error;

  const ClusterNodesState({
    this.nodes = const [],
    this.isLoading = false,
    this.error,
  });

  ClusterNodesState copyWith({
    List<NodeInfo>? nodes,
    bool? isLoading,
    String? error,
  }) {
    return ClusterNodesState(
      nodes: nodes ?? this.nodes,
      isLoading: isLoading ?? this.isLoading,
      error: error,
    );
  }
}

class ClusterNodesNotifier extends StateNotifier<ClusterNodesState> {
  final Ref _ref;

  ClusterNodesNotifier(this._ref) : super(const ClusterNodesState());

  Future<void> loadNodes({required String clusterId}) async {
    state = state.copyWith(isLoading: true, error: null);
    try {
      final nodes = await _ref
          .read(getClusterNodesUseCaseProvider)
          .call(GetClusterNodesParams(clusterId: clusterId));
      state = ClusterNodesState(nodes: nodes);
    } on Exception catch (e) {
      state = state.copyWith(isLoading: false, error: e.toString());
    }
  }
}

final clusterNodesProvider =
    StateNotifierProvider<ClusterNodesNotifier, ClusterNodesState>(
  (ref) => ClusterNodesNotifier(ref),
);
