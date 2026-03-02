/// Central dependency injection barrel file.
///
/// Re-exports all Riverpod providers so that features can be imported
/// from a single location when needed.
///
/// Individual feature providers are defined close to their features
/// (engine_provider.dart, onboarding_provider.dart, browser_provider.dart)
/// following Clean Architecture conventions. This file serves as a
/// convenience barrel for cross-feature access.
library;

export '../../features/engine/presentation/providers/engine_provider.dart';
export '../../features/onboarding/presentation/providers/onboarding_provider.dart';
export '../../features/browser/presentation/providers/browser_provider.dart';
