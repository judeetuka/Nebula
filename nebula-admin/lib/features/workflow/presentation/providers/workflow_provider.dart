import 'dart:ui';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../../core/di/injection.dart';
import '../../data/models/workflow_edge_model.dart';
import '../../data/models/workflow_model.dart';
import '../../data/models/workflow_node_model.dart';
import '../../data/repositories/workflow_repository.dart';

// ---------------------------------------------------------------------------
// Repository provider
// ---------------------------------------------------------------------------

final workflowRepositoryProvider = Provider<WorkflowRepository>((ref) {
  final storage = ref.watch(localStorageProvider);
  final client = ref.watch(authorizedClientProvider);
  final serverUrl = ref.watch(serverUrlProvider);
  return WorkflowRepository(
    storage: storage,
    client: client,
    baseUrlGetter: () => serverUrl,
  );
});

// ---------------------------------------------------------------------------
// Workflow list state
// ---------------------------------------------------------------------------

class WorkflowListState {
  final List<Workflow> workflows;
  final bool isLoading;
  final String? error;

  const WorkflowListState({
    this.workflows = const [],
    this.isLoading = false,
    this.error,
  });

  WorkflowListState copyWith({
    List<Workflow>? workflows,
    bool? isLoading,
    String? error,
  }) {
    return WorkflowListState(
      workflows: workflows ?? this.workflows,
      isLoading: isLoading ?? this.isLoading,
      error: error,
    );
  }
}

class WorkflowListNotifier extends StateNotifier<WorkflowListState> {
  final Ref _ref;

  WorkflowListNotifier(this._ref) : super(const WorkflowListState());

  void loadWorkflows() {
    state = state.copyWith(isLoading: true, error: null);
    try {
      final workflows = _ref.read(workflowRepositoryProvider).getAll();
      state = WorkflowListState(workflows: workflows);
    } on Exception catch (e) {
      state = state.copyWith(isLoading: false, error: e.toString());
    }
  }

  Future<void> deleteWorkflow(String id) async {
    await _ref.read(workflowRepositoryProvider).delete(id);
    loadWorkflows();
  }
}

final workflowListProvider =
    StateNotifierProvider<WorkflowListNotifier, WorkflowListState>(
      (ref) => WorkflowListNotifier(ref),
    );

// ---------------------------------------------------------------------------
// Editor state — manages the currently open workflow on the canvas
// ---------------------------------------------------------------------------

class WorkflowEditorState {
  final Workflow? workflow;
  final String? selectedNodeId;
  final String? connectingFromNodeId;
  final bool isDirty;
  final bool isSaving;
  final String? error;

  const WorkflowEditorState({
    this.workflow,
    this.selectedNodeId,
    this.connectingFromNodeId,
    this.isDirty = false,
    this.isSaving = false,
    this.error,
  });

  WorkflowEditorState copyWith({
    Workflow? workflow,
    String? selectedNodeId,
    String? connectingFromNodeId,
    bool? isDirty,
    bool? isSaving,
    String? error,
    bool clearSelectedNode = false,
    bool clearConnecting = false,
    bool clearError = false,
  }) {
    return WorkflowEditorState(
      workflow: workflow ?? this.workflow,
      selectedNodeId: clearSelectedNode
          ? null
          : (selectedNodeId ?? this.selectedNodeId),
      connectingFromNodeId: clearConnecting
          ? null
          : (connectingFromNodeId ?? this.connectingFromNodeId),
      isDirty: isDirty ?? this.isDirty,
      isSaving: isSaving ?? this.isSaving,
      error: clearError ? null : (error ?? this.error),
    );
  }

  WorkflowNode? get selectedNode {
    if (selectedNodeId == null || workflow == null) return null;
    return workflow!.nodes.where((n) => n.id == selectedNodeId).firstOrNull;
  }
}

class WorkflowEditorNotifier extends StateNotifier<WorkflowEditorState> {
  final Ref _ref;

  WorkflowEditorNotifier(this._ref) : super(const WorkflowEditorState());

  int _nodeCounter = 0;
  int _edgeCounter = 0;

  // -- Lifecycle -------

  void createNew(String name, String description) {
    final now = DateTime.now();
    final id = 'wf_${now.millisecondsSinceEpoch}';
    state = WorkflowEditorState(
      workflow: Workflow(
        id: id,
        name: name,
        description: description,
        createdAt: now,
        updatedAt: now,
      ),
    );
    _nodeCounter = 0;
    _edgeCounter = 0;
  }

  void loadExisting(String workflowId) {
    final wf = _ref.read(workflowRepositoryProvider).getById(workflowId);
    if (wf != null) {
      state = WorkflowEditorState(workflow: wf);
      _nodeCounter = wf.nodes.length;
      _edgeCounter = wf.edges.length;
    }
  }

  // -- Nodes -------

