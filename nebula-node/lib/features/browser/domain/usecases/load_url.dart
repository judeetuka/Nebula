import '../../../../core/usecases/usecase.dart';
import '../repositories/browser_repository.dart';

class LoadUrlParams {
  final String sessionId;
  final String url;

  const LoadUrlParams({required this.sessionId, required this.url});
}

class LoadUrl extends UseCase<void, LoadUrlParams> {
  final BrowserRepository repository;

  LoadUrl(this.repository);

  @override
  Future<void> call(LoadUrlParams params) {
    return repository.loadUrl(params.sessionId, params.url);
  }
}
