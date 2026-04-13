import 'dart:ui';
import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:manny_ui/manny_ui.dart';

import '../../../../config/router.dart';
import '../providers/auth_provider.dart';

class LoginPage extends ConsumerStatefulWidget {
  const LoginPage({super.key});

  @override
  ConsumerState<LoginPage> createState() => _LoginPageState();
}

class _LoginPageState extends ConsumerState<LoginPage>
    with SingleTickerProviderStateMixin {
  final _emailController = TextEditingController();
  final _passwordController = TextEditingController();
  final _formKey = GlobalKey<FormState>();
  bool _obscurePassword = true;
  late AnimationController _glowController;

  @override
  void initState() {
    super.initState();
    _glowController = AnimationController(
      vsync: this,
      duration: const Duration(seconds: 3),
    )..repeat(reverse: true);
  }

  @override
  void dispose() {
    _emailController.dispose();
    _passwordController.dispose();
    _glowController.dispose();
    super.dispose();
  }

  Future<void> _handleSignIn() async {
    if (!_formKey.currentState!.validate()) return;

    final success = await ref
        .read(authProvider.notifier)
        .signIn(
          email: _emailController.text.trim(),
          password: _passwordController.text,
        );

    if (!mounted) return;

    if (success) {
      NotificationToast.success(context, 'Signed in successfully');
      Navigator.of(context).pushReplacementNamed(AppRoutes.dashboard);
    } else {
      final error = ref.read(authProvider).error ?? 'Sign-in failed';
      NotificationToast.error(context, error);
    }
  }

  @override
  Widget build(BuildContext context) {
    final authState = ref.watch(authProvider);
    final theme = Theme.of(context);
    final cs = theme.colorScheme;
    final isDark = cs.brightness == Brightness.dark;

    return Scaffold(
      backgroundColor: isDark
          ? const Color(0xFF0D0D12)
          : const Color(0xFFF0F2F5),
      body: Stack(
        children: [
          // Animated gradient background orbs
          _buildBackgroundOrbs(cs, isDark),

          // Main content
          Center(
            child: SingleChildScrollView(
              padding: const EdgeInsets.all(24),
              child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 420),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    // Logo with glow
                    AnimatedBuilder(
                      animation: _glowController,
                      builder: (context, child) {
                        final glow = _glowController.value * 0.4 + 0.1;
                        return Container(
                          width: 80,
                          height: 80,
                          decoration: BoxDecoration(
                            shape: BoxShape.circle,
                            gradient: RadialGradient(
                              colors: [
                                cs.primary.withValues(alpha: glow),
                                Colors.transparent,
                              ],
                              radius: 1.5,
                            ),
                          ),
                          child: Icon(
                            Icons.cloud_circle_rounded,
                            size: 56,
                            color: cs.primary,
                          ),
                        );
                      },
                    ),

                    const SizedBox(height: 16),

                    Text(
                      'NEBULA',
                      style: theme.textTheme.headlineMedium?.copyWith(
                        fontWeight: FontWeight.w800,
                        letterSpacing: 4,
                        color: cs.onSurface,
                      ),
                    ),
                    const SizedBox(height: 4),
                    Text(
                      'Admin Dashboard',
                      style: theme.textTheme.bodyMedium?.copyWith(
                        color: cs.onSurfaceVariant.withValues(alpha: 0.7),
                        letterSpacing: 1.5,
                      ),
                    ),

                    const SizedBox(height: 40),

                    // Frosted glass login card
                    FrostedGlass(
                      borderRadius: BorderRadius.circular(24),
                      blurSigma: 25,
                      opacity: isDark ? 0.08 : 0.15,
                      padding: const EdgeInsets.all(28),
                      child: Form(
                        key: _formKey,
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [
                            Text(
                              'Sign In',
                              style: theme.textTheme.titleLarge?.copyWith(
                                fontWeight: FontWeight.w700,
                              ),
                            ),
                            const SizedBox(height: 6),
                            Text(
                              'Manage your compute clusters',
                              style: theme.textTheme.bodySmall?.copyWith(
                                color: cs.onSurfaceVariant.withValues(
                                  alpha: 0.6,
                                ),
                              ),
                            ),
                            const SizedBox(height: 28),

                            // Email field
                            _buildGlassTextField(
                              controller: _emailController,
                              label: 'Email',
                              icon: IconlyLight.message,
                              keyboardType: TextInputType.emailAddress,
                              textInputAction: TextInputAction.next,
                              isDark: isDark,
                              cs: cs,
                              validator: (v) => v == null || v.trim().isEmpty
                                  ? 'Email is required'
                                  : null,
                            ),
                            const SizedBox(height: 16),

                            // Password field
                            _buildGlassTextField(
                              controller: _passwordController,
                              label: 'Password',
                              icon: IconlyLight.lock,
                              obscureText: _obscurePassword,
                              textInputAction: TextInputAction.done,
                              isDark: isDark,
                              cs: cs,
                              onFieldSubmitted: (_) => _handleSignIn(),
                              validator: (v) => v == null || v.isEmpty
                                  ? 'Password is required'
                                  : null,
                              suffixIcon: IconButton(
                                icon: Icon(
                                  _obscurePassword
                                      ? IconlyLight.show
                                      : IconlyLight.hide,
                                  color: cs.onSurfaceVariant.withValues(
                                    alpha: 0.5,
                                  ),
                                  size: 20,
                                ),
                                onPressed: () => setState(
                                  () => _obscurePassword = !_obscurePassword,
                                ),
                              ),
                            ),

                            const SizedBox(height: 28),

                            // Sign in button
                            SizedBox(
                              height: 52,
                              child: DecoratedBox(
                                decoration: BoxDecoration(
                                  borderRadius: BorderRadius.circular(16),
                                  gradient: LinearGradient(
                                    colors: [
                                      cs.primary,
                                      cs.primary.withValues(alpha: 0.8),
                                    ],
                                  ),
                                  boxShadow: [
                                    BoxShadow(
                                      color: cs.primary.withValues(alpha: 0.3),
                                      blurRadius: 16,
                                      offset: const Offset(0, 6),
                                    ),
                                  ],
                                ),
                                child: Material(
                                  color: Colors.transparent,
                                  child: InkWell(
                                    borderRadius: BorderRadius.circular(16),
                                    onTap: authState.isLoading
                                        ? null
                                        : _handleSignIn,
                                    child: Center(
                                      child: authState.isLoading
                                          ? SizedBox(
                                              height: 20,
                                              width: 20,
                                              child: CupertinoActivityIndicator(
                                                color: cs.onPrimary,
                                              ),
                                            )
                                          : Text(
                                              'Sign In',
                                              style: TextStyle(
                                                color: cs.onPrimary,
                                                fontWeight: FontWeight.w600,
                                                fontSize: 15,
                                                letterSpacing: 0.5,
                                              ),
                                            ),
                                    ),
                                  ),
                                ),
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),

                    const SizedBox(height: 24),

                    // Server status indicator
                    FrostedGlass(
                      borderRadius: BorderRadius.circular(12),
                      blurSigma: 15,
                      opacity: isDark ? 0.06 : 0.1,
                      padding: const EdgeInsets.symmetric(
                        horizontal: 16,
                        vertical: 10,
                      ),
                      child: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Container(
                            width: 8,
                            height: 8,
                            decoration: BoxDecoration(
                              shape: BoxShape.circle,
                              color: MannyTheme.tertiaryTeal,
                              boxShadow: [
                                BoxShadow(
                                  color: MannyTheme.tertiaryTeal.withValues(
                                    alpha: 0.5,
                                  ),
                                  blurRadius: 6,
                                ),
                              ],
                            ),
                          ),
                          const SizedBox(width: 8),
                          Text(
                            'Server: localhost:8080',
                            style: theme.textTheme.bodySmall?.copyWith(
                              color: cs.onSurfaceVariant.withValues(alpha: 0.6),
                              fontSize: 12,
                            ),
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildGlassTextField({
    required TextEditingController controller,
    required String label,
    required IconData icon,
    required bool isDark,
    required ColorScheme cs,
    TextInputType? keyboardType,
    TextInputAction? textInputAction,
    bool obscureText = false,
    Widget? suffixIcon,
    void Function(String)? onFieldSubmitted,
    String? Function(String?)? validator,
  }) {
    return ClipRRect(
      borderRadius: BorderRadius.circular(14),
      child: BackdropFilter(
        filter: ImageFilter.blur(sigmaX: 10, sigmaY: 10),
        child: Container(
          decoration: BoxDecoration(
            color: (isDark ? Colors.white : Colors.black).withValues(
              alpha: 0.05,
            ),
            borderRadius: BorderRadius.circular(14),
            border: Border.all(
              color: (isDark ? Colors.white : Colors.black).withValues(
                alpha: 0.08,
              ),
            ),
          ),
          child: TextFormField(
            controller: controller,
            keyboardType: keyboardType,
            textInputAction: textInputAction,
            obscureText: obscureText,
            onFieldSubmitted: onFieldSubmitted,
            validator: validator,
            style: TextStyle(color: cs.onSurface, fontSize: 14),
            decoration: InputDecoration(
              labelText: label,
              labelStyle: TextStyle(
                color: cs.onSurfaceVariant.withValues(alpha: 0.5),
                fontSize: 14,
              ),
              prefixIcon: Icon(
                icon,
                color: cs.primary.withValues(alpha: 0.7),
                size: 20,
              ),
              suffixIcon: suffixIcon,
              border: InputBorder.none,
              contentPadding: const EdgeInsets.symmetric(
                horizontal: 16,
                vertical: 16,
              ),
              filled: false,
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildBackgroundOrbs(ColorScheme cs, bool isDark) {
    return AnimatedBuilder(
      animation: _glowController,
      builder: (context, _) {
        final t = _glowController.value;
        return Stack(
          children: [
            Positioned(
              top: -80 + (t * 30),
              right: -60 + (t * 20),
              child: Container(
                width: 300,
                height: 300,
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  gradient: RadialGradient(
                    colors: [
                      cs.primary.withValues(alpha: 0.15),
                      Colors.transparent,
                    ],
                  ),
                ),
              ),
            ),
            Positioned(
              bottom: -100 + (t * 20),
              left: -80 + (t * 15),
              child: Container(
                width: 350,
                height: 350,
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  gradient: RadialGradient(
                    colors: [
                      MannyTheme.tertiaryTeal.withValues(alpha: 0.1),
                      Colors.transparent,
                    ],
                  ),
                ),
              ),
            ),
            Positioned(
              top: MediaQuery.of(context).size.height * 0.4,
              left: MediaQuery.of(context).size.width * 0.5 - 100,
              child: Container(
                width: 200,
                height: 200,
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  gradient: RadialGradient(
                    colors: [
                      cs.secondary.withValues(alpha: 0.08),
                      Colors.transparent,
                    ],
                  ),
                ),
              ),
            ),
          ],
        );
      },
    );
  }
}
