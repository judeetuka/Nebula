import '../../../../core/usecases/usecase.dart';
import '../repositories/browser_repository.dart';

class ExecuteJavaScriptParams {
  final String sessionId;
  final String script;

  const ExecuteJavaScriptParams({
    required this.sessionId,
    required this.script,
  });
}

class ExecuteJavaScript extends UseCase<String?, ExecuteJavaScriptParams> {
  final BrowserRepository repository;

  ExecuteJavaScript(this.repository);

  @override
  Future<String?> call(ExecuteJavaScriptParams params) {
    return repository.executeJavaScript(params.sessionId, params.script);
  }
}
