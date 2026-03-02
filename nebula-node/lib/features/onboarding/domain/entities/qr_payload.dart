class QrPayload {
  final int version;
  final String clusterId;
  final String serverUrl;
  final String authToken;

  const QrPayload({
    required this.version,
    required this.clusterId,
    required this.serverUrl,
    required this.authToken,
  });

  factory QrPayload.fromJson(Map<String, dynamic> json) {
    return QrPayload(
      version: json['version'] as int,
      clusterId: json['cluster_id'] as String,
      serverUrl: json['server_url'] as String,
      authToken: json['auth_token'] as String,
    );
  }

  Map<String, dynamic> toJson() {
    return {
      'version': version,
      'cluster_id': clusterId,
      'server_url': serverUrl,
      'auth_token': authToken,
    };
  }
}
