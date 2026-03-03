import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../data/datasources/engine_native_source.dart';
import '../../data/repositories/engine_repository_impl.dart';
import '../../domain/entities/node_status.dart';
import '../../domain/repositories/engine_repository.dart';

final engineNativeSourceProvider = Provider<EngineNativeSource>((ref) {
  return EngineNativeSource();
});

final engineRepositoryProvider = Provider<EngineRepository>((ref) {
  final nativeSource = ref.watch(engineNativeSourceProvider);
  return EngineRepositoryImpl(nativeSource);
});

final engineInitProvider = FutureProvider<String>((ref) async {
  final repository = ref.watch(engineRepositoryProvider);
  return repository.initEngine('/data/nebula');
});

final nodeStatusProvider = FutureProvider<NodeStatus>((ref) async {
  // Ensure the engine is initialized before fetching status.
  await ref.watch(engineInitProvider.future);
  final repository = ref.watch(engineRepositoryProvider);
  return repository.getNodeStatus();
});

final nodeStatusStreamProvider = StreamProvider<NodeStatus>((ref) async* {
  // Wait for engine init
  await ref.watch(engineInitProvider.future);

  final repo = ref.watch(engineRepositoryProvider);

  while (true) {
    try {
      final status = await repo.getNodeStatus();
      yield status;
    } catch (_) {
      // On error, wait and retry — the stream stays alive
    }
    await Future.delayed(const Duration(seconds: 2));
  }
});

final isConfiguredProvider = FutureProvider<bool>((ref) async {
  await ref.watch(engineInitProvider.future);
  final repository = ref.watch(engineRepositoryProvider);
  return repository.isConfigured();
});
