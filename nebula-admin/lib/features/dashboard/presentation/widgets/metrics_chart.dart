import 'package:fl_chart/fl_chart.dart';
import 'package:flutter/material.dart';
import 'package:manny_ui/manny_ui.dart';

import '../../../cluster/domain/entities/node_info.dart';

/// A performance metrics line chart with gradient fills, frosted legend pills,
/// and styled tooltips.
///
/// Displays battery level, CPU load, and memory usage (placeholder) as separate
/// colored lines. Data is sampled from the provided [nodes] list, treating each
/// node index as a time step on the X axis.
class MetricsChart extends StatelessWidget {
  final List<NodeInfo> nodes;

  const MetricsChart({super.key, required this.nodes});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    if (nodes.isEmpty) {
      return FrostedGlass(
        padding: const EdgeInsets.all(20),
        child: SizedBox(
          height: 220,
          child: Center(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Container(
                  padding: const EdgeInsets.all(16),
                  decoration: BoxDecoration(
                    color:
                        theme.colorScheme.onSurface.withValues(alpha: 0.06),
                    borderRadius:
                        BorderRadius.circular(UIConstants.radiusLarge),
                  ),
                  child: Icon(
                    IconlyBroken.chart,
                    size: 40,
                    color:
                        theme.colorScheme.onSurface.withValues(alpha: 0.25),
                  ),
                ),
                const SizedBox(height: UIConstants.spacingLG),
                Text(
                  'No Node Data',
                  style: theme.textTheme.titleMedium?.copyWith(
                    fontWeight: FontWeight.w600,
                    color:
                        theme.colorScheme.onSurface.withValues(alpha: 0.5),
                  ),
                ),
                const SizedBox(height: UIConstants.spacingXS),
                Text(
                  'Metrics will appear once nodes report in',
                  style: theme.textTheme.bodySmall?.copyWith(
                    color:
                        theme.colorScheme.onSurface.withValues(alpha: 0.35),
                  ),
                ),
              ],
            ),
          ),
        ),
      );
    }

    final batterySpots = <FlSpot>[];
    final cpuSpots = <FlSpot>[];
    final memorySpots = <FlSpot>[];

    for (int i = 0; i < nodes.length; i++) {
      batterySpots
          .add(FlSpot(i.toDouble(), nodes[i].batteryLevel.toDouble()));
      cpuSpots.add(FlSpot(i.toDouble(), nodes[i].cpuLoad * 100));
      // Memory placeholder — simulated from battery/cpu correlation
      memorySpots.add(FlSpot(
        i.toDouble(),
        ((nodes[i].cpuLoad * 60) + 20).clamp(0.0, 100.0),
      ));
    }

    final batteryColor = theme.colorScheme.tertiary;
    final cpuColor = theme.colorScheme.primary;
    final memoryColor = theme.colorScheme.secondary;

    return FrostedGlass(
      padding: const EdgeInsets.all(20),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // -- Header row with icon and title --
          Row(
            children: [
              Container(
                padding: const EdgeInsets.all(8),
                decoration: BoxDecoration(
                  color:
                      theme.colorScheme.primary.withValues(alpha: 0.12),
                  borderRadius:
                      BorderRadius.circular(UIConstants.radiusSmall),
                ),
                child: Icon(
                  IconlyBold.chart,
                  color: theme.colorScheme.primary,
                  size: UIConstants.iconMD,
                ),
              ),
              const SizedBox(width: UIConstants.spacingMD),
              Expanded(
                child: Text(
                  'Performance Trends',
                  style: theme.textTheme.titleLarge?.copyWith(
                    fontWeight: FontWeight.bold,
                  ),
                ),
              ),
              // Time range indicator
              FrostedGlass(
                borderRadius: BorderRadius.circular(20),
                padding: const EdgeInsets.symmetric(
                  horizontal: 10,
                  vertical: 4,
                ),
                shadow: false,
                tintColor: theme.colorScheme.primary,
                opacity: 0.1,
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Icon(
                      IconlyBroken.time_circle,
                      size: 12,
                      color: theme.colorScheme.primary,
                    ),
                    const SizedBox(width: 4),
                    Text(
                      'Real-time',
                      style: theme.textTheme.labelSmall?.copyWith(
                        color: theme.colorScheme.primary,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ),

          const SizedBox(height: UIConstants.spacingLG),

          // -- Legend as frosted pills --
          Wrap(
            spacing: UIConstants.spacingSM,
            runSpacing: UIConstants.spacingSM,
            children: [
              _LegendPill(
                color: cpuColor,
                label: 'CPU %',
                icon: IconlyBroken.activity,
              ),
              _LegendPill(
                color: memoryColor,
                label: 'Memory %',
                icon: IconlyBroken.swap,
              ),
              _LegendPill(
                color: batteryColor,
                label: 'Battery %',
                icon: IconlyBroken.heart,
              ),
            ],
          ),

          const SizedBox(height: UIConstants.spacingXL),

          // -- Chart --
          SizedBox(
            height: 240,
            child: Padding(
              padding: const EdgeInsets.only(right: 8),
              child: LineChart(
                LineChartData(
                  gridData: FlGridData(
                    show: true,
                    drawVerticalLine: false,
                    horizontalInterval: 25,
                    getDrawingHorizontalLine: (value) => FlLine(
                      color: theme.colorScheme.onSurface
                          .withValues(alpha: 0.06),
                      strokeWidth: 1,
                      dashArray: [4, 4],
                    ),
                  ),
                  titlesData: FlTitlesData(
                    leftTitles: AxisTitles(
                      sideTitles: SideTitles(
                        showTitles: true,
                        reservedSize: 42,
                        interval: 25,
                        getTitlesWidget: (value, meta) => Padding(
                          padding: const EdgeInsets.only(right: 8),
                          child: Text(
                            '${value.toInt()}%',
                            style: theme.textTheme.labelSmall?.copyWith(
                              color: theme.colorScheme.onSurface
                                  .withValues(alpha: 0.35),
                              fontSize: 10,
                            ),
                          ),
                        ),
                      ),
                    ),
                    bottomTitles: AxisTitles(
                      sideTitles: SideTitles(
                        showTitles: true,
                        interval: 1,
                        getTitlesWidget: (value, meta) {
                          final idx = value.toInt();
                          if (idx < 0 || idx >= nodes.length) {
                            return const SizedBox.shrink();
                          }
                          final id = nodes[idx].nodeId;
                          final label =
                              id.length > 5 ? id.substring(0, 5) : id;
                          return Padding(
                            padding: const EdgeInsets.only(top: 8),
                            child: Text(
                              label,
                              style: theme.textTheme.labelSmall?.copyWith(
                                color: theme.colorScheme.onSurface
                                    .withValues(alpha: 0.35),
                                fontSize: 10,
                              ),
                            ),
                          );
                        },
                      ),
                    ),
                    topTitles: const AxisTitles(
                      sideTitles: SideTitles(showTitles: false),
                    ),
                    rightTitles: const AxisTitles(
                      sideTitles: SideTitles(showTitles: false),
                    ),
                  ),
                  borderData: FlBorderData(show: false),
                  minY: 0,
                  maxY: 100,
                  lineBarsData: [
                    _lineData(
                      spots: cpuSpots,
                      color: cpuColor,
                    ),
                    _lineData(
                      spots: memorySpots,
                      color: memoryColor,
                    ),
                    _lineData(
                      spots: batterySpots,
                      color: batteryColor,
                    ),
                  ],
                  lineTouchData: LineTouchData(
                    handleBuiltInTouches: true,
                    touchTooltipData: LineTouchTooltipData(
                      tooltipRoundedRadius: UIConstants.radiusMedium,
                      tooltipPadding: const EdgeInsets.symmetric(
                        horizontal: 12,
                        vertical: 8,
                      ),
                      getTooltipItems: (touchedSpots) {
                        return touchedSpots.map((spot) {
                          String label;
                          switch (spot.barIndex) {
                            case 0:
                              label = 'CPU';
                            case 1:
                              label = 'Memory';
                            default:
                              label = 'Battery';
                          }
                          return LineTooltipItem(
                            '$label: ${spot.y.toStringAsFixed(0)}%',
                            TextStyle(
                              color: spot.bar.color,
                              fontWeight: FontWeight.w600,
                              fontSize: 12,
                            ),
                          );
                        }).toList();
                      },
                    ),
                  ),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }

  /// Builds a single line with gradient fill below.
  LineChartBarData _lineData({
    required List<FlSpot> spots,
    required Color color,
  }) {
    return LineChartBarData(
      spots: spots,
      isCurved: true,
      curveSmoothness: 0.3,
      preventCurveOverShooting: true,
      color: color,
      barWidth: 2.5,
      isStrokeCapRound: true,
      dotData: FlDotData(
        show: true,
        getDotPainter: (spot, percent, barData, index) => FlDotCirclePainter(
          radius: 3,
          color: color,
          strokeWidth: 1.5,
          strokeColor: Colors.white,
        ),
      ),
      belowBarData: BarAreaData(
        show: true,
        gradient: LinearGradient(
          begin: Alignment.topCenter,
          end: Alignment.bottomCenter,
          colors: [
            color.withValues(alpha: 0.25),
            color.withValues(alpha: 0.0),
          ],
        ),
      ),
    );
  }
}

/// Frosted pill legend item with icon and colored indicator.
class _LegendPill extends StatelessWidget {
  final Color color;
  final String label;
  final IconData icon;

  const _LegendPill({
    required this.color,
    required this.label,
    required this.icon,
  });

  @override
  Widget build(BuildContext context) {
    return FrostedGlass(
      borderRadius: BorderRadius.circular(20),
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 5),
      shadow: false,
      tintColor: color,
      opacity: 0.1,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Container(
            width: 8,
            height: 8,
            decoration: BoxDecoration(
              color: color,
              shape: BoxShape.circle,
            ),
          ),
          const SizedBox(width: 6),
          Icon(icon, size: 13, color: color),
          const SizedBox(width: 4),
          Text(
            label,
            style: TextStyle(
              color: color,
              fontWeight: FontWeight.w600,
              fontSize: 12,
            ),
          ),
        ],
      ),
    );
  }
}
