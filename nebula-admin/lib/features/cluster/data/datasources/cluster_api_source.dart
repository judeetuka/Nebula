import 'dart:convert';

import 'package:http/http.dart' as http;

import '../../../../core/error/failures.dart';
import '../../domain/entities/cluster.dart';
import '../../domain/entities/node_info.dart';
import 'cluster_remote_source.dart';

/// Real HTTP implementation of [ClusterRemoteSource].
///
/// Communicates with the nebula-server REST API to manage clusters and
/// retrieve node information. Accepts an [http.Client] so the DI layer
/// can inject an [AuthorizedClient] that adds JWT headers automatically.
class ClusterApiSource implements ClusterRemoteSource {
  final String baseUrl;
  final http.Client _client;

  ClusterApiSource({required this.baseUrl, required http.Client client})
    : _client = client;

  @override
  Future<List<Cluster>> getClusters() async {
    final response = await _client.get(Uri.parse('$baseUrl/api/clusters'));
    if (response.statusCode != 200) {
      throw ClusterFailure('Failed to fetch clusters: ${response.statusCode}');
    }
    final decoded = jsonDecode(response.body);
    // Server returns {"clusters": [...]} — extract the list
    final List<dynamic> data = decoded is List
        ? decoded
        : (decoded['clusters'] as List<dynamic>?) ?? [];
    return data
        .map(
          (json) => Cluster(
            id: (json['cluster_id'] ?? json['id'] ?? '') as String,
            name: (json['name'] ?? json['cluster_id'] ?? '') as String,
            nodeCount: (json['node_count'] ?? 0) as int,
            serverUrl: baseUrl,
            createdAt:
                DateTime.tryParse('${json['created_at'] ?? ''}') ??
                DateTime.now(),
          ),
        )
        .toList();
  }

  @override
  Future<Cluster> createCluster({required String name}) async {
    final response = await _client.post(
      Uri.parse('$baseUrl/api/clusters'),
      headers: {'Content-Type': 'application/json'},
      body: jsonEncode({'name': name}),
    );
    if (response.statusCode != 200 && response.statusCode != 201) {
      throw ClusterFailure('Failed to create cluster: ${response.statusCode}');
    }
    final json = jsonDecode(response.body) as Map<String, dynamic>;
    return Cluster(
      id: (json['cluster_id'] ?? json['id'] ?? '') as String,
      name: name,
      nodeCount: 0,
      serverUrl: baseUrl,
      createdAt: DateTime.now(),
    );
  }

  @override
  Future<List<NodeInfo>> getClusterNodes({required String clusterId}) async {
    final response = await _client.get(
      Uri.parse('$baseUrl/api/clusters/$clusterId/nodes'),
    );
    if (response.statusCode != 200) {
      throw ClusterFailure('Failed to fetch nodes: ${response.statusCode}');
    }
    final decoded = jsonDecode(response.body);
    final List<dynamic> data = decoded is List
        ? decoded
        : (decoded['nodes'] as List<dynamic>?) ?? [];
    return data
        .map(
          (json) => NodeInfo(
            nodeId: (json['node_id'] ?? '') as String,
            role: (json['role'] ?? 'Worker') as String,
            batteryLevel: (json['battery_level'] ?? 0) as int,
            cpuLoad: ((json['cpu_load'] ?? 0.0) as num).toDouble(),
            status: (json['status'] ?? 'unknown') as String,
            lastSeen:
                DateTime.tryParse((json['last_seen'] ?? '') as String) ??
                DateTime.now(),
          ),
        )
        .toList();
  }
}
