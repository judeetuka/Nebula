class WorkflowEdge {
  final String id;
  final String fromNodeId;
  final String toNodeId;
  final String? condition;

  const WorkflowEdge({
    required this.id,
    required this.fromNodeId,
    required this.toNodeId,
    this.condition,
  });

  WorkflowEdge copyWith({
    String? id,
    String? fromNodeId,
    String? toNodeId,
    String? condition,
  }) {
    return WorkflowEdge(
      id: id ?? this.id,
      fromNodeId: fromNodeId ?? this.fromNodeId,
      toNodeId: toNodeId ?? this.toNodeId,
      condition: condition ?? this.condition,
    );
  }

  Map<String, dynamic> toJson() {
    return {
      'id': id,
      'from_node_id': fromNodeId,
      'to_node_id': toNodeId,
      if (condition != null) 'condition': condition,
    };
  }

  factory WorkflowEdge.fromJson(Map<String, dynamic> json) {
    return WorkflowEdge(
      id: json['id'] as String,
      fromNodeId: json['from_node_id'] as String,
      toNodeId: json['to_node_id'] as String,
      condition: json['condition'] as String?,
    );
  }
}
