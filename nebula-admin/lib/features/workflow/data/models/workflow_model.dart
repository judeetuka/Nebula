import 'workflow_edge_model.dart';
import 'workflow_node_model.dart';

class Workflow {
  final String id;
  final String name;
  final String description;
  final List<WorkflowNode> nodes;
  final List<WorkflowEdge> edges;
  final DateTime createdAt;
  final DateTime updatedAt;

  const Workflow({
    required this.id,
    required this.name,
    this.description = '',
    this.nodes = const [],
    this.edges = const [],
    required this.createdAt,
    required this.updatedAt,
  });

  Workflow copyWith({
    String? id,
    String? name,
    String? description,
    List<WorkflowNode>? nodes,
    List<WorkflowEdge>? edges,
    DateTime? createdAt,
    DateTime? updatedAt,
  }) {
    return Workflow(
      id: id ?? this.id,
      name: name ?? this.name,
      description: description ?? this.description,
      nodes: nodes ?? this.nodes,
      edges: edges ?? this.edges,
      createdAt: createdAt ?? this.createdAt,
      updatedAt: updatedAt ?? this.updatedAt,
    );
  }

  Map<String, dynamic> toJson() {
    return {
      'id': id,
      'name': name,
      'description': description,
      'nodes': nodes.map((n) => n.toJson()).toList(),
      'edges': edges.map((e) => e.toJson()).toList(),
      'created_at': createdAt.toIso8601String(),
      'updated_at': updatedAt.toIso8601String(),
    };
  }

  factory Workflow.fromJson(Map<String, dynamic> json) {
    return Workflow(
      id: json['id'] as String,
      name: json['name'] as String,
      description: (json['description'] as String?) ?? '',
      nodes:
          (json['nodes'] as List<dynamic>?)
              ?.map((n) => WorkflowNode.fromJson(n as Map<String, dynamic>))
              .toList() ??
          const [],
      edges:
          (json['edges'] as List<dynamic>?)
              ?.map((e) => WorkflowEdge.fromJson(e as Map<String, dynamic>))
              .toList() ??
          const [],
      createdAt:
          DateTime.tryParse((json['created_at'] as String?) ?? '') ??
          DateTime.now(),
      updatedAt:
          DateTime.tryParse((json['updated_at'] as String?) ?? '') ??
          DateTime.now(),
    );
  }

  /// Serializes to the task payload format expected by nebula-server.
  Map<String, dynamic> toTaskPayload() {
    return {'type': 'workflow', 'workflow': toJson()};
  }
}
