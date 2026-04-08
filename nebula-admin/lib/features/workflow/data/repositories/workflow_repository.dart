import 'dart:convert';

import 'package:http/http.dart' as http;

import '../../../../core/error/failures.dart';
import '../../../../core/storage/local_storage.dart';
import '../models/workflow_model.dart';

/// Persists workflows locally via Hive CE and can submit them as tasks to a
/// nebula-server cluster endpoint.
class WorkflowRepository {
  final LocalStorage _storage;
  final http.Client _client;
  final String Function() _baseUrlGetter;

  static const String _storageKey = 'workflows';

  WorkflowRepository({
    required LocalStorage storage,
    required http.Client client,
    required String Function() baseUrlGetter,
  }) : _storage = storage,
       _client = client,
       _baseUrlGetter = baseUrlGetter;

  // ---------------------------------------------------------------------------
  // Local persistence (Hive CE cache box)
  // ---------------------------------------------------------------------------

  List<Workflow> getAll() {
    final raw = _storage.getCachedJson(_storageKey);
    if (raw == null) return [];
    final list = (raw as List<dynamic>).cast<Map<dynamic, dynamic>>();
    return list.map((m) {
      final json = _deepCastMap(m);
      return Workflow.fromJson(json);
    }).toList();
  }

  Workflow? getById(String id) {
    return getAll().where((w) => w.id == id).firstOrNull;
  }

  Future<void> save(Workflow workflow) async {
    final all = getAll();
    final idx = all.indexWhere((w) => w.id == workflow.id);
    if (idx >= 0) {
      all[idx] = workflow;
    } else {
      all.add(workflow);
    }
    await _storage.cacheJson(_storageKey, all.map((w) => w.toJson()).toList());
  }

  Future<void> delete(String id) async {
    final all = getAll().where((w) => w.id != id).toList();
    await _storage.cacheJson(_storageKey, all.map((w) => w.toJson()).toList());
  }

  // ---------------------------------------------------------------------------
  // Remote — submit workflow as a task to a cluster
  // ---------------------------------------------------------------------------

  Future<void> submitToCluster({
    required String clusterId,
    required Workflow workflow,
  }) async {
    final baseUrl = _baseUrlGetter();
    final uri = Uri.parse('$baseUrl/api/clusters/$clusterId/tasks');
    final response = await _client.post(
      uri,
      headers: {'Content-Type': 'application/json'},
      body: jsonEncode(workflow.toTaskPayload()),
    );
    if (response.statusCode != 200 && response.statusCode != 201) {
      throw WorkflowFailure(
        'Failed to submit workflow: ${response.statusCode}',
      );
    }
  }

  /// Recursively casts Hive's `Map<dynamic, dynamic>` to
  /// `Map<String, dynamic>` so that `fromJson` factories work.
  Map<String, dynamic> _deepCastMap(Map<dynamic, dynamic> source) {
    return source.map((key, value) {
      final castKey = key.toString();
      if (value is Map) {
        return MapEntry(castKey, _deepCastMap(value));
      } else if (value is List) {
        return MapEntry(castKey, _deepCastList(value));
      }
      return MapEntry(castKey, value);
    });
  }

  List<dynamic> _deepCastList(List<dynamic> source) {
    return source.map((value) {
      if (value is Map) {
        return _deepCastMap(value);
      } else if (value is List) {
        return _deepCastList(value);
      }
      return value;
    }).toList();
  }
}
