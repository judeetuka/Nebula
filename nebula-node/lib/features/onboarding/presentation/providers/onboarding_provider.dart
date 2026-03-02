import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../engine/presentation/providers/engine_provider.dart';
import '../../data/datasources/qr_scanner_source.dart';
import '../../domain/entities/qr_payload.dart';
import '../../domain/usecases/scan_and_join.dart';

/// Provides a [QrScannerSource] that manages the camera controller lifecycle.
///
/// Auto-disposes the controller when the provider is no longer watched,
/// preventing camera resource leaks.
final qrScannerSourceProvider =
    Provider.autoDispose<QrScannerSource>((ref) {
  final source = QrScannerSource();
  ref.onDispose(() => source.dispose());
  return source;
});

final scanAndJoinUseCaseProvider = Provider<ScanAndJoin>((ref) {
  final repository = ref.watch(engineRepositoryProvider);
  return ScanAndJoin(repository);
});

final joinClusterProvider =
    FutureProvider.family<void, QrPayload>((ref, payload) async {
  final scanAndJoin = ref.watch(scanAndJoinUseCaseProvider);
  await scanAndJoin(payload);
});
