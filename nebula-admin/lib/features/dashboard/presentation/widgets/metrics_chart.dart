import 'package:fl_chart/fl_chart.dart';
import 'package:flutter/material.dart';
import 'package:manny_ui/manny_ui.dart';

import '../../../cluster/domain/entities/node_info.dart';

/// A line chart rendering node metrics over time.
///
/// Displays battery level, CPU load, and (optionally) memory usage as
/// separate colored lines. Data is sampled from the provided [nodes] list,
/// treating each node index as a time step on the X axis.
class MetricsChart extends StatelessWidget {
  final List<NodeInfo> nodes;

  const MetricsChart({super.key, required this.nodes});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    if (nodes.isEmpty) {
      return SizedBox(
        height: 200,
        child: Center(
          child: Text(
            'No node data available',
            style: theme.textTheme.bodyMedium?.copyWith(
              color: theme.colorScheme.onSurfaceVariant,
            ),
          ),
        ),
      );
    }

    final batterySpots = <FlSpot>[];
    final cpuSpots = <FlSpot>[];

    for (int i = 0; i < nodes.length; i++) {
      batterySpots.add(FlSpot(i.toDouble(), nodes[i].batteryLevel.toDouble()));
      cpuSpots.add(FlSpot(i.toDouble(), nodes[i].cpuLoad * 100));
    }

    return Card(
      child: Padding(
        padding: UIConstants.paddingLG,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Node Metrics',
              style: theme.textTheme.titleMedium?.copyWith(
                fontWeight: FontWeight.w600,
              ),
            ),
            const SizedBox(height: UIConstants.spacingSM),
            _Legend(theme: theme),
            const SizedBox(height: UIConstants.spacingMD),
            SizedBox(
              height: 200,
              child: LineChart(
                LineChartData(
                  gridData: FlGridData(
                    show: true,
                    drawVerticalLine: false,
                    horizontalInterval: 25,
                    getDrawingHorizontalLine: (value) => FlLine(
                      color: theme.colorScheme.outlineVariant.withAlpha(80),
                      strokeWidth: 1,
                    ),
                  ),
                  titlesData: FlTitlesData(
                    leftTitles: AxisTitles(
                      sideTitles: SideTitles(
                        showTitles: true,
                        reservedSize: 40,
                        interval: 25,
                        getTitlesWidget: (value, meta) => Text(
                          '${value.toInt()}%',
                          style: theme.textTheme.labelSmall,
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
                          final label = id.length > 6 ? id.substring(0, 6) : id;
                          return Padding(
                            padding: const EdgeInsets.only(top: 6),
                            child: Text(
                              label,
                              style: theme.textTheme.labelSmall,
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
                      spots: batterySpots,
                      color: MannyTheme.tertiaryTeal,
                    ),
                    _lineData(
                      spots: cpuSpots,
                      color: theme.colorScheme.primary,
                    ),
                  ],
                  lineTouchData: LineTouchData(
                    touchTooltipData: LineTouchTooltipData(
                      getTooltipItems: (touchedSpots) {
                        return touchedSpots.map((spot) {
                          final label = spot.barIndex == 0 ? 'Battery' : 'CPU';
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
          ],
        ),
      ),
    );
  }

  LineChartBarData _lineData({
    required List<FlSpot> spots,
    required Color color,
  }) {
    return LineChartBarData(
      spots: spots,
      isCurved: true,
      color: color,
      barWidth: 3,
      isStrokeCapRound: true,
      dotData: FlDotData(
        show: true,
        getDotPainter: (spot, percent, barData, index) => FlDotCirclePainter(
          radius: 4,
          color: color,
          strokeWidth: 2,
          strokeColor: Colors.white,
        ),
      ),
      belowBarData: BarAreaData(show: true, color: color.withAlpha(30)),
    );
  }
}

class _Legend extends StatelessWidget {
  final ThemeData theme;

  const _Legend({required this.theme});

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        _LegendItem(color: MannyTheme.tertiaryTeal, label: 'Battery %'),
        const SizedBox(width: UIConstants.spacingLG),
        _LegendItem(color: theme.colorScheme.primary, label: 'CPU %'),
      ],
    );
  }
}

class _LegendItem extends StatelessWidget {
  final Color color;
  final String label;

  const _LegendItem({required this.color, required this.label});

  @override
  Widget build(BuildContext context) {
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Container(
          width: 12,
          height: 12,
          decoration: BoxDecoration(
            color: color,
            borderRadius: BorderRadius.circular(3),
          ),
        ),
        const SizedBox(width: 6),
        Text(label, style: Theme.of(context).textTheme.labelSmall),
      ],
    );
  }
}
