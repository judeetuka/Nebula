import 'dart:ui';

class WorkflowNode {
  final String id;
  final String type;
  final String pluginId;
  final String action;
  final Map<String, dynamic> config;
  final Offset position;
  final String label;

  static const typePlugin = 'plugin';
  static const typeCondition = 'condition';
  static const typeDelay = 'delay';
  static const typeTrigger = 'trigger';
  static const typeLoop = 'loop';

  const WorkflowNode({
    required this.id,
    required this.type,
    required this.pluginId,
    required this.action,
    this.config = const {},
    this.position = Offset.zero,
    required this.label,
  });

  WorkflowNode copyWith({
    String? id,
    String? type,
    String? pluginId,
    String? action,
    Map<String, dynamic>? config,
    Offset? position,
    String? label,
  }) {
    return WorkflowNode(
      id: id ?? this.id,
      type: type ?? this.type,
      pluginId: pluginId ?? this.pluginId,
      action: action ?? this.action,
      config: config ?? this.config,
      position: position ?? this.position,
      label: label ?? this.label,
    );
  }

  Map<String, dynamic> toJson() {
    return {
      'id': id,
      'type': type,
      'plugin_id': pluginId,
      'action': action,
      'config': config,
      'position': {'x': position.dx, 'y': position.dy},
      'label': label,
    };
  }

  factory WorkflowNode.fromJson(Map<String, dynamic> json) {
    final pos = json['position'] as Map<String, dynamic>?;
    return WorkflowNode(
      id: json['id'] as String,
      type: json['type'] as String? ?? typePlugin,
      pluginId: json['plugin_id'] as String,
      action: json['action'] as String,
      config: (json['config'] as Map<String, dynamic>?) ?? const {},
      position: pos != null
          ? Offset((pos['x'] as num).toDouble(), (pos['y'] as num).toDouble())
          : Offset.zero,
      label: json['label'] as String,
    );
  }
}
