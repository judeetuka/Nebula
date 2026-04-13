import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:manny_ui/manny_ui.dart';

import '../../../cluster/domain/entities/cluster.dart';
import '../../../cluster/domain/entities/node_info.dart';

/// Dashboard metric cards and circular gauges for cluster fleet status.
///
/// Renders a responsive row of 4 metric cards (Clusters, Total Nodes,
/// Online %, Active Tasks) followed by 3 circular gauges (CPU, Memory,
/// Battery).
class ClusterStatsCard extends StatelessWidget {
  final List<Cluster> clusters;
  final List<NodeInfo> nodes;

  const ClusterStatsCard({
    super.key,
    required this.clusters,
    required this.nodes,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    final totalNodes = nodes.length;
    final onlineNodes = nodes
        .where((n) => n.status == 'online' || n.status == 'busy')
        .length;
    final onlinePercent =
        totalNodes > 0 ? ((onlineNodes / totalNodes) * 100).round() : 0;
    final avgCpu = totalNodes > 0
        ? nodes.fold<double>(0, (sum, n) => sum + n.cpuLoad) / totalNodes
        : 0.0;
    final avgMemory = totalNodes > 0 ? 0.58 : 0.0; // Placeholder until wired
    final avgBattery = totalNodes > 0
        ? nodes.fold<int>(0, (sum, n) => sum + n.batteryLevel) ~/ totalNodes
        : 0;
    final activeTasks = totalNodes > 0
        ? nodes.where((n) => n.status == 'busy').length
        : 0;

    return Column(
      children: [
        // -- Metric Cards Row --
        ResponsiveGrid(
          shrinkWrap: true,
          mobileCols: 2,
          tabletCols: 4,
          desktopCols: 4,
          spacing: UIConstants.spacingMD,
          childAspectRatio: context.responsive(
            mobile: 1.45,
            tablet: 1.5,
            desktop: 1.6,
          ),
          children: [
            _MetricCard(
              title: 'Clusters',
              value: '${clusters.length}',
              trend: '+2',
              trendUp: true,
              icon: IconlyBold.discovery,
              accentColor: theme.colorScheme.primary,
            ),
            _MetricCard(
              title: 'Total Nodes',
              value: '$totalNodes',
              trend: '+5',
              trendUp: true,
              icon: IconlyBold.user_3,
              accentColor: theme.colorScheme.secondary,
            ),
            _MetricCard(
              title: 'Online',
              value: '$onlinePercent%',
              trend: onlinePercent >= 90 ? 'Healthy' : 'Degraded',
              trendUp: onlinePercent >= 90,
              icon: IconlyBold.shield_done,
              accentColor: theme.colorScheme.tertiary,
            ),
            _MetricCard(
              title: 'Active Tasks',
              value: '$activeTasks',
              trend: activeTasks > 0 ? 'Running' : 'Idle',
              trendUp: activeTasks > 0,
              icon: IconlyBold.activity,
              accentColor: theme.colorScheme.error,
            ),
          ],
        ),

        const SizedBox(height: UIConstants.spacingXL),

        // -- Circular Gauges Row --
        LayoutBuilder(
          builder: (context, constraints) {
            final isMobile = constraints.maxWidth < 600;
            final gaugeSize = isMobile
                ? (constraints.maxWidth - UIConstants.spacingMD * 2) / 3 *
                    0.75
                : 130.0;
            final effectiveSize = gaugeSize.clamp(80.0, 140.0);

            return Row(
              children: [
                Expanded(
                  child: _GaugeCard(
                    value: avgCpu.clamp(0.0, 1.0),
                    label: 'Avg CPU',
                    color: _cpuColor(avgCpu, theme),
                    size: effectiveSize,
                  ),
                ),
                const SizedBox(width: UIConstants.spacingMD),
                Expanded(
                  child: _GaugeCard(
                    value: avgMemory.clamp(0.0, 1.0),
                    label: 'Avg Memory',
                    color: theme.colorScheme.secondary,
                    size: effectiveSize,
                  ),
                ),
                const SizedBox(width: UIConstants.spacingMD),
                Expanded(
                  child: _GaugeCard(
                    value: (avgBattery / 100).clamp(0.0, 1.0),
                    label: 'Avg Battery',
                    color: _batteryColor(avgBattery, theme),
                    size: effectiveSize,
                  ),
                ),
              ],
            );
          },
        ),
      ],
    );
  }

  Color _cpuColor(double cpu, ThemeData theme) {
    if (cpu > 0.8) return theme.colorScheme.error;
    if (cpu > 0.5) return Colors.orange;
    return theme.colorScheme.primary;
  }

  Color _batteryColor(int battery, ThemeData theme) {
    if (battery < 20) return theme.colorScheme.error;
    if (battery < 50) return Colors.orange;
    return theme.colorScheme.tertiary;
  }
}

// ---------------------------------------------------------------------------
// Metric Card
// ---------------------------------------------------------------------------

/// A single metric card with accent strip, large value, and trend indicator.
class _MetricCard extends StatelessWidget {
  final String title;
  final String value;
  final String? trend;
  final bool trendUp;
  final IconData icon;
  final Color accentColor;