  void addNode(WorkflowNode template, Offset position) {
    if (state.workflow == null) return;
    _nodeCounter++;
    final node = template.copyWith(
      id: 'node_$_nodeCounter',
      position: position,
    );
    final updated = state.workflow!.copyWith(
      nodes: [...state.workflow!.nodes, node],
      updatedAt: DateTime.now(),
    );
    state = state.copyWith(workflow: updated, isDirty: true);
  }

  void moveNode(String nodeId, Offset position) {
    if (state.workflow == null) return;
    final nodes = state.workflow!.nodes.map((n) {
      if (n.id == nodeId) return n.copyWith(position: position);
      return n;
    }).toList();
    final updated = state.workflow!.copyWith(
      nodes: nodes,
      updatedAt: DateTime.now(),
    );
    state = state.copyWith(workflow: updated, isDirty: true);
  }

  void updateNodeConfig(String nodeId, Map<String, dynamic> config) {
    if (state.workflow == null) return;
    final nodes = state.workflow!.nodes.map((n) {
      if (n.id == nodeId) return n.copyWith(config: config);
      return n;
    }).toList();
    final updated = state.workflow!.copyWith(
      nodes: nodes,
      updatedAt: DateTime.now(),
    );
    state = state.copyWith(workflow: updated, isDirty: true);
  }

  void removeNode(String nodeId) {
    if (state.workflow == null) return;
    final nodes = state.workflow!.nodes.where((n) => n.id != nodeId).toList();
    final edges = state.workflow!.edges
        .where((e) => e.fromNodeId != nodeId && e.toNodeId != nodeId)
        .toList();
    final updated = state.workflow!.copyWith(
      nodes: nodes,
      edges: edges,
      updatedAt: DateTime.now(),
    );
    state = state.copyWith(
      workflow: updated,
      isDirty: true,
      clearSelectedNode: nodeId == state.selectedNodeId,
    );
  }

  void selectNode(String? nodeId) {
    state = state.copyWith(
      selectedNodeId: nodeId,
      clearSelectedNode: nodeId == null,
    );
  }

  // -- Edges (connections) -------

  void startConnecting(String fromNodeId) {
    state = state.copyWith(connectingFromNodeId: fromNodeId);
  }

  void finishConnecting(String toNodeId) {
    final fromId = state.connectingFromNodeId;
    if (fromId == null || state.workflow == null || fromId == toNodeId) {
      cancelConnecting();
      return;
    }

    // Prevent duplicate edges.
    final exists = state.workflow!.edges.any(
      (e) => e.fromNodeId == fromId && e.toNodeId == toNodeId,
    );
    if (exists) {
      cancelConnecting();
      return;
    }

    _edgeCounter++;
    final edge = WorkflowEdge(
      id: 'edge_$_edgeCounter',
      fromNodeId: fromId,
      toNodeId: toNodeId,
    );
    final updated = state.workflow!.copyWith(
      edges: [...state.workflow!.edges, edge],
      updatedAt: DateTime.now(),
    );
    state = state.copyWith(
      workflow: updated,
      isDirty: true,
      clearConnecting: true,
    );
  }

  void cancelConnecting() {
    state = state.copyWith(clearConnecting: true);
  }

  void removeEdge(String edgeId) {
    if (state.workflow == null) return;
    final edges = state.workflow!.edges.where((e) => e.id != edgeId).toList();
    final updated = state.workflow!.copyWith(
      edges: edges,
      updatedAt: DateTime.now(),
    );
    state = state.copyWith(workflow: updated, isDirty: true);
  }

  // -- Persistence -------

  Future<void> save() async {
    if (state.workflow == null) return;
    state = state.copyWith(isSaving: true, clearError: true);
    try {
      await _ref.read(workflowRepositoryProvider).save(state.workflow!);
      state = state.copyWith(isSaving: false, isDirty: false);
    } on Exception catch (e) {
      state = state.copyWith(isSaving: false, error: e.toString());
    }
  }

  Future<void> submitToCluster(String clusterId) async {
    if (state.workflow == null) return;
    state = state.copyWith(isSaving: true, clearError: true);
    try {
      // Save locally first.
      await _ref.read(workflowRepositoryProvider).save(state.workflow!);
      // Then submit remotely.
      await _ref
          .read(workflowRepositoryProvider)
          .submitToCluster(clusterId: clusterId, workflow: state.workflow!);
      state = state.copyWith(isSaving: false, isDirty: false);
    } on Exception catch (e) {
      state = state.copyWith(isSaving: false, error: e.toString());
    }
  }

  void clear() {
    if (state.workflow == null) return;
    final updated = state.workflow!.copyWith(
      nodes: [],
      edges: [],
      updatedAt: DateTime.now(),
    );
    state = WorkflowEditorState(workflow: updated, isDirty: true);
    _nodeCounter = 0;
    _edgeCounter = 0;
  }
}

final workflowEditorProvider =
    StateNotifierProvider<WorkflowEditorNotifier, WorkflowEditorState>(
      (ref) => WorkflowEditorNotifier(ref),
    );