  const _MetricCard({
    required this.title,
    required this.value,
    this.trend,
    this.trendUp = true,
    required this.icon,
    required this.accentColor,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    return FrostedGlass(
      padding: EdgeInsets.zero,
      child: Row(
        children: [
          // Accent strip
          Container(
            width: 3,
            height: double.infinity,
            decoration: BoxDecoration(
              color: accentColor,
              borderRadius: const BorderRadius.only(
                topLeft: Radius.circular(20),
                bottomLeft: Radius.circular(20),
              ),
            ),
          ),
          // Content
          Expanded(
            child: Padding(
              padding: const EdgeInsets.symmetric(
                horizontal: 14,
                vertical: 12,
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  // Icon + Title row
                  Row(
                    children: [
                      Container(
                        padding: const EdgeInsets.all(5),
                        decoration: BoxDecoration(
                          color: accentColor.withValues(alpha: 0.12),
                          borderRadius: BorderRadius.circular(
                            UIConstants.radiusSmall,
                          ),
                        ),
                        child: Icon(
                          icon,
                          color: accentColor,
                          size: 14,
                        ),
                      ),
                      const SizedBox(width: 6),
                      Expanded(
                        child: Text(
                          title,
                          style: theme.textTheme.bodySmall?.copyWith(
                            color: theme.colorScheme.onSurface
                                .withValues(alpha: 0.55),
                            fontWeight: FontWeight.w500,
                          ),
                          overflow: TextOverflow.ellipsis,
                        ),
                      ),
                    ],
                  ),

                  const SizedBox(height: 8),

                  // Large value
                  Text(
                    value,
                    style: theme.textTheme.headlineMedium?.copyWith(
                      fontWeight: FontWeight.bold,
                      height: 1.0,
                    ),
                  ),

                  const SizedBox(height: 4),

                  // Trend
                  if (trend != null)
                    Row(
                      children: [
                        Icon(
                          trendUp
                              ? IconlyBold.arrow_up_2
                              : IconlyBold.arrow_down_2,
                          size: 12,
                          color: trendUp ? Colors.green : theme.colorScheme.error,
                        ),
                        const SizedBox(width: 3),
                        Expanded(
                          child: Text(
                            trend!,
                            style: theme.textTheme.labelSmall?.copyWith(
                              color: trendUp
                                  ? Colors.green
                                  : theme.colorScheme.error,
                              fontWeight: FontWeight.w600,
                            ),
                            overflow: TextOverflow.ellipsis,
                          ),
                        ),
                      ],
                    ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Gauge Card (FrostedGlass wrapper around the gauge)
// ---------------------------------------------------------------------------

class _GaugeCard extends StatelessWidget {
  final double value;
  final String label;
  final Color color;
  final double size;

  const _GaugeCard({
    required this.value,
    required this.label,
    required this.color,
    required this.size,
  });

  @override
  Widget build(BuildContext context) {
    return FrostedGlass(
      padding: const EdgeInsets.symmetric(vertical: 20, horizontal: 8),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          CircularGauge(
            value: value,
            label: label,
            color: color,
            size: size,
          ),
        ],
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Circular Gauge (CustomPainter based)
// ---------------------------------------------------------------------------

/// A circular progress gauge drawn with [CustomPainter].
///
/// Renders a 270-degree arc track with a filled portion based on [value].
/// The center displays the percentage and a label below.
class CircularGauge extends StatelessWidget {
  /// Progress value from 0.0 to 1.0.
  final double value;

  /// Label displayed below the percentage text.
  final String label;

  /// Color of the filled arc.
  final Color color;

  /// Diameter of the gauge in logical pixels.
  final double size;

  const CircularGauge({
    super.key,
    required this.value,
    required this.label,
    required this.color,
    this.size = 130,
  });

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final percentage = (value * 100).round();

    return SizedBox(
      width: size,
      height: size,
      child: Stack(
        alignment: Alignment.center,
        children: [
          // Painted gauge arcs
          CustomPaint(
            size: Size(size, size),
            painter: _GaugePainter(
              value: value.clamp(0.0, 1.0),
              color: color,
              trackColor:
                  theme.colorScheme.onSurface.withValues(alpha: 0.08),
              strokeWidth: size * 0.085,
            ),
          ),
          // Center text
          Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(
                '$percentage%',
                style: theme.textTheme.titleLarge?.copyWith(
                  fontWeight: FontWeight.bold,
                  fontSize: size * 0.2,
                  height: 1.1,
                ),
              ),
              const SizedBox(height: 2),
              Text(
                label,
                style: theme.textTheme.labelSmall?.copyWith(
                  color:
                      theme.colorScheme.onSurface.withValues(alpha: 0.5),
                  fontSize: size * 0.085,
                ),
                textAlign: TextAlign.center,
              ),
            ],
          ),
        ],
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Gauge Painter
// ---------------------------------------------------------------------------

/// Custom painter that draws a 270-degree arc gauge.
///
/// The arc starts at 135 degrees (bottom-left) and sweeps 270 degrees
/// clockwise, leaving a gap at the bottom. A background track is drawn
/// at full sweep, and a foreground arc is drawn proportionally to [value].
class _GaugePainter extends CustomPainter {
  final double value;
  final Color color;
  final Color trackColor;
  final double strokeWidth;

  _GaugePainter({
    required this.value,
    required this.color,
    required this.trackColor,
    required this.strokeWidth,
  });

  static const double _startAngle = 135 * (math.pi / 180); // bottom-left
  static const double _sweepTotal = 270 * (math.pi / 180); // 270 degrees

  @override
  void paint(Canvas canvas, Size size) {
    final center = Offset(size.width / 2, size.height / 2);
    final radius = (math.min(size.width, size.height) - strokeWidth) / 2;
    final rect = Rect.fromCircle(center: center, radius: radius);

    // Track (background arc)
    final trackPaint = Paint()
      ..color = trackColor
      ..style = PaintingStyle.stroke
      ..strokeWidth = strokeWidth
      ..strokeCap = StrokeCap.round;

    canvas.drawArc(rect, _startAngle, _sweepTotal, false, trackPaint);

    // Value arc (foreground)
    if (value > 0) {
      final sweepAngle = _sweepTotal * value;

      // Glow shadow behind the arc
      final glowPaint = Paint()
        ..color = color.withValues(alpha: 0.3)
        ..style = PaintingStyle.stroke
        ..strokeWidth = strokeWidth + 4
        ..strokeCap = StrokeCap.round
        ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 6);

      canvas.drawArc(rect, _startAngle, sweepAngle, false, glowPaint);

      // Foreground arc
      final fillPaint = Paint()
        ..color = color
        ..style = PaintingStyle.stroke
        ..strokeWidth = strokeWidth
        ..strokeCap = StrokeCap.round;

      canvas.drawArc(rect, _startAngle, sweepAngle, false, fillPaint);
    }
  }

  @override
  bool shouldRepaint(covariant _GaugePainter oldDelegate) {
    return oldDelegate.value != value ||
        oldDelegate.color != color ||
        oldDelegate.trackColor != trackColor ||
        oldDelegate.strokeWidth != strokeWidth;
  }
}
